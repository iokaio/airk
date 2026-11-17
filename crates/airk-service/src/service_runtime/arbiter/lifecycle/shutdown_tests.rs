// SPDX-License-Identifier: Apache-2.0
//! Actual lease and event gates for service shutdown, without model work.
use super::*;
use airk_store::service::ServiceLease;

#[tokio::test(flavor = "current_thread")]
async fn shutdown_cannot_finish_before_the_owned_hash_returns_and_releases_its_actual_lease()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let lease = Arc::new(ServiceLease::acquire(root.path())?);
    let shared = Arc::new(Mutex::new(State::default()));
    let (serial, cancel) = start(&shared, Sha256Digest::from_bytes([3; 32]), None)?;
    let (entered, observing) = tokio::sync::oneshot::channel();
    let (release, waiting) = std::sync::mpsc::channel();
    let finishing = shared.clone();
    let work = tokio::spawn(async move {
        let result = super::super::assets::owned_asset_operation(async move {
            let _actual_lease = lease;
            entered
                .send(())
                .map_err(|_| anyhow::anyhow!("hash observer closed"))?;
            waiting.recv_timeout(Duration::from_secs(5))?;
            ensure!(!cancel.is_cancelled(), "actual original operation canceled");
            Ok(())
        })
        .await;
        complete(&finishing, serial, &result);
        result
    });
    tokio::time::timeout(Duration::from_secs(5), observing).await??;
    begin_shutdown(&shared, Stage::Context)?;
    let stopping = shared.clone();
    let shutdown = tokio::spawn(async move {
        wait_for_settlement(&stopping, Instant::now() + Duration::from_secs(30)).await
    });
    let control = tokio::spawn(async { true });
    assert!(tokio::time::timeout(Duration::from_secs(5), control).await??);
    assert!(
        !shutdown.is_finished(),
        "a canceled hash is still actual owned work"
    );
    assert!(ServiceLease::acquire(root.path()).is_err());
    release.send(())?;
    assert!(
        tokio::time::timeout(Duration::from_secs(5), work)
            .await??
            .is_err()
    );
    tokio::time::timeout(Duration::from_secs(5), shutdown).await???;
    let _reacquired = ServiceLease::acquire(root.path())?;
    Ok(())
}

#[tokio::test]
async fn shutdown_observes_settlement_before_or_after_wait_registration() -> Result<()> {
    for finish_first in [false, true] {
        let shared = Arc::new(Mutex::new(State::default()));
        let (serial, _) = start(&shared, Sha256Digest::from_bytes([3; 32]), None)?;
        begin_shutdown(&shared, Stage::Context)?;
        if finish_first {
            complete(&shared, serial, &Ok(()));
        }
        let observed = wait_for_settlement(&shared, Instant::now() + Duration::from_secs(30));
        tokio::pin!(observed);
        if !finish_first {
            use std::future::Future;
            let pending = std::future::poll_fn(|cx| {
                std::task::Poll::Ready(observed.as_mut().poll(cx).is_pending())
            })
            .await;
            assert!(pending);
            complete(&shared, serial, &Ok(()));
        }
        tokio::time::timeout(Duration::from_secs(5), observed).await??;
    }
    Ok(())
}

#[tokio::test]
async fn shutdown_refuses_unknown_native_drain_instead_of_claiming_service_completion() -> Result<()>
{
    let shared = Arc::new(Mutex::new(State::default()));
    shared.lock().unwrap().phase = Availability::Draining;
    let shutdown = begin_shutdown(&shared, Stage::Context)?;
    let error = finish_shutdown(&shared, shutdown, Instant::now() + Duration::from_secs(30))
        .await
        .expect_err("unknown native drain cannot finish service lifecycle");
    assert!(error.to_string().contains("no retained observation proof"));
    Ok(())
}

