// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::service::{ProjectLease, registry::ServiceRegistry};
use airk_core::identity::Incarnation;

fn project(root: &Path, source: &str, data: &str, collection: &str) -> Result<ProjectRegistration> {
    ProjectRegistration::new(
        root.join(source).to_str().unwrap().into(),
        root.join(data).to_str().unwrap().into(),
        collection.into(),
    )
}

#[test]
fn managed_layout_origin_preserves_strict_receipts_namespace_and_payload_isolation() -> Result<()> {
    // Inert receipt-format fixture only: no activation or process qualification.
    let directory = tempfile::tempdir()?;
    let base = directory.path().canonicalize()?;
    for name in [
        "install",
        "state",
        "state/control",
        "logs",
        "payload",
        "other-payload",
    ] {
        std::fs::create_dir(base.join(name))?;
    }
    let control = base.join("state/control");
    let namespace = Incarnation::from_bytes([7; 16]);
    let registry = ServiceRegistry::open(&control, namespace)?;
    let origin = super::super::catalog::genesis::ManagedGenesisOrigin {
        schema: 1,
        install: base.join("install"),
        state: base.join("state"),
        control: control.clone(),
        operation: namespace,
        namespace,
        policy_sha256: airk_core::identity::Sha256Digest::from_bytes([1; 32]),
        policy_version: "1.8.8".into(),
        policy_verification_basis: "organization_digest".into(),
        policy_lifecycle: "user_on_demand".into(),
        source: base.join("payload"),
        runtime_sha256: airk_core::identity::Sha256Digest::from_bytes([2; 32]),
        main_sha256: airk_core::identity::Sha256Digest::from_bytes([3; 32]),
        helper_sha256: Some(airk_core::identity::Sha256Digest::from_bytes([4; 32])),
        installer_identity: airk_core::identity::Sha256Digest::from_bytes([5; 32]),
    };
    let layout = serde_json::json!({"schema":1,"install":origin.install,"state":origin.state,
        "logs":base.join("logs"),"channel":"stable","ready":true,"managed_catalog_origin":origin});
    let install_receipt = base.join("install/runtime-layout.json");
    let state_receipt = base.join("state/airk-state-1.4.owner.json");
    let save = |value: &serde_json::Value| -> Result<()> {
        let bytes = serde_json::to_vec(value)?;
        std::fs::write(&install_receipt, &bytes)?;
        std::fs::write(&state_receipt, &bytes)?;
        Ok(())
    };
    save(&layout)?;
    assert!(private_paths(&registry.connection, &control)?.contains(&base.join("payload")));
    for (source, data) in [
        ("payload/nested", "data"),
        ("source", "payload/derived"),
        ("install", "data"),
    ] {
        assert!(
            validate(
                &registry.connection,
                &control,
                &project(&base, source, data, "refused")?,
                &[]
            )
            .unwrap_err()
            .is::<SourceBoundary>()
        );
    }
    let mut crossed = layout.clone();
    crossed["managed_catalog_origin"]["source"] = serde_json::to_value(base.join("other-payload"))?;
    std::fs::write(&state_receipt, serde_json::to_vec(&crossed)?)?;
    assert!(
        private_paths(&registry.connection, &control).is_err(),
        "mismatched paired origins accepted"
    );
    crossed = layout.clone();
    crossed["managed_catalog_origin"]["namespace"] =
        serde_json::to_value(Incarnation::from_bytes([8; 16]))?;
    crossed["managed_catalog_origin"]["operation"] =
        crossed["managed_catalog_origin"]["namespace"].clone();
    save(&crossed)?;
    assert!(
        private_paths(&registry.connection, &control).is_err(),
        "foreign native namespace accepted"
    );
    crossed = layout.clone();
    crossed["managed_catalog_origin"]["claimed_authority"] = true.into();
    save(&crossed)?;
    assert!(
        private_paths(&registry.connection, &control).is_err(),
        "unknown origin fields accepted"
    );
    save(&layout)?;
    assert!(private_paths(&registry.connection, &control)?.contains(&base.join("payload")));
    assert!(!base.join("source").exists());
    assert!(!base.join("data").exists());
    Ok(())
}

