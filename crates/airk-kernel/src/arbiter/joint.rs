// SPDX-License-Identifier: Apache-2.0
//! Local /2 execution. Canonical reads retain the original Gathering owner.
use super::*;
use airk_core::arbiter::context_action::*;
#[cfg(feature = "local-inference")]
use airk_core::native_context_action::NativeContextActionRequest;

struct Joint {
    history: Vec<AssessmentEvent>,
    coverage: PartitionCoverage,
    gaps: Vec<Gap>,
    terminal: Option<PartitionTermination>,
    revoked: bool,
    #[cfg(feature = "local-inference")]
    blocked: bool,
    #[cfg(feature = "local-inference")]
    ordinals: Vec<u8>,
    #[cfg(feature = "local-inference")]
    token_limit: usize,
}
impl Joint {
    fn configuration_changed(&mut self) {
        self.revoked |= self.history.iter().any(has_native_receipt);
        for event in &mut self.history {
            if event.applicability == AssessmentApplicability::Current {
                event.applicability = AssessmentApplicability::HistoricalAfterConfigurationChange;
            }
        }
        self.coverage = PartitionCoverage::Unassessed {
            reason: AssessmentUnavailable::Unavailable,
        };
        self.gaps = vec![Gap::Unsupported];
        if matches!(
            self.terminal,
            Some(
                PartitionTermination::Covered
                    | PartitionTermination::ClarificationRequired
                    | PartitionTermination::NoJustifiedRead
            )
        ) {
            self.terminal = None;
        }
        #[cfg(feature = "local-inference")]
        {
            self.blocked = true;
        }
    }
    fn invalidate(&mut self, reason: AssessmentUnavailable) {
        for event in &mut self.history {
            if event.applicability == AssessmentApplicability::Current {
                event.applicability = AssessmentApplicability::HistoricalAfterRead;
            }
        }
        self.coverage = PartitionCoverage::Unassessed { reason };
        self.gaps = vec![Gap::Unsupported];
    }
    #[cfg(feature = "local-inference")]
    fn lost(&mut self, ordinal: u8, scope: &ScopeBinding, roster: &ContextActionRoster) {
        self.invalidate(AssessmentUnavailable::Unavailable);
        self.blocked = true;
        self.history.push(AssessmentEvent {
            ordinal,
            partition: scope.clone(),
            basis: roster.basis.clone(),
            result: AssessmentResult::Unavailable {
                reason: AssessmentUnavailable::Unavailable,
                work: Work::default(),
                work_unknown: true,
                advice: None,
            },
            applicability: AssessmentApplicability::Current,
        });
    }
    #[cfg(feature = "local-inference")]
    fn apply(
        &mut self,
        gather: &mut Gathering,
        roster: ContextActionRoster,
        ordinal: u8,
        advice: ContextActionAdvice,
        now: Instant,
        can_actuate: bool,
    ) -> Result<()> {
        ensure!(
            !self.blocked
                && self.history.len() < self.ordinals.len()
                && self.ordinals[self.history.len()] == ordinal,
            "joint assessment reservation is not next original ordinal"
        );
        advice.validate()?;
        ensure!(
            advice.scope == gather.scope
                && advice.task_policy == roster.policy
                && gather.output.applications[0].profile == Some(advice.profile),
            "joint advice differs from captured scope/profile/task policy"
        );
        let expected: BTreeSet<_> = roster
            .basis
            .source_units_presented
            .iter()
            .map(|unit| unit.record)
            .collect();
        ensure!(
            advice.evidence.iter().copied().collect::<BTreeSet<_>>() == expected,
            "joint advice differs from shown evidence"
        );
        let remaining = self
            .token_limit
            .saturating_sub(gather.output.work.input_tokens);
        ensure!(
            advice.reserved_input_tokens <= remaining,
            "joint reservation exceeds native allowance"
        );
        gather.output.work.input_tokens = gather
            .output
            .work
            .input_tokens
            .checked_add(advice.work.input_tokens)
            .context("joint token accounting overflow")?;
        for (total, observed) in [
            (&mut gather.output.work.queue_us, advice.work.queue_us),
            (&mut gather.output.work.compile_us, advice.work.compile_us),
            (
                &mut gather.output.work.inference_us,
                advice.work.inference_us,
            ),
            (&mut gather.output.work.policy_us, advice.work.policy_us),
        ] {
            if let Some(observed) = observed {
                *total = Some(
                    total
                        .unwrap_or(0)
                        .checked_add(observed)
                        .context("joint timing overflow")?,
                );
            }
        }
        self.invalidate(AssessmentUnavailable::NewEvidence);
        let result = match &advice.outcome {
            Outcome::Accepted {
                decision: Decision::ContextAction(id),
            } if now < gather.deadline && can_actuate => {
                let card = roster.selected(*id)?.clone();
                self.coverage = PartitionCoverage::Assessed { ordinal };
                self.gaps = vec![card.gap];
                match card.action {
                    ContextAction::Read { choice } => {
                        let position = gather
                            .queue
                            .iter()
                            .position(|dispatch| dispatch.choice.id == choice)
                            .context("joint action is not an available verified read")?;
                        let read = gather.queue.remove(position).unwrap();
                        gather.queue.push_front(read);
                    }
                    ContextAction::Sufficient => {
                        self.terminal = Some(PartitionTermination::Covered)
                    }
                    ContextAction::Clarify => {
                        self.terminal = Some(PartitionTermination::ClarificationRequired)
                    }
                    ContextAction::NoJustifiedRead => {
                        self.terminal = Some(PartitionTermination::NoJustifiedRead)
                    }
                }
                AssessmentResult::Selected { card, advice }
            }
            Outcome::Abstained { .. } if advice.observed_input_tokens.is_some() => {
                AssessmentResult::Abstained { advice }
            }
            Outcome::Accepted { .. } => {
                // A complete score arriving after the deadline remains historical
                // native evidence of work; it cannot actuate a read or terminal.
                self.blocked = true;
                let card = match &advice.outcome {
                    Outcome::Accepted {
                        decision: Decision::ContextAction(id),
                    } => roster.selected(*id)?.clone(),
                    _ => unreachable!(),
                };
                self.history.push(AssessmentEvent {
                    ordinal,
                    partition: gather.scope.clone(),
                    basis: roster.basis,
                    result: AssessmentResult::Selected { card, advice },
                    applicability: if can_actuate {
                        AssessmentApplicability::HistoricalAfterRead
                    } else {
                        AssessmentApplicability::HistoricalAfterConfigurationChange
                    },
                });
                if can_actuate {
                    gather.output.stop = StopReason::Deadline;
                    self.terminal = Some(PartitionTermination::Deadline);
                } else {
                    self.configuration_changed();
                }
                return Ok(());
            }
            _ => {
                self.blocked = true;
                let overrun = gather.output.work.input_tokens > self.token_limit;
                if overrun {
                    self.terminal = Some(PartitionTermination::Budget);
                    gather.output.stop = StopReason::Budget;
                }
                AssessmentResult::Unavailable {
                    reason: if overrun {
                        AssessmentUnavailable::TokenBudget
                    } else {
                        AssessmentUnavailable::Unavailable
                    },
                    work: advice.work.clone(),
                    work_unknown: true,
                    advice: Some(advice),
                }
            }
        };
        self.history.push(AssessmentEvent {
            ordinal,
            partition: gather.scope.clone(),
            basis: roster.basis,
            result,
            applicability: AssessmentApplicability::Current,
        });
        Ok(())
    }
}

