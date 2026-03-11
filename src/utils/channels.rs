/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Copyright © 2026 Eduard Smet */

use tokio::sync::{
    mpsc::{Receiver as MPSCReceiver, Sender as MPSCSender, channel},
    oneshot::Sender as OSSender,
};
use uuid::Uuid;

use crate::{
    Shutdown,
    plugins::{
        PluginRegistrationRequestsApplicationCommand,
        discord_bot::plugin::{
            discord_export_types::DiscordEvents,
            discord_import_types::{DiscordRequests, DiscordResponses},
        },
    },
};

pub enum CoreMessages {
    GetState(String, OSSender<anyhow::Result<Vec<u8>>>),
    InsertState(String, Vec<u8>, OSSender<anyhow::Result<Option<Vec<u8>>>>),
    DeleteState(String, OSSender<Vec<u8>>),

    JobScheduler(JobSchedulerMessages),
    DiscordBotClient(DiscordBotClientMessages),
    Runtime(RuntimeMessages),

    Shutdown(Shutdown),
}

pub enum JobSchedulerMessages {
    AddJob(Uuid, String, OSSender<anyhow::Result<Uuid>>),
    RemoveJob(Uuid, OSSender<anyhow::Result<()>>),
}

pub enum DiscordBotClientMessages {
    RegisterApplicationCommands(Vec<PluginRegistrationRequestsApplicationCommand>),
    Request(
        DiscordRequests,
        OSSender<Result<Option<DiscordResponses>, String>>,
    ),
}

pub enum RuntimeMessages {
    JobScheduler(RuntimeMessagesJobScheduler),
    Discord(RuntimeMessagesDiscord),
}

pub enum RuntimeMessagesJobScheduler {
    CallScheduledJob(Uuid, Uuid),
}

pub enum RuntimeMessagesDiscord {
    CallDiscordEvent(Uuid, DiscordEvents),
}

pub struct Channels {
    pub core: ChannelsCore,
    pub job_scheduler: ChannelsJobScheduler,
    pub discord_bot_client: ChannelsDiscordBotClient,
    pub runtime: ChannelsRuntime,
}

pub struct ChannelsCore {
    pub job_scheduler_tx: MPSCSender<JobSchedulerMessages>,
    pub discord_bot_client_tx: MPSCSender<DiscordBotClientMessages>,
    pub runtime_tx: MPSCSender<RuntimeMessages>,
    pub rx: MPSCReceiver<CoreMessages>,
}

pub struct ChannelsJobScheduler {
    pub core_tx: MPSCSender<CoreMessages>,
    pub rx: MPSCReceiver<JobSchedulerMessages>,
}

pub struct ChannelsDiscordBotClient {
    pub core_tx: MPSCSender<CoreMessages>,
    pub rx: MPSCReceiver<DiscordBotClientMessages>,
}

pub struct ChannelsRuntime {
    pub core_tx: MPSCSender<CoreMessages>,
    pub rx: MPSCReceiver<RuntimeMessages>,
}

pub fn new() -> Channels {
    let (core_tx, core_rx) = channel::<CoreMessages>(1024);
    let (job_scheduler_tx, job_scheduler_rx) = channel::<JobSchedulerMessages>(128);
    let (discord_bot_client_tx, discord_bot_client_rx) = channel::<DiscordBotClientMessages>(512);
    let (runtime_tx, runtime_rx) = channel::<RuntimeMessages>(512);

    Channels {
        core: ChannelsCore {
            job_scheduler_tx,
            discord_bot_client_tx,
            runtime_tx,
            rx: core_rx,
        },
        job_scheduler: ChannelsJobScheduler {
            core_tx: core_tx.clone(),
            rx: job_scheduler_rx,
        },
        discord_bot_client: ChannelsDiscordBotClient {
            core_tx: core_tx.clone(),
            rx: discord_bot_client_rx,
        },
        runtime: ChannelsRuntime {
            core_tx,
            rx: runtime_rx,
        },
    }
}
