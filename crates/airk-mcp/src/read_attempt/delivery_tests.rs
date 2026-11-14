// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    arbiter::Budget,
    combined_context::{ALLOWANCE_SCHEMA, PartitionAllowance, Resources},
    identity::{Incarnation, Sha256Digest},
    index_catalog::IndexId,
};
use std::sync::atomic::AtomicUsize;
use std::task::Poll;
struct Guard {
    id: Sha256Digest,
    live: Arc<AtomicBool>,
}
impl ReadDeliveryGuard for Guard {
    fn bindings_digest(&self) -> Sha256Digest {
        self.id
    }
    fn check_live(&self) -> std::io::Result<()> {
        if self.live.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(std::io::Error::other("synthetic revoked guard"))
        }
    }
    fn poll(
        &self,
        effect: &mut dyn FnMut() -> Poll<std::io::Result<usize>>,
    ) -> Poll<std::io::Result<usize>> {
        match self.check_live() {
            Ok(()) => effect(),
            Err(error) => Poll::Ready(Err(error)),
        }
    }
}
#[test]
fn one_outer_guard_is_shared_and_cannot_be_replaced_by_another_binding_roster() -> anyhow::Result<()>
{
    let parent =
        ReadAttemptContext::new(Instant::now() + Duration::from_secs(5), Default::default());
    let child = parent.clone();
    let live = Arc::new(AtomicBool::new(true));
    child.retain_delivery_guard(Arc::new(Guard {
        id: Sha256Digest::from_bytes([1; 32]),
        live: live.clone(),
    }))?;
    assert!(
        parent
            .retain_delivery_guard(Arc::new(Guard {
                id: Sha256Digest::from_bytes([2; 32]),
                live: Arc::new(AtomicBool::new(true))
            }))
            .is_err()
    );
    live.store(false, Ordering::Release);
    let mut calls = 0;
    assert!(matches!(
        parent.poll_delivery(&mut || {
            calls += 1;
            Poll::Ready(Ok(1))
        }),
        Poll::Ready(Err(_))
    ));
    assert_eq!(calls, 0);
    Ok(())
}
#[test]
fn private_members_defer_registration_to_one_full_parent_reply() -> anyhow::Result<()> {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let factory: ReadDeliveryGuardFactory = Arc::new(move |_, _| {
        count.fetch_add(1, Ordering::AcqRel);
        Ok(Arc::new(Guard {
            id: Sha256Digest::from_bytes([1; 32]),
            live: Arc::new(AtomicBool::new(true)),
        }))
    });
    let id = Incarnation::from_bytes([3; 16]);
    let parent =
        ReadAttemptContext::new(Instant::now() + Duration::from_secs(5), Default::default())
            .with_request_id(id)?
            .with_delivery_guard_factory(factory);
    let member = parent.clone().with_context_partition(PartitionAllowance {
        schema: ALLOWANCE_SCHEMA.into(),
        attempt: id,
        selection: Sha256Digest::from_bytes([4; 32]),
        index_id: IndexId::from_identity(Incarnation::from_bytes([5; 16])),
        parent_budget: Budget::default(),
        resources: Resources {
            calls: 4,
            input_tokens: 1024,
            candidates: 16,
            bytes: 131072,
        },
        assessment_ordinals: vec![1],
    })?;
    member.retain_delivery_for(&serde_json::json!({"schema":"ordinary"}))?;
    assert_eq!(calls.load(Ordering::Acquire), 0);
    parent.retain_delivery_for(&serde_json::json!({"schema":"ordinary"}))?;
    assert_eq!(calls.load(Ordering::Acquire), 1);
    Ok(())
}
#[test]
fn no_guard_still_honors_original_cancellation_at_actual_poll() {
    let context =
        ReadAttemptContext::new(Instant::now() + Duration::from_secs(5), Default::default());
    context.cancellation().cancel();
    let mut calls = 0;
    assert!(matches!(
        context.poll_delivery(&mut || {
            calls += 1;
            Poll::Ready(Ok(1))
        }),
        Poll::Ready(Err(_))
    ));
    assert_eq!(calls, 0);
}
