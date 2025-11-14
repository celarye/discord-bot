use tokio::sync::{
    mpsc::{Receiver as MPSCReceiver, Sender as MPSCSender, channel},
    oneshot::Sender as OSSender,
};

use crate::plugins::{
    PluginRegistrationRequestsApplicationCommand, PluginRegistrationRequestsScheduledJob,
    discord_bot::plugin::host_functions::{DiscordRequests, DiscordResponses},
    exports::discord_bot::plugin::plugin_functions::DiscordEvents,
};

pub enum DiscordBotClientMessages {
    RegisterApplicationCommands(Vec<PluginRegistrationRequestsApplicationCommand>),
    Request(
        DiscordRequests,
        OSSender<Result<Option<DiscordResponses>, String>>,
    ),
    Shutdown(OSSender<()>),
}

pub enum JobSchedulerMessages {
    RegisterScheduledJobs(Vec<PluginRegistrationRequestsScheduledJob>),
    Shutdown(OSSender<()>),
}

pub enum RuntimeMessages {
    CallDiscordEvent(String, DiscordEvents),
    CallScheduledJob(String, String),
}

pub struct Channels {
    pub discord_bot_client: ChannelsDiscordBotClient,
    pub job_scheduler: ChannelsJobScheduler,
    pub runtime: ChannelsRuntime,
}

pub struct ChannelsDiscordBotClient {
    pub sender: MPSCSender<DiscordBotClientMessages>,
    pub receiver: MPSCReceiver<DiscordBotClientMessages>,
}

pub struct ChannelsJobScheduler {
    pub sender: MPSCSender<JobSchedulerMessages>,
    pub receiver: MPSCReceiver<JobSchedulerMessages>,
}

pub struct ChannelsRuntime {
    pub discord_bot_client_sender: MPSCSender<RuntimeMessages>,
    pub job_scheduler_sender: MPSCSender<RuntimeMessages>,
    pub receiver: MPSCReceiver<RuntimeMessages>,
}

pub fn new() -> Channels {
    let (discord_bot_client_tx, discord_bot_client_rx) = channel::<DiscordBotClientMessages>(200);
    let (job_scheduler_tx, job_scheduler_rx) = channel::<JobSchedulerMessages>(200);
    let (runtime_tx, runtime_rx) = channel::<RuntimeMessages>(400);

    Channels {
        discord_bot_client: ChannelsDiscordBotClient {
            sender: discord_bot_client_tx,
            receiver: discord_bot_client_rx,
        },
        job_scheduler: ChannelsJobScheduler {
            sender: job_scheduler_tx,
            receiver: job_scheduler_rx,
        },
        runtime: ChannelsRuntime {
            discord_bot_client_sender: runtime_tx.clone(),
            job_scheduler_sender: runtime_tx,
            receiver: runtime_rx,
        },
    }
}
