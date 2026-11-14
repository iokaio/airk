// SPDX-License-Identifier: Apache-2.0
//! Native service callback; a public scope observation cannot construct input.
use airk_core::service::NativeNextReadRequest;
use airk_core::{
    arbiter::*,
    service::{ControlMethod, ControlResponse, ScopedReadRef, WorkerIdentity},
};
use airk_kernel::arbiter::NativeController;
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

struct Controller(PathBuf);
impl NativeController for Controller {
    fn context_action(
        &self,
        mut request: airk_core::native_context_action::NativeContextActionRequest,
        deadline: Instant,
    ) -> Result<Option<airk_core::arbiter::context_action::ContextActionAdvice>> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        request.remaining_ms = request
            .remaining_ms
            .min(remaining.as_millis().min(30_000) as u64);
        ensure!(
            request.remaining_ms > 0,
            "joint original deadline exhausted"
        );
        // Preserve the ORIGINAL request ID, clock and absolute expiry. A new
        // callback must not mint another attempt or replenish a global slot.
        let clock = if cfg!(windows) {
            airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
        } else {
            airk_core::read_attempt::ReadClock::UnixMonotonic
        };
        let original = request
            .read_attempt
            .remaining_at(clock, airk_process::monotonic::now_ns()?)?;
        request.read_attempt.remaining_ns = request
            .read_attempt
            .remaining_ns
            .min(original.as_nanos().min(remaining.as_nanos()) as u64);
        ensure!(
            request.read_attempt.remaining_ns > 0,
            "joint original attempt expired"
        );
        let worker = request.worker;
        let response = tokio::runtime::Handle::current().block_on(async {
            tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                airk_installer::control_client::request_for_version_and_instance_once(
                    &self.0,
                    ControlMethod::ArbiterContextAction {
                        request: Box::new(request),
                    },
                    env!("CARGO_PKG_VERSION"),
                    worker.service,
                ),
            )
            .await
            .context("joint original deadline exhausted in native transport")?
        })?;
        match response {
            ControlResponse::ArbiterContextActionAdvice { advice } => {
                Ok(advice.map(|advice| *advice))
            }
            ControlResponse::Failure { code } if code == "scoped_read_unavailable" => {
                Err(airk_core::scoped_reads::ScopeReadRefused.into())
            }
            _ => anyhow::bail!("joint native assessment unavailable"),
        }
    }
    fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        let response = tokio::runtime::Handle::current().block_on(
            airk_installer::control_client::request_for_version_and_instance(
                &self.0,
                ControlMethod::ArbiterApplication {
                    worker,
                    authority,
                    publication: publication.clone(),
                },
                env!("CARGO_PKG_VERSION"),
                worker.service,
            ),
        )?;
        match response {
            ControlResponse::ArbiterApplication { application } => Ok(*application),
            ControlResponse::Failure { code } if code == "scoped_read_unavailable" => {
                Err(airk_core::scoped_reads::ScopeReadRefused.into())
            }
            _ => anyhow::bail!("Arbiter native index application unavailable"),
        }
    }
    fn next_read(
        &self,
        mut request: NativeNextReadRequest,
        deadline: Instant,
    ) -> Result<Option<Advice>> {
        let remaining_ms = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(30_000) as u64;
        ensure!(remaining_ms > 0, "Arbiter original deadline exhausted");
        request.remaining_ms = remaining_ms;
        let monotonic = airk_process::monotonic::now_ns()?;
        let remaining_ns = deadline
            .saturating_duration_since(Instant::now())
            .as_nanos()
            .min(30_000_000_000) as u64;
        request.read_attempt = Some(airk_core::read_attempt::ReadAttemptBudget {
            schema: 1,
            request_id: airk_kernel::service_broker::new_instance_id()?,
            clock: if cfg!(windows) {
                airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
            } else {
                airk_core::read_attempt::ReadClock::UnixMonotonic
            },
            expires_monotonic_ns: monotonic
                .checked_add(remaining_ns)
                .context("Arbiter deadline overflow")?,
            remaining_ns,
            native_retry_expires_monotonic_ns: None,
        });
        let worker = request.worker;
        let response = tokio::runtime::Handle::current().block_on(async {
            tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                airk_installer::control_client::request_for_version_and_instance_once(
                    &self.0,
                    ControlMethod::ArbiterNextRead {
                        request: Box::new(request),
                    },
                    env!("CARGO_PKG_VERSION"),
                    worker.service,
                ),
            )
            .await
            .context("Arbiter original deadline exhausted in native transport")?
        })?;
        match response {
            ControlResponse::ArbiterAdvice { advice } => Ok(advice.map(|advice| *advice)),
            ControlResponse::Failure { code } if code == "scoped_read_unavailable" => {
                Err(airk_core::scoped_reads::ScopeReadRefused.into())
            }
            _ => anyhow::bail!("Arbiter native next-read unavailable"),
        }
    }
}
pub(super) fn controller(root: &Path) -> Result<Option<Arc<dyn NativeController>>> {
    // A callback is not authority. The service revalidates the redeemed worker
    // scope on every invocation, including its exact installed generation.
    let root = root
        .canonicalize()
        .context("Arbiter control root unavailable")?;
    Ok(Some(Arc::new(Controller(root))))
}
