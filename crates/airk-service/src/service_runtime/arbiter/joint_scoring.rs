// SPDX-License-Identifier: Apache-2.0
//! One authenticated joint assessment; unknown native work keeps its budget.
use super::*;
use airk_core::{
    arbiter::context_action::ContextActionAdvice, native_context_action::NativeContextActionRequest,
};

#[cfg(any(feature = "local-inference", test))]
fn observe_work(
    advice: &mut ContextActionAdvice,
    digest: airk_core::identity::Sha256Digest,
    tokens: usize,
    queue_us: u64,
    compile_us: u64,
    inference_us: Option<u64>,
) {
    advice.input_digest = digest;
    advice.observed_input_tokens = Some(tokens);
    advice.work.input_tokens = tokens;
    advice.work.queue_us = Some(queue_us);
    advice.work.compile_us = Some(compile_us);
    advice.work.inference_us = inference_us;
}

#[cfg(any(feature = "local-inference", test))]
fn retain_late_scope_refusal(advice: &mut ContextActionAdvice, checked: Result<()>) -> bool {
    if checked.is_ok() {
        return false;
    }
    // Private native custody retains measurements, never a stale score/action.
    // Canonical source/selection owners still refuse external publication.
    advice.outcome=Outcome::Unavailable {reason:"Original source authority changed after native work; the decision was discarded and actual work is retained as a private failed receipt.".into()};
    true
}
impl Owner {
    pub(in crate::service_runtime) async fn context_action(
        &self,
        request: NativeContextActionRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<Option<ContextActionAdvice>> {
        let started = Instant::now();
        let clock = if cfg!(windows) {
            airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
        } else {
            airk_core::read_attempt::ReadClock::UnixMonotonic
        };
        let catalog_clock = self
            .gate
            .catalog_clock()
            .context("joint catalog clock unavailable")?;
        ensure!(
            catalog_clock.namespace == self.namespace,
            airk_core::scoped_reads::ScopeReadRefused
        );
        let issue = self
            .gate
            .capture_current_issue(catalog_clock.namespace, catalog_clock.revision)?;
        // The actual native installed lease is checked under this gate before
        // structural request validation; equal untrusted fields grant nothing.
        let remaining =
            self.gate
                .with_scope_issue(issue, request.worker, request.authority, || {
                    request.validate(
                        request.worker,
                        request.authority,
                        clock,
                        airk_process::monotonic::now_ns()?,
                    )
                })??;
        let deadline = started
            .checked_add(remaining)
            .context("joint assessment deadline overflow")?;
        let application = self.application(
            request.worker,
            request.authority,
            request.scope.publication.clone(),
        )?;
        ensure!(
            application.scope == request.scope,
            "joint assessment source identity changed"
        );
        let configuration = self.configuration()?;
        if configuration.revision != request.scope.configuration_revision
            || !configuration.eligible(request.scope.index_id, Task::ContextAction)
            || application.availability != Availability::Ready
        {
            return Ok(None);
        }
        #[cfg(not(feature = "local-inference"))]
        {
            let _ = (supervisor, deadline);
            Ok(None)
        }
        #[cfg(feature = "local-inference")]
        {
            use airk_core::{arbiter::context_action::ADVICE_SCHEMA_V2, identity::framed_digest};
            use airk_inference::inference_engine::pool::arbiter::{
                CompiledReceipt, KnownScoreFailure, ScoreRequest,
            };
            use airk_retrieval::arbiter::context_action;
            let execution = self.execution_profile(
                configuration
                    .selected_profile
                    .context("joint selected profile missing")?,
            )?;
            let profile = execution.description().clone();
            let task_policy = context_action::policy_digest()?;
            if !profile.tasks.contains(&Task::ContextAction)
                || profile.task_policies.get(&Task::ContextAction) != Some(&task_policy)
            {
                return Ok(None);
            }
            ensure!(
                request.roster.policy == task_policy,
                "joint roster task policy changed"
            );
            let handle = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                supervisor.arbiter(),
            )
            .await
            .context("joint native owner deadline")??;
            let calibration = handle.joint_calibration(profile.manifest)?;
            calibration.validate(&profile, task_policy)?;
            let prompt = context_action::prompt(&request)?;
            let planned_digest = framed_digest(
                b"airk.arbiter.planned-task-input/1",
                &[&serde_json::to_vec(&prompt)?],
            );
            let mut advice=ContextActionAdvice { schema:ADVICE_SCHEMA_V2.into(),task:Task::ContextAction,
                profile:profile.manifest,runtime:profile.runtime.clone(),compiler:profile.compiler,
                calibration:profile.calibration,policy:profile.policy,task_policy,scope:request.scope.clone(),
                input_digest:planned_digest,evidence:request.roster.basis.source_units_presented.iter().map(|unit|unit.record).collect::<std::collections::BTreeSet<_>>().into_iter().collect(),
                work:Work::default(),reserved_input_tokens:request.remaining_input_tokens,observed_input_tokens:None,
                outcome:Outcome::Unavailable {reason:"The joint native outcome is unknown; its original token/ordinal reservation remains held. Canonical gathering remains available.".into()} };
            let mut ledger_attempt = request.read_attempt;
            ledger_attempt.remaining_ns = ledger_attempt.remaining_ns.min(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_nanos() as u64,
            );
            let ticket = self
                .joint_ledger
                .lock()
                .map_err(|_| anyhow::anyhow!("joint NN ledger unavailable"))?
                .reserve(
                    ledger_attempt,
                    &request.parent_budget,
                    request.ordinal,
                    request.remaining_input_tokens,
                    request.partition.as_ref(),
                    airk_process::monotonic::now_ns()?,
                )?;
            let sampled = Instant::now();
            let ledger_remaining =
                Duration::from_nanos(ticket.remaining_ns(airk_process::monotonic::now_ns()?));
            ensure!(
                !ledger_remaining.is_zero(),
                "joint shared assessment deadline exhausted"
            );
            let deadline = deadline.min(
                sampled
                    .checked_add(ledger_remaining)
                    .context("joint ledger deadline overflow")?,
            );
            // Work is a known lower bound; observed_input_tokens=None records
            // unknown compile work separately from the held reservation.
            let gate = self.gate.clone();
            let store = self.configuration.clone();
            let worker = request.worker;
            let authority = request.authority;
            let manifest = profile.manifest;
            let live = Arc::new(move |scope: &ScopeBinding| {
                store.load().is_ok_and(|current| {
                    current.revision == scope.configuration_revision
                        && current.installation == scope.installation
                        && current.catalog == scope.catalog
                        && current.selected_profile == Some(manifest)
                        && current.eligible(scope.index_id, Task::ContextAction)
                }) && gate.catalog_clock().is_some_and(|clock| {
                    clock.namespace == scope.catalog
                        && gate
                            .capture_current_issue(clock.namespace, clock.revision)
                            .is_ok_and(|ticket| {
                                gate.check_scope_issue(ticket, worker, authority).is_ok()
                            })
                })
            });
            let pending = async {
                let resident = handle.resident(profile.manifest)?;
                let admission = supervisor.reuse_optional(resident.reservation()).await?;
                handle.score(ScoreRequest {
                    worker: request.worker.worker,
                    scope: request.scope.clone(),
                    task: Task::ContextAction,
                    deadline,
                    authority: live,
                    state: prompt.state,
                    question: prompt.question,
                    candidates: prompt.candidates,
                    resident,
                    admission,
                    max_input_tokens: request.remaining_input_tokens,
                })
            };
            let mut pending =
                match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), pending)
                    .await
                {
                    Ok(Ok(pending)) => pending,
                    _ => return Ok(Some(advice)),
                };
            let compiled: Option<CompiledReceipt> = match tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                &mut pending.compiled,
            )
            .await
            {
                Ok(Ok(receipt)) => Some(receipt),
                _ => {
                    pending.compiled.close();
                    pending.compiled.try_recv().ok()
                }
            };
            if let Some(receipt) = compiled {
                observe_work(
                    &mut advice,
                    receipt.input_digest,
                    receipt.input_tokens,
                    receipt.queue_us,
                    receipt.compile_us,
                    None,
                );
                if self
                    .joint_ledger
                    .lock()
                    .map_err(|_| anyhow::anyhow!("joint NN ledger unavailable"))
                    .and_then(|mut ledger| ledger.compiled(ticket, receipt.input_tokens))
                    .is_err()
                {
                    advice.outcome=Outcome::Unavailable {reason:"Known compiled input exceeded or contradicted its reservation; native execution is refused and the original reservation remains held.".into()};
                    advice.validate()?;
                    return Ok(Some(advice));
                }
            }
            let completion = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                &mut pending.completed,
            )
            .await;
            if let Ok(Ok(Ok(scored))) = &completion {
                observe_work(
                    &mut advice,
                    scored.input_digest,
                    scored.input_tokens,
                    scored.queue_us,
                    scored.compile_us,
                    Some(scored.inference_us),
                );
                if self
                    .joint_ledger
                    .lock()
                    .map_err(|_| anyhow::anyhow!("joint NN ledger unavailable"))
                    .and_then(|mut ledger| ledger.completed(ticket, scored.input_tokens))
                    .is_err()
                {
                    advice.outcome=Outcome::Unavailable {reason:"Observed native work contradicted its reservation; actual measurements are retained without applying a decision.".into()};
                    advice.validate()?;
                    return Ok(Some(advice));
                }
                let policy_started = Instant::now();
                advice.outcome=match context_action::selected_action(&scored.scores,&request.roster,&calibration) {
                    Ok(Some(card))=>Outcome::Accepted {decision:Decision::ContextAction(card)},
                    Ok(None)=>Outcome::Abstained {reason:"The complete global top card was tied, unsupported, or below its independently fitted raw-score margin.".into()},
                    Err(_)=>Outcome::InvalidInput {reason:"Known native scoring completed, but its score shape, finiteness or whole-card policy was invalid; measured work is retained without a decision.".into()},
                };
                advice.work.policy_us =
                    Some(policy_started.elapsed().as_micros().min(u64::MAX as u128) as u64);
            } else if let Ok(Ok(Err(error))) = &completion
                && let Some(failure) = error.downcast_ref::<KnownScoreFailure>()
            {
                let receipt = &failure.receipt;
                observe_work(
                    &mut advice,
                    receipt.input_digest,
                    receipt.input_tokens,
                    receipt.queue_us,
                    receipt.compile_us,
                    Some(receipt.inference_us),
                );
                let reconciled = self
                    .joint_ledger
                    .lock()
                    .map_err(|_| anyhow::anyhow!("joint NN ledger unavailable"))
                    .and_then(|mut ledger| ledger.completed(ticket, receipt.input_tokens))
                    .is_ok();
                advice.outcome=Outcome::Unavailable {reason:if !reconciled {
                    "Known failed native work contradicted its reservation; measured work is retained and the original hold remains."
                } else if failure.source_delivery_refused {
                    "Native work completed, but original delivery admission changed; measured work is retained without a decision."
                } else {"Native scoring returned a known failure; actual completed work is retained without a decision."}.into()};
            }
            if retain_late_scope_refusal(
                &mut advice,
                self.validate_scope(request.worker, request.authority),
            ) {
                advice.validate()?;
                return Ok(Some(advice));
            }
            if !self.configuration().is_ok_and(|current| {
                current.revision == request.scope.configuration_revision
                    && current.selected_profile == Some(profile.manifest)
                    && current.eligible(request.scope.index_id, Task::ContextAction)
            }) {
                advice.outcome=Outcome::Unavailable {reason:"Joint configuration changed; native work remains charged and the decision was discarded.".into()};
            }
            advice.validate()?;
            Ok(Some(advice))
        }
    }
}
#[cfg(test)]
mod tests;