#[test]
fn shutdown_fence_clears_pending_and_refuses_late_continuation_or_restoration() -> Result<()> {
    let shared = Arc::new(Mutex::new(State::default()));
    let profile = Sha256Digest::from_bytes([3; 32]);
    let (serial, original_cancel) = start(&shared, profile, None)?;
    shared.lock().unwrap().pending = Some(super::tests::pending_fixture(2, 4)?);
    begin_shutdown(&shared, Stage::Context)?;
    assert!(original_cancel.is_cancelled());
    assert!(shared.lock().unwrap().pending.is_none());
    assert!(start(&shared, profile, None).is_err());
    // A continuation prepared before the fence cannot be admitted afterward.
    shared.lock().unwrap().pending = Some(super::tests::pending_fixture(2, 4)?);
    assert!(next_pending(&shared, serial)?.is_none());
    let mut state = shared.lock().unwrap();
    assert!(state.pending.is_none());
    assert!(!state.busy);
    assert_eq!(state.serial, serial);
    assert!(restore_cancellation(&mut state, serial, 0, profile).is_err());
    Ok(())
}

fn actual_reservation(
    resources: &airk_kernel::service_resources::Resources,
    id: u8,
) -> Result<Arc<airk_kernel::service_resources::OptionalReservation>> {
    Ok(Arc::new(resources.reserve_optional(
        airk_core::identity::Incarnation::from_bytes([id; 16]),
        1 << 20,
        Some(64 << 30),
    )?))
}

#[tokio::test]
async fn idle_ready_shutdown_waits_for_the_last_actual_native_reservation_owner() -> Result<()> {
    use std::future::Future;
    let resources = airk_kernel::service_resources::Resources::new(
        airk_core::service_resources::ResourceConfiguration::default(),
        32,
        Some(128 << 30),
    )?;
    let before = resources.snapshot().memory_reserved_bytes;
    let resident = actual_reservation(&resources, 91)?;
    let actual_forward = resident.clone();
    let shared = Arc::new(Mutex::new(State::default()));
    remember_charge(
        &shared,
        Sha256Digest::from_bytes([3; 32]),
        resident.release_observer(),
    )
    .await?;
    shared.lock().unwrap().phase = Availability::Ready;
    assert!(!shared.lock().unwrap().busy);
    let shutdown = begin_shutdown(&shared, Stage::Context)?;
    let complete = finish_shutdown(&shared, shutdown, Instant::now() + Duration::from_secs(30));
    tokio::pin!(complete);
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(complete.as_mut().poll(cx).is_pending()))
            .await
    );
    drop(resident);
    assert_eq!(
        resources.snapshot().memory_reserved_bytes,
        before + (1 << 20)
    );
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(complete.as_mut().poll(cx).is_pending()))
            .await
    );
    drop(actual_forward);
    complete.await?;
    assert_eq!(resources.snapshot().memory_reserved_bytes, before);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn both_stages_share_one_original_shutdown_cutoff_without_renewal() -> Result<()> {
    use std::future::Future;
    let resources = airk_kernel::service_resources::Resources::new(
        airk_core::service_resources::ResourceConfiguration::default(),
        32,
        Some(128 << 30),
    )?;
    let focus_charge = actual_reservation(&resources, 92)?;
    let context_charge = actual_reservation(&resources, 93)?;
    let focus = Arc::new(Mutex::new(State::default()));
    let context = Arc::new(Mutex::new(State::default()));
    remember_charge(
        &focus,
        Sha256Digest::from_bytes([3; 32]),
        focus_charge.release_observer(),
    )
    .await?;
    remember_charge(
        &context,
        Sha256Digest::from_bytes([4; 32]),
        context_charge.release_observer(),
    )
    .await?;
    focus.lock().unwrap().phase = Availability::Ready;
    context.lock().unwrap().phase = Availability::Ready;
    let deadline = Instant::now() + Duration::from_secs(30);
    let focus_shutdown = begin_shutdown(&focus, Stage::Focus)?;
    let context_shutdown = begin_shutdown(&context, Stage::Context)?;
    let focus_done = finish_shutdown(&focus, focus_shutdown, deadline);
    let context_done = finish_shutdown(&context, context_shutdown, deadline);
    tokio::pin!(focus_done, context_done);
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(
            focus_done.as_mut().poll(cx).is_pending()
        ))
        .await
    );
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(
            context_done.as_mut().poll(cx).is_pending()
        ))
        .await
    );
    tokio::time::advance(Duration::from_secs(20)).await;
    drop(focus_charge);
    focus_done.await?;
    tokio::time::advance(Duration::from_secs(10)).await;
    assert!(
        context_done.await.is_err(),
        "Context cannot obtain thirty new seconds after Focus"
    );
    assert!(
        resources.snapshot().memory_reserved_bytes >= 1 << 20,
        "timeout cannot release a live charge"
    );
    drop(context_charge);
    Ok(())
}

