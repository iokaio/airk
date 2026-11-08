// SPDX-License-Identifier: Apache-2.0
//! Per-call routing view; never creates authority, workers or retry credit.
use super::*;
use crate::{read_attempt::ReadAttemptContext, remote::ReadyFuture};

pub(super) fn bind(owner: &Unified, limit: usize, context: ReadAttemptContext) -> Unified {
    Unified {
        projects: owner
            .projects
            .iter()
            .map(|(key, project)| {
                (
                    key.clone(),
                    Project {
                        binding: project.binding.clone(),
                        name: project.name.clone(),
                        collection: project.collection.clone(),
                        backend: Arc::new(BoundNative {
                            backend: project.backend.clone(),
                            context: context.clone(),
                            limit,
                        }),
                    },
                )
            })
            .collect(),
        default: owner.default.clone(),
        explorer: owner.explorer.as_ref().map(|backend| {
            Arc::new(BoundExplorer {
                backend: backend.clone(),
                context: context.clone(),
            }) as Arc<dyn ExplorerTools>
        }),
        client: std::sync::Mutex::new(*owner.client.lock().unwrap()),
    }
}
struct BoundNative {
    backend: Arc<dyn RemoteTools>,
    context: ReadAttemptContext,
    limit: usize,
}
impl RemoteTools for BoundNative {
    fn scoped_selection(&self) -> Option<&airk_core::service::ScopedConnectRequest> {
        self.backend.scoped_selection()
    }
    fn combined_selection(&self) -> Option<&airk_core::service::CombinedConnectRequest> {
        self.backend.combined_selection()
    }
    fn query_concurrency(&self) -> usize {
        self.limit
    }
    fn catalog(&self) -> Vec<Tool> {
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
    fn prepare(&self) -> ReadyFuture<'_> {
        Box::pin(async move {
            self.context.require_live()?;
            tokio::select! {
                biased;
                _ = self.context.cancellation().cancelled() => Err(crate::read_attempt::ReadAttemptClosed.into()),
                _ = tokio::time::sleep_until(tokio::time::Instant::from_std(self.context.deadline())) => Err(crate::remote::PreparationExpired.into()),
                result = self.backend.prepare() => result,
            }
        })
    }
    fn call(&self, tool: String, arguments: Map<String, Value>) -> RemoteFuture<'_> {
        self.call_with_capacity(tool, arguments, self.limit)
    }
    fn call_with_capacity(
        &self,
        tool: String,
        arguments: Map<String, Value>,
        limit: usize,
    ) -> RemoteFuture<'_> {
        Box::pin(async move {
            self.context.require_live()?;
            self.backend
                .call_with_read_context(tool, arguments, limit, self.context.clone())
                .await
        })
    }
    fn call_with_read_context(
        &self,
        tool: String,
        arguments: Map<String, Value>,
        limit: usize,
        context: ReadAttemptContext,
    ) -> RemoteFuture<'_> {
        Box::pin(async move {
            context.require_live()?;
            self.context.require_live()?;
            self.backend
                .call_with_read_context(
                    tool,
                    arguments,
                    limit,
                    self.context.narrowed(context.deadline()),
                )
                .await
        })
    }
}
struct BoundExplorer {
    backend: Arc<dyn ExplorerTools>,
    context: ReadAttemptContext,
}
impl ExplorerTools for BoundExplorer {
    fn call(
        &self,
        project: Option<String>,
        tool: String,
        arguments: Map<String, Value>,
        client: airk_core::usage::ClientFamily,
    ) -> RemoteFuture<'_> {
        Box::pin(async move {
            self.context.require_live()?;
            tokio::select! {
                biased;
                _ = self.context.cancellation().cancelled() => Err(crate::read_attempt::ReadAttemptClosed.into()),
                _ = tokio::time::sleep_until(tokio::time::Instant::from_std(self.context.deadline())) => Err(crate::remote::PreparationExpired.into()),
                result = self.backend.call(project, tool, arguments, client) => result,
            }
        })
    }
}
