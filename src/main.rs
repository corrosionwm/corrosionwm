#![allow(irrefutable_let_patterns)]

// modules
mod handlers;

mod grabs;
mod input;
mod state;
mod udev;
mod winit;

// imports
pub use crate::state::Corrosion;
use crate::winit::{self as winit_corrosion, WinitData};
use smithay::reexports::wayland_server::Display;
use state::Backend;
use std::env;
use tracing::debug;

pub struct CalloopData<BackendData: Backend + 'static> {
    state: Corrosion<BackendData>,
    display: Display<Corrosion<BackendData>>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing::info!("env filter: {}", env_filter);
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
        tracing::info!("initialized with env filter successfully");
    } else {
        tracing_subscriber::fmt().init();
        tracing::info!("no env filter found, using default");
    }

    let backend = match env::var("CORROSION_BACKEND") {
        Ok(ret) => ret,
        Err(_) => String::from("udev"),
    };
    debug!("Udev backend initialized successfully!");

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
            // TODO: Make this configurable
            // TODO: remove this completely as this shit is just for debugging
            std::process::Command::new("kitty").spawn().expect("You may not have kitty installed, if not, please install it, or use the --command/-c flag to specify a different program to run.");
        }
    }

    match backend.as_ref() {
        "winit" => {
            winit_corrosion::init_winit::<WinitData>()
                .expect("Unable to initialize winit backend :(");
        }
        "udev" => {
            tracing::error!("Udev backend is not yet supported by corrosionwm");
        }
        _ => {
            tracing::error!("Backend setting not known");
        }
    };

    Ok(())
}
