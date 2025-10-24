use std::path::PathBuf;

use reqwest::{
    StatusCode,
    header::{HeaderMap, HeaderValue},
};
use tracing::{debug, error};

use crate::http::HttpClient;

impl HttpClient {
    pub async fn get_file_from_registry(
        &self,
        registry: &String,
        path: &PathBuf,
    ) -> Result<Vec<u8>, ()> {
        let mut headers = HeaderMap::with_capacity(2);
        headers.insert(
            "Accept",
            HeaderValue::from_str("application/vnd.github.raw+json").unwrap(),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_str("2022-11-28").unwrap(),
        );

        let url_path = PathBuf::from("https://api.github.com/repos")
            .join(registry)
            .join("contents")
            .join(path);

        let url_str = match url_path.to_str() {
            None => {
                error!(
                    "Failed to construct a valid URL from the provided registry and path, lossy string of the URL: {}",
                    &url_path.to_string_lossy()
                );
                return Err(());
            }
            Some(url) => url,
        };

        debug!("Requested registry file: {}", &url_str);

        match self.client.get(url_str).headers(headers).send().await {
            Ok(raw_response) => {
                if raw_response.status() != StatusCode::OK {
                    error!(
                        "The response was undesired, status code: {}",
                        &raw_response.status(),
                    );
                    return Err(());
                }

                match raw_response.bytes().await {
                    Ok(response) => Ok(response.to_vec()),
                    Err(err) => {
                        error!(
                            "Something went wrong while getting the raw bytes from the response, error: {}",
                            &err
                        );
                        Err(())
                    }
                }
            }
            Err(err) => {
                error!(
                    "Something went wrong while making the request, error: {}",
                    &err
                );
                Err(())
            }
        }
    }
}
