pub mod registry;

use std::time::Duration;

use reqwest::Client;
use tracing::error;

pub struct HttpClient {
    client: Client,
}

impl HttpClient {
    pub fn new() -> Result<Self, ()> {
        match Client::builder()
            .user_agent("celarye/discord-bot")
            .timeout(Duration::new(15, 0))
            .build()
        {
            Ok(client) => Ok(HttpClient { client }),
            Err(err) => {
                error!(
                    "Something went wrong while creating the request client: {}",
                    &err
                );
                Err(())
            }
        }
    }
}
