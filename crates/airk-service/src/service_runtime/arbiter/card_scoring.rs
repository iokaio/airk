// SPDX-License-Identifier: Apache-2.0
//! Answer /3 model calls: the card model observation, one batched loop card
//! call, and one batched Focus call whose pairs each carry their own
//! sub-need question. Every call is charged to the actual original read
//! attempt in the loop ledger, per part and round; unknown native work keeps
//! its reservation and a decision is never minted from it.
use super::*;
#[cfg(any(feature = "local-inference", test))]
use airk_core::arbiter::answer_loop::{CARD_ADVICE_SCHEMA, SegmentOutcome};
use airk_core::{
    arbiter::answer_loop::CardAdvice,
    native_loop::{
        NativeCardModel, NativeCardRequest, NativeFocusRequestV3, NativeFocusResponseV3,
    },
};

// Settled reasons are fixed text, so receipts stay small and comparable.
// Only a build with native inference settles a card call, so each reason
// exists only there, and in the unit tests that name it.
#[cfg(any(feature = "local-inference", test))]
const UNKNOWN: &str = "The loop card call's native outcome is unknown; its reservation remains held and nothing is replayed.";
#[cfg(any(feature = "local-inference", test))]
const NOT_ADMITTED: &str = "The selected Context profile does not admit this loop card task under its policy; no compile or scoring was dispatched.";
#[cfg(feature = "local-inference")]
const ROSTER_REFUSED: &str = "The loop card roster breaks its task policy's roster rules; no compile or scoring was dispatched.";
#[cfg(feature = "local-inference")]
const INPUT_REFUSED: &str =
    "The native lane refused this card input before any model work; the reservation was released.";
#[cfg(feature = "local-inference")]
const QUEUE_REFUSED: &str = "The native lane refused to queue this card call; no model work ran and the reservation was released.";
#[cfg(feature = "local-inference")]
const RESIDENT_UNAVAILABLE: &str = "The resident Context session was unavailable before dispatch; no model work ran and the reservation was released.";
#[cfg(any(feature = "local-inference", test))]
const SCORE_FAILED: &str =
    "Native card scoring returned a known failure; measured work is retained without a decision.";
#[cfg(feature = "local-inference")]
const SCORES_INVALID: &str = "Native card scoring completed, but its score shape or finiteness was invalid; measured work is retained without a decision.";
#[cfg(feature = "local-inference")]
const LATE_CHANGE: &str = "Source authority or the Context configuration changed after native work; measured work is retained and the decision was discarded.";

