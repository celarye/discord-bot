use tracing::{error, info};
use twilight_model::application::command::Command;

use crate::{discord::DiscordBotClient, plugins::DiscordApplicationCommandRegistrationRequest};

impl DiscordBotClient {
    // TODO: Make list, compare with Discords already registered list and make additions and
    // removals
    pub async fn application_command_registrations(
        &self,
        discord_application_command_registration_request: Vec<
            DiscordApplicationCommandRegistrationRequest,
        >,
    ) -> Result<(), ()> {
        let mut commands = vec![];

        for mut command in discord_application_command_registration_request {
            let mut occurence_count: u8 = 0;

            let mut command_data = match simd_json::from_slice::<Command>(&mut command.command_data)
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

                if !self
                    .plugin_registrations
                    .read()
                    .await
                    .discord_events
                    .interaction_create
                    .application_commands
                    .contains_key(&command_data.name)
                {
                    // TODO: Wait with insertion until after it has been successfully pushed to
                    // Discord's servers?
                    self.plugin_registrations
                        .write()
                        .await
                        .discord_events
                        .interaction_create
                        .application_commands
                        .insert(
                            command_data.name.clone(),
                            (command.plugin_id, command.internal_id),
                        );
                    break;
                }

                occurence_count += 1;
            }

            let current_user_id = self.cache.current_user().unwrap().id;

            if command_data.guild_id.is_some()
                && !self
                    .cache
                    .user_guilds(current_user_id)
                    .unwrap()
                    .contains(command_data.guild_id.as_ref().unwrap())
            {
                error!("Plugin provided a Guild Id in which the bot current user is not a member");
                continue;
            }

            info!("New command to be registered, name: {}", &command_data.name);

            commands.push(command_data);
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

        // See the above TODO
        //info!("Setting global commands");
        //if let Err(err) = http_interaction_client.set_global_commands(&commands).await {
        //    error!(
        //        "Something went wrong while registering commands, error: {}",
        //        &err
        //    );
        //    return Err(());
        //}

        Ok(())
    }
}
