#![allow(irrefutable_let_patterns)]

// modules
mod handlers;

mod backend;
mod config;
mod drawing;
mod grabs;
mod input;
mod state;
mod winit;

// imports
pub use crate::config::{CorrosionConfig, Defaults};
use crate::winit::{self as winit_corrosion, WinitData};
use smithay::reexports::wayland_server::Display;
use state::Backend;
pub use state::Corrosion;
use std::env;
use std::process::Command;
use which;

pub struct CalloopData<BackendData: Backend + 'static> {
    state: Corrosion<BackendData>,
    display: Display<Corrosion<BackendData>>,
}

fn find_term(defaults: &Defaults) -> Option<&String> {
    let terminal = &defaults.terminal;
    if which::which(terminal).is_ok() {
        return Some(terminal);
    }
    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_env("CORROSIONWM_LOG") {
        // change this by changing the RUST_LOG environment variable
        tracing::info!("logging initialized with env filter: {}", env_filter);
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
        tracing::info!("initialized with env filter successfully");
    } else {
        tracing_subscriber::fmt().init();
        tracing::info!("logging initialized with default filter");
    }
    tracing::info!("logging initialized");
    tracing::info!("Starting corrosionWM");

    // if using nvidia, error out
    if let Ok(nvidia) = std::fs::read_to_string("/proc/driver/nvidia/version") {
        if nvidia.contains("NVIDIA") {
            tracing::warn!("CorrosionWM does not currently support EGL, so older proprietary nvidia drivers may not work with this compositor. It is advised to use the Nouveau drivers if you have an older nvidia card.");
        }
    }

    let corrosion_config = CorrosionConfig::new();
    let defaults = corrosion_config.get_defaults();

    let backend = match env::var("CORROSION_BACKEND") {
        Ok(ret) => ret,
        Err(_) => String::from("udev"),
    };

    let mut args = std::env::args().skip(1);
    let flag = args.next();
    let arg = args.next();

    match (flag.as_deref(), arg) {
        (Some("-h") | Some("--help"), _) => {
            println!("Usage: corrosionwm [OPTION]...");
            println!("A Wayland compositor written in Rust");
            println!("--command <command> or -c <command> to run a command on startup");
        }
        (Some("-c") | Some("--command"), Some(command)) => {
            std::process::Command::new(command).spawn().ok();
        }
        _ => {
            // use the find_term function to find a terminal
            if let Some(term) = find_term(defaults) {
                Command::new(term).spawn().ok();
            } else {
                tracing::error!("Terminal in the toml config was not found! Falling back to kitty");
                Command::new("kitty").spawn().ok();
            }
        }
    }

    match backend.as_ref() {
        "winit" => {
            winit_corrosion::init_winit::<WinitData>()
                .expect("Unable to initialize winit backend :(");
        }
        "udev" => {
            backend::initialize_backend();
        }
        _ => {
            tracing::error!("Backend setting not known, defaulting to udev");
            tracing::error!("Backend setting was: {}", backend);
            backend::initialize_backend();
        }
    };

    // TODO: event loop for udev backend

    Ok(())
}