impl Owner {
    pub(in crate::service_runtime) fn card_model(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: airk_core::evidence::RevisionEnvelope,
    ) -> Result<Option<NativeCardModel>> {
        self.context.card_model(worker, authority, publication)
    }
    pub(in crate::service_runtime) async fn score_cards(
        &self,
        request: NativeCardRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<Option<CardAdvice>> {
        self.context.score_cards(request, supervisor).await
    }
    pub(in crate::service_runtime) async fn focus_batch(
        &self,
        request: NativeFocusRequestV3,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<Option<NativeFocusResponseV3>> {
        self.focus.focus_batch(request, supervisor).await
    }
}

/// The longest lane deadline of one model operation: the frozen resource
/// policy's per-operation deadline (120 s). An Answer may run up to 600 s,
/// but each card call or Focus pair job is one operation, and the private
/// Answer routes refuse a job whose deadline lies further away than one
/// operation (change-log entry 53); before, every card call made more than
/// 120 s before a 300 s Answer's end was refused there and never ran.
#[cfg(any(feature = "local-inference", test))]
pub(super) fn operation_deadline(answer: Instant, now: Instant) -> Instant {
    let operation = Duration::from_millis(
        airk_core::arbiter::resource_policy::ResourcePolicy::software_v2().deadline_ms,
    );
    now.checked_add(operation)
        .map_or(answer, |limit| answer.min(limit))
}

/// The service-log code of a card call that took no decision, by its
/// settled reason; `None` for a scored, abstained or accepted call.
#[cfg(feature = "local-inference")]
fn card_refusal_code(outcome: &Outcome) -> Option<&'static str> {
    let reason = match outcome {
        Outcome::Unavailable { reason }
        | Outcome::Unsupported { reason }
        | Outcome::InvalidInput { reason } => reason.as_str(),
        _ => return None,
    };
    Some(match reason {
        UNKNOWN => "card_call_outcome_unknown",
        NOT_ADMITTED => "card_call_not_admitted",
        ROSTER_REFUSED => "card_call_roster_refused",
        INPUT_REFUSED => "card_call_input_refused",
        QUEUE_REFUSED => "card_call_queue_refused",
        RESIDENT_UNAVAILABLE => "card_call_resident_unavailable",
        SCORE_FAILED => "card_call_score_failed",
        SCORES_INVALID => "card_call_scores_invalid",
        LATE_CHANGE => "card_call_late_change",
        _ => "card_call_unavailable",
    })
}

/// Every segment's outcome set to one unavailable reason.
#[cfg(any(feature = "local-inference", test))]
fn unavailable(advice: &mut CardAdvice, reason: &str) {
    for segment in &mut advice.segments {
        segment.raw_scores.clear();
        segment.rule = None;
        segment.margin = None;
        segment.outcome = Outcome::Unavailable {
            reason: reason.into(),
        };
    }
}

/// A known non-dispatch: nothing ran and nothing is charged.
#[cfg(any(feature = "local-inference", test))]
fn not_dispatched(advice: &mut CardAdvice, reason: &str, unsupported: bool) {
    advice.observed_input_tokens = Some(0);
    advice.work = Work::default();
    for charge in &mut advice.attribution {
        charge.tokens = 0;
    }
    for segment in &mut advice.segments {
        segment.raw_scores.clear();
        segment.rule = None;
        segment.margin = None;
        segment.outcome = if unsupported {
            Outcome::Unsupported {
                reason: reason.into(),
            }
        } else {
            Outcome::Unavailable {
                reason: reason.into(),
            }
        };
    }
}

/// The lane key of a card call: the original attempt and every part its
/// segments serve, so no other job of the attempt may hold one of them.
#[cfg(feature = "local-inference")]
fn card_lane(
    request: &NativeCardRequest,
) -> airk_inference::inference_engine::pool::arbiter::LaneKey {
    airk_inference::inference_engine::pool::arbiter::LaneKey::answer(
        request.read_attempt.request_id,
        request.segments.iter().map(|segment| segment.part),
    )
}

/// Map per-segment selections to advice outcomes through the task policy's
/// own mapping (`loop_cards::outcomes`), so it is part of the policy identity.
#[cfg(any(feature = "local-inference", test))]
fn apply_choices(
    advice: &mut CardAdvice,
    segments: &[airk_core::native_loop::CardSegment],
    choices: Vec<airk_retrieval::arbiter::loop_cards::SegmentChoice>,
) -> Result<()> {
    ensure!(
        advice.segments.len() == segments.len(),
        "loop selection lost a segment"
    );
    advice.segments = airk_retrieval::arbiter::loop_cards::outcomes(segments, choices)?;
    Ok(())
}

/// One card call after its reservation: the ledger, its ticket, and what
/// selection needs. Kept apart from the stage owner so the settlement can be
/// driven by a fake lane in tests.
#[cfg(feature = "local-inference")]
pub(super) struct CardCall<'a> {
    pub(super) ledger: &'a Mutex<loop_ledger::Ledger>,
    pub(super) ticket: loop_ledger::Ticket,
    pub(super) stage: airk_core::arbiter::answer_loop::LoopStage,
    pub(super) segments: &'a [airk_core::native_loop::CardSegment],
    pub(super) calibration: &'a airk_core::arbiter::answer_loop::LoopCalibration,
    pub(super) mode: airk_retrieval::arbiter::loop_cards::DecisionMode,
    pub(super) max_sub_needs: usize,
    pub(super) reserved: usize,
    pub(super) deadline: Instant,
    #[cfg(all(test, feature = "test-support"))]
    pub(super) score_evidence: Option<&'a mut super::card_capture::ScoreEvidence>,
}

