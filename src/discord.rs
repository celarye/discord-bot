use std::{
    collections::HashMap,
    env,
    sync::{Arc, atomic::Ordering},
};

use tokio::task::JoinHandle;
use tokio_cron_scheduler::JobScheduler;
use tracing::{error, info};
use twilight_cache_inmemory::{DefaultInMemoryCache, InMemoryCache, ResourceType};
use twilight_gateway::{
    CloseFrame, Config, EventType, EventTypeFlags, Intents, MessageSender, Shard, ShardId,
    StreamExt,
};
use twilight_http::Client;
use twilight_model::id::{Id, marker::GuildMarker};

use crate::{
    SHUTDOWN,
    plugins::{
        InitializedPluginRegistrations, InitializedPlugins, InitializedPluginsDiscordEvents,
        runtime::Runtime,
    },
};

pub mod data;
use data::Data;
pub mod commands;
pub mod event_handler;
mod job_scheduler;

pub struct DiscordBotClientReceiver {
    shards: Box<dyn ExactSizeIterator<Item = Shard>>,
    cache: Arc<InMemoryCache>,
    runtime: Arc<Runtime>,
}

pub struct DiscordBotClientSender {
    http_client: Client,
    shard_message_senders: HashMap<Id<GuildMarker>, MessageSender>,
}

impl DiscordBotClientReceiver {
    pub async fn new(
        discord_bot_client_sender: &DiscordBotClientSender,
        runtime: Arc<Runtime>,
    ) -> Result<Self, ()> {
        let token = env::var("DISCORD_BOT_TOKEN").unwrap();
        let intents = Intents::all();

        let config = Config::new(token, intents);

        let shards = match twilight_gateway::create_recommended(
            &discord_bot_client_sender.http_client,
            config,
            |_, builder| builder.build(),
        )
        .await
        {
            Ok(shards) => Box::new(shards),
            Err(err) => {
                error!(
                    "Something went wrong while getting the recommended amount of shards from Discord, error: {}",
                    &err
                );
                return Err(());
            }
        };

        let cache = Arc::new(
            DefaultInMemoryCache::builder()
                .resource_types(ResourceType::all())
                .build(),
        );

        Ok(DiscordBotClientReceiver {
            shards,
            cache,
            runtime,
        })
    }
}

impl DiscordBotClientSender {
    pub async fn new() -> Result<Self, ()> {
        let token = env::var("DISCORD_BOT_TOKEN").unwrap();

        let http_client = Client::new(token.clone());

        let shard_message_senders = HashMap::new();

        Ok(DiscordBotClientSender {
            http_client,
            shard_message_senders,
        })
    }
}

impl DiscordBotClient {
    pub async fn new(
        runtime: Arc<Runtime>,
        initialized_plugins: InitializedPlugins,
    ) -> Result<(Self, impl ExactSizeIterator<Item = Shard>), ()> {
        let token = env::var("DISCORD_BOT_TOKEN").unwrap();
        let intents = Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT;

        let http_client = Arc::new(Client::new(token.clone()));

        let config = Config::new(token, intents);

        let shards = match twilight_gateway::create_recommended(
            &http_client,
            config,
            |_, builder| builder.build(),
        )
        .await
        {
            Ok(shards) => shards,
            Err(err) => {
                error!(
                    "Something went wrong while getting the recommended amount of shards from Discord, error: {}",
                    &err
                );
                return Err(());
            }
        };

        let cache = Arc::new(
            DefaultInMemoryCache::builder()
                .resource_types(ResourceType::all())
                .build(),
        );

        let current_user = match http_client.current_user().await {
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

        let data = Arc::new(Data::new(
            current_user,
            current_user_guilds,
            initialized_plugins,
        ));

        info!("Creating the job scheduler");
        let job_scheduler = Self::new_job_scheduler().await?;

        Ok((
            DiscordBotClient {
                shard_message_senders: HashMap::new(),
                http_client,
                job_scheduler,
                runtime,
                data: data.clone(),
                cache,
            },
            shards,
        ))
    }

    pub async fn registrations(
        &self,
        initialized_plugins_registrations: InitializedPluginRegistrations,
    ) -> Result<(), ()> {
        self.register_commands(initialized_plugins_registrations.commands)
            .await?;

        self.register_scheduled_jobs().await;

        Ok(())
    }

    pub async fn start(
        &mut self,
        tasks: &mut Vec<JoinHandle<()>>,
        shards: impl ExactSizeIterator<Item = Shard>,
    ) {
        for shard in shards {
            let shard_message_sender = Arc::new(shard.sender());

            self.shard_message_senders
                .insert(shard.id(), shard_message_sender.clone());

            tasks.push(tokio::spawn(Self::shard_runner(
                shard,
                shard_message_sender,
                self.cache.clone(),
                self.http_client.clone(),
                self.runtime.clone(),
                self.data.clone(),
            )));
        }
    }

    pub async fn shard_runner(
        mut shard: Shard,
        shard_message_sender: Arc<MessageSender>,
        cache: Arc<InMemoryCache>,
        http_client: Arc<Client>,
        runtime: Arc<Runtime>,
        data: Arc<Data>,
    ) {
        while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
            let Ok(event) = item else {
                error!(
                    "Something went wrong while receiving the next gateway event: {}",
                    item.as_ref().unwrap_err()
                );

                continue;
            };

            if event.kind() == EventType::GatewayClose && SHUTDOWN.load(Ordering::Relaxed) {
                break;
            }

            cache.update(&event);

            tokio::spawn(Self::handle_event(
                event,
                shard_message_sender.clone(),
                http_client.clone(),
                runtime.clone(),
                data.clone(),
            ));
        }
    }

    pub async fn shutdown(&mut self) {
        self.shutdown_job_scheduler().await;

        for sender in self.shard_message_senders.values() {
            _ = sender.close(CloseFrame::NORMAL);
        }
    }
}
