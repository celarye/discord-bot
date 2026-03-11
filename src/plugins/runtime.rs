/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Copyright © 2026 Eduard Smet */

pub mod internal;

use std::{collections::HashMap, fs, path::Path, sync::Arc};

use serde_yaml_ng::Value;
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
    plugins::{
        AvailablePlugin, Plugin, PluginRegistrations, builder::PluginBuilder,
        discord_bot::plugin::discord_export_types::DiscordEvents,
        runtime::internal::InternalRuntime,
    },
    utils::channels::{DiscordBotClientMessages, JobSchedulerMessages, RuntimeMessages},
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
        plugins: HashMap<String, AvailablePlugin>,
        plugin_registrations: Arc<RwLock<PluginRegistrations>>,
        directory: &Path,
    ) -> Result<(), ()> {
        for (plugin_uid, plugin) in plugins {
            let plugin_directory = directory
                .join(&plugin.registry_id)
                .join(&plugin.id)
                .join(plugin.version.to_string());

            let bytes = match fs::read(plugin_directory.join("plugin.wasm")) {
                Ok(bytes) => bytes,
                Err(err) => {
                    error!(
                        "An error occured while reading the {} plugin file: {err}",
                        plugin_uid
                    );
                    continue;
                }
            };

            let component = match Component::new(&plugin_builder.engine, bytes) {
                Ok(component) => component,
                Err(err) => {
                    error!(
                        "An error occured while creating a WASI component from the {plugin_uid} plugin: {err}"
                    );
                    continue;
                }
            };

            let env_hm = plugin.environment.unwrap_or(HashMap::new());

            let env: Box<[(&str, &str)]> = env_hm
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();

            let workspace_plugin_dir = plugin_directory.join("workspace");

            match fs::exists(&workspace_plugin_dir) {
                Ok(exists) => {
                    if !exists && let Err(err) = fs::create_dir(&workspace_plugin_dir) {
                        error!(
                            "Something went wrong while creating the workspace directory for the {} plugin, error: {}",
                            &plugin_uid, &err
                        );
                    }
                }
                Err(err) => {
                    error!(
                        "Something went wrong while checking if the workspace directory of the {} plugin exists, error: {}",
                        &plugin_uid, &err
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
                    plugin_uid.clone(),
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
                            &plugin_uid, &err
                        );
                        continue;
                    }
                };

            match instance
                .discord_bot_plugin_core_export_functions()
                .call_initialization(
                    &mut store,
                    &sonic_rs::to_vec(&plugin.settings.unwrap_or(Value::default())).unwrap(),
                )
                .await
            {
                Ok(init_result) => {
                    if let Err(err) = init_result {
                        error!(
                            "the {plugin_uid} plugin returned an error while intiializing: {err}"
                        );
                        continue;
                    }
                }
                Err(err) => {
                    error!("The {plugin_uid} plugin exprienced a critical error: {err}");
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
                .insert(plugin_uid, plugin_context);
        }

        Ok(())
    }

    // TODO: Remove trapped plugins

    async fn call_discord_event(&self, plugin_name: &str, event: &DiscordEvents) {
        let plugins = self.plugins.read().await;
        let plugin = plugins.get(plugin_name).unwrap();

        match plugin
            .instance
            .discord_bot_plugin_discord_export_functions()
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
            .discord_bot_plugin_core_export_functions()
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
            .discord_bot_plugin_core_export_functions()
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
