// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    identity::{Incarnation, RevisionToken},
    index_catalog::{Membership, PathScope},
};
use serde_json::json;

fn project(n: u8) -> QueryProjectId {
    QueryProjectId::from_identity(Incarnation::from_bytes([n; 16]))
}
fn index(n: u8) -> IndexId {
    IndexId::from_identity(Incarnation::from_bytes([n; 16]))
}
fn root(n: u8) -> SourceRootId {
    SourceRootId::from_identity(Incarnation::from_bytes([n; 16]))
}
fn atom(path: &str, kind: PathScope) -> ScopeAtom {
    ScopeAtom {
        source_root_id: root(1),
        kind,
        path: path.into(),
    }
}
fn member(p: u8, i: u8, path: &str) -> GrantedMembership {
    GrantedMembership {
        membership: Membership {
            project_id: project(p),
            index_id: index(i),
            revision: CatalogRevision::INITIAL,
            scope: vec![atom(path, PathScope::Subtree)],
        },
        source_revision: CatalogRevision::INITIAL,
        policy_epoch: 0,
        policy_digest: None,
    }
}
fn grant(memberships: Vec<GrantedMembership>) -> ReviewedGrant {
    ReviewedGrant {
        id: GrantId::from_identity(Incarnation::from_bytes([1; 16])),
        host: Sha256Digest::from_bytes([2; 32]),
        revision: CatalogRevision::INITIAL,
        memberships,
        revoked: false,
    }
}
fn selection(project_ids: Vec<QueryProjectId>, index_ids: Option<Vec<IndexId>>) -> QuerySelection {
    QuerySelection {
        project_ids,
        index_ids,
        member_availability: MemberAvailability::Report,
    }
}

