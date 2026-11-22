// SPDX-License-Identifier: Apache-2.0
//! Optional ordering of complete canonical search results. No source authority.
use super::*;
use std::collections::BTreeMap;
use airk_core::{
    SearchQuery, SearchResponse,
    arbiter::focus::{FocusObservation, focus_token_allowance, ordinary_search_allocation},
    native_focus::{FocusSourceWindow, NativeFocusCandidate, NativeFocusRequest},
    search_execution::ServedMode,
};

#[derive(Default)]
pub(super) struct Attempt {
    deadline: Option<Instant>,
    parent: Option<Budget>,
    local: Option<Budget>,
    token_limit: usize,
    tokens: usize,
    focus_tokens: usize,
    initial_context_ready: Option<bool>,
    retained_bytes: usize,
    mandatory_bytes: usize,
    mandatory_records: usize,
    context_history_bytes: usize,
    nested: bool,
    ordinal: usize,
    blocked: bool,
    /// Change 4 (kernel half): pairs this Answer may still score, all searches.
    pair_cap: Option<usize>,
    /// Change 4: score a bounded window of each hit instead of its whole text.
    bounded_windows: bool,
    /// Answer /3: loop searches skip in-search Focus; spans are scored in
    /// batches (`rank_spans`), so Focus is never charged twice.
    defer_in_search: bool,
    /// Answer /3 batched Focus calls admitted, and their reserved tokens.
    loop_ordinal: usize,
    loop_tokens: usize,
}

fn state(context: &NativeContext) -> Result<std::sync::MutexGuard<'_, Attempt>> {
    context
        .focus
        .lock()
        .map_err(|_| anyhow::anyhow!("optional request accounting unavailable"))
}

/// The ORIGINAL optional deadline the Host supplied, when one is held. Native
/// metadata observations clip their waits to it; absent, they are unchanged.
pub(super) fn original_deadline(context: &NativeContext) -> Option<Instant> {
    context.focus.lock().ok().and_then(|held| held.deadline)
}

/// Focus eligibility observation whose native wait cannot outlive the read.
fn observe_focus_application(
    context: &NativeContext,
    publication: &airk_core::evidence::RevisionEnvelope,
) -> Result<Option<airk_core::arbiter::FocusApplication>> {
    match original_deadline(context) {
        Some(deadline) => context.owner.focus_application_before(
            context.worker,
            context.authority,
            publication,
            deadline,
        ),
        None => context
            .owner
            .focus_application(context.worker, context.authority, publication),
    }
}

impl crate::Kernel {
    /// The Host supplies the actual absolute deadline, never a replacement
    /// duration derived after selection, queueing or model loading.
    pub fn with_optional_read_attempt(
        &self,
        attempt: airk_core::read_attempt::ReadAttemptBudget,
        deadline: Instant,
    ) -> Result<Self> {
        self.with_optional_read_attempt_for("", attempt, deadline)
    }

    /// As `with_optional_read_attempt`, for the actual tool (Answer may hold
    /// the longer Answer read budget).
    pub fn with_optional_read_attempt_for(
        &self,
        tool: &str,
        attempt: airk_core::read_attempt::ReadAttemptBudget,
        deadline: Instant,
    ) -> Result<Self> {
        if self.arbiter_context.is_none() {
            return Ok(self.clone());
        }
        let kernel = self.with_context_attempt_for(tool, attempt)?;
        let context = kernel
            .arbiter_context
            .as_ref()
            .context("native optional owner absent")?;
        state(context)?.deadline = Some(deadline);
        Ok(kernel)
    }

    pub fn with_focus_reply_budget(&self, budget: FocusReplyBudget) -> Result<Self> {
        let Some(context) = &self.arbiter_context else {
            return Ok(self.clone());
        };
        let mut context = context.as_ref().clone();
        context.focus_reply_budget = Some(budget);
        let mut kernel = self.clone();
        kernel.arbiter_context = Some(Arc::new(context));
        Ok(kernel)
    }

    /// Closed native rendering capacity supplied by the canonical Answer owner.
    /// It is metadata space, never a new work budget or caller authority.
    pub fn with_context_reply_reserve(&self, reserve: usize) -> Result<Self> {
        ensure!(
            reserve <= airk_core::arbiter::answer_loop::MAX_ANSWER_RESPONSE_BYTES,
            "Answer metadata reserve exceeds native frame"
        );
        let Some(context) = &self.arbiter_context else {
            return Ok(self.clone());
        };
        let mut context = context.as_ref().clone();
        context.context_reply_reserve = reserve.max(4096);
        let mut kernel = self.clone();
        kernel.arbiter_context = Some(Arc::new(context));
        Ok(kernel)
    }

