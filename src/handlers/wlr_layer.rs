use smithay::{
    delegate_layer_shell,
    desktop::{self, LayerSurface},
    output::Output,
    wayland::shell::wlr_layer::{WlrLayerShellHandler, WlrLayerShellState},
};

use crate::state::{Backend, Corrosion};

impl<BackendData: Backend + 'static> WlrLayerShellHandler for Corrosion<BackendData> {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.wlr_layer_state
    }

    fn new_layer_surface(
        &mut self,
        surface: smithay::wayland::shell::wlr_layer::LayerSurface,
        output: Option<smithay::reexports::wayland_server::protocol::wl_output::WlOutput>,
        layer: smithay::wayland::shell::wlr_layer::Layer,
        namespace: String,
    ) {
        let output = output
            .as_ref()
            .and_then(Output::from_resource)
            .unwrap_or_else(|| self.space.outputs().next().unwrap().clone());
        tracing::debug!(
            "New layer surface created with the name of: {}, using layer: {:?}",
            &namespace,
            layer
        );
        let mut map = desktop::layer_map_for_output(&output);
        map.map_layer(&LayerSurface::new(surface, namespace))
            .unwrap();
    }
}

delegate_layer_shell!(@<BackendData: Backend + 'static>Corrosion<BackendData>);
