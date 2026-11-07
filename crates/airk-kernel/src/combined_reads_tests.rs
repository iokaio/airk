// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    combined_reads::{MemberAvailability, QuerySelection},
    identity::Incarnation,
    index_catalog::{
        CatalogRevision, GrantId, GrantedMembership, Membership, PathScope, QueryProjectId,
        ReviewedGrant, ScopeAtom,
    },
};

fn index(n: u8) -> IndexId {
    IndexId::from_identity(Incarnation::from_bytes([n; 16]))
}
fn member_result(result: &CombinedReadResult, index: IndexId) -> Result<Value> {
    airk_core::combined_reads::shared_values::materialize_combined_member_result(
        &serde_json::to_value(result)?,
        index,
    )
}
fn project(n: u8) -> QueryProjectId {
    QueryProjectId::from_identity(Incarnation::from_bytes([n; 16]))
}
fn root(n: u8) -> SourceRootId {
    SourceRootId::from_identity(Incarnation::from_bytes([n; 16]))
}
fn revision(n: u64) -> RevisionEnvelope {
    RevisionEnvelope::new(
        "fixture".into(),
        RevisionToken {
            incarnation: Incarnation::from_bytes([1; 16]),
            generation: n,
            revision: Sha256Digest::from_bytes([n as u8; 32]),
        },
    )
}
fn request() -> Result<CompositionRequest> {
    let grant = ReviewedGrant {
        id: GrantId::from_identity(Incarnation::from_bytes([1; 16])),
        host: Sha256Digest::from_bytes([1; 32]),
        revision: CatalogRevision::INITIAL,
        revoked: false,
        memberships: [(1, 1, "api"), (2, 1, "impl"), (1, 2, "api")]
            .into_iter()
            .map(|(p, i, path)| GrantedMembership {
                membership: Membership {
                    project_id: project(p),
                    index_id: index(i),
                    revision: CatalogRevision::INITIAL,
                    scope: vec![ScopeAtom {
                        source_root_id: root(1),
                        kind: PathScope::Subtree,
                        path: path.into(),
                    }],
                },
                source_revision: CatalogRevision::INITIAL,
                policy_epoch: 0,
                policy_digest: None,
            })
            .collect(),
    };
    Ok(CompositionRequest {
        selection: QuerySelection {
            project_ids: vec![project(1), project(2)],
            index_ids: None,
            member_availability: MemberAvailability::Report,
        }
        .resolve(&grant)?,
        budgets: SharedBudgets {
            limit: 4,
            max_bytes: 8192,
            deadline_ms: 2000,
            verification_max_sources: 8,
            verification_max_bytes: 16384,
            verification_deadline_ms: 2000,
        },
        authority: Sha256Digest::from_bytes([7; 32]),
        observed_from_unix_ms: 10,
        observed_to_unix_ms: 20,
    })
}
fn sources() -> Result<Vec<MemberSource>> {
    Ok(vec![MemberSource {
        source_root_id: root(1),
        original_root: std::env::current_dir()?.to_string_lossy().into_owned(),
        primary: true,
        aliases: vec![],
    }])
}
fn hit(path: &str, text: &str) -> Value {
    json!({"citation":{"path":path,"content_hash":"a".repeat(64),"start_line":1,"end_line":1,
        "location":{"Lines":{"start":1,"end":1}}},"text":text,"truncated":false})
}
fn input(i: u8, hits: Vec<Value>) -> Result<MemberInput> {
    Ok(MemberInput {
        is_error: false,
        exhausted: false,
        index_id: index(i),
        sources: sources()?,
        result: json!({"publication":revision(i as u64),"collection":"fixture",
        "hits":hits,"truncated":false,"search_execution":{"served_mode":"lexical","incomplete":false},
        "source_verification":{"full_native_receipt":{"references":[1,2],"dated":true}}}),
    })
}

