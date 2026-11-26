// SPDX-License-Identifier: Apache-2.0
//! Native read budget conversion samples monotonic clocks conservatively.
use airk_core::{
    identity::Incarnation,
    read_attempt::{MAX_ANSWER_READ_BUDGET, ReadAttemptBudget, ReadClock},
    service::{ScopedReadRef, WorkerIdentity},
};
use airk_mcp::read_attempt::{LexicalRepairOrigin, ReadAttemptContext};
use anyhow::Result;
use std::{
    sync::{Arc, Mutex, Weak},
    time::Instant,
};

// Candidate provenance never crosses IPC. Weak entries are usable only while
// the submitting exchange still owns its offer; capacity or lock contention
// simply keeps the ordinary Search path.
const MAX_LEXICAL_REPAIR_OFFERS: usize = 64;
static LEXICAL_REPAIR_OFFERS: Mutex<Vec<Weak<LexicalRepairRegistration>>> = Mutex::new(Vec::new());

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LexicalRepairConsumer {
    pub worker: WorkerIdentity,
    pub scope: ScopedReadRef,
}

struct LexicalRepairRegistration {
    emitted: ReadAttemptBudget,
    consumer: LexicalRepairConsumer,
    deadline: Instant,
    origin: Mutex<Option<LexicalRepairOrigin>>,
    cancellation: tokio_util::sync::CancellationToken,
}

impl LexicalRepairRegistration {
    fn live(&self, now: Instant) -> bool {
        now < self.deadline && !self.cancellation.is_cancelled()
    }
}

#[must_use]
pub struct LexicalRepairOffer(Arc<LexicalRepairRegistration>);

impl Drop for LexicalRepairOffer {
    fn drop(&mut self) {
        self.0.cancellation.cancel();
    }
}

fn same_attempt(left: ReadAttemptBudget, right: ReadAttemptBudget) -> bool {
    left.schema == right.schema
        && left.request_id == right.request_id
        && left.clock == right.clock
        && left.expires_monotonic_ns == right.expires_monotonic_ns
        && left.remaining_ns == right.remaining_ns
        && left.native_retry_expires_monotonic_ns == right.native_retry_expires_monotonic_ns
}

/// Called with the actual emitted frame after selected worker resolution.
pub fn offer_lexical_repair(
    context: &ReadAttemptContext,
    emitted: ReadAttemptBudget,
    consumer: LexicalRepairConsumer,
) -> Option<LexicalRepairOffer> {
    let now = Instant::now();
    if now >= context.deadline()
        || consumer.scope.validate().is_err()
        || consumer.scope.selection.service_instance != consumer.worker.service
    {
        return None;
    }
    let mut offers = LEXICAL_REPAIR_OFFERS.try_lock().ok()?;
    offers.retain(|offer| offer.upgrade().is_some_and(|offer| offer.live(now)));
    if offers.len() >= MAX_LEXICAL_REPAIR_OFFERS {
        return None;
    }
    let origin = context.take_lexical_repair_origin(emitted.request_id, consumer.scope)?;
    let registration = Arc::new(LexicalRepairRegistration {
        emitted,
        consumer,
        deadline: origin.hard_deadline(),
        cancellation: origin.cancellation().clone(),
        origin: Mutex::new(Some(origin)),
    });
    offers.push(Arc::downgrade(&registration));
    Some(LexicalRepairOffer(registration))
}

/// A matching accepted worker may take exactly one original timing receipt.
pub fn claim_lexical_repair(
    emitted: ReadAttemptBudget,
    consumer: LexicalRepairConsumer,
) -> Option<LexicalRepairOrigin> {
    let now = Instant::now();
    let mut offers = LEXICAL_REPAIR_OFFERS.try_lock().ok()?;
    let position = offers.iter().position(|offer| {
        offer.upgrade().is_some_and(|offer| {
            offer.live(now) && offer.consumer == consumer && same_attempt(offer.emitted, emitted)
        })
    })?;
    let offer = offers.swap_remove(position).upgrade()?;
    let origin = offer.origin.try_lock().ok()?.take()?;
    offer.live(now).then_some(origin)
}

fn clock() -> ReadClock {
    #[cfg(unix)]
    {
        ReadClock::UnixMonotonic
    }
    #[cfg(windows)]
    {
        ReadClock::WindowsPerformanceCounter
    }
}

