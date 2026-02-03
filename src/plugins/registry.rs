/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Copyright © 2026 Eduard Smet */

// TODO: Support faster cache fallback and prevent the need for a registry fetch

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::Deserialize;
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

const DEFAULT_REGISTRY_ID: &str = "celarye/discord-bot-plugins";

pub async fn get_plugins(
    http_client: Arc<HttpClient>,
    config: Box<Config>,
    base_plugin_directory: PathBuf,
    cache: bool,
) -> Result<Vec<AvailablePlugin>, ()> {
    info!("Fetching and storing the plugins");

    let mut available_plugins = vec![];

    let mut registries = HashMap::new();

    registries.insert(DEFAULT_REGISTRY_ID.to_string(), vec![]);

    for (plugin_id, mut plugin_options) in config.plugins {
        if let Some(registry) = parse_plugin_string_registry(&mut plugin_options.plugin) {
            registries
                .entry(registry)
                .or_insert(vec![])
                .push((plugin_id, plugin_options));
        } else {
            registries
                .get_mut(DEFAULT_REGISTRY_ID)
                .unwrap()
                .push((plugin_id, plugin_options));
        }
    }

    let mut registry_tasks = vec![];

    for (registry_id, plugins) in registries {
        let registry_id = match registry_id.as_str() {
            DEFAULT_REGISTRY_ID => Arc::new(None),
            &_ => Arc::new(Some(registry_id)),
        };

        let http_client = http_client.clone();
        let base_plugin_directory = base_plugin_directory.clone();
        let registry_id = registry_id.clone();

        registry_tasks.push(tokio::spawn(async move {
            let mut available_registry_plugins = vec![];

            let registry = match fetch_registry(&registry_id, http_client.clone()).await {
                Ok(registry) => Arc::new(registry),
                Err(()) => return  Err(()),
            };

            let mut plugin_tasks = vec![];

            for (plugin_id, mut plugin_options) in plugins {
                let http_client = http_client.clone();
                let mut plugin_directory = base_plugin_directory.clone();
                let registry_id = registry_id.clone();
                let registry = registry.clone();

                plugin_tasks.push(tokio::spawn(async move {
                    let plugin_requested_version = parse_plugin_string_name_version(&mut plugin_options.plugin);

                    let Some(registry_plugin) = registry.plugins.get(&plugin_options.plugin) else {
                        error!("The {} registry has no {} plugin entry", registry_id.as_deref().unwrap_or(DEFAULT_REGISTRY_ID), plugin_options.plugin);
                        return Err(());
                    };

                    let Some(version) =
                        find_plugin_version_match(&plugin_requested_version, &registry_plugin.versions, &plugin_id)
                    else {
                        error!(
                            "The requested {plugin_requested_version} version of the {plugin_id} plugin is deprecated or can not be ran by this version of the program"
                        );
                        return Err(());
                    };

                    plugin_directory.push(&plugin_id);
                    plugin_directory.push(&version);

                    let plugin_registry_path = plugin_options.plugin + "/" + &version + "/";

                    if plugin_options.cache.unwrap_or(cache) && fs::exists(plugin_directory.join("plugin.wasm")).unwrap_or(false) {
                        info!("Using the cached version of the {plugin_id} plugin");

                        return Ok(AvailablePlugin {
                            id: plugin_id,
                            version,
                            permissions: plugin_options.permissions,
                            environment: plugin_options.environment,
                            settings: plugin_options.settings,
                        });
                    }

                    fetch_plugin(&http_client, &registry_id, &plugin_registry_path, &plugin_directory, &plugin_id).await?;

                    Ok(AvailablePlugin {
                        id: plugin_id,
                        version,
                        permissions: plugin_options.permissions,
                        environment: plugin_options.environment,
                        settings: plugin_options.settings,
                    })

                }));
            }

            for plugin_task in plugin_tasks.drain(..) {
                if let Ok(available_plugin) = plugin_task.await.unwrap() {
                    available_registry_plugins.push(available_plugin);
                }
            }

            Ok(available_registry_plugins)
        }));
    }

    for registry_task in registry_tasks.drain(..) {
        if let Ok(mut available_registry_plugins) = registry_task.await.unwrap() {
            available_plugins.append(&mut available_registry_plugins);
        }
    }

    Ok(available_plugins)
}

fn parse_plugin_string_registry(value: &mut String) -> Option<String> {
    let (registry, plugin_name_version) = value.rsplit_once('/')?;

    let registry = registry.to_string();

    *value = plugin_name_version.to_string();

    Some(registry)
}

async fn fetch_registry(
    registry_id: &Arc<Option<String>>,
    http_client: Arc<HttpClient>,
) -> Result<Registry, ()> {
    info!(
        "Fetching the {} registry",
        registry_id.as_deref().unwrap_or(DEFAULT_REGISTRY_ID)
    );

    if let Ok(registry_bytes) = http_client
        .get_file_from_registry(registry_id, "plugins.json")
        .await
    {
        match sonic_rs::from_slice::<Registry>(&registry_bytes) {
            Ok(registry) => Ok(registry),
            Err(err) => {
                error!(
                    "Failed to deserialize the registry plugins file JSON to a struct, error: {err}"
                );
                Err(())
            }
        }
    } else {
        error!(
            "The {} registry is invalid",
            registry_id.as_deref().unwrap_or(DEFAULT_REGISTRY_ID)
        );
        Err(())
    }
}

fn parse_plugin_string_name_version(value: &mut String) -> String {
    match value.rsplit_once(':') {
        Some((plugin_registry_id, plugin_requested_version)) => {
            let plugin_requested_version = plugin_requested_version.to_string();

            *value = plugin_registry_id.to_string();

            plugin_requested_version
        }
        None => String::from("latest"),
    }
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

async fn fetch_plugin(
    http_client: &Arc<HttpClient>,
    registry_id: &Arc<Option<String>>,
    plugin_registry_path: &str,
    plugin_directory: &Path,
    plugin_id: &str,
) -> Result<(), ()> {
    info!("Fetching the {plugin_id} plugin from its registry");

    let plugin_metadata = http_client
        .get_file_from_registry(
            registry_id,
            &(plugin_registry_path.to_string() + "metadata.json"),
        )
        .await?;

    if let Err(err) = fs::create_dir_all(plugin_directory) {
        error!("An error occurred while creating the {plugin_id} plugin directory: {err}");
        return Err(());
    }

    if let Err(err) = fs::write(plugin_directory.join("metadata.json"), plugin_metadata) {
        error!(
            "An error occurred while saving the metadata.json file for the {plugin_id} plugin: {err}"
        );
        return Err(());
    }

    let plugin_wasm = http_client
        .get_file_from_registry(
            registry_id,
            &(plugin_registry_path.to_string() + "plugin.wasm"),
        )
        .await?;

    if let Err(err) = fs::write(plugin_directory.join("plugin.wasm"), plugin_wasm) {
        error!("An error occurred while saving the plugin.wasm file: {err}");
        return Err(());
    }

    Ok(())
}
