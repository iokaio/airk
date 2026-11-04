// SPDX-License-Identifier: Apache-2.0
//! Real native metadata/ownership tests with inert approved payload bytes.
//! These do not qualify execution of an activated paired runtime.
use super::*;
use airk_core::identity::Incarnation;

// Isolated PHYSICAL-FORMAT fixture, not executed native activation, service
// shutdown, provider containment, or a runtime qualification receipt. Only this
// test leaf sets fixture activation markers; production keeps its flag false.
fn settled_format_fixture(
    authority: &InstallerAuthority<'_>,
    policy: &Managed,
    origin: &ManagedGenesisOrigin,
) -> Result<()> {
    use genesis::activation::*;
    struct FixtureProducer;
    impl AuthorityV1ReadinessProducer for FixtureProducer {
        fn required_predecessor_identities(&self) -> Result<Vec<Sha256Digest>> {
            Ok(Vec::new())
        }
        fn qualify(
            &self,
            expected: &AuthorityV1CandidateObservation,
        ) -> Result<AuthorityV1NativeQualification> {
            Ok(AuthorityV1NativeQualification {
                candidate: expected.clone(),
                predecessor_refusals: Vec::new(),
            })
        }
    }
    let mut owner = admit(authority, policy, origin)?;
    owner.backup()?;
    owner.migrate()?;
    owner.promote_authority_v1()?;
    let mut receipt = owner
        .record_authority_v1_readiness(Incarnation::from_bytes([143; 16]), &FixtureProducer)?;
    drop(owner);
    let sql = rusqlite::Connection::open(origin.control.join("service-1.4.sqlite3"))?;
    sql.execute("UPDATE index_catalog_meta SET activated=1", [])?;
    drop(sql);
    receipt.stage = genesis::GenesisStage::Settled;
    receipt.authority_v1.as_mut().unwrap().phase = AuthorityV1Phase::Settled;
    fs::write(
        origin.control.join("index-catalog-genesis.json"),
        serde_json::to_vec(&receipt)?,
    )?;
    let marker = origin.control.join("service-1.4.owner.json");
    let mut owner: serde_json::Value = serde_json::from_slice(&fs::read(&marker)?)?;
    owner.as_object_mut().unwrap().remove("catalog_upgrade");
    fs::write(marker, serde_json::to_vec(&owner)?)?;
    genesis::require_managed_settled(&origin.control, origin)
}

fn fixture(lifecycle: ManagedLifecycle) -> Result<(tempfile::TempDir, PathBuf, PathBuf, Managed)> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let root = base.join("install");
    let state = base.join("state");
    local_ipc::prepare_root(&root)?;
    let source = base.join("provider");
    local_ipc::prepare_root(&source)?;
    let identity = super::super::super::tests::fixture(&source)?;
    let policy = Managed {
        schema: 1,
        install: root.clone(),
        source: source.clone(),
        approval: Approval {
            schema: 1,
            manifest_sha256: identity,
            version: env!("CARGO_PKG_VERSION").into(),
            verification_basis: VerificationBasis::OrganizationDigest,
        },
        executable_sha256: runtime_pair::digest_file(&source.join(main_name()))?,
        lifecycle,
    };
    Ok((owner, root, state, policy))
}

#[test]
fn initial_layout_reservation_survives_without_policy_and_fences_disabled_start() -> Result<()> {
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let reservation = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
    layout::initialize_managed(&root, Some(&state), RuntimeChannel::Stable, &reservation)?;
    let before = fs::read(root.join("runtime-layout.json"))?;
    assert!(!root.join(POLICY).exists());
    assert!(!reservation.origin.control.exists());
    assert!(before_start_enabled(&authority, &policy, None, false).is_err());
    let error = resume_before_lifecycle(&authority, None).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("repeat_same_approved_managed_configure")
    );
    assert!(startup_selected(&root).is_err());
    let retry = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
    assert_eq!(retry.origin, reservation.origin);
    layout::initialize_managed(&root, Some(&state), RuntimeChannel::Stable, &retry)?;
    assert_eq!(fs::read(root.join("runtime-layout.json"))?, before);
    assert!(!reservation.origin.control.exists());
    Ok(())
}

