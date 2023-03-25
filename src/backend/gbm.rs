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
    reexports::nix::fcntl::OFlag,
    utils::DeviceFd,
};

fn run_gbm(
    session: &mut LibSeatSession,
    node: &DrmNode,
    allocator: &mut Box<dyn Allocator<Buffer = Dmabuf, Error = AnyError>>,
) {
    let device_file = unsafe {
        DeviceFd::from_raw_fd(
            session
                .open(
                    node.dev_path().unwrap().as_path(),
                    OFlag::O_NOCTTY | OFlag::O_RDWR | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
                )
                .unwrap(),
        )
    };
    let fd = DrmDeviceFd::new(device_file);
    let device = GbmDevice::new(fd);
    let buffer = allocate_buffer(allocator);
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
            return buffer;
        }
        Err(err) => {
            tracing::error!("Error creating a gbm buffer: {}", err);
            panic!();
        }
    }
}
