use std::{
    collections::VecDeque,
    env,
    ffi::OsString,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, ExitCode, exit},
    sync::{Arc, LazyLock},
};

use clap::Parser;
use tokio::{signal, sync::RwLock};
use tracing::{error, info, warn};
use tracing_appender::non_blocking::WorkerGuard;

mod cli;
mod config;
mod discord;
mod http;
mod job_scheduler;
mod plugins;
mod utils;

use cli::{Cli, CliLogParameters};
use config::Config;
use discord::DiscordBotClient;
use http::HttpClient;
use job_scheduler::JobScheduler;
use plugins::{
    AvailablePlugin, PluginRegistrations, builder::PluginBuilder, registry, runtime::Runtime,
};
use utils::channels;

#[derive(PartialEq)]
enum Shutdown {
    Normal,
    SigInt,
    Restart,
}

static SHUTDOWN: LazyLock<RwLock<Option<Shutdown>>> = LazyLock::new(|| RwLock::new(None));

#[tokio::main]
async fn main() -> ExitCode {
    let _ = run().await;

    if SHUTDOWN.read().await.as_ref().unwrap() != &Shutdown::Restart {
        info!("Exiting the bot");
        return if SHUTDOWN.read().await.as_ref().unwrap() == &Shutdown::SigInt {
            ExitCode::from(130)
        } else {
            ExitCode::from(0)
        };
    }

    restart();

    error!("Exiting the bot");
    ExitCode::from(1)
}

async fn run() -> Result<(), ()> {
    let cli = Cli::parse();
    //let mut tasks: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::new(Mutex::new(vec![])); // TODO: Rework shutdown

    let (_guard, discord_bot_client_token) = initialization(cli.log_parameters, &cli.env_file)?;

    let config = Config::new(&cli.config_file)?;

    let plugin_directory = Arc::new(cli.plugin_directory);

    let available_plugins = registry_get_plugins(
        cli.http_client_timeout_seconds,
        config,
        plugin_directory.clone(),
        cli.cache,
    )
    .await?;

    let plugin_registrations = Arc::new(RwLock::new(PluginRegistrations::new()));

    let channels = channels::new();

    info!("Creating the Discord bot client");
    let (discord_bot_client, shards) = DiscordBotClient::new(
        discord_bot_client_token,
        plugin_registrations.clone(),
        channels.runtime.discord_bot_client_sender,
        channels.discord_bot_client.receiver,
    )
    .await?;

    info!("Creating the job scheduler");
    let job_scheduler = JobScheduler::new(
        plugin_registrations.clone(),
        channels.runtime.job_scheduler_sender,
        channels.job_scheduler.receiver,
    )
    .await?;

    info!("Creating the WASI runtime");
    let runtime = Arc::new(Runtime::new(
        channels.discord_bot_client.sender,
        channels.job_scheduler.sender,
        channels.runtime.receiver,
    ));

    discord_bot_client.start(shards);

    job_scheduler.start().await?;

    plugin_initializations(
        runtime.clone(),
        available_plugins,
        plugin_registrations,
        plugin_directory,
    )
    .await?;

    Runtime::start(runtime.clone());

    shutdown(runtime).await
}

fn initialization(
    cli_log_parameters: CliLogParameters,
    env_file: &Path,
) -> Result<(Option<WorkerGuard>, String), ()> {
    let guard = utils::logger::new(cli_log_parameters)?;

    utils::env::load_env_file(env_file)?;

    let discord_bot_client_token = utils::env::validate()?;

    Ok((guard, discord_bot_client_token))
}

async fn registry_get_plugins(
    http_client_timeout_seconds: u64,
    config: Box<Config>,
    plugin_directory: Arc<PathBuf>,
    cache: bool,
) -> Result<Vec<AvailablePlugin>, ()> {
    let http_client = Arc::new(HttpClient::new(http_client_timeout_seconds)?);

    registry::get_plugins(http_client, config, plugin_directory, cache).await
}

async fn plugin_initializations(
    runtime: Arc<Runtime>,
    available_plugins: Vec<AvailablePlugin>,
    plugin_registrations: Arc<RwLock<PluginRegistrations>>,
    config_directory: Arc<PathBuf>,
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

            runtime.shutdown(Shutdown::SigInt).await;

            Ok(())

        } => {result}
        () = cancellation_token.cancelled() => {Ok(())}
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
