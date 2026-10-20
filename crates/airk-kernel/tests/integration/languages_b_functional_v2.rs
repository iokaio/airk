// SPDX-License-Identifier: Apache-2.0
//! Activate only after the final P10 profile batch is admitted.
use super::current_inspection::InspectCurrent;
use airk_ingest::adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::fs;

fn qualify(path: &str, moved: &str, source: &str, expected: &str) -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let source = source.replace("\r\n", "\n").replace('\n', "\r\n");
    fs::write(root.path().join(path), &source)?;
    fs::write(
        root.path().join("unrelated.rs"),
        "pub fn unaffected() -> u32 {17}\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    let report = kernel.index()?;
    assert_eq!(report.indexed, 2, "{report:?}");
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let catalog = store.current_test_catalog()?.context("catalog")?;
    let artifact = &catalog.snapshot.artifacts[path];
    assert_eq!(store.read_retained_source(artifact)?, source.as_bytes());
    let before = catalog.inventory.as_ref().unwrap().files[path]
        .state
        .clone();
    let untouched = catalog.inventory.as_ref().unwrap().files["unrelated.rs"]
        .state
        .clone();
    let facts = kernel.outline(path, 100, 65_536)?;
    assert!(facts.facts.iter().any(|f| f.name.contains(expected)));
    if matches!(
        path.rsplit_once('.').map(|(_, s)| s),
        Some("ml" | "hs" | "erl" | "fs")
    ) {
        let key = facts
            .facts
            .iter()
            .find_map(|f| f.continuity_key)
            .context("qualified native declaration key")?;
        assert_eq!(kernel.continuity_facts(key, 1, 4096)?.facts.len(), 1);
    }
    let warm = kernel.index()?;
    assert_eq!((warm.indexed, warm.reused), (0, 2));
    fs::rename(root.path().join(path), root.path().join(moved))?;
    let moved_report = kernel.index()?;
    assert_eq!((moved_report.indexed, moved_report.reused), (0, 2));
    let after = store.current_test_catalog()?.context("moved catalog")?;
    assert_eq!(
        after.inventory.as_ref().unwrap().files[moved]
            .state
            .payload_digest,
        before.payload_digest
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
    let after = store.current_test_catalog()?.context("changed catalog")?;
    assert_ne!(
        after.inventory.as_ref().unwrap().files[moved]
            .state
            .profile_id,
        before.profile_id
    );
    assert_eq!(
        after.inventory.as_ref().unwrap().files["unrelated.rs"].state,
        untouched
    );
    Ok(())
}
macro_rules! check {
    ($name:ident,$path:literal,$moved:literal,$expected:literal) => {
        #[test]
        fn $name() -> Result<()> {
            qualify(
                $path,
                $moved,
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../airk-ingest/tests/fixtures/languages-b-functional/",
                    $path
                )),
                $expected,
            )
        }
    };
}
check!(ocaml, "original.ml", "moved.ml", "render");
check!(ocaml_interface, "original.mli", "moved.mli", "render");
check!(haskell, "original.hs", "moved.hs", "render");
check!(literate, "original.lhs", "moved.lhs", "render");
check!(erlang, "original.erl", "moved.erl", "render/1");
check!(erlang_header, "original.hrl", "moved.hrl", "OFFSET");
check!(fsharp, "original.fs", "moved.fs", "render");
check!(fsharp_signature, "original.fsi", "moved.fsi", "render");
check!(fsharp_script, "original.fsx", "moved.fsx", "#load");