/// Settle one card call: `pending` is the lane admission (`None` when the
/// deadline expired before it). A refusal before queueing releases the
/// reservation; unknown work keeps it; observed completion re-attributes it
/// and selects per segment.
#[cfg(feature = "local-inference")]
pub(super) async fn settle_card_call(
    call: CardCall<'_>,
    advice: &mut CardAdvice,
    pending: Option<Result<airk_inference::inference_engine::pool::arbiter::PendingScore>>,
) -> Result<()> {
    use airk_inference::inference_engine::pool::arbiter::{
        AdmissionRefused, CompiledReceipt, KnownInputRefusal, KnownScoreFailure,
    };
    use airk_retrieval::arbiter::loop_cards;
    let CardCall {
        ledger,
        ticket,
        stage,
        segments,
        calibration,
        mode,
        max_sub_needs,
        reserved,
        deadline,
        #[cfg(all(test, feature = "test-support"))]
        mut score_evidence,
    } = call;
    let mut pending = match pending {
        Some(Ok(pending)) => pending,
        // Everything in `pending` precedes the job entering the
        // queue: the resident lookup, the reuse admission, and
        // `score`, whose every error is a typed
        // `AdmissionRefused`. A deadline can only expire while
        // awaiting the reuse admission, before `score` runs. So a
        // failure here is a known non-dispatch, released.
        failed => {
            let reason = match &failed {
                Some(Err(error)) if error.downcast_ref::<AdmissionRefused>().is_some() => {
                    QUEUE_REFUSED
                }
                _ => RESIDENT_UNAVAILABLE,
            };
            if ledger
                .lock()
                .map_err(|_| anyhow::anyhow!("loop NN ledger unavailable"))
                .and_then(|mut ledger| ledger.refused(ticket))
                .is_ok()
            {
                not_dispatched(advice, reason, false);
            }
            return Ok(());
        }
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
    if let Some(receipt) = &compiled {
        #[cfg(all(test, feature = "test-support"))]
        if let Some(evidence) = score_evidence.as_deref_mut() {
            evidence.compiled_input_digest = Some(receipt.input_digest);
        }
        advice.work.queue_us = Some(receipt.queue_us);
        advice.work.compile_us = Some(receipt.compile_us);
        if receipt.input_tokens <= reserved
            && ledger
                .lock()
                .map_err(|_| anyhow::anyhow!("loop NN ledger unavailable"))
                .and_then(|mut ledger| ledger.compiled(ticket, receipt.input_tokens))
                .is_err()
        {
            return Ok(());
        }
    }
    let completion = tokio::time::timeout_at(
        tokio::time::Instant::from_std(deadline),
        &mut pending.completed,
    )
    .await;
    let settle = |tokens: usize| {
        ledger
            .lock()
            .map_err(|_| anyhow::anyhow!("loop NN ledger unavailable"))
            .and_then(|mut ledger| ledger.completed(ticket, tokens))
    };
    match &completion {
        Ok(Ok(Ok(scored))) => {
            let Ok(attribution) = settle(scored.input_tokens) else {
                return Ok(());
            };
            #[cfg(all(test, feature = "test-support"))]
            if let Some(evidence) = score_evidence {
                evidence.scored_input_digest = Some(scored.input_digest);
            }
            advice.observed_input_tokens = Some(scored.input_tokens);
            advice.attribution = attribution;
            advice.work.input_tokens = scored.input_tokens;
            advice.work.queue_us = Some(scored.queue_us);
            advice.work.compile_us = Some(scored.compile_us);
            advice.work.inference_us = Some(scored.inference_us);
            let policy_started = Instant::now();
            match loop_cards::select(
                stage,
                segments,
                &scored.scores,
                calibration,
                mode,
                max_sub_needs,
            )
            .and_then(|choices| apply_choices(advice, segments, choices))
            {
                Ok(()) => {}
                Err(_) => {
                    for segment in &mut advice.segments {
                        segment.raw_scores.clear();
                        segment.rule = None;
                        segment.margin = None;
                        segment.outcome = Outcome::InvalidInput {
                            reason: SCORES_INVALID.into(),
                        };
                    }
                }
            }
            advice.work.policy_us =
                Some(policy_started.elapsed().as_micros().min(u64::MAX as u128) as u64);
        }
        Ok(Ok(Err(error))) if error.downcast_ref::<KnownScoreFailure>().is_some() => {
            let receipt = &error
                .downcast_ref::<KnownScoreFailure>()
                .context("known score failure")?
                .receipt;
            if let Ok(attribution) = settle(receipt.input_tokens) {
                advice.observed_input_tokens = Some(receipt.input_tokens);
                advice.attribution = attribution;
                advice.work.input_tokens = receipt.input_tokens;
                advice.work.queue_us = Some(receipt.queue_us);
                advice.work.compile_us = Some(receipt.compile_us);
                advice.work.inference_us = Some(receipt.inference_us);
            }
            unavailable(advice, SCORE_FAILED);
        }
        Ok(Ok(Err(error))) if error.downcast_ref::<KnownInputRefusal>().is_some() => {
            let released = ledger
                .lock()
                .map_err(|_| anyhow::anyhow!("loop NN ledger unavailable"))
                .and_then(|mut ledger| ledger.refused(ticket))
                .is_ok();
            if released {
                not_dispatched(advice, INPUT_REFUSED, false);
            }
        }
        _ => {}
    }
    Ok(())
}

impl StageOwner {
    /// The Context model loop card calls would use now. `None`: the stage is
    /// disabled, the index is not enrolled, or the selected profile declares
    /// no loop card tasks, so the kernel's deterministic order decides.
    pub(super) fn card_model(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: airk_core::evidence::RevisionEnvelope,
    ) -> Result<Option<NativeCardModel>> {
        let application = self.application(worker, authority, publication)?;
        let configuration = self.read_configuration()?;
        if application.availability != Availability::Ready
            || configuration.revision() != application.scope.configuration_revision
            || !configuration.eligible(application.scope.index_id, Task::ContextAction)
        {
            return Ok(None);
        }
        let Some(manifest) = configuration.selected_profile(Stage::Context) else {
            return Ok(None);
        };
        // A service built without native inference holds no profile
        // description and scores nothing, so it reports no card model, as
        // `score_cards` and `focus_batch` dispatch nothing there.
        #[cfg(not(feature = "local-inference"))]
        {
            let _ = manifest;
            Ok(None)
        }
        #[cfg(feature = "local-inference")]
        {
            NativeCardModel::from_profile(&self.profile_description(manifest)?)
        }
    }

    pub(super) async fn score_cards(
        &self,
        request: NativeCardRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<Option<CardAdvice>> {
        let started = Instant::now();
        let clock = if cfg!(windows) {
            airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
        } else {
            airk_core::read_attempt::ReadClock::UnixMonotonic
        };
        let catalog_clock = self
            .gate
            .catalog_clock()
            .context("loop card catalog clock unavailable")?;
        ensure!(
            catalog_clock.namespace == self.namespace,
            airk_core::scoped_reads::ScopeReadRefused
        );
        let issue = self
            .gate
            .capture_current_issue(catalog_clock.namespace, catalog_clock.revision)?;
        // The installed lease is checked under this gate before structural
        // request validation; equal untrusted fields grant nothing.
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
            .context("loop card deadline overflow")?;
        let application = self.application(
            request.worker,
            request.authority,
            request.scope.publication.clone(),
        )?;
        ensure!(
            application.scope == request.scope,
            "loop card source identity changed"
        );
        let configuration = self.read_configuration()?;
        if configuration.revision() != request.scope.configuration_revision
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
            use airk_inference::inference_engine::pool::arbiter::ScoreRequest;
            use airk_retrieval::arbiter::loop_cards;
            let stage = self.stage;
            let profile = self.profile_description(
                configuration
                    .selected_profile(stage)
                    .context("loop card selected profile missing")?,
            )?;
            let card_stage = loop_cards::task_stage(request.task)
                .context("card request task is not a loop card task")?;
            let mut advice = CardAdvice {
                schema: CARD_ADVICE_SCHEMA.into(),
                task: request.task,
                profile: profile.manifest,
                runtime: profile.advice_runtime()?,
                compiler: profile.compiler,
                calibration: profile.calibration,
                policy: profile.policy,
                task_policy: request.task_policy,
                scope: request.scope.clone(),
                input_digest: request.input_digest()?,
                card_slot: request.segments[0].card_slot,
                reserved_input_tokens: request.remaining_input_tokens,
                observed_input_tokens: None,
                attribution: request.attribution.clone(),
                work: Work::default(),
                segments: request
                    .segments
                    .iter()
                    .map(|segment| SegmentOutcome {
                        ordinal: segment.ordinal,
                        raw_scores: vec![],
                        outcome: Outcome::Unavailable {
                            reason: UNKNOWN.into(),
                        },
                        rule: None,
                        margin: None,
                    })
                    .collect(),
            };
            if !profile.tasks.contains(&request.task)
                || profile.task_policies.get(&request.task) != Some(&request.task_policy)
                || loop_cards::policy_digest(card_stage)? != request.task_policy
            {
                not_dispatched(&mut advice, NOT_ADMITTED, true);
                self.record_model_refusal("card_call_not_admitted");
                advice.validate()?;
                return Ok(Some(advice));
            }
            let Ok(prompt) = loop_cards::prompt(&request) else {
                not_dispatched(&mut advice, ROSTER_REFUSED, true);
                self.record_model_refusal("card_call_roster_refused");
                advice.validate()?;
                return Ok(Some(advice));
            };
            let handle = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                supervisor.arbiter_management().existing(),
            )
            .await
            .context("loop card native owner deadline")??;
            let calibration = handle.loop_calibration(profile.manifest, request.task)?;
            ensure!(
                calibration.task == request.task && calibration.task_policy == request.task_policy,
                "loop calibration differs from the card request's task policy"
            );
            let mut ledger_attempt = request.read_attempt;
            ledger_attempt.remaining_ns = ledger_attempt.remaining_ns.min(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_nanos()
                    .min(u64::MAX as u128) as u64,
            );
            let ticket = self
                .loop_ledger
                .lock()
                .map_err(|_| anyhow::anyhow!("loop NN ledger unavailable"))?
                .reserve_card(
                    ledger_attempt,
                    &request.parent_budget,
                    advice.card_slot,
                    &request.attribution,
                    request.partition.as_ref(),
                    airk_process::monotonic::now_ns()?,
                )?;
            let deadline = operation_deadline(
                deadline.min(
                    Instant::now()
                        .checked_add(Duration::from_nanos(
                            ticket.remaining_ns(airk_process::monotonic::now_ns()?),
                        ))
                        .context("loop ledger deadline overflow")?,
                ),
                Instant::now(),
            );
            let reserved = request.remaining_input_tokens;
            let live = self.card_authority(&request, profile.manifest)?;
            let pending = async {
                let resident = handle.resident(profile.manifest)?;
                let admission = self.reuse_resident(supervisor, &resident).await?;
                self.score_request(
                    &handle,
                    ScoreRequest {
                        worker: request.worker.worker,
                        scope: request.scope.clone(),
                        task: request.task,
                        deadline,
                        authority: live,
                        state: prompt.state,
                        question: prompt.question,
                        candidates: prompt.candidates,
                        resident,
                        admission,
                        max_input_tokens: reserved,
                        lane: card_lane(&request),
                    },
                )
            };
            let pending =
                tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), pending)
                    .await
                    .ok();
            let mode = self.decision_mode();
            #[cfg(all(test, feature = "test-support"))]
            let mut score_evidence = super::card_capture::ScoreEvidence::default();
            settle_card_call(
                CardCall {
                    ledger: &self.loop_ledger,
                    ticket,
                    stage: card_stage,
                    segments: &request.segments,
                    calibration: &calibration,
                    mode,
                    max_sub_needs: request.parent_budget.max_sub_needs,
                    reserved,
                    deadline,
                    #[cfg(all(test, feature = "test-support"))]
                    score_evidence: self.card_capture.as_ref().map(|_| &mut score_evidence),
                },
                &mut advice,
                pending,
            )
            .await?;
            let current = self.read_configuration();
            if advice.observed_input_tokens.is_some()
                && (self
                    .validate_scope(request.worker, request.authority)
                    .is_err()
                    || !current.is_ok_and(|current| {
                        current.revision() == request.scope.configuration_revision
                            && current.selected_profile(stage) == Some(profile.manifest)
                            && current.eligible(request.scope.index_id, Task::ContextAction)
                    }))
            {
                for segment in &mut advice.segments {
                    segment.rule = None;
                    segment.margin = None;
                    segment.outcome = Outcome::Unavailable {
                        reason: LATE_CHANGE.into(),
                    };
                }
            }
            if let Some(code) = advice.segments.first().and_then(|segment| card_refusal_code(&segment.outcome)) {
                self.record_model_refusal(code);
            }
            advice.validate()?;
            #[cfg(all(test, feature = "test-support"))]
            if let Some(observer) = &self.card_capture {
                observer.offer(&request, &advice, mode, score_evidence);
            }
            Ok(Some(advice))
        }
    }

    /// The live delivery authority a card job keeps while it is queued.
    #[cfg(feature = "local-inference")]
    fn card_authority(
        &self,
        request: &NativeCardRequest,
        manifest: airk_core::identity::Sha256Digest,
    ) -> Result<airk_inference::inference_engine::pool::arbiter::Authority> {
        let stage = self.stage;
        let gate = self.gate.clone();
        let store = self.policy_reader()?;
        let worker = request.worker;
        let authority = request.authority;
        Ok(Arc::new(move |scope: &ScopeBinding| {
            store().is_ok_and(|current| {
                current.revision() == scope.configuration_revision
                    && current.installation() == scope.installation
                    && current.catalog() == scope.catalog
                    && current.selected_profile(stage) == Some(manifest)
                    && current.eligible(scope.index_id, Task::ContextAction)
            }) && gate.catalog_clock().is_some_and(|clock| {
                clock.namespace == scope.catalog
                    && gate
                        .capture_current_issue(clock.namespace, clock.revision)
                        .is_ok_and(|ticket| {
                            gate.check_scope_issue(ticket, worker, authority).is_ok()
                        })
            })
        }))
    }

    /// The card decision mode: calibrated, unless the private evaluation
    /// owner set the top-score mode on this stage.
    #[cfg(feature = "local-inference")]
    fn decision_mode(&self) -> airk_retrieval::arbiter::loop_cards::DecisionMode {
        #[cfg(feature = "test-support")]
        if self.evaluation.is_some()
            && let Some(mode) = self.decision_mode
        {
            return mode;
        }
        airk_retrieval::arbiter::loop_cards::DecisionMode::Calibrated
    }
}

#[cfg(all(test, feature = "local-inference"))]
mod settlement_tests;
#[cfg(test)]
mod tests;
