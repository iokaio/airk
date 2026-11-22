// SPDX-License-Identifier: Apache-2.0
//! The model path of Decide: batched card calls and batched Focus at a
//! round barrier. A call is admitted only within the card slots, the token
//! pool, each part-round's token allowance and the part's model time; any
//! decision the model does not take falls back to the deterministic order
//! and the record says why. A submitted call whose outcome is lost keeps
//! its slot and reservation charged and blocks further model work: nothing
//! is ever replayed.
use super::*;
use airk_core::arbiter::context_action::{AssessmentUnavailable, EvidenceRef};
use airk_core::native_loop::{CardSegment, NativeCardRequest, NativeCardResponse};
use decide::ModelFacts;

/// One roster awaiting a decision.
pub(super) struct Seg {
    pub(super) at: decide::At,
    pub(super) cards: Vec<LoopCard>,
    /// The text the roster is about: the question, a part or a sub-need.
    pub(super) subject: String,
    pub(super) basis: Vec<EvidenceRef>,
    /// The deterministic choice.
    pub(super) fallback: Vec<usize>,
    pub(super) rule: v3::DeterministicRule,
}

/// A taken decision: the chosen cards, and whether a model chose them.
pub(super) struct Choice {
    pub(super) chosen: Vec<usize>,
    pub(super) model: bool,
}

type Scored = (v3::DecisionSource, Vec<usize>, ModelFacts);

/// The ModelShare disclosure is defined for these stages only.
fn shared_stage(stage: v3::LoopStage) -> bool {
    matches!(
        stage,
        v3::LoopStage::Plan | v3::LoopStage::Coverage | v3::LoopStage::FollowUp
    )
}

fn charge_key(at: &decide::At) -> (u8, u8) {
    (at.part.unwrap_or(0), at.round.unwrap_or(0))
}

