// SPDX-License-Identifier: Apache-2.0
use airk_core::{DocumentLocation, SearchQuery};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::Result;
use std::{fs, path::PathBuf};

#[test]
fn notebook_regions_keep_original_citations_and_exclude_outputs_across_moves() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../airk-ingest/tests/fixtures/notebooks/original.ipynb");
    let original = fs::read(fixture)?;
    fs::write(root.path().join("original.ipynb"), &original)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let first = store.catalog()?.unwrap();
    let state = first.inventory.as_ref().unwrap().files["original.ipynb"]
        .state
        .clone();
    let query = |text: &str| SearchQuery {
        text: text.into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 8192,
    };
    for excluded in ["OUTPUT_ONLY_SENTINEL", "ATTACHMENT_ONLY_SENTINEL"] {
        assert!(kernel.search(&query(excluded))?.hits.is_empty());
    }
    let hits = kernel.search(&query("beacon_value"))?;
    assert!(!hits.hits.is_empty());
    assert!(
        hits.hits
            .iter()
            .all(|hit| matches!(hit.citation.location, DocumentLocation::Mapped { .. }))
    );
    fs::rename(
        root.path().join("original.ipynb"),
        root.path().join("moved.ipynb"),
    )?;
    assert_eq!(kernel.index()?.indexed, 0);
    let moved = store.catalog()?.unwrap();
    assert_eq!(
        moved.inventory.as_ref().unwrap().files["moved.ipynb"]
            .state
            .artifact_id,
        state.artifact_id
    );
    assert_eq!(
        moved.inventory.as_ref().unwrap().files["moved.ipynb"]
            .state
            .payload_digest,
        state.payload_digest
    );
    assert!(
        kernel
            .search(&query("beacon_value"))?
            .hits
            .iter()
            .all(|hit| hit.citation.path == "moved.ipynb")
    );
    assert_eq!(fs::read(root.path().join("moved.ipynb"))?, original);
    Ok(())
}
