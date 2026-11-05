// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::Incarnation,
    index_catalog::{GrantId, QueryProjectId},
};

fn fixture() -> Backend {
    let identity = Incarnation::from_bytes([1; 16]);
    Backend {
        control: PathBuf::new(),
        selection: CombinedConnectRequest {
            schema: 1,
            context_key: Sha256Digest::from_bytes([2; 32]),
            namespace: identity,
            expected_catalog_revision: CatalogRevision::INITIAL,
            grant_id: GrantId::from_identity(identity),
            expected_grant_revision: CatalogRevision::INITIAL,
            project_ids: vec![QueryProjectId::from_identity(identity)],
            index_ids: Some(vec![IndexId::from_identity(identity)]),
        },
        children: Mutex::new(BTreeMap::new()),
        closed: std::sync::atomic::AtomicBool::new(false),
        client: Mutex::new(airk_core::usage::ClientFamily::Cli),
        service_instance: tokio::sync::OnceCell::new(),
    }
}

#[test]
fn omitted_controls_keep_frozen_selection_but_explicit_projects_do_not_inherit_subset() -> Result<()>
{
    let backend = fixture();
    let default = backend.request_selection(&mut Map::new())?;
    assert_eq!(default.index_ids, backend.selection.index_ids);
    let mut explicit = json!({"project_ids":backend.selection.project_ids})
        .as_object()
        .unwrap()
        .clone();
    assert!(
        backend
            .request_selection(&mut explicit)?
            .index_ids
            .is_none()
    );
    assert!(explicit.is_empty());
    Ok(())
}

#[test]
fn mixed_null_and_index_only_selectors_are_rejected_before_native_work() {
    let backend = fixture();
    for value in [
        json!({"project":"a".repeat(64)}),
        json!({"project_ids":null}),
        json!({"project_ids":[]}),
        json!({"project_ids":backend.selection.project_ids,"index_ids":null}),
        json!({"project_ids":backend.selection.project_ids,"index_ids":[]}),
        json!({"index_ids":backend.selection.index_ids}),
        json!({"member_availability":"require"}),
    ] {
        assert!(
            backend
                .request_selection(&mut value.as_object().unwrap().clone())
                .is_err(),
            "{value}"
        );
    }
    assert!(backend.children.lock().unwrap().is_empty());
}

