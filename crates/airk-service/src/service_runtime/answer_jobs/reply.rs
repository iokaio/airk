// SPDX-License-Identifier: Apache-2.0
//! The Answer /3 replies the job table produces itself, and the adapter
//! from a table reply to what the service's Answer path delivers.
//!
//! A queued job and a running one have no gathered work to show, and the
//! Core reply contract allows a reply without parts only in the ticket shape
//! (partial at `admission` with reason `queued`, no continuation in the
//! marker, the ticket's id as the handle). So both use [`pending_reply`]; a
//! running job's ticket has position 1 and start 0. `PartialMarker::
//! continuation` stays for a reply that carries gathered work.
use super::{
    CallReply, Finished, JobOutcome,
    schedule::{Timing, publication_digest},
};
use airk_core::arbiter::{
    ScopeBinding, Work,
    answer_loop::{
        AnswerDeliveryRecord, AnswerTicket, CONTEXT_SCHEMA_V3, ContextRequestV3, ContextResponseV3, DecisionLogSummary,
        Degradation, LoopModelWork, LoopStage, LoopTimings, PACKET_SCHEMA_V2, PartStop,
        PartialMarker, PartialReason, UnionSummary, empty_log_digest,
    },
};
use airk_core::identity::Sha256Digest;
use anyhow::{Result, ensure};

/// The reply for a job the caller must come back for: no parts, records or
/// work; `degradations` are those already fitted at dispatch, if any. The
/// reply is validated against the request's budget before it is returned.
pub(crate) fn pending_reply(
    request: &ContextRequestV3,
    scope: ScopeBinding,
    ticket: AnswerTicket,
    degradations: Vec<Degradation>,
    elapsed_ms: u64,
) -> Result<ContextResponseV3> {
    let reply = ContextResponseV3 {
        schema: CONTEXT_SCHEMA_V3.into(),
        packet_schema: PACKET_SCHEMA_V2.into(),
        question: request.question.clone(),
        scope,
        parts: vec![],
        records: vec![],
        omitted_evidence: vec![],
        record_uses: vec![],
        paragraphs: vec![],
        union: UnionSummary::default(),
        partial: Some(PartialMarker {
            stage: LoopStage::Admission,
            reason: PartialReason::Queued,
            part: None,
            continuation: None,
        }),
        ticket: Some(ticket),
        degradations,
        decisions: vec![],
        decision_log: DecisionLogSummary {
            digest: empty_log_digest(),
            total: 0,
            included: 0,
            omitted: 0,
            card_calls: 0,
            counts: vec![],
            focus_calls: 0,
            focus_pairs: 0,
            attribution: vec![],
            timings: LoopTimings::default(),
            work_unknown: 0,
        },
        work: Work::default(),
        model_work: LoopModelWork::default(),
        elapsed_ms,
        candidate_count: 0,
        omitted_count: 0,
        applications: vec![],
    };
    reply.validate(&request.budget)?;
    Ok(reply)
}

/// A finished kernel reply as the table retains it: the publication it ran
/// on comes from the reply's own scope, and its retained bytes are its
/// serialized size. The reply itself says whether its deadline cut it: the
/// partial marker or any part's stop names the deadline. The loop keeps a
/// reserve, so such a reply arrives well before its window ends.
pub(crate) fn finished(response: ContextResponseV3) -> Result<Finished<ContextResponseV3>> {
    let publication = publication_digest(&response.scope.publication)?;
    let retained_bytes = serde_json::to_vec(&response)?.len();
    let cut = response
        .partial
        .as_ref()
        .is_some_and(|marker| marker.reason == PartialReason::Deadline)
        || response
            .parts
            .iter()
            .any(|part| part.stop == PartStop::Deadline);
    Ok(Finished {
        value: response,
        publication,
        retained_bytes,
        cut,
    })
}

/// What the service's Answer path delivers for one call.
#[derive(Debug)]
pub(crate) enum Delivered {
    /// A reply rendered for this call: the answer or a ticket-shaped reply,
    /// with this delivery's record.
    Reply {
        response: Box<ContextResponseV3>,
        record: AnswerDeliveryRecord,
    },
    /// The job failed; the failure belongs to the calls it is delivered to.
    Failed(String),
    /// The job did nothing a retry could repeat and is no longer in the
    /// table; `not_run` when it never started.
    Dropped { message: String, not_run: bool },
    /// Nothing was queued; retry after this long.
    Busy { retry_after_ms: u64 },
    /// The invocation key names another request.
    KeyConflict,
}

/// Turn a table reply into what is delivered to one call (CONTRACT "Each
/// delivery is rendered for its caller"). `scope` is the caller's own
/// verified binding: a ticket reply names it, and a finished reply must have
/// run on the same installation, catalog, index and authority. The reply's
/// `elapsed_ms` becomes this call's own wait, and it is validated against
/// this call's own budget before it leaves.
pub(crate) fn deliver(
    reply: CallReply<ContextResponseV3>,
    request: &ContextRequestV3,
    scope: &ScopeBinding,
    waited_ms: u64,
) -> Result<Delivered> {
    let record = |id: Sha256Digest, timing: Timing, reused: bool| AnswerDeliveryRecord {
        job: id,
        queue_ms: timing.queue_ms,
        run_ms: timing.run_ms,
        wait_ms: waited_ms,
        reused,
    };
    Ok(match reply {
        CallReply::Finished {
            id,
            attached,
            outcome,
            timing,
            ..
        } => match outcome.as_ref() {
            JobOutcome::Done { value, .. } => {
                ensure!(
                    value.scope.installation == scope.installation
                        && value.scope.catalog == scope.catalog
                        && value.scope.index_id == scope.index_id
                        && value.scope.authorization == scope.authorization,
                    "Answer job reply names another scope"
                );
                let mut response = value.clone();
                response.elapsed_ms = waited_ms;
                response.validate(&request.budget)?;
                let record = record(id, timing, attached);
                record.validate()?;
                Delivered::Reply {
                    response: Box::new(response),
                    record,
                }
            }
            JobOutcome::Failed(message) => Delivered::Failed(message.clone()),
            JobOutcome::Dropped { message, not_run } => Delivered::Dropped {
                message: message.clone(),
                not_run: *not_run,
            },
        },
        CallReply::Pending {
            ticket,
            degradations,
            timing,
            attached,
            ..
        } => {
            let id = ticket.id;
            let record = record(id, timing, attached);
            record.validate()?;
            Delivered::Reply {
                response: Box::new(pending_reply(
                    request,
                    scope.clone(),
                    ticket,
                    degradations,
                    waited_ms,
                )?),
                record,
            }
        }
        CallReply::Busy { retry_after_ms } => Delivered::Busy { retry_after_ms },
        CallReply::KeyConflict => Delivered::KeyConflict,
    })
}