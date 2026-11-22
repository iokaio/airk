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

/// Drive a synchronous control callback without nesting an async runtime entry.
/// The factory, including timers/IO resources, is created inside the selected
/// driver. No callback is retried and no work outlives the synchronous caller.
fn drive_control<F, Fut, T>(factory: F) -> Result<T>
where
    F: FnOnce() -> Fut + Send,
    Fut: std::future::Future<Output = Result<T>>,
    T: Send,
{
    fn local<F, Fut, T>(factory: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("Arbiter control IO driver unavailable")?;
        runtime.block_on(async move { factory().await })
    }
    match tokio::runtime::Handle::try_current() {
        Ok(handle)
            if matches!(
                handle.runtime_flavor(),
                tokio::runtime::RuntimeFlavor::MultiThread
            ) =>
        {
            // Production project owners have a two-worker multithread runtime.
            // Yield this worker's async duties while borrowing that SAME driver.
            tokio::task::block_in_place(|| handle.block_on(async move { factory().await }))
        }
        Ok(_) => std::thread::scope(|scope| {
            // block_in_place is forbidden on a current-thread runtime. A joined
            // scoped IO driver needs no ambient reactor and cannot detach work.
            let thread = std::thread::Builder::new()
                .name("airk-arbiter-control".into())
                .spawn_scoped(scope, move || local(factory))
                .context("Arbiter scoped control driver unavailable")?;
            match thread.join() {
                Ok(result) => result,
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }),
        Err(_) => local(factory),
    }
}

/// Clip one native transport wait to the ORIGINAL read's absolute deadline.
/// The wrapped request keeps its own five-second I1 ceiling and retry rules;
/// this only prevents the wait from outliving the read. A refusal or other
/// error from the work is returned unchanged.
async fn clipped<T>(
    deadline: Instant,
    what: &'static str,
    work: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    ensure!(
        deadline > Instant::now(),
        "{what} original deadline exhausted before native transport"
    );
    match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), work).await {
        Ok(result) => result,
        Err(_) => anyhow::bail!("{what} original deadline exhausted in native transport"),
    }
}

/// Narrow the caller's original same-host attempt; metadata callbacks cannot
/// mint a fresh request identity, absolute expiry or settings-retry window.
fn next_read_budget(
    attempt: Option<airk_core::read_attempt::ReadAttemptBudget>,
    remaining_ms: u64,
    deadline: Instant,
    sampled_at: Instant,
    clock: airk_core::read_attempt::ReadClock,
    now_ns: u64,
) -> Result<(airk_core::read_attempt::ReadAttemptBudget, u64)> {
    ensure!(
        (1..=120_000).contains(&remaining_ms),
        "invalid Arbiter remaining budget"
    );
    let mut attempt = attempt.context("native Arbiter original attempt budget required")?;
    let remaining = attempt
        .remaining_at(clock, now_ns)?
        .min(deadline.saturating_duration_since(sampled_at))
        .min(std::time::Duration::from_millis(remaining_ms));
    let remaining_ms = remaining.as_millis() as u64;
    ensure!(remaining_ms > 0, "Arbiter original deadline exhausted");
    attempt.remaining_ns = remaining.as_nanos() as u64;
    Ok((attempt, remaining_ms))
}

/// Narrow an Answer loop call to its caller's deadline and Answer's ceiling.
/// The ORIGINAL request ID, clock and absolute expiry are kept: a callback
/// never mints another attempt or replenishes a slot. Returns the narrowed
/// attempt and `remaining_ms`.
fn narrow_answer_call(
    mut attempt: airk_core::read_attempt::ReadAttemptBudget,
    remaining_ms: u64,
    remaining: std::time::Duration,
    clock: airk_core::read_attempt::ReadClock,
    now_ns: u64,
) -> Result<(airk_core::read_attempt::ReadAttemptBudget, u64)> {
    let remaining_ms = remaining_ms.min(remaining.as_millis().min(u128::from(
        airk_core::read_attempt::MAX_ANSWER_ARBITER_REMAINING_MS,
    )) as u64);
    ensure!(remaining_ms > 0, "Answer loop original deadline exhausted");
    let original = attempt.remaining_at(clock, now_ns)?;
    attempt.remaining_ns = attempt
        .remaining_ns
        .min(original.as_nanos().min(remaining.as_nanos()) as u64);
    ensure!(
        attempt.remaining_ns > 0,
        "Answer loop original attempt expired"
    );
    Ok((attempt, remaining_ms))
}

