use tracing::error;
use twilight_http::{
    request::{Request, channel::message::CreateMessage},
    routing::Route,
};
use twilight_model::http::interaction::InteractionResponse;

use crate::{
    discord::DiscordBotClient,
    plugins::discord_bot::plugin::host_functions::{DiscordRequests, DiscordResponses},
};

impl DiscordBotClient {
    pub async fn request(&self, request: DiscordRequests) -> Result<DiscordResponses, String> {
        match request {
            DiscordRequests::CreateMessage((channel_id, body)) => {
                let request = match Request::builder(&Route::CreateMessage { channel_id })
                    .body(body)
                    .build()
                {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<CreateMessage>(request).await
            }
            DiscordRequests::InteractionCallback((interaction_id, interaction_token, body)) => {
                let request = match Request::builder(&Route::InteractionCallback {
                    interaction_id,
                    interaction_token: &interaction_token,
                })
                .body(body)
                .build()
                {
                    Ok(request) => request,
                    Err(err) => {
                        let err = format!(
                            "Something went wrong while building a Discord request, error: {err}"
                        );
                        error!(err);
                        return Err(err);
                    }
                };

                self.request_execute::<InteractionResponse>(request).await
            }
            _ => unimplemented!(),
        }
    }

    async fn request_execute<T: Unpin>(
        &self,
        request: Request,
    ) -> Result<DiscordResponses, String> {
        match self.http_client.request::<T>(request).await {
            Ok(response) => Ok(response.bytes().await.unwrap().to_vec()),
            Err(err) => {
                let err =
                    format!("Something went wrong while making a Discord request, error: {err}");
                error!(err);
                Err(err)
            }
        }
    }
}
