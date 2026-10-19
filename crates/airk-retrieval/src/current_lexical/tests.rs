// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{Chunk, DocumentLocation, SearchQuery};
use std::sync::Arc;

fn artifact(path: &str, marker: u8, lines: &[&str]) -> Artifact {
    Artifact {
        path: path.into(),
        content_hash: Sha256Digest::from_bytes([marker; 32]).to_string(),
        bytes: lines.iter().map(|line| line.len()).sum(),
        chunks: lines
            .iter()
            .enumerate()
            .map(|(index, text)| Chunk {
                start_line: index + 1,
                end_line: index + 1,
                text: (*text).into(),
                location: DocumentLocation::Lines {
                    start: index + 1,
                    end: index + 1,
                },
            })
            .collect(),
        language: "text".into(),
        extraction_method: "test".into(),
        parse_status: "ok".into(),
        facts: Vec::new(),
        retained: None,
    }
}

#[test]
fn verified_reader_scopes_bounded_candidates_and_rejects_changed_files() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("lexical");
    let charge: Charge = Arc::new(|_| Ok(()));
    let lexical = CurrentLexical::create(&path, charge.clone())?;
    let a = artifact("docs/a[1].md", 1, &["shared needle"]);
    let b = artifact("docs/b.md", 2, &["shared needle"]);
    lexical.apply_changes(&[
        LexicalChange {
            path: &a.path,
            artifact: Some(&a),
        },
        LexicalChange {
            path: &b.path,
            artifact: Some(&b),
        },
    ])?;
    let manifest = lexical.capture_manifest(None)?;
    let bytes = manifest.canonical_bytes()?;
    let digest = manifest.digest()?;
    drop(lexical);
    assert!(
        VerifiedCurrentLexical::open(
            &path,
            charge.clone(),
            &bytes,
            Sha256Digest::from_bytes([9; 32]),
        )
        .is_err()
    );
    let reader = VerifiedCurrentLexical::open(&path, charge.clone(), &bytes, digest)?;
    let mut query = SearchQuery {
        text: "shared needle".into(),
        path_prefix: None,
        limit: 1,
        max_bytes: 1024,
    };
    let one = reader.search(&query)?;
    assert_eq!(one.values.len(), 1);
    assert!(one.incomplete);
    query.limit = 2;
    let both = reader.search(&query)?;
    assert_eq!(both.values.len(), 2);
    assert!(!both.incomplete);
    query.path_prefix = Some("docs/a[".into());
    let scoped = reader.search(&query)?;
    assert_eq!(scoped.values.len(), 1);
    assert_eq!(scoped.values[0].hit.citation.path, a.path);
    assert_eq!(scoped.values[0].stable_id, stable_id(&a.path, 0)?);
    assert!(!scoped.incomplete);
    drop(reader);

    let meta = path.join("meta.json");
    let mut changed = std::fs::read(&meta)?;
    changed[0] ^= 1;
    std::fs::write(meta, changed)?;
    assert!(VerifiedCurrentLexical::open(&path, charge, &bytes, digest).is_err());
    Ok(())
}

