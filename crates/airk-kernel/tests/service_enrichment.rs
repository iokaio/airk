// SPDX-License-Identifier: Apache-2.0
//! A separate process keeps the injected transport and service owner isolated.
use airk_core::{
    SearchQuery,
    identity::{Incarnation, Sha256Digest},
    readiness::{DefaultSearchPolicy, SemanticState},
    search_execution::{IncompleteReason, RequestedMode, ServedMode},
    service::WorkerIdentity,
    service_resources::WorkAllowance,
};
use airk_kernel::{Kernel, service_execution};
use airk_retrieval::{
    embedding_worker::{self, Connector, ModelLocation, RemoteSession, Request, Response},
    model::{EmbeddingCacheStats, PlanningTokens},
};
use airk_store::service::model_files::ServiceModelFiles;
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

struct Transport {
    available: AtomicBool,
    failure: AtomicUsize,
    requests: AtomicUsize,
    opens: AtomicUsize,
    embedded: AtomicUsize,
}
impl Transport {
    fn require_available(&self) -> Result<()> {
        ensure!(
            self.available.load(Ordering::Acquire),
            embedding_worker::HelperUnavailable
        );
        match self.failure.load(Ordering::Acquire) {
            0 => Ok(()),
            1 => anyhow::bail!("unrelated fixture failure mentioning helper unavailable"),
            2 => Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into()),
            3 => Err(std::io::Error::from(std::io::ErrorKind::InvalidData).into()),
            _ => anyhow::bail!("fixture model identity mismatch"),
        }
    }
    fn calls(&self) -> usize {
        self.opens.load(Ordering::Acquire) + self.requests.load(Ordering::Acquire)
    }
}
struct Adapter(Arc<Transport>);
impl Connector for Adapter {
    fn open(&self, location: ModelLocation, _: &str) -> Result<Arc<dyn RemoteSession>> {
        ensure!(
            matches!(location, ModelLocation::Shared { .. }),
            "service tried a collection-local model"
        );
        self.0.opens.fetch_add(1, Ordering::AcqRel);
        self.0.require_available()?;
        Ok(self.0.clone())
    }
}
impl RemoteSession for Transport {
    fn request(&self, request: Request) -> Result<Response> {
        // Cached sessions must observe an outage too, not only new Open calls.
        self.requests.fetch_add(1, Ordering::AcqRel);
        self.require_available()?;
        match request {
            Request::Plan { text } => Ok(Response::Planned {
                tokens: PlanningTokens {
                    offsets: if text.is_empty() {
                        vec![]
                    } else {
                        vec![(0, text.len())]
                    },
                    special_tokens: 0,
                },
            }),
            Request::Embed { texts, .. } => {
                self.embedded.fetch_add(texts.len(), Ordering::AcqRel);
                Ok(Response::Embedded {
                    vectors: texts.iter().map(|_| Some(vec![1.0, 0.0])).collect(),
                    execution: Box::new(EmbeddingCacheStats::default()),
                })
            }
            Request::Peak => Ok(Response::Peak { bytes: 4096 }),
            _ => anyhow::bail!("unexpected fixture transport operation"),
        }
    }
}

