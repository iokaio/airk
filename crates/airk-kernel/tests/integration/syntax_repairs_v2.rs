// SPDX-License-Identifier: Apache-2.0
//! Activate after PHP/Ruby repairs and explicit SQL profiles are wired.
use super::current_inspection::InspectCurrent;
use airk_core::{EvidenceBasis, FactQuery};
use airk_ingest::adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::fs;

fn qualify(
    path: &str,
    moved: &str,
    text: &str,
    profile: Option<&str>,
    expected: &[(&str, &str)],
) -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
    fs::write(root.path().join(path), &text)?;
    fs::write(
        root.path().join("unaffected.rs"),
        "pub fn unaffected() -> u32 { 9 }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    if let Some(adapter) = profile {
        kernel.configure_extraction_policy(ExtractionPolicy {
            detail: None,
            version: 1,
            rules: vec![SelectionRule {
                selector: RuleMatch::Scoped {
                    prefix: String::new(),
                    suffix: Some(".sql".into()),
                    basename: None,
                },
                case_sensitive: true,
                adapter_id: adapter.into(),
            }],
        })?;
    }
    let cold = kernel.index()?;
    assert_eq!(cold.indexed, 2);
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let before = store.current_test_catalog()?.context("catalog")?;
    assert_eq!(
        store.read_retained_source(&before.snapshot.artifacts[path])?,
        text.as_bytes()
    );
    let facts = kernel.outline(path, 100, 65536)?;
    for (kind, name) in expected {
        assert!(
            facts
                .facts
                .iter()
                .any(|fact| fact.kind == *kind && fact.name.contains(name)),
            "missing {kind} {name}"
        );
    }
    for fact in facts.facts.iter().filter(|fact| {
        fact.code
            .as_ref()
            .is_some_and(|code| code.producer_version == "repair-facts/1")
    }) {
        let code = fact.code.as_ref().unwrap();
        let span = code.span.as_ref().context("repair source span")?;
        assert!(text[span.start_byte..span.end_byte].contains(&fact.name));
        assert_eq!(
            fact.citation.start_line,
            text[..span.start_byte]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1
        );
        assert_eq!(
            code.basis,
            if fact.kind == "procedure" {
                EvidenceBasis::StructuralRule
            } else {
                EvidenceBasis::Syntax
            }
        );
        assert!(code.fingerprints.is_none());
    }
    let bounded = kernel.facts(&FactQuery {
        path_prefix: path.into(),
        name: None,
        kind: None,
        language: None,
        exact_name: false,
        limit: 1,
        max_bytes: 4096,
        path_exact: true,
        document_only: false,
    })?;
    assert!(bounded.facts.len() <= 1 && bounded.truncated);
    let warm = kernel.index()?;
    assert_eq!((warm.indexed, warm.reused), (0, 2));
    fs::rename(root.path().join(path), root.path().join(moved))?;
    let renamed = kernel.index()?;
    assert_eq!((renamed.indexed, renamed.reused), (0, 2));
    assert_eq!(
        before.inventory.as_ref().unwrap().files[path]
            .state
            .payload_digest,
        store
            .current_test_catalog()?
            .unwrap()
            .inventory
            .as_ref()
            .unwrap()
            .files[moved]
            .state
            .payload_digest
    );
    kernel.configure_extraction_policy(ExtractionPolicy {
        detail: None,
        version: 1,
        rules: vec![SelectionRule {
            selector: RuleMatch::Exact { path: moved.into() },
            case_sensitive: true,
            adapter_id: "text".into(),
        }],
    })?;
    let changed = kernel.index()?;
    assert_eq!((changed.indexed, changed.reused), (1, 1));
    Ok(())
}

#[test]
fn php_receivers_namespaces_and_dynamic_calls_survive_reuse() -> Result<()> {
    qualify(
        "sample.php",
        "moved.php",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../airk-ingest/tests/fixtures/syntax-repairs/sample.php"
        )),
        None,
        &[
            ("namespace", "Fictional\\Archive"),
            ("call", "$reader?->close"),
            ("call", "LocalReader::create"),
            ("dynamic_call", "$reader->$method"),
        ],
    )
}

#[test]
fn ruby_literals_mixins_and_written_receivers_survive_reuse() -> Result<()> {
    qualify(
        "sample.rb",
        "moved.rb",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../airk-ingest/tests/fixtures/syntax-repairs/sample.rb"
        )),
        None,
        &[
            ("import", "fictional/archive"),
            ("mixin", "Named"),
            ("call", "reader&.close"),
            ("dynamic_call", "self.send"),
        ],
    )
}

#[test]
fn postgres_explicit_profile_preserves_ctes_and_unresolved_references() -> Result<()> {
    qualify(
        "sample.sql",
        "moved.sql",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../airk-ingest/tests/fixtures/syntax-repairs/postgresql.sql"
        )),
        Some("sql-postgresql"),
        &[
            ("cte", "recent"),
            ("alias", "entry_id"),
            ("table_reference", "archive.entries"),
        ],
    )
}

#[test]
fn mysql_explicit_profile_preserves_quoted_names() -> Result<()> {
    qualify(
        "sample.sql",
        "moved.sql",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../airk-ingest/tests/fixtures/syntax-repairs/mysql.sql"
        )),
        Some("sql-mysql"),
        &[("alias", "entry_id"), ("table_reference", "`entries`")],
    )
}
