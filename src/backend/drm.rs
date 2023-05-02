use std::{collections::HashMap, os::fd::FromRawFd, sync::Mutex, time::Duration};

use super::{utils::CustomRenderElements, DrmSurfaceDmabufFeedback, UdevData};

use crate::{
    backend::get_surface_dmabuf_feedback,
    state::{post_repaint, take_presentation_feedback, SurfaceDmabufFeedback},
    CalloopData, Corrosion,
};
use smithay::{
    backend::{
        allocator::{
            dmabuf::Dmabuf,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
            Fourcc,
        },
        drm::{
            compositor::DrmCompositor, DrmDevice, DrmDeviceFd, DrmError, DrmEventMetadata, DrmNode,
            DrmSurface, GbmBufferedSurface,
        },
        egl::{display::EGLDisplay, EGLDevice},
        renderer::{
            damage::{Error as OutputDamageTrackerError, OutputDamageTracker},
            element::{
                surface::WaylandSurfaceRenderElement, texture::TextureBuffer, AsRenderElements,
                RenderElement, RenderElementStates,
            },
            gles::{GlesRenderer, GlesTexture},
            multigpu::{gbm::GbmGlesBackend, MultiRenderer},
            Bind, ExportMem, Offscreen, Renderer,
        },
        session::Session,
        SwapBuffersError,
    },
    desktop::{space, utils::OutputPresentationFeedback},
    input::pointer::{CursorImageAttributes, CursorImageStatus},
    output::{Mode as WlMode, Output, PhysicalProperties},
    reexports::{
        calloop::{timer::Timer, RegistrationToken},
        drm::{
            self,
            control::{connector, crtc::Handle as CrtcHandle, ModeTypeFlags},
            Device,
        },
        nix::fcntl::OFlag,
        wayland_protocols::wp::presentation_time::server::wp_presentation_feedback,
        wayland_server::{backend::GlobalId, protocol::wl_output::WlOutput, DisplayHandle},
    },
    utils::{DeviceFd, IsAlive, Scale, Transform},
    wayland::compositor,
};
use smithay_drm_extras::{
    drm_scanner::{
        DrmScanEvent::{Connected, Disconnected},
        DrmScanner,
    },
    edid::EdidInfo,
};
use std::path::Path;

const SUPPORTED_FORMATS: &[Fourcc] = &[Fourcc::Abgr8888, Fourcc::Argb8888];

type RenderingSurface =
    GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, Option<OutputPresentationFeedback>>;

type HardwareCompositor = DrmCompositor<
    GbmAllocator<DrmDeviceFd>,
    GbmDevice<DrmDeviceFd>,
    Option<OutputPresentationFeedback>,
    DrmDeviceFd,
>;

#[derive(PartialEq)]
pub struct UdevOutputId {
    pub crtc: CrtcHandle,
    pub device_id: DrmNode,
}

pub enum SurfaceComposition {
    Surface {
        surface: RenderingSurface,
        damage_tracker: OutputDamageTracker,
    },
    Compositor(HardwareCompositor),
}

type UdevRenderer<'a, 'b> =
    MultiRenderer<'a, 'a, 'b, GbmGlesBackend<GlesRenderer>, GbmGlesBackend<GlesRenderer>>;

pub struct SurfaceData {
    pub dh: DisplayHandle,
    pub compositor: SurfaceComposition,
    pub id: Option<GlobalId>,
    pub render_node: DrmNode,
    pub device_node: DrmNode,
    pub dmabuf_feedback: Option<DrmSurfaceDmabufFeedback>,
}

pub struct BackendData {
    pub token: RegistrationToken,
    pub scanner: DrmScanner,
    pub render_node: DrmNode,
    pub surfaces: HashMap<CrtcHandle, SurfaceData>,
    pub gbm: GbmDevice<DrmDeviceFd>,
    pub drm: DrmDevice,
}

impl Drop for SurfaceData {
    fn drop(&mut self) {
        if let Some(global) = self.id.take() {
            self.dh.remove_global::<WlOutput>(global);
        }
    }
}

impl SurfaceComposition {
    pub fn format(&self) -> smithay::reexports::gbm::Format {
        match self {
            SurfaceComposition::Compositor(compositor) => compositor.format(),
            Self::Surface {
                surface,
                damage_tracker: _,
            } => surface.format(),
        }
    }

