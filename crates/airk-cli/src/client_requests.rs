// SPDX-License-Identifier: Apache-2.0
//! Bounded durable CLI requests. Uncertain work is retried only with its saved key.
//! The journal itself (file format, lock and limits) is `crate::request_journal`.
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse, JobKind, ProjectBinding, RequestKey},
};
use anyhow::{Result, ensure};
use std::path::Path;

pub use crate::request_journal::{Pending, forget, list};

/// A control root held as a `PathBuf` prepares as its `Path`.
impl crate::service_configuration::ConfigurationReads for std::path::PathBuf {
    fn epoch(&self) -> impl std::future::Future<Output = Result<(Incarnation, u64)>> + Send {
        self.as_path().epoch()
    }
    fn inspection(
        &self,
        project: ProjectBinding,
    ) -> impl std::future::Future<Output = Result<ControlResponse>> + Send {
        self.as_path().inspection(project)
    }
    fn administrative_read(
        &self,
        project: ProjectBinding,
        read: airk_core::native_reads::NativeRead,
    ) -> impl std::future::Future<Output = Result<serde_json::Value>> + Send {
        self.as_path().administrative_read(project, read)
    }
}
/// Configuration preparation's service calls for a control root, through the
/// CLI's requests (unchanged; see `ConfigurationReads`).
impl crate::service_configuration::ConfigurationReads for Path {
    fn epoch(&self) -> impl std::future::Future<Output = Result<(Incarnation, u64)>> + Send {
        request_epoch(self)
    }
    fn inspection(
        &self,
        project: ProjectBinding,
    ) -> impl std::future::Future<Output = Result<ControlResponse>> + Send {
        crate::service_runtime::request(self, ControlMethod::Inspection { project })
    }
    fn administrative_read(
        &self,
        project: ProjectBinding,
        read: airk_core::native_reads::NativeRead,
    ) -> impl std::future::Future<Output = Result<serde_json::Value>> + Send {
        crate::service_runtime::administrative_read(
            self,
            project,
            read,
            airk_core::usage::ClientFamily::Unknown,
        )
    }
}

/// Epoch acquisition precedes journal creation and submission. It may prune
/// terminal receipts, so this is deliberately not a general read retry helper.
pub(crate) async fn request_epoch(control: &Path) -> Result<(Incarnation, u64)> {
    acquire_epoch(
        || crate::console_client::request(control, ControlMethod::RequestEpoch),
        std::time::Duration::from_secs(15),
    )
    .await
}

async fn acquire_epoch<F, Fut>(
    mut request: F,
    budget: std::time::Duration,
) -> Result<(Incarnation, u64)>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<ControlResponse>>,
{
    let until = tokio::time::Instant::now() + budget;
    let mut last = None;
    loop {
        let response = match tokio::time::timeout_at(until, request()).await {
            Ok(response) => response?,
            Err(_) => anyhow::bail!(
                "request epoch deadline exceeded; last response: {}",
                last.as_deref().unwrap_or("none")
            ),
        };
        match response {
            ControlResponse::RequestEpoch { namespace, epoch } => return Ok((namespace, epoch)),
            ControlResponse::Failure { code }
                if code == "registry_response_unknown_retry_with_key" =>
            {
                // No job/configuration request or nonce exists yet. Reacquire
                // current authority; never replay the subsequent mutation here.
                // Exact registry_busy already exhausted the shared native
                // retry budget and must not start another acquisition window.
                last = Some(code);
                tokio::time::sleep_until(
                    until.min(tokio::time::Instant::now() + std::time::Duration::from_millis(50)),
                )
                .await;
                ensure!(
                    tokio::time::Instant::now() < until,
                    "request epoch deadline exceeded; last response: {}",
                    last.as_deref().unwrap()
                );
            }
            response => anyhow::bail!(
                "request epoch unavailable: {}",
                serde_json::to_string(&response)?
            ),
        }
    }
}
pub async fn prepare(
    control: &Path,
    project: ProjectBinding,
    kind: JobKind,
) -> Result<Incarnation> {
    let control = control.canonicalize()?;
    let (namespace, epoch) = request_epoch(&control).await?;
    crate::request_journal::persist(
        &control,
        ControlMethod::Enqueue {
            project,
            kind,
            caller: airk_kernel::service_broker::new_instance_id()?,
            key: RequestKey {
                namespace,
                epoch,
                nonce: airk_kernel::service_broker::new_instance_id()?,
            },
        },
    )
}
pub async fn prepare_configuration(
    control: &Path,
    project: ProjectBinding,
    draft: crate::service_configuration::Draft,
) -> Result<Incarnation> {
    let control = control.canonicalize()?;
    let request = crate::service_configuration::prepare(&control, project, draft).await?;
    let (namespace, epoch) = request_epoch(&control).await?;
    crate::request_journal::persist(
        &control,
        ControlMethod::ConfigureProject {
            project,
            request: Box::new(request),
            caller: airk_kernel::service_broker::new_instance_id()?,
            key: RequestKey {
                namespace,
                epoch,
                nonce: airk_kernel::service_broker::new_instance_id()?,
            },
        },
    )
}
pub fn refusal_guidance(code: &str) -> Option<&'static str> {
    (code == "source_change_review_required").then_some(
        "Review the Index's sources or retirement in Indexing, or prepare a CatalogSourceMutationDraft through 'airk service rpc'",
    )
}

