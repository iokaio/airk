// SPDX-License-Identifier: Apache-2.0
//! HTTP binds the existing native read owners to an exact credential selection.
use airk_core::{scoped_reads::ScopeReadRefused, service_http::HttpSelection};
use airk_mcp::{
    read_attempt::ReadAttemptContext,
    remote::{ReadyFuture, RemoteFuture, RemoteTools, http::AuthenticatedBackendFactory},
};
use anyhow::{Result, ensure};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

fn live(context: &ReadAttemptContext) -> Result<()> {
    ensure!(
        !context.cancellation().is_cancelled() && Instant::now() < context.deadline(),
        airk_mcp::remote::PreparationExpired
    );
    Ok(())
}

/// Refresh only the catalog clock. Frozen grant IDs/revisions/projects, native
/// context and imported lineage retain their original meaning.
pub(crate) async fn validate(
    control: &Path,
    selection: &HttpSelection,
    context: &ReadAttemptContext,
) -> Result<HttpSelection> {
    Ok(validate_details(control, selection, context).await?.0)
}

async fn validate_details(
    control: &Path,
    selection: &HttpSelection,
    context: &ReadAttemptContext,
) -> Result<(
    HttpSelection,
    Option<airk_core::service::ProjectRegistration>,
)> {
    live(context)?;
    selection.validate()?;
    let control = control.to_path_buf();
    let selection = selection.clone();
    let deadline = context.deadline();
    let guard = context.lifetime_guard();
    let task = tokio::task::spawn_blocking(move || -> Result<(HttpSelection, Option<airk_core::service::ProjectRegistration>)> {
        let _guard = guard;
        crate::install_layout::validate_control_root(&control)?;
        let mut registry = airk_store::service::registry::ServiceRegistry::open_saved_before(
            &control,
            Some(deadline),
        )?;
        let state = registry.catalog_state()?.ok_or(ScopeReadRefused)?;
        ensure!(state.namespace == selection.namespace(), ScopeReadRefused);
        ensure!(
            registry.catalog_pending_fence()?.is_none(),
            ScopeReadRefused
        );
        match selection {
            HttpSelection::Scoped { mut selection } => {
                ensure!(
                    state.revision >= selection.expected_catalog_revision,
                    ScopeReadRefused
                );
                selection.expected_catalog_revision = state.revision;
                let native =
                    crate::catalog_coordinator::native_scoped_selection(&mut registry, &selection)?;
                crate::catalog_authority::build_catalog_read_authority(&mut registry, &native)?;
                let registration = registry.get(native.backing)?.registration;
                Ok((HttpSelection::Scoped { selection }, Some(registration)))
            }
            HttpSelection::Combined { selection } => Ok((HttpSelection::Combined {
                selection: crate::combined_frontend::validate_http_selection(&control, &selection)?,
            }, None)),
            HttpSelection::ImportedLegacy { selection } => {
                crate::runtime_install::http_legacy::validate(&control, &selection)?;
                let registration = selection.registration.clone();
                Ok((HttpSelection::ImportedLegacy { selection }, Some(registration)))
            }
            HttpSelection::ImportedLegacyUnion { mut selection } => {
                for member in &mut selection.selections {
                    ensure!(Instant::now()<deadline, airk_mcp::remote::PreparationExpired);
                    crate::runtime_install::http_legacy::validate_registry(&mut registry,member)?;
                    member.expected_catalog_revision=state.revision;
                }
                ensure!(registry.catalog_state()? == Some(state),ScopeReadRefused);
                selection.validate()?;
                Ok((HttpSelection::ImportedLegacyUnion { selection },None))
            }
        }
    });
    tokio::select! {
        biased;
        _ = context.cancellation().cancelled() => anyhow::bail!("HTTP scope validation canceled"),
        _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => Err(airk_mcp::remote::PreparationExpired.into()),
        result = task => result?,
    }
}

#[cfg(test)]
pub(crate) async fn factory(
    control: PathBuf,
    selection: HttpSelection,
    context: ReadAttemptContext,
) -> Result<AuthenticatedBackendFactory> {
    factory_attached(control, selection, None, context).await
}

pub(crate) async fn factory_attached(
    control: PathBuf,
    selection: HttpSelection,
    explorer_descriptor: Option<PathBuf>,
    context: ReadAttemptContext,
) -> Result<AuthenticatedBackendFactory> {
    let (selection, registration) = validate_details(&control, &selection, &context).await?;
    Ok(Arc::new(move || {
        if let HttpSelection::ImportedLegacyUnion { selection } = &selection {
            let projects = selection
                .selections
                .iter()
                .map(|member| {
                    let backend = Arc::new(LazyBackend {
                        control: control.clone(),
                        selection: HttpSelection::ImportedLegacy {
                            selection: Box::new(member.clone()),
                        },
                        backend: tokio::sync::OnceCell::new(),
                        client: Mutex::new(airk_core::usage::ClientFamily::Unknown),
                        closed: AtomicBool::new(false),
                    }) as Arc<dyn RemoteTools>;
                    (member.registration.clone(), backend)
                })
                .collect();
            return crate::unified_mcp::http_project_attachments(
                control.clone(),
                projects,
                None,
                explorer_descriptor.clone(),
            );
        }
        let backend = Arc::new(LazyBackend {
            control: control.clone(),
            selection: selection.clone(),
            backend: tokio::sync::OnceCell::new(),
            client: Mutex::new(airk_core::usage::ClientFamily::Unknown),
            closed: AtomicBool::new(false),
        }) as Arc<dyn RemoteTools>;
        crate::unified_mcp::http_attachment(
            backend,
            control.clone(),
            registration.clone(),
            explorer_descriptor.clone(),
        )
    }))
}

