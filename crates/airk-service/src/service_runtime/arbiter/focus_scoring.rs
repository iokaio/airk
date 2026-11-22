// SPDX-License-Identifier: Apache-2.0
//! Exact native pair-prefix execution under the original shared read budget.
use super::*;
use airk_core::native_focus::{NativeFocusRequest, NativeFocusResponse};

impl Owner {
    pub(in crate::service_runtime) async fn focus_rank(
        &self,
        request: NativeFocusRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<Option<NativeFocusResponse>> {
        self.focus.focus_rank(request, supervisor).await
    }
}
impl StageOwner {
    async fn focus_rank(
        &self,
        request: NativeFocusRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<Option<NativeFocusResponse>> {
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
            .context("Focus deadline overflow")?;
        validate_partition_selection(request.partition.as_ref(), request.authority.selection)?;
        let application = self.application(
            request.worker,
            request.authority,
            request.scope.publication.clone(),
        )?;
        ensure!(
            application.scope == request.scope,
            "Focus source publication changed"
        );
        #[cfg(feature = "local-inference")]
        let _model_use = self.begin_model_use(supervisor).await?;
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
                KnownScoreFailure, ScoreRequest,
            };
            use airk_retrieval::arbiter::compiler::Candidate;
            let execution = self.profile_description(
                configuration
                    .selected_profile(Stage::Focus)
                    .context("Focus selected profile absent")?,
            )?;
            let profile = execution;
            profile.validate_stage(Stage::Focus)?;
            // CPU admission may carry the exact native closed source-transition
            // witness. GPU has no such bridge and remains current-policy-only.
            let native_execution = if profile.device_execution.is_none() {
                Some(self.execution_profile(profile.manifest)?)
            } else {
                None
            };
            let task_policy = validated_focus_task_policy(&profile, native_execution.as_ref())?;
            ensure!(
                request.score_count <= profile.max_candidates,
                "Focus prefix exceeds qualified profile"
            );
            let reserved = profile
                .max_tokens
                .checked_mul(request.score_count)
                .context("Focus reservation overflow")?;
            ensure!(
                reserved > 0 && reserved <= request.remaining_input_tokens,
                "Focus prefix exceeds original token allocation"
            );
            let mut original = request.read_attempt;
            original.remaining_ns = original.remaining_ns.min(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_nanos()
                    .min(u64::MAX as u128) as u64,
            );
            let ticket = self
                .joint_ledger
                .lock()
                .map_err(|_| anyhow::anyhow!("shared NN ledger unavailable"))?
                .reserve_focus(
                    original,
                    &request.parent_budget,
                    joint_ledger::FocusInvocation {
                        index_id: request.scope.index_id,
                        ordinal: request.ordinal,
                    },
                    reserved,
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
            let order = request.candidate_order();
            let mut receipt = FocusApplicationReceipt {
                schema: airk_core::arbiter::focus::RANKING_SCHEMA.into(),
                scope: request.scope.clone(),
                identity: Some(FocusIdentity {
                    profile: profile.manifest,
                    runtime: profile.advice_runtime()?,
                    compiler: profile.compiler,
                    calibration: profile.calibration,
                    task_policy,
                }),
                availability: Availability::Failed,
                reason: "Native Focus outcome is unknown; the original reservation remains held."
                    .into(),
                applied: false,
                input_digest: request.input_digest()?,
                original_order: order.clone(),
                ranked_order: order,
                scores: vec![],
                presentations: vec![],
                presented_count: 0,
                candidate_count: request.candidates.len(),
                scored_count: 0,
                omitted_count: request.candidates.len(),
                reserved_input_tokens: reserved,
                observed_input_tokens: None,
                work: Work::default(),
            };
            let pending_handle = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                supervisor.arbiter_management().existing(),
            )
            .await;
            let handle = match pending_handle {
                Ok(Ok(handle)) => handle,
                _ => return focus_response(receipt, &request),
            };
            let gate = self.gate.clone();
            let store = self.policy_reader()?;
            let worker = request.worker;
            let authority = request.authority;
            let manifest = profile.manifest;
            let live: airk_inference::inference_engine::pool::arbiter::Authority =
                Arc::new(move |scope| {
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
                                .is_ok_and(|issue| {
                                    gate.check_scope_issue(issue, worker, authority).is_ok()
                                })
                    })
                });
            let mut tokens = 0usize;
            let mut queue_us = 0u64;
            let mut compile_us = 0u64;
            let mut inference_us = 0u64;
            let mut settled = true;
            for candidate in request.candidates.iter().take(request.score_count) {
                if Instant::now() >= deadline || !live(&request.scope) {
                    receipt.reason = "Original Focus deadline, source authority or configuration changed; original order retained.".into();
                    break;
                }
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
                            question: request.question.clone(),
                            candidates: vec![Candidate {
                                id: candidate.id.to_string(),
                                text: candidate.window.text(&candidate.hit)?.to_owned(),
                            }],
                            resident,
                            admission,
                            max_input_tokens: profile.max_tokens,
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
                    _ => {
                        settled = false;
                        break;
                    }
                };
                receipt.presentations.push(FocusPresentation {
                    candidate: candidate.id,
                    utf8_start: candidate.window.utf8_start,
                    utf8_end: candidate.window.utf8_end,
                    source_bytes: candidate.hit.text.len(),
                });
                // A compiled pair is known work, but does not settle its FFI reservation.
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
                        .joint_ledger
                        .lock()
                        .map_err(|_| anyhow::anyhow!("shared NN ledger unavailable"))?
                        .compiled_prefix(ticket, tokens)
                        .is_err()
                    {
                        settled = false;
                        receipt.reason = "Known Focus compilation contradicted its original reservation; original order retained.".into();
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
                    Ok(Ok(Err(error))) => {
                        receipt.reason = "Native Focus pair completed with a failure; no partial ranking was applied.".into();
                        (
                            error.downcast_ref::<KnownScoreFailure>().map(|failure| {
                                (
                                    failure.receipt.input_tokens,
                                    failure.receipt.queue_us,
                                    failure.receipt.compile_us,
                                    failure.receipt.inference_us,
                                )
                            }),
                            None,
                        )
                    }
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
                    receipt.reason = "Native Focus compilation and completion receipts differ; original order retained.".into();
                    break;
                }
                inference_us = inference_us.saturating_add(inferred);
                let Some(logits) = logits else {
                    break;
                };
                let score = if let [no, yes] = logits.as_slice() {
                    FocusScore::from_logits(candidate.id, *no, *yes).ok()
                } else {
                    None
                };
                let Some(score) = score else {
                    receipt.reason =
                        "Native Focus returned invalid two-logit scores; original order retained."
                            .into();
                    break;
                };
                receipt.scores.push(score);
            }
            receipt.presented_count = receipt.presentations.len();
            receipt.scored_count = receipt.scores.len();
            receipt.omitted_count = receipt.candidate_count - receipt.presented_count;
            receipt.work.candidates = receipt.scored_count;
            receipt.work.queue_us = Some(queue_us);
            receipt.work.compile_us = Some(compile_us);
            receipt.work.inference_us = settled.then_some(inference_us);
            if settled {
                receipt.observed_input_tokens = Some(tokens);
                receipt.work.input_tokens = tokens;
                if tokens > 0
                    && self
                        .joint_ledger
                        .lock()
                        .map_err(|_| anyhow::anyhow!("shared NN ledger unavailable"))?
                        .completed_prefix(ticket, tokens)
                        .is_err()
                {
                    receipt.reason = "Observed Focus tokens exceeded or contradicted their reservation; original order retained.".into();
                } else if receipt.scored_count == request.score_count
                    && Instant::now() < deadline
                    && live(&request.scope)
                {
                    let policy = Instant::now();
                    let mut scored = receipt.scores.iter().enumerate().collect::<Vec<_>>();
                    scored.sort_by(|(left_i, left), (right_i, right)| {
                        right
                            .relevance
                            .total_cmp(&left.relevance)
                            .then(left_i.cmp(right_i))
                    });
                    receipt.ranked_order = scored
                        .into_iter()
                        .map(|(_, score)| score.candidate)
                        .chain(
                            receipt.original_order[request.score_count..]
                                .iter()
                                .copied(),
                        )
                        .collect();
                    receipt.applied = true;
                    receipt.availability = Availability::Ready;
                    receipt.reason = "Complete native Focus prefix ranked by relevance; the unscored tail and full source bodies are preserved.".into();
                    receipt.work.policy_us =
                        Some(policy.elapsed().as_micros().min(u64::MAX as u128) as u64);
                }
            }
            // A later source/configuration change discards application, never work.
            if !live(&request.scope) || Instant::now() >= deadline {
                receipt.applied = false;
                receipt.availability = Availability::Failed;
                receipt.ranked_order = receipt.original_order.clone();
                receipt.reason = "Original Focus delivery authority, configuration or deadline changed; measured work retained without ranking.".into();
            }
            focus_response(receipt, &request)
        }
    }
}
#[cfg(feature = "local-inference")]
fn focus_response(
    ranking: FocusApplicationReceipt,
    request: &NativeFocusRequest,
) -> Result<Option<NativeFocusResponse>> {
    let response = NativeFocusResponse { ranking };
    response.validate(request)?;
    Ok(Some(response))
}

