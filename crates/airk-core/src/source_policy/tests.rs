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

fn trim_change(policy: &SourcePolicy, kind: PathScope, path: &str) -> PolicyChange {
    PolicyChange::Trim {
        rule_id: DenyId::from_identity(Incarnation::from_bytes([42; 16])),
        selector: ScopeAtom {
            source_root_id: policy.roots[0].id,
            kind,
            path: path.into(),
        },
    }
}

#[test]
fn trim_preview_preserves_bindings_and_existing_denies_and_rejects_stale_review() -> Result<()> {
    let policy = fixture()?;
    let original = serde_json::to_vec(&policy)?;
    let change = trim_change(&policy, PathScope::Subtree, "src/private");
    let preview = policy.preview_change(policy.identity()?, change.clone())?;
    preview.validate_against(&policy)?;
    assert_eq!(preview.before_epoch, policy.epoch);
    assert_eq!(preview.after.epoch, policy.epoch.next()?);
    assert_eq!(preview.after.roots, policy.roots);
    assert_eq!(preview.after.index_id, policy.index_id);
    assert_eq!(preview.after.owner_binding, policy.owner_binding);
    assert_eq!(preview.after.collection, policy.collection);
    assert_eq!(preview.after.namespace, policy.namespace);
    assert_eq!(preview.after.positive_includes, policy.positive_includes);
    assert_eq!(&preview.after.denies[..policy.denies.len()], policy.denies);
    assert!(
        !preview
            .after
            .allows_canonical(policy.roots[0].id, "src/private/new.rs")
    );
    assert!(
        preview
            .after
            .allows_canonical(policy.roots[0].id, "src/private-other/new.rs")
    );
    assert!(
        preview
            .after
            .preview_change(policy.identity()?, change)
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&policy)?, original);
    Ok(())
}

#[test]
fn serialized_trim_preview_cannot_replace_its_successor_or_operation() -> Result<()> {
    let policy = fixture()?;
    let preview = policy.preview_change(
        policy.identity()?,
        trim_change(&policy, PathScope::Exact, "src/a.rs"),
    )?;
    let encoded = serde_json::to_vec(&preview)?;
    let roundtrip: PolicyChangePreview = serde_json::from_slice(&encoded)?;
    roundtrip.validate_against(&policy)?;
    let mut changed = preview.clone();
    changed.after.denies.clear();
    assert!(changed.validate_against(&policy).is_err());
    changed = preview.clone();
    changed.after.roots[0].canonical_path = "/different-root".into();
    assert!(changed.validate_against(&policy).is_err());
    changed = preview.clone();
    changed.change = trim_change(&policy, PathScope::Subtree, "src");
    assert!(changed.validate_against(&policy).is_err());
    changed = preview;
    changed.before_epoch = policy.epoch.next()?;
    assert!(changed.validate_against(&policy).is_err());
    Ok(())
}

#[test]
fn explicit_unblock_preserves_other_denies_and_cannot_remove_split_fences() -> Result<()> {
    let policy = fixture()?;
    let added = policy
        .preview_change(
            policy.identity()?,
            trim_change(&policy, PathScope::Exact, "src/a.rs"),
        )?
        .after;
    let rule = added.denies.last().unwrap().rule_id;
    let unblocked =
        added.preview_change(added.identity()?, PolicyChange::Unblock { rule_id: rule })?;
    assert_eq!(unblocked.after.denies, policy.denies);
    assert_eq!(unblocked.after.epoch, added.epoch.next()?);
    assert!(
        unblocked
            .after
            .allows_canonical(policy.roots[0].id, "src/a.rs")
    );
    let mut transfer = added.clone();
    transfer.denies.last_mut().unwrap().reason = DenyReason::SplitTransfer;
    assert!(
        transfer
            .preview_change(
                transfer.identity()?,
                PolicyChange::Unblock { rule_id: rule }
            )
            .is_err()
    );
    assert!(
        policy
            .preview_change(policy.identity()?, PolicyChange::Unblock { rule_id: rule })
            .is_err()
    );
    Ok(())
}

#[test]
fn trim_rejections_leave_the_original_policy_unchanged() -> Result<()> {
    let policy = fixture()?;
    let original = serde_json::to_vec(&policy)?;
    let root = policy.roots[0].id;
    for path in [
        "../private",
        "a/../private",
        "/private",
        "a\\private",
        "a//private",
        "a/",
        "C:private",
        "",
    ] {
        assert!(
            policy
                .preview_change(
                    policy.identity()?,
                    trim_change(&policy, PathScope::Exact, path)
                )
                .is_err(),
            "{path}"
        );
    }
    let mut unknown = trim_change(&policy, PathScope::Exact, "valid.rs");
    if let PolicyChange::Trim { selector, .. } = &mut unknown {
        selector.source_root_id = SourceRootId::from_identity(Incarnation::from_bytes([99; 16]));
    }
    assert!(policy.preview_change(policy.identity()?, unknown).is_err());
    for rule in &policy.denies {
        assert!(
            policy
                .preview_change(
                    policy.identity()?,
                    PolicyChange::Trim {
                        rule_id: DenyId::from_identity(Incarnation::from_bytes([43; 16])),
                        selector: rule.atom(),
                    }
                )
                .is_err()
        );
        assert!(
            policy
                .preview_change(
                    policy.identity()?,
                    PolicyChange::Trim {
                        rule_id: rule.rule_id,
                        selector: ScopeAtom {
                            source_root_id: root,
                            kind: PathScope::Exact,
                            path: "other.rs".into()
                        },
                    }
                )
                .is_err()
        );
    }
    let mut exhausted = policy.clone();
    exhausted.epoch = CatalogRevision::try_from(i64::MAX as u64)?;
    assert!(
        exhausted
            .preview_change(
                exhausted.identity()?,
                trim_change(&exhausted, PathScope::Exact, "valid.rs")
            )
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&policy)?, original);
    Ok(())
}

