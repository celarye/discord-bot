use std::{
    collections::VecDeque,
    env,
    ffi::OsString,
    os::unix::process::CommandExt,
    process::{Command, ExitCode},
};

use clap::Parser;
use tracing::{error, info, level_filters::LevelFilter};
use tracing_appender::non_blocking::WorkerGuard;

mod cli;
use cli::Cli;
mod job_scheduler;
use job_scheduler::JobScheduler;
mod http;
use http::HttpClient;
mod discord;
use discord::DiscordBotClient;
mod config;
use config::Config;
mod plugins;
use plugins::{Runtime, registry};

mod utils;

fn main() -> ExitCode {
    if let Ok(should_restart) = run() {
        if !should_restart {
            info!("Exiting the bot");
            return ExitCode::from(0);
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
    let runtime = Runtime::new();

    info!("Initializing the plugins");
    let (initialized_plugins, initialized_plugins_registrations) = runtime
        .lock()
        .await
        .initialize_plugins(&available_plugins, &config.directory)
        .await?;

    info!("Creating the Discord bot client");
    let (discord_client, data) = DiscordBotClient::new(
        runtime.clone(),
        initialized_plugins.clone(),
        initialized_plugins_registrations,
    )
    .await?;

    // TODO: Shared Data and restart and stop support
    info!("Creating the job scheduler");
    let job_scheduler =
        JobScheduler::new(runtime.clone(), initialized_plugins.clone(), data.clone()).await?;

    info!("Starting the job scheduler");
    job_scheduler.start().await?;

    info!("Starting the Discord bot client");
    discord_client.start().await;

    Ok(data.read().await.restart)
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

fn restart() {
    let bot_executable_path = match env::current_exe() {
        Ok(bot_executable_path) => bot_executable_path,
        Err(err) => {
            error!(
                "Failed to get the current bot executable its path, error: {}",
                &err
            );
            return;
        }
    };

    let mut args: VecDeque<OsString> = env::args_os().collect();

    args.pop_front();

    info!("Restarting the bot");
    let err = Command::new(bot_executable_path).args(args).exec();

    error!("Failed to start a new instance of the bot, error: {}", &err);
}
