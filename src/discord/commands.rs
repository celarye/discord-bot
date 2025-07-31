use tracing::error;
use twilight_model::{
    application::command::Command,
    id::{Id, marker::GuildMarker},
};

use crate::{
    discord::DiscordBotClient,
    plugins::{InitializedPluginRegistrationsCommand, InitializedPluginRegistrationsCommandData},
};

impl DiscordBotClient {
    pub async fn register_commands(
        &self,
        initialized_plugins_registrations_commands: Vec<InitializedPluginRegistrationsCommand>,
    ) -> Result<(), ()> {
        let mut commands = vec![];
        let mut command_count = 0;

        for mut command in initialized_plugins_registrations_commands {
            let mut occurence_count: u8 = 0;

            let mut command_data = match simd_json::from_slice::<
                InitializedPluginRegistrationsCommandData,
            >(&mut command.command_data)
            {
                Ok(command) => command,
                Err(err) => {
                    error!(
                        "Something went wrong while deserializing the command, error: {}",
                        &err
                    );
                    continue;
                }
            };

            loop {
                if occurence_count != 0 {
                    command_data.name += format!("~{occurence_count}").as_str();
                }

                if self
                    .data
                    .initialized_plugins
                    .read()
                    .await
                    .discord_events
                    .interaction_create_commands
                    .contains_key(&command_data.name)
                {
                    // TODO: Wait with insertion until after it has been successfully pushed to
                    // Discord's servers?
                    self.data
                        .initialized_plugins
                        .write()
                        .await
                        .discord_events
                        .interaction_create_commands
                        .insert(command_data.name.clone(), command.plugin_name);
                    break;
                }

                occurence_count += 1;
            }

            if command_data.guild_id.is_some()
                && !self
                    .data
                    .current_user_guilds
                    .read()
                    .await
                    .iter()
                    .map(|g| g.id)
                    .collect::<Vec<Id<GuildMarker>>>()
                    .contains(command_data.guild_id.as_ref().unwrap())
            {
                error!("Plugin provided a Guild Id in which the bot current user is not a member");
                continue;
            }
            command_count += 1;

            commands.push(Command {
                application_id: None,
                contexts: Some(command_data.contexts),
                default_member_permissions: command_data.default_member_permissions,
                #[allow(deprecated)]
                dm_permission: None,
                description: command_data.desscription,
                description_localizations: command_data.desscription_localizations,
                guild_id: command_data.guild_id,
                id: None,
                integration_types: command_data.integration_types,
                kind: command_data.kind,
                name: command_data.name,
                name_localizations: command_data.name_localizations,
                nsfw: command_data.nsfw,
                options: command_data.options,
                version: Id::new(command_count),
            });
        }

        let application_id = self
            .http_client
            .current_user_application()
            .await
            .unwrap()
            .model()
            .await
            .unwrap()
            .id;

        let http_interaction_client = self.http_client.interaction(application_id);

        if let Err(err) = http_interaction_client.set_global_commands(&commands).await {
            error!(
                "Something went wrong while registering commands, error: {}",
                &err
            );
            return Err(());
        }

        Ok(())
    }
}
