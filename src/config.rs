use serde_derive::Deserialize;

use std::env;
use std::fs::{self, create_dir_all, read_to_string};
use std::path::Path;
use std::process::Command;
use smithay::input::keyboard::keysyms;

//nya
//the above comment is a secret compiler option that directly tells ferris to make the code blazingly fast

// tests 
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keybind() {
        let keybind: Keybind = "M-x | kitty".into();
        assert_eq!(keybind.keys, vec!["x"]);
        assert_eq!(keybind.command, "kitty");
        assert_eq!(keybind.special_key, Some(SpecialKey::ModKey));
    }

    #[test]
    fn test_multi_keybind() {
        let keybind: Keybind = "M-x-y | kitty".into();
        assert_eq!(keybind.keys, vec!["x", "y"]);
        assert_eq!(keybind.command, "kitty");
        assert_eq!(keybind.special_key, Some(SpecialKey::ModKey));
    }

    #[test]
    fn test_special_key_detection() {
        let keybind: Keybind = "M-x | kitty".into();
        assert_eq!(keybind.special_key, Some(SpecialKey::ModKey));
        let keybind: Keybind = "S-x | kitty".into();
        assert_eq!(keybind.special_key, Some(SpecialKey::ShiftKey));
        let keybind: Keybind = "C-x | kitty".into();
        assert_eq!(keybind.special_key, Some(SpecialKey::ControlKey));
        let keybind: Keybind = "A-x | kitty".into();
        assert_eq!(keybind.special_key, Some(SpecialKey::AltKey));
    }

    #[test]
    fn test_keycodes() {
        let keycodes = keys_to_keycodes(vec!["x".to_string()]);
        assert_eq!(keycodes, vec![keysyms::KEY_X]);

        let keycodes = keys_to_keycodes(vec!["x".to_string(), "y".to_string()]);
        assert_eq!(keycodes, vec![keysyms::KEY_X, keysyms::KEY_Y]);

        let keybind: Keybind = "M-x | kitty".into();
        let keycodes = keys_to_keycodes(keybind.keys);
        assert_eq!(keycodes, vec![keysyms::KEY_X]);
    }
}

// keybind struct
#[derive(Debug, PartialEq)]
pub struct Keybind {
    pub special_key: Option<SpecialKey>,
    pub keys: Vec<String>,
    pub command: String,
}

// TODO: allow multiple special keys
#[derive(Debug, PartialEq)]
pub enum SpecialKey {
    ModKey,
    ShiftKey,
    ControlKey,
    AltKey,
}

// implementation for keybind struct
// this will allow converting emacs keybindings into that struct
// an example is "M-x | kitty"
// M = Mod4
// - is the seperator between keys
// x = x
// | will be the seperator between the key and the command
// kitty will be the command
impl From<&str> for Keybind {
    fn from(keybind: &str) -> Self {
        let mut keybind = keybind.split(" | ");
        let keys = keybind.next().unwrap();
        let command = keybind.next().unwrap();

        let mut keys = keys.split("-");
        let special_key = keys.next().unwrap();
        let keys: Vec<String> = keys.map(|x| x.to_string()).collect();

        let special_key = match special_key {
            "M" => Some(SpecialKey::ModKey),
            "S" => Some(SpecialKey::ShiftKey),
            "C" => Some(SpecialKey::ControlKey),
            "A" => Some(SpecialKey::AltKey),
            _ => None,
        };

        Keybind {
            special_key,
            keys,
            command: command.to_string(),
        }
    }
}

//The default configuration
const DEFAULT_CONFIG: &str = r#"# This is the default corrosionwm config
[defaults]
terminal = "kitty"
launcher = "wofi --show drun"
"#;

/// top level data struct
#[derive(Deserialize)]
pub struct CorrosionConfig {
    defaults: Defaults, //[defaults]
}