    pub fn with_optional_deadline(&self, deadline: Instant) -> Result<Self> {
        if let Some(context) = &self.arbiter_context {
            let mut held = state(context)?;
            held.deadline = Some(held.deadline.map_or(deadline, |prior| prior.min(deadline)));
        }
        Ok(self.clone())
    }

    /// Bound only by the native Answer owner before its first canonical read.
    /// Every reader clone and Context assessment shares this private accounting.
    pub(super) fn with_focus_parent_budget(
        &self,
        budget: Budget,
        deadline: Instant,
    ) -> Result<Self> {
        budget.validate()?;
        let Some(context) = &self.arbiter_context else {
            return Ok(self.clone());
        };
        if context.attempt.is_none() {
            return Ok(self.clone());
        }
        ensure!(
            context
                .partition
                .as_ref()
                .is_none_or(|partition| !partition.is_ordinary_focus()),
            "Focus-only Search allowance cannot authorize Answer"
        );
        let mut held = state(context)?;
        ensure!(
            held.parent.is_none() && held.ordinal == 0,
            "optional parent budget already bound"
        );
        let parent = context
            .partition
            .as_ref()
            .map_or_else(|| budget.clone(), |part| part.parent_budget.clone());
        held.token_limit = context
            .partition
            .as_ref()
            .map_or(budget.input_tokens, |part| {
                part.resources.input_tokens.min(budget.input_tokens)
            });
        held.parent = Some(parent);
        held.local = Some(budget);
        held.nested = true;
        held.deadline = Some(held.deadline.map_or(deadline, |prior| prior.min(deadline)));
        Ok(self.clone())
    }

    /// Private evaluation change set 1: bound once per Answer with its layers.
    pub(super) fn bind_focus_shape(&self, layers: software::Layers) -> Result<()> {
        if !layers.focus {
            return Ok(());
        }
        if let Some(context) = &self.arbiter_context {
            let mut held = state(context)?;
            // Each Host-captured request has fresh accounting; a reused
            // in-process session keeps the shape it was first given.
            if held.ordinal == 0 && held.pair_cap.is_none() {
                held.pair_cap = Some(software::FOCUS_PAIRS);
                held.bounded_windows = true;
            }
        }
        Ok(())
    }

    pub(super) fn bind_initial_context_readiness(&self, ready: bool) -> Result<()> {
        if let Some(context) = &self.arbiter_context {
            let mut held = state(context)?;
            if held.nested {
                ensure!(
                    held.initial_context_ready.is_none() && held.ordinal == 0,
                    "initial Context readiness already bound"
                );
                held.initial_context_ready = Some(ready);
            }
        }
        Ok(())
    }

    pub(super) fn observe_optional_source_reply(&self, tool: &str, body: &Value) -> Result<()> {
        if let Some(context) = &self.arbiter_context {
            let mut held = state(context)?;
            if held.parent.is_some() {
                held.retained_bytes = held
                    .retained_bytes
                    .checked_add(serde_json::to_vec(body)?.len())
                    .context("optional retained-byte accounting overflow")?;
                if matches!(tool, "search" | "airk_search")
                    && let Ok(search) = serde_json::from_value::<SearchResponse>(body.clone())
                    && search
                        .focus
                        .as_ref()
                        .and_then(FocusObservation::receipt)
                        .is_some_and(|receipt| {
                            receipt.identity.is_some()
                                && (receipt.reserved_input_tokens > 0
                                    || receipt.observed_input_tokens.is_none()
                                    || receipt.work.input_tokens > 0
                                    || !receipt.scores.is_empty())
                        })
                {
                    held.mandatory_bytes = held
                        .mandatory_bytes
                        .checked_add(serde_json::to_vec(body)?.len())
                        .context("mandatory Focus receipt-byte accounting overflow")?;
                    held.mandatory_records += 1;
                }
            }
        }
        Ok(())
    }

    #[cfg(feature = "local-inference")]
    pub(super) fn observe_optional_history(
        &self,
        history: &[airk_core::arbiter::context_action::AssessmentEvent],
    ) -> Result<()> {
        if let Some(context) = &self.arbiter_context {
            state(context)?.context_history_bytes = serde_json::to_vec(history)?.len();
        }
        Ok(())
    }

