// SPDX-License-Identifier: Apache-2.0
use super::current_inspection::InspectCurrent;
use airk_core::{
    changes::ChangeDimension,
    continuity::{ContinuityFamily, ContinuityRecord},
};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::{fs, path::Path};

fn records(store: &FileStore, path: &str) -> Result<Vec<ContinuityRecord>> {
    let catalog = store.current_test_catalog()?.context("catalog missing")?;
    Ok(catalog
        .inventory
        .as_ref()
        .context("inventory missing")?
        .files[path]
        .continuity
        .clone())
}
fn symbol(items: &[ContinuityRecord]) -> &ContinuityRecord {
    items
        .iter()
        .find(|item| item.descriptor.family == ContinuityFamily::Code)
        .expect("code descriptor")
}
fn write(root: &Path, text: &str) -> Result<()> {
    fs::write(root.join("sample.rs"), text)?;
    Ok(())
}

#[test]
fn durable_keys_preserve_body_stamps_across_location_edits_and_reopen() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let original =
        "pub fn calculate_total() -> u32 {\n    let subtotal = 17;\n    subtotal + 29\n}\n";
    write(root.path(), original)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let first = records(&store, "sample.rs")?;
    let first = symbol(&first).clone();
    let baseline = store.current_test_catalog()?.context("catalog")?.token;
    let keyed = kernel.continuity_facts(first.key, 10, 8192)?;
    assert!(!keyed.facts.is_empty());
    assert!(
        keyed
            .facts
            .iter()
            .all(|fact| fact.continuity_key == Some(first.key))
    );
    assert!(
        kernel
            .outline("sample.rs", 10, 8192)?
            .facts
            .iter()
            .any(|fact| fact.continuity_key == Some(first.key))
    );
    write(root.path(), &format!("\n\n{original}"))?;
    kernel.index()?;
    let shifted = records(&store, "sample.rs")?;
    let shifted = symbol(&shifted);
    assert_eq!(shifted.key, first.key);
    assert_eq!(shifted.first_generation, first.first_generation);
    assert_eq!(
        shifted.changed.get(&ChangeDimension::CodeBody),
        first.changed.get(&ChangeDimension::CodeBody)
    );
    assert!(
        shifted.changed[&ChangeDimension::Location] > first.changed[&ChangeDimension::Location]
    );
    write(root.path(), &original.replace("29", "31"))?;
    kernel.index()?;
    drop(kernel);
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    let edited = records(&store, "sample.rs")?;
    let edited = symbol(&edited);
    assert_eq!(edited.key, first.key);
    assert!(edited.changed[&ChangeDimension::CodeBody] > first.changed[&ChangeDimension::CodeBody]);
    reopened.index()?;
    assert_eq!(symbol(&records(&store, "sample.rs")?).key, first.key);
    write(root.path(), original)?;
    reopened.index()?;
    let reverted = records(&store, "sample.rs")?;
    let reverted = symbol(&reverted);
    assert_eq!(reverted.key, first.key);
    assert_eq!(
        reverted.descriptor.dimensions[&ChangeDimension::CodeBody],
        first.descriptor.dimensions[&ChangeDimension::CodeBody]
    );
    assert!(
        reverted.changed[&ChangeDimension::CodeBody] > first.changed[&ChangeDimension::CodeBody],
        "an observed edit/revert remains changed"
    );
    let airk_core::continuity::ContinuityKey::Symbol(symbol_key) = first.key else {
        panic!("symbol key")
    };
    let changes = reopened.changes(&airk_core::changes::ChangesRequest {
        since: Some(baseline),
        include_symbols: true,
        selectors: vec![airk_core::changes::RememberedSelector::Symbol { symbol_key }],
        max_bytes: 65536,
        limit: 100,
        ..Default::default()
    })?;
    assert_eq!(
        changes.validations[0].outcome,
        airk_core::changes::ValidationOutcome::Current
    );
    assert!(changes.changes.iter().any(|change|
        matches!(change.subject, airk_core::changes::ChangeSubject::Symbol { symbol_key: key, .. } if key == symbol_key)
            && change.dimensions.contains(&ChangeDimension::CodeBody) && change.observations >= 2));
    assert_eq!(
        changes.baseline_status,
        airk_core::changes::BaselineStatus::Verified
    );
    fs::remove_file(root.path().join("sample.rs"))?;
    reopened.index()?;
    let removed = reopened.changes(&airk_core::changes::ChangesRequest {
        since: Some(baseline),
        include_symbols: true,
        max_bytes: 65536,
        limit: 100,
        ..Default::default()
    })?;
    assert!(removed.changes.iter().any(|change|
        matches!(change.subject, airk_core::changes::ChangeSubject::Symbol { symbol_key: key, .. } if key == symbol_key)
            && change.before.is_some() && change.after.is_none()));
    Ok(())
}

