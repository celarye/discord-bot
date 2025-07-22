use std::{env, sync::Arc};

use tokio::sync::{Mutex, RwLock};
use tracing::error;
use twilight_cache_inmemory::{DefaultInMemoryCache, InMemoryCache, ResourceType};
use twilight_gateway::{EventTypeFlags, Intents, Shard, ShardId, StreamExt};
use twilight_http::Client;

use crate::plugins::{InitializedPluginRegistrations, InitializedPlugins, Runtime};
pub mod data;
use data::Data;
pub mod commands;
pub mod event_handler;

pub struct DiscordBotClient {
    shard: Shard,
    http_client: Arc<Client>,
    runtime: Arc<Mutex<Runtime>>,
    data: Arc<RwLock<Box<Data>>>,
    cache: InMemoryCache,
}

impl DiscordBotClient {
    pub async fn new(
        runtime: Arc<Mutex<Runtime>>,
        initialized_plugins: Arc<RwLock<InitializedPlugins>>,
        initialized_plugins_registrations: InitializedPluginRegistrations,
    ) -> Result<(Self, Arc<RwLock<Box<Data>>>), ()> {
        let token = env::var("DISCORD_BOT_TOKEN").unwrap();

        let intents = Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT;

        let shard = Shard::new(ShardId::ONE, token.clone(), intents);

        let cache = DefaultInMemoryCache::builder()
            .resource_types(ResourceType::MESSAGE)
            .build();

        let http_client = Arc::new(Client::new(token));

        let current_user = match http_client.current_user().await {
            Ok(rcurrent_user) => match rcurrent_user.model().await {
                Ok(current_user) => current_user,
                Err(err) => {
                    error!(
                        "Failed to diserialize the response when fetching the current user, error: {}",
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

        let current_user_guilds = match http_client.current_user_guilds().await {
            Ok(rcurrent_user_guilds) => match rcurrent_user_guilds.model().await {
                Ok(current_user_guilds) => current_user_guilds,
                Err(err) => {
                    error!(
                        "Failed to diserialize the response when fetching the current user guilds, error: {}",
                        &err
                    );
                    return Err(());
                }
            },
            Err(err) => {
                error!("Failed to fetch the current user guilds, error: {}", &err);
                return Err(());
            }
        };

        let data = Arc::new(RwLock::new(Data::new(
            current_user,
            current_user_guilds,
            initialized_plugins,
        )));

        let application_id = http_client
            .current_user_application()
            .await
            .unwrap()
            .model()
            .await
            .unwrap()
            .id;

        let interaction_http_client = http_client.interaction(application_id);

        commands::register(
            &interaction_http_client,
            data.clone(),
            initialized_plugins_registrations.commands,
        )
        .await?;

        Ok((
            DiscordBotClient {
                shard,
                http_client,
                runtime: runtime.clone(),
                data: data.clone(),
                cache,
            },
            data,
        ))
    }

    pub async fn start(mut self) {
        while let Some(item) = self.shard.next_event(EventTypeFlags::all()).await {
            let Ok(event) = item else {
                error!(
                    "Something went wrong while receiving the next gateway event: {}",
                    item.as_ref().unwrap_err()
                );

                continue;
            };

            self.cache.update(&event);

            tokio::spawn(event_handler::run(
                event,
                self.runtime.clone(),
                self.http_client.clone(),
                self.data.clone(),
            ));
        }
    }
}