    #[cfg(feature = "local-inference")]
    pub(super) fn observe_context_work(&self, observed: Option<usize>) -> Result<()> {
        if let Some(context) = &self.arbiter_context {
            let mut held = state(context)?;
            match observed {
                Some(tokens) => {
                    held.tokens = held
                        .tokens
                        .checked_add(tokens)
                        .context("optional token accounting overflow")?
                }
                None => held.blocked = true,
            }
        }
        Ok(())
    }

    #[cfg(any(feature = "local-inference", test))]
    pub(super) fn optional_work_available(&self) -> Result<bool> {
        let Some(context) = &self.arbiter_context else {
            return Ok(false);
        };
        let held = state(context)?;
        Ok(!held.blocked
            && held
                .local
                .as_ref()
                .is_none_or(|_| held.tokens < held.token_limit))
    }

    pub(crate) fn focus_search(
        &self,
        query: &SearchQuery,
        response: SearchResponse,
    ) -> Result<SearchResponse> {
        self.focus_search_with_metadata(query, response, 0)
    }

    pub(crate) fn focus_search_with_metadata(
        &self,
        query: &SearchQuery,
        mut response: SearchResponse,
        metadata_bytes: usize,
    ) -> Result<SearchResponse> {
        if response.focus.is_some()
            || response
                .execution
                .as_ref()
                .is_none_or(|execution| execution.served_mode == ServedMode::Lexical)
        {
            return Ok(response);
        }
        let Some(context) = &self.arbiter_context else {
            return Ok(response);
        };
        if state(context)?.defer_in_search {
            return Ok(response);
        }
        let Some(publication) = response.publication.as_ref() else {
            return Ok(response);
        };
        if self.scoped_read_fingerprint()? != Some(context.authority.fingerprint) {
            return Err(airk_core::scoped_reads::ScopeReadRefused.into());
        }
        let frame = remaining_frame(self, &response)?.saturating_sub(metadata_bytes);
        let observed = match observe_focus_application(context, publication) {
            Ok(Some(observed)) => observed,
            Ok(None) => return Ok(response),
            Err(error) if error.is::<airk_core::scoped_reads::ScopeReadRefused>() => {
                return Err(error);
            }
            Err(_) => {
                return notice(
                    response,
                    Availability::Failed,
                    "Focus was not dispatched because its native eligibility observation is unavailable.",
                    None,
                    frame,
                );
            }
        };
        if matches!(
            observed.application.availability,
            Availability::Disabled | Availability::IndexDisabled
        ) {
            return Ok(response);
        }
        if observed.validate().is_err()
            || observed.application.scope.publication != *publication
            || observed.application.scope.index_id != context.authority.index_id
            || observed.application.scope.authorization != context.authority.fingerprint
        {
            return notice(
                response,
                Availability::Failed,
                "Focus was not dispatched because its native eligibility did not match the canonical source.",
                None,
                frame,
            );
        }
        let application = &observed.application;
        if application.availability != Availability::Ready {
            return notice(
                response,
                application.availability,
                &application.reason,
                None,
                frame,
            );
        }
        let Some(attempt) = context.attempt else {
            return notice(
                response,
                Availability::Constrained,
                "Focus requires the original Host-captured read attempt; canonical search is unchanged.",
                None,
                frame,
            );
        };
        let (clock, now_ns) = match context.owner.focus_clock() {
            Ok(Some(clock)) => clock,
            _ => {
                return notice(
                    response,
                    Availability::Constrained,
                    "Focus was not dispatched because the native read clock is unavailable.",
                    None,
                    frame,
                );
            }
        };
        let limits = observed
            .limits
            .as_ref()
            .context("ready Focus limits absent")?;
        let (parent, local, deadline, available, ordinal, retained, mandatory, nested, pair_cap, bounded) = {
            let mut held = state(context)?;
            let Some(deadline) = held.deadline else {
                drop(held);
                return notice(
                    response,
                    Availability::Constrained,
                    "Focus requires the actual original native deadline; canonical search is unchanged.",
                    None,
                    frame,
                );
            };
            if held.blocked {
                drop(held);
                return notice(
                    response,
                    Availability::Constrained,
                    "Prior optional work is unsettled; Focus was not dispatched or replayed.",
                    None,
                    frame,
                );
            }
            if held.parent.is_none() {
                // A combined ordinary read without a Host work partition cannot
                // create a new 2048-token allowance for each selected index.
                if context.authority.selection.is_some() && context.partition.is_none() {
                    drop(held);
                    return notice(
                        response,
                        Availability::Constrained,
                        "Focus is not applied to an unpartitioned combined search; complete canonical results are retained.",
                        None,
                        frame,
                    );
                }
                let Some(mut budget) = ordinary_search_allocation(
                    attempt,
                    clock,
                    now_ns,
                    response.hits.len(),
                    query.limit,
                    limits,
                    frame,
                )?
                else {
                    drop(held);
                    return notice(
                        response,
                        Availability::Constrained,
                        "The original search allocation has no available Focus work or receipt space.",
                        None,
                        frame,
                    );
                };
                let parent = if let Some(partition) = &context.partition {
                    ensure!(
                        partition.is_ordinary_focus(),
                        "ordinary Search requires an actual Focus-only allowance"
                    );
                    budget.calls = budget
                        .calls
                        .min(partition.resources.calls)
                        .min(partition.parent_budget.calls);
                    budget.rounds = budget.rounds.min(partition.parent_budget.rounds);
                    budget.fanout = budget.fanout.min(partition.parent_budget.fanout);
                    budget.candidates = budget
                        .candidates
                        .min(partition.resources.candidates)
                        .min(partition.parent_budget.candidates);
                    budget.input_tokens = budget
                        .input_tokens
                        .min(partition.resources.input_tokens)
                        .min(partition.parent_budget.input_tokens);
                    budget.max_bytes = budget
                        .max_bytes
                        .min(partition.resources.bytes)
                        .min(partition.parent_budget.max_bytes);
                    budget.deadline_ms =
                        budget.deadline_ms.min(partition.parent_budget.deadline_ms);
                    budget.validate()?;
                    partition.parent_budget.clone()
                } else {
                    budget.clone()
                };
                held.token_limit = budget.input_tokens;
                // Bind the optional sub-deadline ONCE, before its native queue.
                // The original Host attempt ID/absolute expiry remain untouched.
                held.deadline = Some(
                    deadline.min(
                        Instant::now()
                            .checked_add(Duration::from_millis(budget.deadline_ms))
                            .context("Focus sub-deadline overflow")?,
                    ),
                );
                held.local = Some(budget.clone());
                held.parent = Some(parent);
            }
            let local = held
                .local
                .as_ref()
                .context("optional local budget absent")?
                .clone();
            let deadline = held.deadline.context("original optional deadline absent")?;
            if held.ordinal >= local.calls
                || deadline <= Instant::now()
                || held.tokens >= held.token_limit
            {
                drop(held);
                return notice(
                    response,
                    Availability::Constrained,
                    "Focus was not dispatched because the original optional budget is exhausted.",
                    None,
                    frame,
                );
            }
            (
                held.parent.as_ref().unwrap().clone(),
                local,
                deadline,
                focus_token_allowance(
                    held.token_limit,
                    if held.nested {
                        held.initial_context_ready
                            .context("initial Context readiness absent")?
                    } else {
                        false
                    },
                    held.focus_tokens,
                    held.token_limit.saturating_sub(held.tokens),
                    limits,
                )?,
                held.ordinal + 1,
                held.retained_bytes,
                held.mandatory_bytes
                    .saturating_add(held.mandatory_records.saturating_mul(1024))
                    .saturating_add(held.context_history_bytes),
                held.nested,
                held.pair_cap,
                held.bounded_windows,
            )
        };
        let mut score_count = response
            .hits
            .len()
            .min(local.candidates)
            .min(limits.max_candidates)
            .min(available / limits.pair_input_tokens)
            .min(pair_cap.unwrap_or(usize::MAX));
        let outer_focus_bytes = if nested && context.partition.is_some() {
            crate::combined_reads::context_focus_binding_bytes(
                &application.scope,
                application.profile,
            )?
        } else {
            0
        };
        let frame = if nested {
            // This frame is in receipt bytes; JSON string embedding may double
            // them. Match the additional outer packet reserve before dispatch.
            frame
                .saturating_sub(
                    context
                        .context_reply_reserve
                        .saturating_sub(4096)
                        .div_ceil(2),
                )
                .saturating_sub(mandatory)
                .saturating_sub(outer_focus_bytes)
                .min(
                    local
                        .max_bytes
                        .saturating_div(2)
                        .saturating_sub(retained)
                        .saturating_sub(serde_json::to_vec(&response)?.len())
                        .saturating_sub(outer_focus_bytes),
                )
        } else {
            frame
        };
        while score_count > 0
            && full_receipt_bound(&application.scope, response.hits.len(), score_count)? > frame
        {
            score_count -= 1;
        }
        if score_count == 0 {
            return notice(
                response,
                Availability::Constrained,
                "The original token, candidate or complete-receipt budget cannot admit Focus; canonical search is unchanged.",
                None,
                frame,
            );
        }
        let mut candidates = Vec::with_capacity(response.hits.len());
        for (position, hit) in response.hits.iter().cloned().enumerate() {
            let (utf8_start, utf8_end) = if bounded {
                software::focus_window(&hit.text, &query.text, limits.pair_input_tokens)
            } else {
                (0, hit.text.len())
            };
            let window = FocusSourceWindow {
                utf8_start,
                utf8_end,
            };
            let candidate = match NativeFocusCandidate::new(
                &application.scope,
                &query.text,
                position,
                hit,
                window,
            ) {
                Ok(candidate) => candidate,
                Err(_) => {
                    return notice(
                        response,
                        Availability::UnsupportedTask,
                        "The canonical candidate set does not provide supported complete Focus source windows.",
                        None,
                        frame,
                    );
                }
            };
            candidates.push(candidate);
        }
        let order_digest = NativeFocusRequest::candidate_order_digest(&candidates)?;
        let remaining_ms = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(u128::from(local.deadline_ms)) as u64;
        if remaining_ms == 0 {
            return notice(
                response,
                Availability::Constrained,
                "The original Focus deadline expired before dispatch.",
                Some(order_digest),
                frame,
            );
        }
        let request = NativeFocusRequest {
            read_attempt: attempt,
            parent_budget: parent,
            worker: context.worker,
            authority: context.authority,
            scope: application.scope.clone(),
            question: query.text.clone(),
            candidates,
            order_digest,
            score_count,
            ordinal: u8::try_from(ordinal)?,
            partition: context.partition.clone(),
            remaining_ms,
            remaining_input_tokens: available,
        };
        if request
            .validate(context.worker, context.authority, clock, now_ns)
            .is_err()
        {
            return notice(
                response,
                Availability::UnsupportedTask,
                "The bounded canonical candidate input cannot be admitted for Focus; no scoring was dispatched.",
                Some(order_digest),
                frame,
            );
        }
        let reservation = score_count
            .checked_mul(limits.pair_input_tokens)
            .context("Focus token reservation overflow")?;
        {
            let mut held = state(context)?;
            ensure!(
                held.ordinal + 1 == ordinal
                    && !held.blocked
                    && reservation
                        <= focus_token_allowance(
                            held.token_limit,
                            held.nested && held.initial_context_ready == Some(true),
                            held.focus_tokens,
                            held.token_limit.saturating_sub(held.tokens),
                            limits
                        )?,
                "Focus request allocation changed before dispatch"
            );
            held.ordinal = ordinal;
            held.tokens += reservation;
            held.focus_tokens += reservation;
            if let Some(cap) = &mut held.pair_cap {
                *cap = cap.saturating_sub(score_count);
            }
        }
        let returned = context.owner.focus_rank(request.clone(), deadline);
        let mut ranking = match returned {
            Ok(Some(returned)) if returned.validate(&request).is_ok() => {
                let mut held = state(context)?;
                match returned.ranking.observed_input_tokens {
                    Some(tokens) => {
                        held.tokens = held
                            .tokens
                            .saturating_sub(reservation)
                            .checked_add(tokens)
                            .context("Focus measured token accounting overflow")?;
                        held.blocked |= tokens > returned.ranking.reserved_input_tokens;
                        held.focus_tokens = held
                            .focus_tokens
                            .saturating_sub(reservation)
                            .checked_add(tokens)
                            .context("Focus cumulative token accounting overflow")?;
                    }
                    None => held.blocked = true,
                }
                returned.ranking
            }
            Ok(None) => {
                let mut held = state(context)?;
                held.tokens = held.tokens.saturating_sub(reservation);
                held.focus_tokens = held.focus_tokens.saturating_sub(reservation);
                drop(held);
                return notice(
                    response,
                    Availability::Constrained,
                    "The native owner did not dispatch Focus; canonical search is unchanged.",
                    Some(order_digest),
                    frame,
                );
            }
            Err(error) if error.is::<airk_core::scoped_reads::ScopeReadRefused>() => {
                return Err(error);
            }
            _ => {
                state(context)?.blocked = true;
                unknown_receipt(
                    &request,
                    observed
                        .identity
                        .as_ref()
                        .context("ready Focus identity absent")?
                        .clone(),
                    reservation,
                )?
            }
        };
        let fresh = observe_focus_application(context, publication);
        let valid = match fresh {
            Ok(Some(fresh)) => {
                fresh.validate().is_ok()
                    && fresh.application.scope == application.scope
                    && fresh.application.availability == Availability::Ready
                    && fresh.identity == observed.identity
            }
            Err(error) if error.is::<airk_core::scoped_reads::ScopeReadRefused>() => {
                return Err(error);
            }
            _ => false,
        };
        if !valid
            || self.scoped_read_fingerprint()? != Some(context.authority.fingerprint)
            || self.arbiter_publication()? != *publication
        {
            return Err(FocusPostExecutionStale {
                _custody: Box::new((response, ranking)),
            }
            .into());
        }
        if Instant::now() >= deadline && ranking.applied {
            ranking.applied = false;
            ranking.availability = Availability::Failed;
            ranking.reason = "Focus completed after its original deadline; canonical candidate order is unchanged.".into();
            ranking.ranked_order.clone_from(&ranking.original_order);
        }
        let mut receipt_budget = local;
        receipt_budget.input_tokens = available;
        receipt_budget.deadline_ms = remaining_ms;
        ranking.validate(&receipt_budget)?;
        response.hits =
            ranking.reorder(&response.hits, &request.candidate_order(), &receipt_budget)?;
        response.focus = Some(FocusObservation::Receipt {
            receipt: Box::new(ranking),
        });
        Ok(response)
    }
}

