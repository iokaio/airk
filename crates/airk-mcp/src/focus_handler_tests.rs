// SPDX-License-Identifier: Apache-2.0
//! Real indexed handler/SDK execution with synthetic native Focus scoring.
//! This is not optional-model, installed-host or network transport qualification.
use super::*;
use airk_core::{
    arbiter::*,
    identity::{Incarnation, Sha256Digest},
    native_focus::*,
    read_attempt::{ReadAttemptBudget, ReadClock},
    service::{ScopedReadRef, WorkerIdentity},
};
use airk_kernel::arbiter::NativeController;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn digest(byte: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([byte; 32])
}
fn clock() -> ReadClock {
    if cfg!(windows) {
        ReadClock::WindowsPerformanceCounter
    } else {
        ReadClock::UnixMonotonic
    }
}
fn identity() -> FocusIdentity {
    FocusIdentity {
        profile: digest(20),
        runtime: RUNTIME.into(),
        compiler: digest(21),
        calibration: digest(22),
        task_policy: digest(23),
    }
}
struct FocusOwner {
    enabled: bool,
    requests: Mutex<Vec<NativeFocusRequest>>,
}
impl NativeController for FocusOwner {
    fn application(
        &self,
        _: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        Ok(IndexApplication {
            scope: ScopeBinding {
                installation: digest(7),
                catalog: Incarnation::from_bytes([8; 16]),
                index_id: authority.index_id,
                authorization: authority.fingerprint,
                configuration_revision: 1,
                publication: publication.clone(),
            },
            profile: None,
            availability: Availability::Disabled,
            reason: "Context independently disabled in the handler fixture".into(),
        })
    }
    fn focus_application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<Option<FocusApplication>> {
        if !self.enabled {
            return Ok(None);
        }
        let mut application = self.application(worker, authority, publication)?;
        application.profile = Some(identity().profile);
        application.availability = Availability::Ready;
        application.reason = "Synthetic Focus handler owner".into();
        Ok(Some(FocusApplication {
            application,
            identity: Some(identity()),
            limits: Some(FocusProfileLimits {
                max_candidates: 2,
                pair_input_tokens: 32,
                max_input_tokens: 64,
            }),
        }))
    }
    fn focus_clock(&self) -> Result<Option<(ReadClock, u64)>> {
        Ok(Some((clock(), 1000)))
    }
    fn focus_rank(
        &self,
        request: NativeFocusRequest,
        _: Instant,
    ) -> Result<Option<NativeFocusResponse>> {
        request.validate(request.worker, request.authority, clock(), 1000)?;
        ensure!(
            request.candidates.iter().all(|candidate| candidate
                .hit
                .citation
                .path
                .starts_with("public/")),
            "excluded content entered Focus"
        );
        self.requests.lock().unwrap().push(request.clone());
        let order = request.candidate_order();
        let scores = order
            .iter()
            .take(request.score_count)
            .enumerate()
            .map(|(index, id)| FocusScore::from_logits(*id, 0., index as f32))
            .collect::<Result<Vec<_>>>()?;
        let mut ranked_order = order[..request.score_count]
            .iter()
            .rev()
            .copied()
            .collect::<Vec<_>>();
        ranked_order.extend_from_slice(&order[request.score_count..]);
        Ok(Some(NativeFocusResponse {
            ranking: FocusApplicationReceipt {
                schema: airk_core::arbiter::focus::RANKING_SCHEMA.into(),
                scope: request.scope.clone(),
                identity: Some(identity()),
                availability: Availability::Ready,
                reason: "Complete synthetic relevance prefix".into(),
                applied: true,
                input_digest: request.input_digest()?,
                original_order: order,
                ranked_order,
                scores,
                presentations: request
                    .candidates
                    .iter()
                    .take(request.score_count)
                    .map(|candidate| FocusPresentation {
                        candidate: candidate.id,
                        utf8_start: candidate.window.utf8_start,
                        utf8_end: candidate.window.utf8_end,
                        source_bytes: candidate.hit.text.len(),
                    })
                    .collect(),
                presented_count: request.score_count,
                candidate_count: request.candidates.len(),
                scored_count: request.score_count,
                omitted_count: request.candidates.len() - request.score_count,
                reserved_input_tokens: request.score_count * 32,
                observed_input_tokens: Some(request.score_count * 3),
                work: Work {
                    input_tokens: request.score_count * 3,
                    candidates: request.score_count,
                    compile_us: Some(1),
                    inference_us: Some(2),
                    ..Work::default()
                },
            },
        }))
    }
}
fn attempt() -> crate::read_attempt::ReadAttemptContext {
    crate::read_attempt::ReadAttemptContext::new(
        Instant::now() + Duration::from_secs(30),
        tokio_util::sync::CancellationToken::new(),
    )
    .with_wire_attempt(ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([30; 16]),
        clock: clock(),
        expires_monotonic_ns: 30_000_001_000,
        remaining_ns: 30_000_000_000,
        native_retry_expires_monotonic_ns: None,
    })
    .unwrap()
}
fn payload(reply: CallToolResult) -> Result<Value> {
    ensure!(reply.is_error != Some(true), "{reply:?}");
    Ok(serde_json::from_str(
        &reply.content[0].as_text().unwrap().text,
    )?)
}
fn normalize(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for key in [
                "request_id",
                "observed_unix_ms",
                "cache_age_ms",
                "cached",
                "observed_from_unix_ms",
                "observed_to_unix_ms",
            ] {
                fields.remove(key);
            }
            for value in fields.values_mut() {
                normalize(value)
            }
        }
        Value::Array(values) => {
            for value in values {
                normalize(value)
            }
        }
        _ => {}
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn focus_actual_search_handler_preserves_full_scoped_results_with_context_off() -> Result<()>
{
    let temporary = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let source = temporary.path().join("source");
    let data = temporary.path().join("data");
    std::fs::create_dir_all(source.join("public"))?;
    std::fs::create_dir_all(source.join("private"))?;
    std::fs::write(source.join("public/a.txt"), "amber alpha\n")?;
    std::fs::write(source.join("public/b.txt"), "amber beta\n")?;
    std::fs::write(
        source.join("private/secret.txt"),
        "amber PRIVATE_NEVER_OPTIONAL\n",
    )?;
    let model = data.join("models/local");
    std::fs::create_dir_all(&model)?;
    std::fs::write(
        model.join("vocabulary.json"),
        br#"{"amber":[1.0,0.0],"alpha":[0.8,0.2],"beta":[0.7,0.3]}"#,
    )?;
    std::fs::write(
        model.join("model.json"),
        serde_json::to_vec(
            &json!({"schema":1,"name":"focus-handler-word-vector-fixture","license":"Apache-2.0","dimensions":2,
        "sha256":"2b9dd16aedae16d0b95ac4ef700d04465c401b3bb9f867ceb5fc35617cd2fb87"}),
        )?,
    )?;
    let base = Kernel::open(&source, Some(data))?;
    base.index()?;
    let authority = crate::scoped_observation_tests::authority(&base)?;
    let scoped = base.with_scoped_reads(authority.clone())?;
    let reference = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: authority.index_id,
        grant_id: Some(authority.grant.id),
        grant_revision: Some(authority.grant.revision),
        selection: None,
        fingerprint: scoped.scoped_read_fingerprint()?.unwrap(),
    };
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([8; 16]),
        worker: Incarnation::from_bytes([9; 16]),
        project: authority.backing,
        fencing_generation: 1,
    };
    let off = Arc::new(FocusOwner {
        enabled: false,
        requests: Mutex::new(vec![]),
    });
    let on = Arc::new(FocusOwner {
        enabled: true,
        requests: Mutex::new(vec![]),
    });
    let mut ordinary =
        local::LocalTools::new(scoped.with_native_arbiter(worker, reference, off.clone())?).await?;
    let mut focused =
        local::LocalTools::new(scoped.with_native_arbiter(worker, reference, on.clone())?).await?;
    let args = json!({"query":"amber","mode":"vector","limit":2,"max_bytes":4096,"response_mode":"full","verification":"off"});
    let mut expected = payload(
        ordinary
            .call_with_read_context(
                "airk_search".into(),
                args.as_object().unwrap().clone(),
                attempt(),
            )
            .await?,
    )?;
    let mut result = payload(
        focused
            .call_with_read_context(
                "airk_search".into(),
                args.as_object().unwrap().clone(),
                attempt(),
            )
            .await?,
    )?;
    assert_eq!(expected["hits"].as_array().unwrap().len(), 2);
    let original = expected["hits"].as_array().unwrap().clone();
    assert_eq!(result["hits"], json!([original[1], original[0]]));
    assert_eq!(result["focus"]["kind"], "receipt");
    assert_eq!(result["focus"]["receipt"]["scored_count"], 2);
    let bindings = airk_core::combined_context::context_delivery_bindings(&result)?;
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].stage, Stage::Focus);
    assert_eq!(bindings[0].profile, Some(identity().profile));
    let mut terse = result.clone();
    crate::response::Session::default().display(
        &mut terse,
        crate::response::ResponseMode::Terse,
        false,
    );
    assert_eq!(terse["generation"], result["generation"]);
    assert_eq!(terse["publication"], result["publication"]);
    assert_eq!(
        airk_core::combined_context::context_delivery_bindings(&terse)?,
        bindings
    );
    result.as_object_mut().unwrap().remove("focus");
    result["hits"] = json!(original);
    normalize(&mut result);
    normalize(&mut expected);
    assert_eq!(result, expected);
    assert_eq!(on.requests.lock().unwrap().len(), 1);
    assert!(off.requests.lock().unwrap().is_empty());
    assert!(!serde_json::to_string(&result)?.contains("PRIVATE_NEVER_OPTIONAL"));
    ordinary.close().await?;
    focused.close().await?;
    Ok(())
}

