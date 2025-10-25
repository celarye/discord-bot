use std::{
    collections::HashMap,
    env,
    sync::{Arc, atomic::Ordering},
};

use tokio::{
    sync::{
        Mutex, RwLock,
        mpsc::{Receiver, Sender},
    },
    task::JoinHandle,
};
use tracing::{error, info};
use twilight_cache_inmemory::{DefaultInMemoryCache, InMemoryCache, ResourceType};
use twilight_gateway::{
    CloseFrame, Config, Event, EventType, EventTypeFlags, Intents, MessageSender, Shard, StreamExt,
};
use twilight_http::Client;
use twilight_model::id::{Id, marker::GuildMarker};

use crate::{
    SHUTDOWN,
    channels::{DiscordBotClientMessages, RuntimeMessages},
    plugins::PluginRegistrations,
};

mod events;
mod interactions;
mod requests;

pub struct DiscordBotClient {
    http_client: Arc<Client>,
    shard_message_senders: Arc<RwLock<HashMap<Id<GuildMarker>, Arc<MessageSender>>>>,
    cache: Arc<InMemoryCache>,
    plugin_registrations: Arc<RwLock<PluginRegistrations>>,
    runtime_tx: Arc<Sender<RuntimeMessages>>,
    runtime_rx: Arc<Mutex<Receiver<DiscordBotClientMessages>>>,
}

impl DiscordBotClient {
    pub async fn new(
        plugin_registrations: Arc<RwLock<PluginRegistrations>>,
        runtime_tx: Sender<RuntimeMessages>,
        runtime_rx: Receiver<DiscordBotClientMessages>,
    ) -> Result<(Self, Box<dyn ExactSizeIterator<Item = Shard> + Send>), ()> {
        let token = env::var("DISCORD_BOT_TOKEN").unwrap();
        let intents = Intents::all();

        let http_client = Client::new(token.clone());

        let config = Config::new(token, intents);

        let shards = match twilight_gateway::create_recommended(
            &http_client,
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

        let shard_message_senders = Arc::new(RwLock::new(HashMap::new()));

        let cache = Arc::new(
            DefaultInMemoryCache::builder()
                .resource_types(ResourceType::all())
                .build(),
        );

        Ok((
            DiscordBotClient {
                http_client: Arc::new(http_client),
                shard_message_senders,
                cache,
                plugin_registrations,
                runtime_tx: Arc::new(runtime_tx),
                runtime_rx: Arc::new(Mutex::new(runtime_rx)),
            },
            shards,
        ))
    }

    pub async fn start(
        self,
        shards: Box<dyn ExactSizeIterator<Item = Shard> + Send>,
    ) -> JoinHandle<()> {
        let mut tasks = Vec::with_capacity(shards.len());

        let discord_bot_client = Arc::new(self);

        for shard in shards {
            tasks.push(tokio::spawn(Self::shard_runner(
                discord_bot_client.clone(),
                shard,
            )));
        }

        tokio::spawn(async move {
            while let Some(message) = discord_bot_client.runtime_rx.lock().await.recv().await {
                match message {
                    DiscordBotClientMessages::RegisterApplicationCommands(commands) => {
                        let _ = discord_bot_client
                            .application_command_registrations(commands)
                            .await;
                    }
                    DiscordBotClientMessages::Request(request, response_sender) => {
                        let _ = response_sender.send(discord_bot_client.request(request).await);
                    }
                    DiscordBotClientMessages::Shutdown(is_done) => {
                        for sender in discord_bot_client
                            .shard_message_senders
                            .read()
                            .await
                            .values()
                        {
                            _ = sender.close(CloseFrame::NORMAL);
                        }

                        for task in tasks.drain(..) {
                            let _ = task.await;
                        }

                        let _ = is_done.send(());
                    }
                };
            }
        })
    }

    pub async fn shard_runner(discord_bot_client: Arc<DiscordBotClient>, mut shard: Shard) {
        let shard_message_sender = Arc::new(shard.sender());

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

            discord_bot_client.cache.update(&event);

            match event {
                Event::Ready(ready) => {
                    info!("Shard is ready, logged in as {}", &ready.user.name);

                    for guild in ready.guilds {
                        discord_bot_client
                            .shard_message_senders
                            .write()
                            .await
                            .insert(guild.id, shard_message_sender.clone());
                    }
                }
                _ => {
                    tokio::spawn(Self::handle_event(discord_bot_client.clone(), event));
                }
            };
        }
    }
}