#[test]
fn recommendations_narrow_disjoint_prefixes_and_cover_all_decision_rows() -> Result<()> {
    use recommendations::{Situation, project as advise, select};
    let available = selection(vec![project(1), project(2)], None).resolve(&grant(vec![
        member(1, 1, "app"),
        member(2, 1, "shared"),
        member(2, 2, "specs"),
    ]))?;
    let selected = select(
        &available,
        Some(&selection(vec![project(1)], Some(vec![index(1)]))),
    )?;
    assert_eq!(
        selected.members[0].scope,
        vec![atom("app", PathScope::Subtree)]
    );
    assert!(
        select(
            &available,
            Some(&selection(vec![project(1)], Some(vec![index(2)])))
        )
        .is_err()
    );
    assert!(
        select(
            &available,
            Some(&selection(vec![project(3)], Some(vec![index(1)])))
        )
        .is_err()
    );
    for (situation, action) in [
        (Situation::Keep, "keep_layout"),
        (Situation::Reuse, "review_reuse"),
        (Situation::FocusedTask, "use_query_scope"),
        (Situation::SharedDependencies, "use_query_scope"),
        (Situation::DifferentLifecycles, "review_split"),
        (Situation::Overlap, "review_reuse"),
        (Situation::IrrelevantPath, "use_query_scope"),
        (Situation::NeverIndexPath, "removal_pending"),
    ] {
        let result = advise(&available, &selected, &[], &[], situation, 100)?;
        assert_eq!(result["cards"][0]["action"], action);
        let guidance = result["cards"][0]["tradeoff"].as_str().unwrap();
        assert!(!guidance.contains("until P10") && !guidance.contains("pending P10"));
        if matches!(
            situation,
            Situation::DifferentLifecycles | Situation::NeverIndexPath
        ) {
            assert!(guidance.contains("separate native review"));
            assert!(guidance.contains("Advice does not prepare or apply"));
        }
        assert_eq!(result["omitted_index_ids"], json!([index(2)]));
        assert_eq!(result["available"][0]["readiness"]["state"], "unknown");
        assert_eq!(result["limits"]["source_scans"], 0);
        assert_eq!(result["cards"].as_array().unwrap().len(), 3);
        assert_eq!(result["library_coverage"], "connection_only");
    }
    for invalid in [
        json!({"view":"projects","situation":"keep"}),
        json!({"view":"recommendations","selection":{"project_ids":[project(1)]}}),
        json!({"view":"recommendations","selection":{"project_ids":[project(1)],"index_ids":[]}}),
        json!({"view":"recommendations","project_id":project(1)}),
        json!({"view":"recommendations","cursor":"old"}),
        json!({"view":"recommendations","limit":1}),
        json!({"view":"recommendations","situation":null}),
    ] {
        assert!(
            serde_json::from_value::<query_scope::QueryScopeArgs>(invalid)
                .map_err(anyhow::Error::from)
                .and_then(|args| args.validate())
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn recommendations_match_query_selection_with_unused_selected_projects() -> Result<()> {
    let frozen = grant(vec![member(1, 1, "app"), member(2, 2, "specs")]);
    let available = selection(vec![project(1), project(2)], None).resolve(&frozen)?;
    let choice = selection(vec![project(1), project(2)], Some(vec![index(1)]));
    let expected = choice.resolve(&frozen)?;
    let selected = recommendations::select(&available, Some(&choice))?;
    assert_eq!(selected, expected);
    // The same valid selection can itself be the connection's frozen subset.
    recommendations::project(
        &expected,
        &expected,
        &[],
        &[],
        recommendations::Situation::Keep,
        100,
    )?;
    assert_eq!(selected.project_ids, vec![project(1), project(2)]);
    assert_eq!(selected.members.len(), 1);
    Ok(())
}
fn envelope(sequence: u64, revision: u8) -> RevisionEnvelope {
    RevisionEnvelope::new(
        "fixture".into(),
        RevisionToken {
            incarnation: Incarnation::from_bytes([1; 16]),
            generation: sequence,
            revision: Sha256Digest::from_bytes([revision; 32]),
        },
    )
}
fn vector(generations: [u64; 2]) -> PublicationVector {
    PublicationVector {
        authority: Sha256Digest::from_bytes([3; 32]),
        observed_from_unix_ms: 10,
        observed_to_unix_ms: 20,
        members: generations
            .into_iter()
            .enumerate()
            .map(|(n, sequence)| MemberPublication {
                index_id: index(n as u8 + 1),
                publication: Some(envelope(sequence, sequence as u8)),
            })
            .collect(),
    }
}

#[test]
fn closed_selectors_reject_mixed_legacy_null_empty_duplicates_and_unknowns() -> Result<()> {
    for invalid in [
        json!({"project":"a".repeat(64),"project_ids":[project(1)]}),
        json!({"index_ids":[index(1)]}),
        json!({"project_ids":[project(1)],"index_ids":null}),
        json!({"project_ids":[project(1)],"member_availability":"best_effort"}),
        json!({"project_ids":[project(1)],"unknown":true}),
    ] {
        assert!(serde_json::from_value::<QuerySelection>(invalid).is_err());
    }
    for invalid in [
        selection(vec![], None),
        selection(vec![project(1); 2], None),
        selection(vec![project(1)], Some(vec![])),
        selection(vec![project(1)], Some(vec![index(1); 2])),
        selection((0..33).map(project).collect(), None),
    ] {
        assert!(invalid.validate().is_err());
    }
    let snapshot = grant(vec![member(1, 1, "src"), member(2, 2, "docs")]);
    assert!(
        selection(vec![project(3)], None)
            .resolve(&snapshot)
            .is_err()
    );
    assert!(
        selection(vec![project(1)], Some(vec![index(2)]))
            .resolve(&snapshot)
            .is_err()
    );
    let mut revoked = snapshot;
    revoked.revoked = true;
    assert!(selection(vec![project(1)], None).resolve(&revoked).is_err());
    Ok(())
}

#[test]
fn shared_index_is_executed_once_with_precise_selected_membership_attribution() -> Result<()> {
    let snapshot = grant(vec![
        member(1, 1, "shared/api"),
        member(2, 1, "shared/impl"),
        member(3, 1, ""),
    ]);
    let resolved = selection(vec![project(2), project(1)], None).resolve(&snapshot)?;
    resolved.validate()?;
    assert_eq!(resolved.members.len(), 1);
    let shared = &resolved.members[0];
    assert_eq!(shared.project_ids(), vec![project(1), project(2)]);
    assert_eq!(shared.scope.len(), 2);
    assert_eq!(
        shared.projects_for(root(1), "shared/api/one.rs")?,
        vec![project(1)]
    );
    assert_eq!(
        shared.projects_for(root(1), "shared/impl/two.rs")?,
        vec![project(2)]
    );
    assert!(
        shared
            .projects_for(root(1), "shared/private.rs")?
            .is_empty()
    );
    assert!(
        shared
            .projects_for(root(1), "shared/api-other.rs")?
            .is_empty()
    );
    assert!(
        shared
            .projects_for(root(2), "shared/api/one.rs")?
            .is_empty()
    );
    assert!(
        shared
            .projects_for(root(1), "shared/api/../private.rs")
            .is_err()
    );
    Ok(())
}

#[test]
fn explicit_subset_does_not_import_project_defaults_or_unselected_members() -> Result<()> {
    let snapshot = grant(vec![
        member(1, 1, "one"),
        member(1, 2, "two"),
        member(2, 1, "private"),
    ]);
    let all = selection(vec![project(1)], None).resolve(&snapshot)?;
    assert_eq!(
        all.members.iter().map(|m| m.index_id).collect::<Vec<_>>(),
        vec![index(1), index(2)]
    );
    let subset = selection(vec![project(1)], Some(vec![index(2)])).resolve(&snapshot)?;
    assert_eq!(subset.members.len(), 1);
    assert_eq!(subset.members[0].index_id, index(2));
    assert_ne!(all.identity()?, subset.identity()?);
    Ok(())
}

#[test]
fn public_selection_observation_binds_full_authority_without_repeating_scope_predicates()
-> Result<()> {
    let mut resolved = selection(vec![project(1)], None)
        .resolve(&grant(vec![member(1, 1, "one"), member(1, 2, "two")]))?;
    let observation = resolved.observation()?;
    assert_eq!(observation.project_ids, vec![project(1)]);
    assert_eq!(observation.index_ids, vec![index(1), index(2)]);
    assert_eq!(observation.identity, resolved.identity()?);
    let encoded = serde_json::to_value(&observation)?;
    assert_eq!(encoded.as_object().unwrap().len(), 6);
    assert!(encoded.get("members").is_none());
    assert!(!encoded.to_string().contains("source_root_id"));
    resolved.members[0].memberships[0].membership.revision = CatalogRevision::INITIAL.next()?;
    assert_ne!(observation.identity, resolved.observation()?.identity);
    let before = resolved.observation()?;
    resolved.members[0].memberships[0].source_revision = CatalogRevision::INITIAL.next()?;
    assert_ne!(before.identity, resolved.observation()?.identity);
    Ok(())
}

#[test]
fn canonical_union_preserves_exact_component_and_root_boundaries() -> Result<()> {
    let mut other_root = atom("src", PathScope::Exact);
    other_root.source_root_id = root(2);
    let union = canonical_scope_union([
        atom("src/one", PathScope::Exact),
        atom("src", PathScope::Subtree),
        atom("src-other", PathScope::Exact),
        atom("src", PathScope::Subtree),
        other_root.clone(),
    ])?;
    assert_eq!(union.len(), 3);
    assert!(union.contains(&other_root));
    assert!(union.contains(&atom("src-other", PathScope::Exact)));
    let exact = canonical_scope_union([
        atom("file", PathScope::Exact),
        atom("file/child", PathScope::Exact),
    ])?;
    assert_eq!(exact.len(), 2);
    assert!(canonical_scope_union([atom("", PathScope::Exact)]).is_err());
    assert!(canonical_scope_union((0..4097).map(|_| atom("src", PathScope::Subtree))).is_err());
    Ok(())
}

#[test]
fn generated_utf8_unions_match_exactly_the_input_positive_predicate() -> Result<()> {
    // Fixed-seed, bounded original scopes. The predicate, not the implementation
    // representation, establishes no scope broadening or dropped permissions.
    let mut seed = 0x1a1c_7007_u64;
    for _ in 0..64 {
        let mut input = Vec::new();
        for _ in 0..16 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let root_id = root(((seed >> 8) % 3) as u8);
            let n = seed % 8;
            input.push(ScopeAtom {
                source_root_id: root_id,
                kind: if seed & 1 == 0 {
                    PathScope::Exact
                } else {
                    PathScope::Subtree
                },
                path: format!("café/δ{n}"),
            });
        }
        let union = canonical_scope_union(input.clone())?;
        for r in 0..3 {
            for n in 0..10 {
                for suffix in ["", "/child", "-other"] {
                    let path = format!("café/δ{n}{suffix}");
                    assert_eq!(
                        input.iter().any(|a| a.matches_canonical(root(r), &path)),
                        union.iter().any(|a| a.matches_canonical(root(r), &path))
                    );
                }
            }
        }
        assert_eq!(union, canonical_scope_union(union.clone())?);
        let mut reversed = input;
        reversed.reverse();
        assert_eq!(union, canonical_scope_union(reversed)?);
    }
    Ok(())
}

#[test]
fn all_shared_allocations_sum_to_one_total_in_canonical_index_order() -> Result<()> {
    for count in 1..=32_u8 {
        let snapshot = grant((1..=count).rev().map(|n| member(1, n, "src")).collect());
        let resolved = selection(vec![project(1)], None).resolve(&snapshot)?;
        for limit in [1, 2, 31, 32, 33, 100] {
            let budgets = SharedBudgets {
                limit,
                max_bytes: 101,
                deadline_ms: 97,
                verification_max_sources: 8,
                verification_max_bytes: 103,
                verification_deadline_ms: 89,
            };
            let allocated = budgets.allocate(&resolved)?;
            assert_eq!(allocated.iter().map(|a| a.limit).sum::<usize>(), limit);
            assert_eq!(allocated.iter().map(|a| a.max_bytes).sum::<usize>(), 101);
            assert_eq!(allocated.iter().map(|a| a.deadline_ms).sum::<u64>(), 97);
            assert_eq!(
                allocated
                    .iter()
                    .map(|a| a.verification_max_sources)
                    .sum::<usize>(),
                8
            );
            assert_eq!(
                allocated
                    .iter()
                    .map(|a| a.verification_max_bytes)
                    .sum::<u64>(),
                103
            );
            assert_eq!(
                allocated
                    .iter()
                    .map(|a| a.verification_deadline_ms)
                    .sum::<u64>(),
                89
            );
            assert!(
                allocated
                    .windows(2)
                    .all(|p| p[0].index_id < p[1].index_id && p[0].limit >= p[1].limit)
            );
        }
    }
    Ok(())
}

#[test]
fn publication_vectors_compare_only_the_same_authority_and_selected_set() {
    use PublicationRelation::*;
    assert_eq!(vector([1, 2]).compare(&vector([1, 2])), Equal);
    assert_eq!(vector([1, 2]).compare(&vector([2, 3])), Older);
    assert_eq!(vector([3, 4]).compare(&vector([2, 3])), Newer);
    assert_eq!(vector([1, 4]).compare(&vector([2, 3])), Incomparable);
    let mut foreign = vector([1, 2]);
    foreign.authority = Sha256Digest::from_bytes([8; 32]);
    assert_eq!(foreign.compare(&vector([1, 2])), Incomparable);
    let mut subset = vector([1, 2]);
    subset.members.pop();
    assert_eq!(subset.compare(&vector([1, 2])), Incomparable);
    let mut missing = vector([1, 2]);
    missing.members[0].publication = None;
    assert_eq!(missing.compare(&vector([1, 2])), Unknown);
    let mut fork = vector([1, 2]);
    fork.members[0].publication = Some(envelope(1, 99));
    assert_eq!(fork.compare(&vector([1, 2])), Inconsistent);
    let mut corrupt = vector([1, 2]);
    corrupt.members[0].publication.as_mut().unwrap().generation = 99;
    assert_eq!(corrupt.compare(&vector([1, 2])), Unknown);
    let mut incarnation = vector([1, 2]);
    let p = incarnation.members[0].publication.as_mut().unwrap();
    p.incarnation = Incarnation::from_bytes([7; 16]);
    p.token.incarnation = p.incarnation;
    assert_eq!(incarnation.compare(&vector([1, 2])), Incomparable);
}

#[test]
fn cursor_identity_binds_publications_scope_controls_positions_but_not_observation_dates()
-> Result<()> {
    let resolved = selection(vec![project(1)], None)
        .resolve(&grant(vec![member(1, 1, "one"), member(1, 2, "two")]))?;
    let base = vector([1, 2]);
    let controls = json!({"mode":"lexical","limit":8,"reply_detail":"lean"});
    let positions = BTreeMap::from([(index(1), 3), (index(2), 4)]);
    let identity = continuation_identity(&base, &resolved, &controls, &positions)?;
    let mut dated = base.clone();
    dated.observed_from_unix_ms = 100;
    dated.observed_to_unix_ms = 200;
    assert_eq!(
        identity,
        continuation_identity(&dated, &resolved, &controls, &positions)?
    );
    assert_ne!(
        identity,
        continuation_identity(&vector([1, 3]), &resolved, &controls, &positions)?
    );
    assert_ne!(
        identity,
        continuation_identity(&base, &resolved, &json!({"mode":"vector"}), &positions)?
    );
    assert_ne!(
        identity,
        continuation_identity(
            &base,
            &resolved,
            &controls,
            &BTreeMap::from([(index(1), 4), (index(2), 4)])
        )?
    );
    let mut narrowed = resolved.clone();
    narrowed.members[0].memberships[0].membership.scope[0].path = "one/private".into();
    narrowed.members[0].scope = narrowed.members[0].memberships[0].membership.scope.clone();
    assert_ne!(
        identity,
        continuation_identity(&base, &narrowed, &controls, &positions)?
    );
    let mut subset = base;
    subset.members.pop();
    assert!(continuation_identity(&subset, &resolved, &controls, &positions).is_err());
    assert_eq!(rank_contribution(0), 1.0 / 61.0);
    assert_eq!(rank_contribution(1), 1.0 / 62.0);
    Ok(())
}
