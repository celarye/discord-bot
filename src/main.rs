use std::{
    collections::VecDeque,
    env,
    ffi::OsString,
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, ExitCode, exit},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use clap::Parser;
use indexmap::IndexMap;
use tokio::{signal, sync::RwLock, task::JoinHandle};
use tracing::{error, info, level_filters::LevelFilter, warn};
use tracing_appender::non_blocking::WorkerGuard;

mod cli;
mod config;
mod discord;
mod http;
mod job_scheduler;
mod plugins;
mod utils;

use cli::Cli;
use config::Config;
use discord::{DiscordBotClientReceiver, DiscordBotClientSender};
use http::HttpClient;
use job_scheduler::JobScheduler;
use plugins::{
    AvailablePlugin, ConfigPlugin, PluginRegistrations, registry,
    runtime::{PluginBuilder, Runtime},
};

struct Shutdown {
    shutdown: AtomicBool,
    restart: AtomicBool,
    sigint: AtomicBool,
}

static SHUTDOWN: Shutdown = Shutdown {
    shutdown: AtomicBool::new(false),
    restart: AtomicBool::new(false),
    sigint: AtomicBool::new(false),
};

fn main() -> ExitCode {
    let _ = run();

    if SHUTDOWN.sigint.load(Ordering::Relaxed) && !SHUTDOWN.restart.load(Ordering::Relaxed) {
        info!("Exiting the bot");
        return match SHUTDOWN.sigint.load(Ordering::Relaxed) {
            true => ExitCode::from(130),
            false => ExitCode::from(0),
        };
    }

    restart();

    error!("Exiting the bot");
    ExitCode::from(1)
}

#[tokio::main]
async fn run() -> Result<(), ()> {
    let cli = Cli::parse();

    let _guard = initialization(
        cli.log_stdout_level,
        cli.log_stdout_ansi,
        cli.log_file_level,
        cli.log_file_ansi,
    )?;

    info!("Loading and parsing the config file");
    let config = Config::new(&cli.config_path)?;

    let available_plugins =
        registry_get_plugins(&config.plugins, &config.directory, config.cache).await?;

    let discord_bot_client_sender = Arc::new(DiscordBotClientSender::new().await?);

    let plugin_registrations = Arc::new(RwLock::new(PluginRegistrations::new()));

    let job_scheduler = JobScheduler::new(
        discord_bot_client_sender.clone(),
        plugin_registrations.clone(),
    )
    .await?;

    let runtime = job_scheduler.runtime.clone();

    info!("Creating the WASIp2 runtime");
    //let runtime = Arc::new(Runtime::new(discord_bot_client_sender.clone()));

    let (discord_bot_client_receiver, shard_count) = DiscordBotClientReceiver::new(
        &discord_bot_client_sender,
        runtime.clone(),
        plugin_registrations.clone(),
    )
    .await?;

    let mut tasks = Vec::with_capacity(shard_count);

    discord_bot_client_receiver.start(&mut tasks).await;

    job_scheduler.start().await?;

    plugin_initializations(
        &config.directory,
        runtime,
        plugin_registrations,
        available_plugins,
        discord_bot_client_sender.clone(),
        job_scheduler.clone(),
    )
    .await?;

    shutdown(job_scheduler, discord_bot_client_sender, tasks).await
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

async fn registry_get_plugins(
    config_plugins: &IndexMap<String, ConfigPlugin>,
    config_directory: &Path,
    config_cache: bool,
) -> Result<Vec<AvailablePlugin>, ()> {
    info!("Creating the http client");
    let http_client = HttpClient::new()?;

    info!("Fetching and locally storing the plugins");
    registry::get_plugins(&http_client, config_plugins, config_directory, config_cache).await
}

async fn plugin_initializations(
    config_directory: &Path,
    runtime: Arc<Runtime>,
    plugin_registrations: Arc<RwLock<PluginRegistrations>>,
    available_plugins: Vec<AvailablePlugin>,
    discord_bot_client_sender: Arc<DiscordBotClientSender>,
    job_scheduler: Arc<JobScheduler>,
) -> Result<(), ()> {
    info!("Creating the WASIp2 plugin builder");
    let plugin_builder = PluginBuilder::new();

    info!("Initializing the plugins");
    let initialized_plugin_registrations = Runtime::initialize_plugins(
        runtime.clone(),
        plugin_builder,
        plugin_registrations.clone(),
        available_plugins,
        config_directory,
    )
    .await?;

    info!("Making the Discord bot registrations (commands, scheduled jobs)");
    discord_bot_client_sender
        .command_registrations(
            plugin_registrations,
            initialized_plugin_registrations.commands,
        )
        .await?;

    job_scheduler
        .scheduled_job_registrations(initialized_plugin_registrations.scheduled_jobs)
        .await;

    Ok(())
}

async fn shutdown(
    job_scheduler: Arc<JobScheduler>,
    discord_bot_client_sender: Arc<DiscordBotClientSender>,
    tasks: Vec<JoinHandle<()>>,
) -> Result<(), ()> {
    tokio::spawn(async move {
        if let Err(err) = signal::ctrl_c().await {
            error!(
                "Failed to listen for the terminal interrupt signal, error: {}",
                &err
            );
            return;
        }

        info!("Terminal interrupt signal received, send another to force immediate shutdown");

        tokio::spawn(async {
            signal::ctrl_c()
                .await
                .expect("failed to listen for the terminal interrupt signal");

            warn!("Second terminal interrupt signal received, forcing immediate shutdown");
            exit(130);
        });

        SHUTDOWN.shutdown.store(true, Ordering::Relaxed);
        SHUTDOWN.sigint.store(true, Ordering::Relaxed);

        info!("Shutting down job scheduler");
        job_scheduler.shutdown().await;
        info!("Shutting down Discord bot client receiver shards");
        discord_bot_client_sender.shutdown().await;
    });

    for task in tasks {
        _ = task.await;
    }

    Ok(())
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
