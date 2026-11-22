// SPDX-License-Identifier: Apache-2.0
//! Answer /3 batched Focus: one call over several sub-need questions. Each
//! pair is still its own lane job, so a per-candidate question costs no
//! extra model work: the pair job simply carries its own question (plan
//! 12.1 and the L4 decision recorded in the service CONTRACT.md). The call
//! takes one Focus ordinal and one reservation in the loop ledger, charged
//! to each part and round for its own pairs.
use super::*;
use airk_core::{
    arbiter::answer_loop::{FOCUS_BATCH_SCHEMA, FocusBatchReceipt, FocusPairScore},
    native_loop::{NativeFocusRequestV3, NativeFocusResponseV3},
};

const UNKNOWN: &str = "Native batched Focus outcome is unknown; the reservation remains held and canonical order is kept.";
const APPLIED: &str = "Complete native Focus batch ranked by relevance; each pair was scored against its own sub-need question.";
const CHANGED: &str = "Focus deadline, source authority or configuration changed; measured work is retained without ranking.";
const NOT_QUEUED: &str = "The native lane queued no pair of this batch; no model work ran and the reservation was released.";
const FAILED: &str =
    "A native Focus pair failed or returned invalid logits; no partial ranking was applied.";

impl StageOwner {
    pub(super) async fn focus_batch(
        &self,
        request: NativeFocusRequestV3,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<Option<NativeFocusResponseV3>> {
        let started = Instant::now();
        let clock = if cfg!(windows) {
            airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
        } else {
            airk_core::read_attempt::ReadClock::UnixMonotonic
        };
        let catalog = self
            .gate
            .catalog_clock()
            .context("Focus catalog unavailable")?;
        ensure!(
            catalog.namespace == self.namespace,
            airk_core::scoped_reads::ScopeReadRefused
        );
        let issue = self
            .gate
            .capture_current_issue(catalog.namespace, catalog.revision)?;
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
            .context("Focus batch deadline overflow")?;
        let application = self.application(
            request.worker,
            request.authority,
            request.scope.publication.clone(),
        )?;
        ensure!(
            application.scope == request.scope,
            "Focus batch source publication changed"
        );
        let configuration = self.read_configuration()?;
        if configuration.revision() != request.scope.configuration_revision
            || !configuration.eligible_for(Stage::Focus, request.scope.index_id, Task::FocusRank)
            || application.availability != Availability::Ready
            || request.score_count == 0
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
            use airk_inference::inference_engine::pool::arbiter::{
                KnownScoreFailure, LaneKey, ScoreRequest,
            };
            use airk_retrieval::arbiter::compiler::Candidate;
            let profile = self.profile_description(
                configuration
                    .selected_profile(Stage::Focus)
                    .context("Focus selected profile absent")?,
            )?;
            profile.validate_stage(Stage::Focus)?;
            let native_execution = if profile.device_execution.is_none() {
                Some(self.execution_profile(profile.manifest)?)
            } else {
                None
            };
            let task_policy =
                focus_scoring::validated_focus_task_policy(&profile, native_execution.as_ref())?;
            let pair_tokens = profile.max_tokens;
            ensure!(
                request.score_count <= profile.max_candidates
                    && pair_tokens
                        .checked_mul(request.score_count)
                        .is_some_and(|reserved| reserved <= request.remaining_input_tokens),
                "Focus batch exceeds the qualified profile or its reservation"
            );
            let mut original = request.read_attempt;
            original.remaining_ns = original.remaining_ns.min(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_nanos()
                    .min(u64::MAX as u128) as u64,
            );
            let ticket = self
                .loop_ledger
                .lock()
                .map_err(|_| anyhow::anyhow!("loop NN ledger unavailable"))?
                .reserve_focus(
                    original,
                    &request.parent_budget,
                    loop_ledger::FocusCall {
                        index_id: request.scope.index_id,
                        ordinal: request.ordinal,
                        pairs: request.score_count,
                    },
                    &request.attribution,
                    request.partition.as_ref(),
                    airk_process::monotonic::now_ns()?,
                )?;
            let deadline = deadline.min(
                Instant::now()
                    .checked_add(Duration::from_nanos(
                        ticket.remaining_ns(airk_process::monotonic::now_ns()?),
                    ))
                    .context("Focus ledger deadline overflow")?,
            );
            let count = request.candidates.len();
            let mut receipt = FocusBatchReceipt {
                schema: FOCUS_BATCH_SCHEMA.into(),
                scope_digest: airk_core::arbiter::answer_loop::scope_digest(&request.scope)?,
                identity: Some(FocusIdentity {
                    profile: profile.manifest,
                    runtime: profile.advice_runtime()?,
                    compiler: profile.compiler,
                    calibration: profile.calibration,
                    // The batch's own identity: the pair policy plus the
                    // batched ranking and Core loop semantics.
                    task_policy: airk_retrieval::arbiter::loop_focus::policy_digest(task_policy)?,
                }),
                availability: Availability::Failed,
                reason: UNKNOWN.into(),
                applied: false,
                input_digest: request.input_digest()?,
                order_digest: request.order_digest,
                questions_digest: airk_core::arbiter::answer_loop::questions_digest(
                    &request.questions,
                )?,
                question_count: u8::try_from(request.questions.len())?,
                candidate_count: count,
                ranked: (0..count).map(|index| index as u16).collect(),
                pairs: vec![],
                reserved_input_tokens: request.remaining_input_tokens,
                observed_input_tokens: None,
                attribution: request.attribution.clone(),
                work: Work::default(),
            };
            let pending_handle = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                supervisor.arbiter_management().existing(),
            )
            .await;
            let Ok(Ok(handle)) = pending_handle else {
                return batch_response(receipt, &request);
            };
            let live = self.focus_authority(&request, profile.manifest)?;
            // Every part the batch charges is held by its pair jobs.
            let lane = LaneKey::answer(
                request.read_attempt.request_id,
                request.attribution.iter().map(|charge| charge.part),
            );
            let mut tokens = 0usize;
            let mut queue_us = 0u64;
            let mut compile_us = 0u64;
            let mut inference_us = 0u64;
            let mut settled = true;
            let mut failed = false;
            let mut refused = false;
            for (index, candidate) in request
                .candidates
                .iter()
                .take(request.score_count)
                .enumerate()
            {
                if Instant::now() >= deadline || !live(&request.scope) {
                    break;
                }
                let question = request
                    .questions
                    .get(usize::from(candidate.question))
                    .context("Focus candidate names an unknown question")?;
                let pending = async {
                    let resident = handle.resident(profile.manifest)?;
                    let admission = self.reuse_resident(supervisor, &resident).await?;
                    self.score_request(
                        &handle,
                        ScoreRequest {
                            worker: request.worker.worker,
                            scope: request.scope.clone(),
                            task: Task::FocusRank,
                            deadline,
                            authority: live.clone(),
                            state: String::new(),
                            question: question.clone(),
                            candidates: vec![Candidate {
                                id: candidate.id.to_string(),
                                text: candidate.window.text(&candidate.hit)?.to_owned(),
                            }],
                            resident,
                            admission,
                            max_input_tokens: pair_tokens,
                            lane,
                        },
                    )
                };
                let mut pending = match tokio::time::timeout_at(
                    tokio::time::Instant::from_std(deadline),
                    pending,
                )
                .await
                {
                    Ok(Ok(pending)) => pending,
                    // Not queued (a typed lane refusal, the resident
                    // unavailable, or the deadline while awaiting reuse
                    // admission): this pair ran nothing, so the pairs before
                    // it stay settled and the batch stops here.
                    _ => {
                        refused = true;
                        break;
                    }
                };
                let compiled = match tokio::time::timeout_at(
                    tokio::time::Instant::from_std(deadline),
                    &mut pending.compiled,
                )
                .await
                {
                    Ok(Ok(compiled)) => Some(compiled),
                    _ => {
                        pending.compiled.close();
                        pending.compiled.try_recv().ok()
                    }
                };
                let before = tokens;
                if let Some(compiled) = &compiled {
                    tokens = tokens
                        .checked_add(compiled.input_tokens)
                        .context("Focus token overflow")?;
                    queue_us = queue_us.saturating_add(compiled.queue_us);
                    compile_us = compile_us.saturating_add(compiled.compile_us);
                    if self
                        .loop_ledger
                        .lock()
                        .map_err(|_| anyhow::anyhow!("loop NN ledger unavailable"))?
                        .compiled_prefix(ticket, tokens)
                        .is_err()
                    {
                        settled = false;
                        break;
                    }
                }
                let completion = tokio::time::timeout_at(
                    tokio::time::Instant::from_std(deadline),
                    &mut pending.completed,
                )
                .await;
                let (measured, logits) = match completion {
                    Ok(Ok(Ok(scored))) => (
                        Some((
                            scored.input_tokens,
                            scored.queue_us,
                            scored.compile_us,
                            scored.inference_us,
                        )),
                        Some(scored.scores),
                    ),
                    Ok(Ok(Err(error))) => (
                        error.downcast_ref::<KnownScoreFailure>().map(|failure| {
                            (
                                failure.receipt.input_tokens,
                                failure.receipt.queue_us,
                                failure.receipt.compile_us,
                                failure.receipt.inference_us,
                            )
                        }),
                        None,
                    ),
                    _ => (None, None),
                };
                let Some((actual, queued, compiled_time, inferred)) = measured else {
                    settled = false;
                    break;
                };
                if compiled.is_none() {
                    tokens = before.checked_add(actual).context("Focus token overflow")?;
                    queue_us = queue_us.saturating_add(queued);
                    compile_us = compile_us.saturating_add(compiled_time);
                } else if tokens != before.saturating_add(actual) {
                    settled = false;
                    break;
                }
                inference_us = inference_us.saturating_add(inferred);
                let pair = match logits.as_deref() {
                    Some([no, yes]) if no.is_finite() && yes.is_finite() => FocusPairScore {
                        candidate: u16::try_from(index)?,
                        question: candidate.question,
                        no_logit: *no,
                        yes_logit: *yes,
                    },
                    _ => {
                        failed = true;
                        break;
                    }
                };
                receipt.pairs.push(pair);
            }
            receipt.work.candidates = receipt.pairs.len();
            receipt.work.queue_us = Some(queue_us);
            receipt.work.compile_us = Some(compile_us);
            receipt.work.inference_us = settled.then_some(inference_us);
            let still_live = Instant::now() < deadline && live(&request.scope);
            settle_focus_batch(
                FocusSettle {
                    ledger: &self.loop_ledger,
                    ticket,
                    reserved: &request.attribution,
                    ids: request.candidate_order(),
                    score_count: request.score_count,
                    tokens,
                    settled,
                    failed,
                    refused,
                    still_live,
                },
                &mut receipt,
            )?;
            if receipt.applied && (!live(&request.scope) || Instant::now() >= deadline) {
                receipt.applied = false;
                receipt.availability = Availability::Failed;
                receipt.ranked = (0..count).map(|index| index as u16).collect();
                receipt.reason = CHANGED.into();
            }
            batch_response(receipt, &request)
        }
    }

    /// The live delivery authority a Focus pair job keeps while queued.
    #[cfg(feature = "local-inference")]
    fn focus_authority(
        &self,
        request: &NativeFocusRequestV3,
        manifest: airk_core::identity::Sha256Digest,
    ) -> Result<airk_inference::inference_engine::pool::arbiter::Authority> {
        let gate = self.gate.clone();
        let store = self.policy_reader()?;
        let worker = request.worker;
        let authority = request.authority;
        Ok(Arc::new(move |scope: &ScopeBinding| {
            store().is_ok_and(|current| {
                current.revision() == scope.configuration_revision
                    && current.installation() == scope.installation
                    && current.catalog() == scope.catalog
                    && current.selected_profile(Stage::Focus) == Some(manifest)
                    && current.eligible_for(Stage::Focus, scope.index_id, Task::FocusRank)
            }) && gate.catalog_clock().is_some_and(|clock| {
                clock.namespace == scope.catalog
                    && gate
                        .capture_current_issue(clock.namespace, clock.revision)
                        .is_ok_and(|issue| gate.check_scope_issue(issue, worker, authority).is_ok())
            })
        }))
    }
}

