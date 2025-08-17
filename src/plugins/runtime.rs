use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    sync::Arc,
};

use simd_json::OwnedValue;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, trace, warn};
use wasmtime::{
    Config, Engine, Store,
    component::{Component, HasSelf, Linker},
};
use wasmtime_wasi::{
    DirPerms, FilePerms, ResourceTable,
    p2::{IoView, WasiCtx, WasiCtxBuilder, WasiView},
};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

use crate::plugins::runtime::discord_bot::plugin::host_types::LogLevels;

use super::{
    AvailablePlugin, InitializedPluginRegistrations, InitializedPluginRegistrationsCommand,
    InitializedPlugins, InitializedPluginsDiscordEvents,
    runtime::discord_bot::plugin::{
        discord_types::{
            Events as DiscordEvents, Host as DiscordTypes, Requests as DiscordRequests,
            Responses as DiscordResponses,
        },
        host_functions::Host as HostFunctions,
        host_types::Host as HostTypes,
        plugin_types::{Host as PluginTypes, SupportedRegistrations},
    },
};

wasmtime::component::bindgen!({ async: true});

pub struct PluginBuilder {
    engine: Engine,
    linker: Linker<InternalRuntime>,
}

pub struct Runtime {
    plugins: RwLock<HashMap<String, RuntimePlugin>>,
    discord_bot_client_sender: (),
    data: (),
}

pub struct RuntimePlugin {
    instance: Plugin,
    store: Mutex<Store<InternalRuntime>>,
}

struct InternalRuntime {
    ctx: WasiCtx,
    http: WasiHttpCtx,
    table: ResourceTable,
    runtime: Arc<Runtime>, // TODO: Make this a reference?
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

impl HostFunctions for InternalRuntime {
    async fn shutdown(&mut self, restart: bool) -> () {
        todo!();
    }

    async fn log(&mut self, level: LogLevels, message: String) -> () {
        match level {
            LogLevels::Trace => trace!(message),
            LogLevels::Debug => debug!(message),
            LogLevels::Info => info!(message),
            LogLevels::Warn => warn!(message),
            LogLevels::Error => error!(message),
        }
    }

    async fn discord(&mut self, request: DiscordRequests) -> Result<DiscordResponses, String> {
        todo!();
    }

    async fn dependency(
        &mut self,
        dependency: String,
        function: String,
        params: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        let plugins = self.runtime.plugins.read().await;
        let plugin = plugins.get(&dependency).unwrap();

        // TODO: Check if it is an actual dependency and prevent DoS

        match plugin
            .instance
            .discord_bot_plugin_plugin_functions()
            .call_dependency(&mut *plugin.store.lock().await, &function, &params)
            .await
        {
            Ok(call_result) => match call_result {
                Ok(dependency_result) => Ok(dependency_result),
                Err(err) => {
                    let err = format!("The plugin returned an error: {}", &err);
                    error!(err);
                    Err(err)
                }
            },
            Err(err) => {
                let err = format!("Something went wrong while calling the plugin: {}", &err);
                error!(err);
                Err(err)
            }
        }
    }
}

impl HostTypes for InternalRuntime {}
impl PluginTypes for InternalRuntime {}
impl DiscordTypes for InternalRuntime {}

impl PluginBuilder {
    pub fn new() -> Self {
        let mut config = Config::new();
        config.async_support(true);
        // TODO: Create wasmtime epoch interuption, would maybe prevent things like permantent TCP listeners to work?

        let engine = Engine::new(&config).unwrap();

        // TODO: Configure the linker, here host imports can be defined
        // Maybe there is a better way to link dependency plugins here
        // Maybe there is a better way to add logging support here
        let mut linker = Linker::<InternalRuntime>::new(&engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker).unwrap();
        wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker).unwrap();

        Plugin::add_to_linker::<InternalRuntime, HasSelf<_>>(&mut linker, |internal_runtime| {
            internal_runtime
        })
        .unwrap();

        PluginBuilder { engine, linker }
    }
}

impl Runtime {
    pub fn new() -> Self {
        Runtime {
            plugins: RwLock::new(HashMap::new()),
            discord_bot_client_sender: (),
            data: (),
        }
    }

    pub async fn initialize_plugins(
        runtime: Arc<Runtime>,
        plugin_builder: PluginBuilder,
        plugins: Vec<AvailablePlugin>,
        directory: &Path,
    ) -> Result<(InitializedPlugins, InitializedPluginRegistrations), ()> {
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

            let bytes = fs::read(plugin_dir.join("plugin.wasm")).unwrap();
            let component = match Component::new(&plugin_builder.engine, bytes) {
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

            let mut store = Store::<InternalRuntime>::new(
                &plugin_builder.engine,
                InternalRuntime {
                    ctx,
                    http: WasiHttpCtx::new(),
                    table: ResourceTable::new(),
                    runtime: runtime.clone(),
                },
            );

            // Validate and instantiate the component
            let instance =
                match Plugin::instantiate_async(&mut store, &component, &plugin_builder.linker)
                    .await
                {
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
                    &simd_json::to_vec(&plugin.settings.unwrap_or(OwnedValue::default())).unwrap(),
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

            runtime
                .plugins
                .write()
                .await
                .insert(plugin.name.clone(), plugin_context);
        }

        Ok((initialized_plugins, initialized_plugins_registrations))
    }
}

impl Runtime {
    pub async fn call_event(
        &self,
        plugin_name: &str,
        event: &DiscordEvents,
    ) -> Result<Vec<DiscordRequests>, ()> {
        let plugins = self.plugins.read().await;
        let plugin = plugins.get(plugin_name).unwrap();

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
        let plugins = self.plugins.read().await;
        let plugin = plugins.get(plugin_name).unwrap();

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
