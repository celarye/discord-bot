use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    sync::{Arc, Weak, atomic::Ordering},
};

use simd_json::OwnedValue;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, trace, warn};
use twilight_gateway::{MessageSender, ShardId};
use twilight_model::guild::UnavailableGuild;
use wasmtime::{
    Config, Engine, Store,
    component::{Component, HasSelf, Linker},
};
use wasmtime_wasi::{
    DirPerms, FilePerms, ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView,
};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

use super::{
    AvailablePlugin, PluginRegistrationRequests, PluginRegistrationRequestsCommand,
    PluginRegistrations,
    runtime::discord_bot::plugin::{
        discord_types::{
            Events as DiscordEvents, Host as DiscordTypes, Requests as DiscordRequests,
            Responses as DiscordResponses,
        },
        host_functions::Host as HostFunctions,
        host_types::{Host as HostTypes, LogLevels},
        plugin_types::{
            Host as PluginTypes, SupportedRegistrations, SupportedRegistrationsDiscordEvents,
        },
    },
};
use crate::{SHUTDOWN, discord::DiscordBotClientSender, job_scheduler::JobScheduler};

// TODO: Check bindgen options reference
wasmtime::component::bindgen!({ imports: { default: async }, exports: { default: async }});

pub struct PluginBuilder {
    engine: Engine,
    linker: Linker<InternalRuntime>,
}

pub struct Runtime {
    plugins: RwLock<HashMap<String, RuntimePlugin>>,
    discord_bot_client_sender: Arc<DiscordBotClientSender>,
    job_scheduler: Weak<JobScheduler>,
}

pub struct RuntimePlugin {
    instance: Plugin,
    store: Mutex<Store<InternalRuntime>>,
}

struct InternalRuntime {
    wasi: WasiCtx,
    wasi_http: WasiHttpCtx,
    table: ResourceTable,
    runtime: Arc<Runtime>,
}

