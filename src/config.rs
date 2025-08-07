use std::{collections::HashMap, fs, path::PathBuf};

use indexmap::IndexMap;
use serde::Deserialize;
use simd_json::OwnedValue;
use tracing::{error, warn};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub name: Option<String>,
    #[serde(default = "Config::default_cache")]
    pub cache: bool,
    #[serde(default = "Config::default_directory")]
    pub directory: PathBuf,
    #[serde(default = "Config::default_dotenv")]
    pub dotenv: PathBuf,
    pub plugins: IndexMap<String, ConfigPluginValues>,
}

#[derive(Debug, Deserialize)]
pub struct ConfigPluginValues {
    pub plugin: String,
    pub environment: Option<HashMap<String, String>>,
    pub settings: Option<OwnedValue>,
}

impl Config {
    pub fn new(file_path: &PathBuf) -> Result<Box<Self>, ()> {
        let file_bytes = match fs::read(file_path) {
            Ok(file_bytes) => file_bytes,
            Err(err) => {
                error!("Failed to read the config file, error: {}", &err);
                return Err(());
            }
        };

        match serde_yaml_ng::from_slice::<Config>(&file_bytes) {
            Ok(config) => {
                if let Err(err) = dotenvy::from_path(&config.dotenv) {
                    warn!(
                        "Failed to load a .env file from the path: {:?}, error: {}",
                        &config.dotenv, &err
                    );
                }
                Ok(Box::new(config))
            }
            Err(err) => {
                error!(
                    "Failed to deserialize the config file YAML to a struct, error: {}",
                    &err
                );
                Err(())
            }
        }
    }

    fn default_cache() -> bool {
        true
    }

    fn default_directory() -> PathBuf {
        PathBuf::from("./plugins/")
    }

    fn default_dotenv() -> PathBuf {
        PathBuf::from("./.env")
    }
}
