// imports
use std::{
    ffi::OsString,
    os::unix::io::AsRawFd,
    sync::{Arc, Mutex},
    time::Duration,
};

use smithay::{
    backend::renderer::element::{
        default_primary_scanout_output_compare, utils::select_dmabuf_feedback, RenderElementStates,
    },
    desktop::{
        self,
        utils::{
            surface_presentation_feedback_flags_from_states, surface_primary_scanout_output,
            update_surface_primary_scanout_output, OutputPresentationFeedback,
        },
        PopupManager, Space, Window,
    },
    input::{
        pointer::{CursorImageStatus, PointerHandle},
        Seat, SeatState,
    },
    output::Output,
    reexports::{
        calloop::{generic::Generic, Interest, LoopHandle, LoopSignal, Mode, PostAction},
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
            Display, DisplayHandle,
        },
    },
    utils::{Clock, Logical, Monotonic, Point},
    wayland::{
        compositor::CompositorState,
        data_device::DataDeviceState,
        dmabuf::DmabufFeedback,
        output::OutputManagerState,
        presentation::PresentationState,
        shell::{
            wlr_layer::{Layer, WlrLayerShellState},
            xdg::{decoration::XdgDecorationState, XdgShellState},
        },
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

use crate::CalloopData;

pub struct Corrosion<BackendData: Backend + 'static> {
    pub display_handle: DisplayHandle,
    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub backend_data: BackendData,

    pub space: Space<Window>,
    pub handle: LoopHandle<'static, CalloopData<BackendData>>,

    // Smithay State
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub xdg_decoration_state: XdgDecorationState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Corrosion<BackendData>>,
    pub data_device_state: DataDeviceState,
    pub presentation_state: PresentationState,
    pub popup_manager: PopupManager,
    pub wlr_layer_state: WlrLayerShellState,

    pub cursor_image_status: Arc<Mutex<CursorImageStatus>>,
    pub pointer_location: Point<f64, Logical>,
    pub seat: Seat<Self>,
    pub seat_name: String,
    pub clock: Clock<Monotonic>,
}

impl<BackendData: Backend + 'static> Corrosion<BackendData> {
    pub fn new(
        handle: LoopHandle<'static, CalloopData<BackendData>>,
        display: &mut Display<Self>,
        backend_data: BackendData,
    ) -> Self {
        let clock = Clock::new().expect("Unable to make clock");
        let start_time = std::time::Instant::now();

        let dh = display.handle();

        // Creates a compositor global. Used to store and access surface trees.
        let compositor_state = CompositorState::new::<Self>(&dh);
        // Creates an xdg shell global. Xdg shell is used by many clients and compositors to
        // provide utilities for basic window management
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        // Handles window decoration
        let xdg_decoration_state =
            XdgDecorationState::new::<Corrosion<BackendData>>(&display.handle());

        // Creates an shm global. Shm is used to share wlbuffers between the client and server
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        // Advertises output globals to clients
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        // Handles input devices
        let mut seat_state = SeatState::new();
        // Creates a global that handles clipboard data and drag-n-drop
        let data_device_state = DataDeviceState::new::<Self>(&dh);

        // A seat is a group of keyboards, pointer and touch devices.
        // A seat typically has a pointer and maintains a keyboard focus and a pointer focus.
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, &backend_data.seat_name());

        // Notify clients that we have a keyboard, for the sake of the example we assume that keyboard is always present.
        // You may want to track keyboard hot-plug in real compositor.
        seat.add_keyboard(Default::default(), 500, 500).unwrap();

        // Notify clients that we have a pointer (mouse)
        // Here we assume that there is always pointer plugged in
        seat.add_pointer();

        let cursor_image_status = Arc::new(Mutex::new(CursorImageStatus::Default));

        // A space represents a two-dimensional plane. Windows and Outputs can be mapped onto it.
        //
        // Windows get a position and stacking order through mapping.
        // Outputs become views of a part of the Space and can be rendered via Space::render_output.
        let space = Space::default();
        let presentation_state = PresentationState::new::<Self>(&dh, clock.id() as u32);

        // Manager to track popups and their relations to a surface
        let popup_manager = PopupManager::default();

        // Creates a wlr layer manager. Thanks to the people at wlroots, Surfaces can be layered,
        // so the layer manager global will handle the requests made by a client that supports the
        // protocol
        let wlr_layer_state = WlrLayerShellState::new::<Self>(&dh);

        // Initializes a wayland listener socket
        let socket_name = Self::init_wayland_listener(display, &handle);

        // Return the state
        Self {
            display_handle: dh,
            start_time,

            space,
            handle,
            seat_name: backend_data.seat_name(),
            backend_data,

            socket_name,

            compositor_state,
            xdg_shell_state,
            xdg_decoration_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            presentation_state,
            popup_manager,
            wlr_layer_state,

            cursor_image_status,
            pointer_location: (0.0, 0.0).into(),
            seat,
            clock,
        }
    }

    // This function is used to initialize the wayland listener
    fn init_wayland_listener(
        display: &mut Display<Corrosion<BackendData>>,
        event_loop: &LoopHandle<'static, CalloopData<BackendData>>,
    ) -> OsString {
        // Creates a new listening socket, automatically choosing the next available `wayland` socket name.
        let listening_socket = ListeningSocketSource::new_auto().unwrap();

        // Get the name of the listening socket.
        // Clients will connect to this socket.
        let socket_name = listening_socket.socket_name().to_os_string();

        event_loop
            .insert_source(listening_socket, move |client_stream, _, state| {
                // Inside the callback, you should insert the client into the display.
                //
                // You may also associate some data with the client when inserting the client.
                state
                    .display
                    .handle()
                    .insert_client(client_stream, Arc::new(ClientState))
                    .unwrap();
            })
            .expect("Failed to init the wayland event source.");

        // You also need to add the display itself to the event loop, so that client events will be processed by wayland-server.
        event_loop
            .insert_source(
                Generic::new(
                    display.backend().poll_fd().as_raw_fd(),
                    Interest::READ,
                    Mode::Level,
                ),
                |_, _, state| {
                    state.display.dispatch_clients(&mut state.state).unwrap();
                    Ok(PostAction::Continue)
                },
            )
            .unwrap();

        socket_name
    }

    // This function is used to get the surface under the pointer
    pub fn surface_under_pointer(
        &self,
        pointer: &PointerHandle<Self>,
    ) -> Option<(WlSurface, Point<i32, Logical>)> {
        let pos = pointer.current_location();
        let output = self
            .space
            .outputs()
            .find(|output| {
                let geometry = self.space.output_geometry(output).unwrap();
                geometry.contains(pos.to_i32_round())
            })
            .unwrap();
        let map = desktop::layer_map_for_output(output);
        let mut under = None;

        if let Some(layer) = map
            .layer_under(Layer::Overlay, pos)
            .or_else(|| map.layer_under(Layer::Top, pos))
        {
            let layer_geometry = map.layer_geometry(layer).unwrap().loc;
            under = Some((layer.wl_surface().clone(), layer_geometry))
        } else if let Some((window, location)) = self
            .space
            .element_under(pos)
            .map(|(focus_target, location)| (focus_target.toplevel().wl_surface(), location))
        {
            under = Some((window.clone(), location))
        } else if let Some(layer) = map
            .layer_under(Layer::Bottom, pos)
            .or_else(|| map.layer_under(Layer::Background, pos))
        {
            let layer_geometry = map.layer_geometry(layer).unwrap().loc;
            under = Some((layer.wl_surface().clone(), layer_geometry))
        }
        under
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SurfaceDmabufFeedback<'a> {
    pub render_feedback: &'a DmabufFeedback,
    pub scanout_feedback: &'a DmabufFeedback,
}