/// M7: submit the saved request by nonce with its exact key and return the
/// service's answer (`Job` or `Failure`); a crossed receipt is refused.
pub(crate) async fn submit_response(control: &Path, nonce: Incarnation) -> Result<ControlResponse> {
    let (control, request, project, kind) = crate::request_journal::saved(control, nonce)?;
    let response = crate::console_client::request(&control, request).await?;
    crate::request_journal::check_receipt(&response, project, kind)?;
    Ok(response)
}

pub async fn submit(control: &Path, nonce: Incarnation) -> Result<ControlResponse> {
    let response = submit_response(control, nonce).await?;
    match &response {
        ControlResponse::Job { .. } => {}
        ControlResponse::Failure { code } => {
            if let Some(guidance) = refusal_guidance(code) {
                anyhow::bail!("native request refused: {code}; {guidance}; saved key is preserved")
            }
            anyhow::bail!("native request refused: {code}; saved key is preserved")
        }
        _ => anyhow::bail!("native request outcome unknown; saved key is preserved"),
    }
    Ok(response)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_acquisition_retries_only_unknown_epoch_recovery() -> Result<()> {
        tokio::runtime::Runtime::new()?.block_on(async {
            let namespace = Incarnation::from_bytes([7; 16]);
            let mut responses = vec![
                ControlResponse::Failure {
                    code: "registry_response_unknown_retry_with_key".into(),
                },
                ControlResponse::RequestEpoch {
                    namespace,
                    epoch: 42,
                },
            ]
            .into_iter();
            let result = acquire_epoch(
                || std::future::ready(Ok(responses.next().expect("unexpected replay"))),
                std::time::Duration::from_secs(2),
            )
            .await?;
            assert_eq!(result, (namespace, 42));
            assert!(responses.next().is_none());
            for code in [
                "registry_busy",
                "registry_unavailable",
                "invalid_registry_request",
                "service_rebind_required",
                "service_request_outcome_unknown",
            ] {
                let mut calls = 0;
                let error = acquire_epoch(
                    || {
                        calls += 1;
                        std::future::ready(Ok(ControlResponse::Failure { code: code.into() }))
                    },
                    std::time::Duration::from_secs(2),
                )
                .await
                .unwrap_err();
                assert_eq!(calls, 1, "must not retry {code}");
                assert!(error.to_string().contains(code), "{error}");
            }
            let mut calls = 0;
            let error = acquire_epoch(
                || {
                    calls += 1;
                    std::future::ready(Err(anyhow::anyhow!("transport fixture failure")))
                },
                std::time::Duration::from_secs(2),
            )
            .await
            .unwrap_err();
            assert_eq!(calls, 1);
            assert!(error.to_string().contains("transport fixture failure"));
            Ok(())
        })
    }

    #[test]
    fn epoch_acquisition_stops_exhausted_busy_and_bounds_unknown_or_blocked_requests() -> Result<()>
    {
        tokio::runtime::Runtime::new()?.block_on(async {
            let mut calls = 0;
            let error = acquire_epoch(
                || {
                    calls += 1;
                    std::future::ready(Ok(ControlResponse::Failure {
                        code: "registry_busy".into(),
                    }))
                },
                std::time::Duration::from_millis(20),
            )
            .await
            .unwrap_err();
            assert_eq!(calls, 1);
            assert!(error.to_string().contains("request epoch unavailable"));
            assert!(error.to_string().contains("registry_busy"));
            let error = acquire_epoch(
                || {
                    std::future::ready(Ok(ControlResponse::Failure {
                        code: "registry_response_unknown_retry_with_key".into(),
                    }))
                },
                std::time::Duration::from_millis(20),
            )
            .await
            .unwrap_err();
            assert!(error.to_string().contains("deadline exceeded"));
            assert!(
                error
                    .to_string()
                    .contains("registry_response_unknown_retry_with_key")
            );
            let error = acquire_epoch(
                std::future::pending::<Result<ControlResponse>>,
                std::time::Duration::from_millis(20),
            )
            .await
            .unwrap_err();
            assert!(error.to_string().contains("deadline exceeded"));
            Ok(())
        })
    }
}
