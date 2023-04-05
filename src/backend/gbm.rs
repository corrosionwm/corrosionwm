use std::os::fd::FromRawFd;

use smithay::{
    backend::{
        allocator::{
            dmabuf::{AnyError, Dmabuf},
            gbm::GbmDevice,
            Allocator, Fourcc, Modifier,
        },
        drm::{DrmDeviceFd, DrmNode},
        session::{libseat::LibSeatSession, Session},
    },
    reexports::{
        drm::control::{
            connector::{Handle as ConnectorHandle, State},
            crtc::Handle as CrtcHandle,
            Device,
        },
        gbm::BufferObjectFlags,
        nix::{fcntl::OFlag, sys::stat::dev_t},
    },
    utils::DeviceFd,
};
use std::path::Path;

fn render(device: &GbmDevice<DrmDeviceFd>) {
    let resource_handle = device
        .resource_handles()
        .expect("Unable to get resource handles");
    let mut connectors: Vec<ConnectorHandle> = Vec::new();
    let mut valid_crtcs: Vec<CrtcHandle> = Vec::new();
    for handle in resource_handle.connectors() {
        match device.get_connector(*handle, false) {
            Ok(info) => match info.state() {
                State::Connected => {
                    // If it's connected, add it to the connector vec
                    connectors.push(*handle);
                    tracing::debug!("Found connector: {:?}", info.interface());
                }
                State::Disconnected => {
                    // If it's disconnected, do nothing
                }
                State::Unknown => {
                    // Wtf? why is it unknown? Warn the user about a possibly broken connector
                    tracing::warn!(
                        "Unable to get state of connector with interface: {:?}\nPossibly broken connector",
                        info.interface()
                    );
                }
            },
            Err(err) => {
                tracing::error!("Unable to get connector info: {:?}", err);
            }
        }
    }
    for handle in resource_handle.crtcs() {
        match device.get_crtc(*handle) {
            Ok(info) => {
                match info.framebuffer() {
                    Some(_) => {
                        // Crtc is already assigned a framebuffer. Do nothing
                        tracing::debug!("Crtc already in use: {:?}", handle);
                    }
                    None => {
                        // If crtc isn't already in use, add it to the list of valid crtcs
                        valid_crtcs.push(*handle);
                        tracing::debug!("Found valid crtc: {:?}", handle)
                    }
                }
            }
            Err(err) => {
                tracing::error!("Unable to get information about a crtc: {:?}", err);
            }
        }
    }
    // Create a render buffer to store the pixel data
    let mut buffer = device
        .create_buffer_object::<()>(
            1920, // IMPORTANT! i may be wrong, but you should not guess the size of the buffer. You should get the size of the connector
            1080,
            Fourcc::Abgr8888, // maybe add support for other formats? i dunno
            BufferObjectFlags::RENDERING | BufferObjectFlags::SCANOUT,
        )
        .expect("Unable to allocate render buffer");
    let pixel_data = {
        let mut data = Vec::new();
        for i in 0..1920 {
            for _ in 0..1080 { // again, i may be wrong, but you should not guess the size of the buffer. You should get the size of the connector
                data.push(if i % 2 == 0 { 0 } else { 255 });
            }
        }
        data
    };
    buffer
        .write(&pixel_data)
        .expect("Unable to write to buffer")
        .expect("Unable to write to buffer"); // ignore the double expect for now

    let fb = &device.add_framebuffer(&buffer, 32, 32).unwrap(); // i may be wrong but dont guess the color depth

    device
        .set_crtc(
            *valid_crtcs.get(0).unwrap(), // why are you only using the first crtc? you should use all of them
            Some(*fb),
            (0, 0),
            &connectors,
            Some(
                *device
                    .get_connector(*connectors.get(0).unwrap(), false)
                    .unwrap()
                    .modes()
                    .get(0)
                    .unwrap(),
            ),
        )
        .expect("Unable to set crtc for display");
}

pub fn run_gbm(session: &mut LibSeatSession, dev_id: dev_t, path: &Path) {
    let node = DrmNode::from_dev_id(dev_id);
    tracing::debug!("Found drm node: {:?}", node);
    let device_file = unsafe {
        DeviceFd::from_raw_fd(
            session
                .open(
                    path,
                    OFlag::O_RDWR | OFlag::O_CLOEXEC | OFlag::O_NOCTTY | OFlag::O_NONBLOCK,
                )
                .expect("Failed to open drm node"),
        )
    };
    tracing::info!("Opened drm node: {:?}", device_file);
    let fd = DrmDeviceFd::new(device_file);
    let device = GbmDevice::new(fd).unwrap();
    render(&device);
}

fn allocate_buffer(
    allocator: &mut Box<dyn Allocator<Buffer = Dmabuf, Error = AnyError>>,
) -> Dmabuf {
    match allocator.create_buffer(
        0,
        0,
        Fourcc::Abgr8888,
        &[Modifier::Linear, Modifier::Generic_16_16_tile],
    ) {
        Ok(buffer) => {
            tracing::debug!("Created a gbm buffer: {:?}", buffer);
            return buffer;
        }
        Err(err) => {
            tracing::error!("Error creating a gbm buffer: {}", err);
            panic!();
        }
    }
}