impl Loop<'_> {
    /// The model card calls may use now, if any.
    pub(super) fn model_ready(&self) -> Option<LoopModel> {
        if self.model_blocked
            || self.configuration_changed
            || self.application.availability != Availability::Ready
            || self.context.attempt.is_none()
        {
            return None;
        }
        let model = self.model?;
        let gpu = model.device != v3::ExecutionDevice::Cpu;
        let valid = matches!(
            model.model,
            device::SelectedModel::ContextCompact | device::SelectedModel::ContextLarge
        ) && !(model.model == device::SelectedModel::ContextLarge && !gpu)
            && gpu == model.precision.is_some();
        valid.then_some(model)
    }

    fn disclose_share(&mut self, group: &[Seg]) {
        for seg in group {
            if let Some(part) = seg.at.part
                && shared_stage(seg.at.stage)
                && !self.model_share.contains(&(part, seg.at.stage))
            {
                self.model_share.push((part, seg.at.stage));
            }
        }
    }

    /// Take every roster's decision: model-scored where admitted, in calls of
    /// at most `per_call` segments and eight cards, deterministic otherwise.
    pub(super) fn decide(&mut self, segs: Vec<Seg>, per_call: usize) -> Result<Vec<Choice>> {
        let mut out = Vec::with_capacity(segs.len());
        let mut index = 0;
        while index < segs.len() {
            let scored = segs[index].cards.len() >= 2;
            let model = if scored { self.model_ready() } else { None };
            let Some(model) = model else {
                // A roster the model would have scored but could not (it
                // was blocked, declined or the configuration changed).
                if scored && self.model.is_some() {
                    self.disclose_share(&segs[index..=index]);
                }
                let seg = &segs[index];
                self.ledger.deterministic(
                    seg.at,
                    &seg.cards,
                    &seg.fallback,
                    seg.rule,
                    seg.basis.clone(),
                )?;
                out.push(Choice {
                    chosen: seg.fallback.clone(),
                    model: false,
                });
                index += 1;
                continue;
            };
            let mut end = index + 1;
            let mut cards = segs[index].cards.len();
            // A call batches rosters of one part only, so a part out of its
            // card share never takes another part's roster down with it.
            while end < segs.len()
                && end - index < per_call
                && segs[end].at.part == segs[index].at.part
                && segs[end].cards.len() >= 2
                && cards + segs[end].cards.len() <= v3::MAX_ACTION_CARDS
            {
                cards += segs[end].cards.len();
                end += 1;
            }
            let group = &segs[index..end];
            match self.call(group, model)? {
                Some(results) => {
                    for (seg, (source, chosen, facts)) in group.iter().zip(results) {
                        let model = source.kind() == v3::SourceKind::Model;
                        self.ledger.record(
                            seg.at,
                            &seg.cards,
                            &chosen,
                            source,
                            seg.basis.clone(),
                            Some(facts),
                        )?;
                        out.push(Choice { chosen, model });
                    }
                }
                None => {
                    self.disclose_share(group);
                    for seg in group {
                        self.ledger.deterministic(
                            seg.at,
                            &seg.cards,
                            &seg.fallback,
                            seg.rule,
                            seg.basis.clone(),
                        )?;
                        out.push(Choice {
                            chosen: seg.fallback.clone(),
                            model: false,
                        });
                    }
                }
            }
            index = end;
        }
        Ok(out)
    }

    /// One batched card call. `None`: not dispatched, decide deterministically.
    fn call(&mut self, group: &[Seg], model: LoopModel) -> Result<Option<Vec<Scored>>> {
        let budget = &self.request.budget;
        let stage = group[0].at.stage;
        let Some(attempt) = self.context.attempt else {
            return Ok(None);
        };
        let Ok(Some((clock, now_ns))) = self.context.owner.focus_clock() else {
            self.model_blocked = true;
            return Ok(None);
        };
        if self.stop_due() || self.card_slots >= budget.context_calls {
            self.disclose_share(group);
            return Ok(None);
        }
        // The slot is the first segment's part's (Split is charged before
        // the shares exist).
        let charged = group[0].at.part.and_then(|id| self.part_index(id));
        if let Some(index) = charged
            && self.used_slots[index] >= self.shares.parts[index].card_slots
        {
            self.disclose_share(group);
            return Ok(None);
        }
        // Model time: the part's share of what is left, per round left.
        let left = self
            .deadline
            .saturating_duration_since(self.now())
            .as_millis() as u64;
        // Barriers left, this one included: at barrier r of n, n - r + 1.
        let rounds_left = budget
            .rounds
            .saturating_sub(usize::from(group[0].at.round.unwrap_or(1)).saturating_sub(1));
        if self.cost.estimate(stage) > self.shares.round_model_ms(left, rounds_left) {
            self.disclose_share(group);
            return Ok(None);
        }
        // Tokens: one call's reservation, attributed by card count with the
        // remainder to the lowest part and round.
        let weights: BTreeMap<(u8, u8), usize> =
            group.iter().fold(BTreeMap::new(), |mut map, seg| {
                *map.entry(charge_key(&seg.at)).or_default() += seg.cards.len();
                map
            });
        // The reservation stays within the pool and the charged parts'
        // token shares (Split is charged before the shares exist).
        let used: usize = self.used_tokens.values().sum();
        let parts: BTreeSet<u8> = weights.keys().map(|(part, _)| *part).collect();
        let shares_left = parts
            .iter()
            .map(|part| match self.part_index(*part) {
                Some(index) if *part > 0 => self.shares.parts[index]
                    .tokens
                    .saturating_sub(self.part_tokens(*part)),
                _ => usize::MAX,
            })
            .fold(0usize, usize::saturating_add);
        let reserve = v3::CONTEXT_CALL_TOKENS
            .min(budget.input_tokens.saturating_sub(used))
            .min(shares_left);
        let total: usize = weights.values().sum();
        if reserve < weights.len() || total == 0 {
            self.disclose_share(group);
            return Ok(None);
        }
        // Every charged part and round gets at least one token, the rest
        // by card count, the remainder to the lowest.
        let spread = reserve - weights.len();
        let mut attribution: Vec<v3::TokenAttribution> = weights
            .iter()
            .map(|(&(part, round), weight)| v3::TokenAttribution {
                part,
                round,
                tokens: 1 + spread * weight / total,
            })
            .collect();
        let given: usize = attribution.iter().map(|charge| charge.tokens).sum();
        attribution[0].tokens += reserve - given;
        let over_share = |charge: &v3::TokenAttribution| {
            charge.part > 0
                && self.part_index(charge.part).is_some_and(|index| {
                    self.part_tokens(charge.part) + charge.tokens > self.shares.parts[index].tokens
                })
        };
        if attribution.iter().any(|charge| {
            over_share(charge)
                || charge.round > 0
                    && self
                        .used_tokens
                        .get(&(charge.part, charge.round))
                        .copied()
                        .unwrap_or(0)
                        + charge.tokens
                        > budget.round_input_tokens
        }) {
            self.disclose_share(group);
            return Ok(None);
        }
        let slot = u8::try_from(self.card_slots + 1)?;
        let first = self.ledger.next_ordinal()?;
        let segments: Vec<CardSegment> = group
            .iter()
            .enumerate()
            .map(|(offset, seg)| CardSegment {
                ordinal: first + offset as u8,
                card_slot: slot,
                stage,
                part: seg.at.part.unwrap_or(0),
                need: seg.at.need.unwrap_or(0),
                round: seg.at.round.unwrap_or(0),
                subject: seg.subject.clone(),
                cards: seg.cards.clone(),
                multiple: stage == v3::LoopStage::Plan,
            })
            .collect();
        // A model call never runs into the Compose and Union reserve.
        let model_deadline = self.model_deadline();
        let model_left = model_deadline
            .saturating_duration_since(self.now())
            .as_millis() as u64;
        if model_left == 0 {
            self.disclose_share(group);
            return Ok(None);
        }
        let remaining_ms = model_left.min(budget.deadline_ms);
        let request = NativeCardRequest {
            schema: v3::CARD_REQUEST_SCHEMA.into(),
            read_attempt: attempt,
            parent_budget: budget.clone(),
            worker: self.context.worker,
            authority: self.context.authority,
            scope: self.scope.clone(),
            task: v3::card_task(stage).context("card stage without a task")?,
            task_policy: decide::policy(stage),
            segments,
            attribution: attribution.clone(),
            partition: None,
            remaining_ms,
            remaining_input_tokens: reserve,
        };
        if request
            .validate(self.context.worker, self.context.authority, clock, now_ns)
            .is_err()
        {
            self.model_blocked = true;
            return Ok(None);
        }
        self.card_slots += 1;
        if let Some(index) = charged {
            self.used_slots[index] += 1;
        }
        let ordinals: Vec<u8> = (0..group.len())
            .map(|offset| first + offset as u8)
            .collect();
        let started = self.now();
        let returned = self
            .context
            .owner
            .score_cards(request.clone(), model_deadline);
        let elapsed = self.now().saturating_duration_since(started).as_millis() as u64;
        self.cost.observe(stage, elapsed);
        let gpu = model.device != v3::ExecutionDevice::Cpu;
        let facts = |offset: usize, receipt: Option<Sha256Digest>, unknown: bool| ModelFacts {
            card_slot: Some(slot),
            model: Some(model.model),
            device: (!unknown).then_some(model.device),
            precision: if gpu && !unknown {
                model.precision
            } else {
                None
            },
            batched_with: ordinals
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != offset)
                .map(|(_, ordinal)| *ordinal)
                .collect(),
            receipt,
            work_unknown: unknown,
            ..ModelFacts::default()
        };
        match returned {
            Ok(None) => {
                self.card_slots -= 1;
                if let Some(index) = charged {
                    self.used_slots[index] -= 1;
                }
                self.model_blocked = true;
                Ok(None)
            }
            Err(error) if error.is::<airk_core::scoped_reads::ScopeReadRefused>() => Err(error),
            Ok(Some(advice)) if self.acceptable(&request, &advice, group) => {
                let receipt = advice.input_digest;
                let mut results = Vec::with_capacity(group.len());
                for (offset, (seg, outcome)) in group.iter().zip(&advice.segments).enumerate() {
                    let mut facts = facts(offset, Some(receipt), false);
                    facts.scores.clone_from(&outcome.raw_scores);
                    let result = match &outcome.outcome {
                        Outcome::Accepted { decision } => {
                            facts.margin = outcome.margin;
                            let digests = seg
                                .cards
                                .iter()
                                .map(LoopCard::text_digest)
                                .collect::<Result<Vec<_>>>()?;
                            let picked = match decision {
                                Decision::LoopCard(card) => vec![*card],
                                Decision::LoopCards(cards) => cards.clone(),
                                _ => vec![],
                            };
                            let chosen = picked
                                .iter()
                                .filter_map(|card| digests.iter().position(|digest| digest == card))
                                .collect();
                            let mode = if outcome.rule.is_some() {
                                v3::ModelMode::Rule
                            } else {
                                v3::ModelMode::TopScore
                            };
                            (
                                v3::DecisionSource::Model {
                                    mode,
                                    rule: outcome.rule,
                                },
                                chosen,
                                facts,
                            )
                        }
                        Outcome::Abstained { .. } => (
                            v3::DecisionSource::Abstained {
                                fallback: seg.rule,
                                reason: AssessmentUnavailable::Abstained,
                            },
                            seg.fallback.clone(),
                            facts,
                        ),
                        _ => (
                            v3::DecisionSource::Unavailable {
                                fallback: seg.rule,
                                reason: AssessmentUnavailable::Unavailable,
                            },
                            seg.fallback.clone(),
                            facts,
                        ),
                    };
                    results.push(result);
                }
                for charge in &advice.attribution {
                    *self
                        .used_tokens
                        .entry((charge.part, charge.round))
                        .or_default() += charge.tokens;
                }
                self.ledger.receipts.push(advice);
                Ok(Some(results))
            }
            _ => {
                // Lost or unusable after dispatch: the reservation stays
                // charged, the slot stays used, and no model work follows.
                self.model_blocked = true;
                for charge in &attribution {
                    *self
                        .used_tokens
                        .entry((charge.part, charge.round))
                        .or_default() += charge.tokens;
                    *self
                        .unknown_tokens
                        .entry((charge.part, charge.round))
                        .or_default() += charge.tokens;
                }
                Ok(Some(
                    group
                        .iter()
                        .enumerate()
                        .map(|(offset, seg)| {
                            (
                                v3::DecisionSource::Unavailable {
                                    fallback: seg.rule,
                                    reason: AssessmentUnavailable::Unavailable,
                                },
                                seg.fallback.clone(),
                                facts(offset, None, true),
                            )
                        })
                        .collect(),
                ))
            }
        }
    }

    /// Advice is used only when it is the exact complete answer to this
    /// request, from the active profile, and every accepted segment is
    /// consistent with its own scores: within its stage's card limit, the
    /// margin exactly top minus runner-up, and a top-score choice the
    /// highest-scoring cards.
    fn acceptable(
        &self,
        request: &NativeCardRequest,
        advice: &v3::CardAdvice,
        group: &[Seg],
    ) -> bool {
        if (NativeCardResponse {
            advice: advice.clone(),
        })
        .validate(request)
        .is_err()
            || self.application.profile != Some(advice.profile)
        {
            return false;
        }
        let charged = |at: &decide::At| {
            advice
                .attribution
                .iter()
                .any(|charge| (charge.part, charge.round) == charge_key(at))
        };
        group.iter().zip(&advice.segments).all(|(seg, outcome)| {
            if !charged(&seg.at) {
                return false;
            }
            let Outcome::Accepted { decision } = &outcome.outcome else {
                return true;
            };
            let picked: Vec<Sha256Digest> = match decision {
                Decision::LoopCard(card) => vec![*card],
                Decision::LoopCards(cards) => cards.clone(),
                _ => return false,
            };
            let Ok(digests) = seg
                .cards
                .iter()
                .map(LoopCard::text_digest)
                .collect::<Result<Vec<_>>>()
            else {
                return false;
            };
            // Every chosen card is on the roster, once.
            let chosen: BTreeSet<usize> = picked
                .iter()
                .filter_map(|card| digests.iter().position(|digest| digest == card))
                .collect();
            let mut sorted = outcome.raw_scores.clone();
            sorted.sort_by(|left, right| right.total_cmp(left));
            let margin = match (sorted.first(), sorted.get(1)) {
                (Some(top), Some(second)) => Some(top - second),
                _ => None,
            };
            // Plan may take up to the request's sub-need limit, every other
            // stage one card.
            let limit = if seg.at.stage == v3::LoopStage::Plan {
                self.request.budget.max_sub_needs.min(v3::MAX_SUB_NEEDS)
            } else {
                1
            };
            // A top-score decision takes the highest-scoring cards: none it
            // left out scores above one it chose.
            let top_score = outcome.rule.is_some()
                || (outcome.raw_scores.len() == seg.cards.len() && {
                    let lowest_chosen = chosen
                        .iter()
                        .map(|index| outcome.raw_scores[*index])
                        .fold(f32::INFINITY, f32::min);
                    (0..seg.cards.len())
                        .filter(|index| !chosen.contains(index))
                        .all(|index| outcome.raw_scores[index] <= lowest_chosen)
                });
            !picked.is_empty()
                && chosen.len() == picked.len()
                && picked.len() <= limit
                && top_score
                && margin.is_some()
                && margin.map(f32::to_bits) == outcome.margin.map(f32::to_bits)
        })
    }

    /// Score this barrier's spans with batched Focus, at most 32 pairs and
    /// 16 questions a call. Spans keep canonical order where Focus is absent.
    pub(super) fn focus(
        &mut self,
        entries: &mut [(usize, usize, Vec<coverage::Span>)],
        round: u8,
    ) -> Result<()> {
        if self.focus_blocked || self.configuration_changed {
            return Ok(());
        }
        let Some(pair_tokens) = self
            .kernel
            .focus_pair_tokens()?
            .filter(|tokens| *tokens > 0)
        else {
            self.focus_blocked = true;
            return Ok(());
        };
        let allowance = self.request.budget.round_input_tokens;
        // Focus takes at most half of a part-round's token allowance, so the
        // card calls of the same barrier keep the other half, and never more
        // than the part's own token and pair shares. Only the first `scored`
        // spans of an entry are sent; the rest stay in coverage unscored, in
        // canonical order, disclosed as the part's model share.
        let mut planned: BTreeMap<u8, usize> = BTreeMap::new();
        let mut scored = Vec::with_capacity(entries.len());
        for (part, _, spans) in entries.iter() {
            let id = self.parts[*part].id;
            let used = self.used_tokens.get(&(id, round)).copied().unwrap_or(0);
            let share = self.shares.parts[*part];
            let fits = ((allowance / 2).saturating_sub(used) / pair_tokens)
                .min(share.tokens.saturating_sub(self.part_tokens(id)) / pair_tokens)
                .min(share.focus_pairs.saturating_sub(self.used_pairs[*part]));
            let taken = planned.entry(id).or_default();
            let keep = fits.saturating_sub(*taken).min(spans.len());
            if keep < spans.len() {
                self.disclose_read(id);
            }
            *taken += keep;
            scored.push(keep);
        }
        let per_call = (allowance / pair_tokens).min(v3::MAX_FOCUS_BATCH_PAIRS);
        let mut start = 0;
        let mut calls_here = 0usize;
        while start < entries.len() {
            // Grow a group of whole sub-needs within the batch bounds.
            let mut end = start;
            let mut pairs = 0;
            let mut questions = BTreeSet::new();
            while end < entries.len() {
                let (part, need, _) = &entries[end];
                let query = &self.parts[*part].needs[*need].need.query;
                let more = scored[end];
                let new_question = !questions.contains(query);
                if end > start
                    && (pairs + more > per_call
                        || (new_question && questions.len() >= v3::MAX_FOCUS_QUESTIONS))
                {
                    break;
                }
                questions.insert(query.clone());
                pairs += more;
                end += 1;
            }
            let budget = &self.request.budget;
            if pairs == 0 {
                start = end;
                continue;
            }
            if self.ledger.focus.len() >= budget.focus_calls
                || self.focus_pairs + pairs.min(v3::MAX_FOCUS_BATCH_PAIRS) > budget.focus_pairs
                || self.stop_due()
            {
                self.disclose_rest(entries, &scored, start);
                break;
            }
            let needs: Vec<retrieve::Need> = entries[start..end]
                .iter()
                .map(|(part, need, _)| self.parts[*part].needs[*need].need.clone())
                .collect();
            let ready: Vec<(u8, &retrieve::Need, u8, &[coverage::Span])> = entries[start..end]
                .iter()
                .zip(&scored[start..end])
                .zip(&needs)
                .map(|(((part, _, spans), keep), need)| {
                    (self.parts[*part].id, need, round, &spans[..*keep])
                })
                .collect();
            let (questions, owners, spans) = coverage::batch(&ready, v3::MAX_FOCUS_BATCH_PAIRS);
            if spans.is_empty() {
                start = end;
                continue;
            }
            let count = spans.len();
            // The whole-attempt pool holds card tokens too: a batch that
            // would pass it is not sent.
            let used: usize = self.used_tokens.values().sum();
            if used + count * pair_tokens > budget.input_tokens {
                self.disclose_rest(entries, &scored, start);
                break;
            }
            let mut batch_pairs: BTreeMap<u8, usize> = BTreeMap::new();
            for span in &spans {
                *batch_pairs.entry(span.part).or_default() += 1;
            }
            let ranking = self.kernel.rank_spans(focus::SpanBatch {
                budget: budget.clone(),
                round_left: self
                    .parts
                    .iter()
                    .map(|part| {
                        let used = self
                            .used_tokens
                            .get(&(part.id, round))
                            .copied()
                            .unwrap_or(0);
                        ((part.id, round), allowance.saturating_sub(used))
                    })
                    .collect(),
                questions,
                spans,
                deadline: self.model_deadline(),
            })?;
            let Some(ranking) = ranking else {
                self.focus_blocked = true;
                self.disclose_rest(entries, &scored, start);
                return Ok(());
            };
            calls_here += 1;
            self.batch_split |= calls_here > 1;
            self.focus_pairs += count;
            for (id, pairs) in batch_pairs {
                if let Some(index) = self.part_index(id) {
                    self.used_pairs[index] += pairs;
                }
            }
            for charge in &ranking.receipt.attribution {
                *self
                    .used_tokens
                    .entry((charge.part, charge.round))
                    .or_default() += charge.tokens;
            }
            if ranking.unknown {
                self.focus_blocked = true;
            }
            for ((slot, index), relevance) in owners.into_iter().zip(&ranking.relevance) {
                entries[start + slot].2[index].focus = *relevance;
            }
            ensure!(
                usize::from(ranking.ordinal) > self.ledger.focus.len(),
                "Focus batch ordinals are allocated in order"
            );
            self.ledger.focus.push(ranking.receipt);
            if self.focus_blocked {
                self.disclose_rest(entries, &scored, end);
                return Ok(());
            }
            start = end;
        }
        Ok(())
    }

    fn disclose_read(&mut self, part: u8) {
        if !self.model_share.contains(&(part, v3::LoopStage::Read)) {
            self.model_share.push((part, v3::LoopStage::Read));
        }
    }

    /// Spans from `start` on that Focus would have scored stay unscored.
    fn disclose_rest(
        &mut self,
        entries: &[(usize, usize, Vec<coverage::Span>)],
        scored: &[usize],
        start: usize,
    ) {
        for ((part, _, _), keep) in entries.iter().zip(scored).skip(start) {
            if *keep > 0 {
                self.disclose_read(self.parts[*part].id);
            }
        }
    }
}

#[cfg(test)]
mod tests;
