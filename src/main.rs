#![allow(irrefutable_let_patterns)]

// modules
mod handlers;

mod backend;
mod config;
mod cursor;
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
    // initialize logging
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

    // if using nvidia, warn the user that EGL is not supported
    if let Ok(nvidia) = std::fs::read_to_string("/proc/driver/nvidia/version") {
        if nvidia.contains("NVIDIA") {
            tracing::warn!("corrosionWM does not currently support EGL, so older proprietary nvidia drivers may not work with this compositor. It is advised to use the Nouveau drivers if you have an older nvidia card.");
        }
    }

    let corrosion_config = CorrosionConfig::new(); // get the config
    let defaults = corrosion_config.get_defaults(); // get the defaults from the config

    // the backend to use, can be either udev or winit
    let backend = match env::var("CORROSIONWM_BACKEND") {
        Ok(ret) => ret,
        Err(_) => String::from("udev"),
    };

    let mut args = std::env::args().skip(1); // skip the first argument, which is the binary name
    let flag = args.next(); // get the first argument
    let arg = args.next(); // get the second argument

    // handle the arguments
    // TODO: we should also make it process the arguments first so it doesnt log a bunch of stuff
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
    
    // initialize the backend
    match backend.as_ref() {
        "winit" => {
            // initialize the winit backend
            winit_corrosion::init_winit::<WinitData>()
                .expect("Unable to initialize winit backend :(");
        }
        "udev" => {
            // initialize the udev backend
            backend::initialize_backend();
        }
        _ => {
            // default to udev
            tracing::error!("Backend setting not known, defaulting to udev");
            tracing::error!("Backend setting was: {}", backend);
            backend::initialize_backend();
        }
    };

    // TODO: event loop for udev backend

    Ok(())
}
