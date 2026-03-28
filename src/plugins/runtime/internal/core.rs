/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Copyright © 2026 Eduard Smet */

use tracing::{debug, error, info, trace, warn};

use crate::{
    Shutdown,
    plugins::{
        discord_bot::plugin::{
            core_export_types::Host as CoreExportTypesHost,
            core_import_functions::{Error, Host as CoreImportFunctionsHost},
            core_import_types::{
                CoreRegistrations, CoreRegistrationsResult, Host as CoreImportTypesHost, LogLevels,
                SupportedCoreRegistrations,
            },
            core_types::Host as CoreTypesHost,
        },
        runtime::internal::InternalRuntime,
    },
    utils::channels::CoreMessages,
};

impl CoreTypesHost for InternalRuntime {}
impl CoreImportTypesHost for InternalRuntime {}
impl CoreExportTypesHost for InternalRuntime {}

impl CoreImportFunctionsHost for InternalRuntime {
    async fn get_supported_registrations(&mut self) -> SupportedCoreRegistrations {
        todo!();
    }

    async fn register(&mut self, registrations: CoreRegistrations) -> CoreRegistrationsResult {
        todo!()
    }

    async fn log(&mut self, level: LogLevels, message: String) {
        match level {
            LogLevels::Trace => trace!(message),
            LogLevels::Debug => debug!(message),
            LogLevels::Info => info!(message),
            LogLevels::Warn => warn!(message),
            LogLevels::Error => error!(message),
        }
    }

    async fn shutdown(&mut self, restart: bool) {
        let shutdown_type = if restart {
            Shutdown::Restart
        } else {
            Shutdown::Normal
        };

        self.core_tx.send(CoreMessages::Shutdown(shutdown_type));
    }

    async fn dependency_function(
        &mut self,
        dependency_id: String,
        function_id: String,
        params: Vec<u8>,
    ) -> Result<Vec<u8>, Error> {
        todo!()
    }
}