/// `FocusOwner`, whose configuration revision moves once its native ranking
/// has returned: the Focus result is stale after model work.
struct StaleFocus {
    owner: FocusOwner,
    revision: std::sync::atomic::AtomicU64,
}
impl NativeController for StaleFocus {
    fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        let mut application = self.owner.application(worker, authority, publication)?;
        application.scope.configuration_revision =
            self.revision.load(std::sync::atomic::Ordering::SeqCst);
        Ok(application)
    }
    fn focus_application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<Option<FocusApplication>> {
        let mut focus = self
            .owner
            .focus_application(worker, authority, publication)?;
        if let Some(focus) = &mut focus {
            focus.application.scope.configuration_revision =
                self.revision.load(std::sync::atomic::Ordering::SeqCst);
        }
        Ok(focus)
    }
    fn focus_clock(&self) -> Result<Option<(ReadClock, u64)>> {
        self.owner.focus_clock()
    }
    fn focus_rank(
        &self,
        request: NativeFocusRequest,
        deadline: Instant,
    ) -> Result<Option<NativeFocusResponse>> {
        let response = self.owner.focus_rank(request, deadline)?;
        self.revision.store(2, std::sync::atomic::Ordering::SeqCst);
        Ok(response)
    }
}
/// Change-log entry 67: `FocusPostExecutionStale` renders as the typed,
/// nonreplayable `arbiter_context_action_stale` (it rendered `read_failed`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn focus_policy_change_after_native_work_renders_typed_stale_refusal() -> Result<()> {
    let temporary = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let source = temporary.path().join("source");
    let data = temporary.path().join("data");
    std::fs::create_dir_all(source.join("public"))?;
    std::fs::write(source.join("public/a.txt"), "amber alpha\n")?;
    std::fs::write(source.join("public/b.txt"), "amber beta\n")?;
    let model = data.join("models/local");
    std::fs::create_dir_all(&model)?;
    std::fs::write(
        model.join("vocabulary.json"),
        br#"{"amber":[1.0,0.0],"alpha":[0.8,0.2],"beta":[0.7,0.3]}"#,
    )?;
    std::fs::write(
        model.join("model.json"),
        serde_json::to_vec(
            &json!({"schema":1,"name":"focus-handler-word-vector-fixture","license":"Apache-2.0","dimensions":2,
        "sha256":"2b9dd16aedae16d0b95ac4ef700d04465c401b3bb9f867ceb5fc35617cd2fb87"}),
        )?,
    )?;
    let base = Kernel::open(&source, Some(data))?;
    base.index()?;
    let authority = crate::scoped_observation_tests::authority(&base)?;
    let scoped = base.with_scoped_reads(authority.clone())?;
    let reference = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: authority.index_id,
        grant_id: Some(authority.grant.id),
        grant_revision: Some(authority.grant.revision),
        selection: None,
        fingerprint: scoped.scoped_read_fingerprint()?.unwrap(),
    };
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([8; 16]),
        worker: Incarnation::from_bytes([9; 16]),
        project: authority.backing,
        fencing_generation: 1,
    };
    let owner = Arc::new(StaleFocus {
        owner: FocusOwner {
            enabled: true,
            requests: Mutex::new(vec![]),
        },
        revision: 1.into(),
    });
    let mut tools =
        local::LocalTools::new(scoped.with_native_arbiter(worker, reference, owner.clone())?)
            .await?;
    let args = json!({"query":"amber","mode":"vector","limit":2,"max_bytes":4096,"response_mode":"full","verification":"off"});
    let reply = tools
        .call_with_read_context(
            "airk_search".into(),
            args.as_object().unwrap().clone(),
            attempt(),
        )
        .await?;
    assert_eq!(reply.is_error, Some(true), "{reply:?}");
    let refusal: Value = serde_json::from_str(&reply.content[0].as_text().unwrap().text)?;
    assert_eq!(
        refusal["error"]["code"], "arbiter_context_action_stale",
        "{refusal}"
    );
    assert_eq!(refusal["error"]["retryable"], false);
    assert_eq!(refusal["error"]["query_replay_safe"], false);
    assert_eq!(
        owner.owner.requests.lock().unwrap().len(),
        1,
        "never replayed"
    );
    tools.close().await?;
    Ok(())
}

