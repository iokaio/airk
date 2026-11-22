// SPDX-License-Identifier: Apache-2.0
//! Exactly-once local packets under one whole-selection native allowance.
use super::*;
mod verification;
use airk_core::{
    arbiter::answer_loop::{ContextResponseV3, reply_limit},
    combined_context::{
        CombinedContextSummaryV2, LoopUse, PartitionOutcomeV3, PartitionReportV3,
        SUMMARY_SCHEMA_V2, Termination, UnqueriedReason, grow_allowance, initial_allocations_v3,
        reply_delivery_bindings, reply_work_unknown, returned_allowance, without_models,
    },
};
use verification::AnswerVerification;

/// The combined Answer's outer reply frame: Answer's 131,072 bytes, not the
/// 65,536 of every other tool.
const ANSWER_REPLY_BYTES: usize = airk_core::arbiter::answer_loop::MAX_ANSWER_RESPONSE_BYTES;

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

/// The fewest reply bytes a member Answer can run on: its request must hold
/// the question and a minimal reply (`ContextRequestV3::validate`).
fn member_minimum_bytes(question: &str) -> usize {
    question
        .len()
        .saturating_add(airk_core::arbiter::answer_loop::MIN_REPLY_SKELETON_BYTES)
}

/// Each member reply is bounded by its share of the combined frame's
/// payload (review finding 2 of L3-10 (a)), and a member is admitted only
/// when that share holds `minimum` bytes (finding 3); the members the frame
/// cannot carry lose their allowance and are reported unqueried.
fn frame_shares(
    allowances: &mut [Option<airk_core::native_loop::PartitionAllowanceV3>],
    payload_bytes: usize,
    minimum: usize,
) {
    let fits = payload_bytes / minimum.max(1);
    let mut kept = 0usize;
    for allowance in allowances.iter_mut() {
        if allowance.is_some() {
            if kept >= fits {
                *allowance = None;
            }
            kept += 1;
        }
    }
    let admitted = allowances.iter().flatten().count().max(1);
    for (offset, allowance) in allowances.iter_mut().flatten().enumerate() {
        let share = payload_bytes / admitted + usize::from(offset < payload_bytes % admitted);
        allowance.resources.bytes = allowance.resources.bytes.min(share);
    }
    for allowance in allowances.iter_mut() {
        if allowance
            .as_ref()
            .is_some_and(|value| value.resources.bytes < minimum.max(1024))
        {
            // No native dispatch may consume an invalid minimum allocation.
            *allowance = None;
        }
    }
}

