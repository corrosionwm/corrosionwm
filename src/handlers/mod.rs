// mod.rs
// modules for the src/handlers directory

// modules
mod compositor;
pub mod keybindings;
mod xdg_shell;

// imports
use crate::Neko;

// Wl Seat

use smithay::input::{SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, ServerDndGrabHandler,
};
use smithay::{delegate_data_device, delegate_output, delegate_seat};

impl SeatHandler for Neko {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Neko> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &smithay::input::Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus,
    ) {
    }
    fn focus_changed(&mut self, _seat: &smithay::input::Seat<Self>, _focused: Option<&WlSurface>) {}
}

delegate_seat!(Neko);

//
// Wl Data Device
//

impl DataDeviceHandler for Neko {
    fn data_device_state(&self) -> &smithay::wayland::data_device::DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for Neko {}
impl ServerDndGrabHandler for Neko {}

delegate_data_device!(Neko);

//
// Wl Output & Xdg Output
//

delegate_output!(Neko);
