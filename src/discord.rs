/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Copyright © 2026 Eduard Smet */

use std::{collections::HashMap, sync::Arc};

use tokio::{
    sync::mpsc::{Receiver, Sender},
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
    utils::channels::{CoreMessages, DiscordBotClientMessages},
};

mod events;
mod interactions;
mod requests;

pub struct DiscordBotClient {
    http_client: Arc<Client>,
    shards: Vec<Shard>,
    shard_message_senders: Arc<HashMap<Id<GuildMarker>, Arc<MessageSender>>>,
    cache: Arc<InMemoryCache>,
    core_tx: Arc<Sender<CoreMessages>>,
    rx: Receiver<DiscordBotClientMessages>,
}

impl DiscordBotClient {
    pub async fn new(
        token: String,
        core_tx: Sender<CoreMessages>,
        rx: Receiver<DiscordBotClientMessages>,
    ) -> Result<Self, ()> {
        info!("Creating the Discord bot client");

        let intents = Intents::all(); // TODO: Make this configurable

        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .unwrap();

        let http_client = Client::new(token.clone());

        let config = Config::new(token, intents);

        let mut shard_message_senders = HashMap::new();

        let shards = match twilight_gateway::create_recommended(
            &http_client,
            config,
            |_, builder| builder.build(),
        )
        .await
        {
            Ok(shard_iterator) => {
                Self::map_guild_to_shard_message_sender(
                    Box::new(shard_iterator),
                    &mut shard_message_senders,
                )
                .await
            }
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

        Ok(DiscordBotClient {
            http_client: Arc::new(http_client),
            shards,
            shard_message_senders: Arc::new(shard_message_senders),
            cache,
            core_tx: Arc::new(core_tx),
            rx,
        })
    }

    pub fn start(mut self) -> JoinHandle<()> {
        let mut tasks = Vec::with_capacity(self.shards.len());

        for shard in self.shards.drain(..) {
            tasks.push(tokio::spawn(Self::shard_runner(
                self.cache.clone(),
                self.core_tx.clone(),
                shard,
            )));
        }

        tokio::spawn(async move {
            while let Some(message) = self.rx.recv().await {
                match message {
                    DiscordBotClientMessages::RegisterApplicationCommands(commands) => {
                        let _ = Self::application_command_registrations(
                            self.http_client.clone(),
                            commands,
                        )
                        .await;
                    }
                    DiscordBotClientMessages::Request(request, response_sender) => {
                        let _ = response_sender.send(
                            Self::request(
                                self.http_client.clone(),
                                self.shard_message_senders.clone(),
                                request,
                            )
                            .await,
                        );
                    }
                }
            }

            self.shutdown(tasks);
        })
    }

    async fn shard_runner(
        cache: Arc<InMemoryCache>,
        core_tx: Arc<Sender<CoreMessages>>,
        mut shard: Shard,
    ) {
        while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
            let Ok(event) = item else {
                error!(
                    "Something went wrong while receiving the next gateway event: {}",
                    item.as_ref().unwrap_err()
                );

                continue;
            };

            if event.kind() == EventType::GatewayClose && SHUTDOWN.read().await.is_some() {
                break;
            }

            cache.update(&event);

            tokio::spawn(Self::handle_event(core_tx.clone(), event));
        }
    }

    async fn map_guild_to_shard_message_sender(
        shard_iterator: Box<dyn ExactSizeIterator<Item = Shard>>,
        shard_message_senders: &mut HashMap<Id<GuildMarker>, Arc<MessageSender>>,
    ) -> Vec<Shard> {
        let mut shards = vec![];

        for mut shard in shard_iterator {
            let shard_message_sender = Arc::new(shard.sender());

            let next_event = shard.next_event(EventTypeFlags::READY).await;

            if let Some(item) = next_event
                && let Ok(Event::Ready(ready_event)) = item
            {
                info!("Shard is ready, logged in as {}", &ready_event.user.name);

                for guild in ready_event.guilds {
                    shard_message_senders.insert(guild.id, shard_message_sender.clone());
                }
            }

            shards.push(shard);
        }

        shards
    }

    async fn shutdown(&self, mut tasks: Vec<JoinHandle<()>>) {
        for sender in self.shard_message_senders.values() {
            _ = sender.close(CloseFrame::NORMAL);
        }

        for task in tasks.drain(..) {
            let _ = task.await;
        }
    }
}