#[test]
fn single_required_member_failure_exposes_exact_safe_read_retry_without_losing_native_data()
-> Result<()> {
    use airk_core::combined_reads::read_retry::validated_retry;
    let mut composition = request()?;
    composition.selection.members.truncate(1);
    composition.selection.member_availability = MemberAvailability::Require;
    let hint = json!({"status":"retry","code":"selection_busy","retryable":true,
        "action":"repeat_same_request","retry_after_ms":25});
    let failed = MemberInput {
        index_id: index(1),
        is_error: true,
        exhausted: false,
        result: hint.clone(),
        sources: vec![],
    };
    let result = compose(&composition, vec![failed.clone()])?;
    let value = serde_json::to_value(&result)?;
    let retry =
        validated_retry(&value).context("single safe member lost its explicit retry contract")?;
    assert_eq!(retry.code, "selection_busy");
    assert_eq!(retry.selection_identity, result.selection.identity);
    assert_eq!(value.pointer(&retry.result_pointer), Some(&hint));
    assert_eq!(result.members[0].result, hint);
    assert_eq!(result.error.as_ref().unwrap().code, "member_unavailable");
    assert!(
        !result.error.as_ref().unwrap().retryable,
        "generic whole-union replay stays disabled"
    );
    for (field, value) in [
        ("native_detail", json!({"preserved":[1,2,3]})),
        ("chunks", json!(1)),
        ("symbols", json!([])),
    ] {
        let mut unknown = failed.clone();
        unknown.result[field] = value;
        let original = unknown.result.clone();
        let blocked = compose(&composition, vec![unknown])?;
        assert!(
            blocked.read_retry.is_none(),
            "unknown member fields cannot confer replay authority"
        );
        assert_eq!(
            blocked.members[0].result, original,
            "the full original failure remains available"
        );
    }
    composition.selection.member_availability = MemberAvailability::Report;
    assert!(
        compose(&composition, vec![failed.clone()])?
            .read_retry
            .is_none()
    );
    let mut multiple = request()?;
    multiple.selection.member_availability = MemberAvailability::Require;
    assert!(
        compose(&multiple, vec![failed, input(2, vec![])?])?
            .read_retry
            .is_none()
    );
    Ok(())
}

#[test]
fn shared_index_rank_is_one_contribution_with_precise_origins_and_complete_native_results()
-> Result<()> {
    let request = request()?;
    let first = input(1, vec![hit("api/one.rs", "one"), hit("impl/two.rs", "two")])?;
    let second = input(2, vec![hit("api/one.rs", "one")])?;
    let expected1 = first.result.clone();
    let expected2 = second.result.clone();
    let combined = compose(&request, vec![second, first])?;
    assert_eq!(combined.members.len(), 2);
    assert_eq!(combined.ranking.len(), 3);
    assert_eq!(member_result(&combined, index(1))?, expected1);
    assert_eq!(member_result(&combined, index(2))?, expected2);
    assert_eq!(combined.ranking[0].index_id, index(1));
    assert_eq!(combined.ranking[1].index_id, index(2));
    assert_eq!(combined.ranking[0].rank_contribution, 1.0 / 61.0);
    assert_eq!(combined.ranking[2].rank_contribution, 1.0 / 62.0);
    assert_eq!(combined.ranking[0].origin.project_ids, vec![project(1)]);
    assert_eq!(combined.ranking[2].origin.project_ids, vec![project(2)]);
    let value = serde_json::to_value(&combined)?;
    for ranking in &combined.ranking {
        assert!(value.pointer(&ranking.result_pointer).is_some());
        assert_eq!(
            airk_core::combined_reads::materialize_ranked_origin(
                &value,
                &serde_json::to_value(ranking)?
            )?
            .publication,
            revision(if ranking.index_id == index(1) { 1 } else { 2 })
        );
    }
    assert!(!combined.partial);
    assert!(!combined.truncated);
    assert!(combined.error.is_none());
    assert!(value.get("publication").is_none());
    Ok(())
}

