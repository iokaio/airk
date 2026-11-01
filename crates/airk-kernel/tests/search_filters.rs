// SPDX-License-Identifier: Apache-2.0
//! Filters select authenticated metadata before lexical/vector candidate limits.
use airk_core::{EvidenceKind, SearchFilters, SearchQuery};
use airk_kernel::{Kernel, current::CurrentSearchMode};
use anyhow::Result;
use std::{fs, path::Path};

fn tempdir() -> Result<tempfile::TempDir> {
    Ok(tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?)
}

fn model(directory: &Path) -> Result<()> {
    use sha2::{Digest, Sha256};
    fs::create_dir_all(directory)?;
    let vocabulary = serde_json::to_vec(&serde_json::json!({"authenticate":[1.0,0.0]}))?;
    fs::write(directory.join("vocabulary.json"), &vocabulary)?;
    fs::write(
        directory.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1,"name":"original-filter-fixture","license":"Apache-2.0",
            "dimensions":2,"sha256":format!("{:x}", Sha256::digest(&vocabulary))
        }))?,
    )?;
    Ok(())
}

fn query(limit: usize) -> SearchQuery {
    SearchQuery {
        text: "authenticate".into(),
        path_prefix: None,
        limit,
        max_bytes: 4096,
    }
}

#[test]
fn metadata_filtering_precedes_both_branch_candidate_limits() -> Result<()> {
    let root = tempdir()?;
    let data = tempdir()?;
    model(&data.path().join("models/local"))?;
    fs::create_dir(root.path().join("docs"))?;
    fs::create_dir(root.path().join("src"))?;
    let mut ids = Vec::new();
    for n in 0..150 {
        let path = format!("docs/guide_{n:03}.md");
        ids.push(airk_store::current::lexical::stable_id(&path, 0)?);
        fs::write(root.path().join(path), "# authenticate\n")?;
    }
    ids.sort();
    // Ensure this code hit lies beyond the old lexical top-100, not just top-1.
    let code = (0..10_000)
        .map(|n| format!("src/code_{n}.rs"))
        .find(|path| airk_store::current::lexical::stable_id(path, 0).unwrap() > ids[129])
        .expect("a deterministic code path after at least 130 document IDs");
    fs::write(root.path().join(&code), "pub fn authenticate() {}\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let old = kernel.search(&query(1))?;
    assert!(old.hits[0].citation.path.starts_with("docs/"));
    let filters = SearchFilters {
        language: Some("rust".into()),
        evidence_kind: Some(EvidenceKind::Code),
        ..Default::default()
    };
    for mode in [
        CurrentSearchMode::Lexical,
        CurrentSearchMode::Vector,
        CurrentSearchMode::Hybrid,
        CurrentSearchMode::Default,
    ] {
        let found = kernel.search_response_with_filters(&query(1), mode, None, &filters)?;
        assert_eq!(found.hits.len(), 1, "{}", serde_json::to_string(&found)?);
        assert_eq!(found.hits[0].citation.path, code);
        assert_eq!(
            found.publication.as_ref().unwrap().token,
            old.publication.as_ref().unwrap().token
        );
        assert!(found.hits[0].text.contains("authenticate"));
    }
    Ok(())
}

#[test]
fn combined_filters_use_metadata_and_preserve_empty_and_truncated_outcomes() -> Result<()> {
    let root = tempdir()?;
    let data = tempdir()?;
    fs::create_dir(root.path().join("source"))?;
    fs::create_dir(root.path().join("source/generated"))?;
    for (path, text) in [
        ("source/plain.rs", "pub fn authenticate() {}\n"),
        ("source/generated/auto.rs", "pub fn authenticate() {}\n"),
        ("guide.md", "# authenticate\n"),
        ("policy.toml", "name = 'authenticate'\n"),
        ("unclassified.txt", "authenticate\n"),
    ] {
        fs::write(root.path().join(path), text)?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let filters = SearchFilters {
        language: Some("rust".into()),
        evidence_kind: Some(EvidenceKind::Code),
        exclude_path_prefix: Some("source/generated/".into()),
    };
    let found = kernel.search_response_with_filters(
        &query(10),
        CurrentSearchMode::Lexical,
        None,
        &filters,
    )?;
    assert_eq!(found.hits.len(), 1);
    assert_eq!(found.hits[0].citation.path, "source/plain.rs");
    for (kind, expected) in [
        (EvidenceKind::Document, "guide.md"),
        (EvidenceKind::Configuration, "policy.toml"),
        (EvidenceKind::Unknown, "unclassified.txt"),
    ] {
        let found = kernel.search_response_with_filters(
            &query(10),
            CurrentSearchMode::Lexical,
            None,
            &SearchFilters {
                evidence_kind: Some(kind),
                ..Default::default()
            },
        )?;
        assert_eq!(found.hits.len(), 1, "{}", serde_json::to_string(&found)?);
        assert_eq!(found.hits[0].citation.path, expected);
    }
    for filters in [
        SearchFilters {
            language: Some("not-an-indexed-language".into()),
            ..Default::default()
        },
        SearchFilters {
            language: Some("rust".into()),
            evidence_kind: Some(EvidenceKind::Document),
            ..Default::default()
        },
    ] {
        let empty = kernel.search_response_with_filters(
            &query(10),
            CurrentSearchMode::Lexical,
            None,
            &filters,
        )?;
        assert!(empty.hits.is_empty());
        assert!(!empty.truncated);
    }
    let tiny = SearchQuery {
        max_bytes: 1,
        ..query(10)
    };
    let tiny =
        kernel.search_response_with_filters(&tiny, CurrentSearchMode::Lexical, None, &filters)?;
    assert_eq!(tiny.hits.len(), 1);
    assert!(tiny.truncated);
    assert_eq!(tiny.hits[0].text.len(), 1);
    Ok(())
}

#[test]
fn default_filtering_preserves_wire_evidence_and_explicit_mode_policy() -> Result<()> {
    let root = tempdir()?;
    let data = tempdir()?;
    fs::write(root.path().join("one.rs"), "fn authenticate() {}\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let old = kernel.search_response(&query(10), CurrentSearchMode::Lexical, Some(768))?;
    let new = kernel.search_response_with_filters(
        &query(10),
        CurrentSearchMode::Lexical,
        Some(768),
        &SearchFilters::default(),
    )?;
    assert_eq!(serde_json::to_value(old)?, serde_json::to_value(new)?);
    // A filter cannot authorize hidden lexical fallback for explicit vectors.
    assert!(
        kernel
            .search_response_with_filters(
                &query(10),
                CurrentSearchMode::Vector,
                None,
                &SearchFilters {
                    language: Some("absent".into()),
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert!(
        kernel
            .search_response_with_filters(
                &query(10),
                CurrentSearchMode::Lexical,
                None,
                &SearchFilters {
                    exclude_path_prefix: Some(String::new()),
                    ..Default::default()
                }
            )
            .is_err()
    );
    Ok(())
}

#[test]
fn definition_navigation_is_same_revision_bounded_and_does_not_rewrite_query() -> Result<()> {
    let root = tempdir()?;
    let data = tempdir()?;
    fs::create_dir(root.path().join("src"))?;
    fs::write(root.path().join("src/one.rs"), "pub struct Session;\n")?;
    fs::write(root.path().join("src/two.rs"), "pub struct Session;\n")?;
    fs::write(
        root.path().join("guide.md"),
        "# Session\nHow the code uses Session.\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let ask = SearchQuery {
        text: "how the code uses Session".into(),
        ..query(10)
    };
    let old = kernel.search_response(&ask, CurrentSearchMode::Lexical, None)?;
    let found = kernel.search_response_with_navigation(
        &ask,
        CurrentSearchMode::Lexical,
        None,
        &SearchFilters::default(),
    )?;
    assert_eq!(
        serde_json::to_value(&old)?,
        serde_json::to_value(&found.evidence)?
    );
    assert_eq!(found.definitions.len(), 1);
    let hint = &found.definitions[0];
    assert_eq!(hint.name, "Session");
    assert!(hint.complete);
    assert_eq!(hint.total_matches, Some(2));
    assert_eq!(hint.lower_bound, 2);
    assert_eq!(hint.matches.len(), 2);
    assert!(
        hint.matches
            .iter()
            .all(|definition| definition.citation.path.starts_with("src/"))
    );
    assert_eq!(hint.evidence_scope, "native_definition_records");

    let excluded = kernel.search_response_with_navigation(
        &ask,
        CurrentSearchMode::Lexical,
        None,
        &SearchFilters {
            exclude_path_prefix: Some("src/".into()),
            ..Default::default()
        },
    )?;
    assert!(excluded.definitions.is_empty());
    let missing = kernel.search_response_with_navigation(
        &query(10),
        CurrentSearchMode::Lexical,
        None,
        &SearchFilters::default(),
    )?;
    assert!(
        !serde_json::to_value(missing)?
            .as_object()
            .unwrap()
            .contains_key("definitions")
    );
    Ok(())
}

#[test]
fn definition_navigation_reports_a_lower_bound_when_the_roster_is_larger() -> Result<()> {
    let root = tempdir()?;
    let data = tempdir()?;
    for n in 0..8 {
        fs::write(
            root.path().join(format!("session_{n}.rs")),
            "pub struct Session;\n",
        )?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let found = kernel.search_response_with_navigation(
        &SearchQuery {
            text: "Session".into(),
            ..query(1)
        },
        CurrentSearchMode::Lexical,
        None,
        &SearchFilters::default(),
    )?;
    let hint = &found.definitions[0];
    assert!(!hint.complete);
    assert_eq!(hint.total_matches, None);
    assert_eq!(hint.lower_bound, 4);
    assert_eq!(hint.matches.len(), 4);
    assert_eq!(found.evidence.hits.len(), 1);
    assert!(serde_json::to_vec(&found.definitions)?.len() <= 4096);
    Ok(())
}
