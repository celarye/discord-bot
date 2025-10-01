use std::{any::Any, sync::Arc};

use tokio::sync::RwLock;
use tracing::{debug, error, info};
use twilight_gateway::Event;
use twilight_model::application::interaction::InteractionData;

use crate::{
    discord::DiscordBotClientReceiver,
    plugins::{
        PluginRegistrations,
        runtime::{Runtime, discord_bot::plugin::discord_types::Events as DiscordEvents},
    },
};

impl DiscordBotClientReceiver {
    pub async fn handle_event(
        event: Event,
        runtime: Arc<Runtime>,
        plugin_registrations: Arc<RwLock<PluginRegistrations>>,
    ) {
        match event {
            Event::Ready(ready) => {
                info!("Shard is ready, logged in as {}", &ready.user.name);
                runtime
                    .register_shard_message_senders_to_guilds(ready.shard.unwrap(), ready.guilds)
                    .await;
            }
            Event::MessageCreate(message) => {
                for plugin in plugin_registrations
                    .read()
                    .await
                    .discord_events
                    .message_create
                    .iter()
                {
                    debug!("Plugin function call: \"{}\"", plugin);
                    let _ = runtime
                        .call_event(
                            plugin,
                            &DiscordEvents::MessageCreate(simd_json::to_vec(&message).unwrap()),
                        )
                        .await;

                    // TODO: runtime is supposed to call the DiscordBotClientSender
                    //
                    //let event_responses = match pevent_responses {
                    //    Ok(event_response_responses) => event_response_responses,
                    //    Err(()) => {
                    //        continue;
                    //    }
                    //};
                    //
                    //if event_responses.is_empty() {
                    //    debug!("The plugin had no response");
                    //    continue;
                    //}

                    //for event_response in event_responses {
                    //    match event_response {
                    //        DiscordRequests::CreateMessage((channel_id, body)) => {
                    //            let request = match TwilightRequest::builder(
                    //                &Route::CreateMessage { channel_id },
                    //            )
                    //            .body(body)
                    //            .build()
                    //            {
                    //                Ok(request) => request,
                    //                Err(err) => {
                    //                    error!(
                    //                        "Something went wrong while building the request, error: {}",
                    //                        &err
                    //                    );
                    //                    continue;
                    //                }
                    //            };

                    //            if let Err(err) =
                    //                http_client.request::<CreateMessage>(request).await
                    //            {
                    //                error!(
                    //                    "Something went wrong while sending the request to Discord, error: {}",
                    //                    &err
                    //                );
                    //            }
                    //        }
                    //        _ => unimplemented!(
                    //            "Not all Discord Gateway and HTTP request have been implemented yet"
                    //        ),
                    //    }
                    //}
                }
            }
            Event::InteractionCreate(interaction) => match interaction.data.as_ref() {
                Some(InteractionData::ApplicationCommand(command_data)) => {
                    let initialized_plugins = plugin_registrations.read().await;

                    let plugin = initialized_plugins
                        .discord_events
                        .interaction_create_commands
                        .get(&command_data.name);

                    if plugin.is_none() {
                        return;
                    }

                    let plugin = plugin.unwrap();

                    debug!("Plugin function call: \"{}\"", plugin.0);
                    let _ = runtime
                        .call_event(
                            &plugin.0,
                            &DiscordEvents::InteractionCreate(
                                simd_json::to_vec(&interaction).unwrap(),
                            ),
                        )
                        .await;

                    // TODO: runtime is supposed to call the DiscordBotClientSender
                    //
                    //let event_responses = match pevent_responses {
                    //    Ok(event_response_responses) => event_response_responses,
                    //    Err(()) => {
                    //        return;
                    //    }
                    //};
                    //
                    //if event_responses.is_empty() {
                    //    debug!("The plugin had no response");
                    //    return;
                    //}

                    //for event_response in event_responses {
                    //    match event_response {
                    //        DiscordRequests::InteractionCallback((
                    //            interaction_id,
                    //            interaction_token,
                    //            body,
                    //        )) => {
                    //            let request = match TwilightRequest::builder(
                    //                &Route::InteractionCallback {
                    //                    interaction_id,
                    //                    interaction_token: &interaction_token,
                    //                },
                    //            )
                    //            .body(body)
                    //            .build()
                    //            {
                    //                Ok(request) => request,
                    //                Err(err) => {
                    //                    error!(
                    //                        "Something went wrong while building the request, error: {}",
                    //                        &err
                    //                    );
                    //                    continue;
                    //                }
                    //            };

                    //            // TODO: runtime is supposed to call the DiscordBotClientSender
                    //            //
                    //            //if let Err(err) =
                    //            //    http_client.request::<CreateMessage>(request).await
                    //            //{
                    //            //    error!(
                    //            //        "Something went wrong while sending the request to Discord, error: {}",
                    //            //        &err
                    //            //    );
                    //            //}
                    //        }
                    //        DiscordRequests::CreateMessage((channel_id, body)) => {
                    //            let request = match TwilightRequest::builder(
                    //                &Route::CreateMessage { channel_id },
                    //            )
                    //            .body(body)
                    //            .build()
                    //            {
                    //                Ok(request) => request,
                    //                Err(err) => {
                    //                    error!(
                    //                        "Something went wrong while building the request, error: {}",
                    //                        &err
                    //                    );
                    //                    continue;
                    //                }
                    //            };

                    //            // TODO: runtime is supposed to call the DiscordBotClientSender
                    //            //
                    //            //if let Err(err) =
                    //            //    http_client.request::<CreateMessage>(request).await
                    //            //{
                    //            //    error!(
                    //            //        "Something went wrong while sending the request to Discord, error: {}",
                    //            //        &err
                    //            //    );
                    //            //}
                    //        }
                    //        _ => unimplemented!(
                    //            "Not all Discord Gateway and HTTP request have been implemented yet"
                    //        ),
                    //    }
                    //}
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
