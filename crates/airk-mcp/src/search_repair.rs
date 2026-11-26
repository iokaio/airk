// SPDX-License-Identifier: Apache-2.0
//! Retain the ordinary Search result while the existing Verify owner works.
use crate::read_attempt::{
    ReadAttemptContext,
    lexical_repair::{LexicalRepairBounds, LexicalRepairEligibility},
};
use airk_core::{SearchFilters, SearchQuery};
use airk_kernel::{
    Kernel,
    current::{CurrentSearchMode, SearchWithDefinitionsResponse},
};
use anyhow::{Result, ensure};
use std::time::{Duration, Instant};

pub(crate) struct SearchRequest {
    pub(crate) query: SearchQuery,
    pub(crate) mode: CurrentSearchMode,
    pub(crate) snippet_bytes: Option<usize>,
    pub(crate) filters: SearchFilters,
    pub(crate) automatic_limit: bool,
}

impl SearchRequest {
    fn ordinary(&self, kernel: &Kernel) -> Result<SearchWithDefinitionsResponse> {
        kernel.search_response_with_adaptive_limit(
            &self.query,
            self.mode,
            self.snippet_bytes,
            &self.filters,
            self.automatic_limit,
        )
    }
}

pub(crate) fn search(
    kernel: Kernel,
    request: SearchRequest,
    context: Option<ReadAttemptContext>,
    runtime: tokio::runtime::Handle,
) -> Result<SearchWithDefinitionsResponse> {
    let Some(context) = context.filter(|context| !context.is_combined_member()) else {
        return request.ordinary(&kernel);
    };
    let Some(reserved) = context.deadline().checked_sub(Duration::from_millis(5)) else {
        return request.ordinary(&kernel);
    };
    let nomination = context.clone();
    let kernel = kernel.with_lexical_nomination_checkpoint(move || {
        nomination.check_delivery_live()?;
        ensure!(
            Instant::now() < reserved,
            "lexical nomination delivery reserve"
        );
        Ok(())
    });
    if !matches!(request.mode, CurrentSearchMode::Lexical)
        || request
            .query
            .path_prefix
            .as_ref()
            .is_none_or(String::is_empty)
        || request.filters != SearchFilters::default()
    {
        return request.ordinary(&kernel);
    }
    let Some(mut admission) = context.take_lexical_repair_admission() else {
        return request.ordinary(&kernel);
    };
    let Some(original_cancel) = context.lexical_repair_cancellation() else {
        return request.ordinary(&kernel);
    };
    let eligible = || LexicalRepairEligibility {
        root_known: true,
        pinned: false,
        composed: false,
        busy: false,
        cancelled: original_cancel.is_cancelled() || context.check_delivery_live().is_err(),
    };
    let Some((publication, settings, repair, replay)) = kernel.observed_lexical_repair_times()
    else {
        return request.ordinary(&kernel);
    };
    // The service already reserves a five-second delivery ceiling beyond the
    // ordinary read operation. Reserve it within this same hard deadline.
    let bounds = LexicalRepairBounds {
        repair,
        replay,
        delivery: Duration::from_secs(5),
    };
    let Some(mut attempt) = admission.admit(Instant::now(), eligible(), Some(bounds)) else {
        return request.ordinary(&kernel);
    };
    let (notify, completed) = tokio::sync::oneshot::channel();
    let (kernel, waiter) =
        kernel.with_lexical_repair_waiter(publication, attempt.wait_until(), move |notice| {
            if notice.settings_sha256 == settings {
                let _ = notify.send(notice);
            }
        });
    let ordinary = request.ordinary(&kernel)?;
    if !waiter.is_registered() {
        return Ok(ordinary);
    }
    let stop = context.cancellation().clone();
    let observed_end = Instant::now()
        .checked_add(bounds.repair)
        .unwrap_or(attempt.wait_until());
    let cutoff = tokio::time::Instant::from_std(observed_end.min(attempt.wait_until()));
    let notice = runtime.block_on(async {
        tokio::select! {
            biased;
            _ = stop.cancelled() => None,
            _ = original_cancel.cancelled() => None,
            _ = tokio::time::sleep_until(cutoff) => None,
            notice = completed => notice.ok(),
        }
    });
    drop(waiter);
    let Some(notice) = notice else {
        return Ok(ordinary);
    };
    if Instant::now() >= attempt.deadline() || !attempt.try_replay(Instant::now(), eligible()) {
        return Ok(ordinary);
    }

    // A late read cannot delay delivery of the available original result.
    // Keep the original request context on the task until native work settles.
    let fallback = ordinary.evidence.clone();
    let read_context = context.clone();
    let scope = airk_kernel::service_execution::ExecutionScope::current();
    let deadline = tokio::time::Instant::from_std(attempt.replay_until());
    let replay = runtime.spawn_blocking(move || {
        let _binding = scope.as_ref().map(|scope| scope.bind_thread());
        read_context.check_delivery_live().ok()?;
        kernel
            .replay_verified_lexical_repair(
                notice,
                &fallback,
                &request.query,
                request.snippet_bytes,
                &request.filters,
                request.automatic_limit,
            )
            .ok()
            .flatten()
    });
    let repaired = runtime.block_on(async {
        tokio::select! {
            biased;
            _ = stop.cancelled() => None,
            _ = original_cancel.cancelled() => None,
            _ = tokio::time::sleep_until(deadline) => None,
            result = replay => result.ok().flatten(),
        }
    });
    Ok(repaired.unwrap_or(ordinary))
}