impl crate::Kernel {
    pub fn answer_with_reader_v2(
        &self,
        request: ContextRequestV2,
        deadline: Instant,
        require_live: &dyn Fn() -> Result<()>,
        reader: &CanonicalReader<'_>,
    ) -> Result<ContextResponseV2> {
        request.validate()?;
        require_live()?;
        let started = Instant::now();
        let deadline = deadline.min(
            started
                .checked_add(Duration::from_millis(request.budget.deadline_ms))
                .context("joint deadline overflow")?,
        );
        ensure!(
            deadline > started && deadline.duration_since(started).as_millis() > 0,
            "joint original deadline exhausted before native admission"
        );
        let context = self
            .arbiter_context
            .as_ref()
            .context("joint answer requires native admitted index")?;
        ensure!(
            self.scoped_read_fingerprint()? == Some(context.authority.fingerprint),
            "joint source authority expired"
        );
        let publication = self.arbiter_publication()?;
        let application = observed_application(context, &publication, None)?;
        let scope = application.scope.clone();
        ensure!(
            scope.index_id == context.authority.index_id
                && scope.authorization == context.authority.fingerprint
                && scope.publication == publication,
            "joint owner supplied a foreign binding"
        );
        let (ordinals, token_limit, parent_budget) = if let Some(partition) = &context.partition {
            partition.validate()?;
            ensure!(
                request.budget.calls <= partition.resources.calls
                    && request.budget.candidates <= partition.resources.candidates
                    && request.budget.max_bytes <= partition.resources.bytes
                    && request.budget.rounds <= partition.parent_budget.rounds
                    && request.budget.fanout <= partition.parent_budget.fanout
                    && request.budget.deadline_ms <= partition.parent_budget.deadline_ms
                    && (request.budget.input_tokens <= partition.resources.input_tokens
                        || (partition.resources.input_tokens == 0
                            && request.budget.input_tokens == 1)),
                "joint public child budget exceeds private allocation"
            );
            (
                partition.assessment_ordinals.clone(),
                partition.resources.input_tokens,
                partition.parent_budget.clone(),
            )
        } else {
            (
                (1..=request.budget.rounds.min(3))
                    .map(|ordinal| ordinal as u8)
                    .collect(),
                request.budget.input_tokens,
                request.budget.clone(),
            )
        };
        let mut native_budget = request.budget.clone();
        native_budget.deadline_ms = deadline
            .saturating_duration_since(started)
            .as_millis()
            .min(30_000) as u64;
        ensure!(
            native_budget.deadline_ms > 0,
            "joint original deadline exhausted"
        );
        let mut gather = Gathering::new(
            ContextRequest {
                schema: REQUEST_SCHEMA.into(),
                question: request.question.clone(),
                budget: native_budget,
            },
            scope.clone(),
            application,
            started,
        )?;
        #[cfg(not(feature = "local-inference"))]
        let _ = (&ordinals, token_limit, &parent_budget);
        let initial_reason = initial_reason(
            &gather.output.applications[0],
            token_limit,
            ordinals.is_empty(),
        );
        let mut joint = Joint {
            history: vec![],
            coverage: PartitionCoverage::Unassessed {
                reason: initial_reason,
            },
            gaps: vec![Gap::Unsupported],
            terminal: None,
            revoked: false,
            #[cfg(feature = "local-inference")]
            blocked: false,
            #[cfg(feature = "local-inference")]
            ordinals,
            #[cfg(feature = "local-inference")]
            token_limit,
        };
        loop {
            require_live()?;
            ensure!(
                self.scoped_read_fingerprint()? == Some(context.authority.fingerprint),
                "joint source authority expired"
            );
            let current =
                observed_application(context, &publication, gather.output.applications.first())?;
            // Never actuate an old configuration's optional result. Canonical
            // authority/publication checks continue to own every source read.
            if current.scope.configuration_revision != scope.configuration_revision {
                joint.configuration_changed();
            }
            gather.refresh_application(current.clone())?;
            #[cfg(feature = "local-inference")]
            if !gather.output.records.is_empty()
                && !joint.blocked
                && joint.history.len() < joint.ordinals.len()
                && gather.output.work.input_tokens < joint.token_limit
                && Instant::now() < deadline
                && current.availability == Availability::Ready
            {
                if let Some(attempt) = context.attempt {
                    let eligible = if gather.output.work.calls < gather.request.budget.calls
                        && gather.output.candidate_count < gather.request.budget.candidates
                    {
                        gather
                            .queue
                            .iter()
                            .filter(|dispatch| dispatch.round <= gather.request.budget.rounds)
                            .map(|dispatch| dispatch.choice.id)
                            .collect()
                    } else {
                        BTreeSet::new()
                    };
                    match airk_retrieval::arbiter::context_action::build_roster_with_eligible(
                        &request.question,
                        &scope,
                        &gather.output.records,
                        &gather.choices(),
                        &eligible,
                    ) {
                        Ok(roster) => {
                            let ordinal = joint.ordinals[joint.history.len()];
                            let call = NativeContextActionRequest {
                                read_attempt: attempt,
                                parent_budget: parent_budget.clone(),
                                worker: context.worker,
                                authority: context.authority,
                                scope: scope.clone(),
                                question: request.question.clone(),
                                records: gather.output.records.clone(),
                                choices: gather.choices(),
                                roster: roster.clone(),
                                ordinal,
                                partition: context.partition.clone(),
                                remaining_ms: deadline
                                    .saturating_duration_since(Instant::now())
                                    .as_millis()
                                    .min(30_000)
                                    as u64,
                                remaining_input_tokens: joint
                                    .token_limit
                                    .saturating_sub(gather.output.work.input_tokens),
                            };
                            if call.remaining_ms == 0 || serde_json::to_vec(&call)?.len() > 524_288
                            {
                                joint.blocked = true;
                                joint.invalidate(AssessmentUnavailable::TokenBudget);
                            } else {
                                let returned = context.owner.context_action(call, deadline);
                                let fresh = observed_application(
                                    context,
                                    &publication,
                                    gather.output.applications.first(),
                                )?;
                                let can_actuate = fresh.scope == scope
                                    && fresh.availability == Availability::Ready
                                    && fresh.profile == gather.output.applications[0].profile;
                                match returned {
                                    Ok(Some(advice)) => joint.apply(
                                        &mut gather,
                                        roster,
                                        ordinal,
                                        advice,
                                        Instant::now(),
                                        can_actuate,
                                    )?,
                                    Ok(None) => {
                                        joint.blocked = true;
                                        joint.invalidate(AssessmentUnavailable::TaskUnsupported);
                                    }
                                    Err(error)
                                        if error
                                            .is::<airk_core::scoped_reads::ScopeReadRefused>() =>
                                    {
                                        return Err(error);
                                    }
                                    Err(_) => {
                                        gather.optional_failure=Some("Optional joint action was not applied: native outcome and work may be unknown; no replay occurred.".into());
                                        joint.lost(ordinal, &scope, &roster)
                                    }
                                }
                                gather.refresh_application(fresh)?;
                                if !can_actuate {
                                    joint.configuration_changed();
                                }
                            }
                        }
                        Err(_) => {
                            joint.blocked = true;
                            joint.invalidate(AssessmentUnavailable::NoEvidence);
                        }
                    }
                } else {
                    joint.blocked = true;
                    joint.invalidate(AssessmentUnavailable::Unavailable);
                }
            }
            #[cfg(not(feature = "local-inference"))]
            let _ = &parent_budget;
            if joint.terminal.is_some() {
                break;
            }
            let Some(dispatch) = gather.next(Instant::now())? else {
                break;
            };
            if !joint.history.is_empty() {
                joint.invalidate(AssessmentUnavailable::NewEvidence);
            }
            require_live()?;
            let result = reader(dispatch.tool, &dispatch.arguments, Some(&dispatch.revision));
            gather.observe(result, Instant::now())?;
        }
        require_live()?;
        ensure!(
            self.scoped_read_fingerprint()? == Some(context.authority.fingerprint)
                && self.arbiter_publication()? == publication,
            "joint source authority/publication changed before delivery"
        );
        let final_application =
            observed_application(context, &publication, gather.output.applications.first())?;
        if !policy_matches(&final_application, &scope, &joint.history) {
            joint.configuration_changed();
        }
        gather.refresh_application(final_application)?;
        let stop = joint.terminal.unwrap_or(match gather.output.stop {
            StopReason::Deadline => PartitionTermination::Deadline,
            StopReason::Budget => PartitionTermination::Budget,
            StopReason::Stale => PartitionTermination::Stale,
            StopReason::Refused => PartitionTermination::Refused,
            StopReason::Unsupported => PartitionTermination::Unsupported,
            _ => PartitionTermination::NoProgress,
        });
        let optional_failure = gather.optional_failure.clone();
        let native = gather.finish_native(Instant::now(), false)?;
        let mut response = ContextResponseV2 {
            schema: CONTEXT_SCHEMA_V2.into(),
            packet_schema: PACKET_SCHEMA.into(),
            question: native.question,
            records: native.records,
            paragraphs: native.paragraphs,
            gaps: joint.gaps,
            stop,
            work: native.work,
            candidate_count: native.candidate_count,
            omitted_count: native.omitted_count,
            assessment_history: joint.history,
            assessment: PartitionAssessment {
                scope,
                coverage: joint.coverage,
                termination: stop,
            },
            applications: native.applications,
        };
        if joint.revoked {
            return Err(JointPostExecutionStale {
                _custody: Box::new(response),
            }
            .into());
        }
        let organized = organize(&response)?;
        response.paragraphs.extend(organized.iter().cloned());
        pack(
            &mut response,
            self.scope_observation()
                .map(serde_json::to_value)
                .transpose()?
                .as_ref(),
            request.budget.max_bytes,
        )?;
        if Instant::now() >= deadline {
            response.paragraphs.retain(|paragraph| {
                !organized.iter().any(|group| {
                    group.text == paragraph.text
                        && group.evidence == paragraph.evidence
                        && group.interpretation == paragraph.interpretation
                })
            });
            for event in &mut response.assessment_history {
                if event.applicability == AssessmentApplicability::Current {
                    event.applicability = AssessmentApplicability::HistoricalAfterRead;
                }
            }
            response.stop = PartitionTermination::Deadline;
            response.assessment.termination = response.stop;
            response.assessment.coverage = PartitionCoverage::Unassessed {
                reason: AssessmentUnavailable::Deadline,
            };
            response.gaps = vec![Gap::Unsupported];
        }
        require_live()?;
        ensure!(
            self.scoped_read_fingerprint()? == Some(context.authority.fingerprint)
                && self.arbiter_publication()? == publication,
            "joint source authority/publication changed during packing"
        );
        let mut latest =
            observed_application(context, &publication, response.applications.first())?;
        let policy_current = policy_matches(
            &latest,
            &response.assessment.scope,
            &response.assessment_history,
        );
        if let Some(reason) = optional_failure {
            latest.availability = Availability::Failed;
            latest.reason = reason;
        }
        if !policy_current {
            response.applications = vec![latest];
            for event in &mut response.assessment_history {
                if event.applicability == AssessmentApplicability::Current {
                    event.applicability =
                        AssessmentApplicability::HistoricalAfterConfigurationChange;
                }
            }
            response.assessment.coverage = PartitionCoverage::Unassessed {
                reason: AssessmentUnavailable::Unavailable,
            };
            response.stop = PartitionTermination::NoProgress;
            response.assessment.termination = response.stop;
            response.gaps = vec![Gap::Unsupported];
            response.paragraphs.retain(|paragraph| {
                !organized.iter().any(|group| {
                    group.text == paragraph.text && group.evidence == paragraph.evidence
                })
            });
            if response.assessment_history.iter().any(has_native_receipt) {
                return Err(JointPostExecutionStale {
                    _custody: Box::new(response),
                }
                .into());
            }
        } else {
            response.applications = vec![latest];
        }
        response.validate(&request.budget)?;
        Ok(response)
    }
}