impl WasiView for InternalRuntime {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl WasiHttpView for InternalRuntime {
    fn ctx(&mut self) -> &mut WasiHttpCtx {
        &mut self.wasi_http
    }

    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

impl HostFunctions for InternalRuntime {
    async fn shutdown(&mut self, restart: bool) -> () {
        SHUTDOWN.shutdown.store(true, Ordering::Relaxed);
        SHUTDOWN.restart.store(restart, Ordering::Relaxed);

        self.runtime
            .job_scheduler
            .upgrade()
            .unwrap()
            .shutdown()
            .await;
        self.runtime.discord_bot_client_sender.shutdown().await;
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
        self.runtime
            .discord_bot_client_sender
            .request(request)
            .await
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

        // NOTE: Linker notes
        // Maybe there is a better way to link dependency plugins (not yet supported with the
        // component model)
        // Maybe there is a better way to add logging support
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
    pub fn new(
        discord_bot_client_sender: Arc<DiscordBotClientSender>,
        job_scheduler: Weak<JobScheduler>,
    ) -> Self {
        Runtime {
            plugins: RwLock::new(HashMap::new()),
            discord_bot_client_sender,
            job_scheduler,
        }
    }

    pub async fn initialize_plugins(
        runtime: Arc<Runtime>,
        plugin_builder: PluginBuilder,
        plugin_registrations: Arc<RwLock<PluginRegistrations>>,
        plugins: Vec<AvailablePlugin>,
        directory: &Path,
    ) -> Result<PluginRegistrationRequests, ()> {
        let mut initialized_plugins_registrations = PluginRegistrationRequests {
            scheduled_jobs: vec![],
            commands: vec![],
        };

        let supported_registrations = SupportedRegistrations {
            discord_events: SupportedRegistrationsDiscordEvents {
                message_create: true,
                interaction_create: true,
            },
            scheduled_jobs: true,
        };

        for plugin in plugins {
            let plugin_dir = directory.join(&plugin.id).join(&plugin.version);

            let bytes = fs::read(plugin_dir.join("plugin.wasm")).unwrap();
            let component = match Component::new(&plugin_builder.engine, bytes) {
                Ok(component) => component,
                Err(err) => {
                    error!(
                        "Something went wrong while creating a WASM component from the {} plugin, error: {}",
                        &plugin.id, &err
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
                                &plugin.id, &err
                            );
                        }
                    }
                }
                Err(err) => {
                    error!(
                        "Something went wrong while checking if the workspace directory of the {} plugin exists, error: {}",
                        &plugin.id, &err
                    );
                    return Err(());
                }
            }

            let wasi = WasiCtxBuilder::new()
                .envs(&env)
                .preopened_dir(workspace_plugin_dir, "/", DirPerms::all(), FilePerms::all())
                .unwrap()
                .build();

            let mut store = Store::<InternalRuntime>::new(
                &plugin_builder.engine,
                InternalRuntime {
                    wasi,
                    wasi_http: WasiHttpCtx::new(),
                    table: ResourceTable::new(),
                    runtime: runtime.clone(),
                },
            );

            let instance =
                match Plugin::instantiate_async(&mut store, &component, &plugin_builder.linker)
                    .await
                {
                    Ok(instance) => instance,
                    Err(err) => {
                        error!(
                            "Failed to instantiate the {} plugin, error: {}",
                            &plugin.id, &err
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
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .message_create
                    .push(plugin.id.clone());
            }

            for interaction_create_command in registrations_response
                .discord_events
                .interaction_create_commands
            {
                initialized_plugins_registrations.commands.push(
                    PluginRegistrationRequestsCommand {
                        plugin_id: plugin.id.clone(),
                        internal_id: interaction_create_command.0,
                        command_data: interaction_create_command.1,
                    },
                );
            }

            for dependency_function in registrations_response.dependency_functions {
                let mut plugin_registrations = plugin_registrations.write().await;
                let functions = plugin_registrations
                    .dependencies
                    .entry(plugin.id.clone())
                    .or_insert(HashSet::new());

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
                .insert(plugin.id.clone(), plugin_context);
        }

        Ok(initialized_plugins_registrations)
    }

    pub async fn register_shard_message_senders_to_shard_id(
        &self,
        shard_id: ShardId,
        message_sender: MessageSender,
    ) {
        self.discord_bot_client_sender
            .shard_message_senders_shard_id
            .write()
            .await
            .insert(shard_id, Arc::new(message_sender));
    }

    pub async fn register_shard_message_senders_to_guilds(
        &self,
        shard_id: ShardId,
        guilds: Vec<UnavailableGuild>,
    ) {
        for guild in guilds {
            let message_sender = self
                .discord_bot_client_sender
                .shard_message_senders_shard_id
                .read()
                .await
                .get(&shard_id)
                .unwrap()
                .clone();

            self.discord_bot_client_sender
                .shard_message_senders
                .write()
                .await
                .insert(guild.id, message_sender);
        }
    }
}

impl Runtime {
    pub async fn call_event(&self, plugin_name: &str, event: &DiscordEvents) -> Result<(), ()> {
        let plugins = self.plugins.read().await;
        let plugin = plugins.get(plugin_name).unwrap();

        match plugin
            .instance
            .discord_bot_plugin_plugin_functions()
            .call_discord_event(&mut *plugin.store.lock().await, event)
            .await
        {
            Ok(call_result) => match call_result {
                Ok(()) => Ok(()),
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
    ) -> Result<(), ()> {
        let plugins = self.plugins.read().await;
        let plugin = plugins.get(plugin_name).unwrap();

        match plugin
            .instance
            .discord_bot_plugin_plugin_functions()
            .call_scheduled_job(&mut *plugin.store.lock().await, scheduled_job_name)
            .await
        {
            Ok(call_result) => match call_result {
                Ok(()) => Ok(()),
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
