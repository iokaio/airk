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
            while end < segs.len()
                && end - index < per_call
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
            return Ok(None);
        }
        // Model time: the part's share of what is left, per round left.
        let left = self
            .deadline
            .saturating_duration_since(self.now())
            .as_millis() as u64;
        let rounds_left = budget
            .rounds
            .saturating_sub(usize::from(group[0].at.round.unwrap_or(1)).saturating_sub(1));
        if self.cost.estimate(stage) > self.shares.round_model_ms(left, rounds_left) {
            self.disclose_share(group);
            return Ok(None);
        }
        // Tokens: one call's reservation, attributed by card count with the
        // remainder to the lowest part and round.
        let used: usize = self.used_tokens.values().sum();
        let reserve = v3::CONTEXT_CALL_TOKENS.min(budget.input_tokens.saturating_sub(used));
        let weights: BTreeMap<(u8, u8), usize> =
            group.iter().fold(BTreeMap::new(), |mut map, seg| {
                *map.entry(charge_key(&seg.at)).or_default() += seg.cards.len();
                map
            });
        let total: usize = weights.values().sum();
        if reserve < weights.len() || total == 0 {
            self.disclose_share(group);
            return Ok(None);
        }
        let mut attribution: Vec<v3::TokenAttribution> = weights
            .iter()
            .map(|(&(part, round), weight)| v3::TokenAttribution {
                part,
                round,
                tokens: reserve * weight / total,
            })
            .collect();
        let given: usize = attribution.iter().map(|charge| charge.tokens).sum();
        attribution[0].tokens += reserve - given;
        if attribution.iter().any(|charge| {
            charge.round > 0
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
        let remaining_ms = (left.min(budget.deadline_ms)).max(1);
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
        let ordinals: Vec<u8> = (0..group.len())
            .map(|offset| first + offset as u8)
            .collect();
        let started = self.now();
        let returned = self
            .context
            .owner
            .score_cards(request.clone(), self.deadline);
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
    /// consistent with its own scores.
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
            let count = match decision {
                Decision::LoopCard(_) => 1,
                Decision::LoopCards(cards) => cards.len(),
                _ => return false,
            };
            let mut sorted = outcome.raw_scores.clone();
            sorted.sort_by(|left, right| right.total_cmp(left));
            let margin = match (sorted.first(), sorted.get(1)) {
                (Some(top), Some(second)) => Some(top - second),
                _ => None,
            };
            let limit = if seg.at.stage == v3::LoopStage::Plan {
                v3::MAX_SUB_NEEDS
            } else {
                1
            };
            count <= limit
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
        let mut start = 0;
        let mut calls_here = 0usize;
        while start < entries.len() {
            // Grow a group of whole sub-needs within the batch bounds.
            let mut end = start;
            let mut pairs = 0;
            let mut questions = BTreeSet::new();
            while end < entries.len() {
                let (part, need, spans) = &entries[end];
                let query = &self.parts[*part].needs[*need].need.query;
                let more = spans.len();
                let new_question = !questions.contains(query);
                if end > start
                    && (pairs + more > v3::MAX_FOCUS_BATCH_PAIRS
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
                break;
            }
            let needs: Vec<retrieve::Need> = entries[start..end]
                .iter()
                .map(|(part, need, _)| self.parts[*part].needs[*need].need.clone())
                .collect();
            let ready: Vec<(u8, &retrieve::Need, u8, &[coverage::Span])> = entries[start..end]
                .iter()
                .zip(&needs)
                .map(|((part, _, spans), need)| {
                    (self.parts[*part].id, need, round, spans.as_slice())
                })
                .collect();
            let (questions, owners, spans) = coverage::batch(&ready, v3::MAX_FOCUS_BATCH_PAIRS);
            if spans.is_empty() {
                start = end;
                continue;
            }
            let count = spans.len();
            let ranking = self.kernel.rank_spans(focus::SpanBatch {
                budget: budget.clone(),
                ordinal: u8::try_from(self.ledger.focus.len() + 1)?,
                questions,
                spans,
                deadline: self.deadline,
            })?;
            let Some(ranking) = ranking else {
                self.focus_blocked = true;
                return Ok(());
            };
            calls_here += 1;
            self.batch_split |= calls_here > 1;
            self.focus_pairs += count;
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
            self.ledger.focus.push(ranking.receipt);
            if self.focus_blocked {
                return Ok(());
            }
            start = end;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