fn initial_reason(
    application: &IndexApplication,
    tokens: usize,
    no_ordinals: bool,
) -> AssessmentUnavailable {
    match application.availability {
        Availability::Disabled => AssessmentUnavailable::Disabled,
        Availability::IndexDisabled => AssessmentUnavailable::NotEnrolled,
        Availability::UnsupportedTask => AssessmentUnavailable::TaskUnsupported,
        Availability::Ready if tokens == 0 || no_ordinals => AssessmentUnavailable::TokenBudget,
        Availability::Ready => AssessmentUnavailable::TaskUnsupported,
        _ => AssessmentUnavailable::Unavailable,
    }
}
fn policy_matches(
    application: &IndexApplication,
    scope: &ScopeBinding,
    history: &[AssessmentEvent],
) -> bool {
    if !history.iter().any(has_native_receipt) {
        return application.scope == *scope;
    }
    application.scope == *scope
        && application.availability == Availability::Ready
        && history
            .iter()
            .filter_map(|event| match &event.result {
                AssessmentResult::Selected { advice, .. }
                | AssessmentResult::Abstained { advice } => Some(advice),
                AssessmentResult::Unavailable { advice, .. } => advice.as_ref(),
            })
            .all(|advice| application.profile == Some(advice.profile))
}

/// Actual work stays private when policy withdrawal forbids delivering any NN
/// receipt. This error exposes only a content-free, nonreplayable refusal.
pub struct JointPostExecutionStale {
    _custody: Box<ContextResponseV2>,
}
impl std::fmt::Debug for JointPostExecutionStale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JointPostExecutionStale")
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for JointPostExecutionStale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "Optional read policy changed after native work; the stale answer cannot be replayed.",
        )
    }
}
impl std::error::Error for JointPostExecutionStale {}
impl JointPostExecutionStale {
    pub fn receipt(&self) -> Value {
        json!({"error":{"code":"arbiter_context_action_stale","retryable":false,"query_replay_safe":false,
            "message":self.to_string()}})
    }
}

