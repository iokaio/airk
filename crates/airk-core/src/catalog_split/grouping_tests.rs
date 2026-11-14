// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn split_grouping_operation_identity_matches_independent_fixture() -> Result<()> {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../fixtures/split/catalog/split_grouping.json"
    ))?;
    let operation = fixture["operation_id"].as_str().unwrap().parse()?;
    assert_eq!(
        SplitGrouping::project_id(operation)?.to_string(),
        fixture["project_id"]
    );
    let changed = SplitGrouping::project_id(Incarnation::from_bytes([2; 16]))?;
    assert_ne!(changed.to_string(), fixture["project_id"]);
    label(fixture["name"].as_str().unwrap())?;
    Ok(())
}
