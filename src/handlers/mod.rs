// mod.rs
// modules for the src/handlers directory

// modules
mod compositor;
pub mod keybindings;
mod xdg_shell;

// imports
use crate::state::Backend;
use crate::Corrosion;

// Wl Seat

use smithay::input::{SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, ServerDndGrabHandler,
};
use smithay::{delegate_data_device, delegate_output, delegate_seat};

impl<BackendData: Backend + 'static> SeatHandler for Corrosion<BackendData> {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Corrosion<BackendData>> {
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

delegate_seat!(@<BackendData: Backend + 'static> Corrosion<BackendData>);

//
// Wl Data Device
//

impl<BackendData: Backend + 'static> DataDeviceHandler for Corrosion<BackendData> {
    fn data_device_state(&self) -> &smithay::wayland::data_device::DataDeviceState {
        &self.data_device_state
    }
}

impl<BackendData: Backend + 'static> ClientDndGrabHandler for Corrosion<BackendData> {}
impl<BackendData: Backend + 'static> ServerDndGrabHandler for Corrosion<BackendData> {}

delegate_data_device!(@<BackendData: Backend + 'static> Corrosion<BackendData>);

//
// Wl Output & Xdg Output
//

delegate_output!(@<BackendData: Backend + 'static> Corrosion<BackendData>);