fn live(context: &airk_mcp::read_attempt::ReadAttemptContext) -> bool {
    !context.cancellation().is_cancelled() && Instant::now() < context.deadline()
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
        use airk_kernel::combined_reads::{CompositionRequest, MemberInput, compose_answer};
        ensure!(
            context.context_partition().is_none() && context.answer_partition().is_none(),
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
        let mut verification = AnswerVerification::parse(&args)?;
        let delivery_reserve = airk_kernel::combined_reads::context_delivery_receipt_reserve()?;
        if request.budget.max_bytes.min(ANSWER_REPLY_BYTES) <= delivery_reserve {
            return Ok(failure(
                "context_delivery_admission_budget",
                Some(false),
                &self.selection.requested(),
            ));
        }
        verification.reserve_delivery_bytes(delivery_reserve)?;
        let attachment_reserve = verification.minimum_attachment_bytes()?;
        if request.budget.max_bytes.min(ANSWER_REPLY_BYTES)
            <= delivery_reserve.saturating_add(attachment_reserve)
        {
            return Ok(failure(
                "context_delivery_admission_budget",
                Some(false),
                &self.selection.requested(),
            ));
        }
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
        let mut allowances =
            initial_allocations_v3(attempt, before.identity, &ids, &request.budget)?;
        let outer_bytes = reply_limit(&request.budget);
        let byte_admission = CompositionRequest {
            selection: before.resolved.clone(),
            authority: before.identity,
            budgets: SharedBudgets {
                limit: 100,
                // `SharedBudgets` keeps the bounds of every other combined
                // read (65,536 bytes, 120 s); an Answer wrapper takes its
                // frame from the `airk.combined-context/2` summary, and its
                // members run to the Answer deadline.
                max_bytes: outer_bytes
                    .saturating_sub(delivery_reserve)
                    .saturating_sub(attachment_reserve)
                    .min(airk_core::MAX_RESPONSE_BYTES),
                deadline_ms: request.budget.deadline_ms.min(120_000),
                verification_max_sources: 0,
                verification_max_bytes: 0,
                verification_deadline_ms: 0,
                limit_per_index: None,
            },
            observed_from_unix_ms: 0,
            observed_to_unix_ms: 0,
            index_names: before
                .members
                .iter()
                .map(|member| (member.index.id, member.index.name.clone()))
                .collect(),
        };
        let payload_bytes = airk_kernel::combined_reads::answer_payload_admission(
            &byte_admission,
            &request.budget,
            &allowances,
            attachment_reserve,
        )?;
        let minimum = member_minimum_bytes(&request.question);
        frame_shares(&mut allowances, payload_bytes, minimum);
        let observed_from = now_ms();
        let mut inputs = Vec::new();
        let mut reports = Vec::with_capacity(ids.len());
        let mut known = LoopUse::default();
        let mut held = LoopUse::default();
        let mut unknown_seen = false;
        let mut delivery_bindings = Vec::new();
        // Unused work of a known member, handed to the next by count.
        let mut reclaimed = LoopUse::default();
        let mut termination = Termination::Complete;
        for (member, mut allocation) in before.members.iter().zip(allowances) {
            if !unknown_seen && live(&context) {
                if let Some(current) = &mut allocation {
                    grow_allowance(current, &std::mem::take(&mut reclaimed))?;
                } else {
                    allocation = returned_allowance(
                        attempt,
                        before.identity,
                        member.index.id,
                        &request.budget,
                        &reclaimed,
                        minimum,
                    )?;
                    if allocation.is_some() {
                        reclaimed = LoopUse::default();
                    }
                }
            }
            let reason = if context.cancellation().is_cancelled() {
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
                    _ => Termination::Budget,
                };
                reports.push(PartitionReportV3 {
                    index_id: member.index.id,
                    allowance: None,
                    outcome: PartitionOutcomeV3::Unqueried { reason },
                });
                continue;
            }
            let mut allocation = allocation.context("context allocation missing")?;
            if unknown_seen {
                without_models(&mut allocation);
            }
            allocation.validate()?;
            let child_budget = allocation.clamp(&request.budget)?;
            let mut child_args = args.clone();
            child_args.insert("budget".into(), serde_json::to_value(&child_budget)?);
            child_args.insert("response_mode".into(), json!("full"));
            verification.child_arguments(&mut child_args);
            let child = self.child(member)?;
            let started = Instant::now();
            let outcome = tokio::time::timeout_at(
                tokio::time::Instant::from_std(context.deadline()),
                child.call_with_read_context(
                    "airk_answer".into(),
                    child_args,
                    1,
                    context.clone().with_answer_partition(allocation.clone())?,
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
            let packet = ContextResponseV3::from_canonical(&body, &child_budget);
            let mut sources = Vec::new();
            let report = match packet {
                Ok(packet) => {
                    ensure!(
                        packet.question == request.question
                            && packet.scope.index_id == member.index.id
                            && packet.scope.authorization
                                == member.authority.member_fingerprint()?,
                        ScopeReadRefused
                    );
                    if let Some(expected) = &origin {
                        ensure!(
                            packet.scope.publication.token.to_string() == *expected,
                            ScopeReadRefused
                        );
                    }
                    let used = LoopUse::of_reply(&packet)?;
                    let body_bytes =
                        airk_kernel::combined_reads::context_member_payload_bytes(&value)?;
                    let focus_delivery_bytes =
                        airk_kernel::combined_reads::answer_reply_focus_binding_bytes(&packet)?;
                    let reclaimable_body_bytes =
                        airk_kernel::combined_reads::context_reclaimable_body_bytes(
                            allocation.resources.bytes,
                            used.bytes,
                            body_bytes,
                            focus_delivery_bytes,
                        )?;
                    delivery_bindings.extend(reply_delivery_bindings(&packet)?);
                    let allowed = LoopUse::allowance(&allocation);
                    // A reply validated under its clamped budget fits its
                    // allowance; anything else keeps the whole allowance held.
                    let report = if reply_work_unknown(&packet) || !used.fits(&allowed) {
                        unknown_seen = true;
                        termination = Termination::Partial;
                        held = held.checked_add(&allowed)?;
                        PartitionOutcomeV3::Unknown
                    } else {
                        reclaimed = allowed.checked_sub(&used)?;
                        // Native Work has its unchanged packet-accounting meaning.
                        // Rendering metadata is separately consumed in the delivery
                        // admission pool and cannot be reallocated to another body.
                        reclaimed.bytes = reclaimable_body_bytes;
                        known = known.checked_add(&used)?;
                        PartitionOutcomeV3::Known {
                            used,
                            read_depth: packet.work.rounds,
                            peak_fanout: packet.work.fanout,
                        }
                    };
                    if live(&context) {
                        let expected = packet.scope.publication.token;
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
                    } else {
                        value = source_projection_failure(&value, None);
                    }
                    report
                }
                Err(_) => {
                    // A failure envelope is retained in full; absence of a typed
                    // packet cannot prove no inner read/model work occurred.
                    unknown_seen = true;
                    termination = Termination::Partial;
                    held = held.checked_add(&LoopUse::allowance(&allocation))?;
                    value = retain_failure_or_unknown(value);
                    PartitionOutcomeV3::Unknown
                }
            };
            inputs.push(MemberInput {
                index_id: member.index.id,
                is_error: result_failed(&value),
                exhausted: false,
                result: value,
                sources,
                elapsed_ms: Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)),
            });
            reports.push(PartitionReportV3 {
                index_id: member.index.id,
                allowance: Some(allocation),
                outcome: report,
            });
        }
        let summary = CombinedContextSummaryV2 {
            schema: SUMMARY_SCHEMA_V2.into(),
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
            selection: before.resolved.clone(),
            authority: before.identity,
            budgets: SharedBudgets {
                limit: 100,
                // `SharedBudgets` keeps the bounds of every other combined
                // read (65,536 bytes, 120 s); an Answer wrapper takes its
                // frame from the `airk.combined-context/2` summary, and its
                // members run to the Answer deadline.
                max_bytes: outer_bytes
                    .saturating_sub(delivery_reserve)
                    .saturating_sub(attachment_reserve)
                    .min(airk_core::MAX_RESPONSE_BYTES),
                deadline_ms: request.budget.deadline_ms.min(120_000),
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
        let response = match compose_answer(&composition, inputs, summary) {
            Ok(response) => response,
            Err(error) => {
                if let Some(oversized) =
                    error.downcast_ref::<airk_kernel::combined_reads::ContextReplyBudget>()
                {
                    let actual = serde_json::to_value(oversized.result.as_ref())?;
                    custody.wrapper(&actual)?;
                    let members: Vec<_> = oversized.result.members.iter().map(|member| {
                        let body = canonical_body(&member.result);
                        Ok(json!({"index_id":member.index_id,"body_bytes":serde_json::to_vec(&body)?.len(),
                            "member_envelope_bytes":serde_json::to_vec(&member.result)?.len(),
                            "allocation_bytes":member.allocation.max_bytes}))
                    }).collect::<Result<_>>()?;
                    let geometry = json!({"phase":"compose_context","canonical_bytes":serde_json::to_vec(&actual)?.len(),
                        "public_envelope_bytes":serde_json::to_vec(&native_envelope(actual.clone()))?.len(),
                        "canonical_cap":outer_bytes,"summary_bytes":serde_json::to_vec(&actual["context"])?.len(),
                        "ranking_bytes":serde_json::to_vec(&actual["ranking"])?.len(),"members":members});
                    let _ = airk_host::service_logs::record_service_error(
                        &self.control,
                        "combined_context",
                        "context_delivery_budget",
                        &anyhow::anyhow!(geometry.to_string()),
                    );
                    return Ok(postexecution_failure("context_delivery_budget"));
                }
                return Err(error);
            }
        };
        let value = serde_json::to_value(&response)?;
        let result = native_envelope(value);
        let result = verification
            .apply(self, result, selected, before.clone(), context.clone())
            .await?;
        let bindings = response
            .context
            .as_ref()
            .map(|summary| summary.delivery_bindings())
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
                let _ = airk_host::service_logs::record_service_error(
                    &self.control,
                    "combined_context",
                    "context_delivery_budget_after_receipt",
                    &error,
                );
                Ok(postexecution_failure("context_delivery_budget"))
            }
            Err(_) => Ok(postexecution_failure("context_delivery_refused")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review findings 2 and 3 of L3-10 (a): member bytes are shares of the
    /// frame payload, not of retention, and a share below the question plus
    /// a minimal reply admits no member (the rest are unqueried).
    #[test]
    fn member_bytes_are_frame_shares_and_admit_only_a_question_sized_reply() -> Result<()> {
        use airk_core::{
            arbiter::answer_loop::BudgetV3, combined_context::initial_allocations_v3,
            identity::Incarnation, index_catalog::IndexId,
        };
        let ids: Vec<IndexId> = (1..=4)
            .map(|byte| IndexId::from_identity(Incarnation::from_bytes([byte; 16])))
            .collect();
        let budget = BudgetV3::default();
        let fresh = || {
            initial_allocations_v3(
                Incarnation::from_bytes([9; 16]),
                Sha256Digest::from_bytes([8; 32]),
                &ids,
                &budget,
            )
        };
        // Retention gives each of four members 65,536 bytes; a 40,000-byte
        // payload gives each 10,000, enough for a 5,904-byte minimum.
        let mut allowances = fresh()?;
        assert_eq!(allowances[0].as_ref().unwrap().resources.bytes, 65_536);
        frame_shares(&mut allowances, 40_000, 4096 + 5_904);
        let bytes: Vec<_> = allowances
            .iter()
            .map(|allowance| allowance.as_ref().map(|a| a.resources.bytes))
            .collect();
        assert_eq!(bytes, vec![Some(10_000); 4]);
        // A 16,384-byte question needs 20,480 bytes: only one member fits,
        // with the whole payload; the others are not admitted.
        let mut allowances = fresh()?;
        frame_shares(&mut allowances, 40_000, member_minimum_bytes(&"q".repeat(16_384)));
        let bytes: Vec<_> = allowances
            .iter()
            .map(|allowance| allowance.as_ref().map(|a| a.resources.bytes))
            .collect();
        assert_eq!(bytes, vec![Some(40_000), None, None, None]);
        // Too small for any member.
        let mut allowances = fresh()?;
        frame_shares(&mut allowances, 4095, member_minimum_bytes("q"));
        assert!(allowances.iter().all(Option::is_none));
        Ok(())
    }

    /// A member Answer /3 reply binds both stages from its own facts, unknown
    /// model work is read from its decision log, and its outer Focus binding
    /// is never reclaimed as body bytes.
    #[test]
    fn answer_reply_binds_both_stages_and_unknown_work_holds_the_allowance() -> Result<()> {
        use airk_core::arbiter::Stage;
        let fixtures: Value = serde_json::from_slice(include_bytes!(
            "../../../../fixtures/split/arbiter/answer_loop_cases.json"
        ))?;
        for unknown in [false, true] {
            let mut reply: ContextResponseV3 =
                serde_json::from_value(fixtures["response_cases"][0]["response"].clone())?;
            if unknown {
                reply.decision_log.work_unknown = 1;
            }
            assert_eq!(reply_work_unknown(&reply), unknown);
            let bindings = reply_delivery_bindings(&reply)?;
            assert_eq!(
                bindings
                    .iter()
                    .map(|binding| binding.stage)
                    .collect::<Vec<_>>(),
                vec![Stage::Focus, Stage::Context]
            );
            let body_bytes = serde_json::to_vec(&reply)?.len();
            let outer = airk_kernel::combined_reads::answer_reply_focus_binding_bytes(&reply)?;
            assert!(outer > 0);
            assert_eq!(
                airk_kernel::combined_reads::context_reclaimable_body_bytes(
                    body_bytes + outer,
                    0,
                    body_bytes,
                    outer
                )?,
                0
            );
        }
        Ok(())
    }
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