/// We do not currently support non-alphanumeric keys, but we will in the future
pub fn keys_to_keycodes(keys: Vec<String>) -> Vec<u32> {
    // sadly, we have to use a match statement here
    // because smithay doesn't support a way to convert a string to a keycode yet

    let mut keycodes: Vec<u32> = Vec::new();

    for key in keys {
        // this code makes me want to cry
        // we may also be able to use KEY_(name)
        // but that would require a lot of work
        match key.as_str() {
            "a" => keycodes.push(keysyms::KEY_A),
            "b" => keycodes.push(keysyms::KEY_B),
            "c" => keycodes.push(keysyms::KEY_C),
            "d" => keycodes.push(keysyms::KEY_D),
            "e" => keycodes.push(keysyms::KEY_E),
            "f" => keycodes.push(keysyms::KEY_F),
            "g" => keycodes.push(keysyms::KEY_G),
            "h" => keycodes.push(keysyms::KEY_H),
            "i" => keycodes.push(keysyms::KEY_I),
            "j" => keycodes.push(keysyms::KEY_J),
            "k" => keycodes.push(keysyms::KEY_K),
            "l" => keycodes.push(keysyms::KEY_L),
            "m" => keycodes.push(keysyms::KEY_M),
            "n" => keycodes.push(keysyms::KEY_N),
            "o" => keycodes.push(keysyms::KEY_O),
            "p" => keycodes.push(keysyms::KEY_P),
            "q" => keycodes.push(keysyms::KEY_Q),
            "r" => keycodes.push(keysyms::KEY_R),
            "s" => keycodes.push(keysyms::KEY_S),
            "t" => keycodes.push(keysyms::KEY_T),
            "u" => keycodes.push(keysyms::KEY_U),
            "v" => keycodes.push(keysyms::KEY_V),
            "w" => keycodes.push(keysyms::KEY_W),
            "x" => keycodes.push(keysyms::KEY_X),
            "y" => keycodes.push(keysyms::KEY_Y),
            "z" => keycodes.push(keysyms::KEY_Z),
            _ => (),
        }

        // :sob:
    }

    keycodes
}
//TODO: add more config options here e.g [misc], [config], [keybinds]
//[defaults]
#[derive(Deserialize)]
pub struct Defaults {
    pub terminal: String,
    pub launcher: String,
}

impl CorrosionConfig {
    //initialize corrosion config
    pub fn new() -> Self {
        // use $XDG_CONFIG_HOME, or fallback to $HOME/.config
        let config_directory = match env::var("XDG_CONFIG_HOME") {
            Ok(val) => format!("{}/corrosionwm", val),
            Err(_) => format!("{}/.config/corrosionwm", env::var("HOME").unwrap()),
        };

        let config_file = format!("{}/config.toml", config_directory);

        //check for ~/.config/corrosionwm
        if !Path::new(&config_directory).exists() {
            tracing::info!(
                "Config folder not found, Creating at '{}'.",
                config_directory
            );
            create_dir_all(config_directory).unwrap();
        }

        //check for ~/.config/corrosionwm/config.toml
        if !Path::new(&config_file).exists() {
            tracing::info!("Config file not found, Creating at '{}'.", config_file);
            fs::write(&config_file, DEFAULT_CONFIG).unwrap();
        }

        return match toml::from_str(&read_to_string(&config_file).unwrap()) {
            Ok(c) => {
                tracing::info!("Loaded config from {}", config_file);
                c
            }
            Err(_) => {
                tracing::error!(
                    "Config file is not valid, falling back to default hardcoded config."
                );
                tracing::info!("Loaded hardcoded config file");
                toml::from_str(DEFAULT_CONFIG).unwrap()
            }
        };
    }

    //fetches the [defaults] section and returns it
    pub fn get_defaults(&self) -> &Defaults {
        &self.defaults
    }
}

impl Default for CorrosionConfig {
    fn default() -> Self {
        // return the default config
        toml::from_str(DEFAULT_CONFIG).unwrap()
    }
}