#[test]
fn bounded_generated_trim_cases_never_expand_eligibility() -> Result<()> {
    let mut policy = fixture()?;
    let root = policy.roots[0].id;
    // Fixed, original UTF-8 cases. These are canonical component spellings;
    // filesystem case/alias resolution remains the native adapter's obligation.
    for component in ["a", "café", "日本語", "δ", "dot.name", "under_score"] {
        for kind in [PathScope::Exact, PathScope::Subtree] {
            let path = format!("generated/{component}");
            let next = policy
                .preview_change(policy.identity()?, trim_change(&policy, kind, &path))?
                .after;
            for suffix in ["", "/child.rs", "-sibling", ".rs"] {
                let candidate = format!("{path}{suffix}");
                assert!(
                    !next.allows_canonical(root, &candidate)
                        || policy.allows_canonical(root, &candidate)
                );
                if suffix.is_empty() || kind == PathScope::Subtree && suffix.starts_with('/') {
                    assert!(!next.allows_canonical(root, &candidate));
                }
            }
        }
    }
    policy.positive_includes = Some(vec![]);
    let next = policy
        .preview_change(
            policy.identity()?,
            trim_change(&policy, PathScope::Subtree, ""),
        )?
        .after;
    assert!(!next.allows_canonical(root, "new/file.rs"));
    Ok(())
}

#[test]
fn trim_capacity_failure_preserves_every_existing_rule() -> Result<()> {
    let mut policy = fixture()?;
    let root = policy.roots[0].id;
    for number in policy.denies.len()..MAX_ATOMS {
        policy.denies.push(DenyRule {
            source_root_id: root,
            kind: PathScope::Exact,
            path: format!("generated/{number}.rs"),
            rule_id: format!("deny_{:032x}", number + 1000).parse()?,
            reason: DenyReason::Trim,
            created_epoch: policy.epoch,
            inherited_from: None,
        });
    }
    let digest = policy.identity()?;
    let original = serde_json::to_vec(&policy)?;
    assert!(
        policy
            .preview_change(
                digest,
                trim_change(&policy, PathScope::Exact, "overflow.rs")
            )
            .is_err()
    );
    assert_eq!(policy.identity()?, digest);
    assert_eq!(serde_json::to_vec(&policy)?, original);
    Ok(())
}

#[test]
fn first_trim_uses_epoch_one_and_rejects_unavailable_or_rebound_indexes() -> Result<()> {
    use crate::{
        index_catalog::{IndexDefinition, PolicyReference, SourceImportState},
        service::ProjectRegistration,
    };
    let template = fixture()?;
    let index = IndexDefinition {
        id: template.index_id,
        name: "Original fixture index".into(),
        backing: ProjectRegistration::new(
            template.roots[0].canonical_path.clone(),
            "/fixtures/index-data".into(),
            template.collection.clone(),
        )?,
        definition_revision: CatalogRevision::INITIAL,
        source_revision: CatalogRevision::INITIAL,
        policy: None,
        tombstone: false,
        source_import: SourceImportState::Complete,
        description: None,
    };
    let rule_id = DenyId::from_identity(Incarnation::from_bytes([42; 16]));
    let selector = ScopeAtom {
        source_root_id: template.roots[0].id,
        kind: PathScope::Subtree,
        path: "private".into(),
    };
    let first =
        SourcePolicy::initial_trim(&index, template.roots.clone(), rule_id, selector.clone())?;
    assert_eq!(first.epoch, CatalogRevision::INITIAL);
    assert_eq!(first.denies[0].created_epoch, CatalogRevision::INITIAL);
    assert_eq!(first.owner_binding, index.backing.binding);
    assert_eq!(first.index_id, index.id);
    assert_eq!(first.denies.len(), 1);
    assert!(!first.allows_canonical(selector.source_root_id, "private/new.rs"));
    for variant in 0..3 {
        let mut unavailable = index.clone();
        match variant {
            0 => unavailable.tombstone = true,
            1 => unavailable.source_import = SourceImportState::PendingMetadata,
            _ => {
                unavailable.policy = Some(PolicyReference {
                    epoch: first.epoch,
                    digest: first.identity()?,
                })
            }
        }
        assert!(
            SourcePolicy::initial_trim(
                &unavailable,
                template.roots.clone(),
                rule_id,
                selector.clone()
            )
            .is_err()
        );
    }
    let mut rebound = template.roots.clone();
    rebound[0].canonical_path = "/fixtures/other-source".into();
    assert!(SourcePolicy::initial_trim(&index, rebound, rule_id, selector.clone()).is_err());
    let mut unknown = selector;
    unknown.source_root_id = SourceRootId::from_identity(Incarnation::from_bytes([99; 16]));
    assert!(SourcePolicy::initial_trim(&index, template.roots.clone(), rule_id, unknown).is_err());
    Ok(())
}
