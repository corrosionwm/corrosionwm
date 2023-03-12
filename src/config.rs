use serde_derive::Deserialize;
use toml;
use tracing;

use std::fs::{self, read_to_string, create_dir_all};
use std::env;
use std::path::Path;
use std::process::Command;

//nya
//the above comment is a secret compiler option that directly tells ferris to make the code blazingly fast

//The default configuration
const DEFAULT_CONFIG: &str = r#"# This is the default corrosionwm config
[defaults]
terminal = "auto"
"#;

//top level data struct
#[derive(Deserialize)]
pub struct CorrosionConfig {
    defaults: Defaults, //[defaults]
}

//TODO: add more config options here e.g [misc], [config], [keybinds]
//[defaults]
#[derive(Deserialize)]
pub struct Defaults {
    pub terminal: String,
}

fn get_username() -> String {
    let user;
    match env::var("USER") {
        Ok(val) => {
            user = val;
        }
        Err(_) => {
            let whoami_output = Command::new("whoami")
                .output()
                .unwrap();
            user = String::from_utf8_lossy(&whoami_output.stdout).to_string();
        }
    }
    user
}

impl CorrosionConfig {
    //initialize corrosion config
    pub fn new() -> Self {
        //check if ~/.config/corrosionwm/config.toml exists
        //if not, create it with the default configuration
        let username = get_username();
        let config_directory = format!("/home/{}/.config/corrosionwm", username);
        let config_file = format!("{}/config.toml", config_directory);

        //check for ~/.config/corrosionwm
        if !Path::new(&config_directory).exists() {
            tracing::info!("Config folder not found, Creating at '{}'.", config_directory);
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
                tracing::error!("Config file is not valid, falling back to default hardcoded config.");
                tracing::info!("Loaded hardcoded config file");
                toml::from_str(DEFAULT_CONFIG).unwrap()
            }
        }
    }

    //fetches the [defaults] section and returns it
    pub fn get_defaults(&self) -> &Defaults {
        &self.defaults
    }
}
