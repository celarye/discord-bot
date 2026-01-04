pub mod internal;

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::Arc,
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
    SHUTDOWN, Shutdown,
    channels::{DiscordBotClientMessages, JobSchedulerMessages, RuntimeMessages},
    plugins::{
        AvailablePlugin, Plugin, PluginRegistrationRequests,
        PluginRegistrationRequestsApplicationCommand, PluginRegistrationRequestsScheduledJob,
        PluginRegistrations, builder::PluginBuilder,
        discord_bot::plugin::discord_types::Events as DiscordEvents,
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
    store: Mutex<Store<InternalRuntime>>, // TODO: Add async support, waiting for better WASIp3 component creation support
}

impl Runtime {
    pub fn new(
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

    pub fn start(runtime: Arc<Runtime>) {
        tokio::spawn(async move {
            let mut dbc_js_rx = runtime.dbc_js_rx.write().await;

            tokio::select! {
                () = async {
                    while let Some(message) = dbc_js_rx.recv().await {
                        match message {
                            RuntimeMessages::CallDiscordEvent(plugin_name, event) => {
                                runtime.call_discord_event(&plugin_name, &event).await;
                            }
                            RuntimeMessages::CallScheduledJob(plugin_name, scheduled_job_name) => {
                                runtime.call_scheduled_job(&plugin_name, &scheduled_job_name).await;}
                        }
                    }
                } => {}
                () = runtime.cancellation_token.cancelled() => {
                    dbc_js_rx.close();
                }
            }
        });
    }

    pub async fn initialize_plugins(
        runtime: Arc<Runtime>,
        plugin_builder: PluginBuilder,
        plugins: Vec<AvailablePlugin>,
        plugin_registrations: Arc<RwLock<PluginRegistrations>>,
        directory: Arc<PathBuf>,
    ) -> Result<(), ()> {
        let mut registration_requests = PluginRegistrationRequests {
            discord_event_interaction_create: super::PluginRegistrationRequestsInteractionCreate {
                application_commands: vec![],
                message_component: vec![],
                modals: vec![],
            },
            scheduled_jobs: vec![],
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

            let plugin_registrations_request = match instance
                .discord_bot_plugin_plugin_functions()
                .call_initialization(
                    &mut store,
                    &simd_json::to_vec(&plugin.settings.unwrap_or(OwnedValue::default())).unwrap(),
                    plugin.permissions,
                )
                .await
            {
                Ok(init_result) => match init_result {
                    Ok(registrations_request) => registrations_request,
                    Err(err) => {
                        error!(
                            "Failed to initialize the {} plugin, error: {}",
                            &plugin.id, &err
                        );
                        continue;
                    }
                },
                Err(err) => {
                    error!(
                        "The {} plugin exprienced a critical error: {}",
                        &plugin.id, &err
                    );
                    continue;
                }
            };

            let plugin_context = RuntimePlugin {
                instance,
                store: Mutex::new(store),
            };

            runtime
                .plugins
                .write()
                .await
                .insert(plugin.id.clone(), plugin_context);

            if plugin_registrations_request.discord_events.message_create {
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .message_create
                    .push(plugin.id.clone());
            }

            for interaction_create_application_command in plugin_registrations_request
                .discord_events
                .interaction_create
                .application_commands
            {
                registration_requests
                    .discord_event_interaction_create
                    .application_commands
                    .push(PluginRegistrationRequestsApplicationCommand {
                        plugin_id: plugin.id.clone(),
                        id: interaction_create_application_command.0,
                        data: interaction_create_application_command.1,
                    });
            }

            // TODO: Prevent duplicate entries

            for interaction_create_message_component in plugin_registrations_request
                .discord_events
                .interaction_create
                .message_components
            {
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .interaction_create
                    .message_components
                    .insert(
                        interaction_create_message_component.clone(),
                        plugin.id.clone(),
                    );
            }

            for interaction_create_modal in plugin_registrations_request
                .discord_events
                .interaction_create
                .modals
            {
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .interaction_create
                    .modals
                    .insert(interaction_create_modal.clone(), plugin.id.clone());
            }

            if plugin_registrations_request.discord_events.thread_create {
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .thread_create
                    .push(plugin.id.clone());
            }

            if plugin_registrations_request.discord_events.thread_delete {
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .thread_delete
                    .push(plugin.id.clone());
            }

            if plugin_registrations_request.discord_events.thread_list_sync {
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .thread_list_sync
                    .push(plugin.id.clone());
            }

            if plugin_registrations_request
                .discord_events
                .thread_member_update
            {
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .thread_member_update
                    .push(plugin.id.clone());
            }

            if plugin_registrations_request
                .discord_events
                .thread_members_update
            {
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .thread_members_update
                    .push(plugin.id.clone());
            }

            if plugin_registrations_request.discord_events.thread_update {
                plugin_registrations
                    .write()
                    .await
                    .discord_events
                    .thread_update
                    .push(plugin.id.clone());
            }

            for scheduled_job_registration_request in plugin_registrations_request.scheduled_jobs {
                registration_requests
                    .scheduled_jobs
                    .push(PluginRegistrationRequestsScheduledJob {
                        plugin_id: plugin.id.clone(),
                        id: scheduled_job_registration_request.0,
                        crons: scheduled_job_registration_request.1,
                    });
            }

            for dependency_function in plugin_registrations_request.dependency_functions {
                let mut plugin_registrations = plugin_registrations.write().await;
                let functions = plugin_registrations
                    .dependency_functions
                    .entry(plugin.id.clone())
                    .or_insert(HashSet::new());

                functions.insert(dependency_function);
            }
        }

        let _ = runtime
            .discord_bot_client_tx
            .send(DiscordBotClientMessages::RegisterApplicationCommands(
                registration_requests
                    .discord_event_interaction_create
                    .application_commands,
            ))
            .await;

        let _ = runtime
            .job_scheduler_tx
            .send(JobSchedulerMessages::RegisterScheduledJobs(
                registration_requests.scheduled_jobs,
            ))
            .await;

        Ok(())
    }

    // TODO: Remove trapped plugins

    async fn call_discord_event(&self, plugin_name: &str, event: &DiscordEvents) {
        let plugins = self.plugins.read().await;
        let plugin = plugins.get(plugin_name).unwrap();

        match plugin
            .instance
            .discord_bot_plugin_plugin_functions()
            .call_discord_event(&mut *plugin.store.lock().await, event)
            .await
        {
            Ok(result) => {
                if let Err(err) = result {
                    error!("The {} plugin returned an error: {}", plugin_name, &err);
                }
            }
            Err(err) => {
                error!(
                    "The {} plugin exprienced a critical error: {}",
                    plugin_name, &err
                );
            }
        }
    }

    async fn call_scheduled_job(&self, plugin_name: &str, scheduled_job_name: &str) {
        let plugins = self.plugins.read().await;
        let plugin = plugins.get(plugin_name).unwrap();

        match plugin
            .instance
            .discord_bot_plugin_plugin_functions()
            .call_scheduled_job(&mut *plugin.store.lock().await, scheduled_job_name)
            .await
        {
            Ok(result) => {
                if let Err(err) = result {
                    error!("The {} plugin returned an error: {}", plugin_name, &err);
                }
            }
            Err(err) => {
                error!(
                    "The {} plugin exprienced a critical error: {}",
                    plugin_name, &err
                );
            }
        }
    }

    async fn call_shutdown(&self, plugin_name: String) {
        let plugins = self.plugins.read().await;
        let plugin = plugins.get(&plugin_name).unwrap();

        match plugin
            .instance
            .discord_bot_plugin_plugin_functions()
            .call_shutdown(&mut *plugin.store.lock().await)
            .await
        {
            Ok(result) => {
                if let Err(err) = result {
                    error!("The {} plugin returned an error: {}", plugin_name, &err);
                }
            }
            Err(err) => {
                error!(
                    "The {} plugin exprienced a critical error: {}",
                    plugin_name, &err
                );
            }
        }
    }

    pub async fn shutdown(&self, shutdown_type: Shutdown) {
        if SHUTDOWN.read().await.is_some() {
            // TODO: Do not wait for shutdown to complete, the main function shutdown logic needs to get reworked first
            self.cancellation_token.cancelled().await;
            return;
        }

        *SHUTDOWN.write().await = Some(shutdown_type);

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
