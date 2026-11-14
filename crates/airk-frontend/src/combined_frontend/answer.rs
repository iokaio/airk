// SPDX-License-Identifier: Apache-2.0
//! Exactly-once local packets under one whole-selection native allowance.
use super::*;
use airk_core::{
    arbiter::context_action::{AssessmentResult, ContextResponseV2},
    combined_context::{
        CombinedContextSummary, PartitionOutcome, PartitionReport, Resources, SUMMARY_SCHEMA,
        Termination, UnqueriedReason, initial_allocations,
    },
};

/// Bounded transient native-result custody; no Context or guard references.
struct ContextResultCustody(std::sync::Mutex<(Vec<Value>, Option<Value>)>);
const NATIVE_CUSTODY_PACKET_BYTES: usize = 3 * airk_core::service::MAX_FRAME_BYTES + 4096;
const NATIVE_CUSTODY_WHOLE_BYTES: usize = airk_core::index_catalog::MAX_SELECTION
    * NATIVE_CUSTODY_PACKET_BYTES
    + MAX_PUBLIC_TOOL_ENVELOPE_BYTES;
impl ContextResultCustody {
    fn packet(&self, value: &Value) -> Result<()> {
        airk_core::index_catalog::encoded_size(value, NATIVE_CUSTODY_PACKET_BYTES)?;
        let mut held = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("context result custody unavailable"))?;
        ensure!(
            held.0.len() < airk_core::index_catalog::MAX_SELECTION,
            "context result custody roster bound exceeded"
        );
        held.0.push(value.clone());
        Ok(())
    }
    fn wrapper(&self, value: &Value) -> Result<()> {
        airk_core::index_catalog::encoded_size(value, NATIVE_CUSTODY_WHOLE_BYTES)?;
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("context result custody unavailable"))?
            .1 = Some(value.clone());
        Ok(())
    }
}
pub(super) fn retain_context_wrapper(
    context: &airk_mcp::read_attempt::ReadAttemptContext,
    value: &Value,
) -> Result<()> {
    if let Some(custody) = context.lifetime_guard_of::<ContextResultCustody>() {
        custody.wrapper(value)?;
    }
    Ok(())
}
fn postexecution_failure(code: &str) -> Value {
    native_envelope(
        json!({"error":{"code":code,"retryable":false,"query_replay_safe":false,"query_submitted":true},
        "outcome_unknown":true,"partial":true}),
    )
}

fn live(context: &airk_mcp::read_attempt::ReadAttemptContext) -> bool {
    !context.cancellation().is_cancelled() && Instant::now() < context.deadline()
}

fn packet_work(packet: &ContextResponseV2) -> Result<Resources> {
    Resources::from_packet(packet)
}

fn packet_unknown(packet: &ContextResponseV2) -> bool {
    packet.assessment_history.iter().any(|event| {
        matches!(
            &event.result,
            AssessmentResult::Unavailable {
                work_unknown: true,
                ..
            }
        )
    })
}

fn charged_ordinals(packet: &ContextResponseV2) -> Vec<u8> {
    packet
        .assessment_history
        .iter()
        .filter_map(|event| {
            let tokens = match &event.result {
                AssessmentResult::Selected { advice, .. }
                | AssessmentResult::Abstained { advice } => advice.work.input_tokens,
                AssessmentResult::Unavailable { work, .. } => work.input_tokens,
            };
            (tokens > 0).then_some(event.ordinal)
        })
        .collect()
}

fn unused(reserved: &Resources, used: &Resources) -> Result<Resources> {
    Ok(Resources {
        calls: reserved
            .calls
            .checked_sub(used.calls)
            .context("context call overrun")?,
        input_tokens: reserved
            .input_tokens
            .checked_sub(used.input_tokens)
            .context("context token overrun")?,
        candidates: reserved
            .candidates
            .checked_sub(used.candidates)
            .context("context candidate overrun")?,
        bytes: reserved
            .bytes
            .checked_sub(used.bytes)
            .context("context byte overrun")?,
    })
}

fn retain_failure_or_unknown(value: Value) -> Value {
    if result_failed(&value) {
        return value;
    }
    // The query already returned a nominal success. Invalid accounting/schema
    // cannot establish no work, safe replay or a pre-submission scope refusal.
    native_envelope(json!({"error":{"code":"invalid_native_context_packet",
        "retryable":false,"query_replay_safe":false,"query_submitted":true},
        "outcome_unknown":true,"partial":true}))
}

