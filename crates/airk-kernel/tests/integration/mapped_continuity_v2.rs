// SPDX-License-Identifier: Apache-2.0
use super::current_inspection::InspectCurrent;
use airk_core::{
    DocumentLocation,
    changes::ChangeDimension,
    continuity::{ContinuityFamily, ContinuityRecord},
};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::fs;

fn records(store: &FileStore, path: &str) -> Result<Vec<ContinuityRecord>> {
    Ok(store
        .current_test_catalog()?
        .context("catalog")?
        .inventory
        .as_ref()
        .context("inventory")?
        .files[path]
        .continuity
        .clone())
}

#[test]
fn unchanged_child_shift_changes_original_location_then_child_edit_changes_body() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let original =
        "<html><body><script>function beaconTotal() { return 17 + 29; }</script></body></html>\n";
    fs::write(root.path().join("original.html"), original)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let first = records(&store, "original.html")?
        .into_iter()
        .find(|r| r.descriptor.family == ContinuityFamily::Code)
        .context("child code descriptor")?;
    assert!(matches!(
        first.descriptor.location,
        DocumentLocation::Mapped { .. }
    ));
    fs::write(
        root.path().join("original.html"),
        format!("\r\n\r\n{original}"),
    )?;
    kernel.index()?;
    let shifted = records(&store, "original.html")?
        .into_iter()
        .find(|r| r.key == first.key)
        .context("key preserved across host movement")?;
    assert!(
        shifted.changed[&ChangeDimension::Location] > first.changed[&ChangeDimension::Location]
    );
    for dimension in [
        ChangeDimension::CodeBody,
        ChangeDimension::CodeSignature,
        ChangeDimension::CodeName,
    ] {
        assert_eq!(
            shifted.changed.get(&dimension),
            first.changed.get(&dimension)
        );
    }
    fs::write(
        root.path().join("original.html"),
        format!("\r\n\r\n{}", original.replace("29", "31")),
    )?;
    kernel.index()?;
    let edited = records(&store, "original.html")?
        .into_iter()
        .find(|r| r.key == first.key)
        .context("key preserved across child body edit")?;
    assert!(edited.changed[&ChangeDimension::CodeBody] > first.changed[&ChangeDimension::CodeBody]);
    Ok(())
}

#[test]
fn notebook_markdown_array_gaps_and_paragraph_insertion_preserve_unit_key() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let notebook = |insert: bool| -> Result<String> {
        let mut source = vec!["# Original heading\n", "\n"];
        if insert {
            source.extend(["A newly inserted original paragraph.\n", "\n"]);
        }
        source.extend(["The beacon paragraph contains ", "a fictional café 🛰.\n"]);
        Ok(serde_json::to_string_pretty(
            &serde_json::json!({"nbformat":4,"nbformat_minor":5,"metadata":{},"cells":[
                {"cell_type":"markdown","id":"original-cell","metadata":{},"source":source}
            ]}),
        )?)
    };
    fs::write(root.path().join("original.ipynb"), notebook(false)?)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let first = records(&store, "original.ipynb")?
        .into_iter()
        .find(|r| {
            r.descriptor.family == ContinuityFamily::Documents
                && r.descriptor.category == "paragraph"
        })
        .context("Markdown paragraph unit")?;
    let DocumentLocation::Mapped {
        region: airk_core::location::RegionLocation::NotebookCell { original_spans, .. },
    } = &first.descriptor.location
    else {
        panic!("notebook location")
    };
    assert!(
        original_spans
            .windows(2)
            .any(|pair| pair[0].end < pair[1].start),
        "source array punctuation must remain an original-source gap"
    );
    fs::write(root.path().join("original.ipynb"), notebook(true)?)?;
    kernel.index()?;
    let after = records(&store, "original.ipynb")?;
    let shifted = after
        .iter()
        .find(|r| r.key == first.key)
        .context("paragraph key preserved")?;
    assert_eq!(
        shifted.changed.get(&ChangeDimension::Content),
        first.changed.get(&ChangeDimension::Content)
    );
    assert!(
        shifted.changed[&ChangeDimension::Location] > first.changed[&ChangeDimension::Location]
    );
    let subjects: std::collections::BTreeSet<_> =
        after.iter().map(|r| r.descriptor.subject).collect();
    assert_eq!(
        subjects.len(),
        after.len(),
        "child units must not reuse host/other-cell IDs"
    );
    Ok(())
}

#[test]
fn identical_sibling_scripts_do_not_promote_position_into_region_identity() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let body = "<script>function beaconTotal() { return 17 + 29; }</script>";
    let original = format!("<html><body>{body}{body}</body></html>\n");
    fs::write(root.path().join("duplicate.html"), &original)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let before = records(&store, "duplicate.html")?;
    let regions: Vec<_> = before
        .iter()
        .filter(|r| {
            matches!(
                r.descriptor.subject,
                airk_core::continuity::ContinuitySubject::Region(_)
            )
        })
        .collect();
    assert_eq!(regions.len(), 2);
    assert!(
        regions
            .iter()
            .all(|r| r.descriptor.explicit_identity.is_none())
    );
    let old_keys: std::collections::BTreeSet<_> = regions.iter().map(|r| r.key).collect();
    fs::write(root.path().join("duplicate.html"), format!("\n{original}"))?;
    kernel.index()?;
    let after = records(&store, "duplicate.html")?;
    assert!(
        after
            .iter()
            .filter(|r| matches!(
                r.descriptor.subject,
                airk_core::continuity::ContinuitySubject::Region(_)
            ))
            .all(|r| !old_keys.contains(&r.key))
    );
    Ok(())
}
