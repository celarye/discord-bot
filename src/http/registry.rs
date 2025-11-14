use std::sync::LazyLock;

use reqwest::{
    StatusCode,
    header::{HeaderMap, HeaderValue},
};
use tracing::{debug, error};
use url::{ParseError, Url};

use crate::http::HttpClient;

static REGISTRY_BASE_URL: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://api.github.com/repos").unwrap());

impl HttpClient {
    pub async fn get_file_from_registry(&self, registry: &str, path: &str) -> Result<Vec<u8>, ()> {
        let mut headers = HeaderMap::with_capacity(2);
        headers.insert(
            "Accept",
            HeaderValue::from_str("application/vnd.github.raw+json").unwrap(),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_str("2022-11-28").unwrap(),
        );

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

        match self.client.get(url).headers(headers).send().await {
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

    fn parse_url(registry: &str, path: &str) -> Result<Url, ParseError> {
        REGISTRY_BASE_URL
            .clone()
            .join(registry)?
            .join("contents")
            .unwrap()
            .join(path)
    }
}
