use std::env;

use tracing::{debug, error};

#[cfg(feature = "dotenv")]
use dotenvy::dotenv;

#[cfg(feature = "dotenv")]
pub fn load_dotenv() -> Result<(), ()> {
    if let Err(err) = dotenv() {
        error!(
            "An error occurred wile trying to load the .env file: {}",
            &err
        );
        return Err(());
    }

    Ok(())
}

pub fn validate() -> Result<(), ()> {
    if let Ok(value) = env::var("DISCORD_BOT_TOKEN") {
        debug!(
            "DISCORD_BOT_TOKEN environment variable was found: {:.3}... (redacted)",
            &value
        );
    } else {
        error!(
            "The DISCORD_BOT_TOKEN environment variable was not set, contains an illegal character ('=' or '0') or was not valid UNICODE"
        );
        return Err(());
    }

    Ok(())
}
