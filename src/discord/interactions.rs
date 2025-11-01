use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{error, info};
use twilight_http::{request::Request, routing::Route};
use twilight_model::{
    application::command::Command,
    id::{
        Id,
        marker::{ApplicationMarker, CommandMarker},
    },
};

use crate::{discord::DiscordBotClient, plugins::DiscordApplicationCommandRegistrationRequest};

#[derive(Default, Deserialize, Serialize)]
struct Interactions {
    application_commands: HashMap<Id<CommandMarker>, Command>,
    message_components: HashMap<String, (String, String)>,
    modals: HashMap<String, (String, String)>,
}

impl DiscordBotClient {
    pub async fn application_command_registrations(
        &self,
        discord_application_command_registration_request: Vec<
            DiscordApplicationCommandRegistrationRequest,
        >,
    ) -> Result<(), ()> {
        let mut discord_commands = HashMap::new();

        let mut global_commands = HashMap::new();

        let mut guild_commands = HashMap::new();

        for mut command in discord_application_command_registration_request {
            let command_data = match simd_json::from_slice::<Command>(&mut command.command_data) {
                Ok(command) => command,
                Err(err) => {
                    error!(
                        "Something went wrong while deserializing the {} command from the {} plugin requested to register, error: {}",
                        &command.internal_id, &command.plugin_id, &err
                    );
                    continue;
                }
            };

            if let Some(guild_id) = command_data.guild_id {
                info!(
                    "Guild application command {} from the {} plugin requested to be registered.",
                    &command.internal_id, &command.plugin_id
                );

                guild_commands
                    .entry(guild_id)
                    .or_insert(HashMap::new())
                    .entry(command_data.name.clone())
                    .or_insert(vec![])
                    .push((command.plugin_id, command.internal_id, command_data.clone()));
            } else {
                info!(
                    "Global application command {} from the {} plugin requested to be registered.",
                    &command.internal_id, &command.plugin_id
                );

                global_commands
                    .entry(command_data.name.clone())
                    .or_insert(vec![])
                    .push((command.plugin_id, command.internal_id, command_data.clone()));
            }
        }

        let application_id = match self.http_client.current_user_application().await {
            Ok(response) => match response.model().await {
                Ok(application) => application.id,
                Err(err) => {
                    error!(
                        "Something went wrong while deserializing the application data, error: {}",
                        &err
                    );
                    return Err(());
                }
            },
            Err(err) => {
                error!(
                    "Something went wrong while requesting the application data, error: {}",
                    &err
                );
                return Err(());
            }
        };

        let global_discord_commands_request = match Request::builder(&Route::GetGlobalCommands {
            application_id: application_id.get(),
            with_localizations: Some(true),
        })
        .build()
        {
            Ok(global_discord_commands_request) => global_discord_commands_request,
            Err(err) => {
                error!(
                    "Failed to build the get global commands request, error: {}",
                    &err
                );
                return Err(());
            }
        };

        match self
            .http_client
            .request::<Vec<Command>>(global_discord_commands_request)
            .await
        {
            Ok(response) => match response.model().await {
                Ok(global_discord_commands) => {
                    for global_discord_command in global_discord_commands {
                        discord_commands
                            .insert(global_discord_command.name.clone(), global_discord_command);
                    }
                }
                Err(err) => {
                    error!(
                        "Something went wrong while deserializing the global application commands, error: {}",
                        &err
                    );
                    return Err(());
                }
            },
            Err(err) => {
                error!(
                    "Something went wrong while requesting the global application commands, error: {}",
                    &err
                );
                return Err(());
            }
        }

        // TODO: Endpoint is limited to 200 guilds per request, pagination needs to be implemented.
        let current_user_guilds = match self.http_client.current_user_guilds().await {
            Ok(response) => match response.model().await {
                Ok(current_user_guilds) => current_user_guilds,
                Err(err) => {
                    error!(
                        "Something went wrong while deserializing the global application commands, error: {}",
                        &err
                    );
                    return Err(());
                }
            },
            Err(err) => {
                error!(
                    "Something went wrong while requesting the global application commands, error: {}",
                    &err
                );
                return Err(());
            }
        };

        for current_user_guild in current_user_guilds {
            let guild_commands_request = match Request::builder(&Route::GetGuildCommands {
                application_id: application_id.get(),
                guild_id: current_user_guild.id.get(),
                with_localizations: Some(true),
            })
            .build()
            {
                Ok(guild_commands_request) => guild_commands_request,
                Err(err) => {
                    error!(
                        "Failed to build the get guild commands request, error: {}",
                        &err
                    );
                    continue;
                }
            };

            match self
                .http_client
                .request::<Vec<Command>>(guild_commands_request)
                .await
            {
                Ok(response) => match response.model().await {
                    Ok(single_guild_discord_commands) => {
                        for single_guild_discord_command in single_guild_discord_commands {
                            discord_commands.insert(
                                single_guild_discord_command.name.clone(),
                                single_guild_discord_command,
                            );
                        }
                    }
                    Err(err) => {
                        error!(
                            "Something went wrong while deserializing the guild application commands, error: {}",
                            &err
                        );
                    }
                },
                Err(err) => {
                    error!(
                        "Something went wrong while requesting the guild application commands, error: {}",
                        &err
                    );
                }
            }
        }

        for mut global_commands_by_name in global_commands {
            if global_commands_by_name.1.len() == 1 {
                let global_command = global_commands_by_name.1.remove(0);

                match self
                    .register_global_application_command(
                        application_id,
                        &mut discord_commands,
                        &global_command.2,
                    )
                    .await
                {
                    Ok(command_id) => {
                        self.plugin_registrations
                            .write()
                            .await
                            .discord_events
                            .interaction_create
                            .application_commands
                            .insert(command_id, (global_command.0, global_command.1));
                    }
                    Err(()) => {
                        error!(
                            "Failed to register the {} command from the {} plugin",
                            &global_commands_by_name.0, &global_command.0
                        );
                    }
                }
            } else {
                let mut command_name_occurence_count = 1;

                for mut global_commands_by_name_entry in global_commands_by_name.1 {
                    global_commands_by_name_entry.2.name +=
                        format!("~{command_name_occurence_count}").as_str();

                    match self
                        .register_global_application_command(
                            application_id,
                            &mut discord_commands,
                            &global_commands_by_name_entry.2,
                        )
                        .await
                    {
                        Ok(command_id) => {
                            self.plugin_registrations
                                .write()
                                .await
                                .discord_events
                                .interaction_create
                                .application_commands
                                .insert(
                                    command_id,
                                    (
                                        global_commands_by_name_entry.0,
                                        global_commands_by_name_entry.1,
                                    ),
                                );
                        }
                        Err(()) => {
                            error!(
                                "Failed to register the {} command from the {} plugin",
                                &global_commands_by_name.0, &global_commands_by_name_entry.0
                            );
                        }
                    }

                    command_name_occurence_count += 1;
                }
            }
        }

        for per_guild_commands in guild_commands.into_values() {
            for mut per_guild_commands_by_name in per_guild_commands {
                if per_guild_commands_by_name.1.len() == 1 {
                    let guild_command = per_guild_commands_by_name.1.remove(0);

                    match self
                        .register_guild_application_command(
                            application_id,
                            &mut discord_commands,
                            &guild_command.2,
                        )
                        .await
                    {
                        Ok(command_id) => {
                            self.plugin_registrations
                                .write()
                                .await
                                .discord_events
                                .interaction_create
                                .application_commands
                                .insert(command_id, (guild_command.0, guild_command.1));
                        }
                        Err(()) => {
                            error!(
                                "Failed to register the {} command from the {} plugin",
                                &per_guild_commands_by_name.0, &guild_command.0
                            );
                        }
                    }
                } else {
                    let mut command_name_occurence_count = 1;

                    for mut per_guild_commands_by_name_entry in per_guild_commands_by_name.1 {
                        per_guild_commands_by_name_entry.2.name +=
                            format!("~{command_name_occurence_count}").as_str();

                        match self
                            .register_guild_application_command(
                                application_id,
                                &mut discord_commands,
                                &per_guild_commands_by_name_entry.2,
                            )
                            .await
                        {
                            Ok(command_id) => {
                                self.plugin_registrations
                                    .write()
                                    .await
                                    .discord_events
                                    .interaction_create
                                    .application_commands
                                    .insert(
                                        command_id,
                                        (
                                            per_guild_commands_by_name_entry.0,
                                            per_guild_commands_by_name_entry.1,
                                        ),
                                    );
                            }
                            Err(()) => {
                                error!(
                                    "Failed to register the {} command from the {} plugin",
                                    &per_guild_commands_by_name_entry.0,
                                    &per_guild_commands_by_name_entry.0
                                );
                            }
                        }

                        command_name_occurence_count += 1;
                    }
                }
            }
        }

        self.delete_old_application_commands(application_id, &mut discord_commands)
            .await?;

        Ok(())
    }

