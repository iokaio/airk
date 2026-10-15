// SPDX-License-Identifier: Apache-2.0
//! Activate after GPU/assembly profile/dependency admission; no execution claims.
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
                    "/../airk-ingest/tests/fixtures/specialist-gpu/",
                    $path
                )),
                $name,
            )
        }
    };
}
check!(cuda, "original.cu", "moved.cuh", "record");
check!(wgsl, "original.wgsl", "moved.wgsl", "record");
check!(glsl, "original.vert", "moved.vert", "main");
check!(assembly, "original.s", "moved.asm", "record");

#[test]
fn stage_and_explicit_isa_profile_changes_are_local() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("original.vert"), "void main() {}\n")?;
    fs::write(root.path().join("original.s"), "record:\ncall submit\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let before = store.catalog()?.unwrap();
    let before_inventory = before.inventory.as_ref().unwrap();
    let shader_profile = before_inventory.files["original.vert"].state.profile_id;
    let assembly_profile = before_inventory.files["original.s"].state.profile_id;
    assert!(
        !kernel
            .outline("original.s", 50, 65536)?
            .facts
            .iter()
            .any(|fact| fact.kind == "call")
    );
    fs::rename(
        root.path().join("original.vert"),
        root.path().join("original.frag"),
    )?;
    kernel.configure_extraction_policy(ExtractionPolicy {
        version: 1,
        rules: vec![SelectionRule {
            selector: RuleMatch::Exact {
                path: "original.s".into(),
            },
            case_sensitive: true,
            adapter_id: "assembly-x86-att".into(),
        }],
    })?;
    kernel.index()?;
    let after = store.catalog()?.unwrap();
    let inventory = after.inventory.as_ref().unwrap();
    assert_ne!(
        inventory.files["original.frag"].state.profile_id,
        shader_profile
    );
    assert_ne!(
        inventory.files["original.s"].state.profile_id,
        assembly_profile
    );
    assert!(
        kernel
            .outline("original.s", 50, 65536)?
            .facts
            .iter()
            .any(|fact| fact.kind == "call")
    );
    assert_eq!(
        fs::read_to_string(root.path().join("original.s"))?,
        "record:\ncall submit\n"
    );
    Ok(())
}
