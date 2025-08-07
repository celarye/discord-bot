pub mod runtime;
pub use runtime::Runtime;
use serde::{Deserialize, Serialize};
pub mod registry;

use std::collections::{HashMap, HashSet};

use simd_json::OwnedValue;

#[derive(Clone, Debug)]
pub struct AvailablePlugin {
    pub name: String,
    pub version: String,
    pub environment: Option<HashMap<String, String>>,
    pub settings: Option<OwnedValue>,
}

#[derive(Clone, Debug, Serialize)]
#[allow(dead_code)]
pub struct InitializedPlugins {
    pub discord_events: InitializedPluginsDiscordEvents,
    pub scheduled_jobs: HashMap<String, Vec<(String, String)>>,
    pub dependencies: HashMap<String, HashSet<String>>,
}

#[derive(Clone, Debug, Serialize)]
#[allow(dead_code)]
pub struct InitializedPluginsDiscordEvents {
    pub interaction_create_commands: HashMap<String, String>,
    pub message_create: Vec<String>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct InitializedPluginRegistrations {
    pub commands: Vec<InitializedPluginRegistrationsCommand>,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
pub struct InitializedPluginRegistrationsCommand {
    pub plugin_name: String,
    pub command_data: Vec<u8>,
}
