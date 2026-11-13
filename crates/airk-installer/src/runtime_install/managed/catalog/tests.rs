// SPDX-License-Identifier: Apache-2.0
//! Real native metadata/ownership tests with inert approved payload bytes.
//! These do not qualify execution of an activated paired runtime.
use super::*;
use airk_core::identity::Incarnation;
use airk_store::service::registry::ServiceRegistry;

use genesis::activation::{
    AuthorityV1CandidateObservation, AuthorityV1NativeQualification, AuthorityV1ReadinessProducer,
};
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

// Isolated PHYSICAL-FORMAT fixture, not executed native activation, service
// shutdown, provider containment, or a runtime qualification receipt. Only this
// test leaf sets fixture activation markers; it is not native activation evidence.
fn settled_format_fixture(
    authority: &InstallerAuthority<'_>,
    policy: &Managed,
    origin: &ManagedGenesisOrigin,
) -> Result<()> {
    use genesis::activation::*;
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
fn modern_managed_origin_binds_exact_main_without_a_helper() -> Result<()> {
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::UserOnDemand)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let reservation = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
    assert_eq!(reservation.origin.schema, 2);
    assert_eq!(reservation.origin.helper_sha256, None);
    assert!(!policy.source.join(helper_name()).exists());
    assert!(
        serde_json::to_value(&reservation.origin)?
            .get("helper_sha256")
            .is_none()
    );
    layout::initialize_managed(&root, Some(&state), RuntimeChannel::Stable, &reservation)?;
    verify_origin(&authority, &policy, &reservation.origin)?;
    let main = policy.saved_executable();
    let original = fs::read(&main)?;
    fs::write(&main, b"changed modern provider")?;
    assert!(verify_origin(&authority, &policy, &reservation.origin).is_err());
    fs::write(&main, original)?;
    verify_origin(&authority, &policy, &reservation.origin)?;
    let mut crossed = reservation.origin.clone();
    crossed.schema = 1;
    crossed.helper_sha256 = Some(crossed.main_sha256);
    assert!(verify_origin(&authority, &policy, &crossed).is_err());
    Ok(())
}

#[test]
fn initial_layout_reservation_survives_without_policy_and_fences_resume() -> Result<()> {
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
            airk_host::install_layout::record_control(&lease)?;
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
    fs::write(policy.source.join(main_name()), b"foreign changed main")?;
    assert!(VerifiedManagedReservation::prepare(&authority, &policy, &state).is_err());
    assert_eq!(
        fs::read(policy.source.join(main_name()))?,
        b"foreign changed main"
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
            before_start(&authority, &policy, None)?,
            ManagedCatalogDisposition::LegacyReadCompatible
        ));
        assert_eq!(fs::read(root.join("runtime-layout.json"))?, before);
        assert!(!state.join("control").exists());
    }
    Ok(())
}

#[test]
fn uninitialized_external_policy_refuses_before_callback_or_election() -> Result<()> {
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::External)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    layout::initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    replace_json(&root, POLICY, &policy)?;
    let original_policy = fs::read(root.join(POLICY))?;
    let original_layout = fs::read(root.join("runtime-layout.json"))?;
    let original_provider = fs::read(policy.saved_executable())?;
    let called = std::cell::Cell::new(0);
    let error = tokio::runtime::Runtime::new()?
        .block_on(ensure_checked(&authority, None, || {
            called.set(called.get() + 1);
            Ok(())
        }))
        .err()
        .context("uninitialized external policy must require a clean break")?;
    assert!(
        error.to_string().contains("clean_break_required"),
        "{error:#}"
    );
    assert_eq!(called.get(), 0);
    assert_eq!(fs::read(root.join(POLICY))?, original_policy);
    assert_eq!(fs::read(root.join("runtime-layout.json"))?, original_layout);
    assert_eq!(fs::read(policy.saved_executable())?, original_provider);
    assert!(!state.join("control").exists());
    assert!(!root.join("selected.json").exists());
    assert!(!root.join("bootstrap.json").exists());
    Ok(())
}

