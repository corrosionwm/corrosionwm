use std::collections::HashMap;

use smithay::{
    backend::{
        allocator::{
            dmabuf::{AnyError, Dmabuf},
            Allocator,
        },
        drm::{DrmNode, NodeType},
        renderer::{
            gles::GlesRenderer,
            multigpu::{gbm::GbmGlesBackend, GpuManager},
        },
        session::libseat::LibSeatSession,
        session::Session,
        udev::{self, UdevBackend},
    },
    reexports::{
        calloop::{EventLoop, LoopSignal},
        wayland_server::{protocol::wl_surface::WlSurface, Display},
    },
};

use self::drm::BackendData;
use crate::{state::Backend, Corrosion};

mod drm;

struct UdevData {
    pub loop_signal: LoopSignal,
    pub session: LibSeatSession,
    primary_gpu: DrmNode,
    allocator: Option<Box<dyn Allocator<Buffer = Dmabuf, Error = AnyError>>>,
    gpu_manager: GpuManager<GbmGlesBackend<GlesRenderer>>,
    backends: HashMap<DrmNode, BackendData>,
}

impl Backend for UdevData {
    fn early_import(&mut self, output: &WlSurface) {
        match self
            .gpu_manager
            .early_import(Some(self.primary_gpu), self.primary_gpu, output)
        {
            Ok(()) => {}
            Err(err) => tracing::error!("Error on early buffer import: {}", err),
        };
    }

    fn loop_signal(&self) -> &LoopSignal {
        &self.loop_signal
    }

    fn reset_buffers(&self, _surface: &smithay::output::Output) {
        todo!();
    }

    fn seat_name(&self) -> String {
        self.session.seat()
    }
}

pub fn initialize_backend() {
    let event_loop = EventLoop::try_new().expect("Unable to initialize event loop");
    let (session, mut _notifier) = match LibSeatSession::new() {
        Ok((session, notifier)) => (session, notifier),
        Err(err) => {
            tracing::error!("Error in creating libseat session: {}", err);
            return;
        }
    };
    let mut display = Display::new().expect("Unable to create wayland display");

    let primary_gpu = udev::primary_gpu(&session.seat())
        .unwrap()
        .and_then(|p| {
            DrmNode::from_path(p)
                .ok()
                .expect("Unable to create drm node")
                .node_with_type(NodeType::Render)
                .expect("Unable to create drm node")
                .ok()
        })
        .unwrap_or_else(|| {
            udev::all_gpus(&session.seat())
                .unwrap()
                .into_iter()
                .find_map(|g| DrmNode::from_path(g).ok())
                .expect("no gpu")
        });

    tracing::info!("Using {} as a primary gpu", primary_gpu);

    let gpus = GpuManager::new(GbmGlesBackend::default()).unwrap();

    let data = UdevData {
        loop_signal: event_loop.get_signal(),
        session,
        primary_gpu,
        allocator: None,
        gpu_manager: gpus,
        backends: HashMap::new(),
    };
    let mut state = Corrosion::new(event_loop.handle(), &mut display, data);
    let backend = match UdevBackend::new(&state.seat_name) {
        Ok(backend) => backend,
        Err(err) => {
            tracing::error!("Unable to create udev backend: {}", err);
            return;
        }
    };

    for (dev, path) in backend.device_list() {
        state.device_added(DrmNode::from_dev_id(dev).unwrap(), &path);
    }

    event_loop
        .handle()
        .insert_source(backend, move |event, _, data| match event {
            udev::UdevEvent::Added { device_id, path } => {
                data.state
                    .device_added(DrmNode::from_dev_id(device_id).unwrap(), &path);
            }
            udev::UdevEvent::Changed { device_id } => {
                data.state
                    .device_changed(DrmNode::from_dev_id(device_id).unwrap());
            }
            _ => (),
        })
        .expect("Error inserting event loop source");
}