fn outage_policy_matrix(
    kernel: &Kernel,
    transport: &Transport,
    identity: WorkerIdentity,
    state: SemanticState,
    turn: u8,
) -> Result<()> {
    let query = SearchQuery {
        text: "copper".into(),
        path_prefix: Some("memo-00.md".into()),
        limit: 1,
        max_bytes: 12,
    };
    let was_available = transport.available.swap(false, Ordering::AcqRel);
    for (offset, policy) in [
        DefaultSearchPolicy::PartialHybrid,
        DefaultSearchPolicy::LexicalWhilePending,
        DefaultSearchPolicy::RequireComplete,
    ]
    .into_iter()
    .enumerate()
    {
        work(identity, turn + offset as u8, || {
            kernel.set_default_semantic_search_policy(policy)
        })?;
        let before = kernel.status()?;
        assert_eq!(before.semantic_readiness.state, state);
        let calls = transport.calls();
        let lexical = kernel.search(&query)?;
        assert_eq!(
            transport.calls(),
            calls,
            "lexical read contacted the helper"
        );
        assert_eq!(
            lexical.execution.as_ref().unwrap().requested_mode,
            RequestedMode::Lexical
        );
        assert!(!lexical.execution.as_ref().unwrap().incomplete);
        let default = kernel.default_search(&query);
        if policy == DefaultSearchPolicy::RequireComplete {
            let error = default.unwrap_err();
            if state == SemanticState::Ready {
                let receipt = airk_core::search_execution::helper_unavailable_receipt(&error)
                    .expect("ready strict Default preserves the typed helper receipt");
                assert_eq!(receipt["requested_mode"], "default");
                assert_eq!(receipt["error"]["retryable"], false);
                assert_eq!(
                    transport.calls() - calls,
                    1,
                    "strict query retried helper I/O"
                );
            } else {
                assert!(airk_core::search_execution::helper_unavailable_receipt(&error).is_none());
                assert_eq!(
                    transport.calls(),
                    calls,
                    "coverage refusal opened the helper"
                );
            }
        } else {
            let fallback = default?;
            let execution = fallback.execution.as_ref().unwrap();
            assert_eq!(execution.requested_mode, RequestedMode::Default);
            assert_eq!(execution.served_mode, ServedMode::Lexical);
            assert!(execution.incomplete);
            let lexical_policy =
                policy == DefaultSearchPolicy::LexicalWhilePending && state != SemanticState::Ready;
            assert_eq!(
                execution.incomplete_reason,
                Some(if lexical_policy {
                    IncompleteReason::SemanticCoverageIncomplete
                } else {
                    IncompleteReason::HelperUnavailable
                })
            );
            assert_eq!(transport.calls() - calls, usize::from(!lexical_policy));
            assert_eq!(
                serde_json::to_value(&fallback.hits)?,
                serde_json::to_value(&lexical.hits)?
            );
            assert_eq!(
                fallback.truncated, lexical.truncated,
                "semantic loss is not clipping"
            );
            assert_eq!(fallback.publication, lexical.publication);
            assert_eq!(fallback.semantic_readiness.as_ref().unwrap().state, state);
            assert!(fallback.hits[0].text.len() <= query.max_bytes);
        }
        for mode in [
            airk_kernel::current::CurrentSearchMode::Vector,
            airk_kernel::current::CurrentSearchMode::Hybrid,
        ] {
            let calls = transport.calls();
            let error = kernel.search_response(&query, mode, None).unwrap_err();
            let receipt = airk_core::search_execution::helper_unavailable_receipt(&error)
                .expect("explicit modes do not degrade during a helper outage");
            assert_eq!(receipt["error"]["retryable"], false);
            assert_eq!(
                receipt["publication"],
                serde_json::to_value(&before.publication)?
            );
            assert_eq!(
                receipt["semantic_readiness"]["state"],
                serde_json::to_value(state)?
            );
            assert_eq!(
                transport.calls() - calls,
                1,
                "explicit query retried helper I/O"
            );
        }
        assert_eq!(
            kernel.status()?.publication,
            before.publication,
            "read published new evidence"
        );
        assert_eq!(
            kernel.status()?.semantic_readiness.state,
            state,
            "outage changed indexed coverage"
        );
    }
    work(identity, turn + 3, || {
        kernel.set_default_semantic_search_policy(DefaultSearchPolicy::PartialHybrid)
    })?;
    transport.available.store(was_available, Ordering::Release);
    Ok(())
}