#[test]
fn shutdown_targets_include_actual_retained_and_warming_profiles() {
    let latest = Sha256Digest::from_bytes([3; 32]);
    let actual_retained = Sha256Digest::from_bytes([1; 32]);
    let warming = Sha256Digest::from_bytes([2; 32]);
    assert_eq!(
        shutdown_targets(Some(latest), Some(actual_retained), Some(warming)),
        vec![actual_retained, warming, latest]
    );
    assert_eq!(
        shutdown_targets(Some(latest), Some(actual_retained), Some(latest)),
        vec![actual_retained, latest]
    );
}

#[tokio::test]
async fn charge_observers_are_pruned_only_after_actual_release() -> Result<()> {
    let resources = airk_kernel::service_resources::Resources::new(
        airk_core::service_resources::ResourceConfiguration::default(),
        32,
        Some(128 << 30),
    )?;
    let shared = Arc::new(Mutex::new(State::default()));
    let first = actual_reservation(&resources, 94)?;
    let second = actual_reservation(&resources, 95)?;
    remember_charge(
        &shared,
        Sha256Digest::from_bytes([1; 32]),
        first.release_observer(),
    )
    .await?;
    remember_charge(
        &shared,
        Sha256Digest::from_bytes([2; 32]),
        second.release_observer(),
    )
    .await?;
    assert_eq!(shared.lock().unwrap().charges.len(), 2);
    drop(first);
    let third = actual_reservation(&resources, 96)?;
    remember_charge(
        &shared,
        Sha256Digest::from_bytes([3; 32]),
        third.release_observer(),
    )
    .await?;
    let kept = shared
        .lock()
        .unwrap()
        .charges
        .iter()
        .map(|(manifest, _)| *manifest)
        .collect::<Vec<_>>();
    assert_eq!(
        kept,
        vec![
            Sha256Digest::from_bytes([2; 32]),
            Sha256Digest::from_bytes([3; 32])
        ]
    );
    Ok(())
}

