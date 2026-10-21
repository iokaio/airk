// SPDX-License-Identifier: Apache-2.0
//! Original instruction documents are indexed evidence, never executable instructions.
use super::current_inspection::InspectCurrent;
use airk_core::FactQuery;
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::Result;
use std::{fs, path::PathBuf};

#[test]
fn instruction_markdown_retains_documents_untrusted_cues_and_safe_move_reuse() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../airk-ingest/tests/fixtures/instruction-documents/fictional-agents.md");
    let source = fs::read_to_string(fixture)?.replace('\n', "\r\n");
    fs::write(root.path().join("AGENTS.md"), &source)?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    // Broad instruction-document cue inventory is a Rich Structure capability.
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Balanced)?;
    kernel.index()?;
    let store = FileStore::new(state.path().join("collections-1.2/default"));
    let initial = store.current_test_catalog()?.unwrap();
    let file = initial.inventory.as_ref().unwrap().files["AGENTS.md"].clone();
    assert_eq!(file.language, "instruction-markdown");
    let query = |path: &str| FactQuery {
        path_prefix: path.into(),
        name: None,
        kind: None,
        language: None,
        exact_name: false,
        limit: 100,
        max_bytes: 65536,
        path_exact: true,
        document_only: true,
    };
    let facts = kernel.facts(&query("AGENTS.md"))?;
    assert!(
        facts
            .facts
            .iter()
            .any(|f| matches!(f.kind.as_str(), "heading" | "section"))
    );
    assert!(facts.facts.iter().any(|f| f.kind == "written_constraint"));
    assert!(
        facts
            .facts
            .iter()
            .any(|f| f.kind == "instruction_literal_path" && f.name == "src/beacon.rs")
    );
    for fact in facts
        .facts
        .iter()
        .filter(|f| f.syntax_kind == "instruction_document_cue")
    {
        assert_eq!(
            fact.document.as_ref().unwrap().basis,
            "untrusted_structural_rule"
        );
        assert!(fact.code.is_none());
        assert!(fact.continuity_key.is_none());
        assert!(!fact.name.contains("hidden/"));
    }
    fs::create_dir(root.path().join("nested"))?;
    fs::rename(
        root.path().join("AGENTS.md"),
        root.path().join("nested/AGENTS.md"),
    )?;
    assert_eq!(kernel.index()?.indexed, 0);
    let moved = store.current_test_catalog()?.unwrap();
    assert_eq!(
        moved.inventory.as_ref().unwrap().files["nested/AGENTS.md"]
            .state
            .payload_digest,
        file.state.payload_digest
    );
    assert!(
        kernel
            .facts(&query("nested/AGENTS.md"))?
            .facts
            .iter()
            .all(|f| f.citation.path == "nested/AGENTS.md")
    );
    fs::rename(
        root.path().join("nested/AGENTS.md"),
        root.path().join("nested/CLAUDE.md"),
    )?;
    assert!(
        kernel.index()?.indexed > 0,
        "different filename family invalidates extraction profile"
    );
    let renamed = store.current_test_catalog()?.unwrap();
    assert_ne!(
        renamed.inventory.as_ref().unwrap().files["nested/CLAUDE.md"]
            .state
            .profile_id,
        file.state.profile_id
    );
    assert_eq!(
        fs::read_to_string(root.path().join("nested/CLAUDE.md"))?,
        source
    );
    Ok(())
}