/// One span of a batched Focus call: the sub-need question it is scored
/// against, its hit, and the part and round its tokens are charged to.
pub(super) struct BatchSpan {
    pub(super) question: u8,
    pub(super) hit: Hit,
    pub(super) part: u8,
    pub(super) round: u8,
}

/// One batched Focus call over spans of every ready part (Answer /3).
pub(super) struct SpanBatch {
    pub(super) budget: airk_core::arbiter::answer_loop::BudgetV3,
    /// The call's ordinal over the whole Answer, 1 first.
    pub(super) ordinal: u8,
    pub(super) questions: Vec<String>,
    pub(super) spans: Vec<BatchSpan>,
    pub(super) deadline: Instant,
}

/// A batched Focus call's compact receipt, and each span's relevance.
pub(super) struct SpanRanking {
    pub(super) receipt: airk_core::arbiter::answer_loop::FocusBatchReceipt,
    /// Relevance per span, `None` when unscored or not applied.
    pub(super) relevance: Vec<Option<f32>>,
    /// Submitted, but its outcome and work are unknown; nothing is replayed.
    pub(super) unknown: bool,
}

/// Batched Focus work became stale after native scoring: not delivered.
#[derive(Debug)]
pub struct FocusBatchStale {
    _custody: Box<airk_core::arbiter::answer_loop::FocusBatchReceipt>,
}
impl std::fmt::Display for FocusBatchStale {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Focus policy or source changed after batched native work; the Answer cannot be replayed automatically")
    }
}
impl std::error::Error for FocusBatchStale {}

