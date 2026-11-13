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

#[cfg(test)]
mod workbench_catalog_metadata {
    use super::*;
    use crate::{Kernel, workbench};
    use airk_core::workbench::{REQUEST_SCHEMA, Request};
    use std::{
        cell::Cell,
        future::Future,
        task::{Context, Poll, Waker},
    };

    fn ready<T>(future: impl Future<Output = T>) -> T {
        let mut context = Context::from_waker(Waker::noop());
        match std::pin::pin!(future).as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("bounded in-memory callback must finish without scheduling"),
        }
    }
    fn selection() -> QuerySelection {
        QuerySelection {
            project_ids: vec![project(1)],
            index_ids: Some(vec![index(1)]),
            member_availability: MemberAvailability::Report,
        }
    }
    fn metadata_request() -> Result<Request> {
        Ok(serde_json::from_value(
            json!({"schema":REQUEST_SCHEMA,"mode":"execute","text":"/capabilities","verification":"off"}),
        )?)
    }
    fn metadata_legacy(
        kernel: &Kernel,
        value: &Value,
    ) -> Result<airk_core::workbench::ResultEnvelope> {
        kernel.evidence_plan_with_reader(
            metadata_request()?,
            &Value::Null,
            Some(&|tool, _, _| {
                assert_eq!(tool, "capabilities");
                Ok(value.clone())
            }),
        )
    }
    fn metadata_combined(value: &Value) -> Result<airk_core::workbench::ResultEnvelope> {
        ready(workbench::combined::execute(
            metadata_request()?,
            selection(),
            Sha256Digest::from_bytes([4; 32]),
            |tool, _| {
                assert_eq!(tool, "capabilities");
                std::future::ready(Ok(value.clone()))
            },
        ))
    }
    #[test]
    fn canonical_workbench_catalog_schemas_are_complete_in_legacy_and_combined_plans() -> Result<()>
    {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir(&root)?;
        let kernel = Kernel::open(&root, Some(temp.path().join("state")))?;
        for catalog in [
            workbench::operation_catalog(),
            workbench::combined::operation_catalog(),
        ] {
            assert!(catalog["operations"]["query_scope"]["properties"]["cursor"].is_object());
            let value =
                json!({"workbench":catalog,"nested":{"page":{"cursor":null,"truncated":false}}});
            for result in [
                metadata_legacy(&kernel, &value)?,
                metadata_combined(&value)?,
            ] {
                assert_eq!(result.status, "complete");
                assert!(!result.truncated);
                assert!(result.completeness.is_empty());
                assert_eq!(result.steps[0].result.as_ref(), Some(&value));
            }
        }
        Ok(())
    }
    #[test]
    fn real_continuations_and_flags_remain_visible_beside_catalog_schemas() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir(&root)?;
        let kernel = Kernel::open(&root, Some(temp.path().join("state")))?;
        for page in [
            json!({"cursor":"next-page"}),
            json!({"cursor":{"next":"opaque-object"}}),
            json!({"truncated":true}),
        ] {
            let value = json!({"workbench":workbench::operation_catalog(),"nested":{"page":page}});
            for result in [
                metadata_legacy(&kernel, &value)?,
                metadata_combined(&value)?,
            ] {
                assert!(result.truncated);
                assert_eq!(result.status, "partial");
                assert!(
                    result
                        .completeness
                        .iter()
                        .any(|reason| reason.contains("upstream truncation or continuation"))
                );
            }
        }
        for mut catalog in [
            workbench::operation_catalog(),
            workbench::combined::operation_catalog(),
        ] {
            catalog["cursor"] = json!({"next":"real-catalog-root-page"});
            let result = metadata_combined(&json!({"workbench":catalog}))?;
            assert!(result.truncated);
        }
        let mut unknown = workbench::operation_catalog();
        unknown["catalog"] = json!("unknown-catalog");
        assert!(metadata_combined(&json!({"workbench":unknown}))?.truncated);
        let mut malformed = workbench::operation_catalog();
        malformed["operations"]["query_scope"]["additionalProperties"] = json!(true);
        assert!(metadata_combined(&json!({"workbench":malformed}))?.truncated);
        let partial = json!({"partial":true,"workbench":workbench::combined::operation_catalog()});
        let result = metadata_combined(&partial)?;
        assert_eq!(result.status, "partial");
        assert!(!result.truncated);
        Ok(())
    }
    #[test]
    fn both_definition_bindings_still_refuse_real_upstream_continuations() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let source_dir = temp.path().join("source");
        std::fs::create_dir(&source_dir)?;
        std::fs::write(source_dir.join("file.rs"), "pub struct Queue;\n")?;
        let kernel = Kernel::open(&source_dir, Some(temp.path().join("state")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let plan = json!({"steps":[{"id":"find","tool":"symbols","args":{"name":"Queue"}},{"id":"outline","tool":"outline","bind":[{"arg":"path","from":"find","select":"unique_definition_path"}]}]});
        for cursor in [json!("next-page"), json!({"next":"opaque-object"})] {
            let review = kernel.evidence_plan(serde_json::from_value(
                json!({"schema":REQUEST_SCHEMA,"plan":plan,"verification":"off"}),
            )?)?;
            let publication = review.plan.publication.clone();
            let calls = Cell::new(0);
            let value = json!({"publication":publication,"hits":[hit("file.rs","Queue")],"workbench":workbench::operation_catalog(),"nested":{"cursor":cursor}});
            let result=kernel.evidence_plan_with_reader(serde_json::from_value(json!({"schema":REQUEST_SCHEMA,"mode":"execute","plan":review.plan,"verification":"off"}))?,&Value::Null,Some(&|tool,_,_|{
                calls.set(calls.get()+1);assert_eq!(tool,"symbols");Ok(value.clone())
            }))?;
            assert_eq!(calls.get(), 1);
            assert!(result.steps.iter().any(|step| {
                step.error
                    .as_deref()
                    .is_some_and(|error| error.contains("definition enumeration is incomplete"))
            }));
            let value = json!({"schema":airk_core::combined_reads::COMBINED_READ_SCHEMA,"members":[{"index_id":index(1),"result":{"structuredContent":{"hits":[hit("file.rs","Queue")],"nested":{"cursor":cursor}}}}],"ranking":[{"index_id":index(1),"result_pointer":"/members/0/result/structuredContent/hits/0","origin":{"index_id":index(1),"source_root_id":root(1),"project_ids":[project(1)],"source_relative_path":"file.rs","publication":revision(1)}}]});
            let calls = Cell::new(0);
            let result = ready(workbench::combined::execute(
                serde_json::from_value(
                    json!({"schema":REQUEST_SCHEMA,"mode":"execute","plan":plan,"verification":"off"}),
                )?,
                selection(),
                Sha256Digest::from_bytes([4; 32]),
                |tool, _| {
                    calls.set(calls.get() + 1);
                    assert_eq!(tool, "symbols");
                    std::future::ready(Ok(value.clone()))
                },
            ))?;
            assert_eq!(calls.get(), 1);
            assert!(result.steps.iter().any(|step| {
                step.error
                    .as_deref()
                    .is_some_and(|error| error.contains("definition enumeration is incomplete"))
            }));
        }
        Ok(())
    }
}

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
            limit_per_index: None,
        },
        authority: Sha256Digest::from_bytes([7; 32]),
        observed_from_unix_ms: 10,
        observed_to_unix_ms: 20,
        index_names: Default::default(),
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
        elapsed_ms: None,
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
        elapsed_ms: None,
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
        elapsed_ms: None,
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
                elapsed_ms: None,
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
            elapsed_ms: None,
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
        elapsed_ms: None,
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
        elapsed_ms: None,
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
        id: request.selection.grant_id.expect("catalog-grant fixture"),
        host: Sha256Digest::from_bytes([1; 32]),
        revision: request
            .selection
            .grant_revision
            .expect("catalog-grant fixture"),
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
                elapsed_ms: None,
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
            id: request.selection.grant_id.expect("catalog-grant fixture"),
            host: Sha256Digest::from_bytes([1; 32]),
            revision: request
                .selection
                .grant_revision
                .expect("catalog-grant fixture"),
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
                elapsed_ms: None,
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
            elapsed_ms: None,
        },
        MemberInput {
            index_id: index(2),
            is_error: true,
            exhausted: false,
            result: failure.clone(),
            sources: vec![],
            elapsed_ms: None,
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
        id: request.selection.grant_id.expect("catalog-grant fixture"),
        host: Sha256Digest::from_bytes([1; 32]),
        revision: request
            .selection
            .grant_revision
            .expect("catalog-grant fixture"),
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
            elapsed_ms: None,
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

/// U106(d) regression (open-agent-access step 5): an open read of two whole
/// indexes, on the same source root identity and on different ones, has no
/// Project memberships. Its evidence qualifies against each member's own
/// scope atoms with no Project; evidence outside them still refuses. Before
/// the fix every hit refused ("member returned evidence outside its selected
/// source scope"), so every multi-index open search answered
/// `combined_read_unavailable`.
#[test]
fn open_members_qualify_whole_index_evidence_without_a_project() -> Result<()> {
    use airk_core::{
        identity::Sha256Digest,
        scoped_reads::{ScopeSelectionObservation, SelectionOwnerKind},
    };
    let observation = ScopeSelectionObservation {
        owner: SelectionOwnerKind::Agent,
        scope_identity: Sha256Digest::from_bytes([9; 32]),
        catalog_revision: CatalogRevision::INITIAL,
    };
    let query = QuerySelection {
        project_ids: Vec::new(),
        index_ids: Some(vec![index(1), index(2)]),
        member_availability: MemberAvailability::Report,
    };
    for second_root in [1u8, 2] {
        let mut composition = request()?;
        composition.selection = query.resolve_open(
            &[
                (index(1), vec![root(1)]),
                (index(2), vec![root(second_root)]),
            ],
            observation,
        )?;
        let first = input(1, vec![hit("api/one.rs", "one"), hit("impl/two.rs", "two")])?;
        let mut second = input(2, vec![hit("docs/three.md", "three")])?;
        second.sources[0].source_root_id = root(second_root);
        let combined = compose(&composition, vec![first, second])?;
        assert_eq!(combined.members.len(), 2);
        assert_eq!(combined.ranking.len(), 3);
        assert!(!combined.partial && combined.error.is_none());
        let value = serde_json::to_value(&combined)?;
        assert_eq!(value["selection"]["selection"]["owner"], "agent");
        for ranking in &combined.ranking {
            assert!(ranking.origin.project_ids.is_empty());
            let origin = airk_core::combined_reads::materialize_ranked_origin(
                &value,
                &serde_json::to_value(ranking)?,
            )?;
            assert!(origin.project_ids.is_empty());
        }
        // A member answering from a root outside its scope still refuses.
        let mut foreign = input(2, vec![hit("docs/three.md", "three")])?;
        foreign.sources[0].source_root_id = root(7);
        assert!(compose(&composition, vec![foreign]).is_err());
    }
    // A Project-free origin is valid only in an agent-owned result.
    let open = compose(
        &{
            let mut composition = request()?;
            composition.selection = query.resolve_open(
                &[(index(1), vec![root(1)]), (index(2), vec![root(1)])],
                observation,
            )?;
            composition
        },
        vec![input(1, vec![hit("api/one.rs", "one")])?],
    )?;
    let mut value = serde_json::to_value(&open)?;
    let rank = serde_json::to_value(&open.ranking[0])?;
    value["selection"]["selection"]["owner"] = json!("window");
    assert!(airk_core::combined_reads::materialize_ranked_origin(&value, &rank).is_err());
    // And a Project-scoped composition never yields a Project-free origin.
    let scoped = compose(&request()?, vec![input(1, vec![hit("api/one.rs", "one")])?])?;
    assert_eq!(scoped.ranking[0].origin.project_ids, vec![project(1)]);
    Ok(())
}

/// U108 contract: a composed member without a measured time encodes as
/// before (no `elapsed_ms` key); a measured time round-trips.
#[test]
fn member_elapsed_time_is_optional_and_absent_by_default() -> Result<()> {
    let combined = compose(&request()?, vec![input(1, vec![hit("api/one.rs", "one")])?])?;
    let value = serde_json::to_value(&combined)?;
    for member in value["members"].as_array().unwrap() {
        assert!(member.get("elapsed_ms").is_none(), "{member}");
    }
    let mut member = value["members"][0].clone();
    member["elapsed_ms"] = json!(17);
    let decoded: ComposedMember = serde_json::from_value(member.clone())?;
    assert_eq!(decoded.elapsed_ms, Some(17));
    assert_eq!(serde_json::to_value(&decoded)?, member);
    Ok(())
}

/// U108 behaviour: a read member carries its own elapsed time and every
/// member and ranked reference its catalog display name; an unqueried
/// member has no elapsed time, and without names nothing is added.
#[test]
fn members_carry_their_elapsed_time_and_index_names() -> Result<()> {
    let mut composition = request()?;
    composition.index_names = [(index(1), "Docs".to_owned()), (index(2), "Code".to_owned())]
        .into_iter()
        .collect();
    let mut read = input(1, vec![hit("api/one.rs", "one")])?;
    read.elapsed_ms = Some(12);
    // Member 2 is never queried.
    let combined = compose(&composition, vec![read])?;
    let value = serde_json::to_value(&combined)?;
    assert_eq!(value["members"][0]["elapsed_ms"], 12);
    assert_eq!(value["members"][0]["index_name"], "Docs");
    assert!(value["members"][1].get("elapsed_ms").is_none());
    assert_eq!(value["members"][1]["index_name"], "Code");
    assert_eq!(value["ranking"][0]["index_name"], "Docs");
    // The origin stays name-free: follow-up reads reuse it.
    assert!(value["ranking"][0]["origin"].get("index_name").is_none());
    let unnamed = serde_json::to_value(compose(
        &request()?,
        vec![input(1, vec![hit("api/one.rs", "one")])?],
    )?)?;
    assert!(unnamed["members"][0].get("index_name").is_none());
    assert!(unnamed["ranking"][0].get("index_name").is_none());
    Ok(())
}
