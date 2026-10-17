// SPDX-License-Identifier: Apache-2.0
//! Activate after hardware profile/dependency admission; no execution claims.
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
        "pub fn stable() -> u32 { 17 }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let catalog = store.catalog()?.context("catalog")?;
    let inventory = catalog.inventory.as_ref().unwrap();
    let before = inventory.files[path].state.clone();
    let unrelated = inventory.files["unrelated.rs"].state.clone();
    assert_eq!(
        store.read_retained_source(&catalog.snapshot.artifacts[path])?,
        source.as_bytes()
    );
    let facts = kernel.outline(path, 100, 65536)?;
    assert!(facts.facts.iter().any(|fact| fact.name == expected));
    let key = facts
        .facts
        .iter()
        .find(|fact| fact.name == expected)
        .and_then(|fact| fact.continuity_key)
        .context("qualified written declaration key")?;
    assert_eq!(kernel.continuity_facts(key, 1, 4096)?.facts.len(), 1);
    let limited = kernel.outline(path, 1, 4096)?;
    assert_eq!(limited.facts.len(), 1);
    let warm = kernel.index()?;
    assert_eq!((warm.indexed, warm.reused), (0, 2));
    fs::rename(root.path().join(path), root.path().join(moved))?;
    let report = kernel.index()?;
    assert_eq!((report.indexed, report.reused), (0, 2));
    let moved_catalog = store.catalog()?.unwrap();
    assert_eq!(
        moved_catalog.inventory.as_ref().unwrap().files[moved]
            .state
            .payload_digest,
        before.payload_digest
    );
    assert_eq!(
        kernel.continuity_facts(key, 1, 4096)?.facts[0]
            .citation
            .path,
        moved
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
    let after = store.catalog()?.unwrap();
    let inventory = after.inventory.as_ref().unwrap();
    assert_ne!(inventory.files[moved].state.profile_id, before.profile_id);
    assert_eq!(inventory.files["unrelated.rs"].state, unrelated);
    assert_eq!(fs::read_to_string(root.path().join(moved))?, source);
    Ok(())
}
macro_rules! check {
    ($test:ident,$path:literal,$moved:literal,$name:literal) => {
        #[test]
        fn $test() -> Result<()> {
            qualify(
                $path,
                $moved,
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../airk-ingest/tests/fixtures/specialist-hardware/",
                    $path
                )),
                $name,
            )
        }
    };
}
check!(solidity, "original.sol", "moved.sol", "Ledger");
check!(vhdl, "original.vhdl", "moved.vhd", "ledger");
check!(systemverilog, "original.sv", "moved.svh", "ledger");

#[test]
fn ambiguous_v_suffix_needs_visible_evidence_or_explicit_override() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("proof.v"),
        "Theorem original_identity : forall n : nat, n = n.\n",
    )?;
    fs::write(
        root.path().join("hardware.v"),
        "module ledger(input wire clk); endmodule\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let catalog = store.catalog()?.unwrap();
    let inventory = catalog.inventory.as_ref().unwrap();
    assert_eq!(
        inventory.profiles[&inventory.files["proof.v"].state.profile_id].adapter_id,
        "text"
    );
    assert_eq!(
        inventory.profiles[&inventory.files["hardware.v"].state.profile_id].adapter_id,
        "verilog"
    );
    assert!(
        kernel
            .outline("hardware.v", 10, 8192)?
            .facts
            .iter()
            .any(|fact| fact.name == "ledger")
    );
    kernel.configure_extraction_policy(ExtractionPolicy {
        detail: None,
        version: 1,
        rules: vec![SelectionRule {
            selector: RuleMatch::Exact {
                path: "proof.v".into(),
            },
            case_sensitive: true,
            adapter_id: "verilog".into(),
        }],
    })?;
    kernel.index()?;
    let catalog = store.catalog()?.unwrap();
    let inventory = catalog.inventory.as_ref().unwrap();
    assert_eq!(
        inventory.profiles[&inventory.files["proof.v"].state.profile_id].adapter_id,
        "verilog"
    );
    Ok(())
}
