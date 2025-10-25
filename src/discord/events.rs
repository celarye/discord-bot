use std::{any::Any, sync::Arc};

use tracing::{debug, error};
use twilight_gateway::Event;
use twilight_model::application::interaction::InteractionData;

use crate::{
    channels::RuntimeMessages, discord::DiscordBotClient,
    plugins::discord_bot::plugin::discord_types::Events as DiscordEvents,
};

impl DiscordBotClient {
    pub async fn handle_event(discord_bot_client: Arc<DiscordBotClient>, event: Event) {
        match event {
            Event::InteractionCreate(interaction_create) => {
                match interaction_create.data.as_ref() {
                    Some(InteractionData::ApplicationCommand(command_data)) => {
                        let initialized_plugins =
                            discord_bot_client.plugin_registrations.read().await;

                        let Some(plugin) = initialized_plugins
                            .discord_events
                            .interaction_create
                            .application_commands
                            .get(&command_data.name)
                        else {
                            return;
                        };

                        let _ = discord_bot_client
                            .runtime_tx
                            .send(RuntimeMessages::CallDiscordEvent(
                                plugin.0.clone(),
                                DiscordEvents::InteractionCreate(
                                    simd_json::to_vec(&interaction_create).unwrap(),
                                ),
                            ))
                            .await;
                    }
                    Some(InteractionData::MessageComponent(message_component_interaction_data)) => {
                        let initialized_plugins =
                            discord_bot_client.plugin_registrations.read().await;

                        let Some(plugin) = initialized_plugins
                            .discord_events
                            .interaction_create
                            .message_components
                            .get(&message_component_interaction_data.custom_id)
                        else {
                            return;
                        };

                        let _ = discord_bot_client
                            .runtime_tx
                            .send(RuntimeMessages::CallDiscordEvent(
                                plugin.0.clone(),
                                DiscordEvents::InteractionCreate(
                                    simd_json::to_vec(&interaction_create).unwrap(),
                                ),
                            ))
                            .await;
                    }
                    Some(InteractionData::ModalSubmit(modal_interaction_data)) => {
                        let initialized_plugins =
                            discord_bot_client.plugin_registrations.read().await;

                        let Some(plugin) = initialized_plugins
                            .discord_events
                            .interaction_create
                            .modals
                            .get(&modal_interaction_data.custom_id)
                        else {
                            return;
                        };

                        let _ = discord_bot_client
                            .runtime_tx
                            .send(RuntimeMessages::CallDiscordEvent(
                                plugin.0.clone(),
                                DiscordEvents::InteractionCreate(
                                    simd_json::to_vec(&interaction_create).unwrap(),
                                ),
                            ))
                            .await;
                    }
                    None => error!("Interaction data is required."),
                    _ => error!(
                        "This interaction create data type does not have support yet, interaction data type: {:#?}",
                        &interaction_create.data.as_ref().unwrap().type_id()
                    ),
                }
            }
            Event::MessageCreate(message_create) => {
                for plugin in discord_bot_client
                    .plugin_registrations
                    .read()
                    .await
                    .discord_events
                    .message_create
                    .iter()
                {
                    let _ = discord_bot_client
                        .runtime_tx
                        .send(RuntimeMessages::CallDiscordEvent(
                            plugin.clone(),
                            DiscordEvents::MessageCreate(
                                simd_json::to_vec(&message_create).unwrap(),
                            ),
                        ))
                        .await;
                }
            }
            Event::ThreadCreate(thread_create) => {
                for plugin in discord_bot_client
                    .plugin_registrations
                    .read()
                    .await
                    .discord_events
                    .thread_create
                    .iter()
                {
                    let _ = discord_bot_client
                        .runtime_tx
                        .send(RuntimeMessages::CallDiscordEvent(
                            plugin.clone(),
                            DiscordEvents::ThreadCreate(simd_json::to_vec(&thread_create).unwrap()),
                        ))
                        .await;
                }
            }
            Event::ThreadDelete(thread_delete) => {
                for plugin in discord_bot_client
                    .plugin_registrations
                    .read()
                    .await
                    .discord_events
                    .thread_delete
                    .iter()
                {
                    let _ = discord_bot_client
                        .runtime_tx
                        .send(RuntimeMessages::CallDiscordEvent(
                            plugin.clone(),
                            DiscordEvents::ThreadDelete(simd_json::to_vec(&thread_delete).unwrap()),
                        ))
                        .await;
                }
            }
            Event::ThreadListSync(thread_list_sync) => {
                for plugin in discord_bot_client
                    .plugin_registrations
                    .read()
                    .await
                    .discord_events
                    .thread_list_sync
                    .iter()
                {
                    let _ = discord_bot_client
                        .runtime_tx
                        .send(RuntimeMessages::CallDiscordEvent(
                            plugin.clone(),
                            DiscordEvents::ThreadListSync(
                                simd_json::to_vec(&thread_list_sync).unwrap(),
                            ),
                        ))
                        .await;
                }
            }
            Event::ThreadMemberUpdate(thread_member_update) => {
                for plugin in discord_bot_client
                    .plugin_registrations
                    .read()
                    .await
                    .discord_events
                    .thread_member_update
                    .iter()
                {
                    let _ = discord_bot_client
                        .runtime_tx
                        .send(RuntimeMessages::CallDiscordEvent(
                            plugin.clone(),
                            DiscordEvents::ThreadMemberUpdate(
                                simd_json::to_vec(&thread_member_update).unwrap(),
                            ),
                        ))
                        .await;
                }
            }
            Event::ThreadMembersUpdate(thread_members_update) => {
                for plugin in discord_bot_client
                    .plugin_registrations
                    .read()
                    .await
                    .discord_events
                    .thread_members_update
                    .iter()
                {
                    let _ = discord_bot_client
                        .runtime_tx
                        .send(RuntimeMessages::CallDiscordEvent(
                            plugin.clone(),
                            DiscordEvents::ThreadMembersUpdate(
                                simd_json::to_vec(&thread_members_update).unwrap(),
                            ),
                        ))
                        .await;
                }
            }
            Event::ThreadUpdate(thread_update) => {
                for plugin in discord_bot_client
                    .plugin_registrations
                    .read()
                    .await
                    .discord_events
                    .thread_update
                    .iter()
                {
                    let _ = discord_bot_client
                        .runtime_tx
                        .send(RuntimeMessages::CallDiscordEvent(
                            plugin.clone(),
                            DiscordEvents::ThreadUpdate(simd_json::to_vec(&thread_update).unwrap()),
                        ))
                        .await;
                }
            }
            _ => debug!(
                "Received an unhandled event: {}",
                &event.kind().name().unwrap_or("[No event kind name]")
            ),
        }
    }
}
