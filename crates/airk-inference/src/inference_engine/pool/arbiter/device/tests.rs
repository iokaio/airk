// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    future::Future,
    task::{Context as TaskContext, Poll, Waker},
};

#[test]
fn native_warm_checks_clock_after_authority_and_never_dispatches_revoked_or_late_work() -> Result<()>
{
    let start = Instant::now();
    let deadline = start + Duration::from_secs(120);
    let observed = Arc::new(Mutex::new(start));
    let time = observed.clone();
    let check = move || {
        *time.lock().unwrap() = deadline;
        true
    };
    assert!(warm_admission(Some(deadline), Some(&check), || *observed.lock().unwrap()).is_err());
    assert!(warm_admission(Some(deadline), Some(&|| false), || start).is_err());
    warm_admission(Some(deadline), Some(&|| true), || start)?;
    Ok(())
}
fn shared() -> Arc<Shared> {
    Arc::new(Shared {
        queue: Mutex::new(Queue::default()),
        wake: Condvar::new(),
        accounting: Arc::new(accounting::Accounting::default()),
        closing: Arc::new(AtomicBool::new(false)),
        service: Incarnation::from_bytes([91; 16]),
        done: AtomicBool::new(false),
        unsettled: AtomicBool::new(false),
        running: AtomicUsize::new(1),
        changed: tokio::sync::Notify::new(),
    })
}
struct SyntheticSession {
    events: Arc<Mutex<Vec<&'static str>>>,
    settled: bool,
    late: bool,
    deadline: Instant,
}
impl NativeSettlement for SyntheticSession {
    fn settle(&self, deadline: Instant) -> Result<()> {
        assert_eq!(deadline, self.deadline);
        self.events.lock().unwrap().push("sync");
        if self.late || !self.settled {
            anyhow::bail!("injected native wait result")
        }
        Ok(())
    }
    fn is_settled(&self) -> bool {
        self.settled
    }
}
impl Drop for SyntheticSession {
    fn drop(&mut self) {
        self.events.lock().unwrap().push("session");
    }
}
struct SyntheticPins(Arc<Mutex<Vec<&'static str>>>);
impl AssetSource for SyntheticPins {
    fn read_asset(&self, _: &airk_core::arbiter::Asset) -> Result<Vec<u8>> {
        anyhow::bail!("synthetic pin is never a model source")
    }
}
impl Drop for SyntheticPins {
    fn drop(&mut self) {
        self.0.lock().unwrap().push("pins");
    }
}
struct SyntheticOwner(Arc<Mutex<Vec<&'static str>>>);
impl Drop for SyntheticOwner {
    fn drop(&mut self) {
        self.0.lock().unwrap().push("owner");
    }
}
fn charge() -> Result<Arc<Reservation>> {
    let resources = airk_kernel::service_resources::Resources::new(
        airk_core::service_resources::ResourceConfiguration::default(),
        8,
        Some(64 << 30),
    )?;
    Ok(Arc::new(resources.reserve_optional(
        Incarnation::from_bytes([92; 16]),
        128 << 20,
        Some(8 << 30),
    )?))
}
#[test]
fn actual_guard_drops_session_then_pins_then_host_charge_only_after_known_settlement() -> Result<()>
{
    for late in [false, true] {
        let events = Arc::new(Mutex::new(Vec::new()));
        let shared = shared();
        let charge = charge()?;
        let release = charge.release_observer();
        let deadline = Instant::now();
        let resident = GuardedResident {
            native_device: None,
            session: std::mem::ManuallyDrop::new(ResidentSession {
                scorer: SyntheticSession {
                    events: events.clone(),
                    settled: true,
                    late,
                    deadline,
                },
                _source: Box::new(SyntheticPins(events.clone())),
            }),
            charge: std::mem::ManuallyDrop::new(charge),
            owner: Arc::new(SyntheticOwner(events.clone())),
            shared: Arc::downgrade(&shared),
            deadline: std::cell::Cell::new(Some(deadline)),
        };
        drop(resident);
        assert_eq!(
            *events.lock().unwrap(),
            ["sync", "session", "pins", "owner"]
        );
        assert!(!shared.unsettled.load(Ordering::Acquire));
        let mut wait = Box::pin(release.wait());
        assert!(matches!(
            wait.as_mut()
                .poll(&mut TaskContext::from_waker(Waker::noop())),
            Poll::Ready(Ok(()))
        ));
    }
    Ok(())
}
#[test]
fn failed_native_wait_retains_pins_host_charge_owner_and_incomplete_lane_barrier() -> Result<()> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let shared = shared();
    let charge = charge()?;
    let release = charge.release_observer();
    let deadline = Instant::now();
    let resident = GuardedResident {
        native_device: None,
        session: std::mem::ManuallyDrop::new(ResidentSession {
            scorer: SyntheticSession {
                events: events.clone(),
                settled: false,
                late: false,
                deadline,
            },
            _source: Box::new(SyntheticPins(events.clone())),
        }),
        charge: std::mem::ManuallyDrop::new(charge),
        owner: Arc::new(SyntheticOwner(events.clone())),
        shared: Arc::downgrade(&shared),
        deadline: std::cell::Cell::new(Some(deadline)),
    };
    drop(resident);
    assert_eq!(*events.lock().unwrap(), ["sync"]);
    assert!(shared.unsettled.load(Ordering::Acquire));
    let mut wait = Box::pin(release.wait());
    assert!(matches!(
        wait.as_mut()
            .poll(&mut TaskContext::from_waker(Waker::noop())),
        Poll::Pending
    ));
    drop(LaneCompleted(shared.clone()));
    assert_eq!(shared.running.load(Ordering::Acquire), 1);
    assert_eq!(*events.lock().unwrap(), ["sync"]);
    Ok(())
}
#[test]
fn actual_optional_lane_keeps_configuration4_typed_and_refuses_same_revision_downgrade()
-> Result<()> {
    use airk_core::arbiter::device::*;
    let shared = shared();
    let handle = Handle(shared.clone());
    let install = Sha256Digest::from_bytes([1; 32]);
    let catalog = shared.service;
    let doc = DeviceConfiguration::disabled(
        install,
        catalog,
        DeviceCatalogBinding {
            catalog: Sha256Digest::from_bytes([2; 32]),
            source: Sha256Digest::from_bytes([3; 32]),
        },
    );
    handle.configure_native(NativeConfiguration::device(doc.clone(), vec![])?)?;
    assert!(
        handle
            .configure(Configuration::disabled(install, catalog))
            .is_err()
    );
    let mut changed = doc.clone();
    changed.device_catalog.source = Sha256Digest::from_bytes([4; 32]);
    assert!(
        handle
            .configure_native(NativeConfiguration::device(changed, vec![])?)
            .is_err()
    );
    let mut next = doc;
    next.revision = 2;
    handle.configure_native(NativeConfiguration::device(next, vec![])?)?;
    assert!(matches!(
        shared.queue.lock().unwrap().configuration,
        Some(NativeConfiguration::Device { .. })
    ));
    Ok(())
}

#[test]
fn genuine_device_fault_is_required_and_deadline_source_or_cancellation_alone_never_recovers() {
    assert!(fault_reporting_allowed(true, true, true, true, true));
    assert!(!fault_reporting_allowed(false, true, true, true, true));
    assert!(!fault_reporting_allowed(true, false, true, true, true));
    assert!(!fault_reporting_allowed(true, true, false, true, true));
    assert!(!fault_reporting_allowed(true, true, true, false, true));
    assert!(!fault_reporting_allowed(true, true, true, true, false));
}

#[test]
fn unqualified_evaluation_bounds_cannot_mint_qualification_or_exceed_recorded_host_and_device_caps()
-> Result<()> {
    let bounds = airk_core::arbiter::device::GpuProbeReservation {
        schema: "airk.arbiter.gpu-probe-reservation/1".into(),
        host_limit_bytes: 16 << 30,
        device_limit_bytes: 20 << 30,
        deadline_ms: 120000,
    };
    validate_probe_bounds(&bounds, 16 << 30, 12 << 30, 18 << 30, 2 << 30)?;
    assert!(validate_probe_bounds(&bounds, 15 << 30, 12 << 30, 18 << 30, 2 << 30).is_err());
    assert!(validate_probe_bounds(&bounds, 16 << 30, 17 << 30, 18 << 30, 2 << 30).is_err());
    assert!(validate_probe_bounds(&bounds, 16 << 30, 12 << 30, 19 << 30, 2 << 30).is_err());
    assert!(validate_probe_bounds(&bounds, 16 << 30, 12 << 30, u64::MAX, 1).is_err());
    let mut expired = bounds;
    expired.deadline_ms = 120001;
    assert!(validate_probe_bounds(&expired, 16 << 30, 12 << 30, 18 << 30, 2 << 30).is_err());
    Ok(())
}

#[test]
fn queued_runtime_probe_holds_actual_lease_and_host_charge_after_delivery_cancel_until_queue_owner_releases()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(root.path())?);
    let shared = shared();
    let handle = Handle(shared.clone());
    let host = charge()?;
    let released = host.release_observer();
    let receive = handle.probe_devices(
        lease.clone(),
        airk_core::arbiter::device::SelectedModel::Focus,
        Vec::new(),
        host.clone(),
        Instant::now() + Duration::from_secs(5),
        Arc::new(|| true),
    )?;
    drop(host);
    drop(lease);
    drop(receive);
    let mut wait = Box::pin(released.wait());
    assert!(matches!(
        wait.as_mut()
            .poll(&mut TaskContext::from_waker(Waker::noop())),
        Poll::Pending
    ));
    assert!(airk_store::service::ServiceLease::acquire(root.path()).is_err());
    handle.close();
    assert!(matches!(
        wait.as_mut()
            .poll(&mut TaskContext::from_waker(Waker::noop())),
        Poll::Ready(Ok(()))
    ));
    let _original_reacquired = airk_store::service::ServiceLease::acquire(root.path())?;
    assert!(shared.queue.lock().unwrap().probes.is_empty());
    Ok(())
}

