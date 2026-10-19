// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{Artifact, Chunk, DocumentLocation, identity::Sha256Digest};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn artifact(path: &str, hash: u8, text: &str) -> Artifact {
    Artifact {
        path: path.into(),
        content_hash: Sha256Digest::from_bytes([hash; 32]).to_string(),
        bytes: text.len(),
        chunks: vec![Chunk {
            start_line: 1,
            end_line: 1,
            text: text.into(),
            location: DocumentLocation::Lines { start: 1, end: 1 },
        }],
        language: "text".into(),
        extraction_method: "test".into(),
        parse_status: "ok".into(),
        facts: vec![],
        retained: None,
    }
}

#[test]
fn private_fork_changes_one_path_and_preserves_verified_base() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let base_path = directory.path().join("base");
    let target_path = directory.path().join("next");
    let charge: Charge = Arc::new(|_| Ok(()));
    let base = CurrentLexical::create(&base_path, charge.clone())?;
    let a = artifact("docs/a.md", 1, "old alpha");
    let b = artifact("docs/b.md", 2, "stable beta");
    base.apply_changes(&[
        LexicalChange {
            path: &a.path,
            artifact: Some(&a),
        },
        LexicalChange {
            path: &b.path,
            artifact: Some(&b),
        },
    ])?;
    let old_manifest = base.capture_manifest(None)?;
    let verified = old_manifest.verify_owned(&base_path, old_manifest.digest()?)?;
    let original_b = serde_json::to_value(base.path_documents(&b.path)?)?;
    drop(base);
    let changed = artifact("docs/a.md", 3, "new gamma");
    let (next, next_manifest, work) = CurrentLexical::fork_changed(
        &verified,
        &target_path,
        charge.clone(),
        &[LexicalChange {
            path: &changed.path,
            artifact: Some(&changed),
        }],
    )?;
    assert_eq!(work.changed.changed_artifacts, 1);
    assert!(work.hardlinked_files > 0 && work.hardlinked_bytes > 0);
    assert!(work.admitted_clone_bytes >= work.hardlinked_bytes);
    assert_eq!(
        serde_json::to_value(next.path_documents(&b.path)?)?,
        original_b
    );
    assert_eq!(next.path_documents(&changed.path)?[0].1.text, "new gamma");
    next_manifest
        .manifest()
        .verify(&target_path, next_manifest.digest())?;
    drop(next);
    let old = CurrentLexical::open(&base_path, charge)?;
    assert_eq!(old.path_documents(&a.path)?[0].1.text, "old alpha");
    assert_eq!(
        serde_json::to_value(old.path_documents(&b.path)?)?,
        original_b
    );
    old_manifest.verify(&base_path, old_manifest.digest()?)?;
    Ok(())
}

#[test]
fn fork_admission_denial_never_creates_target_or_changes_base() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let base_path = directory.path().join("base");
    let target_path = directory.path().join("next");
    let base = CurrentLexical::create(&base_path, Arc::new(|_| Ok(())))?;
    let a = artifact("docs/a.md", 1, "old alpha");
    base.apply_changes(&[LexicalChange {
        path: &a.path,
        artifact: Some(&a),
    }])?;
    let manifest = base.capture_manifest(None)?;
    let verified = manifest.verify_owned(&base_path, manifest.digest()?)?;
    drop(base);
    let changed = artifact("docs/a.md", 2, "new beta");
    assert!(
        CurrentLexical::fork_changed(
            &verified,
            &target_path,
            Arc::new(|_| anyhow::bail!("quota denied")),
            &[LexicalChange {
                path: &changed.path,
                artifact: Some(&changed)
            }],
        )
        .is_err()
    );
    assert!(!target_path.exists());
    manifest.verify(&base_path, manifest.digest()?)?;
    Ok(())
}

#[test]
fn fork_write_failure_cleans_private_staging_and_preserves_base() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let base_path = directory.path().join("base");
    let target_path = directory.path().join("next");
    let base = CurrentLexical::create(&base_path, Arc::new(|_| Ok(())))?;
    let a = artifact("docs/a.md", 1, "old alpha");
    base.apply_changes(&[LexicalChange {
        path: &a.path,
        artifact: Some(&a),
    }])?;
    let manifest = base.capture_manifest(None)?;
    let verified = manifest.verify_owned(&base_path, manifest.digest()?)?;
    drop(base);
    let calls = Arc::new(AtomicUsize::new(0));
    let gate = calls.clone();
    let charge: Charge = Arc::new(move |_| {
        if gate.fetch_add(1, Ordering::SeqCst) == 0 {
            Ok(())
        } else {
            anyhow::bail!("injected Tantivy write denial")
        }
    });
    let changed = artifact("docs/a.md", 2, "new beta");
    assert!(
        CurrentLexical::fork_changed(
            &verified,
            &target_path,
            charge,
            &[LexicalChange {
                path: &changed.path,
                artifact: Some(&changed)
            }],
        )
        .is_err()
    );
    assert!(calls.load(Ordering::SeqCst) > 1);
    assert!(!target_path.exists());
    manifest.verify(&base_path, manifest.digest()?)?;
    Ok(())
}