impl<S: Session> Backend<S> {
    pub(super) async fn execute_answer(
        &self,
        mut args: Map<String, Value>,
        selected: &QuerySelection,
        before: Snapshot<S::Authority, S::Child>,
        context: airk_mcp::read_attempt::ReadAttemptContext,
    ) -> Result<Value> {
        use airk_kernel::combined_reads::{CompositionRequest, MemberInput, compose_context};
        ensure!(
            context.context_partition().is_none(),
            "allocated partition cannot recursively expand a combined scope"
        );
        context.begin_context_answer()?;
        let custody = Arc::new(ContextResultCustody(std::sync::Mutex::new((
            Vec::new(),
            None,
        ))));
        context.retain_lifetime_guard(custody.clone())?;
        let origin = args
            .remove("origin_publication")
            .map(serde_json::from_value::<String>)
            .transpose()?;
        if origin
            .as_ref()
            .is_some_and(|value| value.parse::<airk_core::identity::RevisionToken>().is_err())
        {
            return Ok(failure(
                "invalid_native_arguments",
                Some(false),
                &self.selection.requested(),
            ));
        }
        if origin.is_some() && before.members.len() != 1 {
            return Ok(failure(
                "originating_index_required",
                Some(false),
                &self.selection.requested(),
            ));
        }
        if airk_mcp::combined::validate_native_arguments("airk_answer", &args).is_err() {
            return Ok(failure(
                "invalid_native_arguments",
                Some(false),
                &self.selection.requested(),
            ));
        }
        let parsed: airk_mcp::AnswerArgs = serde_json::from_value(Value::Object(args.clone()))?;
        parsed.request.validate()?;
        let request = parsed.request;
        let ids: Vec<_> = before
            .members
            .iter()
            .map(|member| member.index.id)
            .collect();
        let attempt = match context.request_id() {
            Some(id) => id,
            None => airk_kernel::service_broker::new_instance_id()?,
        };
        let context = context.with_request_id(attempt)?;
        let captured = airk_host::read_attempt::encode(&context, attempt)?;
        let context = context.capture_transport_attempt(captured)?;
        let mut allowances = initial_allocations(attempt, before.identity, &ids, &request.budget)?;
        let outer_bytes = request.budget.max_bytes.min(airk_core::MAX_RESPONSE_BYTES);
        let admitted = allowances.iter().flatten().count();
        for (offset, allowance) in allowances.iter_mut().flatten().enumerate() {
            let share = outer_bytes / admitted + usize::from(offset < outer_bytes % admitted);
            allowance.resources.bytes = allowance.resources.bytes.min(share);
            if allowance.resources.bytes < 1024 {
                // No native dispatch may consume an invalid minimum allocation.
                allowance.resources.bytes = 0;
            }
        }
        for allowance in &mut allowances {
            if allowance
                .as_ref()
                .is_some_and(|value| value.resources.bytes == 0)
            {
                *allowance = None;
            }
        }
        let observed_from = now_ms();
        let mut inputs = Vec::new();
        let mut reports = Vec::with_capacity(ids.len());
        let mut known = Resources::default();
        let mut held = Resources::default();
        let mut unknown_seen = false;
        let mut accounting_violation = false;
        let mut delivery_bindings = Vec::new();
        let mut reclaimed = Resources::default();
        let mut returned_ordinals = std::collections::BTreeSet::new();
        let mut termination = Termination::Complete;
        for (member, mut allocation) in before.members.iter().zip(allowances) {
            if !accounting_violation && !unknown_seen && live(&context) {
                if let Some(current) = &mut allocation {
                    current.resources = current.resources.checked_add(&reclaimed)?;
                    current
                        .assessment_ordinals
                        .extend(returned_ordinals.iter().copied());
                    current.assessment_ordinals.sort_unstable();
                    reclaimed = Resources::default();
                    returned_ordinals.clear();
                } else if reclaimed.calls > 0 && reclaimed.candidates > 0 && reclaimed.bytes >= 1024
                {
                    allocation = Some(airk_core::combined_context::PartitionAllowance {
                        schema: airk_core::combined_context::ALLOWANCE_SCHEMA.into(),
                        attempt,
                        selection: before.identity,
                        index_id: member.index.id,
                        parent_budget: request.budget.clone(),
                        resources: std::mem::take(&mut reclaimed),
                        assessment_ordinals: std::mem::take(&mut returned_ordinals)
                            .into_iter()
                            .collect(),
                    });
                }
                if let Some(current) = &mut allocation
                    && current.assessment_ordinals.is_empty()
                {
                    current.resources.input_tokens = 0;
                }
            }
            let reason = if accounting_violation {
                Some(UnqueriedReason::AccountingViolation)
            } else if context.cancellation().is_cancelled() {
                Some(UnqueriedReason::Canceled)
            } else if Instant::now() >= context.deadline() {
                Some(UnqueriedReason::Deadline)
            } else if allocation.is_none() {
                Some(UnqueriedReason::Budget)
            } else {
                None
            };
            if let Some(reason) = reason {
                termination = match reason {
                    UnqueriedReason::Canceled => Termination::Canceled,
                    UnqueriedReason::Deadline => Termination::Deadline,
                    UnqueriedReason::AccountingViolation => Termination::AccountingViolation,
                    _ => Termination::Budget,
                };
                reports.push(PartitionReport {
                    index_id: member.index.id,
                    allowance: None,
                    outcome: PartitionOutcome::Unqueried { reason },
                });
                continue;
            }
            let mut allocation = allocation.context("context allocation missing")?;
            if unknown_seen {
                allocation.assessment_ordinals.clear();
                allocation.resources.input_tokens = 0;
            }
            allocation.validate()?;
            let mut child_budget = request.budget.clone();
            child_budget.calls = allocation.resources.calls;
            child_budget.input_tokens = allocation.resources.input_tokens.max(1);
            child_budget.candidates = allocation.resources.candidates;
            child_budget.max_bytes = allocation.resources.bytes;
            let mut child_args = args.clone();
            child_args.insert("budget".into(), serde_json::to_value(&child_budget)?);
            child_args.insert("response_mode".into(), json!("full"));
            let child = self.child(member)?;
            let started = Instant::now();
            let outcome = tokio::time::timeout_at(
                tokio::time::Instant::from_std(context.deadline()),
                child.call_with_read_context(
                    "airk_answer".into(),
                    child_args,
                    1,
                    context.clone().with_context_partition(allocation.clone())?,
                ),
            )
            .await;
            let mut value = match outcome {
                Ok(Ok(value)) => value,
                Ok(Err(_)) => failure(
                    "member_observation_unavailable",
                    None,
                    &self.selection.requested(),
                ),
                Err(_) => failure(
                    "member_deadline_exhausted",
                    None,
                    &self.selection.requested(),
                ),
            };
            custody.packet(&value)?;
            let body = canonical_body(&value);
            let packet = ContextResponseV2::from_canonical(&body, &child_budget);
            let mut sources = Vec::new();
            let report = match packet {
                Ok(packet) => {
                    ensure!(
                        packet.question == request.question
                            && packet.assessment.scope.index_id == member.index.id
                            && packet.assessment.scope.authorization
                                == member.authority.member_fingerprint()?,
                        ScopeReadRefused
                    );
                    if let Some(expected) = &origin {
                        ensure!(
                            packet.assessment.scope.publication.token.to_string() == *expected,
                            ScopeReadRefused
                        );
                    }
                    let used = packet_work(&packet)?;
                    if let Some(binding) =
                        airk_core::combined_context::ContextDeliveryBinding::from_packet(&packet)?
                    {
                        delivery_bindings.push(binding);
                    }
                    let ordinals = charged_ordinals(&packet);
                    ensure!(
                        ordinals
                            .iter()
                            .all(|ordinal| allocation.assessment_ordinals.contains(ordinal)),
                        "native context packet used unallocated assessment ordinal"
                    );
                    let unknown = packet_unknown(&packet);
                    let report = if !used.fits(&allocation.resources) {
                        ensure!(
                            airk_core::combined_context::known_budget_failure(&packet),
                            "successful native packet exceeded private allocation"
                        );
                        accounting_violation = true;
                        termination = Termination::AccountingViolation;
                        known = known.checked_add(&used)?;
                        PartitionOutcome::KnownViolation {
                            observed: used,
                            assessment_ordinals: ordinals,
                            read_depth: packet.work.rounds,
                            peak_fanout: packet.work.fanout,
                        }
                    } else if unknown {
                        unknown_seen = true;
                        termination = Termination::Partial;
                        held = held.checked_add(&allocation.resources)?;
                        PartitionOutcome::Unknown
                    } else {
                        reclaimed = unused(&allocation.resources, &used)?;
                        returned_ordinals.extend(
                            allocation
                                .assessment_ordinals
                                .iter()
                                .filter(|ordinal| {
                                    !packet.assessment_history.iter().any(|event| {
                                        event.ordinal == **ordinal
                                            && airk_core::combined_context::has_native_receipt(
                                                event,
                                            )
                                    })
                                })
                                .copied(),
                        );
                        known = known.checked_add(&used)?;
                        PartitionOutcome::Known {
                            used,
                            assessment_ordinals: ordinals,
                            read_depth: packet.work.rounds,
                            peak_fanout: packet.work.fanout,
                        }
                    };
                    if live(&context) && !accounting_violation {
                        let expected = packet.assessment.scope.publication.token;
                        let projection = tokio::time::timeout_at(
                            tokio::time::Instant::from_std(context.deadline()),
                            child.call_native_with_context(
                                airk_core::native_reads::NativeRead::SourceProjection {
                                    publication: expected,
                                },
                                context.clone(),
                            ),
                        )
                        .await;
                        match projection {
                            Ok(Ok(projection)) => {
                                let parsed = serde_json::from_value::<
                                    airk_core::scoped_reads::ScopedSourceProjection,
                                >(canonical_body(
                                    &projection,
                                ));
                                match parsed {
                                    Ok(projection)
                                        if projection.validate().is_ok()
                                            && projection.publication.token == expected
                                            && projection.scope_fingerprint
                                                == member.authority.member_fingerprint()? =>
                                    {
                                        sources = projection
                                            .sources
                                            .into_iter()
                                            .map(|source| {
                                                airk_kernel::combined_reads::MemberSource {
                                                    source_root_id: source.source_root_id,
                                                    original_root: source.original_root,
                                                    primary: source.primary,
                                                    aliases: source.aliases,
                                                }
                                            })
                                            .collect();
                                    }
                                    _ => {
                                        value = source_projection_failure(&value, Some(projection))
                                    }
                                }
                            }
                            _ => value = source_projection_failure(&value, None),
                        }
                    } else if !accounting_violation {
                        value = source_projection_failure(&value, None);
                    }
                    report
                }
                Err(_) => {
                    // A failure envelope is retained in full; absence of a typed
                    // packet cannot prove no inner read/model work occurred.
                    unknown_seen = true;
                    termination = Termination::Partial;
                    held = held.checked_add(&allocation.resources)?;
                    value = retain_failure_or_unknown(value);
                    PartitionOutcome::Unknown
                }
            };
            inputs.push(MemberInput {
                index_id: member.index.id,
                is_error: accounting_violation || result_failed(&value),
                exhausted: false,
                result: value,
                sources,
                elapsed_ms: Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)),
            });
            reports.push(PartitionReport {
                index_id: member.index.id,
                allowance: Some(allocation),
                outcome: report,
            });
        }
        let summary = CombinedContextSummary {
            schema: SUMMARY_SCHEMA.into(),
            attempt,
            selection: before.identity,
            budget: request.budget.clone(),
            members: reports,
            known_used: known,
            unknown_held: held,
            termination,
            delivery_bindings,
        };
        summary.validate(&ids)?;
        let after = self.selection.snapshot(&self.control, selected).await?;
        ensure!(before.identity == after.identity, ScopeReadRefused);
        let composition = CompositionRequest {
            selection: before.resolved,
            authority: before.identity,
            budgets: SharedBudgets {
                limit: 100,
                max_bytes: outer_bytes,
                deadline_ms: request.budget.deadline_ms,
                verification_max_sources: 0,
                verification_max_bytes: 0,
                verification_deadline_ms: 0,
                limit_per_index: None,
            },
            observed_from_unix_ms: observed_from,
            observed_to_unix_ms: now_ms().max(observed_from),
            index_names: before
                .members
                .iter()
                .map(|member| (member.index.id, member.index.name.clone()))
                .collect(),
        };
        let response = match compose_context(&composition, inputs, summary) {
            Ok(response) => response,
            Err(error) => {
                if let Some(oversized) =
                    error.downcast_ref::<airk_kernel::combined_reads::ContextReplyBudget>()
                {
                    custody.wrapper(&serde_json::to_value(oversized.result.as_ref())?)?;
                    return Ok(postexecution_failure("context_delivery_budget"));
                }
                return Err(error);
            }
        };
        let value = serde_json::to_value(&response)?;
        let result = native_envelope(value);
        let bindings = response
            .context
            .as_ref()
            .map(|summary| summary.delivery_bindings.as_slice())
            .unwrap_or(&[]);
        match self
            .finalize_bound(
                selected,
                before.identity,
                result,
                context.deadline(),
                ReplyBudget::Context {
                    max_bytes: request.budget.max_bytes,
                },
                (bindings, Some(&context)),
            )
            .await
        {
            Ok(value) => Ok(value),
            Err(error) if error.is::<airk_kernel::combined_reads::CombinedReplyBudget>() => {
                Ok(postexecution_failure("context_delivery_budget"))
            }
            Err(_) => Ok(postexecution_failure("context_delivery_refused")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_context_success_is_unknown_nonreplayable_and_failure_bytes_stay_full() {
        let malformed = native_envelope(
            json!({"schema":"unexpected","unvalidated_source":"must not escape","records":[]}),
        );
        let refused = retain_failure_or_unknown(malformed);
        assert!(result_failed(&refused));
        let body = canonical_body(&refused);
        assert_eq!(body["error"]["code"], "invalid_native_context_packet");
        assert_eq!(body["error"]["retryable"], false);
        assert_eq!(body["error"]["query_replay_safe"], false);
        assert_eq!(body["error"]["query_submitted"], true);
        assert_eq!(body["outcome_unknown"], true);
        assert!(!refused.to_string().contains("must not escape"));
        let failure = native_envelope(
            json!({"error":{"code":"original_native_failure","retryable":false},"future_public_data":{"kept":[1,2,3]}}),
        );
        assert_eq!(retain_failure_or_unknown(failure.clone()), failure);
    }
}
