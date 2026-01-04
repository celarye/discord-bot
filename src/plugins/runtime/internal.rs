use std::sync::Weak;

use tokio::sync::oneshot;
use tracing::{debug, error, info, trace, warn};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

use crate::{
    Shutdown,
    channels::DiscordBotClientMessages,
    plugins::{
        discord_bot::plugin::{
            discord_types::{
                Host as DiscordTypes, Requests as DiscordRequests, Responses as DiscordResponses,
            },
            host_functions::Host as HostFunctions,
            host_types::{Host as HostTypes, LogLevels},
            plugin_types::Host as PluginTypes,
        },
        runtime::Runtime,
    },
};

pub struct InternalRuntime {
    wasi: WasiCtx,
    wasi_http: WasiHttpCtx,
    table: ResourceTable,
    runtime: Weak<Runtime>,
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
    async fn log(&mut self, level: LogLevels, message: String) {
        match level {
            LogLevels::Trace => trace!(message),
            LogLevels::Debug => debug!(message),
            LogLevels::Info => info!(message),
            LogLevels::Warn => warn!(message),
            LogLevels::Error => error!(message),
        }
    }

    async fn discord_request(
        &mut self,
        request: DiscordRequests,
    ) -> Result<Option<DiscordResponses>, String> {
        let runtime = self.runtime.upgrade().unwrap();

        let (tx, rx) = oneshot::channel();

        if let Err(err) = runtime
            .discord_bot_client_tx
            .send(DiscordBotClientMessages::Request(request, tx))
            .await
        {
            let err = format!(
                "Something went wrong while sending a message over the Discord channel, error: {err}"
            );

            error!(err);

            return Err(err);
        }

        match rx.await {
            Ok(result) => result,
            Err(err) => {
                let err = format!("The OneShot sender was dropped: {err}");
                error!(err);
                Err(err)
            }
        }
    }

    async fn dependency_function(
        &mut self,
        dependency: String,
        function: String,
        params: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        let runtime = self.runtime.upgrade().unwrap();

        let plugins = runtime.plugins.read().await;
        let plugin = plugins.get(&dependency).unwrap();

        // TODO: Check if it is an actual dependency and prevent deadlocks, the channel rework should fix
        // the potential deadlocks.

        match plugin
            .instance
            .discord_bot_plugin_plugin_functions()
            .call_dependency_function(&mut *plugin.store.lock().await, &function, &params)
            .await
        {
            Ok(call_result) => match call_result {
                Ok(dependency_result) => Ok(dependency_result),
                Err(err) => {
                    let err = format!("The plugin returned an error: {err}");
                    error!(err);
                    Err(err)
                }
            },
            Err(err) => {
                let err = format!("Something went wrong while calling the plugin: {err}");
                error!(err);
                Err(err)
            }
        }
    }

    async fn shutdown(&mut self, restart: bool) {
        let shutdown_type = if restart {
            Shutdown::Restart
        } else {
            Shutdown::Normal
        };

        self.runtime
            .upgrade()
            .unwrap()
            .shutdown(shutdown_type)
            .await;
    }
}

impl HostTypes for InternalRuntime {}
impl PluginTypes for InternalRuntime {}
impl DiscordTypes for InternalRuntime {}

impl InternalRuntime {
    pub fn new(
        wasi: WasiCtx,
        wasi_http: WasiHttpCtx,
        table: ResourceTable,
        runtime: Weak<Runtime>,
    ) -> Self {
        InternalRuntime {
            wasi,
            wasi_http,
            table,
            runtime,
        }
    }
}
