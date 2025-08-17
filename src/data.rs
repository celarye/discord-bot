use std::collections::HashMap;

use serde::Serialize;
use tokio::sync::RwLock;
use tracing::error;
use twilight_model::user::{CurrentUser, CurrentUserGuild};

use crate::{
    discord::DiscordBotClientSender,
    plugins::{InitializedPlugins, InitializedPluginsDiscordEvents},
};

#[derive(Serialize)]
pub struct Data {
    #[serde(with = "rwlock_serde")]
    pub current_user: RwLock<CurrentUser>,
    #[serde(with = "rwlock_serde")]
    pub current_user_guilds: RwLock<Vec<CurrentUserGuild>>,
    #[serde(with = "rwlock_serde")]
    pub initialized_plugins: RwLock<InitializedPlugins>,
}

mod rwlock_serde {
    use serde::Serialize;
    use serde::ser::Serializer;
    use tokio::sync::RwLock;

    pub fn serialize<S, T>(val: &RwLock<T>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        T::serialize(&*val.blocking_read(), s)
    }
}

impl Data {
    pub async fn new(discord_bot_client_sender: &DiscordBotClientSender) -> Result<Self, ()> {
        let current_user = match discord_bot_client_sender.http_client.current_user().await {
            Ok(rcurrent_user) => match rcurrent_user.model().await {
                Ok(current_user) => current_user,
                Err(err) => {
                    error!(
                        "Failed to deserialize the response when fetching the current user, error: {}",
                        &err
                    );
                    return Err(());
                }
            },
            Err(err) => {
                error!("Failed to fetch the current user, error: {}", &err);
                return Err(());
            }
        };

        Ok(Data {
            current_user: RwLock::new(current_user),
            current_user_guilds: RwLock::new(vec![]),
            initialized_plugins: RwLock::new(InitializedPlugins {
                discord_events: InitializedPluginsDiscordEvents {
                    interaction_create_commands: HashMap::new(),
                    message_create: vec![],
                },
                scheduled_jobs: HashMap::new(),
                dependencies: HashMap::new(),
            }),
        })
    }
}
