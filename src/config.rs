use std::{fs, path::Path};

use indexmap::IndexMap;
use serde::Deserialize;
use tracing::{error, info};

use crate::plugins::ConfigPlugin;

#[derive(Deserialize)]
pub struct Config {
    #[allow(unused)] // Will be used when multi discord bot client support gets added
    pub id: String,
    pub plugins: IndexMap<String, ConfigPlugin>,
}

impl Config {
    pub fn new(file_path: &Path) -> Result<Box<Self>, ()> {
        info!("Loading and parsing the config file");

        let file_bytes = match fs::read(file_path) {
            Ok(file_bytes) => file_bytes,
            Err(err) => {
                error!("An error occurred while trying to read the config file: {err}");
                return Err(());
            }
        };

        match serde_yaml_ng::from_slice::<Config>(&file_bytes) {
            Ok(config) => Ok(Box::new(config)), // TODO: Env var interpolation, maybe via YAML 1.2's' `!val`
            Err(err) => {
                error!(
                    "An error occurred while trying to deserialize the config file YAML to a struct: {err}"
                );
                Err(())
            }
        }
    }
}
