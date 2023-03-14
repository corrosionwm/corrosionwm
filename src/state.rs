// imports
use std::{ffi::OsString, os::unix::io::AsRawFd, sync::Arc};

use smithay::{
    desktop::{Space, Window, WindowSurfaceType},
    input::{pointer::PointerHandle, Seat, SeatState},
    output::Output,
    reexports::{
        calloop::{generic::Generic, Interest, LoopHandle, LoopSignal, Mode, PostAction},
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
            Display,
        },
    },
    utils::{Logical, Point},
    wayland::{
        compositor::CompositorState,
        data_device::DataDeviceState,
        output::OutputManagerState,
        shell::xdg::{decoration::XdgDecorationState, XdgShellState},
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

use crate::CalloopData;

pub struct Corrosion<BackendData: Backend + 'static> {
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

    pub seat: Seat<Self>,
}


impl<BackendData: Backend + 'static> Corrosion<BackendData> {
    pub fn new(
        handle: LoopHandle<'static, CalloopData<BackendData>>,
        display: &mut Display<Self>,
        backend_data: BackendData,
    ) -> Self {
        let start_time = std::time::Instant::now();

        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let xdg_decoration_state =
            XdgDecorationState::new::<Corrosion<BackendData>>(&display.handle());
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&dh);

        // A seat is a group of keyboards, pointer and touch devices.
        // A seat typically has a pointer and maintains a keyboard focus and a pointer focus.
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "winit");

        // Notify clients that we have a keyboard, for the sake of the example we assume that keyboard is always present.
        // You may want to track keyboard hot-plug in real compositor.
        seat.add_keyboard(Default::default(), 500, 500).unwrap();

        // Notify clients that we have a pointer (mouse)
        // Here we assume that there is always pointer plugged in
        seat.add_pointer();

        // A space represents a two-dimensional plane. Windows and Outputs can be mapped onto it.
        //
        // Windows get a position and stacking order through mapping.
        // Outputs become views of a part of the Space and can be rendered via Space::render_output.
        let space = Space::default();

        let socket_name = Self::init_wayland_listener(display, &handle);

        // Return the state
        Self {
            start_time,

            space,
            handle,
            backend_data,

            socket_name,

            compositor_state,
            xdg_shell_state,
            xdg_decoration_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            seat,
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
        self.space
            .element_under(pos)
            .and_then(|(window, location)| {
                window
                    .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(s, p)| (s, p + location))
            })
    }
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
    fn early_import(&self, output: &Output);
    fn reset_buffers(&self, surface: &WlSurface);
}
