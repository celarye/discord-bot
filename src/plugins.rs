pub mod builder;
pub mod registry;
pub mod runtime;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Deserializer, Serialize};
use simd_json::OwnedValue;
use twilight_model::id::{Id, marker::CommandMarker};

use crate::plugins::discord_bot::plugin::plugin_types::SupportedRegistrations;

wasmtime::component::bindgen!({ imports: { default: async }, exports: { default: async } });

#[derive(Deserialize)]
pub struct ConfigPlugin {
    pub plugin: String,
    pub cache: Option<bool>,
    #[serde(default = "ConfigPlugin::permissions_default")]
    pub permissions: SupportedRegistrations,
    pub environment: Option<HashMap<String, String>>,
    pub settings: Option<OwnedValue>,
}

impl ConfigPlugin {
    fn permissions_default() -> SupportedRegistrations {
        SupportedRegistrations::all()
    }
}

impl<'de> Deserialize<'de> for SupportedRegistrations {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut supported_registrations = SupportedRegistrations::empty();

        let supported_registration_strings = Vec::<String>::deserialize(deserializer)?;

        for supported_registration_string in supported_registration_strings {
            match supported_registration_string.to_uppercase().as_str() {
                "DEPENDENCY_FUNCTIONS" => {
                    supported_registrations |= SupportedRegistrations::DEPENDENCY_FUNCTIONS;
                }
                "DISCORD_EVENT_MESSAGE_CREATE" => {
                    supported_registrations |= SupportedRegistrations::DISCORD_EVENT_MESSAGE_CREATE;
                }
                "DISCORD_EVENT_INTERACTION_CREATE" => {
                    supported_registrations |=
                        SupportedRegistrations::DISCORD_EVENT_INTERACTION_CREATE;
                }
                "DISCORD_EVENT_THREAD_CREATE" => {
                    supported_registrations |= SupportedRegistrations::DISCORD_EVENT_THREAD_CREATE;
                }
                "DISCORD_EVENT_THREAD_DELETE" => {
                    supported_registrations |= SupportedRegistrations::DISCORD_EVENT_THREAD_DELETE;
                }
                "DISCORD_EVENT_THREAD_LIST_SYNC" => {
                    supported_registrations |=
                        SupportedRegistrations::DISCORD_EVENT_THREAD_LIST_SYNC;
                }
                "DISCORD_EVENT_THREAD_MEMBER_UPDATE" => {
                    supported_registrations |=
                        SupportedRegistrations::DISCORD_EVENT_THREAD_MEMBER_UPDATE;
                }
                "DISCORD_EVENT_THREAD_MEMBERS_UPDATE" => {
                    supported_registrations |=
                        SupportedRegistrations::DISCORD_EVENT_THREAD_MEMBERS_UPDATE;
                }
                "DISCORD_EVENT_THREAD_UPDATE" => {
                    supported_registrations |= SupportedRegistrations::DISCORD_EVENT_THREAD_UPDATE;
                }
                "SHUTDOWN" => {
                    supported_registrations |= SupportedRegistrations::SHUTDOWN;
                }
                &_ => unimplemented!(),
            }
        }

        Ok(supported_registrations)
    }
}

pub struct AvailablePlugin {
    pub id: String,
    pub version: String,
    pub permissions: SupportedRegistrations,
    pub environment: Option<HashMap<String, String>>,
    pub settings: Option<OwnedValue>,
}

// TODO: Plugins which did not register anything should get dropped
#[derive(Serialize)]
pub struct PluginRegistrations {
    pub discord_events: PluginRegistrationsDiscordEvents,
    pub scheduled_jobs: HashMap<u128, (String, String)>, // UUID, plugin ID, internal ID
    pub dependency_functions: HashMap<String, HashSet<String>>,
}

#[derive(Serialize)]
pub struct PluginRegistrationsDiscordEvents {
    pub interaction_create: PluginRegistrationsInteractionCreate,
    pub message_create: Vec<String>,
    pub thread_create: Vec<String>,
    pub thread_delete: Vec<String>,
    pub thread_list_sync: Vec<String>,
    pub thread_member_update: Vec<String>,
    pub thread_members_update: Vec<String>,
    pub thread_update: Vec<String>,
}

#[derive(Serialize)]
pub struct PluginRegistrationsInteractionCreate {
    pub application_commands: HashMap<Id<CommandMarker>, (String, String)>, // Command ID, plugin ID, internal ID
    pub message_components: HashMap<String, String>, // Message Component ID, plugin ID ISSUE: ID overlap is possible
    pub modals: HashMap<String, String>, // Modal ID, plugin ID ISSUE: ID overlap is possible
}

pub struct PluginRegistrationRequests {
    pub discord_event_interaction_create: PluginRegistrationRequestsInteractionCreate,
    pub scheduled_jobs: Vec<PluginRegistrationRequestsScheduledJob>,
}

pub struct PluginRegistrationRequestsInteractionCreate {
    pub application_commands: Vec<PluginRegistrationRequestsApplicationCommand>,
    #[allow(unused)] // Will be used when wasmtime provides component information on host calls
    pub message_component: Vec<PluginRegistrationRequestsMessageComponent>,
    #[allow(unused)] // Will be used when wasmtime provides component information on host calls
    pub modals: Vec<PluginRegistrationRequestsModal>,
}

pub struct PluginRegistrationRequestsApplicationCommand {
    pub plugin_id: String,
    pub id: String,
    pub data: Vec<u8>,
}

#[allow(unused)] // Will be used when wasmtime provides component information on host calls
pub struct PluginRegistrationRequestsMessageComponent {
    pub plugin_id: String,
    pub id: String,
}

#[allow(unused)] // Will be used when wasmtime provides component information on host calls
pub struct PluginRegistrationRequestsModal {
    pub plugin_id: String,
    pub id: String,
}

pub struct PluginRegistrationRequestsScheduledJob {
    pub plugin_id: String,
    pub id: String,
    pub crons: Vec<String>,
}

impl PluginRegistrations {
    pub fn new() -> Self {
        PluginRegistrations {
            discord_events: PluginRegistrationsDiscordEvents {
                interaction_create: PluginRegistrationsInteractionCreate {
                    application_commands: HashMap::new(),
                    message_components: HashMap::new(),
                    modals: HashMap::new(),
                },
                message_create: vec![],
                thread_create: vec![],
                thread_delete: vec![],
                thread_list_sync: vec![],
                thread_member_update: vec![],
                thread_members_update: vec![],
                thread_update: vec![],
            },
            scheduled_jobs: HashMap::new(),
            dependency_functions: HashMap::new(),
        }
    }
}
