use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::{keysyms, FilterResult},
        pointer::{AxisFrame, ButtonEvent, Focus, GrabStartData, MotionEvent, RelativeMotionEvent},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, SERIAL_COUNTER},
};

use crate::{
    backend::UdevData,
    grabs::{resize_grab::ResizeEdge, MoveSurfaceGrab, ResizeSurfaceGrab},
    handlers::keybindings::{self, KeyAction},
    state::Corrosion,
    CorrosionConfig,
};

impl Corrosion<UdevData> {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);
                let press_state = event.state();
                let action = self.seat.get_keyboard().unwrap().input::<KeyAction, _>(
                    self,
                    event.key_code(),
                    press_state,
                    serial,
                    time,
                    |_, modifier, handle| {
                        let action: KeyAction;
                        if keybindings::get_mod_key_and_compare(modifier)
                            && press_state == KeyState::Pressed
                        {
                            // our shitty keybindings
                            // TODO: get rid of this shit
                            let corrosion_config = CorrosionConfig::new();
                            let defaults = corrosion_config.get_defaults();
                            if handle.modified_sym() == keysyms::KEY_h | keysyms::KEY_H {
                                tracing::info!("running wofi");
                                let launcher = &defaults.launcher;
                                action = KeyAction::_Launcher(launcher.to_string());
                            } else if handle.modified_sym() == keysyms::KEY_q | keysyms::KEY_Q {
                                tracing::info!("Quitting");
                                action = KeyAction::Quit;
                            } else if handle.modified_sym() == keysyms::KEY_Return {
                                tracing::info!("spawn terminal");
                                let terminal = &defaults.terminal;
                                action = KeyAction::Spawn(terminal.to_string());
                            } else if handle.modified_sym() == keysyms::KEY_x | keysyms::KEY_X {
                                // TODO: make it so you can close windows
                                action = KeyAction::_CloseWindow;
                            } else if (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12)
                                .contains(&handle.modified_sym())
                            {
                                action = KeyAction::VTSwitch(
                                    (handle.modified_sym() - keysyms::KEY_XF86Switch_VT_1 + 1)
                                        as i32,
                                )
                            } else {
                                return FilterResult::Forward;
                            }
                        } else {
                            return FilterResult::Forward;
                        }
                        FilterResult::Intercept(action)
                    },
                );
                if let Some(action) = action {
                    self.parse_keybindings(action);
                }
            }
            InputEvent::PointerMotion { event } => {
                let serial = SERIAL_COUNTER.next_serial();

                self.pointer_location += event.delta();
                self.pointer_location = self.clamp_coords(self.pointer_location);
                let surface_under = self.surface_under_pointer(&self.seat.get_pointer().unwrap());
                if let Some(pointer) = self.seat.get_pointer() {
                    pointer.motion(
                        self,
                        surface_under.clone(),
                        &MotionEvent {
                            location: self.pointer_location,
                            serial,
                            time: event.time_msec(),
                        },
                    );
                    pointer.relative_motion(
                        self,
                        surface_under.clone(),
                        &RelativeMotionEvent {
                            delta: event.delta(),
                            delta_unaccel: event.delta_unaccel(),
                            utime: event.time(),
                        },
                    )
                }
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();

                let max_x = self.space.outputs().fold(0, |acc, o| {
                    acc + self.space.output_geometry(o).unwrap().size.w
                });

                let max_h_output = self
                    .space
                    .outputs()
                    .max_by_key(|o| self.space.output_geometry(o).unwrap().size.h)
                    .unwrap();

                let max_y = self.space.output_geometry(max_h_output).unwrap().size.h;

                self.pointer_location.x = event.x_transformed(max_x);
                self.pointer_location.y = event.y_transformed(max_y);

                // clamp to screen limits
                self.pointer_location = self.clamp_coords(self.pointer_location);

                let under = self.surface_under_pointer(&self.seat.get_pointer().unwrap());
                if let Some(ptr) = self.seat.get_pointer() {
                    ptr.motion(
                        self,
                        under,
                        &MotionEvent {
                            location: self.pointer_location,
                            serial,
                            time: event.time_msec(),
                        },
                    );
                }
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let keyboard = self.seat.get_keyboard().unwrap();

                let serial = SERIAL_COUNTER.next_serial();

                let button = event.button_code();

                let button_state = event.state();

                if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
                    if let Some((window, _loc)) = self
                        .space
                        .element_under(pointer.current_location())
                        .map(|(w, l)| (w.clone(), l))
                    {
                        self.space.raise_element(&window, true);
                        keyboard.set_focus(
                            self,
                            Some(window.toplevel().wl_surface().clone()),
                            serial,
                        );
                        self.space.elements().for_each(|window| {
                            window.toplevel().send_configure();
                        });

                        // Check for compositor initiated move grab
                        if self.seat.get_keyboard().unwrap().modifier_state().logo {
                            let start_data = GrabStartData {
                                focus: None,
                                button,
                                location: pointer.current_location(),
                            };

                            let initial_window_location =
                                self.space.element_location(&window).unwrap();

                            let edges = ResizeEdge::all();

                            let initial_rect = &window.geometry();

                            match button {
                                0x110 => {
                                    let move_grab = MoveSurfaceGrab {
                                        start_data,
                                        window,
                                        initial_window_location,
                                    };

                                    pointer.set_grab(self, move_grab, serial, Focus::Clear);
                                }
                                0x111 => {
                                    let resize_grab = ResizeSurfaceGrab::start(
                                        start_data,
                                        window,
                                        edges,
                                        *initial_rect,
                                    );
                                    pointer.set_grab(self, resize_grab, serial, Focus::Clear);
                                }
                                _ => (),
                            }
                        };
                    } else if let Some((window, _loc)) = self.surface_under_pointer(&pointer) {
                        keyboard.set_focus(self, Some(window), serial);
                    } else {
                        self.space.elements().for_each(|window| {
                            window.set_activated(false);
                            window.toplevel().send_configure();
                        });
                        keyboard.set_focus(self, Option::<WlSurface>::None, serial);
                    }
                };

                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time: event.time_msec(),
                    },
                );
            }
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();

                let horizontal_amount = event
                    .amount(Axis::Horizontal)
                    .unwrap_or_else(|| event.amount_discrete(Axis::Horizontal).unwrap() * 3.0);
                let vertical_amount = event
                    .amount(Axis::Vertical)
                    .unwrap_or_else(|| event.amount_discrete(Axis::Vertical).unwrap() * 3.0);
                let horizontal_amount_discrete = event.amount_discrete(Axis::Horizontal);
                let vertical_amount_discrete = event.amount_discrete(Axis::Vertical);

                let mut frame = AxisFrame::new(event.time_msec()).source(source);
                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);
                    if let Some(discrete) = horizontal_amount_discrete {
                        frame = frame.discrete(Axis::Horizontal, discrete as i32);
                    }
                } else if source == AxisSource::Finger {
                    frame = frame.stop(Axis::Horizontal);
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.discrete(Axis::Vertical, discrete as i32);
                    }
                } else if source == AxisSource::Finger {
                    frame = frame.stop(Axis::Vertical);
                }

                self.seat.get_pointer().unwrap().axis(self, frame);
            }
            _ => {}
        }
    }
    fn clamp_coords(&self, pos: Point<f64, Logical>) -> Point<f64, Logical> {
        if self.space.outputs().next().is_none() {
            return pos;
        }

        let (pos_x, pos_y) = pos.into();
        let max_x = self.space.outputs().fold(0, |acc, o| {
            acc + self.space.output_geometry(o).unwrap().size.w
        });
        let clamped_x = pos_x.max(0.0).min(max_x as f64);
        let max_y = self
            .space
            .outputs()
            .find(|o| {
                let geo = self.space.output_geometry(o).unwrap();
                geo.contains((clamped_x as i32, 0))
            })
            .map(|o| self.space.output_geometry(o).unwrap().size.h);

        if let Some(max_y) = max_y {
            let clamped_y = pos_y.max(0.0).min(max_y as f64);
            (clamped_x, clamped_y).into()
        } else {
            (clamped_x, pos_y).into()
        }
    }
}