#[tokio::test]
async fn internal_projection_callback_is_not_a_public_tool() -> Result<()> {
    let backend = fixture();
    let reply = backend
        .call("__airk_verify_projection".into(), Map::new())
        .await?;
    assert_eq!(
        reply["structuredContent"]["error"]["code"],
        "unknown_combined_tool"
    );
    assert_eq!(
        reply["structuredContent"]["error"]["query_submitted"],
        false
    );
    assert!(backend.children.lock().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
async fn public_discovery_rejects_plural_controls_before_metadata_or_worker_access() -> Result<()> {
    let backend = fixture();
    for args in [
        json!({"view":"projects","project_ids":backend.selection.project_ids}),
        json!({"view":"projects","member_availability":"require"}),
        json!({"view":"projects","origin_publication":"invalid"}),
    ] {
        let reply = backend
            .call("airk_query_scope".into(), args.as_object().unwrap().clone())
            .await?;
        assert_eq!(
            reply["structuredContent"]["error"]["code"],
            "invalid_query_scope_arguments"
        );
        assert_eq!(
            reply["structuredContent"]["error"]["query_submitted"],
            false
        );
    }
    assert!(backend.children.lock().unwrap().is_empty());
    Ok(())
}

#[test]
fn vector_interval_retains_cached_native_dates_without_rewriting_them() {
    let body = json!({"observed_unix_ms":100,"cache_age_ms":25,"source_verification":{"observed_from_unix_ms":120,"observed_to_unix_ms":130}});
    let mut from = 125;
    let mut to = 125;
    extend_observation_interval(&body, &mut from, &mut to);
    assert_eq!((from, to), (100, 130));
    assert_eq!(body["observed_unix_ms"], 100);
    assert_eq!(body["cache_age_ms"], 25);
}

#[test]
fn public_transport_duplication_does_not_reduce_the_canonical_body_budget() -> Result<()> {
    let body = json!({"text":"\\\"".repeat(12000)});
    assert!(serde_json::to_vec(&body)?.len() < airk_core::MAX_RESPONSE_BYTES);
    let envelope = native_envelope(body);
    assert!(serde_json::to_vec(&envelope)?.len() > airk_core::MAX_RESPONSE_BYTES);
    check_public_reply(&envelope, 0)?;
    assert!(check_public_reply(&native_envelope(json!({"text":"x".repeat(65536)})), 0).is_err());
    Ok(())
}

#[tokio::test]
async fn combined_status_observation_has_one_two_second_window_and_unknown_evidence() -> Result<()>
{
    struct SlowStatus {
        selection: CombinedConnectRequest,
        calls: std::sync::atomic::AtomicUsize,
    }
    impl RemoteTools for SlowStatus {
        fn combined_selection(&self) -> Option<&CombinedConnectRequest> {
            Some(&self.selection)
        }
        fn call(&self, _: String, _: Map<String, Value>) -> RemoteFuture<'_> {
            Box::pin(async move {
                self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tokio::time::sleep(Duration::from_secs(20)).await;
                Ok(json!({"unexpected":"late status"}))
            })
        }
    }
    let owner = Arc::new(SlowStatus {
        selection: fixture().selection,
        calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let backend = crate::unified_mcp::with_runtime_readiness(None, PathBuf::new(), owner.clone());
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        backend.call_with_capacity("airk_status".into(), Map::new(), 16),
    )
    .await??;
    let body = canonical_body(&result);
    assert_eq!(body["error"]["code"], "combined_status_timeout");
    assert_eq!(body["error"]["query_submitted"], Value::Null);
    assert_eq!(body["health"], "unknown");
    assert_eq!(body["publication"], Value::Null);
    assert_eq!(body["publication_vector"], Value::Null);
    assert_eq!(body["member_observations"]["members"], Value::Null);
    assert_eq!(owner.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    backend.shutdown().await?;
    Ok(())
}

#[test]
fn verification_timeout_retains_usable_report_evidence_and_require_failure() {
    let original = json!({"hits":[{"citation":{"path":"one.rs"},"text":"preserved"}],"publication":{"token":"original"}});
    let report = canonical_body(&verification_uncertainty(
        &original,
        airk_core::verification::VerificationMode::Report,
    ));
    assert_eq!(report["hits"], original["hits"]);
    assert!(report.get("source_verification").is_none());
    assert!(report.get("error").is_none());
    assert_eq!(
        report["source_verification_observation"]["state"],
        "unavailable"
    );
    let require = verification_uncertainty(
        &original,
        airk_core::verification::VerificationMode::Require,
    );
    assert!(result_failed(&require));
    assert_eq!(canonical_body(&require)["hits"], original["hits"]);
}

#[test]
fn verification_windows_never_replace_zero_or_elapsed_allowances() {
    assert!(verification_window(Instant::now() - Duration::from_millis(1), 2000).is_none());
    assert!(verification_window(Instant::now() + Duration::from_secs(10), 0).is_none());
    let window = verification_window(Instant::now() + Duration::from_secs(10), 3).unwrap();
    assert!(window <= Duration::from_millis(3));
    let original = json!({"hits":[{"citation":{"path":"one.rs"},"text":"retained"}]});
    let report = canonical_body(&verification_not_submitted(
        &original,
        airk_core::verification::VerificationMode::Report,
    ));
    assert_eq!(report["hits"], original["hits"]);
    assert_eq!(
        report["source_verification_observation"]["error"]["query_submitted"],
        false
    );
    assert!(report.get("source_verification").is_none());
    assert!(!result_failed(&report));
    assert!(result_failed(&verification_not_submitted(
        &original,
        airk_core::verification::VerificationMode::Require
    )));
}

fn expired_projection_fixture() -> Result<(Backend, QuerySelection, Snapshot, Value)> {
    use airk_core::{
        grant_provenance::GrantProvenance,
        identity::RevisionToken,
        index_catalog::{
            GrantedMembership, Membership, PathScope, ReviewedGrant, ScopeAtom, SourceImportState,
            SourceRootId,
        },
        scoped_reads::SourceDescriptor,
        service::ProjectRegistration,
    };
    let mut backend = fixture();
    backend.selection.index_ids = None;
    let selected = backend.selection.selection();
    let project_id = selected.project_ids[0];
    let root = std::env::current_dir()?.to_string_lossy().into_owned();
    let grant = ReviewedGrant {
        id: backend.selection.grant_id,
        host: Sha256Digest::from_bytes([9; 32]),
        revision: CatalogRevision::INITIAL,
        revoked: false,
        memberships: (1..=32)
            .map(|n| GrantedMembership {
                membership: Membership {
                    project_id,
                    index_id: IndexId::from_identity(Incarnation::from_bytes([n; 16])),
                    revision: CatalogRevision::INITIAL,
                    scope: vec![ScopeAtom {
                        source_root_id: SourceRootId::from_identity(Incarnation::from_bytes(
                            [n; 16],
                        )),
                        kind: PathScope::Subtree,
                        path: String::new(),
                    }],
                },
                source_revision: CatalogRevision::INITIAL,
                policy_epoch: 0,
                policy_digest: None,
            })
            .collect(),
    };
    let resolved = selected.resolve(&grant)?;
    let mut members = Vec::new();
    let mut bodies = Vec::new();
    for member in &resolved.members {
        let backing = ProjectRegistration::new(
            root.clone(),
            root.clone(),
            format!("fixture_{}", members.len()),
        )?;
        let authority = ScopedReadAuthority {
            schema: 1,
            backing: backing.binding,
            index_id: member.index_id,
            project_ids: vec![project_id],
            grant: grant.clone(),
            provenance: GrantProvenance::UserReviewedProjectGrant,
            sources: vec![SourceDescriptor {
                source_root_id: member.scope[0].source_root_id,
                original_root: root.clone(),
            }],
        };
        authority.validate()?;
        members.push(Member {
            index: IndexDefinition {
                id: member.index_id,
                name: "deadline fixture".into(),
                backing,
                definition_revision: CatalogRevision::INITIAL,
                source_revision: CatalogRevision::INITIAL,
                policy: None,
                tombstone: false,
                source_import: SourceImportState::Complete,
            },
            authority,
            sources: vec![],
            selection: ScopedConnectRequest {
                schema: 1,
                context_key: backend.selection.context_key,
                namespace: backend.selection.namespace,
                expected_catalog_revision: CatalogRevision::INITIAL,
                grant_id: grant.id,
                expected_grant_revision: grant.revision,
                index_id: member.index_id,
                project_ids: vec![project_id],
            },
        });
        let token = RevisionToken {
            incarnation: member.index_id.identity(),
            generation: 1,
            revision: Sha256Digest::from_bytes([8; 32]),
        };
        bodies.push(json!({"index_id":member.index_id,"is_error":false,"result":{"publication":{"token":token},"hits":[{"citation":{"path":"queue.rs","content_hash":"ab".repeat(32)},"text":"retained"}]}}));
    }
    let identity = resolved.identity()?;
    let before = Snapshot {
        revision: CatalogRevision::INITIAL,
        resolved,
        projects: vec![],
        members,
        identity,
    };
    let projection =
        json!({"steps":[{"result":{"schema":"airk.combined-read/1","members":bodies}}]});
    Ok((backend, selected, before, projection))
}

#[tokio::test]
async fn exhausted_final_verification_records_all_32_members_without_native_submission()
-> Result<()> {
    for mode in ["report", "require"] {
        let (backend, selected, before, projection) = expired_projection_fixture()?;
        let args=json!({"projection":projection,"remaining_deadline_ms":0,"receipt_bytes_budget":32768,"verification":mode,"project_ids":selected.project_ids}).as_object().unwrap().clone();
        let result = backend
            .verify_final_callback(args, selected, before)
            .await?;
        assert_eq!(result["partial"], true);
        assert_eq!(result["members"].as_array().unwrap().len(), 32);
        for member in result["members"].as_array().unwrap() {
            assert_eq!(member["outcome"], "unqueried");
            assert_eq!(
                member["result"]["error"]["code"],
                "verification_deadline_exhausted"
            );
            assert_eq!(member["result"]["error"]["query_submitted"], false);
            assert!(member["result"].get("source_verification").is_none());
            assert_eq!(
                member["result"]["steps"][0]["result"]["hits"][0]["text"],
                "retained"
            );
        }
        assert_eq!(
            result.get("error").is_some_and(|error| !error.is_null()),
            mode == "require"
        );
        assert!(
            backend.children.lock().unwrap().is_empty(),
            "no child pool or worker can be admitted after the shared deadline"
        );
    }
    Ok(())
}
