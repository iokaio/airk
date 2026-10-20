// SPDX-License-Identifier: Apache-2.0
//! Activate only after vendor dependency/profile admission.
use super::current_inspection::InspectCurrent;
use airk_ingest::adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::fs;

fn qualify(adapter: &str, path: &str, moved: &str, source: &str, expected: &str) -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let source = source.replace("\r\n", "\n").replace('\n', "\r\n");
    fs::write(root.path().join(path), &source)?;
    fs::write(
        root.path().join("unrelated.rs"),
        "pub fn stable() -> u32 { 17 }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    if adapter == "tsql" {
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
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let catalog = store.current_test_catalog()?.context("catalog")?;
    let inventory = catalog.inventory.as_ref().unwrap();
    let before = inventory.files[path].state.clone();
    let unrelated = inventory.files["unrelated.rs"].state.clone();
    assert_eq!(inventory.profiles[&before.profile_id].adapter_id, adapter);
    assert_eq!(
        store.read_retained_source(&catalog.snapshot.artifacts[path])?,
        source.as_bytes()
    );
    let facts = kernel.outline(path, 100, 65536)?;
    let found = facts
        .facts
        .iter()
        .find(|f| f.name == expected)
        .context("vendor declaration")?;
    let key = if adapter == "apex" {
        Some(found.continuity_key.context("native Apex continuity")?)
    } else {
        None
    };
    assert_eq!(kernel.outline(path, 1, 4096)?.facts.len(), 1);
    let warm = kernel.index()?;
    assert_eq!((warm.indexed, warm.reused), (0, 2));
    fs::rename(root.path().join(path), root.path().join(moved))?;
    let report = kernel.index()?;
    assert_eq!((report.indexed, report.reused), (0, 2));
    let moved_catalog = store.current_test_catalog()?.unwrap();
    assert_eq!(
        moved_catalog.inventory.as_ref().unwrap().files[moved]
            .state
            .payload_digest,
        before.payload_digest
    );
    if let Some(key) = key {
        assert_eq!(
            kernel.continuity_facts(key, 1, 4096)?.facts[0]
                .citation
                .path,
            moved
        );
    }
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
    let after = store.current_test_catalog()?.unwrap();
    assert_ne!(
        after.inventory.as_ref().unwrap().files[moved]
            .state
            .profile_id,
        before.profile_id
    );
    assert_eq!(
        after.inventory.as_ref().unwrap().files["unrelated.rs"].state,
        unrelated
    );
    assert_eq!(fs::read_to_string(root.path().join(moved))?, source);
    Ok(())
}

macro_rules! check {
    ($test:ident,$adapter:literal,$path:literal,$moved:literal,$name:literal) => {
        #[test]
        fn $test() -> Result<()> {
            qualify(
                $adapter,
                $path,
                $moved,
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../airk-ingest/tests/fixtures/specialist-vendor/",
                    $path
                )),
                $name,
            )
        }
    };
}
check!(plsql, "plsql", "original.pkb", "moved.pkb", "archive_tools");
check!(
    tsql,
    "tsql",
    "tsql.sql",
    "moved.sql",
    "[archive].[list_items]"
);
check!(
    abap,
    "abap",
    "original.abap",
    "moved.abap",
    "z_archive_report"
);
check!(apex, "apex", "original.cls", "moved.cls", "ArchiveService");

#[test]
fn sql_dialect_is_explicit_and_visual_basic_cls_does_not_select_apex() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("query.sql"),
        "SELECT id FROM archive_items;\n",
    )?;
    fs::write(
        root.path().join("legacy.cls"),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../airk-ingest/tests/fixtures/specialist-vendor/ambiguous.cls"
        )),
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let catalog = store.current_test_catalog()?.unwrap();
    let inventory = catalog.inventory.as_ref().unwrap();
    assert_eq!(
        inventory.profiles[&inventory.files["query.sql"].state.profile_id].adapter_id,
        "sql"
    );
    assert_eq!(
        inventory.profiles[&inventory.files["legacy.cls"].state.profile_id].adapter_id,
        "text"
    );
    Ok(())
}
