// SPDX-License-Identifier: Apache-2.0
//! Answer /3 model calls: the card model observation, one batched loop card
//! call, and one batched Focus call whose pairs each carry their own
//! sub-need question. Every call is charged to the actual original read
//! attempt in the loop ledger, per part and round; unknown native work keeps
//! its reservation and a decision is never minted from it.
use super::*;
use airk_core::{
    arbiter::answer_loop::{CARD_ADVICE_SCHEMA, CardAdvice, SegmentOutcome},
    native_loop::{
        NativeCardModel, NativeCardRequest, NativeFocusRequestV3, NativeFocusResponseV3,
    },
};

/// Settled reasons are fixed text, so receipts stay small and comparable.
const UNKNOWN: &str = "The loop card call's native outcome is unknown; its reservation remains held and nothing is replayed.";
const NOT_ADMITTED: &str = "The selected Context profile does not admit this loop card task under its policy; no compile or scoring was dispatched.";
const ROSTER_REFUSED: &str = "The loop card roster breaks its task policy's roster rules; no compile or scoring was dispatched.";
const INPUT_REFUSED: &str =
    "The native lane refused this card input before any model work; the reservation was released.";
const SCORE_FAILED: &str =
    "Native card scoring returned a known failure; measured work is retained without a decision.";
const SCORES_INVALID: &str = "Native card scoring completed, but its score shape or finiteness was invalid; measured work is retained without a decision.";
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

/// The lane key of a card call: the original attempt and its lowest part.
#[cfg(feature = "local-inference")]
fn card_lane(
    request: &NativeCardRequest,
) -> airk_inference::inference_engine::pool::arbiter::LaneKey {
    airk_inference::inference_engine::pool::arbiter::LaneKey {
        attempt: Some(request.read_attempt.request_id),
        part: request
            .segments
            .iter()
            .map(|segment| segment.part)
            .min()
            .unwrap_or(0),
    }
}

/// Map per-segment selections to advice outcomes.
#[cfg(any(feature = "local-inference", test))]
fn apply_choices(
    advice: &mut CardAdvice,
    segments: &[airk_core::native_loop::CardSegment],
    choices: Vec<airk_retrieval::arbiter::loop_cards::SegmentChoice>,
) -> Result<()> {
    use airk_core::native_loop::LoopCard;
    ensure!(
        choices.len() == segments.len() && advice.segments.len() == segments.len(),
        "loop selection lost a segment"
    );
    for ((outcome, segment), choice) in advice.segments.iter_mut().zip(segments).zip(choices) {
        outcome.raw_scores = choice.raw_scores;
        if let Some(reason) = choice.abstained {
            outcome.rule = None;
            outcome.margin = None;
            outcome.outcome = Outcome::Abstained {
                reason: reason.into(),
            };
            continue;
        }
        let digests = choice
            .chosen
            .iter()
            .map(|index| {
                segment
                    .cards
                    .get(*index)
                    .context("loop selection chose outside its roster")
                    .and_then(LoopCard::text_digest)
            })
            .collect::<Result<Vec<_>>>()?;
        outcome.rule = choice.rule;
        outcome.margin = choice.margin;
        outcome.outcome = Outcome::Accepted {
            decision: if segment.multiple {
                Decision::LoopCards(digests)
            } else {
                Decision::LoopCard(*digests.first().context("loop selection chose no card")?)
            },
        };
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
        NativeCardModel::from_profile(&self.profile_description(manifest)?)
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
            use airk_inference::inference_engine::pool::arbiter::{
                CompiledReceipt, KnownInputRefusal, KnownScoreFailure, ScoreRequest,
            };
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
                advice.validate()?;
                return Ok(Some(advice));
            }
            let Ok(prompt) = loop_cards::prompt(&request) else {
                not_dispatched(&mut advice, ROSTER_REFUSED, true);
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
            let deadline = deadline.min(
                Instant::now()
                    .checked_add(Duration::from_nanos(
                        ticket.remaining_ns(airk_process::monotonic::now_ns()?),
                    ))
                    .context("loop ledger deadline overflow")?,
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
            let mut pending =
                match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), pending)
                    .await
                {
                    Ok(Ok(pending)) => pending,
                    _ => {
                        advice.validate()?;
                        return Ok(Some(advice));
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
                advice.work.queue_us = Some(receipt.queue_us);
                advice.work.compile_us = Some(receipt.compile_us);
                if receipt.input_tokens <= reserved
                    && self
                        .loop_ledger
                        .lock()
                        .map_err(|_| anyhow::anyhow!("loop NN ledger unavailable"))
                        .and_then(|mut ledger| ledger.compiled(ticket, receipt.input_tokens))
                        .is_err()
                {
                    advice.validate()?;
                    return Ok(Some(advice));
                }
            }
            let completion = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                &mut pending.completed,
            )
            .await;
            let settle = |tokens: usize| {
                self.loop_ledger
                    .lock()
                    .map_err(|_| anyhow::anyhow!("loop NN ledger unavailable"))
                    .and_then(|mut ledger| ledger.completed(ticket, tokens))
            };
            match &completion {
                Ok(Ok(Ok(scored))) => {
                    let Ok(attribution) = settle(scored.input_tokens) else {
                        advice.validate()?;
                        return Ok(Some(advice));
                    };
                    advice.observed_input_tokens = Some(scored.input_tokens);
                    advice.attribution = attribution;
                    advice.work.input_tokens = scored.input_tokens;
                    advice.work.queue_us = Some(scored.queue_us);
                    advice.work.compile_us = Some(scored.compile_us);
                    advice.work.inference_us = Some(scored.inference_us);
                    let policy_started = Instant::now();
                    match loop_cards::select(
                        card_stage,
                        &request.segments,
                        &scored.scores,
                        &calibration,
                        self.decision_mode(),
                    )
                    .and_then(|choices| apply_choices(&mut advice, &request.segments, choices))
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
                    unavailable(&mut advice, SCORE_FAILED);
                }
                Ok(Ok(Err(error))) if error.downcast_ref::<KnownInputRefusal>().is_some() => {
                    let released = self
                        .loop_ledger
                        .lock()
                        .map_err(|_| anyhow::anyhow!("loop NN ledger unavailable"))
                        .and_then(|mut ledger| ledger.refused(ticket))
                        .is_ok();
                    if released {
                        not_dispatched(&mut advice, INPUT_REFUSED, false);
                    }
                }
                _ => {}
            }
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
            advice.validate()?;
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

#[cfg(test)]
mod tests;