    async fn register_global_application_command(
        &self,
        application_id: Id<ApplicationMarker>,
        discord_commands: &mut HashMap<String, Command>,
        command: &Command,
    ) -> Result<Id<CommandMarker>, ()> {
        if let Some(discord_command) = discord_commands.remove(&command.name) {
            let request = match Request::builder(&Route::UpdateGlobalCommand {
                application_id: application_id.get(),
                command_id: discord_command.id.unwrap().get(),
            })
            .body(simd_json::to_vec(command).unwrap())
            .build()
            {
                Ok(request) => request,
                Err(err) => {
                    error!(
                        "Failed to build the create global command request, error: {}",
                        &err
                    );
                    return Err(());
                }
            };

            match self.http_client.request::<Command>(request).await {
                Ok(response) => match response.model().await {
                    Ok(command) => Ok(command.id.unwrap()),
                    Err(err) => {
                        error!(
                            "Something went wrong while deserializing the update global command response, error: {}",
                            &err
                        );
                        Err(())
                    }
                },
                Err(err) => {
                    error!(
                        "Something went wrong while requesting a global command update, error: {}",
                        &err
                    );
                    Err(())
                }
            }
        } else {
            let request = match Request::builder(&Route::CreateGlobalCommand {
                application_id: application_id.get(),
            })
            .body(simd_json::to_vec(command).unwrap())
            .build()
            {
                Ok(request) => request,
                Err(err) => {
                    error!(
                        "Failed to build the create global command request, error: {}",
                        &err
                    );
                    return Err(());
                }
            };

            match self.http_client.request::<Command>(request).await {
                Ok(response) => match response.model().await {
                    Ok(command) => Ok(command.id.unwrap()),
                    Err(err) => {
                        error!(
                            "Something went wrong while deserializing the create global command response, error: {}",
                            &err
                        );
                        Err(())
                    }
                },
                Err(err) => {
                    error!(
                        "Something went wrong while requesting a global command creation, error: {}",
                        &err
                    );
                    Err(())
                }
            }
        }
    }

