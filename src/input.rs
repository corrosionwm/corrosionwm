use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    },
    input::{
        keyboard::{keysyms, FilterResult},
        pointer::{AxisFrame, ButtonEvent, Focus, GrabStartData, MotionEvent},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::SERIAL_COUNTER,
};

use crate::{
    grabs::{resize_grab::ResizeEdge, MoveSurfaceGrab, ResizeSurfaceGrab},
    handlers::keybindings::{self, KeyAction},
    state::Corrosion,
    CorrosionConfig,
};

impl Corrosion {
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
            InputEvent::PointerMotion { .. } => {}
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output = self.space.outputs().next().unwrap();

                let output_geo = self.space.output_geometry(output).unwrap();

                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.seat.get_pointer().unwrap();

                let under = self.surface_under_pointer(&pointer);

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time_msec(),
                    },
                );
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
                        if self.seat.get_keyboard().unwrap().modifier_state().alt {
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
}