#[test]
fn changed_paths_keep_unrelated_document_ids_and_survive_reopen() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("lexical");
    let charge: Charge = Arc::new(|_| Ok(()));
    let lexical = CurrentLexical::create(&path, charge.clone())?;
    let a = artifact("src/a.txt", 1, &["first alpha"]);
    let ab = artifact("src/ab.txt", 2, &["separate beta"]);
    assert_eq!(lexical.apply_changes(&[])?, None);
    let work = lexical
        .apply_changes(&[
            LexicalChange {
                path: &a.path,
                artifact: Some(&a),
            },
            LexicalChange {
                path: &ab.path,
                artifact: Some(&ab),
            },
        ])?
        .unwrap();
    assert_eq!((work.changed_artifacts, work.added_documents), (2, 2));
    let original_ab = lexical.path_documents(&ab.path)?;
    assert_eq!(original_ab.len(), 1);
    let changed_a = artifact("src/a.txt", 3, &["new gamma", "second delta"]);
    let work = lexical
        .apply_changes(&[LexicalChange {
            path: &changed_a.path,
            artifact: Some(&changed_a),
        }])?
        .unwrap();
    assert_eq!((work.changed_artifacts, work.added_documents), (1, 2));
    assert_eq!(
        serde_json::to_value(lexical.path_documents(&ab.path)?)?,
        serde_json::to_value(&original_ab)?
    );
    let changed_docs = lexical.path_documents(&changed_a.path)?;
    assert_eq!(changed_docs.len(), 2);
    assert!(changed_docs.iter().any(|(_, hit)| hit.text == "new gamma"));
    assert!(
        changed_docs
            .iter()
            .all(|(_, hit)| hit.citation.content_hash == changed_a.content_hash)
    );
    lexical.apply_changes(&[LexicalChange {
        path: &changed_a.path,
        artifact: None,
    }])?;
    assert!(lexical.path_documents(&changed_a.path)?.is_empty());
    assert_eq!(
        serde_json::to_value(lexical.path_documents(&ab.path)?)?,
        serde_json::to_value(&original_ab)?
    );
    drop(lexical);
    let reopened = CurrentLexical::open(&path, charge)?;
    assert_eq!(
        serde_json::to_value(reopened.path_documents(&ab.path)?)?,
        serde_json::to_value(&original_ab)?
    );
    assert!(reopened.path_documents(&changed_a.path)?.is_empty());
    Ok(())
}

#[test]
fn invalid_or_duplicate_changes_do_not_mutate_existing_documents() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let lexical = CurrentLexical::create(&directory.path().join("lexical"), Arc::new(|_| Ok(())))?;
    let a = artifact("docs/a.md", 1, &["original"]);
    lexical.apply_changes(&[LexicalChange {
        path: &a.path,
        artifact: Some(&a),
    }])?;
    let original = lexical.path_documents(&a.path)?;
    let invalid = artifact("docs/b.md", 2, &["foreign"]);
    assert!(
        lexical
            .apply_changes(&[LexicalChange {
                path: &a.path,
                artifact: Some(&invalid)
            }])
            .is_err()
    );
    assert!(
        lexical
            .apply_changes(&[
                LexicalChange {
                    path: &a.path,
                    artifact: None
                },
                LexicalChange {
                    path: &a.path,
                    artifact: Some(&a)
                },
            ])
            .is_err()
    );
    assert!(
        lexical
            .apply_changes(&[LexicalChange {
                path: "../outside",
                artifact: None
            }])
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(lexical.path_documents(&a.path)?)?,
        serde_json::to_value(&original)?
    );
    Ok(())
}

#[test]
fn denied_write_does_not_publish_a_changed_path() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("lexical");
    let unlimited: Charge = Arc::new(|_| Ok(()));
    let lexical = CurrentLexical::create(&path, unlimited.clone())?;
    let original = artifact("docs/a.md", 1, &["original"]);
    lexical.apply_changes(&[LexicalChange {
        path: &original.path,
        artifact: Some(&original),
    }])?;
    let before = serde_json::to_value(lexical.path_documents(&original.path)?)?;
    drop(lexical);
    let denied = CurrentLexical::open(&path, Arc::new(|_| anyhow::bail!("quota denied")))?;
    let updated = artifact("docs/a.md", 2, &["updated"]);
    assert!(
        denied
            .apply_changes(&[LexicalChange {
                path: &updated.path,
                artifact: Some(&updated)
            }])
            .is_err()
    );
    drop(denied);
    let reopened = CurrentLexical::open(&path, unlimited)?;
    assert_eq!(
        serde_json::to_value(reopened.path_documents(&original.path)?)?,
        before
    );
    Ok(())
}