#[test]
fn failed_and_unqueried_members_keep_complete_native_retry_data_in_report_and_require() -> Result<()>
{
    let mut request = request()?;
    let failed = MemberInput {
        is_error: false,
        exhausted: false,
        index_id: index(2),
        sources: vec![],
        result: json!({"isError":true,
        "content":[{"type":"text","text":"helper unavailable"}],"structuredContent":{"error":{"code":"helper_unavailable",
        "retry":{"fresh_brokerage_required":true,"query_replay_safe":false},"custom_nested_detail":[1,2,3]},
        "publication":revision(2)}}),
    };
    let original = failed.result.clone();
    let result = compose(
        &request,
        vec![input(1, vec![hit("api/one.rs", "one")])?, failed.clone()],
    )?;
    assert!(result.partial);
    assert!(result.error.is_none());
    assert_eq!(result.members[1].result, original);
    assert_eq!(result.members[1].outcome, MemberOutcome::Failed);
    assert_eq!(
        result.publication_vector.members[1].publication,
        Some(revision(2))
    );
    request.selection.member_availability = MemberAvailability::Require;
    let result = compose(
        &request,
        vec![input(1, vec![hit("api/one.rs", "one")])?, failed],
    )?;
    assert_eq!(result.error.unwrap().code, "member_unavailable");
    let result = compose(&request, vec![])?;
    assert!(result.partial && result.truncated);
    assert_eq!(result.members.len(), 2);
    assert!(
        result
            .members
            .iter()
            .all(|m| m.outcome == MemberOutcome::Unqueried && m.result.is_null())
    );
    assert_eq!(
        result.error.unwrap().failed_members,
        vec![index(1), index(2)]
    );
    Ok(())
}

#[test]
fn forged_unknown_duplicate_out_of_scope_and_overbudget_inputs_fail_closed() -> Result<()> {
    let request = request()?;
    let mut foreign = input(1, vec![])?;
    foreign.index_id = index(3);
    assert!(compose(&request, vec![foreign]).is_err());
    let first = input(1, vec![])?;
    assert!(compose(&request, vec![first.clone(), first]).is_err());
    assert!(
        compose(
            &request,
            vec![input(1, vec![hit("api-other/secret.rs", "private")])?]
        )
        .is_err()
    );
    assert!(
        compose(
            &request,
            vec![input(1, vec![hit("api/../secret.rs", "private")])?]
        )
        .is_err()
    );
    let mut missing = input(1, vec![hit("api/one.rs", "one")])?;
    missing.sources.clear();
    assert!(compose(&request, vec![missing]).is_err());
    assert!(
        compose(
            &request,
            vec![input(
                1,
                vec![
                    hit("api/one.rs", "x"),
                    hit("api/two.rs", "x"),
                    hit("api/three.rs", "x")
                ]
            )?]
        )
        .is_err()
    );
    assert!(
        compose(
            &request,
            vec![input(1, vec![hit("api/one.rs", &"x".repeat(4097))])?]
        )
        .is_err()
    );
    let mut zero = request;
    zero.budgets.limit = 1;
    assert!(compose(&zero, vec![input(2, vec![])?]).is_err());
    Ok(())
}

#[test]
fn source_projection_preserves_external_alias_namespace_and_root_identity() -> Result<()> {
    let mut request = request()?;
    let member = &mut request.selection.members[0];
    member.memberships[0].membership.scope.push(ScopeAtom {
        source_root_id: root(2),
        kind: PathScope::Subtree,
        path: "api".into(),
    });
    member.scope = airk_core::combined_reads::canonical_scope_union(
        member
            .memberships
            .iter()
            .flat_map(|membership| membership.membership.scope.iter().cloned()),
    )?;
    let mut external = input(1, vec![hit("@docs/api/one.md", "external")])?;
    external.sources.push(MemberSource {
        source_root_id: root(2),
        original_root: std::env::current_dir()?
            .join("external")
            .to_string_lossy()
            .into_owned(),
        primary: false,
        aliases: vec!["docs".into()],
    });
    let result = compose(&request, vec![external.clone()])?;
    assert_eq!(result.ranking[0].origin.source_root_id, root(2));
    assert_eq!(result.ranking[0].origin.source_relative_path, "api/one.md");
    assert_eq!(result.ranking[0].origin.project_ids, vec![project(1)]);
    external.result["hits"][0] = hit("@docs/api-other/private.md", "private");
    assert!(compose(&request, vec![external.clone()]).is_err());
    external.result["hits"][0] = hit("@docs-other/api/one.md", "private");
    assert!(compose(&request, vec![external]).is_err());
    Ok(())
}