pub fn post_repaint(
    output: &Output,
    render_states: &RenderElementStates,
    space: &Space<Window>,
    dmabuf_feedback: Option<SurfaceDmabufFeedback>,
    time: impl Into<Duration>,
) {
    let time = time.into();
    let throttle = Some(Duration::from_secs(1));

    space.elements().for_each(|window| {
        window.with_surfaces(|surface, states| {
            update_surface_primary_scanout_output(
                surface,
                output,
                states,
                render_states,
                default_primary_scanout_output_compare,
            );

            // TODO: implement fractional scale support
        });
        if space.outputs_for_element(window).contains(output) {
            window.send_frame(output, time, throttle, surface_primary_scanout_output);
            if let Some(dmabuf_feedback) = dmabuf_feedback {
                window.send_dmabuf_feedback(output, surface_primary_scanout_output, |surface, _| {
                    select_dmabuf_feedback(
                        surface,
                        render_states,
                        dmabuf_feedback.render_feedback,
                        dmabuf_feedback.scanout_feedback,
                    )
                })
            }
        }
    });

    let map = desktop::layer_map_for_output(output);
    for layer_surface in map.layers() {
        layer_surface.with_surfaces(|surface, states| {
            update_surface_primary_scanout_output(
                surface,
                output,
                states,
                render_states,
                default_primary_scanout_output_compare,
            );
        });
        layer_surface.send_frame(output, time, throttle, surface_primary_scanout_output);
        if let Some(dmabuf_feedback) = dmabuf_feedback {
            layer_surface.send_dmabuf_feedback(
                output,
                surface_primary_scanout_output,
                |surface, _| {
                    select_dmabuf_feedback(
                        surface,
                        render_states,
                        dmabuf_feedback.render_feedback,
                        dmabuf_feedback.scanout_feedback,
                    )
                },
            );
        }
    }
}

pub fn take_presentation_feedback(
    output: &Output,
    space: &Space<Window>,
    states: &RenderElementStates,
) -> OutputPresentationFeedback {
    let mut output_presentation_feedback = OutputPresentationFeedback::new(output);

    space.elements().for_each(|window| {
        if space.outputs_for_element(window).contains(output) {
            window.take_presentation_feedback(
                &mut output_presentation_feedback,
                surface_primary_scanout_output,
                |surface, _| surface_presentation_feedback_flags_from_states(surface, states),
            )
        }
    });
    let map = desktop::layer_map_for_output(output);
    for layer_surface in map.layers() {
        layer_surface.take_presentation_feedback(
            &mut output_presentation_feedback,
            surface_primary_scanout_output,
            |surface, _| surface_presentation_feedback_flags_from_states(surface, states),
        );
    }
    output_presentation_feedback
}

pub struct ClientState;
impl ClientData for ClientState {
    fn initialized(&self, client_id: ClientId) {
        tracing::debug!("Client id '{:?}' initialized", client_id);
    }
    fn disconnected(&self, client_id: ClientId, reason: DisconnectReason) {
        tracing::debug!(
            "Client id '{:?} disconnected with reason: {:?}",
            client_id,
            reason
        );
    }
}

pub trait Backend {
    fn loop_signal(&self) -> &LoopSignal;
    fn seat_name(&self) -> String;
    fn early_import(&mut self, output: &WlSurface);
    fn reset_buffers(&mut self, surface: &Output);
}
