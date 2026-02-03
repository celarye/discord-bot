/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Copyright © 2026 Eduard Smet */

use std::{
    str::FromStr,
    sync::{Arc, LazyLock},
};

use reqwest::StatusCode;
use tracing::{debug, error};
use url::{ParseError, Url};

use crate::http::HttpClient;

static DEFAULT_REGISTRY_URL: LazyLock<Url> = LazyLock::new(|| {
    Url::parse("https://raw.githubusercontent.com/celarye/discord-bot-plugins/refs/heads/master/")
        .unwrap()
});

impl HttpClient {
    pub async fn get_file_from_registry(
        &self,
        registry: &Arc<Option<String>>,
        path: &str,
    ) -> Result<Vec<u8>, ()> {
        let url = match Self::parse_url(registry, path) {
            Ok(url) => url,
            Err(err) => {
                error!(
                    "An error occurred while trying to construct a valid URL from the provided registry and path: {err}"
                );
                return Err(());
            }
        };

        debug!("Requested registry file: {url}");

        match self.client.get(url).send().await {
            Ok(raw_response) => {
                if raw_response.status() != StatusCode::OK {
                    error!(
                        "The response was undesired, status code: {}",
                        raw_response.status(),
                    );
                    return Err(());
                }

                match raw_response.bytes().await {
                    Ok(response) => Ok(response.to_vec()),
                    Err(err) => {
                        error!(
                            "Something went wrong while getting the raw bytes from the response, error: {err}"
                        );
                        Err(())
                    }
                }
            }
            Err(err) => {
                error!("Something went wrong while making the request, error: {err}");
                Err(())
            }
        }
    }

    fn parse_url(registry: &Arc<Option<String>>, path: &str) -> Result<Url, ParseError> {
        if let Some(registry) = registry.as_deref() {
            return Url::from_str(registry)?.join(path);
        }

        DEFAULT_REGISTRY_URL.join(path)
    }
}
