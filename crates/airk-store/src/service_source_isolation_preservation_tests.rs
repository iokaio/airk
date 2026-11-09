// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::service::{ServiceLease, registry::ServiceRegistry};
use airk_core::identity::Incarnation;

fn fixture() -> Result<(tempfile::TempDir, ServiceLease, ServiceRegistry)> {
    let directory = tempfile::tempdir()?;
    let control = directory.path().canonicalize()?.join("control");
    std::fs::create_dir(&control)?;
    let lease = ServiceLease::acquire(&control)?;
    let registry = ServiceRegistry::open(&control, Incarnation::from_bytes([171; 16]))?;
    Ok((directory, lease, registry))
}

#[test]
fn preserved_private_paths_restrict_future_source_and_data_registration_without_source_reads()
-> Result<()> {
    let (directory, lease, mut registry) = fixture()?;
    let base = directory.path().canonicalize()?;
    let old_data = base.join("old-retained-data");
    registry.preserve_private_paths(&lease, std::slice::from_ref(&old_data))?;
    assert_eq!(registry.saved_private_paths()?, vec![old_data.clone()]);
    for (source, data) in [
        (old_data.join("nested-source"), base.join("new-data")),
        (base.join("new-source"), old_data.join("nested-data")),
    ] {
        let registration = ProjectRegistration::new(
            source.to_str().unwrap().into(),
            data.to_str().unwrap().into(),
            "refused".into(),
        )?;
        assert!(
            registry
                .preflight_source_isolation(&registration, &[])
                .unwrap_err()
                .is::<SourceBoundary>()
        );
        assert!(!source.exists() && !data.exists());
    }
    assert!(
        !old_data.exists(),
        "metadata protection created old retained storage"
    );
    Ok(())
}

#[test]
fn private_path_preservation_is_atomic_bounded_and_requires_the_matching_service_lease()
-> Result<()> {
    let (directory, lease, mut registry) = fixture()?;
    let base = directory.path().canonicalize()?;
    let paths = (0..16)
        .map(|i| base.join(format!("protected-{i}")))
        .collect::<Vec<_>>();
    registry.preserve_private_paths(&lease, &paths)?;
    let before = registry.saved_private_paths()?;
    registry.preserve_private_paths(&lease, &paths)?;
    assert_eq!(registry.saved_private_paths()?, before);
    assert!(
        registry
            .preserve_private_paths(&lease, &[base.join("over-capacity")])
            .is_err()
    );
    assert_eq!(
        registry.saved_private_paths()?,
        before,
        "failed admission partially changed protection"
    );
    let other = base.join("other-control");
    std::fs::create_dir(&other)?;
    let other = ServiceLease::acquire(&other)?;
    assert!(registry.preserve_private_paths(&other, &[]).is_err());
    assert_eq!(registry.saved_private_paths()?, before);
    assert!(
        registry
            .preserve_private_paths(&lease, &[std::path::PathBuf::from("relative")])
            .is_err()
    );
    assert_eq!(registry.saved_private_paths()?, before);
    Ok(())
}

#[test]
fn private_path_preservation_accounts_for_native_layout_paths_in_the_same_transaction() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let base = directory.path().canonicalize()?;
    for name in ["install", "state", "state/control"] {
        std::fs::create_dir(base.join(name))?;
    }
    let control = base.join("state/control");
    let lease = ServiceLease::acquire(&control)?;
    let mut registry = ServiceRegistry::open(&control, Incarnation::from_bytes([172; 16]))?;
    let layout = serde_json::json!({"schema":1,"install":base.join("install"),"state":base.join("state"),
        "logs":base.join("logs"),"channel":"stable","ready":true});
    let encoded = serde_json::to_vec(&layout)?;
    std::fs::write(base.join("install/runtime-layout.json"), &encoded)?;
    std::fs::write(base.join("state/airk-state-1.4.owner.json"), &encoded)?;
    let paths = (0..14)
        .map(|i| base.join(format!("old-private-{i}")))
        .collect::<Vec<_>>();
    assert!(registry.preserve_private_paths(&lease, &paths).is_err());
    assert!(
        registry.saved_private_paths()?.is_empty(),
        "layout union failure must roll back all inserts"
    );
    registry.preserve_private_paths(&lease, &paths[..13])?;
    assert_eq!(registry.saved_private_paths()?.len(), 13);
    assert!(!base.join("old-private-0").exists());
    Ok(())
}
