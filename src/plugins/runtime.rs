pub mod internal;

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    sync::{Arc, atomic::Ordering},
};

use simd_json::OwnedValue;
use tokio::sync::{
    Mutex, RwLock,
    mpsc::{Receiver, Sender},
    oneshot,
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use wasmtime::{Store, component::Component};
use wasmtime_wasi::{DirPerms, FilePerms, ResourceTable, WasiCtxBuilder};
use wasmtime_wasi_http::WasiHttpCtx;

use crate::{
    SHUTDOWN,
    channels::{DiscordBotClientMessages, JobSchedulerMessages, RuntimeMessages},
    plugins::{
        AvailablePlugin, DiscordApplicationCommandRegistrationRequest, Plugin, PluginRegistrations,
        ScheduledJobRegistrationRequest,
        builder::PluginBuilder,
        discord_bot::plugin::{
            discord_types::Events as DiscordEvents,
            plugin_types::{SupportedRegistrations, SupportedRegistrationsDiscordEvents},
        },
        runtime::internal::InternalRuntime,
    },
};

pub struct Runtime {
    plugins: RwLock<HashMap<String, RuntimePlugin>>,
    discord_bot_client_tx: Arc<Sender<DiscordBotClientMessages>>,
    job_scheduler_tx: Arc<Sender<JobSchedulerMessages>>,
    dbc_js_rx: RwLock<Receiver<RuntimeMessages>>,
    pub cancellation_token: CancellationToken,
}

pub struct RuntimePlugin {
    instance: Plugin,
    store: Mutex<Store<InternalRuntime>>, // TODO: Add async support
}

impl Runtime {
    pub async fn new(
        discord_bot_client_tx: Sender<DiscordBotClientMessages>,
        job_scheduler_tx: Sender<JobSchedulerMessages>,
        dbc_js_rx: Receiver<RuntimeMessages>,
    ) -> Self {
        Runtime {
            plugins: RwLock::new(HashMap::new()),
            discord_bot_client_tx: Arc::new(discord_bot_client_tx),
            job_scheduler_tx: Arc::new(job_scheduler_tx),
            dbc_js_rx: RwLock::new(dbc_js_rx),
            cancellation_token: CancellationToken::new(),
        }
    }

    pub async fn initialize_plugins(
        runtime: Arc<Runtime>,
        plugin_builder: PluginBuilder,
        plugins: Vec<AvailablePlugin>,
        plugin_registrations: Arc<RwLock<PluginRegistrations>>,
        directory: &Path,
    ) -> Result<(), ()> {
        let mut scheduled_job_registration_requests = vec![];
        let mut discord_application_commands_registration_requests = vec![];

        // TODO:
        // - Replace this with flags and expect flags back
        // - Move flag parser to its own method
        let supported_registrations = SupportedRegistrations {
            discord_events: SupportedRegistrationsDiscordEvents {
                interaction_create: true,
                message_create: true,
                thread_create: true,
                thread_delete: true,
                thread_list_sync: true,
                thread_member_update: true,
                thread_members_update: true,
                thread_update: true,
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
                        "Something went wrong while creating a WASI component from the {} plugin, error: {}",
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
                    if !exists && let Err(err) = fs::create_dir(&workspace_plugin_dir) {
                        error!(
                            "Something went wrong while creating the workspace directory for the {} plugin, error: {}",
                            &plugin.id, &err
                        );
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
                InternalRuntime::new(
                    wasi,
                    WasiHttpCtx::new(),
                    ResourceTable::new(),
                    Arc::downgrade(&runtime),
                ),
            );

            let instance = match Plugin::instantiate(&mut store, &component, &plugin_builder.linker)
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
                discord_application_commands_registration_requests.push(
                    DiscordApplicationCommandRegistrationRequest {
                        plugin_id: plugin.id.clone(),
                        internal_id: interaction_create_command.0,
                        command_data: interaction_create_command.1,
                    },
                );
            }

            if registrations_response.discord_events.thread_create {
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .thread_create
                    .push(plugin.id.clone());
            }

            if registrations_response.discord_events.thread_delete {
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .thread_delete
                    .push(plugin.id.clone());
            }

            if registrations_response.discord_events.thread_list_sync {
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .thread_list_sync
                    .push(plugin.id.clone());
            }

            if registrations_response.discord_events.thread_member_update {
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .thread_member_update
                    .push(plugin.id.clone());
            }

            if registrations_response.discord_events.thread_members_update {
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .thread_members_update
                    .push(plugin.id.clone());
            }

            if registrations_response.discord_events.thread_update {
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .thread_update
                    .push(plugin.id.clone());
            }

            for scheduled_job_registration_request in registrations_response.scheduled_jobs {
                scheduled_job_registration_requests.push(ScheduledJobRegistrationRequest {
                    plugin_id: plugin.id.clone(),
                    internal_id: scheduled_job_registration_request.0,
                    crons: scheduled_job_registration_request.1,
                })
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

        let _ = runtime
            .discord_bot_client_tx
            .send(DiscordBotClientMessages::RegisterApplicationCommands(
                discord_application_commands_registration_requests,
            ))
            .await;
        let _ = runtime
            .job_scheduler_tx
            .send(JobSchedulerMessages::RegisterScheduledJobs(
                scheduled_job_registration_requests,
            ))
            .await;

        Ok(())
    }

    pub async fn start(runtime: Arc<Runtime>) {
        tokio::spawn(async move {
            let mut dbc_js_rx = runtime.dbc_js_rx.write().await;

            tokio::select! {
                _ = async {
                    while let Some(message) = dbc_js_rx.recv().await {
                        match message {
                            RuntimeMessages::CallDiscordEvent(plugin_name, event) => {
                                runtime.call_discord_event(&plugin_name, &event).await;
                            }
                            RuntimeMessages::CallScheduledJob(plugin_name, scheduled_job_name) => {
                                runtime.call_scheduled_job(&plugin_name, &scheduled_job_name).await;}
                        };
                    }
                } => {}
                _ = runtime.cancellation_token.cancelled() => {
                    dbc_js_rx.close();
                }
            }
        });
    }

    async fn call_discord_event(&self, plugin_name: &str, event: &DiscordEvents) {
        let plugins = self.plugins.read().await;
        let plugin = plugins.get(plugin_name).unwrap();

        let _ = plugin
            .instance
            .discord_bot_plugin_plugin_functions()
            .call_discord_event(&mut *plugin.store.lock().await, event);
    }

    async fn call_scheduled_job(&self, plugin_name: &str, scheduled_job_name: &str) {
        let plugins = self.plugins.read().await;
        let plugin = plugins.get(plugin_name).unwrap();

        let _ = plugin
            .instance
            .discord_bot_plugin_plugin_functions()
            .call_scheduled_job(&mut *plugin.store.lock().await, scheduled_job_name);
    }

    pub async fn shutdown(&self, restart: bool) {
        if SHUTDOWN.shutdown.load(Ordering::Relaxed) {
            self.cancellation_token.cancelled().await;
            return;
        }

        SHUTDOWN.shutdown.store(true, Ordering::Relaxed);
        SHUTDOWN.restart.store(restart, Ordering::Relaxed);

        let job_scheduler_is_done = oneshot::channel();
        let discord_bot_client_is_done = oneshot::channel();

        info!("Shutting down the job scheduler");
        let _ = self
            .job_scheduler_tx
            .send(JobSchedulerMessages::Shutdown(job_scheduler_is_done.0))
            .await;
        let _ = job_scheduler_is_done.1.await;

        info!("Shutting down the Discord bot client shards");
        let _ = self
            .discord_bot_client_tx
            .send(DiscordBotClientMessages::Shutdown(
                discord_bot_client_is_done.0,
            ))
            .await;
        let _ = discord_bot_client_is_done.1.await;

        // TODO: Allow all plugin calls to finish, call the shutdown methods on them and only then return

        self.cancellation_token.cancel();
    }
}
