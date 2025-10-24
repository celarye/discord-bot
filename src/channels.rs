use tokio::sync::{
    mpsc::{Receiver as MPSCReceiver, Sender as MPSCSender, channel},
    oneshot::Sender as OSSender,
};

use crate::plugins::{
    DiscordApplicationCommandRegistrationRequest, ScheduledJobRegistrationRequest,
    discord_bot::plugin::host_functions::{DiscordRequests, DiscordResponses},
    exports::discord_bot::plugin::plugin_functions::DiscordEvents,
};

pub enum DiscordBotClientMessages {
    RegisterApplicationCommands(Vec<DiscordApplicationCommandRegistrationRequest>),
    Request(DiscordRequests, OSSender<Result<DiscordResponses, String>>),
    Shutdown(OSSender<()>),
}

pub enum JobSchedulerMessages {
    RegisterScheduledJobs(Vec<ScheduledJobRegistrationRequest>),
    Shutdown(OSSender<()>),
}

pub enum RuntimeMessages {
    CallDiscordEvent(String, DiscordEvents),
    CallScheduledJob(String, String),
}

type Channels = (
    (
        MPSCSender<DiscordBotClientMessages>,
        MPSCReceiver<DiscordBotClientMessages>,
    ),
    (
        MPSCSender<JobSchedulerMessages>,
        MPSCReceiver<JobSchedulerMessages>,
    ),
    (
        MPSCSender<RuntimeMessages>,
        MPSCSender<RuntimeMessages>,
        MPSCReceiver<RuntimeMessages>,
    ),
);

pub fn new() -> Channels {
    let (runtime_tx, runtime_rx) = channel::<RuntimeMessages>(400);

    (
        (channel::<DiscordBotClientMessages>(200)),
        (channel::<JobSchedulerMessages>(200)),
        (runtime_tx.clone(), runtime_tx, runtime_rx),
    )
}
