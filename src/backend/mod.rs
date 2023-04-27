use std::collections::{HashMap, HashSet};

#[cfg(feature = "egl")]
use smithay::backend::renderer::ImportEgl;
use smithay::{
    backend::{
        allocator::{
            dmabuf::{AnyError, Dmabuf, DmabufAllocator},
            gbm::{GbmAllocator, GbmBufferFlags},
            Allocator,
        },
        drm::{DrmNode, NodeType},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            gles::GlesRenderer,
            multigpu::{gbm::GbmGlesBackend, GpuManager},
            ImportDma, ImportMemWl,
        },
        session::libseat::LibSeatSession,
        session::Session,
        udev::{self, UdevBackend},
    },
    delegate_dmabuf,
    reexports::{
        calloop::{EventLoop, LoopSignal},
        input::Libinput,
        wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_feedback_v1,
        wayland_server::{protocol::wl_surface::WlSurface, Display},
    },
    wayland::dmabuf::{
        DmabufFeedback, DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState,
        ImportError,
    },
};

use self::drm::{BackendData, SurfaceComposition};
use crate::{state::Backend, CalloopData, Corrosion};

mod drm;

struct UdevData {
    pub loop_signal: LoopSignal,
    pub session: LibSeatSession,
    primary_gpu: DrmNode,
    dmabuf_state: Option<(DmabufState, DmabufGlobal)>,
    allocator: Option<Box<dyn Allocator<Buffer = Dmabuf, Error = AnyError>>>,
    gpu_manager: GpuManager<GbmGlesBackend<GlesRenderer>>,
    backends: HashMap<DrmNode, BackendData>,
}

impl DmabufHandler for Corrosion<UdevData> {
    fn dmabuf_state(&mut self) -> &mut smithay::wayland::dmabuf::DmabufState {
        &mut self.backend_data.dmabuf_state.as_mut().unwrap().0
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
    ) -> Result<(), smithay::wayland::dmabuf::ImportError> {
        self.backend_data
            .gpu_manager
            .single_renderer(&self.backend_data.primary_gpu)
            .and_then(|mut renderer| renderer.import_dmabuf(&dmabuf, None))
            .map(|_| ())
            .map_err(|_| ImportError::Failed)
    }
}
delegate_dmabuf!(Corrosion<UdevData>);

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
    let mut event_loop = EventLoop::try_new().expect("Unable to initialize event loop");
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
        dmabuf_state: None,
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

    state.shm_state.update_formats(
        state
            .backend_data
            .gpu_manager
            .single_renderer(&primary_gpu)
            .unwrap()
            .shm_formats(),
    );

    let mut libinput_context = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(
        state.backend_data.session.clone().into(),
    );
    libinput_context.udev_assign_seat(&state.seat_name).unwrap();
    let libinput_backend = LibinputInputBackend::new(libinput_context);

    state
        .handle
        .insert_source(libinput_backend, move |event, _, data| {
            data.state.process_input_event(event);
        })
        .unwrap();

    let gbm = state
        .backend_data
        .backends
        .get(&primary_gpu)
        // If the primary_gpu failed to initialize, we likely have a kmsro device
        .or_else(|| state.backend_data.backends.values().next())
        // Don't fail, if there is no allocator. There is a chance, that this a single gpu system and we don't need one.
        .map(|backend| backend.gbm.clone());
    state.backend_data.allocator = gbm.map(|gbm| {
        Box::new(DmabufAllocator(GbmAllocator::new(
            gbm,
            GbmBufferFlags::RENDERING,
        ))) as Box<_>
    });
    #[cfg_attr(not(feature = "egl"), allow(unused_mut))]
    let mut renderer = state
        .backend_data
        .gpu_manager
        .single_renderer(&primary_gpu)
        .unwrap();

    #[cfg(feature = "egl")]
    {
        match renderer.bind_wl_display(&state.display_handle) {
            Ok(_) => tracing::info!("Enabled egl hardware acceleration"),
            Err(err) => tracing::error!("Error in enabling egl hardware acceleration: {:?}", err),
        }
    }

    let dmabuf_formats = renderer.dmabuf_formats().collect::<Vec<_>>();
    let default_feedback = DmabufFeedbackBuilder::new(primary_gpu.dev_id(), dmabuf_formats)
        .build()
        .unwrap();
    let mut dmabuf_state = DmabufState::new();
    let dmabuf_global = dmabuf_state.create_global_with_default_feedback::<Corrosion<UdevData>>(
        &display.handle(),
        &default_feedback,
    );
    state.backend_data.dmabuf_state = Some((dmabuf_state, dmabuf_global));

    let gpus = &mut state.backend_data.gpu_manager;
    state
        .backend_data
        .backends
        .values_mut()
        .for_each(|backend_data| {
            backend_data.surfaces.values_mut().for_each(|surface_data| {
                surface_data.dmabuf_feedback = surface_data.dmabuf_feedback.take().or_else(|| {
                    get_surface_dmabuf_feedback(
                        primary_gpu,
                        surface_data.render_node,
                        gpus,
                        &surface_data.compositor,
                    )
                });
            });
        });
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
            udev::UdevEvent::Removed { device_id } => {
                data.state
                    .device_removed(DrmNode::from_dev_id(device_id).unwrap());
            }
        })
        .expect("Error inserting event loop source");

    std::env::set_var("WAYLAND_DISPLAY", &state.socket_name);
    let mut calloop_data = CalloopData { state, display };

    event_loop
        .run(
            std::time::Duration::from_millis(16),
            &mut calloop_data,
            |data| {
                data.state.space.refresh();
                data.display.flush_clients().unwrap();
            },
        )
        .unwrap();
}

