use std::path::PathBuf;

use clap::{ArgAction, Parser};
use tracing::level_filters::LevelFilter;

#[derive(Parser)]
#[command(about, long_about = None, version, author)]
pub struct Cli {
    #[arg(default_value = "INFO", short, long, value_name = "LEVEL", help = "The level at which the bot should log to stdout", long_help = None)]
    pub log_stdout_level: LevelFilter,

    #[arg(action=ArgAction::Set, default_value_t = true, short = 'a', long, value_name = "BOOL", help = "Enable ANSI escape code for the output of the stdout logger", long_help = None, hide_possible_values = true)]
    pub log_stdout_ansi: bool,

    #[arg(default_value = "INFO", short = 'L', long, value_name = "LEVEL", help = "The level at which the bot should log to a file", long_help = None)]
    pub log_file_level: LevelFilter,

    #[arg(action=ArgAction::Set, default_value_t = false, short = 'A', long, value_name = "BOOL", help = "Enable ANSI escape code for the output of the file logger", long_help = None, hide_possible_values = true)]
    pub log_file_ansi: bool,

    #[arg(default_value = "./config.yaml", short, long, value_name = "FILE PATH", help = "The path to the bot its configuration file", long_help = None)]
    pub config_path: PathBuf,
}
