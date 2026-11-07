// SPDX-License-Identifier: Apache-2.0
//! Complete native handler bodies through the actual MCP and public adapters.
use super::*;
use airk_core::{
    combined_reads::{MemberAvailability, QuerySelection, SharedBudgets},
    grant_provenance::GrantProvenance,
    identity::{Incarnation, Sha256Digest},
    index_catalog::*,
    scoped_reads::SourceDescriptor,
    service::{CombinedConnectRequest, ProjectRegistration},
};
use airk_kernel::{
    Kernel,
    combined_reads::{CompositionRequest, MemberInput, MemberSource, compose},
    scoped_reads::ScopedReadAuthority,
};
use rmcp::{ServiceExt, model::CallToolRequestParams};
use serde_json::{Map, Value, json};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct Retained {
    selection: CombinedConnectRequest,
    result: Value,
    calls: AtomicUsize,
    failed: AtomicBool,
    typed_failure: AtomicBool,
    arguments: Mutex<Vec<(String, Map<String, Value>)>>,
}
struct Finalized {
    selection: CombinedConnectRequest,
    member_body: Value,
    observed_capacity: AtomicUsize,
}
impl RemoteTools for Finalized {
    fn combined_selection(&self) -> Option<&CombinedConnectRequest> {
        Some(&self.selection)
    }
    fn query_concurrency(&self) -> usize {
        32
    }
    fn catalog(&self) -> Vec<Tool> {
        crate::combined::catalog()
    }
    fn call(&self, tool: String, args: Map<String, Value>) -> RemoteFuture<'_> {
        self.call_with_capacity(tool, args, 16)
    }
    fn call_with_capacity(
        &self,
        tool: String,
        _: Map<String, Value>,
        limit: usize,
    ) -> RemoteFuture<'_> {
        Box::pin(async move {
            self.observed_capacity.store(limit, Ordering::SeqCst);
            let mut body = if matches!(tool.as_str(), "parse" | "airk_parse") {
                let result = json!({"query_concurrency":limit,"native_members":self.member_body});
                let bytes = serde_json::to_vec(&result)?.len();
                json!({"schema":"airk.workbench.result/1","collection":"combined","publication":null,
                    "status":"complete","executed":true,"plan":airk_core::workbench::Plan::default(),
                    "requires_input":[],"steps":[{"id":"query","tool":"status","arguments":{},"status":"complete","elapsed_ms":1,"result":result,"error":null}],
                    "evidence":[],"warnings":[],"truncated":false,"completeness":[],"calls":1,"payload_bytes":bytes})
            } else {
                let mut body = self.member_body.clone();
                body["query_concurrency"] = limit.into();
                body
            };
            let digest = airk_core::identity::framed_digest(
                b"airk/combined-delivery-result/1",
                &[&serde_json::to_vec(&body)?],
            );
            body["delivery_observation"] = json!({"digest_basis":"canonical_body_without_delivery_observation","receipt":airk_core::service::CombinedDeliveryReceipt {
                schema:1,service_instance:id(41),namespace:self.selection.namespace,catalog_revision:CatalogRevision::INITIAL,
                scope_identity:Sha256Digest::from_bytes([42;32]),result_digest:digest,
            }});
            Ok(crate::unified::success(body))
        })
    }
}
type SubmittedDraft = (Option<String>, Map<String, Value>);
#[derive(Default)]
struct Drafts(Mutex<Vec<SubmittedDraft>>);
impl crate::unified::ExplorerTools for Drafts {
    fn call(
        &self,
        project: Option<String>,
        _: String,
        arguments: Map<String, Value>,
        _: airk_core::usage::ClientFamily,
    ) -> RemoteFuture<'_> {
        Box::pin(async move {
            self.0.lock().unwrap().push((project, arguments));
            Ok(crate::unified::success(
                json!({"applied":false,"executed":false,"draft_received":true}),
            ))
        })
    }
}
impl RemoteTools for Retained {
    fn combined_selection(&self) -> Option<&CombinedConnectRequest> {
        Some(&self.selection)
    }
    fn catalog(&self) -> Vec<Tool> {
        crate::combined::catalog()
    }
    fn call(&self, tool: String, arguments: Map<String, Value>) -> RemoteFuture<'_> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.arguments.lock().unwrap().push((tool, arguments));
            if self.typed_failure.load(Ordering::SeqCst) {
                return Err(anyhow::anyhow!("native publication admission")
                    .context(airk_kernel::CurrentGateContention::Applying));
            }
            if self.failed.load(Ordering::SeqCst) {
                anyhow::bail!("private/transport/path");
            }
            Ok(self.result.clone())
        })
    }
}

