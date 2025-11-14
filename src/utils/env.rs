use std::{env, path::Path};

use tracing::{debug, error, info};

use dotenvy;

pub fn load_env_file(env_file: &Path) -> Result<(), ()> {
    info!("Loading the env file");

    if let Err(err) = dotenvy::from_path(env_file) {
        if err.not_found() {
            debug!("No env file found for the following path: {env_file:?}");
            return Ok(());
        }

        error!("An error occurred wile trying to load the env file: {err}");

        return Err(());
    }

    Ok(())
}

pub fn validate() -> Result<String, ()> {
    info!("Validating the environment variables (DISCORD_BOT_CLIENT_TOKEN)");

    if let Ok(value) = env::var("DISCORD_BOT_CLIENT_TOKEN") {
        debug!("DISCORD_BOT_CLIENT_TOKEN environment variable was found: {value:.3}... (redacted)");

        Ok(value)
    } else {
        error!(
            "The DISCORD_BOT_CLIENT_TOKEN environment variable was not set, contains an illegal character ('=' or '0') or was not valid unicode"
        );
        Err(())
    }
}
