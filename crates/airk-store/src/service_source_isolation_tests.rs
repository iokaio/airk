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

#[cfg(windows)]
#[test]
fn windows_verbatim_prefixes_preserve_drive_and_unc_share_isolation() -> Result<()> {
    for (plain, canonical) in [
        (r"C:\private\café", r"\\?\c:\PRIVATE\CAFÉ\collection"),
        (
            r"\\server\share\private",
            r"\\?\UNC\SERVER\SHARE\private\collection",
        ),
    ] {
        assert!(disjoint(Path::new(plain), Path::new(canonical)).is_err());
        assert!(disjoint(Path::new(canonical), Path::new(plain)).is_err());
    }
    for (left, right) in [
        (r"C:\private", r"\\?\D:\private\collection"),
        (
            r"\\server\share\private",
            r"\\?\UNC\server\other-share\private",
        ),
        (
            r"\\server\share\private",
            r"\\?\UNC\other-server\share\private",
        ),
    ] {
        disjoint(Path::new(left), Path::new(right))?;
    }
    Ok(())
}
