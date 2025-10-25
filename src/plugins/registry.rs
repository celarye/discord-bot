use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::PathBuf,
};

use serde::Deserialize;
use tracing::{error, info, warn};

use crate::{config::Config, http::HttpClient, plugins::AvailablePlugin};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Registry {
    pub name: String,
    pub description: String,
    pub maintainers: Vec<String>,
    pub tooling: RegistryTooling,
    pub plugins: BTreeMap<String, RegistryPlugin>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RegistryTooling {
    pub build_time: String,
    pub built_with: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RegistryPlugin {
    pub versions: Vec<RegistryPluginVersion>,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub deprecation_reason: String,
    pub description: String,
    pub release_time: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RegistryPluginVersion {
    pub version: String,
    pub compatible_bot_version: String,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub deprecation_reason: String,
}

pub async fn get_plugins(
    http_client: &HttpClient,
    config: &Config,
) -> Result<Vec<AvailablePlugin>, ()> {
    let mut registries: HashMap<String, Option<Registry>> = HashMap::new();
    let mut available_plugins = vec![];

    for (plugin_id, plugin_options) in &config.plugins {
        let (registry, name_version) = match plugin_options.plugin.rsplit_once('/') {
            Some((registry, name_version)) => (&registry.to_string(), &name_version.to_string()),
            None => (
                &String::from("celarye/discord-bot-plugins"),
                &plugin_options.plugin,
            ),
        };

        let (name, requested_version) = match name_version.rsplit_once(':') {
            Some((name, version)) => (&name.to_string(), &version.to_string()),
            None => (name_version, &String::from("latest")),
        };

        info!("Fetching the registry plugins list");

        let registry_plugins = match registries.get(registry) {
            Some(pregistry_plugins) => {
                if let Some(registry_plugins) = pregistry_plugins {
                    registry_plugins
                } else {
                    error!("Invalid registry: {}", &registry);
                    continue;
                }
            }
            None => {
                if let Ok(mut registry_plugins_bytes) = http_client
                    .get_file_from_registry(registry, &PathBuf::from("plugins.json"))
                    .await
                {
                    match simd_json::from_slice::<Registry>(&mut registry_plugins_bytes) {
                        Ok(registry_plugins) => {
                            registries.insert(registry.clone(), Some(registry_plugins));
                            registries.get(registry).unwrap().as_ref().unwrap()
                        }
                        Err(err) => {
                            error!(
                                "Failed to deserialize the registry plugins file JSON to a struct, error: {}",
                                &err
                            );
                            continue;
                        }
                    }
                } else {
                    error!("Invalid registry: {}", &registry);
                    continue;
                }
            }
        };

        let plugin_versions = if let Some(plugin) = registry_plugins.plugins.get(name) {
            &plugin.versions
        } else {
            error!("Plugin was not found in the registry, name: {}", &name);
            continue;
        };

        let version = if requested_version.as_str() == "latest" {
            let mut version = None;
            for plugin_version in plugin_versions.iter().rev() {
                if plugin_version.deprecated
                    || plugin_version.compatible_bot_version
                        != env!("CARGO_PKG_VERSION")[..plugin_version.compatible_bot_version.len()]
                {
                    continue;
                }
                version = Some(plugin_version.version.clone());
                break;
            }

            if version.is_none() {
                error!(
                    "No versions of the plugin are not deprecated or can be ran by this version of the bot"
                );
                continue;
            }

            version.unwrap()
        } else {
            let mut version = None;
            for plugin_version in plugin_versions {
                if &plugin_version.version != requested_version
                    || plugin_version.compatible_bot_version
                        != env!("CARGO_PKG_VERSION")[..plugin_version.compatible_bot_version.len()]
                {
                    continue;
                }
                if plugin_version.deprecated {
                    warn!(
                        "The requested version of the {} plugin is deprecated, consider looking into switching to a non deprecated version or to another plugin",
                        name
                    );
                }
                version = Some(plugin_version.version.clone());
                break;
            }

            if version.is_none() {
                continue;
            }

            version.unwrap()
        };

        let plugin_dir = config.directory.join(name).join(&version);

        if config.cache && fs::exists(plugin_dir.join("plugin.wasm")).unwrap_or(false) {
            available_plugins.push(AvailablePlugin {
                id: plugin_id.clone(),
                version: version.clone(),
                environment: plugin_options.environment.clone(),
                settings: plugin_options.settings.clone(),
            });
            continue;
        }

        let Ok(plugin_metadata) = http_client
            .get_file_from_registry(
                registry,
                &PathBuf::from(name).join(&version).join("metadata.json"),
            )
            .await
        else {
            continue;
        };

        if let Err(err) = fs::create_dir_all(config.directory.join(name).join(&version)) {
            error!(
                "Something went wrong while creating the plugin directory, error: {}",
                err
            );
            return Err(());
        }

        if let Err(err) = fs::write(
            config
                .directory
                .join(name)
                .join(&version)
                .join("metadata.json"),
            plugin_metadata,
        ) {
            error!(
                "Something went wrong while saving the metadata.json file, error: {}",
                err
            );
            return Err(());
        }

        let Ok(plugin_wasm) = http_client
            .get_file_from_registry(
                registry,
                &PathBuf::from(name).join(&version).join("plugin.wasm"),
            )
            .await
        else {
            continue;
        };

        if let Err(err) = fs::write(
            config
                .directory
                .join(name)
                .join(&version)
                .join("plugin.wasm"),
            plugin_wasm,
        ) {
            error!(
                "Something went wrong while saving the plugin.wasm file, error: {}",
                err
            );
            return Err(());
        }

        available_plugins.push(AvailablePlugin {
            id: plugin_id.clone(),
            version: version.clone(),
            environment: plugin_options.environment.clone(),
            settings: plugin_options.settings.clone(),
        });
    }

    Ok(available_plugins)
}