fn organize(response: &ContextResponseV2) -> Result<Vec<GroundedParagraph>> {
    let Some(event) = response
        .assessment_history
        .last()
        .filter(|event| event.applicability == AssessmentApplicability::Current)
    else {
        return Ok(vec![]);
    };
    let AssessmentResult::Selected { card, .. } = &event.result else {
        return Ok(vec![]);
    };
    // These are native question-facing extractive groups, not generated facts.
    // Every quoted byte and role hypothesis retains its exact unit dependency.
    let mut grouped = std::collections::BTreeMap::<String, (String, Vec<Sha256Digest>)>::new();
    for assessed in &card.assessments {
        if assessed.role == EvidenceRole::Unknown
            && assessed.relevance == RelevanceHypothesis::Uncertain
        {
            continue;
        }
        assessed
            .evidence
            .validate(&response.records, &response.assessment.scope)?;
        let record = response
            .records
            .iter()
            .find(|record| record.id == assessed.evidence.record)
            .unwrap();
        let hit: Hit = serde_json::from_value(
            record
                .canonical_result
                .pointer(&assessed.evidence.result_pointer)
                .unwrap()
                .clone(),
        )?;
        let (text, ids) = grouped
            .entry(format!("{:?} / {:?}", assessed.role, assessed.relevance))
            .or_default();
        text.push_str(&format!(
            "\n\n{}:\n{}",
            hit.citation.path,
            &hit.text[assessed.evidence.utf8_start..assessed.evidence.utf8_end]
        ));
        if !ids.contains(&record.id) {
            ids.push(record.id);
        }
    }
    let mut paragraphs = Vec::new();
    for (role, (text, evidence)) in grouped {
        paragraphs.push(GroundedParagraph {
            text: format!("Evidence assessed as {role}:{text}"),
            evidence,
            interpretation: true,
        });
    }
    Ok(paragraphs)
}

