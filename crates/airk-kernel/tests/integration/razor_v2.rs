// SPDX-License-Identifier: Apache-2.0
use super::current_inspection::InspectCurrent;
use airk_core::{DocumentLocation, SearchQuery};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::Result;
use std::{fs, path::PathBuf};

#[test]
fn razor_members_keep_mapped_locations_and_rebind_on_compatible_move() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let original = fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../airk-ingest/tests/fixtures/razor/Beacon.razor"),
    )?;
    fs::write(root.path().join("Beacon.razor"), &original)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let first = store.current_test_catalog()?.unwrap();
    let state = first.inventory.as_ref().unwrap().files["Beacon.razor"]
        .state
        .clone();
    let query = SearchQuery {
        text: "BeaconValue".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 8192,
    };
    let hits = kernel.search(&query)?;
    assert!(
        hits.hits
            .iter()
            .any(|hit| matches!(hit.citation.location, DocumentLocation::Mapped { .. }))
    );
    assert_eq!(kernel.index()?.indexed, 0);
    fs::rename(
        root.path().join("Beacon.razor"),
        root.path().join("Moved.razor"),
    )?;
    assert_eq!(kernel.index()?.indexed, 0);
    let moved = store.current_test_catalog()?.unwrap();
    let moved_state = &moved.inventory.as_ref().unwrap().files["Moved.razor"].state;
    assert_eq!(moved_state.artifact_id, state.artifact_id);
    assert_eq!(moved_state.payload_digest, state.payload_digest);
    assert!(
        kernel
            .search(&query)?
            .hits
            .iter()
            .all(|hit| hit.citation.path == "Moved.razor")
    );
    assert_eq!(fs::read(root.path().join("Moved.razor"))?, original);
    Ok(())
}
