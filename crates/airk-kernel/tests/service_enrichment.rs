// SPDX-License-Identifier: Apache-2.0
//! A separate process keeps the injected transport and service owner isolated.
use airk_core::{
    SearchQuery,
    identity::{Incarnation, Sha256Digest},
    readiness::SemanticState,
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
    opens: AtomicUsize,
    embedded: AtomicUsize,
}
struct Adapter(Arc<Transport>);
impl Connector for Adapter {
    fn open(&self, location: ModelLocation, _: &str) -> Result<Arc<dyn RemoteSession>> {
        ensure!(
            matches!(location, ModelLocation::Shared { .. }),
            "service tried a collection-local model"
        );
        self.0.opens.fetch_add(1, Ordering::AcqRel);
        ensure!(
            self.0.available.load(Ordering::Acquire),
            "fixture helper is unavailable"
        );
        Ok(self.0.clone())
    }
}
impl RemoteSession for Transport {
    fn request(&self, request: Request) -> Result<Response> {
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
    let owner = tempfile::tempdir()?;
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
        opens: AtomicUsize::new(0),
        embedded: AtomicUsize::new(0),
    });
    embedding_worker::install_connector(Arc::new(Adapter(transport.clone())))?;
    let kernel = Kernel::open_service_collection(&source, Some(data.clone()), "phases")?
        .with_service_models(&runtime, plan)?;
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
    assert!(work(identity, 2, || kernel.service_enrichment_turn()).is_err());
    assert_eq!(
        kernel.status()?.semantic_readiness.state,
        SemanticState::Failed
    );
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
    let current = kernel.refresh_service_snapshot()?;
    let offline = owner.path().join("offline-source");
    fs::rename(&source, &offline)?;
    transport.available.store(true, Ordering::Release);
    let first = work(identity, 4, || current.service_enrichment_turn())?;
    let readiness = first.semantic_readiness.unwrap();
    assert_eq!(readiness.state, SemanticState::Partial);
    assert_eq!(readiness.completed_artifacts, 32);
    assert_eq!(readiness.selected_artifacts, 40);
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
    work(identity, 9, || {
        current.begin_service_vector_regeneration(regeneration)
    })?;
    assert_eq!(current.status()?.semantic_readiness.completed_artifacts, 32);
    work(identity, 10, || current.service_enrichment_turn())?;
    assert_eq!(
        current.status()?.semantic_readiness.state,
        SemanticState::Ready
    );
    assert!(
        transport.embedded.load(Ordering::Acquire) >= embedded + 39,
        "regeneration reused cached embeddings"
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
    Ok(())
}