fn ready_execution_observations(
    kernel: &Kernel,
    transport: &Transport,
    identity: WorkerIdentity,
) -> Result<()> {
    outage_policy_matrix(kernel, transport, identity, SemanticState::Ready, 32)?;
    let query = SearchQuery {
        text: "copper".into(),
        path_prefix: Some("memo-00.md".into()),
        limit: 4,
        max_bytes: 4096,
    };
    let healthy = kernel.default_search(&query)?;
    let execution = healthy.execution.as_ref().unwrap();
    assert_eq!(execution.requested_mode, RequestedMode::Default);
    assert_eq!(execution.served_mode, ServedMode::Hybrid);
    assert!(!execution.incomplete);
    let publication = healthy.publication;

    for failure in 1..=4 {
        transport.failure.store(failure, Ordering::Release);
        let calls = transport.calls();
        let error = kernel.default_search(&query).unwrap_err();
        assert!(
            error
                .downcast_ref::<embedding_worker::HelperUnavailable>()
                .is_none()
        );
        assert!(airk_core::search_execution::helper_unavailable_receipt(&error).is_none());
        assert_eq!(
            transport.calls() - calls,
            1,
            "unrelated error retried helper I/O"
        );
        assert_eq!(kernel.status()?.publication, publication);
    }
    transport.failure.store(0, Ordering::Release);
    transport.available.store(false, Ordering::Release);
    let mut empty_query = query.clone();
    empty_query.text = "unseen_literal_marker_691".into();
    let empty = kernel.default_search(&empty_query)?;
    assert!(empty.hits.is_empty());
    assert_eq!(
        empty.execution.as_ref().unwrap().incomplete_reason,
        Some(IncompleteReason::HelperUnavailable)
    );
    assert!(
        !empty.truncated,
        "semantic availability is not a count/text cut"
    );

    work(identity, 40, || kernel.configure_vectors(false))?;
    let calls = transport.calls();
    let opted_out = kernel.default_search(&query)?;
    let execution = opted_out.execution.unwrap();
    assert_eq!(execution.requested_mode, RequestedMode::Default);
    assert_eq!(execution.served_mode, ServedMode::Lexical);
    assert!(!execution.incomplete, "saved opt-out was called an outage");
    assert_eq!(
        transport.calls(),
        calls,
        "saved opt-out contacted the helper"
    );
    work(identity, 41, || kernel.configure_vectors(true))?;

    // Exercise the actual Kernel Workbench fallback, not a renderer or injected
    // pre-rendered result. Off preserves the separate source-verification choice.
    let execute_plan = |mode: Option<&str>| -> Result<airk_core::workbench::ResultEnvelope> {
        let mut arguments = serde_json::json!({
            "query":"copper", "path_prefix":"memo-00.md", "full_chunks":true,
            "limit":4, "max_bytes":4096
        });
        if let Some(mode) = mode {
            arguments["mode"] = mode.into();
        }
        kernel.evidence_plan(serde_json::from_value(serde_json::json!({
            "schema":"airk.parse.request/1", "mode":"execute", "verification":"off",
            "plan":{"steps":[{"id":"outage","tool":"search","args":arguments}]}
        }))?)
    };
    let strict = execute_plan(Some("vector"))?;
    assert_eq!(strict.status, "failed");
    assert_eq!(strict.steps[0].status, "failed");
    let envelope = strict.steps[0].result.as_ref().unwrap();
    assert_eq!(envelope["isError"], true);
    let body: serde_json::Value =
        serde_json::from_str(envelope["content"][0]["text"].as_str().unwrap())?;
    let direct_error = kernel.semantic_search(&query).unwrap_err();
    assert_eq!(
        body,
        airk_core::search_execution::helper_unavailable_receipt(&direct_error).unwrap()
    );
    let allowed = execute_plan(None)?;
    assert_eq!(allowed.status, "partial");
    assert_eq!(allowed.steps[0].status, "complete");
    let body = allowed.steps[0].result.as_ref().unwrap();
    assert_eq!(body["execution"]["incomplete_reason"], "helper_unavailable");
    assert_eq!(body["truncated"], false);
    assert!(
        !allowed.truncated,
        "semantic degradation was reported as clipping"
    );

    transport.available.store(true, Ordering::Release);
    let recovered = kernel.default_search(&query)?;
    assert!(!recovered.execution.unwrap().incomplete);
    assert_eq!(
        recovered.publication, publication,
        "helper recovery replaced indexed evidence"
    );
    Ok(())
}
fn work<T>(identity: WorkerIdentity, turn: u8, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    let allowance = WorkAllowance {
        id: Incarnation::from_bytes([turn; 16]),
        identity,
        job: Sha256Digest::from_bytes([turn; 32]),
        extraction_threads: 1,
        inference_threads: 1,
        working_memory_bytes: 768 << 20,
        background_io_bytes_per_second: 32 << 20,
        ttl_ms: 300_000,
    };
    service_execution::activate(&allowance)?;
    service_execution::run(allowance, operation)
}