fn pack(response: &mut ContextResponseV2, scope: Option<&Value>, maximum: usize) -> Result<()> {
    let organized = organize(response)?;
    let active_ordinal = response
        .assessment_history
        .last()
        .filter(|event| event.applicability == AssessmentApplicability::Current)
        .map(|event| event.ordinal);
    loop {
        let mut value = serde_json::to_value(&*response)?;
        if let Some(scope) = scope {
            value["scope_observation"] = scope.clone();
        }
        if serde_json::to_vec(&*response)?.len() <= maximum
            && serde_json::to_vec(&serde_json::to_string(&value)?)?
                .len()
                .saturating_add(4096)
                <= airk_core::MAX_RESPONSE_BYTES
        {
            break;
        }
        ensure!(
            response.records.len() > 1,
            "joint mandatory initial receipt/metadata does not fit native packet"
        );
        let omitted = response.records.pop().unwrap().id;
        response.omitted_count += 1;
        response
            .paragraphs
            .retain(|paragraph| !paragraph.evidence.contains(&omitted));
        let mut invalidated = false;
        for event in &mut response.assessment_history {
            if event
                .basis
                .source_units_presented
                .iter()
                .any(|unit| unit.record == omitted)
            {
                event.applicability = AssessmentApplicability::BasisOmitted;
                invalidated |= Some(event.ordinal) == active_ordinal;
            }
        }
        if invalidated {
            response.paragraphs.retain(|paragraph| {
                !organized.iter().any(|group| {
                    group.text == paragraph.text
                        && group.evidence == paragraph.evidence
                        && group.interpretation == paragraph.interpretation
                })
            });
            response.assessment.coverage = PartitionCoverage::Unassessed {
                reason: AssessmentUnavailable::BasisOmitted,
            };
            response.stop = PartitionTermination::Budget;
            response.assessment.termination = response.stop;
            response.gaps = vec![Gap::Unsupported];
        }
    }
    Ok(())
}

#[cfg(all(test, feature = "local-inference"))]
mod tests;
