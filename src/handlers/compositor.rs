use crate::{grabs::resize_grab, state::Backend, Corrosion};
use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    delegate_compositor, delegate_shm,
    reexports::wayland_server::protocol::{wl_buffer, wl_surface::WlSurface},
    wayland::{
        buffer::BufferHandler,
        compositor::{get_parent, is_sync_subsurface, CompositorHandler, CompositorState},
        shm::{ShmHandler, ShmState},
    },
};

use super::xdg_shell;

impl<BackendData: Backend> CompositorHandler for Corrosion<BackendData> {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler(surface);
        self.backend_data.early_import(surface);
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self
                .space
                .elements()
                .find(|w| w.toplevel().wl_surface() == &root)
            {
                window.on_commit();
            }
        };

        xdg_shell::handle_commit(&self.space, surface);
        resize_grab::handle_commit(&mut self.space, surface);
    }
}

impl<BackendData: Backend> BufferHandler for Corrosion<BackendData> {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl<BackendData: Backend + 'static> ShmHandler for Corrosion<BackendData> {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

delegate_compositor!(@<BackendData: Backend + 'static> Corrosion<BackendData>);
delegate_shm!(@<BackendData: Backend + 'static> Corrosion<BackendData>);