// This follows Core native-request validation; it grants no source authority.
fn validate_partition_selection(
    partition: Option<&airk_core::combined_context::PartitionAllowance>,
    selection: Option<airk_core::scoped_reads::SelectionScopeRef>,
) -> Result<()> {
    if let Some(partition) = partition {
        partition.validate()?;
        if partition.is_ordinary_focus() {
            ensure!(
                selection.is_some_and(|selection| partition.selection == selection.scope_identity),
                "Focus partition differs from the actual installed selection"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(feature = "local-inference")]
fn validated_focus_task_policy(
    profile: &Profile,
    execution: Option<&airk_retrieval::arbiter::runtime::ExecutionProfile>,
) -> Result<airk_core::identity::Sha256Digest> {
    profile.validate_stage(Stage::Focus)?;
    if profile.device_execution.is_none() {
        let execution = execution.context("Focus CPU requires actual native profile admission")?;
        ensure!(
            serde_json::to_vec(execution.description())? == serde_json::to_vec(profile)?,
            "Focus native admitted description changed"
        );
        return execution.validated_task_policy(Task::FocusRank);
    }
    ensure!(
        execution.is_none(),
        "GPU task policy cannot use CPU source-transition admission"
    );
    let current = airk_retrieval::arbiter::compiler::focus::policy_digest()?;
    ensure!(
        profile.task_policies.get(&Task::FocusRank) == Some(&current),
        "GPU Focus task policy changed"
    );
    Ok(current)
}
