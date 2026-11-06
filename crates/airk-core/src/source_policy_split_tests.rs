// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::source_policy::{PolicyChange, SourceRoot};

fn fixtures() -> Result<serde_json::Value> {
    Ok(serde_json::from_str(&std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/research/contracts-v190/fixtures.json"
    ))?)?)
}
fn policy() -> Result<SourcePolicy> {
    Ok(serde_json::from_value(
        fixtures()?["policy_cases"][0]["value"].clone(),
    )?)
}
fn target(before: &SourcePolicy) -> SplitDestination {
    SplitDestination {
        index_id: IndexId::from_identity(Incarnation::from_bytes([211; 16])),
        owner_binding: Sha256Digest::from_bytes([212; 32]),
        collection: "split-fixture".into(),
        source_ids: before
            .roots
            .iter()
            .enumerate()
            .map(|(i, root)| {
                (
                    root.id,
                    SourceRootId::from_identity(Incarnation::from_bytes([220 + i as u8; 16])),
                )
            })
            .collect(),
    }
}
fn atom(root: SourceRootId, kind: PathScope, path: &str) -> ScopeAtom {
    ScopeAtom {
        source_root_id: root,
        kind,
        path: path.into(),
    }
}
fn compile(before: &SourcePolicy, selection: &[ScopeAtom]) -> Result<SplitPolicyPlan> {
    before.compile_split(
        before.identity()?,
        Incarnation::from_bytes([210; 16]),
        selection,
        target(before),
    )
}
fn assert_partition(before: &SourcePolicy, plan: &SplitPolicyPlan, paths: &[String]) {
    for root in &before.roots {
        let destination_root = plan.destination_binding.source_ids[&root.id];
        for path in paths {
            let old = before.allows_canonical(root.id, path);
            let selected = plan
                .selectors
                .iter()
                .any(|rule| rule.matches_canonical(root.id, path));
            let moved = plan.destination.allows_canonical(destination_root, path);
            let retained = plan.remainder.allows_canonical(root.id, path);
            assert_eq!(moved, old && selected, "destination {}/{path}", root.id);
            assert_eq!(retained, old && !selected, "remainder {}/{path}", root.id);
            assert!(!(moved && retained));
            assert_eq!(moved || retained, old);
        }
    }
}

