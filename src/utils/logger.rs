use std::{fs, io};

use tracing::level_filters::LevelFilter;
use tracing_appender::{non_blocking::WorkerGuard, rolling::RollingFileAppender};
use tracing_subscriber::{Layer, Registry, fmt, layer::SubscriberExt};

use crate::cli::CliLogParameters;

pub fn new(cli_log_parameters: CliLogParameters) -> Result<Option<WorkerGuard>, ()> {
    if cli_log_parameters.stdout_level != LevelFilter::OFF {
        println!("Initializing the logger");
    }

    if cli_log_parameters.file_level != LevelFilter::OFF
        && !cli_log_parameters.file_directory.is_dir()
        && let Err(err) = fs::create_dir_all(&cli_log_parameters.file_directory)
    {
        eprintln!(
            "The provided logging directory does not exist and it could not be created, error: {err}"
        );
        return Err(());
    }

    if cli_log_parameters.stdout_level == LevelFilter::OFF {
        if cli_log_parameters.file_level == LevelFilter::OFF {
            Ok(None)
        } else {
            let p_rolling_file_appender = RollingFileAppender::builder()
                .rotation(cli_log_parameters.file_rotation.0)
                .filename_prefix(&cli_log_parameters.file_prefix)
                .filename_suffix(&cli_log_parameters.file_suffix)
                .max_log_files(cli_log_parameters.file_max)
                .build(&cli_log_parameters.file_directory);

            match p_rolling_file_appender {
                Ok(rolling_file_appender) => {
                    let (non_blocking, guard) =
                        tracing_appender::non_blocking(rolling_file_appender);

                    let subscriber = Registry::default().with(
                        fmt::Layer::default()
                            .with_writer(non_blocking)
                            .with_ansi(cli_log_parameters.file_ansi)
                            .with_filter(cli_log_parameters.file_level),
                    );

                    match tracing::subscriber::set_global_default(subscriber) {
                        Ok(()) => Ok(Some(guard)),
                        Err(err) => {
                            eprintln!("An error occurred while initializing the logger: {err}");
                            Err(())
                        }
                    }
                }
                Err(err) => {
                    eprintln!("An error occurred while initializing the logger: {err}");
                    Err(())
                }
            }
        }
    } else if cli_log_parameters.file_level == LevelFilter::OFF {
        let subscriber = Registry::default().with(
            fmt::Layer::default()
                .with_writer(io::stdout)
                .with_ansi(cli_log_parameters.stdout_ansi)
                .with_filter(cli_log_parameters.stdout_level),
        );

        match tracing::subscriber::set_global_default(subscriber) {
            Ok(()) => Ok(None),
            Err(err) => {
                eprintln!("An error occurred while initializing the logger: {err}");
                Err(())
            }
        }
    } else {
        let prolling_file_appender = RollingFileAppender::builder()
            .rotation(cli_log_parameters.file_rotation.0)
            .filename_prefix(&cli_log_parameters.file_prefix)
            .filename_suffix(&cli_log_parameters.file_suffix)
            .max_log_files(cli_log_parameters.file_max)
            .build(&cli_log_parameters.file_directory);

        match prolling_file_appender {
            Ok(rolling_file_appender) => {
                let (non_blocking, guard) = tracing_appender::non_blocking(rolling_file_appender);

                let subscriber = Registry::default()
                    .with(
                        fmt::Layer::default()
                            .with_writer(std::io::stdout)
                            .with_ansi(cli_log_parameters.stdout_ansi)
                            .with_filter(cli_log_parameters.stdout_level),
                    )
                    .with(
                        fmt::Layer::default()
                            .with_writer(non_blocking)
                            .with_ansi(cli_log_parameters.file_ansi)
                            .with_filter(cli_log_parameters.file_level),
                    );

                match tracing::subscriber::set_global_default(subscriber) {
                    Ok(()) => Ok(Some(guard)),
                    Err(err) => {
                        eprintln!("An error occurred while initializing the logger: {err}");
                        Err(())
                    }
                }
            }
            Err(err) => {
                eprintln!("An error occurred while initializing the logger: {err}");
                Err(())
            }
        }
    }
}