#[test]
fn actual_projection_shapes_have_qualified_real_pointer_targets() -> Result<()> {
    let request = request()?;
    let cases = [
        json!({"items":[{"citation":{"path":"api/one.rs"},"name":"one","kind":"function"}]}),
        json!({"facts":[{"citation":{"path":"api/one.rs"},"name":"one"}]}),
        json!({"files":[{"path":"api/one.rs","content_hash":"a".repeat(64)}]}),
        json!({"files":["api/one.rs"]}),
        json!({"paths":["api/one.rs"]}),
        json!({"evidence":{"publication":revision(1),"hits":[hit("api/one.rs","one")],"truncated":false},"page":{"more":false}}),
        json!({"representation":"minimal","items":[{"file":0,"name":"one","lines":[1,1]}],
            "files":[{"path":"api/one.rs","content_hash":"a".repeat(64)}],"provenance":[{"basis":"syntax"}]}),
        json!({"minimal":{"items":[{"name":"one","lines":[1,1]}],"files":[{"path":"api/one.rs"}],"provenance":[]},"facts":[]}),
    ];
    for mut body in cases {
        body["publication"] = serde_json::to_value(revision(1))?;
        let combined = compose(
            &request,
            vec![MemberInput {
                is_error: false,
                exhausted: false,
                index_id: index(1),
                sources: sources()?,
                result: body.clone(),
            }],
        )?;
        assert_eq!(combined.ranking.len(), 1, "{body}");
        let value = serde_json::to_value(&combined)?;
        assert!(
            value.pointer(&combined.ranking[0].result_pointer).is_some(),
            "{value}"
        );
    }
    Ok(())
}

#[test]
fn textual_native_envelope_is_materialized_without_losing_original_transport_data() -> Result<()> {
    let request = request()?;
    let body = input(1, vec![hit("api/one.rs", "one")])?.result;
    let original = json!({"content":[{"type":"text","text":body.to_string()}],"isError":false,"transport_identity":"retained"});
    let result = compose(
        &request,
        vec![MemberInput {
            is_error: false,
            exhausted: false,
            index_id: index(1),
            sources: sources()?,
            result: original.clone(),
        }],
    )?;
    assert_eq!(result.members[0].result["content"], original["content"]);
    assert_eq!(result.members[0].result["structuredContent"], body);
    assert_eq!(result.members[0].result["transport_identity"], "retained");
    assert_eq!(result.ranking.len(), 1);
    let bad = MemberInput {
        is_error: false,
        exhausted: false,
        index_id: index(1),
        sources: sources()?,
        result: json!({"content":[{"type":"text","text":"not JSON evidence"}],"isError":false}),
    };
    assert!(compose(&request, vec![bad]).is_err());
    Ok(())
}

#[test]
fn framing_excess_is_typed_and_never_silently_drops_member_observations() -> Result<()> {
    let request = request()?;
    let mut first = input(1, vec![])?;
    first.result["large_native_receipt"] = Value::String("a".repeat(34_000));
    let mut second = input(2, vec![])?;
    second.result["large_native_receipt"] = Value::String("b".repeat(34_000));
    let error = compose(&request, vec![first, second]).unwrap_err();
    assert!(error.is::<CombinedReplyBudget>(), "{error:?}");
    assert_eq!(
        CombinedReplyBudget.receipt()["error"]["query_replay_safe"],
        false
    );
    Ok(())
}

#[test]
fn exhausted_cursor_members_retain_exact_native_observations_without_false_partial() -> Result<()> {
    let mut request = request()?;
    request.budgets.limit = 1;
    let exhausted = MemberInput {
        is_error: false,
        exhausted: true,
        index_id: index(2),
        sources: vec![],
        result: json!({"publication":revision(2),"files":17,"index_state":"available","observed_unix_ms":20}),
    };
    let result = compose(
        &request,
        vec![input(1, vec![hit("api/one.rs", "one")])?, exhausted],
    )?;
    assert!(!result.partial);
    assert_eq!(result.ranking.len(), 1);
    assert_eq!(result.members[1].outcome, MemberOutcome::Exhausted);
    assert_eq!(
        result.members[1].cutoff.as_deref(),
        Some("continuation_exhausted")
    );
    assert_eq!(
        result.publication_vector.members[1].publication,
        Some(revision(2))
    );
    assert_eq!(result.members[1].allocation.limit, 0);
    let mut failure = input(1, vec![])?;
    failure.is_error = true;
    failure.result =
        json!({"content":[{"type":"text","text":"non JSON native failure"}],"isError":true});
    let result = compose(&request, vec![failure])?;
    assert!(result.members[0].is_error);
    assert_eq!(result.members[0].outcome, MemberOutcome::Failed);
    Ok(())
}

