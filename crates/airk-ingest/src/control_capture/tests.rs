// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::Sha256Digest,
    source_control::{ControlInputState, SourceControlBaseline},
};
use std::collections::BTreeMap;

#[test]
fn control_system_alias_requires_exact_fixed_target_and_root_security() -> Result<()> {
    decision::directory(0, 0o040755)?;
    decision::alias(0, 0o120755, 1, b"private/etc")?;
    for (uid, mode) in [(501, 0o040755), (0, 0o040775), (0, 0o100755), (0, 0o120755)] {
        assert!(decision::directory(uid, mode).is_err());
    }
    for target in [
        b"/private/etc".as_slice(),
        b"private/../etc",
        b"private/etc/",
        b"private/other",
        b"",
    ] {
        assert!(decision::alias(0, 0o120755, 1, target).is_err());
    }
    for (uid, mode, links) in [
        (501, 0o120755, 1),
        (0, 0o120777, 1),
        (0, 0o040755, 1),
        (0, 0o120755, 2),
    ] {
        assert!(decision::alias(uid, mode, links, b"private/etc").is_err());
    }
    Ok(())
}
#[test]
fn control_system_acl_requires_successful_known_absence_not_errno_guessing() -> Result<()> {
    decision::no_acl(0, 0)?;
    for (status, present) in [(-1, 0), (1, 0), (0, -1), (0, 1), (0, 32)] {
        assert!(decision::no_acl(status, present).is_err());
    }
    Ok(())
}
#[test]
fn control_capture_retains_complete_original_baseline_and_drift_latch() -> Result<()> {
    let owned = tempfile::tempdir()?;
    let root = owned.path().canonicalize()?;
    let file = root.join("control");
    let absent = root.join("absent");
    std::fs::write(&file, b"ignored\n")?;
    let snapshot = crate::control_snapshot::ControlSnapshot::capture();
    snapshot.record_environment(&root)?;
    let selected = snapshot.capture_path(&file)?;
    assert_eq!(selected.physical(), file);
    let bytes = selected.read(65536, 65537)?;
    assert_eq!(bytes, b"ignored\n");
    snapshot.file(&file, &bytes)?;
    snapshot.absent_qualified(&absent)?;
    snapshot.directory_qualified(&root)?;
    let configuration = Sha256Digest::from_bytes([2; 32]);
    let settings = Sha256Digest::from_bytes([3; 32]);
    let baseline = snapshot.seal_complete(configuration, settings)?;
    let expected = SourceControlBaseline {
        schema: 1,
        publication: None,
        configuration,
        settings,
        contexts: baseline.contexts.clone(),
        inputs: BTreeMap::from([
            (
                file.to_str().unwrap().to_owned(),
                ControlInputState::File {
                    sha256: serde_json::from_value(serde_json::json!(
                        "a6c7e3c5ddd823efc8b7a30853eabc152dac866843bd7a39dc4a29e0a0eb68b3"
                    ))?,
                    bytes: 8,
                },
            ),
            (
                absent.to_str().unwrap().to_owned(),
                ControlInputState::Absent,
            ),
            (
                root.to_str().unwrap().to_owned(),
                ControlInputState::Directory,
            ),
        ]),
    };
    assert_eq!(
        serde_json::to_vec(&baseline)?,
        serde_json::to_vec(&expected)?
    );
    let retained = crate::control_snapshot::ControlSnapshot::revalidate(baseline.clone())?;
    retained.revalidate_inputs()?;
    assert_eq!(
        serde_json::to_vec(&retained.seal_complete(configuration, settings)?)?,
        serde_json::to_vec(&expected)?
    );
    assert_eq!(
        retained
            .seal_complete(configuration, settings)?
            .identity()?,
        baseline.identity()?
    );
    std::fs::write(&file, b"changed\n")?;
    assert!(retained.revalidate_inputs().is_err());
    std::fs::write(&file, b"ignored\n")?;
    assert!(
        retained.revalidate_inputs().is_err(),
        "restoration cannot clear captured failure"
    );
    Ok(())
}
#[cfg(unix)]
#[test]
fn ordinary_control_and_source_suffix_aliases_remain_refused() -> Result<()> {
    let owned = tempfile::tempdir()?;
    let root = owned.path().canonicalize()?;
    let file = root.join("control");
    let alias = root.join("alias");
    std::fs::write(&file, b"safe\n")?;
    std::os::unix::fs::symlink(&file, &alias)?;
    let selected = Owner::default().path(&alias)?;
    assert!(selected.read(64, 65).is_err());
    assert!(crate::source_read::read(&root, &alias, 64, 65).is_err());
    Ok(())
}

#[test]
fn control_system_refusal_latches_without_observing_restored_authority() -> Result<()> {
    use std::{cell::Cell, sync::atomic::AtomicBool};
    let failed = AtomicBool::new(false);
    let observations = Cell::new(0);
    decision::observe_once(&failed, || {
        observations.set(1);
        Ok(())
    })?;
    assert!(
        decision::observe_once(&failed, || {
            observations.set(2);
            anyhow::bail!("actual identity changed")
        })
        .is_err()
    );
    assert!(
        decision::observe_once(&failed, || {
            observations.set(3);
            Ok(())
        })
        .is_err()
    );
    assert_eq!(
        observations.get(),
        2,
        "same owner must not accept a restored or newly observed boundary"
    );
    decision::observe_once(&AtomicBool::new(false), || Ok(()))?;
    Ok(())
}
