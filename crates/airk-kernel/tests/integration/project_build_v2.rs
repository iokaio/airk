// SPDX-License-Identifier: Apache-2.0
//! Move into tests/ after project build profiles are admitted.
use airk_core::{EvidenceBasis, FactQuery};
use airk_ingest::adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::fs;

fn qualify(path: &str, moved: &str, text: &str, kind: &str) -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let source = text.replace("\r\n", "\n").replace('\n', "\r\n");
    fs::write(workspace.path().join(path), &source)?;
    fs::write(
        workspace.path().join("unchanged.rs"),
        "pub fn unaffected() -> u32 { 31 }\n",
    )?;
    let kernel = Kernel::open(workspace.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    assert_eq!(kernel.index()?.indexed, 2);
    let store = FileStore::new(data.path().join("collections/default"));
    let original = store.catalog()?.context("catalog")?;
    assert_eq!(
        store.read_retained_source(&original.snapshot.artifacts[path])?,
        source.as_bytes()
    );
    let baseline = &original.inventory.as_ref().context("inventory")?.files;
    let before = baseline[path].state.clone();
    let unchanged = baseline["unchanged.rs"].state.clone();
    let facts = kernel.outline(path, 100, 65536)?;
    assert!(facts.facts.iter().any(|fact| fact.kind == kind));
    for fact in facts.facts.iter().filter(|fact| {
        fact.code
            .as_ref()
            .is_some_and(|code| code.basis == EvidenceBasis::StructuralRule)
    }) {
        let code = fact.code.as_ref().unwrap();
        assert_eq!(code.producer_version, "project-facts/1");
        assert!(code.fingerprints.is_none());
        assert!(
            fact.continuity_key.is_none(),
            "structural overlays do not invent declaration identity"
        );
        let span = code.span.as_ref().context("span")?;
        assert!(source[span.start_byte..span.end_byte].contains(&fact.name));
        assert_eq!(
            fact.citation.start_line,
            source[..span.start_byte]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1
        );
    }
    let limited = kernel.facts(&FactQuery {
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
    assert!(limited.facts.len() <= 1 && limited.truncated);
    let warm = kernel.index()?;
    assert_eq!((warm.indexed, warm.reused), (0, 2));
    kernel.configure_extraction_policy(ExtractionPolicy {
        detail: None,
        version: 1,
        rules: vec![SelectionRule {
            selector: RuleMatch::Exact { path: path.into() },
            case_sensitive: true,
            adapter_id: "text".into(),
        }],
    })?;
    let changed = kernel.index()?;
    assert_eq!((changed.indexed, changed.reused), (1, 1));
    assert_eq!(
        store.catalog()?.unwrap().inventory.as_ref().unwrap().files["unchanged.rs"].state,
        unchanged
    );
    kernel.configure_extraction_policy(ExtractionPolicy {
        detail: None,
        version: 1,
        rules: vec![],
    })?;
    kernel.index()?;
    fs::rename(workspace.path().join(path), workspace.path().join(moved))?;
    let report = kernel.index()?;
    assert_eq!((report.indexed, report.reused), (0, 2));
    let final_catalog = store.catalog()?.context("final catalog")?;
    let after = &final_catalog
        .inventory
        .as_ref()
        .context("final inventory")?
        .files[moved]
        .state;
    assert_eq!(before.payload_digest, after.payload_digest);
    assert_eq!(before.source_hash, after.source_hash);
    assert!(
        kernel
            .outline(moved, 100, 65536)?
            .facts
            .iter()
            .all(|fact| fact.citation.path == moved)
    );
    Ok(())
}

#[test]
fn bazel_project_cues_survive_bounded_queries_policy_changes_and_move_reuse() -> Result<()> {
    qualify(
        "BUILD.bazel",
        "rules.bzl",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../airk-ingest/tests/fixtures/project-build/BUILD.bazel"
        )),
        "build_target",
    )
}

#[test]
fn gradle_overlay_preserves_native_evidence_and_profile_scoped_reuse() -> Result<()> {
    qualify(
        "build.gradle.kts",
        "settings.gradle.kts",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../airk-ingest/tests/fixtures/project-build/build.gradle.kts"
        )),
        "dependency",
    )
}