pub fn encode(context: &ReadAttemptContext, request_id: Incarnation) -> Result<ReadAttemptBudget> {
    // Tick first, Instant second: sampling overhead consumes rather than adds time.
    let ticks = airk_process::monotonic::now_ns()?;
    let now = Instant::now();
    encode_sampled(context, request_id, ticks, now)
}

fn encode_sampled(
    context: &ReadAttemptContext,
    request_id: Incarnation,
    ticks: u64,
    now: Instant,
) -> Result<ReadAttemptBudget> {
    anyhow::ensure!(
        !context.cancellation().is_cancelled(),
        "native read canceled"
    );
    let remaining = context
        .deadline()
        .saturating_duration_since(now)
        .min(context.read_ceiling());
    let remaining_ns = u64::try_from(remaining.as_nanos())?;
    anyhow::ensure!(remaining_ns > 0, "native read deadline expired");
    if let Some(captured) = context.captured_transport_attempt() {
        anyhow::ensure!(
            captured.request_id == request_id,
            "captured native read identity mismatch"
        );
        let actual = captured.remaining_at(clock(), ticks)?;
        anyhow::ensure!(!actual.is_zero(), "native read deadline expired");
        return Ok(ReadAttemptBudget {
            remaining_ns: remaining_ns.min(u64::try_from(actual.as_nanos())?),
            ..captured
        });
    }
    let original = context.wire_attempt();
    let expires_monotonic_ns = ticks
        .checked_add(remaining_ns)
        .ok_or_else(|| anyhow::anyhow!("native read deadline overflow"))?;
    let expires_monotonic_ns = if let Some(prior) = original {
        let actual = prior.remaining_at(clock(), ticks)?;
        anyhow::ensure!(
            !actual.is_zero() && prior.request_id == request_id,
            "original native read expired or identity changed"
        );
        expires_monotonic_ns.min(prior.expires_monotonic_ns)
    } else {
        expires_monotonic_ns
    };
    let native_retry_expires_monotonic_ns = context
        .native_retry_deadline()
        .map(|deadline| -> Result<_> {
            let allowance = u64::try_from(
                deadline
                    .saturating_duration_since(now)
                    .min(remaining)
                    .as_nanos(),
            )?;
            ticks
                .checked_add(allowance)
                .ok_or_else(|| anyhow::anyhow!("native settings deadline overflow"))
        })
        .transpose()?;
    Ok(ReadAttemptBudget {
        schema: 1,
        request_id,
        clock: clock(),
        expires_monotonic_ns,
        remaining_ns: remaining_ns.min(expires_monotonic_ns.saturating_sub(ticks)),
        native_retry_expires_monotonic_ns: native_retry_expires_monotonic_ns
            .map(|expiry| expiry.min(expires_monotonic_ns)),
    })
}

/// Decode a wire attempt whose tool is known at this entry point: refuses an
/// attempt longer than that tool's ceiling (`read_budget_for_tool`).
pub fn decode_for_tool(budget: ReadAttemptBudget, tool: &str) -> Result<ReadAttemptContext> {
    budget.remaining_for_tool(tool, clock(), airk_process::monotonic::now_ns()?)?;
    decode(budget)
}

/// Decode a wire attempt. The attempt may be an Answer's (up to its 605 s);
/// the context never outlives the wire expiry, whatever its ceiling.
pub fn decode(budget: ReadAttemptBudget) -> Result<ReadAttemptContext> {
    // Instant first, tick second: IPC/decoding and sampling overhead are charged.
    let now = Instant::now();
    let ticks = airk_process::monotonic::now_ns()?;
    decode_sampled(budget, now, ticks)
}

fn decode_sampled(
    budget: ReadAttemptBudget,
    now: Instant,
    ticks: u64,
) -> Result<ReadAttemptContext> {
    let remaining = budget.remaining_at(clock(), ticks)?;
    let mut context = ReadAttemptContext::new(now + remaining, Default::default())
        .with_read_ceiling(MAX_ANSWER_READ_BUDGET)
        .with_wire_attempt(budget)?
        .remote_admission();
    if let Some(deadline) = budget.native_retry_expires_monotonic_ns {
        let allowance = std::time::Duration::from_nanos(deadline.saturating_sub(ticks))
            .min(remaining)
            .min(std::time::Duration::from_secs(5));
        context = context.with_native_retry_deadline(now + allowance);
    }
    Ok(context)
}