#[test]
fn focus_sdk_headroom_preserves_scope_envelope_and_does_not_widen_status_budget() -> Result<()> {
    let response = airk_core::SearchResponse {
        focus: None,
        execution: None,
        publication: None,
        semantic_readiness: None,
        discovery_readiness: None,
        generation: 1,
        collection: "fixture".into(),
        hits: vec![],
        truncated: false,
    };
    let available = crate::response::focus_reply_headroom(&response, None, false)?;
    assert!(available > 1024);
    let notice = FocusObservation::not_applied_if_fits(
        Availability::Constrained,
        "Native budget prevented optional work; source remains unchanged.".into(),
        0,
        None,
        available,
    )?
    .unwrap();
    let mut observed = response.clone();
    observed.focus = Some(notice);
    let encoded =
        CallToolResult::success(vec![ContentBlock::text(serde_json::to_string(&observed)?)]);
    assert!(serde_json::to_vec(&encoded)?.len() + 2048 <= airk_core::MAX_RESPONSE_BYTES);
    assert_eq!(
        crate::response::focus_reply_headroom(&response, None, true)?,
        available
    );
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn focus_selection_requires_actual_singleton_allowance_and_retains_direct_custody()
-> Result<()> {
    use airk_core::combined_context::{ALLOWANCE_SCHEMA, PartitionAllowance, Resources};
    use airk_core::index_catalog::CatalogRevision;
    use airk_core::scoped_reads::{SelectionOwnerKind, SelectionScopeRef};
    let temporary = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let source = temporary.path().join("source");
    let data = temporary.path().join("data");
    std::fs::create_dir_all(source.join("public"))?;
    std::fs::create_dir_all(source.join("private"))?;
    std::fs::write(source.join("public/a.txt"), "amber alpha\n")?;
    std::fs::write(source.join("public/b.txt"), "amber beta\n")?;
    std::fs::write(
        source.join("private/secret.txt"),
        "amber PRIVATE_NEVER_OPTIONAL\n",
    )?;
    let model = data.join("models/local");
    std::fs::create_dir_all(&model)?;
    std::fs::write(
        model.join("vocabulary.json"),
        br#"{"amber":[1.0,0.0],"alpha":[0.8,0.2],"beta":[0.7,0.3]}"#,
    )?;
    std::fs::write(
        model.join("model.json"),
        serde_json::to_vec(
            &json!({"schema":1,"name":"focus-handler-word-vector-fixture","license":"Apache-2.0","dimensions":2,"sha256":"2b9dd16aedae16d0b95ac4ef700d04465c401b3bb9f867ceb5fc35617cd2fb87"}),
        )?,
    )?;
    let base = Kernel::open(&source, Some(data))?;
    base.index()?;
    let reviewed = crate::scoped_observation_tests::authority(&base)?;
    let member = &reviewed.grant.memberships[0].membership;
    let selection = SelectionScopeRef {
        owner: SelectionOwnerKind::Window,
        grant_nonce: Incarnation::from_bytes([60; 16]),
        service_instance: Incarnation::from_bytes([61; 16]),
        scope_identity: digest(62),
        catalog_revision: CatalogRevision::INITIAL,
    };
    let selected =
        base.with_selection_reads(airk_kernel::scoped_reads::SelectionReadAuthority {
            schema: 1,
            backing: reviewed.backing,
            index_id: reviewed.index_id,
            project_ids: reviewed.project_ids.clone(),
            selection,
            atoms: member
                .scope
                .iter()
                .cloned()
                .map(|atom| (member.project_id, atom))
                .collect(),
            sources: reviewed.sources.clone(),
            membership_revisions: vec![(member.project_id, CatalogRevision::INITIAL)],
            source_revision: CatalogRevision::INITIAL,
            policy_epoch: 0,
            policy_digest: None,
        })?;
    let authority = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: reviewed.index_id,
        grant_id: None,
        grant_revision: None,
        selection: Some(selection),
        fingerprint: selected.scoped_read_fingerprint()?.unwrap(),
    };
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([8; 16]),
        worker: Incarnation::from_bytes([9; 16]),
        project: reviewed.backing,
        fencing_generation: 1,
    };
    let owner = Arc::new(FocusOwner {
        enabled: true,
        requests: Mutex::new(vec![]),
    });
    let mut tools =
        local::LocalTools::new(selected.with_native_arbiter(worker, authority, owner.clone())?)
            .await?;
    let args = json!({"query":"amber","mode":"vector","limit":2,"max_bytes":4096,"response_mode":"full","verification":"off"});
    // An installed per-index Kernel cannot infer original selection cardinality.
    let unpartitioned = payload(
        tools
            .call_with_read_context(
                "airk_search".into(),
                args.as_object().unwrap().clone(),
                attempt(),
            )
            .await?,
    )?;
    assert_eq!(unpartitioned["focus"]["kind"], "not_applied");
    assert!(owner.requests.lock().unwrap().is_empty());
    let parent = Budget {
        calls: 1,
        rounds: 1,
        fanout: 1,
        input_tokens: 64,
        candidates: 2,
        max_bytes: 65_536,
        deadline_ms: 30_000,
    };
    let context = attempt();
    let original_attempt = context.captured_transport_attempt().unwrap();
    let allowance = PartitionAllowance {
        schema: ALLOWANCE_SCHEMA.into(),
        attempt: original_attempt.request_id,
        selection: selection.scope_identity,
        index_id: authority.index_id,
        resources: Resources::budget(&parent),
        parent_budget: parent.clone(),
        assessment_ordinals: vec![],
    };
    assert!(allowance.is_ordinary_focus());
    let context = context.with_context_partition(allowance.clone())?;
    let focused = payload(
        tools
            .call_with_read_context(
                "airk_search".into(),
                args.as_object().unwrap().clone(),
                context.clone(),
            )
            .await?,
    )?;
    assert_eq!(focused["focus"]["kind"], "receipt");
    {
        let requests = owner.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].parent_budget, parent);
        assert_eq!(
            requests[0].partition.as_ref().unwrap().selection,
            selection.scope_identity
        );
        assert_eq!(
            requests[0].read_attempt.request_id,
            original_attempt.request_id
        );
    }
    let mut restored = focused.clone();
    restored.as_object_mut().unwrap().remove("focus");
    let hits = restored["hits"].as_array_mut().unwrap();
    hits.reverse();
    let mut original = unpartitioned.clone();
    original.as_object_mut().unwrap().remove("focus");
    normalize(&mut restored);
    normalize(&mut original);
    assert_eq!(restored, original);
    let registrations = Arc::new(Mutex::new(vec![]));
    let captured = registrations.clone();
    struct Guard;
    impl crate::read_attempt::ReadDeliveryGuard for Guard {
        fn bindings_digest(&self) -> Sha256Digest {
            digest(63)
        }
        fn poll(
            &self,
            effect: &mut dyn FnMut() -> std::task::Poll<std::io::Result<usize>>,
        ) -> std::task::Poll<std::io::Result<usize>> {
            effect()
        }
    }
    let context = context.with_delivery_guard_factory(Arc::new(move |bindings, _| {
        captured.lock().unwrap().push(bindings.to_vec());
        Ok(Arc::new(Guard))
    }));
    context.retain_delivery_for(&focused)?;
    {
        let registered = registrations.lock().unwrap();
        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0].len(), 1);
        assert_eq!(registered[0][0].stage, Stage::Focus);
    }
    assert!(
        tools
            .call_with_read_context(
                "airk_answer".into(),
                json!({}).as_object().unwrap().clone(),
                context
            )
            .await
            .is_err()
    );
    tools.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_sdk_refuses_partition_on_other_canonical_tools_before_dispatch() -> Result<()> {
    let temporary = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let source = temporary.path().join("source");
    std::fs::create_dir_all(source.join("public"))?;
    let saved = b"fixed source remains unchanged\n";
    std::fs::write(source.join("public/a.txt"), saved)?;
    let base = Kernel::open(&source, Some(temporary.path().join("data")))?;
    base.configure_vectors(false)?;
    base.index()?;
    let reviewed = crate::scoped_observation_tests::authority(&base)?;
    let scoped = base.with_scoped_reads(reviewed.clone())?;
    let authority = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: reviewed.index_id,
        grant_id: Some(reviewed.grant.id),
        grant_revision: Some(reviewed.grant.revision),
        selection: None,
        fingerprint: scoped.scoped_read_fingerprint()?.unwrap(),
    };
    let owner = Arc::new(FocusOwner {
        enabled: true,
        requests: Mutex::new(vec![]),
    });
    let mut tools = local::LocalTools::new(scoped.with_native_arbiter(
        WorkerIdentity {
            service: Incarnation::from_bytes([8; 16]),
            worker: Incarnation::from_bytes([9; 16]),
            project: reviewed.backing,
            fencing_generation: 1,
        },
        authority,
        owner.clone(),
    )?)
    .await?;
    // Bundled Help is permitted by this exact reviewed scope. Global Status
    // is intentionally denied (see scoped_observation_tests), not a fixture
    // authority that an original attempt may replace.
    payload(tools.call("airk_help".into(), Default::default()).await?)?;
    let fixtures: Value = serde_json::from_slice(include_bytes!(
        "../../../fixtures/split/arbiter/focus_only_allowance_cases.json"
    ))?;
    for row in ["ordinary_focus", "context", "zero_optional_work"] {
        let allowance: airk_core::combined_context::PartitionAllowance =
            serde_json::from_value(fixtures[row]["allowance"].clone())?;
        let mut wire = attempt().wire_attempt().unwrap();
        wire.request_id = allowance.attempt;
        let context = crate::read_attempt::ReadAttemptContext::new(
            Instant::now() + Duration::from_secs(30),
            Default::default(),
        )
        .with_wire_attempt(wire)?
        .with_context_partition(allowance)?;
        assert!(context.captured_transport_attempt().is_some());
        for tool in ["airk_status", "status", "airk_help", "help"] {
            let error = tools
                .call_with_read_context(tool.into(), Default::default(), context.clone())
                .await
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("native partition purpose differs"),
                "{error:#}"
            );
        }
    }
    assert!(
        owner.requests.lock().unwrap().is_empty(),
        "a rejected purpose scored candidates"
    );
    assert_eq!(std::fs::read(source.join("public/a.txt"))?, saved);
    tools.close().await?;
    Ok(())
}