    pub fn frame_submitted(
        &mut self,
    ) -> Result<Option<Option<OutputPresentationFeedback>>, SwapBuffersError> {
        match self {
            SurfaceComposition::Compositor(compositor) => compositor
                .frame_submitted()
                .map_err(Into::<SwapBuffersError>::into),

            Self::Surface { surface, .. } => surface
                .frame_submitted()
                .map_err(Into::<SwapBuffersError>::into),
        }
    }

    pub fn surface(&self) -> &DrmSurface {
        match self {
            SurfaceComposition::Compositor(compositor) => compositor.surface(),
            Self::Surface {
                surface,
                damage_tracker: _,
            } => surface.surface(),
        }
    }

    pub fn reset_buffers(&mut self) {
        match self {
            SurfaceComposition::Compositor(comp) => {
                comp.reset_buffers();
            }
            Self::Surface { surface, .. } => {
                surface.reset_buffers();
            }
        }
    }

    pub fn queue_frame(
        &mut self,
        user_data: Option<OutputPresentationFeedback>,
    ) -> Result<(), SwapBuffersError> {
        match self {
            SurfaceComposition::Compositor(comp) => comp
                .queue_frame(user_data)
                .map_err(Into::<SwapBuffersError>::into),
            Self::Surface { surface, .. } => surface
                .queue_buffer(None, user_data)
                .map_err(Into::<SwapBuffersError>::into),
        }
    }

    // hell
    fn render_frame<'a, R, E, Target>(
        &'a mut self,
        renderer: &mut R,
        elements: &'a [E],
        clear_color: [f32; 4],
    ) -> Result<(bool, RenderElementStates), SwapBuffersError>
    where
        R: Renderer + Bind<Dmabuf> + Bind<Target> + Offscreen<Target> + ExportMem,
        <R as Renderer>::TextureId: 'static,
        <R as Renderer>::Error: Into<SwapBuffersError>,
        E: RenderElement<R>,
    {
        match self {
            SurfaceComposition::Surface {
                surface,
                damage_tracker,
            } => {
                let (dmabuf, age) = surface
                    .next_buffer()
                    .map_err(Into::<SwapBuffersError>::into)?;
                renderer
                    .bind(dmabuf)
                    .expect("Unable to bind dmabuf to renderer");
                let res = damage_tracker
                    .render_output(renderer, age.into(), elements, clear_color)
                    .map(|(damage, states)| (damage.is_some(), states))
                    .map_err(|err| match err {
                        OutputDamageTrackerError::Rendering(err) => err.into(),
                        _ => unreachable!(),
                    });
                res
            }
            SurfaceComposition::Compositor(comp) => comp
                .render_frame(renderer, elements, clear_color)
                .map(|render_frame_result| {
                    (
                        render_frame_result.damage.is_some(),
                        render_frame_result.states,
                    )
                })
                .map_err(|err| match err {
                    smithay::backend::drm::compositor::RenderFrameError::PrepareFrame(err) => {
                        err.into()
                    }
                    smithay::backend::drm::compositor::RenderFrameError::RenderFrame(
                        OutputDamageTrackerError::Rendering(err),
                    ) => err.into(),
                    _ => unreachable!(),
                }),
        }
    }
}

impl Corrosion<UdevData> {
    pub fn device_added(&mut self, node: DrmNode, path: &Path) {
        // Opens the device file and returns a file descriptor to the file
        let fd = self
            .backend_data
            .session
            .open(
                path,
                OFlag::O_RDWR | OFlag::O_NOCTTY | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            )
            .expect("Unable to open device file");

        // We create our device structs
        let fd = DrmDeviceFd::new(unsafe { DeviceFd::from_raw_fd(fd) });
        let (drm, notifier) =
            DrmDevice::new(fd.clone(), true).expect("Could not create drm device");
        let gbm = GbmDevice::new(fd.clone()).expect("Could not create gbm device");

        // Insert the device's event source into the event loop
        let registration_token = self
            .handle
            .insert_source(
                notifier,
                move |event, meta, data: &mut CalloopData<_>| match event {
                    smithay::backend::drm::DrmEvent::VBlank(crtc) => {
                        data.state.frame_finish(node, crtc, meta)
                    }
                    smithay::backend::drm::DrmEvent::Error(err) => {
                        tracing::error!("{}", err);
                    }
                },
            )
            .expect("Unable to create registration token for drm event source");

        // Create a new EGL display with our gbm device and use it to find the render node
        let render_node = EGLDevice::device_for_display(&EGLDisplay::new(gbm.clone()).unwrap())
            .ok()
            .and_then(|x| x.try_get_render_node().ok().flatten())
            .unwrap_or(node);

        self.backend_data
            .gpu_manager
            .as_mut()
            .add_node(render_node, gbm.clone())
            .expect("Unable to add render node to renderer");

        // Insert the key(the node) and the value(the initialized BackendData struct) into the
        // backends hashmap of UdevData
        self.backend_data.backends.insert(
            node,
            BackendData {
                token: registration_token,
                scanner: DrmScanner::new(),
                render_node,
                surfaces: HashMap::new(),
                gbm,
                drm,
            },
        );
        self.device_changed(node);
    }