pub fn supported(version: &str) -> bool {
    let parts = version
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>();
    parts.is_ok_and(|parts| parts.len() == 3 && parts.as_slice() >= [1, 8, 14].as_slice())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod lexical_repair_tests {
    use super::*;
    use airk_core::{
        identity::Sha256Digest,
        index_catalog::{CatalogRevision, IndexId},
        scoped_reads::{SelectionOwnerKind, SelectionScopeRef},
    };

    fn consumer() -> LexicalRepairConsumer {
        let service = Incarnation::from_bytes([1; 16]);
        LexicalRepairConsumer {
            worker: WorkerIdentity {
                service,
                worker: Incarnation::from_bytes([2; 16]),
                project: Sha256Digest::from_bytes([3; 32]),
                fencing_generation: 1,
            },
            scope: ScopedReadRef {
                schema: 1,
                lease_generation: 1,
                index_id: IndexId::from_identity(Incarnation::from_bytes([4; 16])),
                selection: SelectionScopeRef {
                    owner: SelectionOwnerKind::Window,
                    grant_nonce: Incarnation::from_bytes([5; 16]),
                    service_instance: service,
                    scope_identity: Sha256Digest::from_bytes([6; 32]),
                    catalog_revision: CatalogRevision::INITIAL,
                },
                fingerprint: Sha256Digest::from_bytes([7; 32]),
            },
        }
    }

    #[test]
    fn private_lexical_repair_claim_matches_exact_wire_and_consumer_once() -> Result<()> {
        let consumer = consumer();
        let request = Incarnation::from_bytes([8; 16]);
        let context =
            ReadAttemptContext::for_standalone_tools_call("airk_search", Default::default())
                .bind_standalone_lexical_repair(
                    request,
                    consumer.scope.selection,
                    consumer.scope.index_id,
                )?;
        let wire = encode(&context, request)?;
        assert!(claim_lexical_repair(wire, consumer).is_none());
        let mut substituted = consumer;
        substituted.scope.selection.grant_nonce = Incarnation::from_bytes([9; 16]);
        assert!(offer_lexical_repair(&context, wire, substituted).is_none());
        let offer = offer_lexical_repair(&context, wire, consumer).expect("standalone origin");
        assert!(offer_lexical_repair(&context, wire, consumer).is_none());
        let changed = ReadAttemptBudget {
            remaining_ns: wire.remaining_ns - 1,
            ..wire
        };
        assert!(claim_lexical_repair(changed, consumer).is_none());
        assert!(claim_lexical_repair(wire, substituted).is_none());
        substituted = consumer;
        substituted.worker.fencing_generation += 1;
        assert!(claim_lexical_repair(wire, substituted).is_none());
        let origin = claim_lexical_repair(wire, consumer).expect("exact same-process claim");
        assert_eq!(origin.hard_deadline(), context.deadline());
        assert!(claim_lexical_repair(wire, consumer).is_none());

        // Dropping the exchange cancels its repair only. Normal caller
        // cancellation continues to belong to the original read context.
        drop(offer);
        assert!(origin.cancellation().is_cancelled());
        assert!(!context.cancellation().is_cancelled());

        let carried = decode_for_tool(wire, "airk_search")?;
        assert!(!carried.can_bind_standalone_lexical_repair());
        assert!(offer_lexical_repair(&carried, wire, consumer).is_none());
        let other = Incarnation::from_bytes([10; 16]);
        let canceled =
            ReadAttemptContext::for_standalone_tools_call("airk_search", Default::default())
                .bind_standalone_lexical_repair(
                    other,
                    consumer.scope.selection,
                    consumer.scope.index_id,
                )?;
        let canceled_wire = encode(&canceled, other)?;
        let _canceled_offer = offer_lexical_repair(&canceled, canceled_wire, consumer).unwrap();
        canceled.cancellation().cancel();
        assert!(claim_lexical_repair(canceled_wire, consumer).is_none());
        Ok(())
    }
}
