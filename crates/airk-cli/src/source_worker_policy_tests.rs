// SPDX-License-Identifier: Apache-2.0
use super::*;

fn pending(registration: airk_core::service::ProjectRegistration, epoch: u64) -> Pending {
    Pending {
        registration,
        epoch,
        observed: true,
        expected: 0,
        digest: airk_core::identity::Sha256Digest::from_bytes([7; 32]),
        roots: Vec::new(),
    }
}

#[test]
fn unavailable_policy_discards_binding_and_recovers_at_same_epoch() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let source = base.join("source");
    std::fs::create_dir(&source)?;
    std::fs::write(source.join("retained.txt"), b"original source")?;
    let registration = airk_core::service::ProjectRegistration::new(
        source.to_str().unwrap().into(),
        base.join("data").to_str().unwrap().into(),
        "policy-recovery".into(),
    )?;
    let mut watcher = SourceWatcher::new();
    assert!(install_policy_observation(
        &mut watcher,
        pending(registration.clone(), 1),
        Ok(None)
    )?);
    assert_eq!(watcher.counts().1, 1);
    let unavailable = Err(std::io::Error::new(
        std::io::ErrorKind::WouldBlock,
        "native policy observation temporarily unavailable",
    )
    .into());
    assert!(!install_policy_observation(
        &mut watcher,
        pending(registration.clone(), 1),
        unavailable
    )?);
    assert_eq!(watcher.counts(), (0, 0));
    let observations = watcher.observations(16);
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].project, registration.binding);
    assert_eq!(observations[0].epoch, 1);
    assert_eq!(
        observations[0].failure,
        Some(airk_core::service_watch::WatchFailure::MetadataUnavailable)
    );
    assert!(watcher.notices(16)?.is_empty());
    assert!(install_policy_observation(
        &mut watcher,
        pending(registration, 1),
        Ok(None)
    )?);
    assert_eq!(watcher.counts().1, 1);
    assert_eq!(
        std::fs::read(source.join("retained.txt"))?,
        b"original source"
    );
    assert!(!base.join("data").exists());
    Ok(())
}

#[test]
fn legacy_policy_absence_is_known_but_active_catalog_missing_index_refuses() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let legacy = base.join("legacy");
    crate::local_ipc::prepare_root(&legacy)?;
    let instance = airk_kernel::service_broker::new_instance_id()?;
    let registry = airk_store::service::registry::ServiceRegistry::open(&legacy, instance)?;
    let binding = airk_core::identity::Sha256Digest::from_bytes([8; 32]);
    assert!(source_policy(&registry, binding)?.is_none());
    drop(registry);
    let active = crate::catalog_test_fixture::authority_one(&base.join("active"), instance)?;
    let error = source_policy(&active, binding).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("no current native index binding")
    );
    Ok(())
}
