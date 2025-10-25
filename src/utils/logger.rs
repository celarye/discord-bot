// TODO: Make the file logger configurable
use tracing::level_filters::LevelFilter;
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{Layer, Registry, fmt, layer::SubscriberExt};

pub fn new(
    log_stdout_level: LevelFilter,
    log_stdout_ansi: bool,
    log_file_level: LevelFilter,
    log_file_ansi: bool,
) -> Result<Option<WorkerGuard>, ()> {
    if log_stdout_level != LevelFilter::OFF {
        println!("Initializing the logger");
    }

    match log_stdout_level {
        LevelFilter::OFF => {
            if log_file_level == LevelFilter::OFF {
                Ok(None)
            } else {
                let p_rolling_file_appender = RollingFileAppender::builder()
                    .rotation(Rotation::DAILY)
                    .filename_prefix("discord-bot")
                    .filename_suffix("log")
                    .max_log_files(7)
                    .build("logs");

                match p_rolling_file_appender {
                    Ok(rolling_file_appender) => {
                        let (non_blocking, guard) =
                            tracing_appender::non_blocking(rolling_file_appender);

                        let subscriber = Registry::default().with(
                            fmt::Layer::default()
                                .with_writer(non_blocking)
                                .with_ansi(log_stdout_ansi)
                                .with_filter(log_file_level),
                        );

                        match tracing::subscriber::set_global_default(subscriber) {
                            Ok(()) => Ok(Some(guard)),
                            Err(err) => {
                                eprintln!(
                                    "An error occurred while initializing the logger: {}",
                                    &err
                                );
                                Err(())
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("An error occurred while initializing the logger: {}", &err);
                        Err(())
                    }
                }
            }
        }
        _ => {
            if log_file_level == LevelFilter::OFF {
                let subscriber = Registry::default().with(
                    fmt::Layer::default()
                        .with_writer(std::io::stdout)
                        .with_ansi(log_stdout_ansi)
                        .with_filter(log_stdout_level),
                );

                match tracing::subscriber::set_global_default(subscriber) {
                    Ok(()) => Ok(None),
                    Err(err) => {
                        eprintln!("An error occurred while initializing the logger: {}", &err);
                        Err(())
                    }
                }
            } else {
                let prolling_file_appender = RollingFileAppender::builder()
                    .rotation(Rotation::DAILY)
                    .filename_prefix("discord-bot")
                    .filename_suffix("log")
                    .max_log_files(7)
                    .build("logs");

                match prolling_file_appender {
                    Ok(rolling_file_appender) => {
                        let (non_blocking, guard) =
                            tracing_appender::non_blocking(rolling_file_appender);

                        let subscriber = Registry::default()
                            .with(
                                fmt::Layer::default()
                                    .with_writer(std::io::stdout)
                                    .with_ansi(log_stdout_ansi)
                                    .with_filter(log_stdout_level),
                            )
                            .with(
                                fmt::Layer::default()
                                    .with_writer(non_blocking)
                                    .with_ansi(log_file_ansi)
                                    .with_filter(log_file_level),
                            );

                        match tracing::subscriber::set_global_default(subscriber) {
                            Ok(()) => Ok(Some(guard)),
                            Err(err) => {
                                eprintln!(
                                    "An error occurred while initializing the logger: {}",
                                    &err
                                );
                                Err(())
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("An error occurred while initializing the logger: {}", &err);
                        Err(())
                    }
                }
            }
        }
    }
}