    async fn register_guild_application_command(
        &self,
        application_id: Id<ApplicationMarker>,
        discord_commands: &mut HashMap<String, Command>,
        command: &Command,
    ) -> Result<Id<CommandMarker>, ()> {
        if let Some(discord_command) = discord_commands.remove(&command.name) {
            let request = match Request::builder(&Route::UpdateGuildCommand {
                application_id: application_id.get(),
                command_id: discord_command.id.unwrap().get(),
                guild_id: command.guild_id.unwrap().get(),
            })
            .body(simd_json::to_vec(command).unwrap())
            .build()
            {
                Ok(request) => request,
                Err(err) => {
                    error!(
                        "Failed to build the create global command request, error: {}",
                        &err
                    );
                    return Err(());
                }
            };

            match self.http_client.request::<Command>(request).await {
                Ok(response) => match response.model().await {
                    Ok(command) => Ok(command.id.unwrap()),
                    Err(err) => {
                        error!(
                            "Something went wrong while deserializing the update guild command response, error: {}",
                            &err
                        );
                        Err(())
                    }
                },
                Err(err) => {
                    error!(
                        "Something went wrong while requesting a guild command update, error: {}",
                        &err
                    );
                    Err(())
                }
            }
        } else {
            let request = match Request::builder(&Route::CreateGlobalCommand {
                application_id: application_id.get(),
            })
            .body(simd_json::to_vec(command).unwrap())
            .build()
            {
                Ok(request) => request,
                Err(err) => {
                    error!(
                        "Failed to build the create global command request, error: {}",
                        &err
                    );
                    return Err(());
                }
            };

            match self.http_client.request::<Command>(request).await {
                Ok(response) => match response.model().await {
                    Ok(command) => Ok(command.id.unwrap()),
                    Err(err) => {
                        error!(
                            "Something went wrong while deserializing the create guild command response, error: {}",
                            &err
                        );
                        Err(())
                    }
                },
                Err(err) => {
                    error!(
                        "Something went wrong while requesting a guild command creation, error: {}",
                        &err
                    );
                    Err(())
                }
            }
        }
    }

    async fn delete_old_application_commands(
        &self,
        application_id: Id<ApplicationMarker>,
        discord_commands: &mut HashMap<String, Command>,
    ) -> Result<(), ()> {
        for discord_command in discord_commands.values() {
            let route = match discord_command.guild_id {
                Some(guild_id) => Route::DeleteGuildCommand {
                    application_id: application_id.get(),
                    command_id: discord_command.id.unwrap().get(),
                    guild_id: guild_id.get(),
                },
                None => Route::DeleteGlobalCommand {
                    application_id: application_id.get(),
                    command_id: discord_command.id.unwrap().get(),
                },
            };

            let request = match Request::builder(&route).build() {
                Ok(request) => request,
                Err(err) => {
                    error!(
                        "Failed to build the create global command request, error: {}",
                        &err
                    );
                    return Err(());
                }
            };

            match self.http_client.request::<()>(request).await {
                Ok(_) => (),
                Err(err) => {
                    error!(
                        "Something went wrong while requesting a command deletion, error: {}",
                        &err
                    );
                    return Err(());
                }
            }
        }

        Ok(())
    }
}