#[test]
fn explicit_document_identity_survives_statement_edit_but_not_another_file() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("requirements.md"),
        "# Requirements\n\nREQ-731: The fictional service must retain 17 records.\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let before = records(&store, "requirements.md")?;
    let original = before
        .iter()
        .find(|item| item.descriptor.explicit_identity.is_some())
        .context("explicit requirement missing")?;
    let keyed = kernel.continuity_facts(original.key, 10, 8192)?;
    assert!(
        keyed
            .facts
            .iter()
            .any(|fact| fact.kind == "requirement" && fact.continuity_key == Some(original.key))
    );
    fs::write(
        root.path().join("requirements.md"),
        "# Requirements\n\nREQ-731: The fictional service must retain 23 records.\n",
    )?;
    fs::write(
        root.path().join("independent.md"),
        "# Requirements\n\nREQ-731: The fictional service must retain 23 records.\n",
    )?;
    kernel.index()?;
    let after = records(&store, "requirements.md")?;
    let current = after
        .iter()
        .find(|item| item.descriptor.subject == original.descriptor.subject)
        .context("requirement missing")?;
    assert_eq!(current.key, original.key);
    assert!(
        current
            .changed
            .values()
            .any(|stamp| *stamp > original.first_generation)
    );
    assert!(
        records(&store, "independent.md")?
            .iter()
            .all(|item| item.key != original.key)
    );
    Ok(())
}

#[test]
fn informative_file_move_and_full_reindex_preserve_file_and_symbol_identity() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    write(
        root.path(),
        "/// Computes an original shipping estimate from several independent line items.\npub fn shipping_total(first_item: u32, second_item: u32) -> u32 {\n    let subtotal = first_item + second_item;\n    let service_charge = 17;\n    subtotal + service_charge\n}\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let first = symbol(&records(&store, "sample.rs")?).clone();
    let old_file = store
        .current_test_catalog()?
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
        .files["sample.rs"]
        .clone();
    fs::rename(root.path().join("sample.rs"), root.path().join("moved.rs"))?;
    let report = kernel.index()?;
    assert_eq!(report.indexed, 0, "verified move avoids parsing");
    assert_eq!(report.reused, 1);
    let moved = symbol(&records(&store, "moved.rs")?).clone();
    let new_file = store
        .current_test_catalog()?
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
        .files["moved.rs"]
        .clone();
    assert_eq!(new_file.state.artifact_id, old_file.state.artifact_id);
    assert_eq!(new_file.state.payload_digest, old_file.state.payload_digest);
    assert_eq!(moved.key, first.key);
    assert_eq!(
        moved.changed[&ChangeDimension::CodeBody],
        first.changed[&ChangeDimension::CodeBody]
    );
    assert!(
        kernel
            .continuity_facts(first.key, 10, 8192)?
            .facts
            .iter()
            .all(|fact| fact.citation.path == "moved.rs")
    );
    let rebuilt = kernel.reindex()?;
    assert_eq!(rebuilt.indexed, 1);
    assert_eq!(symbol(&records(&store, "moved.rs")?).key, first.key);
    drop(kernel);
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert!(
        !reopened
            .continuity_facts(first.key, 10, 8192)?
            .facts
            .is_empty()
    );
    Ok(())
}