#[tokio::test]
async fn authentic_charge_roster_refuses_capacity_without_evicting_owned_observations() -> Result<()>
{
    let resources = airk_kernel::service_resources::Resources::new(
        airk_core::service_resources::ResourceConfiguration::default(),
        32,
        Some(128 << 30),
    )?;
    let shared = Arc::new(Mutex::new(State::default()));
    let mut actual = Vec::new();
    for id in 1..=16 {
        let reservation = actual_reservation(&resources, id)?;
        remember_charge(
            &shared,
            Sha256Digest::from_bytes([id; 32]),
            reservation.release_observer(),
        )
        .await?;
        actual.push(reservation);
    }
    assert_eq!(
        airk_kernel::service_resources::MAX_OPTIONAL_RESERVATIONS,
        16
    );
    let before = shared
        .lock()
        .unwrap()
        .charges
        .iter()
        .map(|(manifest, _)| *manifest)
        .collect::<Vec<_>>();
    assert_eq!(before.len(), 16);
    // A distinct authentic seventeenth charge cannot displace an unfinished
    // owner. A repeated observer of an existing charge is intentionally deduped.
    let independent_resources = airk_kernel::service_resources::Resources::new(
        airk_core::service_resources::ResourceConfiguration::default(),
        32,
        Some(128 << 30),
    )?;
    let seventeenth = actual_reservation(&independent_resources, 99)?;
    assert!(
        remember_charge(
            &shared,
            Sha256Digest::from_bytes([99; 32]),
            seventeenth.release_observer()
        )
        .await
        .is_err()
    );
    let after = shared
        .lock()
        .unwrap()
        .charges
        .iter()
        .map(|(manifest, _)| *manifest)
        .collect::<Vec<_>>();
    assert_eq!(after, before);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn shutdown_waits_for_bounded_device_continuation_owner_not_only_active_native_work()
-> Result<()> {
    use std::{
        future::Future,
        task::{Context as TaskContext, Poll, Waker},
    };
    let shared = Arc::new(Mutex::new(State::default()));
    {
        let mut state = shared.lock().unwrap();
        state.device_tasks = 1;
    }
    begin_shutdown(&shared, Stage::Context)?;
    let wait = wait_for_settlement(&shared, Instant::now() + Duration::from_secs(5));
    tokio::pin!(wait);
    assert!(matches!(
        wait.as_mut()
            .poll(&mut TaskContext::from_waker(Waker::noop())),
        Poll::Pending
    ));
    {
        let mut state = shared.lock().unwrap();
        assert!(state.stopping);
        state.device_tasks = 0;
        state.settled.notify_waiters();
    }
    assert!(matches!(
        wait.as_mut()
            .poll(&mut TaskContext::from_waker(Waker::noop())),
        Poll::Ready(Ok(()))
    ));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn repeated_borrowed_probe_observation_deduplicates_actual_charge_and_preserves_distinct_unresolved_owners()
-> Result<()> {
    use std::{
        future::Future,
        task::{Context as TaskContext, Poll, Waker},
    };
    let shared = Arc::new(Mutex::new(State::default()));
    let resources = airk_kernel::service_resources::Resources::new(
        airk_core::service_resources::ResourceConfiguration::default(),
        8,
        Some(64 << 30),
    )?;
    let manifest = Sha256Digest::from_bytes([73; 32]);
    let first = Arc::new(resources.reserve_optional(
        airk_core::identity::Incarnation::from_bytes([71; 16]),
        1,
        Some(8 << 30),
    )?);
    let release = first.release_observer();
    for _ in 0..64 {
        remember_charge(&shared, manifest, release.clone()).await?;
    }
    assert_eq!(shared.lock().unwrap().charges.len(), 1);
    let other = resources.reserve_optional(
        airk_core::identity::Incarnation::from_bytes([72; 16]),
        1,
        Some(8 << 30),
    )?;
    assert!(!release.same_charge(&other.release_observer()));
    remember_charge(&shared, manifest, other.release_observer()).await?;
    assert_eq!(
        shared.lock().unwrap().charges.len(),
        2,
        "same manifest never collapses distinct unresolved charges"
    );
    let actual_owner = first.clone();
    drop(first);
    let mut waiting = Box::pin(release.wait());
    assert!(matches!(
        waiting
            .as_mut()
            .poll(&mut TaskContext::from_waker(Waker::noop())),
        Poll::Pending
    ));
    for _ in 0..64 {
        remember_charge(&shared, manifest, other.release_observer()).await?;
    }
    assert_eq!(shared.lock().unwrap().charges.len(), 2);
    drop(actual_owner);
    assert!(matches!(
        waiting
            .as_mut()
            .poll(&mut TaskContext::from_waker(Waker::noop())),
        Poll::Ready(Ok(()))
    ));
    remember_charge(&shared, manifest, other.release_observer()).await?;
    assert_eq!(
        shared.lock().unwrap().charges.len(),
        1,
        "only actual released charge is pruned"
    );
    drop(other);
    Ok(())
}