#[test]
fn actual_control_registry_and_genesis_prefixes_resume_one_reserved_namespace() -> Result<()> {
    for prefix in 0..=3 {
        let (_owner, root, state, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
        let original_source = fs::read(policy.source.join(main_name()))?;
        let lock = install_lock(&root)?;
        let authority = InstallerAuthority {
            root: &root,
            _lock: &lock,
        };
        let reservation = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
        layout::initialize_managed(&root, Some(&state), RuntimeChannel::Stable, &reservation)?;
        let origin = reservation.origin();
        if prefix >= 1 {
            layout::prepare_managed_control(&authority, origin)?;
            let lease = ServiceLease::acquire(&origin.control)?;
            layout::record_control(&lease)?;
            if prefix >= 2 {
                drop(airk_store::service::registry::ServiceRegistry::open(
                    &origin.control,
                    origin.namespace,
                )?);
            }
            drop(lease);
        }
        if prefix == 3 {
            drop(admit(&authority, &policy, origin)?);
        }
        let retry = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
        assert_eq!(retry.origin(), origin);
        let owner = admit(&authority, &policy, retry.origin())?;
        let receipt = owner.receipt()?;
        assert_eq!(receipt.namespace, origin.namespace);
        assert_eq!(receipt.operation, origin.operation);
        assert_eq!(receipt.coordinator_origin.as_ref(), Some(origin));
        assert!(ServiceLease::acquire(&origin.control).is_err());
        assert!(!root.join(POLICY).exists());
        drop(owner);
        let resumed = admit(&authority, &policy, origin)?;
        assert_eq!(resumed.receipt()?.namespace, receipt.namespace);
        assert_eq!(fs::read(policy.source.join(main_name()))?, original_source);
        assert!(!root.join("selected.json").exists());
    }
    Ok(())
}

#[test]
fn origin_cannot_be_attached_to_old_layout_or_changed_by_new_policy_or_pair() -> Result<()> {
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    layout::initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    let before = fs::read(root.join("runtime-layout.json"))?;
    let reservation = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
    assert!(
        layout::initialize_managed(&root, Some(&state), RuntimeChannel::Stable, &reservation)
            .is_err()
    );
    assert_eq!(fs::read(root.join("runtime-layout.json"))?, before);
    assert!(!String::from_utf8(before)?.contains("managed_catalog_origin"));
    Ok(())
}

#[test]
fn changed_reserved_policy_or_payload_is_preserved_and_refused() -> Result<()> {
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let reservation = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
    layout::initialize_managed(&root, Some(&state), RuntimeChannel::Stable, &reservation)?;
    let before = fs::read(root.join("runtime-layout.json"))?;
    let mut changed = policy.clone();
    changed.lifecycle = ManagedLifecycle::External;
    assert!(VerifiedManagedReservation::prepare(&authority, &changed, &state).is_err());
    fs::write(policy.source.join(helper_name()), b"foreign changed helper")?;
    assert!(VerifiedManagedReservation::prepare(&authority, &policy, &state).is_err());
    assert_eq!(
        fs::read(policy.source.join(helper_name()))?,
        b"foreign changed helper"
    );
    assert_eq!(fs::read(root.join("runtime-layout.json"))?, before);
    assert!(!reservation.origin.control.exists());
    Ok(())
}

#[test]
fn legacy_none_dispositions_do_not_create_catalogs_or_claim_cutover() -> Result<()> {
    for lifecycle in [ManagedLifecycle::UserOnDemand, ManagedLifecycle::External] {
        let (_owner, root, state, policy) = fixture(lifecycle)?;
        let lock = install_lock(&root)?;
        let authority = InstallerAuthority {
            root: &root,
            _lock: &lock,
        };
        layout::initialize(&root, Some(&state), RuntimeChannel::Stable)?;
        let before = fs::read(root.join("runtime-layout.json"))?;
        assert!(matches!(
            before_start_enabled(&authority, &policy, None, false)?,
            ManagedCatalogDisposition::Disabled
        ));
        assert!(matches!(
            before_start_enabled(&authority, &policy, None, true)?,
            ManagedCatalogDisposition::LegacyReadCompatible
        ));
        assert_eq!(fs::read(root.join("runtime-layout.json"))?, before);
        assert!(!state.join("control").exists());
    }
    Ok(())
}

#[test]
fn external_legacy_callback_runs_once_before_health_without_election() -> Result<()> {
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::External)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    layout::initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    replace_json(&root, POLICY, &policy)?;
    let called = std::cell::Cell::new(0);
    let result = tokio::runtime::Runtime::new()?.block_on(ensure_checked(&authority, None, || {
        assert!(!root.join("selected.json").exists());
        called.set(called.get() + 1);
        Ok(())
    }));
    assert!(result.is_err());
    assert_eq!(called.get(), 1);
    assert!(!root.join("selected.json").exists());
    assert!(!root.join("bootstrap.json").exists());
    Ok(())
}

