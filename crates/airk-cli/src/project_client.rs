// SPDX-License-Identifier: Apache-2.0
//! A query client obtains one grant, then communicates only with the project worker.
use crate::{
    local_ipc::{self, Stream},
    project_protocol::{self as protocol, Accepted, Query},
    service_runtime,
};
use airk_core::service::{ControlMethod, ControlResponse, PROTOCOL_VERSION, ProjectBinding};
use anyhow::{Result, ensure};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub struct Connection {
    stream: Stream,
}
impl Connection {
    pub async fn connect(root: &Path, project: ProjectBinding) -> Result<Self> {
        let ControlResponse::Health { observation } =
            service_runtime::request(root, ControlMethod::Health).await?
        else {
            anyhow::bail!("service health unavailable")
        };
        let ControlResponse::Connection { role, grant } =
            service_runtime::request(root, ControlMethod::Connect { project }).await?
        else {
            anyhow::bail!("project unavailable or busy; no direct-store fallback")
        };
        ensure!(
            grant.protocol == PROTOCOL_VERSION
                && grant.identity.service == observation.instance
                && grant.identity.project == project
                && grant.caller == local_ipc::caller_identity()?
                && role == protocol::role(grant.identity.worker),
            "brokered worker identity mismatch"
        );
        let mut stream = tokio::time::timeout(
            Duration::from_secs(5),
            local_ipc::connect(&local_ipc::endpoint(root, &role)?),
        )
        .await??;
        local_ipc::write_message(&mut stream, &grant, Duration::from_secs(5)).await?;
        let accepted: Accepted = local_ipc::read_message(&mut stream, Duration::from_secs(5))
            .await?
            .ok_or_else(|| anyhow::anyhow!("project rejected connection grant"))?;
        ensure!(
            accepted.identity == grant.identity,
            "direct worker identity mismatch"
        );
        Ok(Self { stream })
    }

    pub async fn call(
        &mut self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        local_ipc::write_message(
            &mut self.stream,
            &Query { tool, arguments },
            Duration::from_secs(5),
        )
        .await?;
        local_ipc::read_message(&mut self.stream, Duration::from_secs(120))
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("project query disconnected; retry through fresh brokerage")
            })
    }
}

struct Backend {
    root: PathBuf,
    project: ProjectBinding,
    connection: tokio::sync::Mutex<Option<Connection>>,
}
impl airk_mcp::remote::RemoteTools for Backend {
    fn call(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> airk_mcp::remote::RemoteFuture<'_> {
        Box::pin(async move {
            let mut connection = self.connection.lock().await;
            if connection.is_none() {
                *connection = Some(Connection::connect(&self.root, self.project).await?);
            }
            let result = connection
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("project connection unavailable"))?
                .call(tool, arguments)
                .await;
            if result.is_err() {
                *connection = None;
            }
            result
        })
    }
}

pub async fn bridge(root: PathBuf, project: ProjectBinding) -> Result<()> {
    airk_mcp::remote::serve(Arc::new(Backend {
        root,
        project,
        connection: tokio::sync::Mutex::new(None),
    }))
    .await
}