#[test]
fn thirty_two_selected_indexes_with_one_result_fit_the_unchanged_native_frame() -> Result<()> {
    let mut request = request()?;
    let first = request.selection.members[0].memberships[0].clone();
    let grant = ReviewedGrant {
        id: request.selection.grant_id,
        host: Sha256Digest::from_bytes([1; 32]),
        revision: request.selection.grant_revision,
        revoked: false,
        memberships: (1..=32)
            .map(|n| {
                let mut member = first.clone();
                member.membership.index_id = index(n);
                member
            })
            .collect(),
    };
    request.selection = QuerySelection {
        project_ids: vec![project(1)],
        index_ids: None,
        member_availability: MemberAvailability::Report,
    }
    .resolve(&grant)?;
    request.budgets.limit = 1;
    let original = input(1, vec![hit("api/one.rs", "one")])?;
    let body = original.result.clone();
    let result = compose(&request, vec![original])?;
    assert_eq!(result.members.len(), 32);
    assert_eq!(result.ranking.len(), 1);
    assert_eq!(member_result(&result, index(1))?, body);
    assert!(
        result.members[1..]
            .iter()
            .all(|member| member.outcome == MemberOutcome::Unqueried)
    );
    assert_eq!(result.selection.index_ids.len(), 32);
    assert_eq!(result.selection.identity, request.selection.identity()?);
    let value = serde_json::to_value(&result)?;
    let envelope = json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false});
    airk_core::index_catalog::encoded_size(
        &envelope,
        airk_core::MAX_RESPONSE_BYTES.saturating_sub(2048),
    )?;
    Ok(())
}

#[test]
fn conditional_file_info_qualifies_its_exact_metadata_root_without_inventing_locations()
-> Result<()> {
    let mut request = request()?;
    request.selection.members.truncate(1);
    let body = json!({"read":{"publication":revision(1),"current_publication":revision(1),"status":"content"},
        "path":"api/one.rs","content_hash":"a".repeat(64),
        "source":{"root":std::env::current_dir()?.to_string_lossy(),"relative_path":"api/one.rs"},
        "source_bytes":12,"language":"rust","line_basis":"source"});
    for envelope in [false, true] {
        let result = if envelope {
            json!({"structuredContent":body,"content":[{"type":"text","text":body.to_string()}],"isError":false})
        } else {
            body.clone()
        };
        let result = compose(
            &request,
            vec![MemberInput {
                index_id: index(1),
                is_error: false,
                exhausted: false,
                result,
                sources: sources()?,
            }],
        )?;
        assert_eq!(result.ranking.len(), 1);
        let ranked = &result.ranking[0];
        assert_eq!(ranked.origin.source_root_id, root(1));
        assert_eq!(ranked.origin.project_ids, vec![project(1)]);
        assert_eq!(ranked.origin.source_relative_path, "api/one.rs");
        assert_eq!(
            airk_core::combined_reads::materialize_ranked_origin(
                &serde_json::to_value(&result)?,
                &serde_json::to_value(ranked)?
            )?
            .publication,
            revision(1)
        );
        assert_eq!(
            ranked.result_pointer,
            if envelope {
                "/members/0/result/structuredContent"
            } else {
                "/members/0/result"
            }
        );
        let value = serde_json::to_value(&result)?;
        assert!(value.pointer(&ranked.result_pointer).is_some());
        let logical = member_result(&result, index(1))?;
        let metadata = if envelope {
            logical.get("structuredContent").unwrap()
        } else {
            &logical
        };
        assert_eq!(metadata, &body);
        assert!(metadata.get("citation").is_none());
        assert!(metadata.get("location").is_none());
    }
    Ok(())
}

