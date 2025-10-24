pub mod builder;
pub mod registry;
pub mod runtime;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use simd_json::OwnedValue;

wasmtime::component::bindgen!();

#[derive(Deserialize)]
pub struct ConfigPlugin {
    pub plugin: String,
    pub environment: Option<HashMap<String, String>>,
    pub settings: Option<OwnedValue>,
}

pub struct AvailablePlugin {
    pub id: String,
    pub version: String,
    pub environment: Option<HashMap<String, String>>,
    pub settings: Option<OwnedValue>,
}

// TODO: Plugins which did not register anything should be dropped
#[derive(Serialize)]
pub struct PluginRegistrations {
    pub discord_events: PluginRegistrationsDiscordEvents,
    pub scheduled_jobs: HashMap<u128, (String, String)>, // UUID, plugin ID,
    // internal ID
    pub dependencies: HashMap<String, HashSet<String>>,
}

#[derive(Serialize)]
pub struct PluginRegistrationsDiscordEvents {
    pub interaction_create_commands: HashMap<String, (String, String)>, // ID (with ~x), plugin ID,
    // internal ID
    pub message_create: Vec<String>,
}

pub struct DiscordApplicationCommandRegistrationRequest {
    pub plugin_id: String,
    pub internal_id: String,
    pub command_data: Vec<u8>,
}

pub struct ScheduledJobRegistrationRequest {
    pub plugin_id: String,
    pub internal_id: String,
    pub crons: Vec<String>,
}

impl PluginRegistrations {
    pub fn new() -> Self {
        PluginRegistrations {
            discord_events: PluginRegistrationsDiscordEvents {
                interaction_create_commands: HashMap::new(),
                message_create: vec![],
            },
            scheduled_jobs: HashMap::new(),
            dependencies: HashMap::new(),
        }
    }
}