    // Gets called when the scanner of BackendData sees a connected connector
    pub fn connector_connected(
        &mut self,
        node: DrmNode,
        crtc: CrtcHandle,
        connector: connector::Info,
    ) {
        let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
            device
        } else {
            return;
        };

        let mut renderer = self
            .backend_data
            .gpu_manager
            .single_renderer(&device.render_node)
            .unwrap();
        let render_formats = renderer
            .as_mut()
            .egl_context()
            .dmabuf_render_formats()
            .clone();

        tracing::info!(
            ?crtc,
            "Setting up connector: {:?}-{}",
            connector.interface(),
            connector.interface_id()
        );

        // Get the preferred output mode
        let mode = connector
            .modes()
            .iter()
            .position(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
            .unwrap_or(0);

        let drm_mode = connector.modes()[mode];
        let wl_mode = WlMode::from(drm_mode);

        // Create the drm surface
        let surface = match device
            .drm
            .create_surface(crtc, drm_mode, &[connector.handle()])
        {
            Ok(surface) => surface,
            Err(err) => {
                tracing::error!("Failure to create drm surface: {}", err);
                return;
            }
        };
        let output_name = format!(
            "{}-{}",
            connector.interface().as_str(),
            connector.interface_id()
        );

        let (make, model) = EdidInfo::for_connector(&device.drm, connector.handle())
            .map(|info| (info.manufacturer, info.model))
            .unwrap();
        let (physical_width, physical_height) = connector.size().unwrap_or((0, 0));
        let output = Output::new(
            output_name,
            PhysicalProperties {
                size: (physical_width as i32, physical_height as i32).into(),
                subpixel: smithay::output::Subpixel::Unknown,
                make,
                model,
            },
        );
        let global = output.create_global::<Corrosion<UdevData>>(&self.display_handle);

        let x = self.space.outputs().fold(0, |acc, o| {
            acc + self.space.output_geometry(o).unwrap().size.w
        });

        let position = (x, 0).into();
        output.set_preferred(wl_mode);
        output.change_current_state(Some(wl_mode), None, None, Some(position));
        self.space.map_output(&output, position);

        output.user_data().insert_if_missing(|| UdevOutputId {
            crtc,
            device_id: node,
        });

        let allocator = GbmAllocator::new(
            device.gbm.clone(),
            GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
        );
        // We initialize the compositor.
        let compositor = if std::env::var("CORROSION_DISABLE_HARDWARE_COMPOSITOR").is_ok() {
            tracing::info!("Creating software-rendered compositor");
            let gbm_surface = match GbmBufferedSurface::new(
                surface,
                allocator,
                SUPPORTED_FORMATS,
                render_formats,
            ) {
                Ok(render_surface) => render_surface,
                Err(err) => {
                    tracing::error!("Error creating gbm rendering surface: {}", err);
                    return;
                }
            };
            SurfaceComposition::Surface {
                surface: gbm_surface,
                damage_tracker: OutputDamageTracker::from_output(&output),
            }
        } else {
            let drivers = match device.drm.get_driver() {
                Ok(driver) => driver,
                Err(err) => {
                    tracing::error!("Unable to get device driver: {}", err);
                    return;
                }
            };

            let mut planes = match surface.planes() {
                Ok(planes) => planes,
                Err(err) => {
                    tracing::error!("Unable to get surface planes: {}", err);
                    return;
                }
            };

            if drivers
                .name()
                .to_string_lossy()
                .to_lowercase()
                .contains("nvidia")
                || drivers
                    .description()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains("nvidia")
            {
                // Nvidia, frik you >:(
                // (Overlay planes on nvidia gpus break)
                planes.overlay = vec![];
            }

            let compositor = match DrmCompositor::new(
                &output,
                surface,
                Some(planes),
                allocator,
                device.gbm.clone(),
                SUPPORTED_FORMATS,
                render_formats,
                device.drm.cursor_size(),
                Some(device.gbm.clone()),
            ) {
                Ok(compositor) => compositor,
                Err(err) => {
                    tracing::error!("Error creating hardware-accelerated compositor: {}", err);
                    return;
                }
            };
            SurfaceComposition::Compositor(compositor)
        };

        let dmabuf_feedback = get_surface_dmabuf_feedback(
            self.backend_data.primary_gpu,
            device.render_node,
            &mut self.backend_data.gpu_manager,
            &compositor,
        );

        device.surfaces.insert(
            crtc,
            SurfaceData {
                dh: self.display_handle.clone(),
                compositor,
                id: Some(global),
                render_node: device.render_node,
                device_node: node,
                dmabuf_feedback,
            },
        );

        self.schedule_initial_render(node, crtc);
    }

