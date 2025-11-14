use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::PathBuf,
    sync::Arc,
};

use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use crate::{config::Config, http::HttpClient, plugins::AvailablePlugin};

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(unused)]
pub struct Registry {
    pub name: String,
    pub description: String,
    pub maintainers: Vec<String>,
    pub plugins: BTreeMap<String, RegistryPlugin>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(unused)]
pub struct RegistryPlugin {
    pub versions: Vec<RegistryPluginVersion>,
    pub deprecated: Option<(bool, String)>,
    pub description: String,
    pub release_time: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RegistryPluginVersion {
    pub version: String,
    pub deprecated: Option<(bool, String)>,
    pub compatible_program_version: String,
}

type Registries = Arc<RwLock<HashMap<String, Option<Arc<Registry>>>>>;

const DEFAULT_REGISTRY_ID: &str = "celarye/discord-bot-plugins";

pub async fn get_plugins(
    http_client: Arc<HttpClient>,
    config: Box<Config>,
    base_plugin_directory: Arc<PathBuf>,
    cache: bool,
) -> Result<Vec<AvailablePlugin>, ()> {
    info!("Fetching and storing the plugins");

    let mut available_plugins = vec![];
    let registries = Arc::new(RwLock::new(HashMap::new()));

    let mut tasks = vec![];

    for (plugin_id, plugin_options) in config.plugins {
        let registries = registries.clone();

        let http_client = http_client.clone();
        let base_plugin_directory = base_plugin_directory.clone();

        tasks.push(tokio::spawn(async move {
            let (registry_id, plugin_registry_id, requested_version) =
                parse_plugin_string(&plugin_options.plugin);

            if !registries.read().await.contains_key(registry_id)
                && fetch_registry(&registries, registry_id, &plugin_id, http_client.clone())
                    .await
                    .is_err()
            {
                return Err(());
            }

            let Some(registry) = registries.read().await.get(registry_id).unwrap().clone() else {
                error!("The {plugin_id} plugin has an invalid registry defined: {registry_id}");
                return Err(());
            };

            let Some(plugin) = registry.plugins.get(&plugin_id) else {
                error!("The {registry_id} registry has an no {plugin_registry_id} plugin entry");
                return Err(());
            };

            let Some(version) =
                find_plugin_version_match(requested_version, &plugin.versions, &plugin_id)
            else {
                error!(
                    "The requested {requested_version} version of the {plugin_id} plugin is deprecated or can not be ran by this version of the program"
                );
                return Err(());
            };

            let plugin_base = plugin_registry_id + "/" + &version;
            let plugin_directory = base_plugin_directory.join(&plugin_base);

            if plugin_options.cache.unwrap_or(cache) && fs::exists(plugin_directory.join("plugin.wasm")).unwrap_or(false) {
                return Ok(AvailablePlugin {
                    id: plugin_id,
                    version,
                    permissions: plugin_options.permissions,
                    environment: plugin_options.environment,
                    settings: plugin_options.settings,
                });
            }

            let Ok(plugin_metadata) = http_client
                .get_file_from_registry(registry_id, &(plugin_base.clone() + "metadata.json"))
                .await
            else {
                return Err(());
            };

            if let Err(err) = fs::create_dir_all(&plugin_directory) {
                error!("An error occurred while creating the {plugin_id} plugin directory: {err}");
                return Err(());
            }

            if let Err(err) = fs::write(plugin_directory.join("metadata.json"), plugin_metadata) {
                error!(
                    "An error occurred while saving the metadata.json file for the {plugin_id} plugin: {err}"
                );
                return Err(());
            }

            let Ok(plugin_wasm) = http_client
                .get_file_from_registry(registry_id, &(plugin_base + "plugin.wasm"))
                .await
            else {
                return Err(());
            };

            if let Err(err) = fs::write(plugin_directory.join("plugin.wasm"), plugin_wasm) {
                error!("An error occurred while saving the plugin.wasm file: {err}");
                return Err(());
            }

            Ok(AvailablePlugin {
                id: plugin_id,
                version,
                permissions: plugin_options.permissions,
                environment: plugin_options.environment,
                settings: plugin_options.settings,
            })
        }));
    }

    for task in tasks.drain(..) {
        if let Ok(available_plugin) = task.await.unwrap() {
            available_plugins.push(available_plugin);
        }
    }

    Ok(available_plugins)
}

fn parse_plugin_string(value: &str) -> (&str, String, &str) {
    let (registry_id, plugin_registry_id_version) = match value.rsplit_once('/') {
        Some((registry_id, plugin_registry_id_version)) => {
            (registry_id, plugin_registry_id_version)
        }
        None => (DEFAULT_REGISTRY_ID, value),
    };

    let (plugin_registry_id, requested_version) = match plugin_registry_id_version.rsplit_once(':')
    {
        Some((plugin_registry_id, plugin_registry_version)) => {
            (plugin_registry_id, plugin_registry_version)
        }
        None => (plugin_registry_id_version, "latest"),
    };

    (
        registry_id,
        plugin_registry_id.to_string(),
        requested_version,
    )
}

async fn fetch_registry(
    registries: &Registries,
    registry_id: &str,
    plugin_id: &str,
    http_client: Arc<HttpClient>,
) -> Result<(), ()> {
    info!("Fetching the {registry_id} registry");

    if let Ok(mut registry_bytes) = http_client
        .get_file_from_registry(registry_id, "plugins.json")
        .await
    {
        match simd_json::from_slice::<Registry>(&mut registry_bytes) {
            Ok(registry) => {
                registries
                    .write()
                    .await
                    .insert(registry_id.to_string(), Some(Arc::new(registry)));
            }
            Err(err) => {
                registries
                    .write()
                    .await
                    .insert(registry_id.to_string(), None);

                error!(
                    "Failed to deserialize the registry plugins file JSON to a struct, error: {err}"
                );
                return Err(());
            }
        }
    } else {
        registries
            .write()
            .await
            .insert(registry_id.to_string(), None);

        error!("The {plugin_id} plugin has an invalid registry defined: {registry_id}");
        return Err(());
    }

    Ok(())
}

fn find_plugin_version_match(
    requested_version: &str,
    plugin_versions: &[RegistryPluginVersion],
    plugin_id: &str,
) -> Option<String> {
    if requested_version == "latest" {
        for plugin_version in plugin_versions.iter().rev() {
            if check_plugin_version_usability(plugin_version, plugin_id) {
                return Some(plugin_version.version.clone());
            }
        }
    } else if let Some(plugin_version) = plugin_versions
        .iter()
        .find(|v| v.version == requested_version)
        && check_plugin_version_usability(plugin_version, plugin_id)
    {
        return Some(plugin_version.version.clone());
    }

    None
}

fn check_plugin_version_usability(plugin_version: &RegistryPluginVersion, plugin_id: &str) -> bool {
    if let Some(deprecated) = plugin_version.deprecated.as_ref()
        && deprecated.0
    {
        debug!(
            "The {plugin_id} plugin version {} is marked as deprecated: {}",
            plugin_version.version, deprecated.1
        );
        return false;
    } else if plugin_version.compatible_program_version
        != env!("CARGO_PKG_VERSION")[..plugin_version.compatible_program_version.len()]
    {
        debug!(
            "The {plugin_id} plugin version {} is not compatible with this version of the program: {} != {}",
            plugin_version.version,
            plugin_version.compatible_program_version,
            &env!("CARGO_PKG_VERSION")[..plugin_version.compatible_program_version.len()]
        );
        return false;
    }

    true
}
