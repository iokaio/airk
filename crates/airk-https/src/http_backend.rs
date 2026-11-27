// SPDX-License-Identifier: Apache-2.0
//! HTTP binds the local agent session (U110) to the native open-read path:
//! the catalog check, the open backend and the query-capacity decoration.
use airk_core::{identity::Incarnation, scoped_reads::ScopeReadRefused};
use airk_mcp::{
    read_attempt::ReadAttemptContext,
    remote::{ReadyFuture, RemoteFuture, RemoteTools, http::AuthenticatedBackendFactory},
};
use anyhow::{Result, ensure};
use std::{path::Path, sync::Arc, time::Instant};
mod open;
pub use open::{OpenHooks, OpenRead, OpenReader, open_factory};

fn live(context: &ReadAttemptContext) -> Result<()> {
    ensure!(
        !context.cancellation().is_cancelled() && Instant::now() < context.deadline(),
        airk_mcp::remote::PreparationExpired
    );
    Ok(())
}

/// The local agent session (U110) needs only a ready catalog of its
/// namespace with no catalog fence pending; every call then reads through
/// the open path, which resolves and leases its own scope.
pub async fn validate_open(
    control: &Path,
    namespace: Incarnation,
    context: &ReadAttemptContext,
) -> Result<()> {
    live(context)?;
    let control = control.to_path_buf();
    let deadline = context.deadline();
    let guard = context.lifetime_guard();
    let task = tokio::task::spawn_blocking(move || -> Result<()> {
        let _guard = guard;
        airk_host::install_layout::validate_control_root(&control)?;
        let registry = airk_store::service::registry::ServiceRegistry::open_saved_before(
            &control,
            Some(deadline),
        )?;
        let state = registry.catalog_state()?.ok_or(ScopeReadRefused)?;
        ensure!(state.namespace == namespace, ScopeReadRefused);
        ensure!(
            registry.catalog_pending_fence()?.is_none(),
            ScopeReadRefused
        );
        Ok(())
    });
    tokio::select! {
        biased;
        _ = context.cancellation().cancelled() => anyhow::bail!("HTTP scope validation canceled"),
        _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => Err(airk_mcp::remote::PreparationExpired.into()),
        result = task => result?,
    }
}

/// Admission reads a live service-owned scalar; nested canonical adapters use
/// the same reader for the lifetime of this future, without disk I/O on Tokio.
pub fn with_query_capacity(
    factory: AuthenticatedBackendFactory,
    reader: airk_host::service_resource_policy::QueryCapacityReader,
) -> AuthenticatedBackendFactory {
    Arc::new(move || {
        Ok(Arc::new(CapacityBackend {
            backend: factory()?,
            reader: reader.clone(),
        }) as Arc<dyn RemoteTools>)
    })
}
struct CapacityBackend {
    backend: Arc<dyn RemoteTools>,
    reader: airk_host::service_resource_policy::QueryCapacityReader,
}
impl RemoteTools for CapacityBackend {
    fn scoped_scope(&self) -> Option<airk_mcp::remote::ScopedScope> {
        self.backend.scoped_scope()
    }
    fn combined_scope(&self) -> Option<airk_mcp::remote::CombinedScope> {
        self.backend.combined_scope()
    }
    fn query_concurrency(&self) -> usize {
        (self.reader)()
    }
    fn catalog(&self) -> Vec<airk_mcp::remote::Tool> {
        self.backend.catalog()
    }
    fn guidance(&self) -> &'static str {
        self.backend.guidance()
    }
    fn local_help(&self) -> bool {
        self.backend.local_help()
    }
    /// Forwarded: without it the bridge filled neither the configured scope
    /// nor the MCP-roots workspace for the local agent session (found 27
    /// November 2026).
    fn fills_workspace(&self, tool: &str) -> bool {
        self.backend.fills_workspace(tool)
    }
    fn uses_response_mode(&self, tool: &str) -> bool {
        self.backend.uses_response_mode(tool)
    }
    fn client(&self, client: airk_core::usage::ClientFamily) {
        self.backend.client(client);
    }
    fn close(&self) {
        self.backend.close();
    }
    fn prepare(&self) -> ReadyFuture<'_> {
        Box::pin(airk_host::service_resource_policy::with_query_capacity(
            self.reader.clone(),
            async move { self.backend.prepare().await },
        ))
    }
    fn shutdown(&self) -> ReadyFuture<'_> {
        Box::pin(airk_host::service_resource_policy::with_query_capacity(
            self.reader.clone(),
            async move { self.backend.shutdown().await },
        ))
    }
    fn call(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> RemoteFuture<'_> {
        Box::pin(airk_host::service_resource_policy::with_query_capacity(
            self.reader.clone(),
            async move { self.backend.call(tool, arguments).await },
        ))
    }
    fn call_with_capacity(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        limit: usize,
    ) -> RemoteFuture<'_> {
        Box::pin(airk_host::service_resource_policy::with_query_capacity(
            self.reader.clone(),
            async move {
                self.backend
                    .call_with_capacity(tool, arguments, limit)
                    .await
            },
        ))
    }
    fn call_with_read_context(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        limit: usize,
        context: ReadAttemptContext,
    ) -> RemoteFuture<'_> {
        Box::pin(airk_host::service_resource_policy::with_query_capacity(
            self.reader.clone(),
            async move {
                self.backend
                    .call_with_read_context(tool, arguments, limit, context)
                    .await
            },
        ))
    }
}

#[cfg(test)]
mod scope_tests;