impl crate::Kernel {
    /// Answer /3 binds this once before its first read: loop searches defer
    /// in-search Focus, and the original deadline can only shorten.
    pub(super) fn bind_answer_loop(&self, deadline: Instant) -> Result<()> {
        if let Some(context) = &self.arbiter_context {
            let mut held = state(context)?;
            ensure!(
                !held.defer_in_search && held.ordinal == 0 && held.loop_ordinal == 0,
                "Answer loop accounting already bound"
            );
            held.defer_in_search = true;
            held.deadline = Some(held.deadline.map_or(deadline, |prior| prior.min(deadline)));
        }
        Ok(())
    }

    /// Score spans against their own sub-need questions in one native call.
    /// `None` means Focus is absent or declined and nothing was charged.
    /// The ordinal and token reservation are taken in one critical section,
    /// so concurrent batches can never share or skip an allocation.
    pub(super) fn rank_spans(&self, batch: SpanBatch) -> Result<Option<SpanRanking>> {
        use airk_core::arbiter::answer_loop::{
            FOCUS_BATCH_SCHEMA, FocusBatchReceipt, MAX_FOCUS_BATCH_PAIRS, TokenAttribution,
            questions_digest, scope_digest,
        };
        use airk_core::native_loop::{NativeFocusCandidateV3, NativeFocusRequestV3};
        let Some(context) = &self.arbiter_context else {
            return Ok(None);
        };
        if batch.spans.is_empty() || batch.spans.len() > MAX_FOCUS_BATCH_PAIRS {
            return Ok(None);
        }
        if self.scoped_read_fingerprint()? != Some(context.authority.fingerprint) {
            return Err(airk_core::scoped_reads::ScopeReadRefused.into());
        }
        let publication = self.arbiter_publication()?;
        let observed = match observe_focus_application(context, &publication) {
            Ok(Some(observed)) => observed,
            Err(error) if error.is::<airk_core::scoped_reads::ScopeReadRefused>() => {
                return Err(error);
            }
            _ => return Ok(None),
        };
        let application = &observed.application;
        if observed.validate().is_err()
            || application.availability != Availability::Ready
            || application.scope.publication != publication
            || application.scope.index_id != context.authority.index_id
            || application.scope.authorization != context.authority.fingerprint
        {
            return Ok(None);
        }
        let (Some(limits), Some(identity), Some(attempt)) =
            (observed.limits.as_ref(), observed.identity.clone(), context.attempt)
        else {
            return Ok(None);
        };
        let Ok(Some((clock, now_ns))) = context.owner.focus_clock() else {
            return Ok(None);
        };
        let pair_tokens = limits.pair_input_tokens;
        let mut charges: BTreeMap<(u8, u8), usize> = BTreeMap::new();
        for span in &batch.spans {
            *charges.entry((span.part, span.round)).or_default() += pair_tokens;
        }
        let attribution: Vec<TokenAttribution> = charges
            .into_iter()
            .map(|((part, round), tokens)| TokenAttribution { part, round, tokens })
            .collect();
        let reservation: usize = attribution.iter().map(|charge| charge.tokens).sum();
        let mut candidates = Vec::with_capacity(batch.spans.len());
        for (position, span) in batch.spans.iter().enumerate() {
            let question = batch
                .questions
                .get(usize::from(span.question))
                .context("Focus span names an unknown question")?;
            let (utf8_start, utf8_end) =
                software::focus_window(&span.hit.text, question, pair_tokens);
            let window = FocusSourceWindow { utf8_start, utf8_end };
            match NativeFocusCandidateV3::new(
                &application.scope,
                &batch.questions,
                span.question,
                position,
                span.hit.clone(),
                window,
            ) {
                Ok(candidate) => candidates.push(candidate),
                Err(_) => return Ok(None),
            }
        }
        let order_digest = NativeFocusRequestV3::candidate_order_digest(&candidates)?;
        let remaining_ms = batch
            .deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(u128::from(batch.budget.deadline_ms)) as u64;
        if remaining_ms == 0 {
            return Ok(None);
        }
        let request = NativeFocusRequestV3 {
            read_attempt: attempt,
            parent_budget: batch.budget.clone(),
            worker: context.worker,
            authority: context.authority,
            scope: application.scope.clone(),
            questions: batch.questions.clone(),
            score_count: candidates.len(),
            candidates,
            order_digest,
            ordinal: batch.ordinal,
            attribution: attribution.clone(),
            partition: None,
            remaining_ms,
            remaining_input_tokens: reservation,
        };
        if request
            .validate(context.worker, context.authority, clock, now_ns)
            .is_err()
        {
            return Ok(None);
        }
        {
            let mut held = state(context)?;
            if held.blocked || usize::from(batch.ordinal) != held.loop_ordinal + 1 {
                return Ok(None);
            }
            held.loop_ordinal += 1;
            held.loop_tokens += reservation;
        }
        let returned = context.owner.focus_batch(request.clone(), batch.deadline);
        let (mut receipt, unknown) = match returned {
            Ok(Some(response)) if response.validate(&request).is_ok() => {
                let mut held = state(context)?;
                match response.ranking.observed_input_tokens {
                    Some(tokens) => {
                        held.loop_tokens = held.loop_tokens.saturating_sub(reservation) + tokens;
                        held.blocked |= tokens > reservation;
                    }
                    None => held.blocked = true,
                }
                let unknown = response.ranking.observed_input_tokens.is_none();
                (response.ranking, unknown)
            }
            Ok(None) => {
                let mut held = state(context)?;
                held.loop_ordinal -= 1;
                held.loop_tokens = held.loop_tokens.saturating_sub(reservation);
                return Ok(None);
            }
            Err(error) if error.is::<airk_core::scoped_reads::ScopeReadRefused>() => {
                return Err(error);
            }
            _ => {
                state(context)?.blocked = true;
                let count = request.candidates.len();
                (
                    FocusBatchReceipt {
                        schema: FOCUS_BATCH_SCHEMA.into(),
                        scope_digest: scope_digest(&request.scope)?,
                        identity: Some(identity.clone()),
                        availability: Availability::Failed,
                        reason: "Native Focus outcome and work are unknown; canonical order is retained and nothing is replayed.".into(),
                        applied: false,
                        input_digest: request.input_digest()?,
                        order_digest,
                        questions_digest: questions_digest(&request.questions)?,
                        question_count: u8::try_from(request.questions.len())?,
                        candidate_count: count,
                        ranked: (0..count).map(|index| index as u16).collect(),
                        pairs: vec![],
                        reserved_input_tokens: reservation,
                        observed_input_tokens: None,
                        attribution,
                        work: Work::default(),
                    },
                    true,
                )
            }
        };
        let fresh = observe_focus_application(context, &publication);
        let valid = match fresh {
            Ok(Some(fresh)) => {
                fresh.validate().is_ok()
                    && fresh.application.scope == application.scope
                    && fresh.application.availability == Availability::Ready
                    && fresh.identity.as_ref() == Some(&identity)
            }
            Err(error) if error.is::<airk_core::scoped_reads::ScopeReadRefused>() => {
                return Err(error);
            }
            _ => false,
        };
        if !valid
            || self.scoped_read_fingerprint()? != Some(context.authority.fingerprint)
            || self.arbiter_publication()? != publication
        {
            return Err(FocusBatchStale {
                _custody: Box::new(receipt),
            }
            .into());
        }
        if Instant::now() >= batch.deadline && receipt.applied {
            receipt.applied = false;
            receipt.availability = Availability::Failed;
            receipt.reason = "Focus completed after its original deadline; canonical order is unchanged.".into();
            receipt.ranked = (0..receipt.candidate_count).map(|index| index as u16).collect();
        }
        let mut relevance = vec![None; batch.spans.len()];
        if receipt.applied {
            let ids = request.candidate_order();
            for pair in &receipt.pairs {
                let score = receipt.relevance(pair, &ids)?;
                if let Some(slot) = relevance.get_mut(usize::from(pair.candidate)) {
                    *slot = Some(score.relevance);
                }
            }
        }
        Ok(Some(SpanRanking {
            receipt,
            relevance,
            unknown,
        }))
    }
}