#[test]
fn capability_common_fields_roundtrip_one_two_and_thirty_two_complete_members() -> Result<()> {
    use airk_core::combined_reads::shared_values::materialize_combined_member_result;
    for count in [1_u8, 2, 32] {
        let mut request = request()?;
        let first = request.selection.members[0].memberships[0].clone();
        let grant = ReviewedGrant {
            id: request.selection.grant_id,
            host: Sha256Digest::from_bytes([1; 32]),
            revision: request.selection.grant_revision,
            revoked: false,
            memberships: (1..=count)
                .map(|n| {
                    let mut member = first.clone();
                    member.membership.index_id = index(n);
                    member
                })
                .collect(),
        };
        request.selection = QuerySelection {
            project_ids: vec![project(1)],
            index_ids: None,
            member_availability: MemberAvailability::Report,
        }
        .resolve(&grant)?;
        request.budgets.limit = count as usize;
        let bodies=(1..=count).map(|n|json!({"workbench":{"operations":["search","symbols"],"documentation":"x".repeat(3000)},
            "adapters":[{"id":"rust"}],"history":{"supported":false},"request_admission":{"limit":16},
            "runtime":{"worker":n},"scope_observation":{"index_id":index(n)}})).collect::<Vec<_>>();
        let inputs = bodies
            .iter()
            .enumerate()
            .map(|(offset, body)| MemberInput {
                index_id: index(offset as u8 + 1),
                is_error: false,
                exhausted: false,
                result: body.clone(),
                sources: vec![],
            })
            .collect();
        let response = compose_capabilities(&request, inputs)?;
        assert_eq!(response.members.len(), count as usize);
        assert!(response.shared_values.is_some());
        let mut value = serde_json::to_value(&response)?;
        value["workbench"] = json!({"combined":true,"operations":["search","query_scope"]});
        for (offset, body) in bodies.iter().enumerate() {
            assert_eq!(
                materialize_combined_member_result(&value, index(offset as u8 + 1))?,
                *body
            );
            assert_eq!(
                value["members"][offset]["result"]["runtime"],
                body["runtime"]
            );
            assert_eq!(
                value["members"][offset]["result"]["request_admission"],
                body["request_admission"]
            );
        }
        let envelope = json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false});
        airk_core::index_catalog::encoded_size(
            &envelope,
            airk_core::MAX_RESPONSE_BYTES.saturating_sub(2048),
        )?;
    }
    Ok(())
}

#[test]
fn capability_sharing_keeps_failed_member_and_distinct_fields_and_other_reads_inline() -> Result<()>
{
    use airk_core::combined_reads::shared_values::materialize_combined_member_result;
    let request = request()?;
    let first = json!({"workbench":{"operations":["search"]},"adapters":[{"id":"rust"}],"runtime":{"worker":1}});
    let failure =
        json!({"error":{"code":"native_capability_unavailable","retry":{"details":[1,2,3]}}});
    let inputs = vec![
        MemberInput {
            index_id: index(1),
            is_error: false,
            exhausted: false,
            result: first.clone(),
            sources: vec![],
        },
        MemberInput {
            index_id: index(2),
            is_error: true,
            exhausted: false,
            result: failure.clone(),
            sources: vec![],
        },
    ];
    let ordinary = compose(&request, inputs.clone())?;
    assert!(ordinary.shared_values.is_none());
    assert_eq!(ordinary.members[0].result, first);
    let result = compose_capabilities(&request, inputs)?;
    assert_eq!(result.members[1].result, failure);
    assert!(result.members[1].is_error);
    let value = serde_json::to_value(result)?;
    assert_eq!(materialize_combined_member_result(&value, index(1))?, first);
    assert_eq!(
        materialize_combined_member_result(&value, index(2))?,
        failure
    );
    Ok(())
}