#[test]
fn initialized_external_callback_runs_once_before_health_without_election() -> Result<()> {
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::External)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let reservation = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
    layout::initialize_managed(&root, Some(&state), RuntimeChannel::Stable, &reservation)?;
    let mut genesis = admit(&authority, &policy, reservation.origin())?;
    genesis.backup()?;
    genesis.migrate()?;
    genesis.promote_authority_v1()?;
    genesis.record_authority_v1_readiness(Incarnation::from_bytes([144; 16]), &FixtureProducer)?;
    state_contract::initialize(&root, &genesis)?;
    genesis.settle_authority_v1()?;
    drop(genesis);
    genesis::require_managed_settled(&reservation.origin().control, reservation.origin())?;
    replace_json(&root, POLICY, &policy)?;
    let called = std::cell::Cell::new(0);
    let result = tokio::runtime::Runtime::new()?.block_on(ensure_checked(&authority, None, || {
        assert!(!root.join("selected.json").exists());
        called.set(called.get() + 1);
        Ok(())
    }));
    let error = result
        .err()
        .context("no external supervisor should report health")?;
    assert!(error.is::<SupervisorUnavailable>(), "{error:#}");
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
fn settled_some_checks_current_policy_and_pair() -> Result<()> {
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
    verify_origin(&authority, &policy, reservation.origin())?;
    airk_host::install_layout::require_managed_ready(&root)?;
    let mut changed = policy.clone();
    changed.lifecycle = ManagedLifecycle::External;
    replace_json(&root, POLICY, &changed)?;
    assert!(
        saved_policy(&root).is_err(),
        "changed lifecycle cannot select STOP ownership"
    );
    assert!(verify_origin(&authority, &changed, reservation.origin()).is_err());
    replace_json(&root, POLICY, &policy)?;
    fs::write(policy.source.join(main_name()), b"changed main preserved")?;
    assert!(verify_origin(&authority, &policy, reservation.origin()).is_err());
    assert_eq!(
        fs::read(policy.source.join(main_name()))?,
        b"changed main preserved"
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
    assert_eq!(
        airk_host::install_layout::managed_origin(&root)?
            .unwrap()
            .namespace,
        namespace
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires fresh sealed AIRK_F03_RUNTIME and explicit approval; run serially"]
async fn actual_managed_first_pin_settles_before_connection_and_external_health() -> Result<()> {
    let source = PathBuf::from(
        std::env::var_os("AIRK_F03_RUNTIME").context("fresh sealed runtime required")?,
    )
    .canonicalize()?;
    let approved =
        std::env::var("AIRK_F03_APPROVED_SHA256").context("explicit runtime approval required")?;
    ensure!(
        runtime_pair::digest_file(&source.join("airk-install.json"))? == approved,
        "approved input differs"
    );
    for lifecycle in [ManagedLifecycle::UserOnDemand, ManagedLifecycle::External] {
        let directory = tempfile::tempdir()?;
        let base = directory.path().canonicalize()?;
        let root = base.join("install");
        let state = base.join("state");
        local_ipc::prepare_root(&root)?;
        let mut external = None;
        let result = async {
            let configured = configure(
                &source,
                &root,
                &approved,
                VerificationBasis::OrganizationDigest,
                lifecycle,
                Some(&state),
                RuntimeChannel::Stable,
            )
            .await?;
            let origin = airk_host::install_layout::managed_origin(&root)?
                .context("actual first pin lacks reservation")?;
            let receipt =
                genesis::pending(&origin.control)?.context("actual Genesis receipt missing")?;
            assert_eq!(receipt.stage, genesis::GenesisStage::Settled);
            assert_eq!(receipt.coordinator_origin.as_ref(), Some(&origin));
            assert!(receipt.authority_v1.is_some());
            genesis::require_managed_settled(&origin.control, &origin)?;
            let saved = ServiceRegistry::open_saved(&origin.control)?;
            let catalog = saved
                .catalog_state()?
                .context("actual activated catalog missing")?;
            assert!(catalog.activated);
            assert_eq!(catalog.authority_contract, 1);
            drop(saved);
            if lifecycle == ManagedLifecycle::External {
                assert_eq!(configured["awaiting_external_supervisor"], true);
                assert!(!root.join("selected.json").exists());
                let _excluded = ServiceLease::acquire(&origin.control)?;
                drop(_excluded);
                let mut command = Command::new(source.join(main_name()));
                command
                    .args(["serve", "--runtime-dir"])
                    .arg(&origin.control)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                hidden(&mut command);
                external = Some(airk_host::native_process::spawn(&mut command)?);
            } else {
                assert_eq!(configured["selected"], true);
            }
            let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            let first = loop {
                match ensure_running(&root).await {
                    Ok(health) => break health,
                    Err(error)
                        if tokio::time::Instant::now() < deadline
                            && lifecycle == ManagedLifecycle::External =>
                    {
                        ensure!(
                            error.downcast_ref::<SupervisorUnavailable>().is_some(),
                            "unexpected external refusal: {error:#}"
                        );
                        tokio::time::sleep(Duration::from_millis(25)).await;
                    }
                    Err(error) => return Err(error),
                }
            };
            assert_eq!(selection_record(&root)?.active, approved);
            assert!(
                !root.join("runtimes").exists(),
                "managed pin copied its immutable provider"
            );
            let called = std::cell::Cell::new(0);
            let lock = install_lock(&root)?;
            let authority = InstallerAuthority {
                root: &root,
                _lock: &lock,
            };
            let second = ensure_running_locked_after_settlement(&authority, || {
                genesis::require_managed_settled(&origin.control, &origin)?;
                called.set(called.get() + 1);
                Ok(())
            })
            .await?;
            assert_eq!(called.get(), 1);
            assert_eq!(
                first.instance, second.instance,
                "healthy connection re-elected service"
            );
            assert_eq!(
                genesis::pending(&origin.control)?.unwrap().operation,
                receipt.operation
            );
            let primary = base.join("source");
            fs::create_dir(&primary)?;
            let original = b"export function ManagedFirstEvidence() { return 17; }\n";
            fs::write(primary.join("fixture.ts"), original)?;
            let epoch = control_client::request_for_version(
                &origin.control,
                ControlMethod::RequestEpoch,
                env!("CARGO_PKG_VERSION"),
            )
            .await?;
            let ControlResponse::RequestEpoch { namespace, epoch } = epoch else {
                anyhow::bail!("actual managed request epoch missing");
            };
            let current = ServiceRegistry::open_saved(&origin.control)?
                .catalog_state()?
                .context("managed catalog missing")?;
            let selection = airk_core::catalog_setup::IndexSetupSelection {
                key: airk_core::service::RequestKey {
                    namespace,
                    epoch,
                    nonce: airk_kernel::service_broker::new_instance_id()?,
                },
                expected_catalog_revision: current.revision,
                name: "Managed first Index".into(),
                primary_root: primary.to_str().context("source path")?.into(),
                data_root: base
                    .join("index-data")
                    .to_str()
                    .context("data path")?
                    .into(),
                collection: "managed-first".into(),
                additional_roots: vec![],
                setup_watch: false,
            };
            let draft = control_client::request_for_version(
                &origin.control,
                ControlMethod::CatalogIndexSetupDraft { selection },
                env!("CARGO_PKG_VERSION"),
            )
            .await?;
            let ControlResponse::CatalogIndexSetupDraft { draft, .. } = draft else {
                anyhow::bail!("actual managed native setup review missing");
            };
            let applied = control_client::request_for_version(
                &origin.control,
                ControlMethod::CatalogIndexSetupApply { draft },
                env!("CARGO_PKG_VERSION"),
            )
            .await?;
            let ControlResponse::CatalogIndexSetupApplied { receipt: setup } = applied else {
                anyhow::bail!(
                    "actual managed native setup was not applied: {}",
                    serde_json::to_string(&applied)?
                );
            };
            assert_eq!(setup.stage, airk_core::catalog_setup::SetupStage::Ready);
            assert_eq!(fs::read(primary.join("fixture.ts"))?, original);
            let eligibility = control_client::request_for_version(
                &origin.control,
                ControlMethod::CatalogLegacyBinding {
                    project: setup.request.registration.binding,
                    expected_catalog_revision: None,
                },
                env!("CARGO_PKG_VERSION"),
            )
            .await?;
            let ControlResponse::CatalogLegacyBinding { observation } = eligibility else {
                anyhow::bail!("actual managed compatibility observation missing");
            };
            assert!(
                matches!(
                    observation.eligibility,
                    airk_core::catalog_host::NativeLegacyBindingEligibility::Unavailable
                ),
                "setup alone granted evidence access"
            );
            Ok::<_, anyhow::Error>(())
        }
        .await;
        let control = state.join("control");
        let _ = control_client::request_for_version(
            &control,
            ControlMethod::Shutdown,
            env!("CARGO_PKG_VERSION"),
        )
        .await;
        let cleanup = wait_quiescent(&root).await;
        if let Some(mut child) = external {
            if cleanup.is_err() {
                let _ = child.kill();
            }
            child.wait()?;
        }
        result.and(cleanup)?;
    }
    Ok(())
}

#[test]
fn settled_managed_detach_preserves_reservation_and_external_owner_and_retries() -> Result<()> {
    let (_owner, root, state, policy) = fixture(ManagedLifecycle::External)?;
    let lock = install_lock(&root)?;
    let authority = InstallerAuthority {
        root: &root,
        _lock: &lock,
    };
    let reservation = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
    layout::initialize_managed(&root, Some(&state), RuntimeChannel::Stable, &reservation)?;
    let origin = reservation.origin();
    settled_format_fixture(&authority, &policy, origin)?;
    replace_json(&root, POLICY, &policy)?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: policy.approval.manifest_sha256.clone(),
            previous: None,
        },
    )?;
    let layout_before = fs::read(root.join("runtime-layout.json"))?;
    let genesis_before = fs::read(origin.control.join("index-catalog-genesis.json"))?;
    let provider_before = fs::read(policy.saved_executable())?;
    let lease = ServiceLease::acquire(&origin.control)?;
    let runtime = tokio::runtime::Runtime::new()?;
    for _ in 0..2 {
        let reply = runtime.block_on(detach(&root))?;
        assert_eq!(reply["detached"], true);
        assert_eq!(reply["external_supervisor_continues"], true);
        assert!(ServiceLease::acquire(&origin.control).is_err());
        assert!(!root.join(POLICY).exists());
        assert!(!root.join("selected.json").exists());
        assert!(!root.join(DETACH).exists());
        assert_eq!(fs::read(root.join("runtime-layout.json"))?, layout_before);
        assert_eq!(
            fs::read(origin.control.join("index-catalog-genesis.json"))?,
            genesis_before
        );
        assert_eq!(fs::read(policy.saved_executable())?, provider_before);
        assert!(require_unmanaged(&root).is_err());
        let error = runtime
            .block_on(ensure_running_locked(&authority))
            .err()
            .expect("detached managed runtime must not start");
        assert!(
            error
                .to_string()
                .contains("repeat_same_approved_managed_configure")
        );
    }
    let reattach = VerifiedManagedReservation::prepare(&authority, &policy, &state)?;
    assert_eq!(reattach.origin(), origin);
    drop(lease);
    drop(lock);
    let error = runtime
        .block_on(lifecycle::explicit_start(&root))
        .err()
        .expect("detached managed runtime must not start");
    assert!(error.to_string().contains("lifecycle_managed_externally"));
    assert!(!root.join(POLICY).exists());
    Ok(())
}

#[test]
fn settled_managed_detach_rejects_crossed_policy_and_resumes_missing_provider() -> Result<()> {
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
    let mut crossed = policy.clone();
    crossed.lifecycle = ManagedLifecycle::External;
    replace_json(&root, POLICY, &crossed)?;
    let runtime = tokio::runtime::Runtime::new()?;
    assert!(runtime.block_on(detach(&root)).is_err());
    assert!(!root.join(DETACH).exists());
    assert_eq!(read(&root)?, Some(crossed));
    replace_json(&root, POLICY, &policy)?;
    // Exact accepted intent survives removal of user references and provider.
    replace_json(&root, DETACH, &policy)?;
    fs::remove_file(root.join(POLICY))?;
    let retained = policy.source.join("retained-provider-main");
    fs::rename(policy.saved_executable(), &retained)?;
    let before = fs::read(&retained)?;
    let reply = runtime.block_on(detach(&root))?;
    assert_eq!(reply["detached"], true);
    assert_eq!(reply["external_supervisor_continues"], false);
    assert_eq!(fs::read(&retained)?, before);
    assert!(!root.join(POLICY).exists());
    assert!(!root.join(DETACH).exists());
    assert_eq!(
        airk_host::install_layout::managed_origin(&root)?.as_ref(),
        Some(origin)
    );
    genesis::require_managed_settled(&origin.control, origin)?;
    drop(lock);
    let error = runtime
        .block_on(lifecycle::explicit_start(&root))
        .err()
        .expect("detached managed runtime must not start");
    assert!(
        error
            .to_string()
            .contains("repeat_same_approved_managed_configure")
    );
    assert!(!root.join("selected.json").exists());
    Ok(())
}

#[test]
fn settled_managed_origin_survives_authenticated_embedded_schema_promotion() -> Result<()> {
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
    replace_json(&root, POLICY, &policy)?;
    let before = fs::read(origin.control.join("index-catalog-genesis.json"))?;
    airk_store::service::registry::runtime_contract::require_embedded_default_write_for_namespace(
        &origin.control,
        origin.namespace,
    )?;
    let registry = ServiceRegistry::open_saved(&origin.control)?;
    let current = registry
        .catalog_state()?
        .context("promoted catalog missing")?;
    assert_eq!(current.schema, 13);
    assert!(current.accepted_new_writes);
    drop(registry);
    genesis::require_managed_settled(&origin.control, origin)?;
    require_detach_policy(&root, &policy)?;
    assert_eq!(
        fs::read(origin.control.join("index-catalog-genesis.json"))?,
        before
    );
    // Both excluded and live-owner read-only readiness paths retain identity.
    let lease = ServiceLease::acquire(&origin.control)?;
    genesis::require_managed_settled(&origin.control, origin)?;
    drop(lease);
    Ok(())
}
