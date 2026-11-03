// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{identity::Incarnation, index_catalog::PathScope};

fn fixture() -> Result<SourcePolicy> {
    let fixtures: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/research/contracts-v190/fixtures.json"
    ))?)?;
    Ok(serde_json::from_value(
        fixtures["policy_cases"][0]["value"].clone(),
    )?)
}

#[test]
fn native_policy_format_matches_the_approved_positive_and_negative_wire_cases() -> Result<()> {
    let fixtures: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/research/contracts-v190/fixtures.json"
    ))?)?;
    for case in fixtures["policy_cases"].as_array().unwrap() {
        let valid = serde_json::from_value::<SourcePolicy>(case["value"].clone())
            .and_then(|policy| policy.validate().map_err(serde::de::Error::custom))
            .is_ok();
        assert_eq!(valid, case["valid"].as_bool().unwrap(), "{}", case["name"]);
    }
    Ok(())
}

#[test]
fn denies_win_across_alias_changes_and_positive_empty_is_not_all() -> Result<()> {
    let mut policy = fixture()?;
    policy.validate()?;
    let root = policy.roots[0].id;
    assert!(policy.allows_canonical(root, "src/public.rs"));
    assert!(!policy.allows_canonical(root, "private/secret.rs"));
    assert!(policy.allows_canonical(root, "private-other/public.rs"));
    assert!(!policy.allows_canonical(
        SourceRootId::from_identity(Incarnation::from_bytes([9; 16])),
        "src/public.rs"
    ));
    let digest = policy.identity()?;
    policy.roots[0].aliases.push("renamed-source".into());
    policy.validate()?;
    assert_ne!(policy.identity()?, digest);
    assert!(!policy.allows_canonical(root, "private/secret.rs"));
    policy.positive_includes = Some(vec![ScopeAtom {
        source_root_id: root,
        kind: PathScope::Subtree,
        path: String::new(),
    }]);
    policy.validate()?;
    assert!(!policy.allows_canonical(root, "private/secret.rs"));
    policy.positive_includes = Some(vec![]);
    policy.validate()?;
    assert!(!policy.allows_canonical(root, "src/public.rs"));
    Ok(())
}

#[test]
fn policies_reject_duplicate_unknown_and_future_authority() -> Result<()> {
    let policy = fixture()?;
    let mut duplicate = policy.clone();
    duplicate.roots.push(duplicate.roots[0].clone());
    assert!(duplicate.validate().is_err());
    let mut future = policy.clone();
    future.denies[0].created_epoch = CatalogRevision::INITIAL.next()?;
    assert!(future.validate().is_err());
    let mut unknown = policy.clone();
    unknown.denies[0].source_root_id =
        SourceRootId::from_identity(Incarnation::from_bytes([9; 16]));
    assert!(unknown.validate().is_err());
    let mut oversized = policy.clone();
    oversized.roots[0].aliases = vec!["é".repeat(65)];
    assert!(oversized.validate().is_err());
    let mut incompatible = serde_json::to_value(policy)?;
    incompatible["schema"] = "airk.source-policy/2".into();
    assert!(
        serde_json::from_value::<SourcePolicy>(incompatible)?
            .validate()
            .is_err()
    );
    Ok(())
}