#[test]
fn registry_rejects_control_and_cross_project_data_in_both_registration_orders() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let runtime = base.join("runtime");
    std::fs::create_dir(&runtime)?;
    let mut registry = ServiceRegistry::open(&runtime, Incarnation::from_bytes([1; 16]))?;
    for (source, data) in [
        ("runtime", "data"),
        ("runtime/child", "data"),
        (".", "data"),
        ("source", "runtime"),
        ("source", "source/derived"),
        ("source/nested", "source"),
    ] {
        let candidate = project(&base, source, data, "refused")?;
        assert!(
            registry
                .register(&candidate)
                .err()
                .unwrap()
                .is::<SourceBoundary>()
        );
    }
    let first = project(&base, "source-one", "shared-data", "one")?;
    registry.register(&first)?;
    let second = project(&base, "source-two", "shared-data", "two")?;
    registry.register(&second)?;
    for (source, data) in [
        ("shared-data/collections-1.4/one", "other-data"),
        ("shared-data", "other-data"),
        ("source-three", "source-one/cache"),
        ("source-three", "source-one"),
    ] {
        let candidate = project(&base, source, data, "refused")?;
        assert!(
            registry
                .register(&candidate)
                .err()
                .unwrap()
                .is::<SourceBoundary>()
        );
    }
    assert_eq!(registry.list(None, None, 32)?.projects.len(), 2);
    assert!(
        !base.join("source-one").exists(),
        "metadata validation opened or created a source"
    );
    assert!(!base.join("shared-data").exists());
    Ok(())
}

#[test]
fn additional_roots_and_retained_unknown_roots_protect_future_data_registration() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let runtime = base.join("runtime");
    std::fs::create_dir(&runtime)?;
    let mut registry = ServiceRegistry::open(&runtime, Incarnation::from_bytes([2; 16]))?;
    let first = project(&base, "source-one", "data-one", "one")?;
    let second = project(&base, "source-two", "data-two", "two")?;
    registry.register(&first)?;
    registry.register(&second)?;
    let mut lease = ProjectLease::acquire(&runtime, first.binding)?;
    registry.admit_worker(&mut lease)?;
    let extra = base.join("additional").to_str().unwrap().to_owned();
    registry.record_watch_roots(&lease, std::slice::from_ref(&extra))?;
    let before = registry.watch(first.binding)?;
    for root in [
        base.join("data-two"),
        base.join("data-two/nested"),
        runtime.clone(),
        base.clone(),
    ] {
        assert!(
            registry
                .record_watch_roots(&lease, &[root.to_str().unwrap().into()])
                .err()
                .unwrap()
                .is::<SourceBoundary>()
        );
        assert_eq!(
            registry.watch(first.binding)?.configuration_epoch,
            before.configuration_epoch
        );
        assert_eq!(
            registry.watch(first.binding)?.additional_roots,
            vec![extra.clone()]
        );
    }
    registry.invalidate_watch_roots(&lease)?;
    let candidate = project(&base, "source-three", "additional/derived", "three")?;
    assert!(
        registry
            .register(&candidate)
            .err()
            .unwrap()
            .is::<SourceBoundary>()
    );
    assert_eq!(registry.list(None, None, 32)?.projects.len(), 2);
    Ok(())
}

#[test]
fn racing_data_registration_and_root_update_cannot_both_commit() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let runtime = base.join("runtime");
    std::fs::create_dir(&runtime)?;
    let instance = Incarnation::from_bytes([3; 16]);
    let mut registry = ServiceRegistry::open(&runtime, instance)?;
    let first = project(&base, "source-one", "data-one", "one")?;
    registry.register(&first)?;
    let mut lease = ProjectLease::acquire(&runtime, first.binding)?;
    registry.admit_worker(&mut lease)?;
    let mut competitor = ServiceRegistry::open(&runtime, instance)?;
    let candidate = project(&base, "source-two", "contested", "two")?;
    let extra = base.join("contested/nested").to_str().unwrap().to_owned();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let other = barrier.clone();
    let writer = std::thread::spawn(move || {
        other.wait();
        registry.record_watch_roots(&lease, &[extra]).map(|_| ())
    });
    barrier.wait();
    let registered = competitor.register(&candidate);
    let recorded = writer.join().unwrap();
    assert_ne!(registered.is_ok(), recorded.is_ok());
    assert!(
        registered
            .err()
            .or_else(|| recorded.err())
            .unwrap()
            .is::<SourceBoundary>()
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_unicode_case_aliases_cannot_cross_the_data_boundary() -> Result<()> {
    assert!(!component_equal(
        std::ffi::OsStr::new("café"),
        std::ffi::OsStr::new("cafe")
    )?);
    assert!(component_equal(
        std::ffi::OsStr::new("CAFÉ"),
        std::ffi::OsStr::new("café")
    )?);
    assert!(
        disjoint(
            Path::new(r"C:\PRIVATE\CAFÉ"),
            Path::new(r"c:\private\café\collection")
        )
        .is_err()
    );
    assert!(
        disjoint(
            Path::new(r"C:\private\data"),
            Path::new(r"C:\private\database")
        )
        .is_ok()
    );
    Ok(())
}
