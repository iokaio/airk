// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn lifecycle_and_continuation_mutations_keep_saved_registry_reads_separate() {
    fn compact(source: &str) -> String {
        source
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect()
    }
    let lifecycle = compact(include_str!("runtime_lifecycle.rs"));
    // These formerly returned a read-only connection directly to write callers.
    for forbidden in [
        "letmutledger=registry(",
        "letmutregistry=registry(",
        "registry(root)?.begin_lifecycle(",
        "registry(root)?.advance_lifecycle(",
        "registry(root)?.clear_service_stop(",
    ] {
        assert!(
            !lifecycle.contains(forbidden),
            "saved read used for mutation: {forbidden}"
        );
    }
    assert!(lifecycle.contains("ServiceRegistry::with_existing_lifecycle("));
    assert!(lifecycle.contains("authority:&InstallerAuthority<'_>"));
    let rebuild = include_str!("runtime_rebuild.rs");
    let continuation = rebuild
        .split("fn continue_clean_break_observing(")
        .nth(1)
        .expect("production continuation")
        .split("fn record_preferences_result(")
        .next()
        .unwrap();
    assert!(!continuation.contains("ServiceRegistry::open_saved("));
    assert!(continuation.contains("ServiceRegistry::with_existing_maintenance("));
}

#[test]
fn lifecycle_mutation_uses_existing_registry_while_native_service_owner_is_live() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("install");
    local_ipc::prepare_root(&root)?;
    layout::initialize(
        &root,
        Some(&owner.path().join("state")),
        RuntimeChannel::Stable,
    )?;
    let _lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &_lock,
    };
    let control = layout::control(&root)?;
    local_ipc::prepare_root(&control)?;
    let lease = airk_store::service::ServiceLease::acquire(&control)?;
    layout::record_control(&lease)?;
    let namespace = Incarnation::from_bytes([81; 16]);
    let instance = Incarnation::from_bytes([82; 16]);
    let mut native = ServiceRegistry::open(&control, namespace)?;
    native.record_service_instance(&lease, instance)?;
    layout::record_registry(&lease, namespace)?;
    let (namespace, epoch) = native.saved_request_epoch()?;
    drop(native);
    // Durable lifecycle admission must work while the actual service owner
    // remains held; it must not need to reacquire or replace that owner.
    assert!(airk_store::service::ServiceLease::acquire(&control).is_err());
    let request = LifecycleRequest {
        caller: Incarnation::from_bytes([83; 16]),
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([84; 16]),
        },
        action: LifecycleAction::Stop,
        expected_instance: instance,
        runtime: Sha256Digest::from_bytes([85; 32]),
        manager: false,
    };
    let accepted = with_lifecycle(&authority, |ledger| ledger.begin_lifecycle(&request))?;
    assert_eq!(accepted.request, request);
    assert_eq!(accepted.phase, LifecyclePhase::Draining);
    let completed = with_lifecycle(&authority, |ledger| {
        ledger.advance_lifecycle(&accepted, LifecyclePhase::Complete)
    })?;
    assert_eq!(completed.request, request);
    assert!(registry(&root)?.service_explicitly_stopped()?);
    with_lifecycle(&authority, |ledger| ledger.clear_service_stop())?;
    assert!(!registry(&root)?.service_explicitly_stopped()?);
    drop(lease);
    Ok(())
}

#[test]
fn lifecycle_observation_preserves_an_older_completed_registry_without_migration() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("install");
    local_ipc::prepare_root(&root)?;
    layout::initialize(
        &root,
        Some(&owner.path().join("state")),
        RuntimeChannel::Stable,
    )?;
    let control = layout::control(&root)?;
    local_ipc::prepare_root(&control)?;
    let lease = airk_store::service::ServiceLease::acquire(&control)?;
    layout::record_control(&lease)?;
    let registry = ServiceRegistry::open(&control, Incarnation::from_bytes([86; 16]))?;
    let namespace = registry.incarnation()?;
    layout::record_registry(&lease, namespace)?;
    drop(registry);
    let database = control.join("service-1.4.sqlite3");
    rusqlite::Connection::open(&database)?
        .execute_batch("DROP TABLE service_configuration; PRAGMA user_version=2;")?;
    let before = fs::read(&database)?;
    let observation = inspect(&root)?;
    assert_eq!(observation["explicitly_stopped"], false);
    assert_eq!(observation["namespace"], namespace.to_string());
    assert_eq!(fs::read(&database)?, before);
    let version: i64 =
        rusqlite::Connection::open(&database)?
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
    assert_eq!(version, 2);
    Ok(())
}