pub struct DrmSurfaceDmabufFeedback {
    render_feedback: DmabufFeedback,
    scanout_feedback: DmabufFeedback,
}

fn get_surface_dmabuf_feedback(
    primary_node: DrmNode,
    render_node: DrmNode,
    gpus: &mut GpuManager<GbmGlesBackend<GlesRenderer>>,
    composition: &SurfaceComposition,
) -> Option<DrmSurfaceDmabufFeedback> {
    let primary_formats = gpus
        .single_renderer(&primary_node)
        .ok()?
        .dmabuf_formats()
        .collect::<HashSet<_>>();

    let render_formats = gpus
        .single_renderer(&render_node)
        .ok()?
        .dmabuf_formats()
        .collect::<HashSet<_>>();

    let all_render_formats = primary_formats
        .iter()
        .chain(render_formats.iter())
        .copied()
        .collect::<HashSet<_>>();

    let surface = composition.surface();
    let planes = surface.planes().unwrap();

    let planes_formats = surface
        .supported_formats(planes.primary.handle)
        .unwrap()
        .into_iter()
        .chain(
            planes
                .overlay
                .iter()
                .flat_map(|p| surface.supported_formats(p.handle).unwrap()),
        )
        .collect::<HashSet<_>>()
        .intersection(&all_render_formats)
        .copied()
        .collect::<Vec<_>>();

    let builder = DmabufFeedbackBuilder::new(primary_node.dev_id(), primary_formats);
    let render_feedback = builder
        .clone()
        .add_preference_tranche(render_node.dev_id(), None, render_formats.clone())
        .build()
        .unwrap();

    let scanout_feedback = builder
        .clone()
        .add_preference_tranche(
            surface.device_fd().dev_id().unwrap(),
            Some(zwp_linux_dmabuf_feedback_v1::TrancheFlags::Scanout),
            planes_formats,
        )
        .add_preference_tranche(render_node.dev_id(), None, render_formats)
        .build()
        .unwrap();

    Some(DrmSurfaceDmabufFeedback {
        render_feedback,
        scanout_feedback,
    })
}