struct Controller(PathBuf);
impl NativeController for Controller {
    fn device_read_status(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
        read_attempt: airk_core::read_attempt::ReadAttemptBudget,
        deadline: Instant,
    ) -> Result<Option<device::ScopedDeviceStatus>> {
        let response = drive_control(|| {
            clipped(
                deadline,
                "scoped device status",
                airk_installer::control_client::request_for_version_and_instance(
                    &self.0,
                    ControlMethod::ArbiterDeviceRead {
                        worker,
                        authority,
                        publication: publication.clone(),
                        read_attempt,
                    },
                    env!("CARGO_PKG_VERSION"),
                    worker.service,
                ),
            )
        })?;
        match response {
            ControlResponse::ArbiterDeviceRead { observation } => Ok(Some(*observation)),
            ControlResponse::Failure { code } if code == "scoped_read_unavailable" => {
                Err(airk_core::scoped_reads::ScopeReadRefused.into())
            }
            ControlResponse::Failure { code } if code == "arbiter_catalog_required" => Ok(None),
            _ => anyhow::bail!("scoped device observation unavailable"),
        }
    }

    fn focus_clock(&self) -> Result<Option<(airk_core::read_attempt::ReadClock, u64)>> {
        let clock = if cfg!(windows) {
            airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
        } else {
            airk_core::read_attempt::ReadClock::UnixMonotonic
        };
        Ok(Some((clock, airk_process::monotonic::now_ns()?)))
    }
    fn focus_application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<Option<FocusApplication>> {
        let response = drive_control(|| async {
            airk_installer::control_client::request_for_version_and_instance_once(
                &self.0,
                ControlMethod::ArbiterFocusApplication {
                    worker,
                    authority,
                    publication: publication.clone(),
                },
                env!("CARGO_PKG_VERSION"),
                worker.service,
            )
            .await
        })?;
        match response {
            ControlResponse::ArbiterFocusApplication { application } => {
                Ok(application.map(|application| *application))
            }
            ControlResponse::Failure { code } if code == "scoped_read_unavailable" => {
                Err(airk_core::scoped_reads::ScopeReadRefused.into())
            }
            _ => anyhow::bail!("Arbiter Focus native application unavailable"),
        }
    }
    fn focus_application_before(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
        deadline: Instant,
    ) -> Result<Option<FocusApplication>> {
        let response = drive_control(|| {
            clipped(
                deadline,
                "Focus metadata",
                airk_installer::control_client::request_for_version_and_instance_once(
                    &self.0,
                    ControlMethod::ArbiterFocusApplication {
                        worker,
                        authority,
                        publication: publication.clone(),
                    },
                    env!("CARGO_PKG_VERSION"),
                    worker.service,
                ),
            )
        })?;
        match response {
            ControlResponse::ArbiterFocusApplication { application } => {
                Ok(application.map(|application| *application))
            }
            ControlResponse::Failure { code } if code == "scoped_read_unavailable" => {
                Err(airk_core::scoped_reads::ScopeReadRefused.into())
            }
            _ => anyhow::bail!("Arbiter Focus native application unavailable"),
        }
    }
    fn focus_rank(
        &self,
        mut request: airk_core::native_focus::NativeFocusRequest,
        deadline: Instant,
    ) -> Result<Option<airk_core::native_focus::NativeFocusResponse>> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        request.remaining_ms = request
            .remaining_ms
            .min(remaining.as_millis().min(120_000) as u64);
        ensure!(
            request.remaining_ms > 0,
            "Focus original deadline exhausted"
        );
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
            "Focus original attempt expired"
        );
        let worker = request.worker;
        let response = drive_control(|| async {
            tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                airk_installer::control_client::request_for_version_and_instance_once(
                    &self.0,
                    ControlMethod::ArbiterFocusRank {
                        request: Box::new(request),
                    },
                    env!("CARGO_PKG_VERSION"),
                    worker.service,
                ),
            )
            .await
            .context("Focus original deadline exhausted in native transport")?
        })?;
        match response {
            ControlResponse::ArbiterFocusRank { response } => {
                Ok(response.map(|response| *response))
            }
            ControlResponse::Failure { code } if code == "scoped_read_unavailable" => {
                Err(airk_core::scoped_reads::ScopeReadRefused.into())
            }
            _ => anyhow::bail!("Arbiter Focus native ranking unavailable"),
        }
    }
    fn context_action(
        &self,
        mut request: airk_core::native_context_action::NativeContextActionRequest,
        deadline: Instant,
    ) -> Result<Option<airk_core::arbiter::context_action::ContextActionAdvice>> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        // Context actions are Answer-only: Answer's ceiling, not Focus's 120 s.
        request.remaining_ms = request
            .remaining_ms
            .min(remaining.as_millis().min(u128::from(
                airk_core::read_attempt::MAX_ANSWER_ARBITER_REMAINING_MS,
            )) as u64);
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
        let response = drive_control(|| async {
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
    fn card_model(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<Option<airk_kernel::arbiter::LoopModel>> {
        let response = drive_control(|| async {
            airk_installer::control_client::request_for_version_and_instance_once(
                &self.0,
                ControlMethod::ArbiterCardModel {
                    worker,
                    authority,
                    publication: publication.clone(),
                },
                env!("CARGO_PKG_VERSION"),
                worker.service,
            )
            .await
        })?;
        match response {
            ControlResponse::ArbiterCardModel { model } => Ok(model
                .filter(|model| model.validate().is_ok())
                .map(|model| airk_kernel::arbiter::LoopModel {
                    model: model.model,
                    device: model.device,
                    precision: model.precision,
                })),
            ControlResponse::Failure { code } if code == "scoped_read_unavailable" => {
                Err(airk_core::scoped_reads::ScopeReadRefused.into())
            }
            ControlResponse::Failure { code } if code == "arbiter_catalog_required" => Ok(None),
            _ => anyhow::bail!("loop card model observation unavailable"),
        }
    }
    fn score_cards(
        &self,
        mut request: airk_core::native_loop::NativeCardRequest,
        deadline: Instant,
    ) -> Result<Option<airk_core::arbiter::answer_loop::CardAdvice>> {
        let clock = if cfg!(windows) {
            airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
        } else {
            airk_core::read_attempt::ReadClock::UnixMonotonic
        };
        (request.read_attempt, request.remaining_ms) = narrow_answer_call(
            request.read_attempt,
            request.remaining_ms,
            deadline.saturating_duration_since(Instant::now()),
            clock,
            airk_process::monotonic::now_ns()?,
        )?;
        let worker = request.worker;
        let response = drive_control(|| async {
            tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                airk_installer::control_client::request_for_version_and_instance_once(
                    &self.0,
                    ControlMethod::ArbiterScoreCards {
                        request: Box::new(request),
                    },
                    env!("CARGO_PKG_VERSION"),
                    worker.service,
                ),
            )
            .await
            .context("loop card original deadline exhausted in native transport")?
        })?;
        match response {
            ControlResponse::ArbiterCardAdvice { advice } => Ok(advice.map(|advice| *advice)),
            ControlResponse::Failure { code } if code == "scoped_read_unavailable" => {
                Err(airk_core::scoped_reads::ScopeReadRefused.into())
            }
            _ => anyhow::bail!("loop card native call unavailable"),
        }
    }
    fn focus_batch(
        &self,
        mut request: airk_core::native_loop::NativeFocusRequestV3,
        deadline: Instant,
    ) -> Result<Option<airk_core::native_loop::NativeFocusResponseV3>> {
        let clock = if cfg!(windows) {
            airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
        } else {
            airk_core::read_attempt::ReadClock::UnixMonotonic
        };
        (request.read_attempt, request.remaining_ms) = narrow_answer_call(
            request.read_attempt,
            request.remaining_ms,
            deadline.saturating_duration_since(Instant::now()),
            clock,
            airk_process::monotonic::now_ns()?,
        )?;
        let worker = request.worker;
        let response = drive_control(|| async {
            tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                airk_installer::control_client::request_for_version_and_instance_once(
                    &self.0,
                    ControlMethod::ArbiterFocusBatch {
                        request: Box::new(request),
                    },
                    env!("CARGO_PKG_VERSION"),
                    worker.service,
                ),
            )
            .await
            .context("batched Focus original deadline exhausted in native transport")?
        })?;
        match response {
            ControlResponse::ArbiterFocusBatch { response } => {
                Ok(response.map(|response| *response))
            }
            ControlResponse::Failure { code } if code == "scoped_read_unavailable" => {
                Err(airk_core::scoped_reads::ScopeReadRefused.into())
            }
            _ => anyhow::bail!("batched Focus native call unavailable"),
        }
    }
    fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        let response = drive_control(|| async {
            airk_installer::control_client::request_for_version_and_instance(
                &self.0,
                ControlMethod::ArbiterApplication {
                    worker,
                    authority,
                    publication: publication.clone(),
                },
                env!("CARGO_PKG_VERSION"),
                worker.service,
            )
            .await
        })?;
        match response {
            ControlResponse::ArbiterApplication { application } => Ok(*application),
            ControlResponse::Failure { code } if code == "scoped_read_unavailable" => {
                Err(airk_core::scoped_reads::ScopeReadRefused.into())
            }
            _ => anyhow::bail!("Arbiter native index application unavailable"),
        }
    }
    fn application_before(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
        deadline: Instant,
    ) -> Result<IndexApplication> {
        let response = drive_control(|| {
            clipped(
                deadline,
                "Arbiter application metadata",
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
            )
        })?;
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
        // Sample the local deadline before the host clock. Both independently
        // shorten the supplied original attempt, never establish a successor.
        let sampled_at = Instant::now();
        let (attempt, remaining_ms) = next_read_budget(
            request.read_attempt,
            request.remaining_ms,
            deadline,
            sampled_at,
            if cfg!(windows) {
                airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
            } else {
                airk_core::read_attempt::ReadClock::UnixMonotonic
            },
            airk_process::monotonic::now_ns()?,
        )?;
        request.read_attempt = Some(attempt);
        request.remaining_ms = remaining_ms;
        let worker = request.worker;
        let response = drive_control(|| async {
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

#[cfg(test)]
mod tests;