#[test]
fn some_without_policy_preserves_managed_mode_and_refuses_detach_before_effects() -> Result<()> {
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let reservation = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
    layout::initialize_managed(&root, Some(&state), RuntimeChannel::Stable, &reservation)?;
    let before = fs::read(root.join("runtime-layout.json"))?;
    assert!(is_configured(&root)?);
    assert!(require_unmanaged(&root).is_err());
    let result = tokio::runtime::Runtime::new()?.block_on(detach(&root));
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("supported_transition")
    );
    assert!(!root.join(DETACH).exists());
    assert_eq!(fs::read(root.join("runtime-layout.json"))?, before);
    assert!(!reservation.origin.control.exists());
    Ok(())
}

#[test]
fn settled_some_checks_current_policy_and_pair_even_with_activation_disabled() -> Result<()> {
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let reservation = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
    layout::initialize_managed(&root, Some(&state), RuntimeChannel::Stable, &reservation)?;
    settled_format_fixture(&authority, &policy, reservation.origin())?;
    replace_json(&root, POLICY, &policy)?;
    assert!(matches!(
        before_start_enabled(&authority, &policy, None, false)?,
        ManagedCatalogDisposition::Disabled
    ));
    let mut changed = policy.clone();
    changed.lifecycle = ManagedLifecycle::External;
    replace_json(&root, POLICY, &changed)?;
    assert!(
        saved_policy(&root).is_err(),
        "changed lifecycle cannot select STOP ownership"
    );
    assert!(before_start_enabled(&authority, &changed, None, false).is_err());
    replace_json(&root, POLICY, &policy)?;
    fs::write(
        policy.source.join(helper_name()),
        b"changed helper preserved",
    )?;
    assert!(before_start_enabled(&authority, &policy, None, false).is_err());
    assert_eq!(
        fs::read(policy.source.join(helper_name()))?,
        b"changed helper preserved"
    );
    Ok(())
}

