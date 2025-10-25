use std::sync::Weak;

use futures::executor::block_on;
use tokio::sync::oneshot;
use tracing::{debug, error, info, trace, warn};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

use crate::{
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
    fn shutdown(&mut self, restart: bool) {
        block_on(self.runtime.upgrade().unwrap().shutdown(restart))
    }

    fn log(&mut self, level: LogLevels, message: String) {
        match level {
            LogLevels::Trace => trace!(message),
            LogLevels::Debug => debug!(message),
            LogLevels::Info => info!(message),
            LogLevels::Warn => warn!(message),
            LogLevels::Error => error!(message),
        }
    }

    fn discord_request(&mut self, request: DiscordRequests) -> Result<DiscordResponses, String> {
        let runtime = self.runtime.upgrade().unwrap();

        let (tx, rx) = oneshot::channel();

        if let Err(err) = block_on(
            runtime
                .discord_bot_client_tx
                .send(DiscordBotClientMessages::Request(request, tx)),
        ) {
            let err = format!(
                "Something went wrong while sending a message over the Discord channel, error: {err}"
            );

            error!(err);

            return Err(err);
        }

        match block_on(rx) {
            Ok(result) => result,
            Err(err) => {
                let err = format!("The OneShot sender was dropped: {err}");
                error!(err);
                Err(err)
            }
        }
    }

    fn dependency(
        &mut self,
        dependency: String,
        function: String,
        params: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        let runtime = self.runtime.upgrade().unwrap();

        let plugins = block_on(runtime.plugins.read());
        let plugin = plugins.get(&dependency).unwrap();

        // TODO: Check if it is an actual dependency and prevent deadlocks, the channel rework should fix
        // the potential deadlocks.

        match plugin
            .instance
            .discord_bot_plugin_plugin_functions()
            .call_dependency(&mut *block_on(plugin.store.lock()), &function, &params)
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