#[test]
fn source_and_replacement_publish_without_helper_and_semantics_resume_in_bounded_turns()
-> Result<()> {
    // Retained reads require the exact saved spelling, including on hosts
    // whose temporary directory is a symlink or a Windows short-name alias.
    let owner = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    let runtime = owner.path().join("runtime");
    let model = owner.path().join("model");
    for path in [&source, &data, &runtime, &model] {
        fs::create_dir(path)?;
    }
    for n in 0..40 {
        fs::write(
            source.join(format!("memo-{n:02}.md")),
            format!(
                "Copper beacon original entry {}.\n",
                if n == 39 { 0 } else { n }
            ),
        )?;
    }
    let vocabulary = br#"{"copper":[1.0,0.0],"beacon":[0.0,1.0]}"#;
    fs::write(model.join("vocabulary.json"), vocabulary)?;
    fs::write(
        model.join("model.json"),
        serde_json::to_vec(
            &serde_json::json!({"schema":1,"name":"original bounded phase fixture","license":"Apache-2.0","dimensions":2,"sha256":format!("{:x}",Sha256::digest(vocabulary))}),
        )?,
    )?;
    let plan = airk_retrieval::model::inspect_object_plan(&model)?;
    let mut files = ServiceModelFiles::open(&runtime)?;
    let assets = files.install_readers(&plan, |name| {
        Ok(Box::new(fs::File::open(model.join(name.filename()))?))
    })?;
    files.publish_default(&plan)?;
    let transport = Arc::new(Transport {
        available: AtomicBool::new(false),
        failure: AtomicUsize::new(0),
        requests: AtomicUsize::new(0),
        opens: AtomicUsize::new(0),
        embedded: AtomicUsize::new(0),
    });
    embedding_worker::install_connector(Arc::new(Adapter(transport.clone())))?;
    let kernel = Kernel::open_service_collection(&source, Some(data.clone()), "phases")?
        .with_service_models(&runtime, plan.clone())?;
    let project = airk_kernel::service_projects::registration(&source, &data, "phases")?;
    let identity = WorkerIdentity {
        service: Incarnation::from_bytes([81; 16]),
        worker: Incarnation::from_bytes([82; 16]),
        project: project.binding,
        fencing_generation: 1,
    };
    service_execution::install(&data, identity)?;
    let source_report = work(identity, 1, || kernel.index_service_sources())?;
    assert_eq!(source_report.indexed, 40);
    assert_eq!(
        kernel.status()?.semantic_readiness.state,
        SemanticState::Pending
    );
    assert_eq!(
        transport.opens.load(Ordering::Acquire),
        0,
        "source publication opened the helper"
    );
    assert!(
        !kernel
            .search(&SearchQuery {
                text: "copper".into(),
                path_prefix: None,
                limit: 4,
                max_bytes: 4096
            })?
            .hits
            .is_empty()
    );
    let original = kernel.status()?.publication.unwrap().token;
    outage_policy_matrix(&kernel, &transport, identity, SemanticState::Pending, 20)?;
    assert!(work(identity, 2, || kernel.service_enrichment_turn()).is_err());
    assert_eq!(
        kernel.status()?.semantic_readiness.state,
        SemanticState::Failed
    );
    outage_policy_matrix(&kernel, &transport, identity, SemanticState::Failed, 24)?;
    assert!(
        !kernel
            .search(&SearchQuery {
                text: "beacon".into(),
                path_prefix: None,
                limit: 4,
                max_bytes: 4096
            })?
            .hits
            .is_empty()
    );
    let attempts = transport.opens.load(Ordering::Acquire);
    let rebuilt = work(identity, 3, || {
        kernel.rebuild_service(Sha256Digest::from_bytes([3; 32]))
    })?;
    assert_ne!(rebuilt.incarnation, original.incarnation);
    assert_eq!(
        transport.opens.load(Ordering::Acquire),
        attempts,
        "replacement opened the helper"
    );
    let mut current = kernel.refresh_service_snapshot()?;
    let offline = owner.path().join("offline-source");
    fs::rename(&source, &offline)?;
    transport.available.store(true, Ordering::Release);
    let first = work(identity, 4, || current.service_enrichment_turn())?;
    let readiness = first.semantic_readiness.unwrap();
    assert_eq!(readiness.state, SemanticState::Partial);
    assert_eq!(readiness.completed_artifacts, 32);
    assert_eq!(readiness.selected_artifacts, 40);
    outage_policy_matrix(&current, &transport, identity, SemanticState::Partial, 28)?;
    work(identity, 5, || current.service_enrichment_turn())?;
    assert_eq!(
        current.status()?.semantic_readiness.state,
        SemanticState::Ready
    );
    assert_eq!(
        fs::read_to_string(offline.join("memo-00.md"))?,
        "Copper beacon original entry 0.\n"
    );
    assert!(
        !data.join("models").exists(),
        "service copied collection-local model assets"
    );
    assert!(assets.directory().is_dir());
    let before = current.status()?;
    let embedded = transport.embedded.load(Ordering::Acquire);
    let regeneration = Sha256Digest::from_bytes([6; 32]);
    let queued = work(identity, 6, || {
        current.begin_service_vector_regeneration(regeneration)
    })?;
    assert_eq!(queued.state, SemanticState::Pending);
    assert_eq!(queued.selected_artifacts, 40);
    assert_eq!(queued.completed_artifacts, 0);
    assert_eq!(current.status()?.indexed_vectors, before.indexed_vectors);
    assert_eq!(
        current.status()?.publication.unwrap().token.incarnation,
        before.publication.unwrap().token.incarnation
    );
    let query = SearchQuery {
        text: "beacon".into(),
        path_prefix: None,
        limit: 4,
        max_bytes: 4096,
    };
    assert!(!current.search(&query)?.hits.is_empty());
    let queued_token = current.status()?.publication.unwrap().token;
    work(identity, 7, || {
        current.begin_service_vector_regeneration(regeneration)
    })?;
    assert_eq!(
        current.status()?.publication.unwrap().token,
        queued_token,
        "same accepted job requeued the manifest"
    );
    let partial = work(identity, 8, || current.service_enrichment_turn())?
        .semantic_readiness
        .unwrap();
    assert_eq!(partial.completed_artifacts, 32);
    let before_queries = transport.embedded.load(Ordering::Acquire);
    for path in ["memo-00.md", "memo-39.md"] {
        let shared = current.semantic_search(&SearchQuery {
            text: "beacon".into(),
            path_prefix: Some(path.into()),
            limit: 4,
            max_bytes: 4096,
        })?;
        assert!(
            shared.hits.iter().any(|hit| hit.citation.path == path),
            "citation lost between regeneration turns: {path}"
        );
    }
    let query_inputs = transport.embedded.load(Ordering::Acquire) - before_queries;
    assert_eq!(
        query_inputs, 2,
        "account query inference separately from regeneration"
    );
    current =
        Kernel::open_service_collection_for_diagnostics(&source, Some(data.clone()), "phases")?
            .with_service_models(&runtime, plan.clone())?;
    work(identity, 9, || {
        current.begin_service_vector_regeneration(regeneration)
    })?;
    assert_eq!(current.status()?.semantic_readiness.completed_artifacts, 32);
    work(identity, 10, || current.service_enrichment_turn())?;
    assert_eq!(
        current.status()?.semantic_readiness.state,
        SemanticState::Ready
    );
    assert_eq!(
        transport.embedded.load(Ordering::Acquire),
        embedded + 39 + query_inputs,
        "regeneration must recompute old inputs once and share fresh identical inputs across turns and reopen"
    );
    assert_eq!(
        fs::read_to_string(offline.join("memo-00.md"))?,
        "Copper beacon original entry 0.\n"
    );
    for path in ["memo-00.md", "memo-39.md"] {
        let all = current.semantic_search(&SearchQuery {
            text: "beacon".into(),
            path_prefix: Some(path.into()),
            limit: 4,
            max_bytes: 4096,
        })?;
        assert!(
            all.hits.iter().any(|hit| hit.citation.path == path),
            "shared citation lost: {path}"
        );
    }
    // A deleted input can leave a proven empty/tombstoned binding. Readding
    // that source and explicitly regenerating must not require a new live-to-
    // dead transition for the node that was already dead in the base.
    fs::rename(&offline, &source)?;
    fs::remove_file(source.join("memo-00.md"))?;
    fs::remove_file(source.join("memo-39.md"))?;
    work(identity, 11, || current.index_service_sources())?;
    fs::write(
        source.join("memo-00.md"),
        "Copper beacon original entry 0.\n",
    )?;
    work(identity, 12, || current.index_service_sources())?;
    work(identity, 13, || {
        current.begin_service_vector_regeneration(Sha256Digest::from_bytes([13; 32]))
    })?;
    work(identity, 14, || current.service_enrichment_turn())?;
    work(identity, 15, || current.service_enrichment_turn())?;
    let ready = current.status()?.semantic_readiness;
    assert_eq!(ready.state, SemanticState::Ready);
    assert_eq!(ready.completed_artifacts, 39);
    assert!(
        !current
            .semantic_search(&SearchQuery {
                text: "beacon".into(),
                path_prefix: Some("memo-00.md".into()),
                limit: 4,
                max_bytes: 4096,
            })?
            .hits
            .is_empty()
    );
    assert_eq!(
        fs::read_to_string(source.join("memo-00.md"))?,
        "Copper beacon original entry 0.\n"
    );
    ready_execution_observations(&current, &transport, identity)?;
    Ok(())
}
