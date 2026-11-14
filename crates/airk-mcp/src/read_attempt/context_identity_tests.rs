// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    arbiter::Budget,
    combined_context::initial_allocations,
    identity::{Incarnation, Sha256Digest},
    index_catalog::IndexId,
    read_attempt::{ReadAttemptBudget, ReadClock},
};

fn wire() -> ReadAttemptBudget {
    ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([9; 16]),
        clock: ReadClock::UnixMonotonic,
        expires_monotonic_ns: 3_000_000_000,
        remaining_ns: 2_000_000_000,
        native_retry_expires_monotonic_ns: None,
    }
}

#[test]
fn context_partition_narrowing_keeps_original_identity_cancel_and_single_retry()
-> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let root = ReadAttemptContext::new(deadline, Default::default()).with_wire_attempt(wire())?;
    let allowance = initial_allocations(
        wire().request_id,
        Sha256Digest::from_bytes([8; 32]),
        &[IndexId::from_identity(Incarnation::from_bytes([1; 16]))],
        &Budget::default(),
    )?
    .remove(0)
    .unwrap();
    let child = root
        .clone()
        .with_context_partition(allowance)?
        .narrowed(deadline - Duration::from_secs(1))
        .execution_child();
    assert_eq!(child.request_id(), Some(wire().request_id));
    assert_eq!(
        child.wire_attempt().unwrap().expires_monotonic_ns,
        3_000_000_000
    );
    assert!(child.deadline() < root.deadline());
    assert!(Arc::ptr_eq(&child.retry_spent, &root.retry_spent));
    root.cancellation().cancel();
    assert!(child.cancellation().is_cancelled());
    assert!(child.context_partition().is_some());
    assert!(root.context_partition().is_none());
    Ok(())
}

#[test]
fn context_partition_cannot_rebind_or_replace_original_wire_identity() -> anyhow::Result<()> {
    let root = ReadAttemptContext::new(Instant::now() + Duration::from_secs(2), Default::default())
        .with_wire_attempt(wire())?;
    assert!(
        root.clone()
            .with_request_id(Incarnation::from_bytes([7; 16]))
            .is_err()
    );
    assert!(root.clone().with_wire_attempt(wire()).is_err());
    let mut allocation = initial_allocations(
        wire().request_id,
        Sha256Digest::from_bytes([8; 32]),
        &[IndexId::from_identity(Incarnation::from_bytes([1; 16]))],
        &Budget::default(),
    )?
    .remove(0)
    .unwrap();
    allocation.attempt = Incarnation::from_bytes([7; 16]);
    assert!(root.with_context_partition(allocation).is_err());
    Ok(())
}