#[test]
fn duplicate_requirement_identifiers_receive_explicitly_ambiguous_fresh_keys() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let original = "# Original requirements\n\nREQ-882: The fictional queue must retain 17 entries.\n\nREQ-882: The fictional worker must retry 23 attempts.\n";
    fs::write(root.path().join("requirements.md"), original)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let requirements = |records: Vec<ContinuityRecord>| {
        records
            .into_iter()
            .filter(|record| {
                record.descriptor.category == "requirement"
                    && record.descriptor.explicit_identity.is_some()
            })
            .collect::<Vec<_>>()
    };
    let old = requirements(records(&store, "requirements.md")?);
    assert_eq!(old.len(), 2);
    fs::write(
        root.path().join("requirements.md"),
        original.replace("17", "19").replace("23", "29"),
    )?;
    kernel.index()?;
    let new = requirements(records(&store, "requirements.md")?);
    assert_eq!(new.len(), 2);
    for record in new {
        assert!(old.iter().all(|prior| prior.key != record.key));
        assert!(record.ambiguous_predecessor);
        assert!(
            kernel
                .continuity_facts(record.key, 10, 8192)?
                .facts
                .iter()
                .all(|fact| fact.continuity_key == Some(record.key))
        );
    }
    Ok(())
}

#[test]
fn rename_uses_unique_informative_body_but_trivial_and_equal_overloads_are_not_guessed()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let original = "fn original(first_value: u32, second_value: u32, final_value: u32) -> u32 { let combined = first_value + second_value; consume(combined, final_value) }\nfn trivial() {}\n";
    write(root.path(), original)?;
    fs::write(
        root.path().join("Overloaded.java"),
        "class Overloaded { int run(int value) { return first_value + second_value + third_value + fourth_value; } int run(String value) { return first_value + second_value + third_value + fourth_value; } }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let old = records(&store, "sample.rs")?;
    let informative = old
        .iter()
        .find(|record| record.descriptor.body_informative)
        .unwrap();
    let trivial = old
        .iter()
        .find(|record| !record.descriptor.body_informative)
        .unwrap();
    let overloaded = records(&store, "Overloaded.java")?
        .into_iter()
        .filter(|record| record.descriptor.category == "method_declaration")
        .collect::<Vec<_>>();
    assert_eq!(overloaded.len(), 2);
    for record in &overloaded {
        let facts = kernel.continuity_facts(record.key, 10, 8192)?.facts;
        assert_eq!(facts.len(), 1, "overload read binds the exact native span");
        let span = facts[0].code.as_ref().unwrap().span.as_ref().unwrap();
        assert_eq!(
            (span.start_byte, span.end_byte),
            (record.descriptor.span.start, record.descriptor.span.end)
        );
    }
    write(
        root.path(),
        &original
            .replace("fn original", "fn renamed")
            .replace("fn trivial", "fn other_trivial"),
    )?;
    let java = fs::read_to_string(root.path().join("Overloaded.java"))?;
    fs::write(
        root.path().join("Overloaded.java"),
        java.replace("run(", "execute("),
    )?;
    kernel.index()?;
    let current = records(&store, "sample.rs")?;
    assert_eq!(
        current
            .iter()
            .find(|record| record.descriptor.body_informative)
            .unwrap()
            .key,
        informative.key
    );
    assert_ne!(
        current
            .iter()
            .find(|record| !record.descriptor.body_informative)
            .unwrap()
            .key,
        trivial.key
    );
    let current_overloads = records(&store, "Overloaded.java")?
        .into_iter()
        .filter(|record| record.descriptor.category == "method_declaration")
        .collect::<Vec<_>>();
    assert_eq!(current_overloads.len(), 2);
    for record in current_overloads {
        assert!(overloaded.iter().all(|old| old.key != record.key));
        assert!(record.ambiguous_predecessor);
    }
    Ok(())
}

#[test]
fn unsupported_documentation_dimension_returns_unknown_instead_of_unchanged() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("original.py"),
        "def original(value):\n    return value + 17\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let record = symbol(&records(&store, "original.py")?).clone();
    assert!(
        record
            .descriptor
            .unsupported_dimensions
            .contains(&ChangeDimension::Documentation)
    );
    let baseline = store.current_test_catalog()?.unwrap().token;
    let airk_core::continuity::ContinuityKey::Symbol(symbol_key) = record.key else {
        panic!("symbol")
    };
    let response = kernel.changes(&airk_core::changes::ChangesRequest {
        since: Some(baseline),
        include_symbols: true,
        selectors: vec![airk_core::changes::RememberedSelector::Symbol { symbol_key }],
        ..Default::default()
    })?;
    assert_eq!(
        response.validations[0].outcome,
        airk_core::changes::ValidationOutcome::Current
    );
    Ok(())
}
