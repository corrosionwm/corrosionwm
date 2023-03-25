use smithay::{
    backend::{
        allocator::{
            dmabuf::{AnyError, Dmabuf, DmabufAllocator},
            gbm::GbmAllocator,
            Allocator,
        },
        drm::{DrmNode, NodeType},
        renderer::{
            gles2::Gles2Renderer,
            multigpu::{
                gbm::{GbmGlesBackend, GbmGlesDevice},
                GpuManager,
            },
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

use crate::{state::Backend, CalloopData, Corrosion};

mod gbm;

struct UdevData {
    pub loop_signal: LoopSignal,
    pub session: LibSeatSession,
    primary_gpu: DrmNode,
    allocator: Option<Box<dyn Allocator<Buffer = Dmabuf, Error = AnyError>>>,
    gpu_manager: GpuManager<GbmGlesBackend<Gles2Renderer>>,
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

    fn reset_buffers(&self, surface: &smithay::output::Output) {
        todo!();
    }

    fn seat_name(&self) -> String {
        self.session.seat()
    }
}

pub fn initialize_backend() {
    let event_loop = EventLoop::try_new().expect("Unable to initialize event loop");
    let (mut session, mut notifier) = match LibSeatSession::new() {
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
    };
    let state = Corrosion::new(event_loop.handle(), &mut display, data);
    let backend = match UdevBackend::new(&state.seat_name) {
        Ok(backend) => backend,
        Err(err) => {
            tracing::error!("Unable to create udev backend: {}", err);
            return;
        }
    };

    event_loop
        .handle()
        .insert_source(backend, move |event, _, data| match event {
            udev::UdevEvent::Added { device_id, path } => {
                tracing::info!("Device id: {:?} added with path {:?}", device_id, path);
            }
            _ => (),
        })
        .expect("Error inserting event loop source");
}
