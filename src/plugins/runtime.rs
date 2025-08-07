use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use tokio::sync::Mutex;
use tracing::error;
use wasmtime::{Config, Engine, Store, component::Linker};
use wasmtime_wasi::{
    DirPerms, FilePerms, ResourceTable,
    p2::{IoView, WasiCtx, WasiCtxBuilder, WasiView},
};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

use super::{AvailablePlugin, InitializedPlugins};
use crate::plugins::{
    InitializedPluginRegistrations, InitializedPluginRegistrationsCommand,
    InitializedPluginsDiscordEvents,
    runtime::{
        discord_bot::plugin::discord_types::{
            Events as DiscordEvents, Requests as DiscordRequests,
        },
        exports::discord_bot::plugin::plugin_types::SupportedRegistrations,
    },
};

wasmtime::component::bindgen!({ async: true});

pub struct RuntimeBuilder {
    engine: Engine,
    linker: Linker<InternalRuntime>,
}

pub struct Runtime {
    plugins: HashMap<String, RuntimePlugin>,
}

pub struct RuntimePlugin {
    instance: Plugin,
    store: Mutex<Store<InternalRuntime>>,
}

struct InternalRuntime {
    ctx: WasiCtx,
    http: WasiHttpCtx,
    table: ResourceTable,
}

impl WasiView for InternalRuntime {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
}

impl WasiHttpView for InternalRuntime {
    fn ctx(&mut self) -> &mut WasiHttpCtx {
        &mut self.http
    }
}

impl IoView for InternalRuntime {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

impl RuntimeBuilder {
    pub fn new() -> Self {
        let mut config = Config::new();
        config.async_support(true);
        // TODO: Create wasmtime epoch interuption, would maybe prevent things like permantent TCP listeners to work?

        let engine = wasmtime::Engine::new(&config).unwrap();

        // TODO: Configure the linker, here host exports can be defined (still need to manually define some functions, see the WIT file)
        // Maybe there is a better way to link dependency plugins here
        // Maybe there is a better way to add logging support here
        let mut linker = wasmtime::component::Linker::<InternalRuntime>::new(&engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker).unwrap();
        wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker).unwrap();

        RuntimeBuilder { engine, linker }
    }