fn unknown_receipt(
    request: &NativeFocusRequest,
    identity: airk_core::arbiter::FocusIdentity,
    reservation: usize,
) -> Result<airk_core::arbiter::FocusApplicationReceipt> {
    let order = request.candidate_order();
    Ok(airk_core::arbiter::FocusApplicationReceipt {
        schema: airk_core::arbiter::focus::RANKING_SCHEMA.into(), scope: request.scope.clone(), identity: Some(identity),
        availability: Availability::Failed, reason: "Native Focus outcome and work are unknown; original order is retained and no optional work is replayed. Native reservations and pins remain owned until completion.".into(),
        applied: false, input_digest: request.input_digest()?, original_order: order.clone(), ranked_order: order,
        scores: vec![], presentations: vec![], presented_count: 0, candidate_count: request.candidates.len(), scored_count: 0,
        omitted_count: request.candidates.len(), reserved_input_tokens: reservation, observed_input_tokens: None, work: Work::default(),
    })
}

fn notice(
    mut response: SearchResponse,
    availability: Availability,
    reason: &str,
    order: Option<Sha256Digest>,
    frame: usize,
) -> Result<SearchResponse> {
    response.focus = FocusObservation::not_applied_if_fits(
        availability,
        reason.into(),
        response.hits.len(),
        order,
        frame,
    )?;
    Ok(response)
}

fn full_receipt_bound(scope: &ScopeBinding, candidates: usize, scores: usize) -> Result<usize> {
    // Includes worst-case escaped 1024-byte native reason, all original/tail
    // identities, two finite f32 logits, windows, phase counters and envelope.
    Ok(12_288usize
        .saturating_add(serde_json::to_vec(scope)?.len())
        .saturating_add(candidates.saturating_mul(136))
        .saturating_add(scores.saturating_mul(512)))
}

fn remaining_frame(kernel: &crate::Kernel, response: &SearchResponse) -> Result<usize> {
    match kernel
        .arbiter_context
        .as_ref()
        .and_then(|context| context.focus_reply_budget.as_ref())
    {
        Some(budget) => Ok(budget(response)?.min(65_536)),
        None => Ok(0),
    }
}

#[derive(Debug)]
pub struct FocusPostExecutionStale {
    _custody: Box<(SearchResponse, airk_core::arbiter::FocusApplicationReceipt)>,
}
impl std::fmt::Display for FocusPostExecutionStale {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Focus policy or source changed after native work; the original read cannot be replayed automatically")
    }
}
impl std::error::Error for FocusPostExecutionStale {}

#[cfg(test)]
mod tests;