/// Admission reads a live service-owned scalar; nested canonical adapters use
/// the same reader for the lifetime of this future, without disk I/O on Tokio.
pub(crate) fn with_query_capacity(
    factory: AuthenticatedBackendFactory,
    reader: crate::service_resource_policy::QueryCapacityReader,
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
    reader: crate::service_resource_policy::QueryCapacityReader,
}
impl RemoteTools for CapacityBackend {
    fn combined_selection(&self) -> Option<&airk_core::service::CombinedConnectRequest> {
        self.backend.combined_selection()
    }
    fn scoped_selection(&self) -> Option<&airk_core::service::ScopedConnectRequest> {
        self.backend.scoped_selection()
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
        Box::pin(crate::service_resource_policy::with_query_capacity(
            self.reader.clone(),
            async move { self.backend.prepare().await },
        ))
    }
    fn shutdown(&self) -> ReadyFuture<'_> {
        Box::pin(crate::service_resource_policy::with_query_capacity(
            self.reader.clone(),
            async move { self.backend.shutdown().await },
        ))
    }
    fn call(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> RemoteFuture<'_> {
        Box::pin(crate::service_resource_policy::with_query_capacity(
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
        Box::pin(crate::service_resource_policy::with_query_capacity(
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
        Box::pin(crate::service_resource_policy::with_query_capacity(
            self.reader.clone(),
            async move {
                self.backend
                    .call_with_read_context(tool, arguments, limit, context)
                    .await
            },
        ))
    }
}

struct LazyBackend {
    control: PathBuf,
    selection: HttpSelection,
    backend: tokio::sync::OnceCell<Arc<dyn RemoteTools>>,
    client: Mutex<airk_core::usage::ClientFamily>,
    closed: AtomicBool,
}
impl LazyBackend {
    async fn backend(&self, context: &ReadAttemptContext) -> Result<&Arc<dyn RemoteTools>> {
        live(context)?;
        ensure!(!self.closed.load(Ordering::Acquire), "HTTP session closed");
        let initialized = self.backend.get_or_try_init(|| async {
            let backend = match &self.selection {
                HttpSelection::Scoped { selection } => {
                    let resolved = crate::service_frontend::resolve_scoped(
                        None,
                        Some(&self.control),
                        selection,
                    )
                    .await?;
                    crate::project_client::scoped_tools(
                        resolved.project.control,
                        resolved.project.registration.binding,
                        resolved.selection,
                    )?
                }
                HttpSelection::Combined { selection } => {
                    crate::combined_frontend::resolve(None, Some(&self.control), selection).await?
                }
                HttpSelection::ImportedLegacy { selection } => {
                    crate::runtime_install::http_legacy::create_imported_backend(
                        self.control.clone(),
                        selection.as_ref().clone(),
                    )?
                }
                HttpSelection::ImportedLegacyUnion { .. } => anyhow::bail!(
                    "multi-project HTTP routing requires its original explicit project selector"
                ),
            };
            backend.client(*self.client.lock().unwrap_or_else(|e| e.into_inner()));
            if self.closed.load(Ordering::Acquire) {
                backend.close();
                anyhow::bail!("HTTP session closed");
            }
            Ok::<_, anyhow::Error>(backend)
        });
        tokio::select! {
            biased;
            _ = context.cancellation().cancelled() => anyhow::bail!("HTTP session setup canceled"),
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(context.deadline())) => Err(airk_mcp::remote::PreparationExpired.into()),
            backend = initialized => backend,
        }
    }
}
impl RemoteTools for LazyBackend {
    fn local_help(&self) -> bool {
        false
    }
    fn scoped_selection(&self) -> Option<&airk_core::service::ScopedConnectRequest> {
        if let HttpSelection::Scoped { selection } = &self.selection {
            Some(selection)
        } else {
            None
        }
    }
    fn combined_selection(&self) -> Option<&airk_core::service::CombinedConnectRequest> {
        if let HttpSelection::Combined { selection } = &self.selection {
            Some(selection)
        } else {
            None
        }
    }
    fn catalog(&self) -> Vec<airk_mcp::remote::Tool> {
        if self.combined_selection().is_some() {
            airk_mcp::combined::catalog()
        } else {
            airk_mcp::local::tools()
        }
    }
    fn query_concurrency(&self) -> usize {
        crate::service_resource_policy::query_concurrency(&self.control)
    }
    fn client(&self, client: airk_core::usage::ClientFamily) {
        *self.client.lock().unwrap_or_else(|e| e.into_inner()) = client;
        if let Some(backend) = self.backend.get() {
            backend.client(client);
        }
    }
    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        if let Some(backend) = self.backend.get() {
            backend.close();
        }
    }
    fn shutdown(&self) -> ReadyFuture<'_> {
        Box::pin(async move {
            self.close();
            if let Some(backend) = self.backend.get() {
                backend.shutdown().await?;
            }
            Ok(())
        })
    }
    fn call(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> RemoteFuture<'_> {
        self.call_with_read_context(
            tool,
            arguments,
            self.query_concurrency(),
            ReadAttemptContext::new(
                Instant::now() + Duration::from_secs(125),
                Default::default(),
            ),
        )
    }
    fn call_with_read_context(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        limit: usize,
        context: ReadAttemptContext,
    ) -> RemoteFuture<'_> {
        Box::pin(async move {
            validate(&self.control, &self.selection, &context).await?;
            let backend = self.backend(&context).await?;
            backend
                .call_with_read_context(tool, arguments, limit, context)
                .await
        })
    }
}
