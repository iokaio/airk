// SPDX-License-Identifier: Apache-2.0
//! Original first-batch specialist publication and reuse qualification.
use airk_ingest::adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::fs;

fn qualify(path: &str, moved: &str, source: &str, expected: &str) -> Result<()> {
    qualify_profile(path, moved, source, expected, None)
}
fn qualify_profile(
    path: &str,
    moved: &str,
    source: &str,
    expected: &str,
    profile: Option<&str>,
) -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let source = source.replace("\r\n", "\n").replace('\n', "\r\n");
    fs::write(root.path().join(path), &source)?;
    fs::write(
        root.path().join("unrelated.rs"),
        "pub fn unaffected() -> u32 {17}\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    if let Some(profile) = profile {
        kernel.configure_extraction_policy(ExtractionPolicy {
            version: 1,
            rules: vec![SelectionRule {
                selector: RuleMatch::Scoped {
                    prefix: String::new(),
                    suffix: Some(".cob".into()),
                    basename: None,
                },
                case_sensitive: true,
                adapter_id: profile.into(),
            }],
        })?;
    }
    assert_eq!(kernel.index()?.indexed, 2);
    let store = FileStore::new(data.path().join("collections/default"));
    let catalog = store.catalog()?.context("catalog")?;
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
    if path.ends_with(".f90") || path.ends_with(".pas") {
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
    let after = store.catalog()?.context("moved catalog")?;
    assert_eq!(
        after.inventory.as_ref().unwrap().files[moved]
            .state
            .payload_digest,
        before.payload_digest
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
    let after = store.catalog()?.context("changed catalog")?;
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
#[test]
fn cobol_free_requires_explicit_profile_and_survives_move() -> Result<()> {
    qualify_profile(
        "free.cob",
        "moved-free.cob",
        include_str!("../../../airk-ingest/tests/fixtures/specialist-business/free.cob"),
        "FREE-ARCHIVE",
        Some("cobol-free"),
    )
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
                    "/../airk-ingest/tests/fixtures/specialist-business/",
                    $path
                )),
                $expected,
            )
        }
    };
}
check!(fortran_free, "original.f90", "moved.f90", "render");
check!(fortran_fixed, "original.f", "moved.f", "RENDER");
check!(cobol, "original.cob", "moved.cob", "ARCHIVE");
check!(copybook, "original.cpy", "moved.cpy", "ARCHIVE");
check!(vbnet, "original.vb", "moved.vb", "Render");
check!(delphi, "original.pas", "moved.pas", "Render");
check!(delphi_program, "original.dpr", "moved.dpr", "ArchiveMain");
check!(
    delphi_package,
    "original.dpk",
    "moved.dpk",
    "ArchivePackage"
);