#[test]
fn settled_missing_policy_recovers_saved_snapshot_and_stop_metadata_without_start() -> Result<()> {
    use airk_core::{
        service::RequestKey,
        service_lifecycle::{LifecycleAction, LifecyclePhase, LifecycleRequest},
    };
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let reservation = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
    layout::initialize_managed(&root, Some(&state), RuntimeChannel::Stable, &reservation)?;
    let origin = reservation.origin();
    settled_format_fixture(&authority, &policy, origin)?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: policy.approval.manifest_sha256.clone(),
            previous: None,
        },
    )?;
    // No policy file and no executable is run. The pending native record tests
    // saved identity/order only; absence of a process is not a drain proof.
    let instance = Incarnation::from_bytes([144; 16]);
    let lease = ServiceLease::acquire(&origin.control)?;
    let mut registry =
        airk_store::service::registry::ServiceRegistry::open(&origin.control, instance)?;
    registry.record_service_instance(&lease, instance)?;
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let request = LifecycleRequest {
        caller: Incarnation::from_bytes([145; 16]),
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([146; 16]),
        },
        action: LifecycleAction::Stop,
        expected_instance: instance,
        runtime: policy.approval.manifest_sha256.parse()?,
        manager: false,
    };
    registry.begin_lifecycle(&request)?;
    drop(registry);
    drop(lease);
    let saved = snapshot::identity(&root)?;
    assert_eq!(saved.executable, policy.saved_executable());
    assert_eq!(saved.approval, policy.approval);
    let called = std::cell::Cell::new(0);
    let result = tokio::runtime::Runtime::new()?.block_on(
        ensure_running_locked_checked_after_settlement(&authority, None, || {
            called.set(called.get() + 1);
            Ok(())
        }),
    );
    assert!(
        result
            .err()
            .context("saved STOP unexpectedly admitted startup")?
            .to_string()
            .contains("service_explicitly_stopped")
    );
    let registry = airk_store::service::registry::ServiceRegistry::open_saved(&origin.control)?;
    assert_eq!(
        registry.lifecycle_record(&request)?.unwrap().phase,
        LifecyclePhase::Complete
    );
    assert!(registry.service_explicitly_stopped()?);
    assert_eq!(called.get(), 0);
    assert!(!root.join(POLICY).exists());
    assert!(!root.join("bootstrap.json").exists());
    Ok(())
}

#[test]
fn canonical_policy_identity_does_not_depend_on_policy_file_whitespace() -> Result<()> {
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let reservation = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
    layout::initialize_managed(&root, Some(&state), RuntimeChannel::Stable, &reservation)?;
    fs::write(root.join(POLICY), serde_json::to_vec_pretty(&policy)?)?;
    assert_eq!(
        policy_digest(&saved_policy(&root)?.unwrap())?,
        reservation.origin.policy_sha256
    );
    Ok(())
}

#[test]
fn legacy_p5_active_zero_is_read_compatible_but_readiness_and_inactive_are_not() -> Result<()> {
    use airk_store::service::registry::catalog::upgrade::{
        self, CatalogUpgrade, ReadinessEvidence, RuntimeRefusal, UpgradeStage,
    };
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    layout::initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    let control = layout::control(&root)?;
    local_ipc::prepare_root(&control)?;
    let lease = ServiceLease::acquire(&control)?;
    layout::record_control(&lease)?;
    let instance = Incarnation::from_bytes([147; 16]);
    drop(airk_store::service::registry::ServiceRegistry::open(
        &control, instance,
    )?);
    layout::record_registry(&lease, instance)?;
    let owner = CatalogUpgrade::acquire_with_service(lease)?;
    let admitted = airk_kernel::service_catalog_upgrade::prepare(
        &owner,
        runtime_pair::engine_identity_at(&policy.saved_executable())?,
    )?;
    owner.migrate()?;
    let observed = upgrade::observe(
        &control,
        admitted.plan.id,
        Incarnation::from_bytes([148; 16]),
    )?;
    let digest = Sha256Digest::from_bytes([149; 32]);
    let mut receipt = owner.record_readiness(ReadinessEvidence {
        schema: 1,
        candidate: observed,
        predecessor_refusals: vec![RuntimeRefusal {
            payload_identity: digest,
            read_refusal_digest: digest,
            write_refusal_digest: digest,
            native_provenance: None,
        }],
    })?;
    assert!(
        before_start_enabled(&authority, &policy, None, true).is_err(),
        "pending readiness is not legacy startup"
    );
    drop(owner);
    // Physical old-P5 active format only. This does not prove an executed
    // installer, predecessor refusal, or historical helper completion.
    let sql = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
    sql.execute("UPDATE index_catalog_meta SET activated=1", [])?;
    drop(sql);
    receipt.stage = UpgradeStage::Settled;
    fs::write(
        control.join("index-catalog-upgrade.json"),
        serde_json::to_vec(&receipt)?,
    )?;
    let path = control.join("service-1.4.owner.json");
    let mut marker: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    marker.as_object_mut().unwrap().remove("catalog_upgrade");
    fs::write(path, serde_json::to_vec(&marker)?)?;
    assert!(matches!(
        before_start_enabled(&authority, &policy, None, true)?,
        ManagedCatalogDisposition::LegacyReadCompatible
    ));
    let before = fs::read(control.join("index-catalog-upgrade.json"))?;
    let sql = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
    sql.execute("UPDATE index_catalog_meta SET activated=0", [])?;
    drop(sql);
    assert!(before_start_enabled(&authority, &policy, None, true).is_err());
    assert_eq!(
        fs::read(control.join("index-catalog-upgrade.json"))?,
        before
    );
    assert!(!root.join("selected.json").exists());
    Ok(())
}

