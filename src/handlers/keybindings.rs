use smithay::{
    input::keyboard::ModifiersState,
};
use std::process::Command;

use crate::state::Corrosion;

// code to convert emacs style keybindings to xkb keysyms
pub fn get_mod_key_and_compare(state: &ModifiersState) -> bool {
    let mod_key = match std::env::var("MOD_KEY") {
        Ok(value) => value,
        Err(_) => String::from("alt"),
    };

    if &mod_key == "ctrl" && state.ctrl {
        return true;
    }
    if &mod_key == "alt" && state.alt {
        return true;
    }
    if &mod_key == "shift" && state.shift {
        return true;
    }
    if &mod_key == "super" && state.logo {
        return true;
    }

    false
}

pub enum KeyAction {
    Quit,
    _CloseWindow,
    Spawn(String),
    _Launcher(String),
}

impl Corrosion {
    pub fn parse_keybindings(&self, action: KeyAction) {
        match action {
            KeyAction::Spawn(program) => {
                let mut args: Vec<&str> = program.split(' ').collect();
                let program: &str;
                let mut execution;
                if let Some(command) = args.get(0) {
                    program = command;
                } else {
                    tracing::error!("Program argument in spawn is null");
                    return;
                }
                execution = Command::new(program);
                args.remove(0);
                println!("args: {:?}", args);
                execution.args(args);
                execution.spawn().ok();
                tracing::info!("Spawned program: {}", program);
            }
            KeyAction::Quit => {
                // TODO: cleanup
                tracing::info!("Quitting");
                self.loop_signal.stop();
            }
            KeyAction::_CloseWindow => {
                // todo: put something here
                tracing::warn!("CloseWindow not implemented yet");
            }
            KeyAction::_Launcher(program) => {
                let mut args: Vec<&str> = program.split(' ').collect();
                let program: &str;
                let mut execution;
                if let Some(command) = args.get(0) {
                    program = command;
                } else {
                    tracing::error!("Program argument in spawn is null");
                    return;
                }
                execution = Command::new(program);
                args.remove(0);
                println!("args: {:?}", args);
                execution.args(args);
                execution.spawn().ok();
                tracing::info!("Spawned program: {}", program);
            }
            _ => {
                tracing::error!("Function not implemented yet");
            }
        };
    }
}