#[test]
fn gpu_fault_circuit_survives_wrapper_epoch_churn_but_real_runtime_identity_changes_do_not_match()
-> Result<()> {
    let key = DeviceFaultKey {
        profile: Sha256Digest::from_bytes([1; 32]),
        execution_identity: "native-synthetic-execution".into(),
        runtime_build: Sha256Digest::from_bytes([2; 32]),
        adapter: [3; 16],
        driver_api_version: 13000,
        driver_image_sha256: Sha256Digest::from_bytes([4; 32]),
        configuration_revision: 7,
        device_catalog: Sha256Digest::from_bytes([5; 32]),
        catalog_source: Sha256Digest::from_bytes([6; 32]),
    };
    assert_eq!(
        stable_circuit_key(key.clone(), 1)?,
        stable_circuit_key(key.clone(), 29)?
    );
    assert!(stable_circuit_key(key.clone(), 0).is_err());
    let mut changed = key.clone();
    changed.driver_image_sha256 = Sha256Digest::from_bytes([8; 32]);
    assert_ne!(stable_circuit_key(changed, 29)?, key);
    let mut changed = key.clone();
    changed.profile = Sha256Digest::from_bytes([9; 32]);
    assert_ne!(stable_circuit_key(changed, 29)?, key);
    let mut changed = key.clone();
    changed.configuration_revision += 1;
    assert_ne!(stable_circuit_key(changed, 29)?, key);
    let mut changed = key.clone();
    changed.catalog_source = Sha256Digest::from_bytes([10; 32]);
    assert_ne!(stable_circuit_key(changed, 29)?, key);
    Ok(())
}
