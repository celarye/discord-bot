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
use tokio::{signal, sync::RwLock};
use tracing::{error, info, level_filters::LevelFilter, warn};
use tracing_appender::non_blocking::WorkerGuard;

mod channels;
mod cli;
mod config;
mod discord;
mod http;
mod job_scheduler;
mod plugins;
mod utils;

use cli::Cli;
use config::Config;
use discord::DiscordBotClient;
use http::HttpClient;
use job_scheduler::JobScheduler;
use plugins::{
    AvailablePlugin, PluginRegistrations, builder::PluginBuilder, registry, runtime::Runtime,
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

    if !SHUTDOWN.restart.load(Ordering::Relaxed) || SHUTDOWN.sigint.load(Ordering::Relaxed) {
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

    let available_plugins = registry_get_plugins(&config).await?;

    let plugin_registrations = Arc::new(RwLock::new(PluginRegistrations::new()));

    let channels = channels::new();

    info!("Creating the Discord bot client");
    let (discord_bot_client, shards) =
        DiscordBotClient::new(plugin_registrations.clone(), channels.2.0, channels.0.1).await?;

    info!("Creating the job scheduler");
    let job_scheduler =
        JobScheduler::new(plugin_registrations.clone(), channels.2.1, channels.1.1).await?;

    info!("Creating the WASI runtime");
    let runtime = Arc::new(Runtime::new(channels.0.0, channels.1.0, channels.2.2).await);

    discord_bot_client.start(shards).await;

    job_scheduler.start().await?;

    plugin_initializations(
        runtime.clone(),
        available_plugins,
        plugin_registrations,
        &config.directory,
    )
    .await?;

    Runtime::start(runtime.clone()).await;

    shutdown(runtime).await
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

async fn registry_get_plugins(config: &Config) -> Result<Vec<AvailablePlugin>, ()> {
    info!("Creating the http client");
    let http_client = HttpClient::new()?;

    info!("Fetching and locally storing the plugins");
    registry::get_plugins(&http_client, config).await
}

async fn plugin_initializations(
    runtime: Arc<Runtime>,
    available_plugins: Vec<AvailablePlugin>,
    plugin_registrations: Arc<RwLock<PluginRegistrations>>,
    config_directory: &Path,
) -> Result<(), ()> {
    info!("Creating the WASI plugin builder");
    let plugin_builder = PluginBuilder::new();

    info!("Initializing the plugins");
    Runtime::initialize_plugins(
        runtime,
        plugin_builder,
        available_plugins,
        plugin_registrations,
        config_directory,
    )
    .await
}

async fn shutdown(runtime: Arc<Runtime>) -> Result<(), ()> {
    let cancellation_token = runtime.cancellation_token.clone();

    tokio::select! {
            result = async move {
            if let Err(err) = signal::ctrl_c().await {
                error!(
                    "Failed to listen for the terminal interrupt signal, error: {}",
                    &err
                );
                return Err(());
            }

            info!("Terminal interrupt signal received, send another to force immediate shutdown");

            tokio::spawn(async {
                signal::ctrl_c()
                    .await
                    .expect("failed to listen for the terminal interrupt signal");

                warn!("Second terminal interrupt signal received, forcing immediate shutdown");
                exit(130);
            });

            SHUTDOWN.sigint.store(true, Ordering::Relaxed);

            runtime.shutdown(false).await;

            Ok(())

        } => {result}
        _ = cancellation_token.cancelled() => {Ok(())}
    }
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