#[test]
fn approved_partition_fixture_compiles_to_complete_independent_predicates() -> Result<()> {
    let before = policy()?;
    let original = serde_json::to_vec(&before)?;
    let fixture = &fixtures()?["partition"];
    let root: SourceRootId = serde_json::from_value(fixture["root_id"].clone())?;
    assert!(before.roots.iter().any(|source| source.id == root));
    let selected = fixture["selection"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            Ok(ScopeAtom {
                source_root_id: root,
                kind: serde_json::from_value(value["kind"].clone())?,
                path: value["path"].as_str().unwrap().into(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let plan = compile(&before, &selected)?;
    plan.validate_against(&before)?;
    assert_eq!(plan.remainder.index_id, before.index_id);
    assert_eq!(plan.remainder.owner_binding, before.owner_binding);
    assert_eq!(plan.remainder.collection, before.collection);
    assert_eq!(plan.remainder.roots, before.roots);
    assert_eq!(plan.remainder.positive_includes, before.positive_includes);
    assert_eq!(plan.remainder.denies[..before.denies.len()], before.denies);
    let mapped = plan.destination_binding.source_ids[&root];
    for name in ["expected_selected", "expected_remainder"] {
        for value in fixture[name].as_array().unwrap() {
            let path = value.as_str().unwrap();
            assert_eq!(
                plan.destination.allows_canonical(mapped, path),
                name == "expected_selected"
            );
            assert_eq!(
                plan.remainder.allows_canonical(root, path),
                name == "expected_remainder"
            );
        }
    }
    for value in fixture["new_paths"].as_array().unwrap() {
        assert_eq!(
            plan.destination
                .allows_canonical(mapped, value["path"].as_str().unwrap()),
            value["destination"].as_bool().unwrap()
        );
    }
    for (old, inherited) in before.denies.iter().zip(&plan.destination.denies) {
        assert_eq!(
            inherited.source_root_id,
            plan.destination_binding.source_ids[&old.source_root_id]
        );
        assert_eq!(inherited.reason, old.reason);
        assert_eq!(inherited.path, old.path);
        assert_eq!(inherited.kind, old.kind);
        assert_eq!(inherited.created_epoch, CatalogRevision::INITIAL);
        assert_eq!(
            inherited.inherited_from,
            Some(InheritedDeny {
                index_id: before.index_id,
                rule_id: old.rule_id,
                epoch: before.epoch
            })
        );
    }
    for transfer in &plan.remainder.denies[before.denies.len()..] {
        assert_eq!(transfer.reason, DenyReason::SplitTransfer);
        assert!(
            plan.remainder
                .preview_change(
                    plan.remainder.identity()?,
                    PolicyChange::Unblock {
                        rule_id: transfer.rule_id
                    }
                )
                .is_err()
        );
    }
    assert_eq!(serde_json::to_vec(&before)?, original);
    Ok(())
}

#[test]
fn fixed_seed_partition_properties_cover_positive_intersection_unicode_and_multiple_roots()
-> Result<()> {
    let mut seed = 0x9e37_79b9_u32;
    let mut draw = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let mut accepted = 0;
    for case in 0..512 {
        let mut before = policy()?;
        let root = before.roots[0].id;
        let second = SourceRootId::from_identity(Incarnation::from_bytes([209; 16]));
        before.roots.push(SourceRoot {
            id: second,
            canonical_path: "/independent-second-root".into(),
            aliases: vec!["second".into()],
            owner: false,
        });
        let pool = [
            atom(root, PathScope::Exact, "src/a.rs"),
            atom(root, PathScope::Subtree, "docs"),
            atom(root, PathScope::Subtree, "src"),
            atom(root, PathScope::Exact, "é/中.rs"),
            atom(root, PathScope::Subtree, "private"),
            atom(second, PathScope::Subtree, "docs"),
        ];
        if case % 3 != 0 {
            before.positive_includes = Some(
                (0..pool.len())
                    .filter(|_| draw() & 1 == 0)
                    .map(|at| pool[at].clone())
                    .collect(),
            );
        }
        let selectors = (0..pool.len())
            .filter(|_| draw() & 1 == 0)
            .map(|at| pool[at].clone())
            .collect::<Vec<_>>();
        let original = serde_json::to_vec(&before)?;
        let paths = [
            "src/a.rs",
            "src/a.rs.backup",
            "src/new.rs",
            "docs/spec.md",
            "docs/new.md",
            "private/key.rs",
            "private/new.rs",
            "private-other/public.rs",
            "é/中.rs",
            "é/中.rs/descendant",
            "unselected/future.rs",
        ]
        .map(str::to_owned);
        if let Ok(plan) = compile(&before, &selectors) {
            accepted += 1;
            assert_partition(&before, &plan, &paths);
            plan.validate_against(&before)?;
            let mut permuted = selectors.clone();
            permuted.reverse();
            permuted.extend(selectors.iter().take(2).cloned());
            assert_eq!(
                compile(&before, &permuted)?,
                plan,
                "canonical plan is independent of redundant selector ordering"
            );
        }
        assert_eq!(serde_json::to_vec(&before)?, original);
    }
    assert!(
        accepted > 100,
        "generated cases must exercise accepted partitions"
    );
    Ok(())
}

#[test]
fn split_rejects_empty_overflow_stale_crossed_mapping_and_modified_successors() -> Result<()> {
    let before = policy()?;
    let root = before.roots[0].id;
    assert!(compile(&before, &[]).is_err());
    assert!(compile(&before, &[atom(root, PathScope::Subtree, "private")]).is_err());
    let all = before
        .roots
        .iter()
        .map(|source| atom(source.id, PathScope::Subtree, ""))
        .collect::<Vec<_>>();
    assert!(compile(&before, &all).is_err());
    assert!(
        compile(
            &before,
            &vec![atom(root, PathScope::Subtree, "docs"); MAX_ATOMS + 1]
        )
        .is_err()
    );
    let selection = [atom(root, PathScope::Subtree, "docs")];
    assert!(
        before
            .compile_split(
                Sha256Digest::from_bytes([255; 32]),
                Incarnation::from_bytes([210; 16]),
                &selection,
                target(&before)
            )
            .is_err()
    );
    let mut crossed = target(&before);
    crossed.source_ids.clear();
    assert!(
        before
            .compile_split(
                before.identity()?,
                Incarnation::from_bytes([210; 16]),
                &selection,
                crossed
            )
            .is_err()
    );
    let mut exact_only = before.clone();
    exact_only.positive_includes = Some(vec![atom(root, PathScope::Exact, "docs/spec.md")]);
    assert!(compile(&exact_only, &selection).is_err());
    let mut full = before.clone();
    full.denies = (0..MAX_ATOMS)
        .map(|at| DenyRule {
            source_root_id: root,
            kind: PathScope::Exact,
            path: format!("omitted/{at}"),
            rule_id: DenyId::from_identity(Incarnation::from_bytes([at as u8; 16])),
            reason: DenyReason::Trim,
            created_epoch: full.epoch,
            inherited_from: None,
        })
        .collect();
    assert!(compile(&full, &selection).is_err());
    let plan = compile(&before, &selection)?;
    let mut changed = plan.clone();
    changed.remainder.denies.clear();
    assert!(changed.validate_against(&before).is_err());
    let mut changed = plan.clone();
    changed.destination.positive_includes = None;
    assert!(changed.validate_against(&before).is_err());
    let mut changed = plan;
    changed.destination.roots[0].canonical_path = "/replacement".into();
    assert!(changed.validate_against(&before).is_err());
    Ok(())
}
