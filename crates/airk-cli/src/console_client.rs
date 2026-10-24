// SPDX-License-Identifier: Apache-2.0
//! Trusted console metadata over bounded parent pipes; never an MCP/evidence proxy.
use crate::{runtime_pair, service_ipc, service_runtime};
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: Incarnation,
    pub method: ControlMethod,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub id: Incarnation,
    pub response: ControlResponse,
}
fn failure(code: &str) -> ControlResponse {
    ControlResponse::Failure { code: code.into() }
}
fn metadata_only(response: ControlResponse) -> ControlResponse {
    match response {
        ControlResponse::Connection { .. } => failure("console_brokerage_forbidden"),
        response => response,
    }
}
async fn handle(
    root: &Path,
    method: ControlMethod,
    engine: &str,
    runtime: Option<&str>,
) -> ControlResponse {
    // The console never receives an evidence bearer credential. Evidence
    // brokerage remains the dedicated direct-project client/stdio MCP path.
    match &method {
        ControlMethod::Connect { .. } => return failure("console_brokerage_forbidden"),
        // Exhaustive review: a new service method must be considered here before
        // a trusted console can expose it. No default credential-bearing path.
        ControlMethod::Logs { .. }
        | ControlMethod::Health
        | ControlMethod::Shutdown
        | ControlMethod::Register { .. }
        | ControlMethod::ReserveProject { .. }
        | ControlMethod::Projects { .. }
        | ControlMethod::RequestEpoch
        | ControlMethod::PlanCreate { .. }
        | ControlMethod::Plan { .. }
        | ControlMethod::PlanItems { .. }
        | ControlMethod::PlanExecute { .. }
        | ControlMethod::PlanPause { .. }
        | ControlMethod::PlanCancel { .. }
        | ControlMethod::Events { .. }
        | ControlMethod::Maintenance { .. }
        | ControlMethod::Watch { .. }
        | ControlMethod::Progress { .. }
        | ControlMethod::Inspection { .. }
        | ControlMethod::ConfigureWatch { .. }
        | ControlMethod::ConfigureProject { .. }
        | ControlMethod::ConfigurationResult { .. }
        | ControlMethod::Enqueue { .. }
        | ControlMethod::Job { .. }
        | ControlMethod::Retry { .. }
        | ControlMethod::Pause { .. }
        | ControlMethod::Cancel { .. }
        | ControlMethod::CancelPending { .. }
        | ControlMethod::Console { .. }
        | ControlMethod::Workers { .. }
        | ControlMethod::Worker { .. }
        | ControlMethod::RestartProject { .. }
        | ControlMethod::EmbeddingStatus
        | ControlMethod::StartEmbedding
        | ControlMethod::StopEmbedding { .. }
        | ControlMethod::SourceStatus
        | ControlMethod::Resources
        | ControlMethod::ConfigureResources { .. } => {}
    }
    let health = service_runtime::request(root, ControlMethod::Health).await;
    let observation = match health {
        Ok(ControlResponse::Health { observation }) => observation,
        Ok(ControlResponse::Incompatible { .. }) => {
            return failure("service_protocol_incompatible");
        }
        _ => return failure("service_unavailable"),
    };
    if observation.version != env!("CARGO_PKG_VERSION")
        || observation.executable_sha256 != engine
        || runtime.is_some_and(|expected| observation.runtime_sha256.as_deref() != Some(expected))
    {
        return failure("service_rebind_required");
    }
    if matches!(method, ControlMethod::Health) {
        return ControlResponse::Health { observation };
    }
    service_runtime::request_for_instance(root, method, observation.instance)
        .await
        .unwrap_or_else(|_| failure("service_request_outcome_unknown"))
}
/// Same-version/engine and instance-fenced metadata call for native frontends.
/// No connection grant or evidence response can leave through this adapter.
pub async fn request(root: &Path, method: ControlMethod) -> Result<ControlResponse> {
    let engine = runtime_pair::current_executable_sha256()?;
    Ok(metadata_only(handle(root, method, &engine, None).await))
}

pub async fn run(root: &Path, runtime: Option<airk_core::identity::Sha256Digest>) -> Result<()> {
    let runtime = runtime.map(|digest| digest.to_string());
    let engine = runtime_pair::digest_file(&std::env::current_exe()?)?;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    std::thread::Builder::new()
        .name("airk-console-input".into())
        .spawn(move || {
            let mut input = std::io::stdin().lock();
            loop {
                let request = service_ipc::read_frame::<_, Request>(&mut input);
                let done = !matches!(request, Ok(Some(_)));
                if sender.blocking_send(request).is_err() || done {
                    break;
                }
            }
        })?;
    while let Some(request) = receiver.recv().await {
        let Some(request) = request? else {
            break;
        };
        let response = Response {
            id: request.id,
            response: metadata_only(
                handle(root, request.method, &engine, runtime.as_deref()).await,
            ),
        };
        let mut bytes = Vec::new();
        if service_ipc::write_frame(&mut bytes, &response).is_err() {
            bytes.clear();
            service_ipc::write_frame(
                &mut bytes,
                &Response {
                    id: request.id,
                    response: failure("console_response_oversize"),
                },
            )?;
        }
        ensure!(
            bytes.len() <= airk_core::service::MAX_FRAME_BYTES + 4,
            "console response budget exceeded"
        );
        let mut output = std::io::stdout().lock();
        output.write_all(&bytes)?;
        output.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unexpected_broker_response_cannot_put_a_bearer_on_the_console_pipe() {
        use airk_core::{
            identity::Sha256Digest,
            service::{ConnectionGrant, WorkerIdentity},
        };
        let token = Sha256Digest::from_bytes([0xaa; 32]);
        let response = metadata_only(ControlResponse::Connection {
            role: "original-private-fixture".into(),
            grant: ConnectionGrant {
                client: airk_core::usage::ClientFamily::Unknown,
                protocol: 1,
                identity: WorkerIdentity {
                    service: Incarnation::from_bytes([1; 16]),
                    worker: Incarnation::from_bytes([2; 16]),
                    project: Sha256Digest::from_bytes([3; 32]),
                    fencing_generation: 1,
                },
                caller: Incarnation::from_bytes([4; 16]),
                token,
                ttl_ms: 30000,
            },
        });
        let serialized = serde_json::to_string(&response).unwrap();
        assert!(serialized.contains("console_brokerage_forbidden"));
        assert!(!serialized.contains(&token.to_string()));
        assert!(!serialized.contains("grant"));
        assert!(matches!(
            metadata_only(ControlResponse::Stopping),
            ControlResponse::Stopping
        ));
    }
}