#[test]
fn thirty_two_full_native_shaped_search_members_fit_and_materialize_without_dropping_evidence()
-> Result<()> {
    use airk_core::combined_reads::{
        materialize_combined_read, shared_values::materialize_combined_member_result,
    };
    let mut request = request()?;
    let seed = request.selection.members[0].memberships[0].clone();
    let mut memberships = (1..=32)
        .map(|n| {
            let mut member = seed.clone();
            member.membership.index_id = index(n);
            member
        })
        .collect::<Vec<_>>();
    for n in [1, 3] {
        let mut member = seed.clone();
        member.membership.index_id = index(n);
        member.membership.project_id = project(2);
        memberships.push(member);
    }
    let grant = ReviewedGrant {
        id: request.selection.grant_id,
        host: Sha256Digest::from_bytes([1; 32]),
        revision: request.selection.grant_revision,
        revoked: false,
        memberships,
    };
    request.selection = QuerySelection {
        project_ids: vec![project(1), project(2)],
        index_ids: None,
        member_availability: MemberAvailability::Report,
    }
    .resolve(&grant)?;
    request.budgets.limit = 32;
    request.budgets.max_bytes = 2048;
    let mut originals = Vec::new();
    let mut inputs = Vec::new();
    for n in 1..=32_u8 {
        let mut publication = revision(n as u64);
        publication.collection = format!("combined{n}");
        let selected = request.selection.member(index(n)).unwrap();
        let mut body = json!({"collection":publication.collection,"generation":publication.generation,"publication":publication,
            "request_id":format!("{n:032x}"),"execution":{"version":1,"requested_mode":"lexical","served_mode":"lexical","incomplete":false},
            "freshness":{"completed_watermark":null,"error":"scoped_freshness_unavailable","last_success_unix_ms":null,"live_source_verified":false,
                "observed_unix_ms":1793838000000_u64+u64::from(n),"requested_watermark":null,"running_watermark":null,"state":"unavailable"},
            "scope_observation":{"schema":1,"qualifier":"selected_subset","fingerprint":format!("{n:064x}"),"provenance":"user_reviewed_project_grant",
                "grant":{"id":grant.id,"revision":grant.revision},"members":[{"index_id":index(n),
                    "memberships":selected.memberships.iter().map(|member|json!({"membership_revision":member.membership.revision,"project_id":member.membership.project_id})).collect::<Vec<_>>(),
                    "policy_digest":null,"policy_epoch":0,"source_revision":1,"source_root_ids":[root(1)]}]},
            "hits":[hit("api/policy.md","# Original boundary\nKeep the saved source scope.\n")],"truncated":false});
        if n == 3 {
            body["discovery_readiness"] = json!({"committed_artifacts":4,"epoch":"a".repeat(32),"failure":null,"processed_artifacts":4,"state":"ready","total_artifacts":4,"version":1});
            body["semantic_readiness"] = json!({"completed_artifacts":0,"completed_units":0,"default_search_policy":"partial_hybrid","failure":null,"first_ready_unix_ms":null,
                "fully_ready_unix_ms":null,"pipeline_digest":null,"selected_artifacts":0,"selected_units":null,"selection_digest":"b".repeat(64),"state":"disabled","version":1});
        } else {
            body["discovery_observation"] = json!("unavailable_after_read");
        }
        originals.push(body.clone());
        inputs.push(MemberInput {
            index_id: index(n),
            is_error: false,
            exhausted: false,
            result: body,
            sources: sources()?,
        });
    }
    let result = compose(&request, inputs)?;
    assert_eq!(result.ranking.len(), 32);
    assert_eq!(result.members.len(), 32);
    assert!(!result.partial && !result.truncated);
    assert!(result.shared_values.is_some());
    let mut raw = serde_json::to_value(&result)?;
    raw["delivery_observation"] = json!({"original_fixture_receipt":"untouched raw body"});
    let unchanged = raw.clone();
    let bytes = airk_core::index_catalog::encoded_size(&raw, airk_core::MAX_RESPONSE_BYTES)?;
    let public = json!({"content":[{"type":"text","text":raw.to_string()}],"structuredContent":raw,"isError":false});
    airk_core::index_catalog::encoded_size(&public, 3 * airk_core::MAX_RESPONSE_BYTES + 4096)?;
    let raw = &public["structuredContent"];
    let logical = materialize_combined_read(raw)?;
    assert!(logical.get("delivery_observation").is_none());
    assert_eq!(raw, &unchanged);
    for (offset, original) in originals.iter().enumerate() {
        assert_eq!(
            materialize_combined_member_result(raw, index(offset as u8 + 1))?,
            *original
        );
        assert_eq!(logical["members"][offset]["result"], *original);
        let pointer = logical["ranking"][offset]["result_pointer"]
            .as_str()
            .unwrap();
        assert!(raw.pointer(pointer).is_some());
        assert!(
            logical["ranking"][offset]["origin"]
                .get("publication")
                .is_some()
        );
    }
    eprintln!("32 full native-shaped members: canonical bytes={bytes}");
    Ok(())
}
