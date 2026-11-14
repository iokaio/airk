// SPDX-License-Identifier: Apache-2.0
//! Whole-proof validation over the Rust-emitted complete two-member Answer.
use super::*;
use serde_json::{Value, json};

fn whole_proof() -> Result<Value> {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/split/arbiter/combined_answer_cases.json"
    ))?;
    let mut projection = cases["cases"][0]["output"].clone();
    ensure!(
        projection["schema"] == crate::combined_reads::COMBINED_READ_SCHEMA,
        "owning combined fixture"
    );
    ensure!(
        projection.get("scope_identity").is_none(),
        "fixture must exercise native combined authority placement"
    );
    let mut proofs = Vec::new();
    for position in 0..2 {
        let member = &projection["members"][position];
        let own = member["result"].clone();
        // This literal pointer is independent of the inventory walker. Each
        // complete emitted packet retains one canonical Search citation.
        let pointer = "/steps/0/result/records/0/canonical_result/hits/0/citation";
        proofs.push(json!({"index_id":member["index_id"],
            "reference_bindings":[{"member_pointer":"/steps/0/result","result_pointer":format!("/members/{position}/result")}],
            "result":{"steps":[{"result":own}],"source_verification":{"schema":"airk.saved-source-verification/1",
                "references":[{"result_pointer":pointer}]}}}));
    }
    projection["member_verification"] = json!({"schema":"airk.combined-verification/1",
        "scope_identity":projection["publication_vector"]["authority"],"members":proofs});
    Ok(projection)
}

#[test]
fn combined_answer_whole_verification_uses_native_authority_and_preserves_both_complete_members()
-> Result<()> {
    let projection = whole_proof()?;
    let original = projection.clone();
    validate_projection(&projection)?;
    assert_eq!(
        projection, original,
        "validation cannot rewrite native packets, proofs or future fields"
    );
    assert_eq!(projection["members"].as_array().unwrap().len(), 2);
    assert_eq!(
        projection["members"][0]["result"]["future_public_data"],
        json!({"kept":[1,"exact"],"nothing_dropped":true})
    );
    let inventory = ReferenceInventory::collect(&projection);
    assert!(inventory.complete);
    assert_eq!(inventory.references.len(), 2);
    Ok(())
}

#[test]
fn combined_answer_whole_verification_rejects_foreign_missing_or_invalid_authority_and_rosters()
-> Result<()> {
    let original = whole_proof()?;
    validate_projection(&original)?;
    let mut changed = original.clone();
    changed["member_verification"]["scope_identity"] = json!("ff".repeat(32));
    assert!(validate_projection(&changed).is_err());
    let mut changed = original.clone();
    changed
        .as_object_mut()
        .unwrap()
        .remove("publication_vector");
    assert!(validate_projection(&changed).is_err());
    let mut changed = original.clone();
    changed["publication_vector"]["observed_from_unix_ms"] = json!(u64::MAX);
    assert!(validate_projection(&changed).is_err());
    let mut changed = original.clone();
    changed["publication_vector"]["members"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(validate_projection(&changed).is_err());
    let mut changed = original;
    changed["member_verification"]["members"][0]["index_id"] =
        json!(crate::index_catalog::IndexId::from_identity(
            crate::identity::Incarnation::from_bytes([3; 16])
        ));
    assert!(validate_projection(&changed).is_err());
    Ok(())
}

#[test]
fn combined_answer_whole_verification_rejects_cross_member_rebinding_and_changed_checked_reference()
-> Result<()> {
    let original = whole_proof()?;
    validate_projection(&original)?;
    let mut changed = original.clone();
    changed["member_verification"]["members"][0]["reference_bindings"][0]["result_pointer"] =
        json!("/members/1/result");
    assert!(
        validate_projection(&changed).is_err(),
        "equal-looking citations never transfer index authority"
    );
    let mut changed = original.clone();
    changed["member_verification"]["members"][1]["result"]["steps"][0]["result"]["records"][0]["canonical_result"]
        ["hits"][0]["citation"]["path"] = json!("changed.rs");
    assert!(validate_projection(&changed).is_err());
    let mut changed = original;
    changed["members"][1]["result"]["records"][0]["canonical_result"]["hits"] = json!([]);
    assert!(
        validate_projection(&changed).is_err(),
        "removing a checked reference must still refuse"
    );
    Ok(())
}
