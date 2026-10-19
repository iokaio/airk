// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    bounded::Charge,
    current_lexical::{CurrentLexical, LexicalChange},
};
use airk_core::{Artifact, Chunk, DocumentLocation};
use std::{
    fs,
    io::{Seek, SeekFrom, Write},
    sync::Arc,
};

fn artifact(path: &str, text: &str) -> Artifact {
    Artifact {
        path: path.into(),
        content_hash: Sha256Digest::from_bytes([7; 32]).to_string(),
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
fn active_files_are_bound_across_changed_path_commits_and_reopen() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("lexical");
    let charge: Charge = Arc::new(|_| Ok(()));
    let lexical = CurrentLexical::create(&path, charge.clone())?;
    let initial = lexical.capture_manifest(None)?;
    initial.verify(&path, initial.digest()?)?;
    let a = artifact("docs/a.md", "alpha");
    lexical.apply_changes(&[LexicalChange {
        path: &a.path,
        artifact: Some(&a),
    }])?;
    let first = lexical.capture_manifest(Some((&initial, initial.digest()?)))?;
    let first_digest = first.digest()?;
    first.verify(&path, first_digest)?;
    assert_eq!(
        LexicalManifest::decode(&first.canonical_bytes()?, first_digest)?,
        first
    );
    let b = artifact("docs/b.md", "beta");
    lexical.apply_changes(&[LexicalChange {
        path: &b.path,
        artifact: Some(&b),
    }])?;
    let second = lexical.capture_manifest(Some((&first, first_digest)))?;
    assert_ne!(second.digest()?, first_digest);
    second.verify(&path, second.digest()?)?;
    assert!(first.verify(&path, first_digest).is_err());
    drop(lexical);
    let reopened = CurrentLexical::open(&path, charge)?;
    assert_eq!(reopened.capture_manifest(None)?, second);
    Ok(())
}

#[test]
fn same_size_corruption_and_missing_active_file_fail_verification() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("lexical");
    let lexical = CurrentLexical::create(&path, Arc::new(|_| Ok(())))?;
    let a = artifact("docs/a.md", "alpha");
    lexical.apply_changes(&[LexicalChange {
        path: &a.path,
        artifact: Some(&a),
    }])?;
    let manifest = lexical.capture_manifest(None)?;
    let digest = manifest.digest()?;
    assert!(
        manifest
            .verify(&path, Sha256Digest::from_bytes([1; 32]))
            .is_err()
    );
    drop(lexical);
    let meta = path.join("meta.json");
    let original_meta = fs::read(&meta)?;
    let mut file = fs::OpenOptions::new().write(true).open(&meta)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(b"X")?;
    file.sync_all()?;
    drop(file);
    assert!(manifest.verify(&path, digest).is_err());
    fs::write(&meta, original_meta)?;
    manifest.verify(&path, digest)?;
    let segment = manifest
        .files
        .iter()
        .find(|file| file.name != "meta.json")
        .context("no active segment")?;
    let segment_path = path.join(&segment.name);
    let original_segment = fs::read(&segment_path)?;
    ensure!(!original_segment.is_empty(), "empty test segment");
    let mut file = fs::OpenOptions::new().write(true).open(&segment_path)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(b"X")?;
    file.sync_all()?;
    drop(file);
    assert!(manifest.verify(&path, digest).is_err());
    fs::write(&segment_path, original_segment)?;
    manifest.verify(&path, digest)?;
    fs::remove_file(segment_path)?;
    assert!(manifest.verify(&path, digest).is_err());
    Ok(())
}