    pub async fn initialize_plugins(
        &self,
        plugins: Vec<AvailablePlugin>,
        directory: &Path,
    ) -> Result<(Runtime, InitializedPlugins, InitializedPluginRegistrations), ()> {
        let mut runtime = Runtime {
            plugins: HashMap::new(),
        };

        let mut initialized_plugins = InitializedPlugins {
            discord_events: InitializedPluginsDiscordEvents {
                interaction_create_commands: HashMap::new(),
                message_create: vec![],
            },
            scheduled_jobs: HashMap::new(),
            dependencies: HashMap::new(),
        };

        let mut initialized_plugins_registrations =
            InitializedPluginRegistrations { commands: vec![] };

        let supported_registrations = SupportedRegistrations {
            interaction_create_commands: true,
            message_create: true,
        };

        for plugin in plugins {
            let plugin_dir = directory.join(&plugin.name).join(&plugin.version);

            let bytes = std::fs::read(plugin_dir.join("plugin.wasm")).unwrap();
            let component = match wasmtime::component::Component::new(&self.engine, bytes) {
                Ok(component) => component,
                Err(err) => {
                    error!(
                        "Something went wrong while creating a WASM component from the {} plugin, error: {}",
                        &plugin.name, &err
                    );

                    continue;
                }
            };

            let env_hm = plugin.environment.clone().unwrap_or(HashMap::new());

            let env: Box<[(&str, &str)]> = env_hm
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();

            let workspace_plugin_dir = plugin_dir.join("workspace");

            match fs::exists(&workspace_plugin_dir) {
                Ok(exists) => {
                    if !exists {
                        if let Err(err) = fs::create_dir(&workspace_plugin_dir) {
                            error!(
                                "Something went wrong while creating the workspace directory for the {} plugin, error: {}",
                                &plugin.name, &err
                            );
                        }
                    }
                }
                Err(err) => {
                    error!(
                        "Something went wrong while checking if the workspace directory of the {} plugin exists, error: {}",
                        &plugin.name, &err
                    );
                    return Err(());
                }
            }

            let ctx = WasiCtxBuilder::new()
                .envs(&env)
                .preopened_dir(workspace_plugin_dir, "/", DirPerms::all(), FilePerms::all())
                .unwrap()
                .build();

            let mut store = wasmtime::Store::<InternalRuntime>::new(
                &self.engine,
                InternalRuntime {
                    ctx,
                    http: WasiHttpCtx::new(),
                    table: ResourceTable::new(),
                },
            );

            // Validate and instantiate the component
            let instance =
                match Plugin::instantiate_async(&mut store, &component, &self.linker).await {
                    Ok(instance) => instance,
                    Err(err) => {
                        error!(
                            "Failed to instantiate the {} plugin, error: {}",
                            &plugin.name, &err
                        );
                        continue;
                    }
                };

            let registrations_response = match instance
                .discord_bot_plugin_plugin_functions()
                .call_registrations(
                    &mut store,
                    &simd_json::to_vec(
                        &plugin.settings.unwrap_or(simd_json::OwnedValue::default()),
                    )
                    .unwrap(),
                    supported_registrations,
                )
                .await
                .unwrap()
            {
                Ok(init_result) => init_result,
                Err(err) => {
                    error!("Failed to initialize plugin, error: {}", &err);
                    continue;
                }
            };

            if registrations_response.discord_events.message_create {
                initialized_plugins
                    .discord_events
                    .message_create
                    .push(plugin.name.clone());
            }

            for interaction_create_command in registrations_response
                .discord_events
                .interaction_create_commands
            {
                initialized_plugins_registrations.commands.push(
                    InitializedPluginRegistrationsCommand {
                        plugin_name: plugin.name.clone(),
                        command_data: interaction_create_command,
                    },
                );
            }

            for dependency_function in registrations_response.dependency_functions {
                let functions = if let Some(functions) =
                    initialized_plugins.dependencies.get_mut(&plugin.name)
                {
                    functions
                } else {
                    initialized_plugins
                        .dependencies
                        .insert(plugin.name.clone(), HashSet::new());

                    initialized_plugins
                        .dependencies
                        .get_mut(&plugin.name)
                        .unwrap()
                };

                functions.insert(dependency_function);
            }

            let plugin_context = RuntimePlugin {
                instance,
                store: Mutex::new(store),
            };

            runtime.plugins.insert(plugin.name.clone(), plugin_context);
        }

        Ok((
            runtime,
            initialized_plugins,
            initialized_plugins_registrations,
        ))
    }
}

impl Runtime {
    pub async fn call_event(
        &self,
        plugin_name: &str,
        event: &DiscordEvents,
    ) -> Result<Vec<DiscordRequests>, ()> {
        let plugin = self.plugins.get(plugin_name).unwrap();

        match plugin
            .instance
            .discord_bot_plugin_plugin_functions()
            .call_discord_event(&mut *plugin.store.lock().await, event)
            .await
        {
            Ok(call_result) => match call_result {
                Ok(discord_requests) => Ok(discord_requests),
                Err(err) => {
                    error!("The plugin returned an error: {}", &err);
                    Err(())
                }
            },
            Err(err) => {
                error!("Something went wrong while calling the plugin: {}", &err);
                Err(())
            }
        }
    }

    pub async fn call_scheduled_job(
        &self,
        plugin_name: &str,
        scheduled_job_name: &str,
    ) -> Result<Vec<DiscordRequests>, ()> {
        let plugin = self.plugins.get(plugin_name).unwrap();

        match plugin
            .instance
            .discord_bot_plugin_plugin_functions()
            .call_scheduled_job(&mut *plugin.store.lock().await, scheduled_job_name)
            .await
        {
            Ok(call_result) => match call_result {
                Ok(discord_requests) => Ok(discord_requests),
                Err(err) => {
                    error!("The plugin returned an error: {}", &err);
                    Err(())
                }
            },
            Err(err) => {
                error!("Something went wrong while calling the plugin: {}", &err);
                Err(())
            }
        }
    }
}
