// SPDX-License-Identifier: Apache-2.0
//! Actual Store metadata/lease tests; these do not qualify a native executable.
use super::*;

fn origin(base: &Path) -> ManagedGenesisOrigin {
    let id = Incarnation::from_bytes([123; 16]);
    let digest = Sha256Digest::from_bytes([124; 32]);
    ManagedGenesisOrigin {
        schema: 1,
        install: base.join("install"),
        state: base.join("state"),
        control: base.join("state/control"),
        operation: id,
        namespace: id,
        policy_sha256: digest,
        policy_version: "1.8.8".into(),
        policy_verification_basis: "organization_digest".into(),
        policy_lifecycle: "user_on_demand".into(),
        source: base.join("provider"),
        runtime_sha256: digest,
        main_sha256: digest,
        helper_sha256: digest,
        installer_identity: digest,
    }
}

fn existing_owned_control(origin: &ManagedGenesisOrigin) -> Result<()> {
    // Store's ServiceLease deliberately does not create its runtime directory;
    // the native adapter owns that precondition. These are fresh fixture dirs.
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&origin.control)?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(&origin.control)?;
    Ok(())
}

#[test]
fn managed_origin_uses_real_genesis_and_is_bound_in_native_candidate_observation() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let origin = origin(&base);
    existing_owned_control(&origin)?;
    let mut owner =
        CatalogGenesis::begin_managed(ServiceLease::acquire(&origin.control)?, &origin)?;
    let receipt = owner.receipt()?;
    assert_eq!(receipt.namespace, origin.namespace);
    assert_eq!(receipt.coordinator_origin.as_ref(), Some(&origin));
    owner.backup()?;
    owner.migrate()?;
    owner.promote_authority_v1()?;
    let observed = activation::observe_authority_v1(
        &origin.control,
        origin.operation,
        Incarnation::from_bytes([125; 16]),
    )?;
    match observed.basis {
        activation::AuthorityV1Basis::FreshGenesis {
            coordinator_origin_digest,
            ..
        } => assert_eq!(coordinator_origin_digest, Some(origin.digest()?)),
        _ => anyhow::bail!("unexpected managed candidate basis"),
    }
    assert!(!observed.catalog.activated);
    assert!(
        owner.settle_authority_v1().is_err(),
        "disabled native activation must remain closed"
    );
    let lease = owner.into_service_lease();
    let resumed = CatalogGenesis::begin_managed(lease, &origin)?;
    assert_eq!(resumed.receipt()?.stage, GenesisStage::Promoted);
    Ok(())
}

#[test]
fn copied_or_changed_origin_cannot_resume_saved_managed_admission() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let origin = origin(&temporary.path().canonicalize()?);
    existing_owned_control(&origin)?;
    drop(CatalogGenesis::begin_managed(
        ServiceLease::acquire(&origin.control)?,
        &origin,
    )?);
    let saved = fs::read(origin.control.join(INTENT))?;
    assert!(
        CatalogGenesis::begin(
            ServiceLease::acquire(&origin.control)?,
            origin.installer_identity,
            origin.operation
        )
        .is_err()
    );
    let mut changed = origin.clone();
    changed.policy_sha256 = Sha256Digest::from_bytes([126; 32]);
    assert!(
        CatalogGenesis::begin_managed(ServiceLease::acquire(&origin.control)?, &changed).is_err()
    );
    assert_eq!(fs::read(origin.control.join(INTENT))?, saved);
    assert!(!origin.control.join("index-catalog-upgrade.json").exists());
    Ok(())
}

#[test]
fn wrong_existing_namespace_is_not_adopted_or_replaced() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let origin = origin(&temporary.path().canonicalize()?);
    existing_owned_control(&origin)?;
    let lease = ServiceLease::acquire(&origin.control)?;
    let foreign = Incarnation::from_bytes([127; 16]);
    drop(super::super::super::ServiceRegistry::open(
        &origin.control,
        foreign,
    )?);
    let before = fs::read(origin.control.join(DATABASE))?;
    assert!(CatalogGenesis::begin_managed(lease, &origin).is_err());
    assert_eq!(fs::read(origin.control.join(DATABASE))?, before);
    assert!(!origin.control.join(INTENT).exists());
    assert_eq!(
        super::super::super::retained_namespace(&origin.control)?,
        foreign
    );
    Ok(())
}

#[test]
fn absent_origin_keeps_old_receipt_and_basis_serialization() -> Result<()> {
    let root = tempfile::tempdir()?;
    let id = Incarnation::from_bytes([128; 16]);
    let digest = Sha256Digest::from_bytes([129; 32]);
    let owner = CatalogGenesis::begin(ServiceLease::acquire(root.path())?, digest, id)?;
    let encoded = serde_json::to_value(owner.receipt()?)?;
    assert!(encoded.get("coordinator_origin").is_none());
    let basis = activation::AuthorityV1Basis::FreshGenesis {
        operation: id,
        legacy_data_digest: digest,
        coordinator_origin_digest: None,
    };
    assert_eq!(
        serde_json::to_value(basis)?,
        serde_json::json!({
        "kind":"fresh_genesis", "operation":id, "legacy_data_digest":digest })
    );
    Ok(())
}

#[test]
fn full_native_receipt_budget_is_checked_without_creating_any_state() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let mut origin = origin(&base);
    preflight_managed_origin(&origin)?;
    let long_path = |label: &str| {
        (0..60).fold(base.join(label), |path, _| {
            path.join("bounded-native-path-segment-abcdefghijklmnopqrstuvwxyz")
        })
    };
    origin.install = long_path("install");
    origin.state = long_path("state");
    origin.source = long_path("provider");
    origin.control = origin.state.join("control");
    origin.validate()?;
    assert!(preflight_managed_origin(&origin).is_err());
    assert!(!base.join("state").exists());
    assert!(!base.join("install").exists());
    assert!(!base.join("provider").exists());
    Ok(())
}
