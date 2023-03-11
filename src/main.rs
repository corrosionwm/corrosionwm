#![allow(irrefutable_let_patterns)]

// modules
mod handlers;

mod grabs;
mod input;
mod state;
mod winit;

// imports
use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
pub use state::Neko;

pub struct CalloopData {
    state: Neko,
    display: Display<Neko>,
}

// HACK: this is temporary just to find a term
fn find_term() -> &'static str {
    let terms = ["kitty", "wezterm", "urxvt", "weston-terminal"];
    for term in terms.iter() {
        if which::which(term).is_ok() {
            tracing::info!("Found terminal!: {}", term);
            return term;
        }
    }
    tracing::error!("No terminal found, please install one of the following: kitty, wezterm, urxvt, weston-terminal, or use the -c flag to specify a terminal");
    ""
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_env("NekoWM_LOG") {
        // change this by changing the RUST_LOG environment variable
        tracing::info!("logging initialized with env filter: {}", env_filter);
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
        tracing::info!("logging initialized with default filter");
    }
    tracing::info!("logging initialized");
    tracing::info!("Starting NekoWM");

    let mut event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;

    let mut display: Display<Neko> = Display::new()?;
    let state = Neko::new(&mut event_loop, &mut display);

    let mut data = CalloopData { state, display };

    crate::winit::init_winit(&mut event_loop, &mut data)?;

    let mut args = std::env::args().skip(1);
    let flag = args.next();
    let arg = args.next();

    match (flag.as_deref(), arg) {
        (Some("-h") | Some("--help"), _) => {
            println!("Usage: NekoWM [OPTION]...");
            println!("A Wayland compositor written in Rust");
            println!("--command <command> or -c <command> to run a command on startup");
        }
        (Some("-c") | Some("--command"), Some(command)) => {
            std::process::Command::new(command).spawn().ok();
        }
        _ => {
            // TODO: Make this configurable
            // TODO: remove this completely as this shit is just for debugging
            // use the find_term function to find a terminal
            let term = find_term();
            if term != "" {
                std::process::Command::new(term).spawn().ok();
            }
        }
    }

    tracing::info!("Starting NekoWM event loop");
    event_loop.run(None, &mut data, move |_| {
        // NekoWM is running
    })?;

    Ok(())
}