fn id(n: u8) -> Incarnation {
    Incarnation::from_bytes([n; 16])
}
async fn native_composition() -> anyhow::Result<(tempfile::TempDir, Arc<Retained>, String)> {
    let owner = tempfile::tempdir()?;
    let project = QueryProjectId::from_identity(id(1));
    let shared_project = QueryProjectId::from_identity(id(2));
    let indexes = [IndexId::from_identity(id(3)), IndexId::from_identity(id(4))];
    let roots = [
        SourceRootId::from_identity(id(5)),
        SourceRootId::from_identity(id(6)),
    ];
    let grant_id = GrantId::from_identity(id(7));
    let member = |project_id, index_id, source_root_id, path: &str| GrantedMembership {
        membership: Membership {
            project_id,
            index_id,
            revision: CatalogRevision::INITIAL,
            scope: vec![ScopeAtom {
                source_root_id,
                kind: PathScope::Subtree,
                path: path.into(),
            }],
        },
        source_revision: CatalogRevision::INITIAL,
        policy_epoch: 0,
        policy_digest: None,
    };
    let grant = ReviewedGrant {
        id: grant_id,
        host: Sha256Digest::from_bytes([8; 32]),
        revision: CatalogRevision::INITIAL,
        revoked: false,
        memberships: vec![
            member(project, indexes[0], roots[0], "public/api"),
            member(shared_project, indexes[0], roots[0], "public/impl"),
            member(project, indexes[1], roots[1], ""),
        ],
    };
    let selection = CombinedConnectRequest {
        schema: 1,
        context_key: Sha256Digest::from_bytes([9; 32]),
        namespace: id(10),
        expected_catalog_revision: CatalogRevision::INITIAL,
        grant_id,
        expected_grant_revision: CatalogRevision::INITIAL,
        project_ids: vec![project, shared_project],
        index_ids: None,
    };
    let resolved = QuerySelection {
        project_ids: selection.project_ids.clone(),
        index_ids: None,
        member_availability: MemberAvailability::Report,
    }
    .resolve(&grant)?;
    let mut inputs = Vec::new();
    let mut first_binding = None;
    for n in 0..2 {
        let source = owner.path().join(format!("source{n}"));
        std::fs::create_dir_all(source.join("public/api"))?;
        std::fs::create_dir_all(source.join("public/impl"))?;
        std::fs::write(
            source.join("public/api/a.rs"),
            "// Marker\npub struct MarkerApi;\n",
        )?;
        std::fs::write(
            source.join("public/impl/b.rs"),
            "// Marker\npub struct MarkerImpl;\n",
        )?;
        std::fs::write(
            source.join("private.rs"),
            "// Marker\npub struct MarkerPrivate;\n",
        )?;
        let kernel = Kernel::open(&source, Some(owner.path().join(format!("data{n}"))))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let backing = ProjectRegistration::new(
            kernel.root().to_str().unwrap().into(),
            kernel.data_dir().to_str().unwrap().into(),
            kernel.collection().into(),
        )?
        .binding;
        first_binding.get_or_insert_with(|| backing.to_string());
        let kernel = kernel.with_scoped_reads(ScopedReadAuthority {
            schema: 1,
            backing,
            index_id: indexes[n],
            project_ids: resolved.members[n].project_ids(),
            grant: grant.clone(),
            provenance: GrantProvenance::UserReviewedProjectGrant,
            sources: vec![SourceDescriptor {
                source_root_id: roots[n],
                original_root: kernel.root().to_str().unwrap().into(),
            }],
        })?;
        let mut tools = crate::local::LocalTools::new(kernel.clone()).await?;
        let result=tools.call("airk_search".into(),json!({"query":"Marker","mode":"lexical","limit":3,"max_bytes":1024,"verification":"report","response_mode":"full"}).as_object().unwrap().clone()).await?;
        assert_ne!(result.is_error, Some(true), "{result:?}");
        let mut result = serde_json::to_value(result)?;
        if result.get("structuredContent").is_none() {
            result["structuredContent"] =
                serde_json::from_str(result["content"][0]["text"].as_str().unwrap())?;
        }
        inputs.push(MemberInput {
            index_id: indexes[n],
            result,
            is_error: false,
            exhausted: false,
            sources: vec![MemberSource {
                source_root_id: roots[n],
                original_root: kernel.root().to_str().unwrap().into(),
                primary: true,
                aliases: Vec::new(),
            }],
        });
        tools.close().await?;
    }
    let composition = compose(
        &CompositionRequest {
            selection: resolved,
            budgets: SharedBudgets {
                limit: 6,
                max_bytes: 2048,
                deadline_ms: 5000,
                verification_max_sources: 8,
                verification_max_bytes: 16 * 1024 * 1024,
                verification_deadline_ms: 2000,
            },
            authority: Sha256Digest::from_bytes([11; 32]),
            observed_from_unix_ms: 1,
            observed_to_unix_ms: 2,
        },
        inputs,
    )?;
    assert_eq!(composition.ranking.len(), 5);
    assert_eq!(
        composition
            .ranking
            .iter()
            .filter(|row| row.index_id == indexes[0])
            .count(),
        2
    );
    let result = crate::unified::success(serde_json::to_value(composition)?);
    Ok((
        owner,
        Arc::new(Retained {
            selection,
            result,
            calls: AtomicUsize::new(0),
            failed: AtomicBool::new(false),
            typed_failure: AtomicBool::new(false),
            arguments: Mutex::new(Vec::new()),
        }),
        first_binding.unwrap(),
    ))
}

