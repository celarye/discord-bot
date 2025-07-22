use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info};
use twilight_gateway::Event;
use twilight_http::{
    Client,
    request::{Request as TwilightRequest, channel::message::CreateMessage},
    routing::Route,
};

use crate::plugins::{
    Runtime,
    runtime::{
        discord_bot::plugin::discord_types::Requests as DiscordRequests,
        exports::discord_bot::plugin::plugin_functions::DiscordEvents,
    },
};

use super::Data;

pub async fn run(
    event: Event,
    runtime: Arc<Mutex<Runtime>>,
    http_client: Arc<Client>,
    data: Arc<RwLock<Box<Data>>>,
) {
    match event {
        Event::Ready(ready) => {
            info!("Shard is ready, logged in as {}", &ready.user.name);
        }
        Event::MessageCreate(message) => {
            for plugin in &data
                .read()
                .await
                .initialized_plugins
                .read()
                .await
                .events
                .message_create
            {
                debug!("Plugin function call: \"{}\"", plugin);
                let event_response = runtime
                    .lock()
                    .await
                    .call_event(
                        plugin,
                        &DiscordEvents::MessageCreate(simd_json::to_vec(&message).unwrap()),
                        data.clone(),
                    )
                    .await;

                runtime
                    .lock()
                    .await
                    .plugins
                    .get_mut(plugin)
                    .unwrap()
                    .internal_context = event_response.context.plugin;

                let event_response_responses = match event_response.response {
                    Ok(event_response_responses) => event_response_responses,
                    Err(err) => {
                        error!("The plugin returned an error: {}", &err);
                        continue;
                    }
                };

                if event_response_responses.is_empty() {
                    debug!("The plugin had no response");
                    continue;
                }

                for event_response_response in event_response_responses {
                    match event_response_response {
                        DiscordRequests::CreateMessage((channel_id, body)) => {
                            let request = match TwilightRequest::builder(&Route::CreateMessage {
                                channel_id,
                            })
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

                            if let Err(err) = http_client.request::<CreateMessage>(request).await {
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
