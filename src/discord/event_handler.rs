use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{debug, error, info};
use twilight_gateway::{Event, MessageSender};
use twilight_http::{
    Client,
    request::{Request as TwilightRequest, channel::message::CreateMessage},
    routing::Route,
};

use crate::{
    discord::{DiscordBotClient, data::Data},
    plugins::{
        Runtime,
        runtime::{
            discord_bot::plugin::discord_types::Requests as DiscordRequests,
            exports::discord_bot::plugin::plugin_resources::DiscordEvents,
        },
    },
};

impl DiscordBotClient {
    pub async fn handle_event(
        event: Event,
        shard_message_sender: Arc<MessageSender>,
        http_client: Arc<Client>,
        runtime: Arc<Mutex<Runtime>>,
        data: Arc<Data>,
    ) {
        match event {
            Event::Ready(ready) => {
                info!("Shard is ready, logged in as {}", &ready.user.name);
            }
            Event::MessageCreate(message) => {
                for plugin in data
                    .initialized_plugins
                    .read()
                    .await
                    .discord_events
                    .message_create
                    .iter()
                {
                    debug!("Plugin function call: \"{}\"", plugin);
                    let pevent_responses = runtime
                        .lock()
                        .await
                        .call_event(
                            plugin,
                            &DiscordEvents::MessageCreate(simd_json::to_vec(&message).unwrap()),
                        )
                        .await;

                    let event_responses = match pevent_responses {
                        Ok(event_response_responses) => event_response_responses,
                        Err(err) => {
                            error!("The plugin returned an error: {}", &err);
                            continue;
                        }
                    };

                    if event_responses.is_empty() {
                        debug!("The plugin had no response");
                        continue;
                    }

                    for event_response in event_responses {
                        match event_response {
                            DiscordRequests::CreateMessage((channel_id, body)) => {
                                let request = match TwilightRequest::builder(
                                    &Route::CreateMessage { channel_id },
                                )
                                .body(body)
                                .build()
                                {
                                    Ok(request) => request,
                                    Err(err) => {
                                        error!(
                                            "Something went wrong while building the request, error: {}",
                                            &err
                                        );
                                        continue;
                                    }
                                };

                                if let Err(err) =
                                    http_client.request::<CreateMessage>(request).await
                                {
                                    error!(
                                        "Something went wrong while sending the request to Discord, error: {}",
                                        &err
                                    );
                                }
                            }
                            _ => unimplemented!(
                                "Not all Discord Gateway and HTTP request have been implemented yet"
                            ),
                        }
                    }
                }
            }
            Event::InteractionCreate(interaction) => {
                info!("InteractionCreate event: {:#?}", interaction);
            }
            _ => debug!(
                "Received an unhandled event: {}",
                &event.kind().name().unwrap_or("[No event kind name]")
            ),
        }
    }
}