#[tokio::test]
async fn actual_combined_mcp_and_public_routes_preserve_complete_native_results()
-> anyhow::Result<()> {
    let (_owner, backend, _binding) = native_composition().await?;
    let finalized = Arc::new(Finalized {
        selection: backend.selection.clone(),
        member_body: backend.result["structuredContent"].clone(),
        observed_capacity: AtomicUsize::new(0),
    });
    let bridge = Bridge {
        backend: backend.clone(),
        admission: Arc::new(Admission::default()),
        response_mode: crate::response::ResponseMode::Full,
    };
    let (client_transport, server_transport) = tokio::io::duplex(128 * 1024);
    let (mut client, mut server) = tokio::try_join!(
        async { Ok::<_, anyhow::Error>(().serve(client_transport).await?) },
        async { Ok::<_, anyhow::Error>(bridge.serve(server_transport).await?) }
    )?;
    let catalog = client.list_all_tools().await?;
    let search = catalog
        .iter()
        .find(|tool| tool.name == "airk_search")
        .unwrap();
    assert!(
        search.input_schema["properties"]
            .get("project_ids")
            .is_some()
    );
    assert!(catalog.iter().any(|tool| tool.name == "airk_query_scope"));
    let args = json!({"query":"Marker","project_ids":backend.selection.project_ids,"response_mode":"full"});
    let wire = client
        .call_tool(
            CallToolRequestParams::new("airk_search")
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await?;
    assert_eq!(
        wire.structured_content,
        Some(backend.result["structuredContent"].clone())
    );
    let drafts = Arc::new(Drafts::default());
    let unified = crate::unified::Unified::combined(backend.clone(), Some(drafts.clone()))?;
    let public = unified
        .call("search".into(), args.as_object().unwrap().clone())
        .await?;
    assert_eq!(public, backend.result);
    assert_eq!(backend.arguments.lock().unwrap()[1].0, "airk_search");
    assert_eq!(
        backend.arguments.lock().unwrap()[1].1,
        args.as_object().unwrap().clone()
    );
    assert_eq!(
        unified
            .catalog()
            .iter()
            .find(|tool| tool.name == "search")
            .unwrap()
            .input_schema["properties"]["project_ids"],
        search.input_schema["properties"]["project_ids"]
    );
    let discovery = unified.call("projects".into(), Map::new()).await?;
    assert_eq!(discovery["structuredContent"]["projects"], json!([]));
    assert_eq!(
        discovery["structuredContent"]["default_project"],
        Value::Null
    );
    let draft = json!({"schema_version":"1.0","idempotency_key":"combined-draft","title":"Selected search","draft":{"query":"Marker","project_ids":backend.selection.project_ids}});
    let draft_reply = unified
        .call("draft_search".into(), draft.as_object().unwrap().clone())
        .await?;
    assert_eq!(draft_reply["structuredContent"]["executed"], false);
    assert_eq!(drafts.0.lock().unwrap()[0].0, None);
    assert_eq!(
        drafts.0.lock().unwrap()[0].1,
        draft.as_object().unwrap().clone()
    );
    let bad = unified
        .call(
            "draft_search".into(),
            json!({"draft":{"query":"Marker"}})
                .as_object()
                .unwrap()
                .clone(),
        )
        .await?;
    assert_eq!(bad["isError"], true);
    assert_eq!(drafts.0.lock().unwrap().len(), 1);
    backend.typed_failure.store(true, Ordering::SeqCst);
    let native_error = backend
        .call("airk_help".into(), Map::new())
        .await
        .unwrap_err();
    let expected =
        serde_json::to_value(crate::response::typed_error_result(&native_error).unwrap())?;
    for tool in ["help", "airk_help", "search", "airk_search"] {
        let before = backend.calls.load(Ordering::SeqCst);
        let actual = unified.call(tool.into(), Map::new()).await?;
        assert_eq!(actual, expected, "complete typed {tool} refusal");
        assert_eq!(
            backend.calls.load(Ordering::SeqCst),
            before + 1,
            "no implicit replay"
        );
    }
    backend.typed_failure.store(false, Ordering::SeqCst);
    backend.failed.store(true, Ordering::SeqCst);
    for tool in ["help", "airk_help", "search", "airk_search"] {
        let before = backend.calls.load(Ordering::SeqCst);
        let actual = unified.call(tool.into(), Map::new()).await?;
        let body = &actual["structuredContent"];
        assert_eq!(body["error"]["code"], "combined_observation_unavailable");
        assert_eq!(body["error"]["query_submitted"], Value::Null);
        assert_eq!(body["error"]["query_replay_safe"], false);
        assert!(!actual.to_string().contains("private/transport"));
        assert_eq!(backend.calls.load(Ordering::SeqCst), before + 1);
    }
    let lost = call_observed(backend.as_ref(), "airk_search".into(), Map::new()).await?;
    assert_eq!(lost["isError"], true);
    assert_eq!(
        lost["structuredContent"]["error"]["query_submitted"],
        Value::Null
    );
    assert_eq!(
        lost["structuredContent"]["error"]["query_replay_safe"],
        false
    );
    assert!(!lost.to_string().contains("private/transport"));
    client.close().await?;
    server.close().await?;
    // The owner seals the genuine native member payload after the outer
    // frontend supplies capacity. Neither direct nor public MCP may then alter
    // it or drop its delivery receipt while mapping a prepared plan.
    let public = Arc::new(crate::unified::Unified::combined(finalized.clone(), None)?);
    let bridge = Bridge {
        backend: public,
        admission: Arc::new(Admission::default()),
        response_mode: crate::response::ResponseMode::Full,
    };
    let (client_io, server_io) = tokio::io::duplex(128 * 1024);
    let (mut client, mut server) = tokio::try_join!(
        async { Ok::<_, anyhow::Error>(().serve(client_io).await?) },
        async { Ok::<_, anyhow::Error>(bridge.serve(server_io).await?) }
    )?;
    for tool in ["status", "airk_status", "parse", "airk_parse"] {
        let reply = client.call_tool(CallToolRequestParams::new(tool)).await?;
        assert_ne!(reply.is_error, Some(true), "{reply:?}");
        assert_eq!(finalized.observed_capacity.load(Ordering::SeqCst), 32);
        let mut body = reply.structured_content.unwrap();
        let observation = body
            .as_object_mut()
            .unwrap()
            .remove("delivery_observation")
            .unwrap();
        let receipt: airk_core::service::CombinedDeliveryReceipt =
            serde_json::from_value(observation["receipt"].clone())?;
        assert_eq!(
            receipt.result_digest,
            airk_core::identity::framed_digest(
                b"airk/combined-delivery-result/1",
                &[&serde_json::to_vec(&body)?]
            ),
            "{tool}"
        );
        if matches!(tool, "parse" | "airk_parse") {
            assert_eq!(body["steps"][0]["result"]["query_concurrency"], 32);
        } else {
            assert_eq!(body["query_concurrency"], 32);
        }
    }
    client.close().await?;
    server.close().await?;
    Ok(())
}