#[test]
fn bounded_native_revalidation_does_not_rehash_model_assets_after_full_preflight() -> Result<()> {
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let reservation = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
    layout::initialize_managed(&root, Some(&state), RuntimeChannel::Stable, &reservation)?;
    // Topology/full-payload validation remains the owner's separate preflight.
    // Per-probe identity verification reads actual metadata/main/helper only.
    fs::write(
        policy.source.join("unlisted-model-asset.bin"),
        b"retained model bytes",
    )?;
    assert!(policy.verify_inventory().is_err());
    verify_origin(&authority, &policy, reservation.origin())?;
    assert_eq!(
        fs::read(policy.source.join("unlisted-model-asset.bin"))?,
        b"retained model bytes"
    );
    Ok(())
}

#[test]
fn control_creation_requires_the_exact_claimed_managed_reservation() -> Result<()> {
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let reservation = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
    layout::initialize_managed(&root, Some(&state), RuntimeChannel::Stable, &reservation)?;
    let mut changed = reservation.origin().clone();
    changed.control = root.join("foreign-control");
    assert!(layout::prepare_managed_control(&authority, &changed).is_err());
    assert!(!changed.control.exists());
    assert!(!reservation.origin.control.exists());
    layout::prepare_managed_control(&authority, reservation.origin())?;
    local_ipc::require_existing_root(&reservation.origin.control)?;
    let lease = ServiceLease::acquire(&reservation.origin.control)?;
    assert_eq!(lease.root(), reservation.origin.control);
    Ok(())
}

#[test]
fn lost_initialized_control_is_not_recreated_or_assigned_another_namespace() -> Result<()> {
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let reservation = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
    layout::initialize_managed(&root, Some(&state), RuntimeChannel::Stable, &reservation)?;
    let owner = admit(&authority, &policy, reservation.origin())?;
    let namespace = owner.receipt()?.namespace;
    drop(owner);
    let mut markers = Vec::new();
    for entry in fs::read_dir(&state)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            markers.push((entry.path(), fs::read(entry.path())?));
        }
    }
    assert!(!markers.is_empty());
    // Only this test's freshly created, process-free control subtree is lost.
    assert_eq!(reservation.origin.control.parent(), Some(state.as_path()));
    fs::remove_dir_all(&reservation.origin.control)?;
    assert!(layout::prepare_managed_control(&authority, reservation.origin()).is_err());
    assert!(!reservation.origin.control.exists());
    for (path, bytes) in markers {
        assert_eq!(fs::read(path)?, bytes);
    }
    assert_eq!(layout::managed_origin(&root)?.unwrap().namespace, namespace);
    Ok(())
}
