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
            Event::MessageCreate(message) => {
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
                            DiscordEvents::MessageCreate(simd_json::to_vec(&message).unwrap()),
                        ))
                        .await;
                }
            }
            Event::InteractionCreate(interaction) => match interaction.data.as_ref() {
                Some(InteractionData::ApplicationCommand(command_data)) => {
                    let initialized_plugins = discord_bot_client.plugin_registrations.read().await;

                    let plugin = initialized_plugins
                        .discord_events
                        .interaction_create_commands
                        .get(&command_data.name);

                    if plugin.is_none() {
                        return;
                    }

                    let plugin = plugin.unwrap();

                    debug!("Plugin function call: \"{}\"", plugin.0);
                    let _ = discord_bot_client
                        .runtime_tx
                        .send(RuntimeMessages::CallDiscordEvent(
                            plugin.0.clone(),
                            DiscordEvents::InteractionCreate(
                                simd_json::to_vec(&interaction).unwrap(),
                            ),
                        ))
                        .await;
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
