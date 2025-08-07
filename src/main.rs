use std::{
    collections::VecDeque,
    env,
    ffi::OsString,
    os::unix::process::CommandExt,
    process::{Command, ExitCode, exit},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use clap::Parser;
use tokio::{signal, sync::Mutex, task::JoinHandle};
use tracing::{error, info, level_filters::LevelFilter, warn};
use tracing_appender::non_blocking::WorkerGuard;

mod cli;
use cli::Cli;
mod http;
use http::HttpClient;
mod discord;
use discord::DiscordBotClient;
mod config;
use config::Config;
mod plugins;
use plugins::{Runtime, registry};

use crate::plugins::runtime::RuntimeBuilder;

mod utils;

static SHUTDOWN: AtomicBool = AtomicBool::new(false);
static RESTART: AtomicBool = AtomicBool::new(false);
static CTRL_C_SIGNAL: AtomicBool = AtomicBool::new(false);

fn main() -> ExitCode {
    let no_error_exit_code = match CTRL_C_SIGNAL.load(Ordering::Relaxed) {
        true => ExitCode::from(130),
        false => ExitCode::from(0),
    };

    if let Ok(should_restart) = run() {
        if !should_restart {
            info!("Exiting the bot");
            return no_error_exit_code;
        }

        restart();
    }

    error!("Exiting the bot");
    ExitCode::from(1)
}

#[tokio::main]
async fn run() -> Result<bool, ()> {
    let cli = Cli::parse();

    let _guard = initialization(
        cli.log_stdout_level,
        cli.log_stdout_ansi,
        cli.log_file_level,
        cli.log_file_ansi,
    )?;

    info!("Loading and parsing the config file");
    let config = Config::new(&cli.config_path)?;

    info!("Creating the http client");
    let http_client = HttpClient::new()?;

    info!("Fetching and locally storing the plugins");
    let available_plugins = registry::get_plugins(
        &http_client,
        &config.plugins,
        &config.directory,
        config.cache,
    )
    .await?;

    info!("Creating the WASIp2 runtime");
    let runtime_builder = RuntimeBuilder::new();

    info!("Initializing the plugins");
    let (runtime, initialized_plugins, initialized_plugins_registrations) = runtime_builder
        .initialize_plugins(available_plugins, &config.directory)
        .await?;

    let runtime = Arc::new(runtime);

    info!("Creating the Discord bot client");
    let (mut discord_bot_client, discord_bot_shards) =
        DiscordBotClient::new(runtime.clone(), initialized_plugins).await?;

    info!("Making the Discord bot registrations (commands, scheduled jobs)");
    discord_bot_client
        .registrations(initialized_plugins_registrations)
        .await?;

    let mut tasks = Vec::with_capacity(discord_bot_shards.len());

    info!("Starting the job scheduler");
    discord_bot_client.start_job_scheduler(&mut tasks).await?;

    info!("Starting the Discord bot client shards");
    discord_bot_client
        .start(&mut tasks, discord_bot_shards)
        .await;

    shutdown(discord_bot_client, tasks).await
}

fn initialization(
    log_stdout_level: LevelFilter,
    log_stdout_ansi: bool,
    log_file_level: LevelFilter,
    log_file_ansi: bool,
) -> Result<Option<WorkerGuard>, ()> {
    let guard = utils::logger::new(
        log_stdout_level,
        log_stdout_ansi,
        log_file_level,
        log_file_ansi,
    )?;

    #[cfg(feature = "dotenv")]
    {
        info!("Loading the .env file");
        utils::env::load_dotenv()?;
    }

    info!("Validating the environment variables (DISCORD_BOT_TOKEN)");
    utils::env::validate()?;

    Ok(guard)
}

async fn shutdown(
    mut discord_bot_client: DiscordBotClient,
    tasks: Vec<JoinHandle<()>>,
) -> Result<bool, ()> {
    tokio::spawn(async move {
        if let Err(err) = signal::ctrl_c().await {
            error!("Failed to listen for the Ctrl-C signal, error: {}", &err);
            return;
        }

        info!("Shutdown signal received, press Ctrl-C again to force immediate shutdown");

        tokio::spawn(async {
            signal::ctrl_c()
                .await
                .expect("failed to listen for the Ctrl-C signal");

            warn!("Second shutdown signal received, forcing immediate shutdown");
            exit(130);
        });

        SHUTDOWN.store(true, Ordering::Relaxed);
        CTRL_C_SIGNAL.store(true, Ordering::Relaxed);

        discord_bot_client.shutdown().await;
    });

    for task in tasks {
        _ = task.await;
    }

    Ok(!CTRL_C_SIGNAL.load(Ordering::Relaxed) && RESTART.load(Ordering::Relaxed))
}

fn restart() {
    let executable_path = match env::current_exe() {
        Ok(executable_path) => executable_path,
        Err(err) => {
            error!("Failed to get the path to this executable, error: {}", &err);
            return;
        }
    };

    let mut args: VecDeque<OsString> = env::args_os().collect();

    args.pop_front();

    info!("Restarting the bot");
    let err = Command::new(executable_path).args(args).exec();

    error!("Failed to start a new instance of the bot, error: {}", &err);
}