    // Gets called when the device changes
    pub fn device_changed(&mut self, node: DrmNode) {
        let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
            device
        } else {
            return;
        };

        // Scans the device for any connectors
        for event in device.scanner.scan_connectors(&device.drm) {
            match event {
                Connected {
                    connector,
                    crtc: Some(crtc),
                } => {
                    self.connector_connected(node, crtc, connector);
                }
                Disconnected {
                    connector,
                    crtc: Some(crtc),
                } => {
                    self.connector_disconnected(node, connector, crtc);
                }
                _ => (),
            };
        }
    }

    pub fn device_removed(&mut self, node: DrmNode) {
        let device = if let Some(device) = self.backend_data.backends.get(&node) {
            device
        } else {
            return;
        };

        let crtcs: Vec<_> = device
            .scanner
            .crtcs()
            .map(|(info, crtc)| (info.clone(), crtc))
            .collect();

        for (connector, crtc) in crtcs {
            self.connector_disconnected(node, connector, crtc);
        }

        tracing::info!("Removed surfaces");

        if let Some(backend_data) = self.backend_data.backends.remove(&node) {
            self.backend_data.gpu_manager.as_mut().remove_node(&node);

            self.handle.remove(backend_data.token);

            tracing::debug!("Dropped device");
        }
    }

    pub fn frame_finish(
        &mut self,
        node: DrmNode,
        crtc: CrtcHandle,
        meta: &mut Option<DrmEventMetadata>,
    ) {
        let device = match self.backend_data.backends.get_mut(&node) {
            Some(device) => device,
            None => {
                tracing::error!("No backend of \"{}\" found", node);
                return;
            }
        };

        let surface = match device.surfaces.get_mut(&crtc) {
            Some(surface) => surface,
            None => {
                tracing::error!("Could not get surface data of crtc: {:?}", crtc);
                return;
            }
        };

        let output = if let Some(output) = self.space.outputs().find(|o| {
            o.user_data().get::<UdevOutputId>()
                == Some(&UdevOutputId {
                    device_id: surface.device_node,
                    crtc,
                })
        }) {
            output.clone()
        } else {
            return;
        };

        let schedule_render = match surface
            .compositor
            .frame_submitted()
            .map_err(Into::<SwapBuffersError>::into)
        {
            Ok(user_data) => {
                if let Some(mut feedback) = user_data.flatten() {
                    let tp = meta.as_ref().and_then(|metadata| match metadata.time {
                        smithay::backend::drm::DrmEventTime::Monotonic(time) => Some(time),
                        smithay::backend::drm::DrmEventTime::Realtime(_) => None,
                    });
                    let seq = meta.as_ref().map(|metadata| metadata.sequence).unwrap_or(0);

                    let (clock, flags) = if let Some(tp) = tp {
                        (
                            tp.into(),
                            wp_presentation_feedback::Kind::Vsync
                                | wp_presentation_feedback::Kind::HwClock
                                | wp_presentation_feedback::Kind::HwCompletion,
                        )
                    } else {
                        (self.clock.now(), wp_presentation_feedback::Kind::Vsync)
                    };

                    feedback.presented(
                        clock,
                        output
                            .current_mode()
                            .map(|mode| mode.refresh as u32)
                            .unwrap_or_default(),
                        seq as u64,
                        flags,
                    );
                }
                true
            }
            Err(err) => {
                tracing::error!("Error occurred wile rendering: {}", err);
                match err {
                    SwapBuffersError::AlreadySwapped => true,
                    SwapBuffersError::TemporaryFailure(err)
                        if matches!(
                            err.downcast_ref::<DrmError>(),
                            Some(&DrmError::DeviceInactive)
                        ) =>
                    {
                        false
                    }

                    SwapBuffersError::TemporaryFailure(err) => matches!(
                        err.downcast_ref::<DrmError>(),
                        Some(&DrmError::Access {
                            source: drm::SystemError::PermissionDenied,
                            ..
                        })
                    ),

                    SwapBuffersError::ContextLost(err) => {
                        panic!("Rendering loop has been lost: {}", err)
                    }
                }
            }
        };

        if schedule_render {
            let output_refresh = match output.current_mode() {
                Some(mode) => mode.refresh,
                None => {
                    return;
                }
            };

            let repaint_delay =
                Duration::from_millis(((1_000_000f32 / output_refresh as f32) * 0.6f32) as u64);
            let timer = if self.backend_data.primary_gpu != surface.render_node {
                tracing::info!("Scheduling repaint timer for {:?} immediately", crtc);
                Timer::immediate()
            } else {
                tracing::info!(
                    "Scheduling repaint timer for {:?} with a delay of {:?}",
                    crtc,
                    repaint_delay
                );
                Timer::from_duration(repaint_delay)
            };

            self.handle
                .insert_source(timer, move |_, _, data| {
                    data.state.render_surface(node, crtc);
                    smithay::reexports::calloop::timer::TimeoutAction::Drop
                })
                .expect("Unable to insert rendering function into event loop");
        }
    }

    pub fn connector_disconnected(
        &mut self,
        node: DrmNode,
        connector: connector::Info,
        crtc: CrtcHandle,
    ) {
        let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
            device
        } else {
            return;
        };

        tracing::info!(
            "Connector {:?}-{} Disconnected",
            connector.interface(),
            connector.interface_id()
        );

        device.surfaces.remove(&crtc);

        let output = self
            .space
            .outputs()
            .find(|output| {
                output
                    .user_data()
                    .get::<UdevOutputId>()
                    .map(|id| id.device_id == node && id.crtc == crtc)
                    .unwrap_or(false)
            })
            .cloned();
        if let Some(output) = output {
            self.space.unmap_output(&output);
        }
    }
    pub fn render_surface(&mut self, node: DrmNode, crtc: CrtcHandle) {
        let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
            device
        } else {
            return;
        };

        let surface = if let Some(surface) = device.surfaces.get_mut(&crtc) {
            surface
        } else {
            return;
        };

        let frame = self
            .backend_data
            .cursor_image
            .get_image(1, self.clock.now().try_into().unwrap());

        let render_node = surface.render_node;
        let primary_gpu = self.backend_data.primary_gpu;
        let mut renderer = if primary_gpu == render_node {
            self.backend_data.gpu_manager.single_renderer(&render_node)
        } else {
            let format = surface.compositor.format();
            self.backend_data.gpu_manager.renderer(
                &primary_gpu,
                &render_node,
                self.backend_data
                    .allocator
                    .as_mut()
                    .expect("No allocator found")
                    .as_mut(),
                format,
            )
        }
        .unwrap();

        let pointer_images = &mut self.backend_data.cursor_images;
        let pointer_image = pointer_images
            .iter()
            .find_map(|(image, texture)| {
                if image == &frame {
                    Some(texture.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                let texture = TextureBuffer::from_memory(
                    &mut renderer,
                    &frame.pixels_rgba,
                    Fourcc::Abgr8888,
                    (frame.width as i32, frame.height as i32),
                    false,
                    1,
                    Transform::Normal,
                    None,
                )
                .expect("Failed to import cursor bitmap");
                pointer_images.push((frame, texture.clone()));
                texture
            });

        let output = if let Some(output) = self.space.outputs().find(|o| {
            o.user_data().get()
                == Some(&UdevOutputId {
                    crtc,
                    device_id: surface.device_node,
                })
        }) {
            output.clone()
        } else {
            return;
        };

        let mut elements: Vec<CustomRenderElements<_, _>> = Vec::new();

        let output_geometry = self.space.output_geometry(&output).unwrap();
        let scale = Scale::from(output.current_scale().fractional_scale());
        if output_geometry.to_f64().contains(self.pointer_location) {
            let cursor_hotspot = if let CursorImageStatus::Surface(ref surface) =
                *self.cursor_image_status.lock().unwrap()
            {
                compositor::with_states(surface, |states| {
                    states
                        .data_map
                        .get::<Mutex<CursorImageAttributes>>()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .hotspot
                })
            } else {
                (0, 0).into()
            };
            let cursor_pos =
                self.pointer_location - output_geometry.loc.to_f64() - cursor_hotspot.to_f64();
            let cursor_pos_scaled = cursor_pos.to_physical(scale).to_i32_round();

            // set cursor
            self.backend_data
                .pointer_element
                .set_texture(pointer_image.clone());

            // draw the cursor as relevant
            {
                // reset the cursor if the surface is no longer alive
                let mut reset = false;
                if let CursorImageStatus::Surface(ref surface) =
                    *self.cursor_image_status.lock().unwrap()
                {
                    reset = !surface.alive();
                }
                if reset {
                    *self.cursor_image_status.lock().unwrap() = CursorImageStatus::Default;
                }

                self.backend_data
                    .pointer_element
                    .set_status(self.cursor_image_status.lock().unwrap().clone());
            }

            elements.extend(self.backend_data.pointer_element.render_elements(
                &mut renderer,
                cursor_pos_scaled,
                scale,
            ));
        }

        elements.extend(
            space::space_render_elements(&mut renderer, [&self.space], &output)
                .expect("Output without mode")
                .into_iter()
                .map(|element| CustomRenderElements::Space(element)),
        );
        let (rendered, states) = surface
            .compositor
            .render_frame::<_, _, GlesTexture>(
                &mut renderer,
                &elements,
                [0.2f32, 0.05f32, 0.6f32, 1.0f32],
            )
            .unwrap();

        post_repaint(
            &output,
            &states,
            &self.space,
            surface
                .dmabuf_feedback
                .as_ref()
                .map(|feedback| SurfaceDmabufFeedback {
                    render_feedback: &feedback.render_feedback,
                    scanout_feedback: &feedback.scanout_feedback,
                }),
            self.clock.now(),
        );
        if rendered {
            let output_feedback = take_presentation_feedback(&output, &self.space, &states);
            surface
                .compositor
                .queue_frame(Some(output_feedback))
                .unwrap();
        } else {
            let output_refresh = match output.current_mode() {
                Some(mode) => mode.refresh,
                None => return,
            };
            // If reschedule is true we either hit a temporary failure or more likely rendering
            // did not cause any damage on the output. In this case we just re-schedule a repaint
            // after approx. one frame to re-test for damage.
            let reschedule_duration =
                Duration::from_millis((1_000_000f32 / output_refresh as f32) as u64);
            tracing::trace!(
                "reschedule repaint timer with delay {:?} on {:?}",
                reschedule_duration,
                crtc,
            );
            let timer = Timer::from_duration(reschedule_duration);
            self.handle
                .insert_source(timer, move |_, _, data| {
                    data.state.render_surface(node, crtc);
                    smithay::reexports::calloop::timer::TimeoutAction::Drop
                })
                .expect("failed to schedule frame timer");
        }
    }

    pub fn schedule_initial_render(&mut self, node: DrmNode, crtc: CrtcHandle) {
        let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
            device
        } else {
            return;
        };

        let mut surface = if let Some(surface) = device.surfaces.get_mut(&crtc) {
            surface
        } else {
            return;
        };

        let node = surface.render_node;
        let mut renderer = self
            .backend_data
            .gpu_manager
            .single_renderer(&node)
            .unwrap();

        initial_render(&mut surface, &mut renderer);
    }
}

fn initial_render(surface: &mut SurfaceData, renderer: &mut UdevRenderer<'_, '_>) {
    surface
        .compositor
        .render_frame::<_, CustomRenderElements<_, WaylandSurfaceRenderElement<_>>, GlesTexture>(
            renderer,
            &[],
            [0.2f32, 0.05f32, 0.6f32, 1.0f32],
        )
        .expect("Unable to render");
    surface.compositor.queue_frame(None).unwrap();
    surface.compositor.reset_buffers();
}
