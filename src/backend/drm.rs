use std::{collections::HashMap, os::fd::FromRawFd};

use super::UdevData;
use crate::{state::Backend, Corrosion};
use smithay::{
    backend::{
        allocator::{
            dmabuf::{AnyError, Dmabuf},
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
            Allocator, Fourcc, Modifier,
        },
        drm::{compositor::DrmCompositor, DrmDevice, DrmDeviceFd, DrmNode, GbmBufferedSurface},
        egl::{display::EGLDisplay, EGLDevice},
        renderer::damage::OutputDamageTracker,
        session::{libseat::LibSeatSession, Session},
    },
    desktop::utils::OutputPresentationFeedback,
    output::{Mode as WlMode, Output, PhysicalProperties},
    reexports::{
        calloop::RegistrationToken,
        drm::{
            control::{
                connector::{self, Handle as ConnectorHandle, State},
                crtc::Handle as CrtcHandle,
                ModeTypeFlags,
            },
            Device,
        },
        gbm::BufferObjectFlags,
        nix::{fcntl::OFlag, sys::stat::dev_t},
        wayland_server::{backend::GlobalId, DisplayHandle},
    },
    utils::DeviceFd,
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

struct UdevOutputId {
    crtc: CrtcHandle,
    device_id: DrmNode,
}

enum SurfaceComposition {
    Surface {
        surface: RenderingSurface,
        damage_tracker: OutputDamageTracker,
    },
    Compositor(HardwareCompositor),
}

pub struct SurfaceData {
    dh: DisplayHandle,
    compositor: SurfaceComposition,
    id: Option<GlobalId>,
    render_node: DrmNode,
    device_node: DrmNode,
}

pub struct BackendData {
    token: RegistrationToken,
    scanner: DrmScanner,
    render_node: DrmNode,
    surfaces: HashMap<CrtcHandle, SurfaceData>,
    gbm: GbmDevice<DrmDeviceFd>,
    drm: DrmDevice,
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
            .insert_source(notifier, |event, _, _| match event {
                smithay::backend::drm::DrmEvent::VBlank(_crtc) => {
                    tracing::info!("VBlank event occurred");
                }
                smithay::backend::drm::DrmEvent::Error(err) => {
                    tracing::error!("{}", err);
                }
            })
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
            .as_ref()
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

        device.surfaces.insert(
            crtc,
            SurfaceData {
                dh: self.display_handle.clone(),
                compositor,
                id: Some(global),
                render_node: device.render_node,
                device_node: node,
            },
        );
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
}
