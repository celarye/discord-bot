use std::{
    collections::HashMap,
    env,
    sync::{Arc, atomic::Ordering},
};

use tokio::{sync::RwLock, task::JoinHandle};
use tracing::error;
use twilight_cache_inmemory::{DefaultInMemoryCache, InMemoryCache, ResourceType};
use twilight_gateway::{
    CloseFrame, Config, EventType, EventTypeFlags, Intents, MessageSender, Shard, ShardId,
    StreamExt,
};
use twilight_http::Client;
use twilight_model::id::{Id, marker::GuildMarker};

use crate::{
    SHUTDOWN,
    plugins::{PluginRegistrations, runtime::Runtime},
};

mod commands;
mod event_handler;
mod requests;

pub struct DiscordBotClientReceiver {
    shards: Box<dyn ExactSizeIterator<Item = Shard>>,
    cache: Arc<InMemoryCache>,
    runtime: Arc<Runtime>,
    plugin_registrations: Arc<RwLock<PluginRegistrations>>,
}

pub struct DiscordBotClientSender {
    http_client: Client,
    pub shard_message_senders_shard_id: RwLock<HashMap<ShardId, Arc<MessageSender>>>,
    pub shard_message_senders: RwLock<HashMap<Id<GuildMarker>, Arc<MessageSender>>>,
    cache: Arc<InMemoryCache>,
}

impl DiscordBotClientReceiver {
    pub async fn new(
        discord_bot_client_sender: &DiscordBotClientSender,
        runtime: Arc<Runtime>,
        plugin_registrations: Arc<RwLock<PluginRegistrations>>,
    ) -> Result<(Self, usize), ()> {
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

        let shard_count = shards.len();

        Ok((
            DiscordBotClientReceiver {
                shards,
                cache: discord_bot_client_sender.cache.clone(),
                runtime,
                plugin_registrations,
            },
            shard_count,
        ))
    }

    pub async fn start(self, tasks: &mut Vec<JoinHandle<()>>) {
        for shard in self.shards {
            let shard_message_sender = shard.sender();
            self.runtime
                .register_shard_message_senders_to_shard_id(shard.id(), shard_message_sender)
                .await;

            tasks.push(tokio::spawn(Self::shard_runner(
                shard,
                self.cache.clone(),
                self.runtime.clone(),
                self.plugin_registrations.clone(),
            )));
        }
    }

    pub async fn shard_runner(
        mut shard: Shard,
        cache: Arc<InMemoryCache>,
        runtime: Arc<Runtime>,
        plugin_registations: Arc<RwLock<PluginRegistrations>>,
    ) {
        while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
            let Ok(event) = item else {
                error!(
                    "Something went wrong while receiving the next gateway event: {}",
                    item.as_ref().unwrap_err()
                );

                continue;
            };

            if event.kind() == EventType::GatewayClose && SHUTDOWN.shutdown.load(Ordering::Relaxed)
            {
                break;
            }

            cache.update(&event);

            tokio::spawn(Self::handle_event(
                event,
                runtime.clone(),
                plugin_registations.clone(),
            ));
        }
    }
}

impl DiscordBotClientSender {
    pub async fn new() -> Result<Self, ()> {
        let token = env::var("DISCORD_BOT_TOKEN").unwrap();

        let http_client = Client::new(token);

        let shard_message_senders_shard_id = RwLock::new(HashMap::new());
        let shard_message_senders = RwLock::new(HashMap::new());

        let cache = Arc::new(
            DefaultInMemoryCache::builder()
                .resource_types(ResourceType::all())
                .build(),
        );

        Ok(DiscordBotClientSender {
            http_client,
            shard_message_senders_shard_id,
            shard_message_senders,
            cache,
        })
    }

    pub async fn shutdown(&self) {
        for sender in self.shard_message_senders.read().await.values() {
            _ = sender.close(CloseFrame::NORMAL);
        }
    }
}
