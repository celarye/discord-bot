use std::{any::Any, sync::Arc};

use tracing::{debug, error, info};
use twilight_gateway::{Event, MessageSender};
use twilight_http::{
    Client,
    request::{Request as TwilightRequest, channel::message::CreateMessage},
    routing::Route,
};
use twilight_model::application::interaction::InteractionData;

use crate::{
    discord::{DiscordBotClient, data::Data},
    plugins::{
        Runtime,
        runtime::discord_bot::plugin::discord_types::{
            Events as DiscordEvents, Requests as DiscordRequests,
        },
    },
};

impl DiscordBotClient {
    pub async fn handle_event(
        event: Event,
        shard_message_sender: Arc<MessageSender>,
        http_client: Arc<Client>,
        runtime: Arc<Runtime>,
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
                        .call_event(
                            plugin,
                            &DiscordEvents::MessageCreate(simd_json::to_vec(&message).unwrap()),
                        )
                        .await;

                    let event_responses = match pevent_responses {
                        Ok(event_response_responses) => event_response_responses,
                        Err(()) => {
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
            Event::InteractionCreate(interaction) => match interaction.data.as_ref() {
                Some(InteractionData::ApplicationCommand(command_data)) => {
                    let initialized_plugins = data.initialized_plugins.read().await;

                    let plugin = initialized_plugins
                        .discord_events
                        .interaction_create_commands
                        .get(&command_data.name);

                    if plugin.is_none() {
                        return;
                    }

                    let plugin = plugin.unwrap();

                    debug!("Plugin function call: \"{}\"", plugin);
                    let pevent_responses = runtime
                        .call_event(
                            plugin,
                            &DiscordEvents::InteractionCreate(
                                simd_json::to_vec(&interaction).unwrap(),
                            ),
                        )
                        .await;

                    let event_responses = match pevent_responses {
                        Ok(event_response_responses) => event_response_responses,
                        Err(()) => {
                            return;
                        }
                    };

                    if event_responses.is_empty() {
                        debug!("The plugin had no response");
                        return;
                    }

                    for event_response in event_responses {
                        match event_response {
                            DiscordRequests::InteractionCallback((
                                interaction_id,
                                interaction_token,
                                body,
                            )) => {
                                let request = match TwilightRequest::builder(
                                    &Route::InteractionCallback {
                                        interaction_id,
                                        interaction_token: &interaction_token,
                                    },
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
                _ => error!(
                    "This interaction create data type does not have support yet, interaction data type: {:#?}",
                    interaction.data.as_ref().unwrap().type_id()
                ),
            },
            _ => debug!(
                "Received an unhandled event: {}",
                &event.kind().name().unwrap_or("[No event kind name]")
            ),
        }
    }
}
