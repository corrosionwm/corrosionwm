#![allow(irrefutable_let_patterns)]

// modules
mod handlers;

mod config;
mod grabs;
mod input;
mod state;
mod winit;

// imports
pub use crate::config::{CorrosionConfig, Defaults};
use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
pub use state::Corrosion;
use std::process::Command;

pub struct CalloopData {
    state: Corrosion,
    display: Display<Corrosion>,
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
    } else {
        tracing_subscriber::fmt().init();
        tracing::info!("logging initialized with default filter");
    }
    tracing::info!("logging initialized");
    tracing::info!("Starting corrosionWM");

    let mut event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;

    let mut display: Display<Corrosion> = Display::new()?;
    let state = Corrosion::new(&mut event_loop, &mut display);

    let mut data = CalloopData { state, display };

    crate::winit::init_winit(&mut event_loop, &mut data)?;

    let corrosion_config = CorrosionConfig::new();
    let defaults = corrosion_config.get_defaults();

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

    tracing::info!("Starting corrosionWM event loop");
    event_loop.run(None, &mut data, move |_| {
        // corrosionWM is running
    })?;

    Ok(())
}