/// A batch after its pair loop: what ran, and whether authority held.
#[cfg(feature = "local-inference")]
pub(super) struct FocusSettle<'a> {
    pub(super) ledger: &'a Mutex<loop_ledger::Ledger>,
    pub(super) ticket: loop_ledger::Ticket,
    /// The reservation per part and round, the weights of re-attribution.
    pub(super) reserved: &'a [airk_core::arbiter::answer_loop::TokenAttribution],
    pub(super) ids: Vec<airk_core::identity::Sha256Digest>,
    pub(super) score_count: usize,
    pub(super) tokens: usize,
    /// Every dispatched pair's work is known.
    pub(super) settled: bool,
    /// A pair completed with a failure or invalid logits.
    pub(super) failed: bool,
    /// The lane queued no further pair.
    pub(super) refused: bool,
    pub(super) still_live: bool,
}

/// Settle a batch in the loop ledger and rank it when it is complete. No
/// dispatched pair: released. Known work: re-attributed in proportion to
/// the reservation. Unknown work: the reservation stays held.
#[cfg(feature = "local-inference")]
pub(super) fn settle_focus_batch(
    settle: FocusSettle<'_>,
    receipt: &mut FocusBatchReceipt,
) -> Result<()> {
    let FocusSettle {
        ledger,
        ticket,
        reserved,
        ids,
        score_count,
        tokens,
        settled,
        failed,
        refused,
        still_live,
    } = settle;
    let count = receipt.candidate_count;
    if settled && tokens == 0 && receipt.pairs.is_empty() {
        // No pair was dispatched: a known zero-work batch, released.
        if ledger
            .lock()
            .map_err(|_| anyhow::anyhow!("loop NN ledger unavailable"))?
            .refused(ticket)
            .is_ok()
        {
            receipt.observed_input_tokens = Some(0);
            receipt.work.input_tokens = 0;
            for charge in &mut receipt.attribution {
                charge.tokens = 0;
            }
            receipt.reason = if refused { NOT_QUEUED } else { CHANGED }.into();
        }
    } else if settled
        && tokens > 0
        && let Ok(attribution) = ledger
            .lock()
            .map_err(|_| anyhow::anyhow!("loop NN ledger unavailable"))?
            .completed_prefix(ticket, tokens, reserved)
    {
        receipt.observed_input_tokens = Some(tokens);
        receipt.work.input_tokens = tokens;
        receipt.attribution = attribution;
        receipt.reason = FAILED.into();
        if !failed && receipt.pairs.len() == score_count && still_live {
            let policy = Instant::now();
            let scored = receipt
                .pairs
                .iter()
                .map(|pair| {
                    receipt
                        .relevance(pair, &ids)
                        .map(|score| (pair.candidate, score.relevance))
                })
                .collect::<Result<Vec<_>>>()?;
            receipt.ranked = airk_retrieval::arbiter::loop_focus::rank(&scored, count)?;
            receipt.applied = true;
            receipt.availability = Availability::Ready;
            receipt.reason = APPLIED.into();
            receipt.work.policy_us =
                Some(policy.elapsed().as_micros().min(u64::MAX as u128) as u64);
        }
    }
    Ok(())
}

#[cfg(feature = "local-inference")]
fn batch_response(
    ranking: FocusBatchReceipt,
    request: &NativeFocusRequestV3,
) -> Result<Option<NativeFocusResponseV3>> {
    let response = NativeFocusResponseV3 { ranking };
    response.validate(request)?;
    Ok(Some(response))
}

#[cfg(all(test, feature = "local-inference"))]
mod tests;
