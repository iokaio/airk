// SPDX-License-Identifier: Apache-2.0
//! The HTTP backend of a migrated imported legacy selection (moved from
//! `runtime_install::http_legacy`): revalidated before every read.
use airk_core::service_http::NativeImportedHttpSelection;
use airk_installer::runtime_install::http_legacy::validate;
use anyhow::{Result, ensure};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(crate) fn create_imported_backend(
    control: PathBuf,
    selection: NativeImportedHttpSelection,
) -> Result<Arc<dyn airk_mcp::remote::RemoteTools>> {
    validate(&control, &selection)?;
    Ok(Arc::new(ImportedBackend {
        control,
        selection,
        client: Mutex::new(airk_core::usage::ClientFamily::Unknown),
    }))
}

struct ImportedBackend {
    control: PathBuf,
    selection: NativeImportedHttpSelection,
    client: Mutex<airk_core::usage::ClientFamily>,
}

impl airk_mcp::remote::RemoteTools for ImportedBackend {
    fn local_help(&self) -> bool {
        false
    }
    fn query_concurrency(&self) -> usize {
        airk_host::service_resource_policy::query_concurrency(&self.control)
    }
    fn client(&self, client: airk_core::usage::ClientFamily) {
        *self.client.lock().unwrap_or_else(|e| e.into_inner()) = client;
    }
    fn call(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> airk_mcp::remote::RemoteFuture<'_> {
        self.call_with_read_context(
            tool,
            arguments,
            self.query_concurrency(),
            airk_mcp::read_attempt::ReadAttemptContext::new(
                Instant::now() + Duration::from_secs(125),
                Default::default(),
            ),
        )
    }
    fn call_with_read_context(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        _limit: usize,
        context: airk_mcp::read_attempt::ReadAttemptContext,
    ) -> airk_mcp::remote::RemoteFuture<'_> {
        Box::pin(async move {
            let fingerprint = validate(&self.control, &self.selection)?;
            let client = *self.client.lock().unwrap_or_else(|e| e.into_inner());
            // Under activated catalogs Connect uses native imported compatibility
            // admission, never authority-zero fallback. Require actual scoped
            // authority matching the frozen selection before submitting any read.
            let mut connection = tokio::select! {
                biased;
                _ = context.cancellation().cancelled() => anyhow::bail!("imported connection setup canceled"),
                _ = tokio::time::sleep_until(tokio::time::Instant::from_std(context.deadline())) =>
                    return Err(airk_mcp::remote::PreparationExpired.into()),
                connection = airk_frontend::project_client::Connection::connect_as(
                    &self.control, self.selection.registration.binding, client,
                ) => connection?,
            };
            connection.require_scope_fingerprint(fingerprint)?;
            ensure!(
                validate(&self.control, &self.selection)? == fingerprint,
                airk_core::scoped_reads::ScopeReadRefused
            );
            connection
                .call_with_read_context(tool, arguments, context)
                .await
        })
    }
}
