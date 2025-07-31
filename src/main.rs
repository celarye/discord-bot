use std::{
    collections::VecDeque,
    env,
    ffi::OsString,
    os::unix::process::CommandExt,
    process::{Command, ExitCode},
};

use clap::Parser;
use tokio::signal;
use tracing::{debug, error, info, level_filters::LevelFilter};
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
        .initialize_plugins(available_plugins, &config.directory)
        .await?;

    // TODO: Add simple multi shard support: https://github.com/twilight-rs/twilight/tree/main/twilight-gateway#example
    info!("Creating the Discord bot client");
    let (mut discord_bot_client, discord_bot_shards, data) =
        DiscordBotClient::new(runtime.clone(), initialized_plugins).await?;

    info!("Making the Discord bot registrations (commands, scheduled jobs)");
    discord_bot_client
        .registrations(initialized_plugins_registrations)
        .await?;

    info!("Starting the job scheduler");
    discord_bot_client.start_job_scheduler().await?;

    let mut tasks = Vec::with_capacity(discord_bot_shards.len());

    info!("Starting the Discord bot client shards");
    discord_bot_client
        .start(&mut tasks, discord_bot_shards)
        .await;

    // FIXME: Send close signals and shut down the job scheduler
    // TODO: Move this to the background to add support for other shut down signals
    if let Err(err) = signal::ctrl_c().await {
        error!(
            "Something went wrong while receiving the Ctrl C signal, error: {}",
            &err
        );
        return Err(());
    }

    for join_handle in tasks {
        _ = join_handle.await;
    }

    Ok(*data.restart.read().await)
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
    let executable_path = match env::current_exe() {
        Ok(executable_path) => executable_path,
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
    let err = Command::new(executable_path).args(args).exec();

    error!("Failed to start a new instance of the bot, error: {}", &err);
}
