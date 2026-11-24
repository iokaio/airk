// SPDX-License-Identifier: Apache-2.0
//! Canonical Focus owner tests with an injected scorer; no model-quality claim.
use super::*;
use airk_core::{
    Citation, DocumentLocation,
    arbiter::{
        FocusApplication, FocusApplicationReceipt, FocusIdentity, FocusPresentation,
        FocusProfileLimits, FocusScore,
    },
    identity::Incarnation,
    index_catalog::*,
    read_attempt::{ReadAttemptBudget, ReadClock},
    scoped_reads::SourceDescriptor,
    search_execution::{RequestedMode, SearchExecution},
    service::{ScopedReadRef, WorkerIdentity},
};
use std::collections::BTreeMap;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy)]
enum Mode {
    Off,
    Ranked,
    ContextReady,
    Tied,
    Unknown,
    Stale,
    /// A newer publication lands while the native ranking runs (entry 67).
    Republish,
    Refused,
    Foreign,
}
struct Controller {
    mode: Mode,
    /// The fixture's directory, for `Mode::Republish`.
    root: std::path::PathBuf,
    revision: AtomicU64,
    requests: Mutex<Vec<NativeFocusRequest>>,
    batches: Mutex<Vec<airk_core::native_loop::NativeFocusRequestV3>>,
}
fn digest(byte: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([byte; 32])
}
fn identity() -> FocusIdentity {
    FocusIdentity {
        profile: digest(30),
        runtime: RUNTIME.into(),
        compiler: digest(31),
        calibration: digest(32),
        task_policy: digest(33),
    }
}
impl NativeController for Controller {
    fn focus_batch(
        &self,
        request: airk_core::native_loop::NativeFocusRequestV3,
        _: Instant,
    ) -> Result<Option<airk_core::native_loop::NativeFocusResponseV3>> {
        use airk_core::arbiter::answer_loop::*;
        request.validate(
            request.worker,
            request.authority,
            ReadClock::UnixMonotonic,
            1000,
        )?;
        self.batches.lock().unwrap().push(request.clone());
        if matches!(self.mode, Mode::Unknown) {
            anyhow::bail!("synthetic lost native outcome");
        }
        let pairs: Vec<FocusPairScore> = request
            .candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| FocusPairScore {
                candidate: index as u16,
                question: candidate.question,
                no_logit: 0.,
                yes_logit: [0., 2., 1., -3.][index % 4],
            })
            .collect();
        let mut ranked: Vec<u16> = (0..request.candidates.len() as u16).collect();
        ranked.sort_by(|a, b| {
            pairs[usize::from(*b)]
                .yes_logit
                .total_cmp(&pairs[usize::from(*a)].yes_logit)
                .then(a.cmp(b))
        });
        let tokens = request.remaining_input_tokens;
        Ok(Some(airk_core::native_loop::NativeFocusResponseV3 {
            ranking: FocusBatchReceipt {
                schema: FOCUS_BATCH_SCHEMA.into(),
                scope_digest: scope_digest(&request.scope)?,
                identity: Some(identity()),
                availability: Availability::Ready,
                reason: "Synthetic batched scoring".into(),
                applied: true,
                input_digest: request.input_digest()?,
                order_digest: request.order_digest,
                questions_digest: questions_digest(&request.questions)?,
                question_count: request.questions.len() as u8,
                candidate_count: request.candidates.len(),
                ranked,
                work: Work {
                    input_tokens: tokens,
                    candidates: pairs.len(),
                    queue_us: Some(1),
                    compile_us: Some(2),
                    inference_us: Some(3),
                    policy_us: Some(4),
                    ..Work::default()
                },
                pairs,
                reserved_input_tokens: tokens,
                observed_input_tokens: Some(tokens),
                attribution: request.attribution.clone(),
            },
        }))
    }
    fn application(
        &self,
        _: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        Ok(IndexApplication {
            scope: ScopeBinding {
                installation: digest(28),
                catalog: Incarnation::from_bytes([29; 16]),
                index_id: authority.index_id,
                authorization: authority.fingerprint,
                configuration_revision: self.revision.load(Ordering::Acquire),
                publication: publication.clone(),
            },
            profile: matches!(self.mode, Mode::ContextReady).then_some(identity().profile),
            availability: if matches!(self.mode, Mode::ContextReady) {
                Availability::Ready
            } else {
                Availability::Disabled
            },
            reason: "Context is independently disabled in this fixture".into(),
        })
    }
    fn focus_application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<Option<FocusApplication>> {
        if matches!(self.mode, Mode::Off) {
            return Ok(None);
        }
        let mut application = self.application(worker, authority, publication)?;
        application.profile = Some(identity().profile);
        application.availability = Availability::Ready;
        application.reason = "Synthetic Focus owner, not a qualified model".into();
        Ok(Some(FocusApplication {
            application,
            identity: Some(identity()),
            limits: Some(FocusProfileLimits {
                max_candidates: 3,
                pair_input_tokens: 32,
                max_input_tokens: 96,
            }),
        }))
    }
    fn focus_clock(&self) -> Result<Option<(ReadClock, u64)>> {
        Ok(Some((ReadClock::UnixMonotonic, 1000)))
    }
    fn focus_rank(
        &self,
        request: NativeFocusRequest,
        _: Instant,
    ) -> Result<Option<airk_core::native_focus::NativeFocusResponse>> {
        request.validate(
            request.worker,
            request.authority,
            ReadClock::UnixMonotonic,
            1000,
        )?;
        self.requests.lock().unwrap().push(request.clone());
        if matches!(self.mode, Mode::Unknown) {
            anyhow::bail!("synthetic lost native outcome");
        }
        if matches!(self.mode, Mode::Refused) {
            return Err(airk_core::scoped_reads::ScopeReadRefused.into());
        }
        let order = request.candidate_order();
        let scores = order
            .iter()
            .take(request.score_count)
            .enumerate()
            .map(|(position, id)| {
                FocusScore::from_logits(
                    *id,
                    0.,
                    if matches!(self.mode, Mode::Tied) {
                        0.
                    } else {
                        [0., 2., 1.][position]
                    },
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let mut ranked = (0..request.score_count).collect::<Vec<_>>();
        ranked.sort_by(|a, b| {
            scores[*b]
                .relevance
                .total_cmp(&scores[*a].relevance)
                .then(a.cmp(b))
        });
        let ranked_order = ranked
            .into_iter()
            .map(|position| order[position])
            .chain(order[request.score_count..].iter().copied())
            .collect();
        let mut receipt = FocusApplicationReceipt {
            schema: airk_core::arbiter::focus::RANKING_SCHEMA.into(),
            scope: request.scope.clone(),
            identity: Some(identity()),
            availability: Availability::Ready,
            reason: "Complete synthetic prefix scoring".into(),
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
            observed_input_tokens: Some(
                request.score_count
                    * if matches!(self.mode, Mode::ContextReady) {
                        32
                    } else {
                        3
                    },
            ),
            work: Work {
                input_tokens: request.score_count
                    * if matches!(self.mode, Mode::ContextReady) {
                        32
                    } else {
                        3
                    },
                candidates: request.score_count,
                queue_us: Some(1),
                compile_us: Some(2),
                inference_us: Some(3),
                policy_us: Some(4),
                ..Work::default()
            },
        };
        if matches!(self.mode, Mode::Stale) {
            self.revision.store(2, Ordering::Release);
        }
        if matches!(self.mode, Mode::Republish) {
            let source = self.root.join("source");
            std::fs::write(source.join("public/later.md"), "# later\nA newer file.\n")?;
            crate::Kernel::open(&source, Some(self.root.join("data")))?.index()?;
        }
        if matches!(self.mode, Mode::Foreign) {
            receipt.ranked_order[0] = digest(99);
        }
        Ok(Some(airk_core::native_focus::NativeFocusResponse {
            ranking: receipt,
        }))
    }
}

fn fixture(
    mode: Mode,
) -> Result<(
    tempfile::TempDir,
    crate::Kernel,
    Arc<Controller>,
    SearchResponse,
)> {
    let temporary = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let source = temporary.path().join("source");
    std::fs::create_dir_all(source.join("public"))?;
    std::fs::write(
        source.join("public/policy.md"),
        "# policy\nThe original request keeps its deadline.\n",
    )?;
    let kernel = crate::Kernel::open(&source, Some(temporary.path().join("data")))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
    let index = IndexId::from_identity(Incarnation::from_bytes([2; 16]));
    let source_id = SourceRootId::from_identity(Incarnation::from_bytes([3; 16]));
    let authority = crate::scoped_reads::SelectionReadAuthority {
        schema: 1,
        backing: airk_core::service::ProjectRegistration::new(
            kernel.root().to_str().unwrap().into(),
            kernel.data_dir().to_str().unwrap().into(),
            kernel.collection().into(),
        )?
        .binding,
        index_id: index,
        project_ids: vec![project],
        selection: airk_core::scoped_reads::SelectionScopeRef {
            owner: airk_core::scoped_reads::SelectionOwnerKind::Window,
            grant_nonce: airk_core::identity::Incarnation::from_bytes([10; 16]),
            service_instance: airk_core::identity::Incarnation::from_bytes([11; 16]),
            scope_identity: airk_core::identity::Sha256Digest::from_bytes([12; 32]),
            catalog_revision: airk_core::index_catalog::CatalogRevision::try_from(5_u64).unwrap(),
        },
        atoms: vec![ScopeAtom {
            source_root_id: source_id,
            kind: PathScope::Subtree,
            path: "public".into(),
        }]
        .into_iter()
        .map(|atom| (project, atom))
        .collect(),
        sources: vec![SourceDescriptor {
            source_root_id: source_id,
            original_root: kernel.root().to_str().unwrap().into(),
        }],
        membership_revisions: vec![(project, airk_core::index_catalog::CatalogRevision::INITIAL)],
        source_revision: airk_core::index_catalog::CatalogRevision::INITIAL,
        policy_epoch: 0,
        policy_digest: None,
    };
    let scoped = kernel.with_selection_reads(authority.clone())?;
    let reference = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: index,
        selection: airk_core::scoped_reads::SelectionScopeRef {
            owner: airk_core::scoped_reads::SelectionOwnerKind::Window,
            grant_nonce: airk_core::identity::Incarnation::from_bytes([10; 16]),
            service_instance: airk_core::identity::Incarnation::from_bytes([11; 16]),
            scope_identity: airk_core::identity::Sha256Digest::from_bytes([12; 32]),
            catalog_revision: airk_core::index_catalog::CatalogRevision::try_from(5_u64).unwrap(),
        },
        fingerprint: scoped.scoped_read_fingerprint()?.unwrap(),
    };
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([8; 16]),
        worker: Incarnation::from_bytes([9; 16]),
        project: authority.backing,
        fencing_generation: 1,
    };
    let controller = Arc::new(Controller {
        mode,
        root: temporary.path().to_path_buf(),
        revision: AtomicU64::new(1),
        requests: Mutex::new(vec![]),
        batches: Mutex::new(vec![]),
    });
    let native = scoped
        .with_native_arbiter(worker, reference, controller.clone())?
        .with_optional_read_attempt(
            ReadAttemptBudget {
                schema: 1,
                request_id: Incarnation::from_bytes([10; 16]),
                clock: ReadClock::UnixMonotonic,
                expires_monotonic_ns: 30_000_001_000,
                remaining_ns: 30_000_000_000,
                native_retry_expires_monotonic_ns: None,
            },
            Instant::now() + Duration::from_secs(30),
        )?
        .with_focus_reply_budget(Arc::new(|response| {
            Ok(airk_core::MAX_RESPONSE_BYTES
                .saturating_sub(serde_json::to_vec(response)?.len().saturating_add(4096))
                / 2)
        }))?;
    let publication = native.arbiter_publication()?;
    let response = SearchResponse {
        focus: None,
        execution: Some(SearchExecution::new(
            RequestedMode::Vector,
            ServedMode::Vector,
            None,
        )),
        publication: Some(publication.clone()),
        semantic_readiness: None,
        discovery_readiness: None,
        generation: publication.generation,
        collection: publication.collection,
        hits: (0..4)
            .map(|position| Hit {
                citation: Citation {
                    path: format!("public/{position}.md"),
                    content_hash: digest(position + 40).to_string(),
                    start_line: 1,
                    end_line: 1,
                    location: DocumentLocation::Lines { start: 1, end: 1 },
                },
                text: format!("original source {position}"),
                truncated: false,
            })
            .collect(),
        truncated: true,
    };
    Ok((temporary, native, controller, response))
}
fn query() -> SearchQuery {
    SearchQuery {
        text: "Where is the rule?".into(),
        path_prefix: None,
        limit: 4,
        max_bytes: 4096,
    }
}

#[test]
fn direct_answer_entry_binds_actual_parent_before_its_focus_search() -> Result<()> {
    let (_temp, kernel, owner, original) = fixture(Mode::Ranked)?;
    let budget = Budget {
        calls: 3,
        rounds: 2,
        fanout: 2,
        candidates: 8,
        input_tokens: 96,
        max_bytes: 65_536,
        deadline_ms: 30_000,
    };
    let request = airk_core::arbiter::context_action::ContextRequestV2 {
        schema: airk_core::arbiter::context_action::REQUEST_SCHEMA_V2.into(),
        question: "Where is the rule?".into(),
        budget: budget.clone(),
    };
    let publication = original
        .publication
        .as_ref()
        .context("fixture publication")?;
    let expected_revision = publication.token.to_string();
    let response = kernel.answer_with_reader_v2(
        request,
        Instant::now() + Duration::from_secs(30),
        &|| Ok(()),
        &|tool, arguments, revision| {
            ensure!(revision == Some(expected_revision.as_str()), "fixture read changed its captured publication");
            if matches!(tool, "search" | "airk_search") {
                let mut source = original.clone();
                let limit = arguments["limit"].as_u64().unwrap() as usize;
                source.hits.truncate(limit);
                let query = SearchQuery {
                    text: arguments["query"].as_str().unwrap().into(),
                    path_prefix: None,
                    limit,
                    max_bytes: arguments["max_bytes"].as_u64().unwrap() as usize,
                };
                Ok(serde_json::to_value(kernel.focus_search(&query, source)?)?)
            } else {
                Ok(json!({"publication":publication,"error":{"code":"fixture_source_unavailable","retryable":false}}))
            }
        },
    )?;
    let requests = owner.requests.lock().unwrap();
    assert!(!requests.is_empty());
    assert!(
        requests
            .iter()
            .all(|request| request.parent_budget == budget)
    );
    assert!(
        requests
            .iter()
            .all(|request| request.read_attempt.request_id == Incarnation::from_bytes([10; 16]))
    );
    let observed = airk_core::arbiter::focus::observed_focus_work(&response.records, &budget)?;
    assert!(observed.has_native_receipt);
    assert!(observed.work.input_tokens > 0);
    assert!(response.work.input_tokens >= observed.work.input_tokens);
    assert!(
        response
            .records
            .iter()
            .all(|record| record.canonical_result["publication"]
                == serde_json::to_value(publication).unwrap())
    );
    Ok(())
}

#[test]
fn focus_reorders_only_complete_prefix_with_context_disabled_and_keeps_every_hit() -> Result<()> {
    let (_temp, kernel, owner, original) = fixture(Mode::Ranked)?;
    let result = kernel.focus_search(&query(), original.clone())?;
    assert_eq!(
        serde_json::to_value(&result.hits)?,
        serde_json::to_value([
            &original.hits[1],
            &original.hits[2],
            &original.hits[0],
            &original.hits[3]
        ])?
    );
    assert!(result.truncated);
    assert_eq!(result.publication, original.publication);
    let receipt = result.focus.as_ref().unwrap().receipt().unwrap();
    assert_eq!(receipt.scored_count, 3);
    assert_eq!(receipt.omitted_count, 1);
    assert_eq!(
        receipt.presentations[0].utf8_end,
        original.hits[0].text.len()
    );
    assert_eq!(receipt.work.input_tokens, 9);
    let retained = kernel.focus_search(&query(), result.clone())?;
    assert_eq!(serde_json::to_vec(&retained)?, serde_json::to_vec(&result)?);
    let requests = owner.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].read_attempt.request_id,
        Incarnation::from_bytes([10; 16])
    );
    assert_eq!(requests[0].score_count, 3);
    assert_eq!(requests[0].ordinal, 1);
    Ok(())
}

#[test]
fn ready_context_retains_capacity_after_focus_reaches_its_cumulative_ceiling() -> Result<()> {
    let (_temp, kernel, owner, original) = fixture(Mode::ContextReady)?;
    let budget = Budget {
        calls: 3,
        rounds: 2,
        fanout: 2,
        input_tokens: 96,
        candidates: 8,
        max_bytes: 65_536,
        deadline_ms: 30_000,
    };
    kernel.with_focus_parent_budget(budget, Instant::now() + Duration::from_secs(30))?;
    let context = kernel.arbiter_context.as_ref().unwrap();
    let application = owner.application(
        context.worker,
        context.authority,
        original.publication.as_ref().unwrap(),
    )?;
    assert_eq!(application.availability, Availability::Ready);
    kernel.bind_initial_context_readiness(true)?;
    let first = kernel.focus_search(&query(), original.clone())?;
    let receipt = first.focus.as_ref().unwrap().receipt().unwrap();
    assert_eq!(receipt.scored_count, 1);
    assert_eq!(receipt.observed_input_tokens, Some(32));
    let second = kernel.focus_search(&query(), original.clone())?;
    assert_eq!(
        serde_json::to_value(&second.hits)?,
        serde_json::to_value(&original.hits)?
    );
    assert!(second.focus.as_ref().unwrap().receipt().is_none());
    assert_eq!(owner.requests.lock().unwrap().len(), 1);
    assert!(kernel.optional_work_available()?);
    let held = state(context)?;
    assert_eq!(held.focus_tokens, 32);
    assert_eq!(held.token_limit - held.tokens, 64);
    Ok(())
}
#[test]
fn focus_off_and_lexical_search_preserve_exact_original_serialization() -> Result<()> {
    let (_temp, kernel, owner, original) = fixture(Mode::Off)?;
    assert_eq!(
        serde_json::to_vec(&kernel.focus_search(&query(), original.clone())?)?,
        serde_json::to_vec(&original)?
    );
    assert!(owner.requests.lock().unwrap().is_empty());
    let (_temp, kernel, owner, mut original) = fixture(Mode::Ranked)?;
    original.execution = Some(SearchExecution::new(
        RequestedMode::Lexical,
        ServedMode::Lexical,
        None,
    ));
    assert_eq!(
        serde_json::to_vec(&kernel.focus_search(&query(), original.clone())?)?,
        serde_json::to_vec(&original)?
    );
    assert!(owner.requests.lock().unwrap().is_empty());
    Ok(())
}
#[test]
fn focus_exact_ties_keep_original_order_and_unscored_tail() -> Result<()> {
    let (_temp, kernel, _owner, original) = fixture(Mode::Tied)?;
    let result = kernel.focus_search(&query(), original.clone())?;
    assert_eq!(
        serde_json::to_vec(&result.hits)?,
        serde_json::to_vec(&original.hits)?
    );
    Ok(())
}
#[test]
fn focus_unknown_native_work_retains_original_order_and_blocks_another_dispatch() -> Result<()> {
    let (_temp, kernel, owner, original) = fixture(Mode::Unknown)?;
    let result = kernel.focus_search(&query(), original.clone())?;
    let receipt = result.focus.as_ref().unwrap().receipt().unwrap();
    assert!(!receipt.applied);
    assert!(receipt.observed_input_tokens.is_none());
    assert_eq!(receipt.reserved_input_tokens, 96);
    assert_eq!(
        serde_json::to_vec(&result.hits)?,
        serde_json::to_vec(&original.hits)?
    );
    let repeated = kernel.focus_search(&query(), original.clone())?;
    assert!(matches!(
        repeated.focus,
        Some(FocusObservation::NotApplied { .. })
    ));
    assert_eq!(owner.requests.lock().unwrap().len(), 1);
    Ok(())
}
#[test]
fn malformed_optional_permutation_is_not_delivered_or_replayed() -> Result<()> {
    let (_temp, kernel, owner, original) = fixture(Mode::Foreign)?;
    let result = kernel.focus_search(&query(), original.clone())?;
    assert_eq!(
        serde_json::to_vec(&result.hits)?,
        serde_json::to_vec(&original.hits)?
    );
    assert!(!result.focus.as_ref().unwrap().receipt().unwrap().applied);
    assert!(!kernel.optional_work_available()?);
    assert_eq!(owner.requests.lock().unwrap().len(), 1);
    Ok(())
}
#[test]
fn focus_configuration_change_after_native_work_refuses_stale_delivery() -> Result<()> {
    let (_temp, kernel, owner, original) = fixture(Mode::Stale)?;
    let error = kernel.focus_search(&query(), original).unwrap_err();
    assert!(error.is::<FocusPostExecutionStale>());
    assert!(!error.to_string().contains("original source"));
    assert_eq!(owner.requests.lock().unwrap().len(), 1);
    Ok(())
}
/// Change-log entry 67: the joint (v2) and v1 delivery fences. The reader
/// republishes the index on its first read, as an Index landing during the
/// gathering would; delivery is then the typed `publication_advanced`.
fn republishing_reader<'a>(
    root: &'a std::path::Path,
    original: &'a SearchResponse,
    reads: &'a std::sync::atomic::AtomicUsize,
) -> impl Fn(&str, &Map<String, Value>, Option<&str>) -> Result<Value> + 'a {
    move |_, _, _| {
        if reads.fetch_add(1, Ordering::SeqCst) == 0 {
            let source = root.join("source");
            std::fs::write(source.join("public/later.md"), "# later\nA newer file.\n")?;
            crate::Kernel::open(&source, Some(root.join("data")))?.index()?;
        }
        Ok(serde_json::to_value(original)?)
    }
}
#[test]
fn joint_delivery_after_a_publication_landed_is_publication_advanced() -> Result<()> {
    let (temp, kernel, _owner, original) = fixture(Mode::Off)?;
    let reads = std::sync::atomic::AtomicUsize::new(0);
    let error = kernel
        .answer_with_reader_v2(
            airk_core::arbiter::context_action::ContextRequestV2 {
                schema: airk_core::arbiter::context_action::REQUEST_SCHEMA_V2.into(),
                question: "Where is the rule?".into(),
                budget: Budget::default(),
            },
            Instant::now() + Duration::from_secs(30),
            &|| Ok(()),
            &republishing_reader(temp.path(), &original, &reads),
        )
        .unwrap_err();
    assert!(reads.load(Ordering::SeqCst) >= 1);
    assert!(
        error.is::<airk_core::scoped_reads::PublicationAdvanced>(),
        "{error:#}"
    );
    Ok(())
}
#[test]
fn v1_delivery_after_a_publication_landed_is_publication_advanced() -> Result<()> {
    let (temp, kernel, _owner, original) = fixture(Mode::Off)?;
    let reads = std::sync::atomic::AtomicUsize::new(0);
    let error = kernel
        .answer_with_reader(
            ContextRequest {
                schema: REQUEST_SCHEMA.into(),
                question: "Where is the rule?".into(),
                budget: Budget::default(),
            },
            Instant::now() + Duration::from_secs(30),
            &|| Ok(()),
            &republishing_reader(temp.path(), &original, &reads),
        )
        .unwrap_err();
    assert!(reads.load(Ordering::SeqCst) >= 1);
    assert!(
        error.is::<airk_core::scoped_reads::PublicationAdvanced>(),
        "{error:#}"
    );
    Ok(())
}
/// Change-log entry 67: a publication that lands during the native ranking
/// is the typed, non-replayable `publication_advanced`, not the policy-stale
/// refusal and not a scope refusal.
#[test]
fn focus_publication_moved_after_native_work_is_publication_advanced() -> Result<()> {
    let (_temp, kernel, owner, original) = fixture(Mode::Republish)?;
    let error = kernel.focus_search(&query(), original).unwrap_err();
    let advanced = error
        .downcast_ref::<airk_core::scoped_reads::PublicationAdvanced>()
        .context("typed publication_advanced")?;
    assert_eq!(advanced.receipt()["error"]["code"], "publication_advanced");
    assert_eq!(advanced.receipt()["error"]["retryable"], false);
    assert_eq!(advanced.receipt()["error"]["query_replay_safe"], false);
    assert!(!error.is::<FocusPostExecutionStale>());
    assert_eq!(owner.requests.lock().unwrap().len(), 1, "never replayed");
    Ok(())
}
#[test]
fn focus_source_authority_refusal_is_not_hidden_by_ordinary_fallback() -> Result<()> {
    let (_temp, kernel, _owner, original) = fixture(Mode::Refused)?;
    assert!(
        kernel
            .focus_search(&query(), original)
            .unwrap_err()
            .is::<airk_core::scoped_reads::ScopeReadRefused>()
    );
    Ok(())
}
#[test]
fn focus_reserves_actual_receipt_space_before_any_native_scoring() -> Result<()> {
    let (_temp, kernel, owner, mut original) = fixture(Mode::Ranked)?;
    for hit in &mut original.hits {
        hit.text = "x".repeat(14_000);
    }
    let result = kernel.focus_search(&query(), original.clone())?;
    assert_eq!(
        serde_json::to_vec(&result.hits)?,
        serde_json::to_vec(&original.hits)?
    );
    assert!(matches!(
        result.focus,
        Some(FocusObservation::NotApplied { .. })
    ));
    assert!(owner.requests.lock().unwrap().is_empty());
    let (_temp, kernel, owner, mut original) = fixture(Mode::Ranked)?;
    for hit in &mut original.hits {
        hit.text = "x".repeat(17_000);
    }
    assert_eq!(
        serde_json::to_vec(&kernel.focus_search(&query(), original.clone())?)?,
        serde_json::to_vec(&original)?
    );
    assert!(owner.requests.lock().unwrap().is_empty());
    Ok(())
}

#[test]
fn combined_focus_outer_binding_is_admitted_before_nn_while_off_keeps_fitting_bytes() -> Result<()>
{
    use airk_core::combined_context::{ALLOWANCE_SCHEMA, PartitionAllowance, Resources};
    for (mode, combined, expected_calls) in [
        (Mode::Off, true, 0),
        (Mode::Ranked, true, 0),
        (Mode::Ranked, false, 1),
    ] {
        let (_temp, mut kernel, owner, original) = fixture(mode)?;
        let current = kernel.arbiter_context.as_ref().unwrap().as_ref();
        let application = owner.application(
            current.worker,
            current.authority,
            original.publication.as_ref().unwrap(),
        )?;
        let bytes = 2
            * (full_receipt_bound(&application.scope, original.hits.len(), 1)?
                + serde_json::to_vec(&original)?.len());
        let budget = Budget {
            calls: 3,
            rounds: 2,
            fanout: 2,
            input_tokens: 96,
            candidates: 8,
            max_bytes: bytes,
            deadline_ms: 30_000,
        };
        if combined {
            let mut context = current.clone();
            context.partition = Some(PartitionAllowance {
                schema: ALLOWANCE_SCHEMA.into(),
                attempt: context.attempt.unwrap().request_id,
                selection: digest(95),
                index_id: context.authority.index_id,
                parent_budget: budget.clone(),
                resources: Resources::budget(&budget),
                assessment_ordinals: vec![1],
            });
            kernel.arbiter_context = Some(Arc::new(context));
        }
        kernel.with_focus_parent_budget(budget, Instant::now() + Duration::from_secs(30))?;
        kernel.bind_initial_context_readiness(false)?;
        let result = kernel.focus_search(&query(), original.clone())?;
        assert_eq!(owner.requests.lock().unwrap().len(), expected_calls);
        if matches!(mode, Mode::Off) {
            assert_eq!(serde_json::to_vec(&result)?, serde_json::to_vec(&original)?);
        } else if combined {
            assert!(result.focus.as_ref().unwrap().receipt().is_none());
            assert_eq!(
                serde_json::to_vec(&result.hits)?,
                serde_json::to_vec(&original.hits)?
            );
        } else {
            assert_eq!(
                result
                    .focus
                    .as_ref()
                    .unwrap()
                    .receipt()
                    .unwrap()
                    .scored_count,
                1
            );
        }
    }
    Ok(())
}

#[test]
fn native_context_partition_keeps_snapshot_digest_separate_from_installed_selection() -> Result<()>
{
    use airk_core::{
        combined_context::PartitionAllowance,
        scoped_reads::{SelectionOwnerKind, SelectionScopeRef},
    };
    let (_temp, original, owner, _) = fixture(Mode::Off)?;
    let current = original.arbiter_context.as_ref().unwrap();
    let worker = current.worker;
    let wire = current.attempt.unwrap();
    let fixtures: Value = serde_json::from_slice(include_bytes!(
        "../../../../../fixtures/split/arbiter/focus_only_allowance_cases.json"
    ))?;
    for selected_owner in [SelectionOwnerKind::Window, SelectionOwnerKind::Agent] {
        let base = crate::Kernel::open(original.root(), Some(original.data_dir().to_path_buf()))?;
        let selection = SelectionScopeRef {
            owner: selected_owner,
            grant_nonce: Incarnation::from_bytes([80; 16]),
            service_instance: worker.service,
            scope_identity: digest(81),
            catalog_revision: CatalogRevision::INITIAL,
        };
        let sources = vec![SourceDescriptor {
            source_root_id: SourceRootId::from_identity(Incarnation::from_bytes([3; 16])),
            original_root: original.root().to_str().unwrap().into(),
        }];
        let scoped = if selected_owner == SelectionOwnerKind::Agent {
            base.with_open_reads(crate::scoped_reads::OpenReadAuthority {
                schema: 1,
                backing: worker.project,
                index_id: current.authority.index_id,
                selection,
                sources,
                source_revision: CatalogRevision::INITIAL,
                policy_epoch: 0,
                policy_digest: None,
            })?
        } else {
            let project = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
            base.with_selection_reads(crate::scoped_reads::SelectionReadAuthority {
                schema: 1,
                backing: worker.project,
                index_id: current.authority.index_id,
                project_ids: vec![project],
                selection,
                sources,
                atoms: vec![(
                    project,
                    ScopeAtom {
                        source_root_id: SourceRootId::from_identity(Incarnation::from_bytes(
                            [3; 16],
                        )),
                        kind: PathScope::Subtree,
                        path: "public".into(),
                    },
                )],
                membership_revisions: vec![(project, CatalogRevision::INITIAL)],
                source_revision: CatalogRevision::INITIAL,
                policy_epoch: 0,
                policy_digest: None,
            })?
        };
        let authority = ScopedReadRef {
            schema: 1,
            lease_generation: 1,
            index_id: current.authority.index_id,
            selection,
            fingerprint: scoped.scoped_read_fingerprint()?.unwrap(),
        };
        let native = scoped.with_native_arbiter(worker, authority, owner.clone())?;
        for row in ["context", "zero_optional_work", "ordinary_focus"] {
            let mut allowance: PartitionAllowance =
                serde_json::from_value(fixtures[row]["allowance"].clone())?;
            allowance.attempt = wire.request_id;
            allowance.index_id = authority.index_id;
            // Ordinary Focus's single selection identity is exact. Context
            // keeps its differently domained whole-member snapshot digest.
            allowance.selection = if allowance.is_ordinary_focus() {
                selection.scope_identity
            } else {
                digest(82)
            };
            let injected = native.with_context_partition(allowance.clone(), wire)?;
            assert_eq!(
                serde_json::to_value(
                    injected
                        .arbiter_context
                        .as_ref()
                        .unwrap()
                        .partition
                        .as_ref()
                )?,
                serde_json::to_value(Some(&allowance))?
            );
            assert_eq!(
                injected.scoped_read_fingerprint()?,
                Some(authority.fingerprint)
            );
            let mut different = allowance.clone();
            different.selection = digest(83);
            assert_eq!(
                native.with_context_partition(different, wire).is_err(),
                allowance.is_ordinary_focus()
            );
            let mut foreign = allowance.clone();
            foreign.index_id = IndexId::from_identity(Incarnation::from_bytes([84; 16]));
            assert!(native.with_context_partition(foreign, wire).is_err());
            let mut foreign = wire;
            foreign.request_id = Incarnation::from_bytes([85; 16]);
            assert!(
                native
                    .with_context_partition(allowance.clone(), foreign)
                    .is_err()
            );
        }
        let mut captured: PartitionAllowance =
            serde_json::from_value(fixtures["context"]["allowance"].clone())?;
        captured.attempt = wire.request_id;
        captured.index_id = authority.index_id;
        native.revoke_scoped_reads();
        assert!(
            native.with_context_partition(captured, wire).is_err(),
            "revoked installed authority was reused"
        );
    }
    // A catalog-backed Context share remains valid; a Focus-only selection
    // assertion cannot be invented when no selection credential is installed.
    let native = original.with_native_arbiter(worker, current.authority, owner.clone())?;
    for row in ["context", "zero_optional_work", "ordinary_focus"] {
        let mut allowance: PartitionAllowance =
            serde_json::from_value(fixtures[row]["allowance"].clone())?;
        allowance.attempt = wire.request_id;
        allowance.index_id = current.authority.index_id;
        assert_eq!(
            native.with_context_partition(allowance, wire).is_err(),
            row == "ordinary_focus"
        );
    }
    assert!(owner.requests.lock().unwrap().is_empty());
    Ok(())
}

fn batch(original: &SearchResponse) -> SpanBatch {
    SpanBatch {
        budget: airk_core::arbiter::answer_loop::BudgetV3::default(),
        round_left: BTreeMap::new(),
        questions: vec!["where is the rule".into(), "which tests cover it".into()],
        spans: original
            .hits
            .iter()
            .take(3)
            .enumerate()
            .map(|(index, hit)| BatchSpan {
                question: u8::from(index == 2),
                hit: hit.clone(),
                part: if index == 2 { 2 } else { 1 },
                round: 1,
            })
            .collect(),
        deadline: Instant::now() + Duration::from_secs(30),
    }
}

#[test]
fn loop_search_defers_in_search_focus() -> Result<()> {
    let (_temp, kernel, owner, original) = fixture(Mode::Ranked)?;
    kernel.bind_answer_loop(Instant::now() + Duration::from_secs(30))?;
    let result = kernel.focus_search(&query(), original.clone())?;
    assert!(
        result.focus.is_none(),
        "no in-search Focus receipt or notice"
    );
    assert_eq!(
        serde_json::to_value(&result)?,
        serde_json::to_value(&original)?
    );
    assert!(owner.requests.lock().unwrap().is_empty());
    assert!(
        kernel.bind_answer_loop(Instant::now()).is_err(),
        "bound once"
    );
    Ok(())
}

#[test]
fn one_focus_call_spans_all_ready_parts_with_per_candidate_questions() -> Result<()> {
    let (_temp, kernel, owner, original) = fixture(Mode::Ranked)?;
    kernel.bind_answer_loop(Instant::now() + Duration::from_secs(30))?;
    let ranking = kernel.rank_spans(batch(&original))?.context("Focus ran")?;
    let batches = owner.batches.lock().unwrap();
    assert_eq!(batches.len(), 1);
    let questions: Vec<u8> = batches[0]
        .candidates
        .iter()
        .map(|candidate| candidate.question)
        .collect();
    assert_eq!(questions, [0, 0, 1]);
    let charged: Vec<_> = batches[0]
        .attribution
        .iter()
        .map(|charge| (charge.part, charge.round, charge.tokens))
        .collect();
    assert_eq!(
        charged,
        [(1, 1, 64), (2, 1, 32)],
        "pairs times pair tokens per part"
    );
    assert!(ranking.receipt.applied && !ranking.unknown);
    assert!(ranking.relevance[1].unwrap() > ranking.relevance[0].unwrap());
    assert!(owner.requests.lock().unwrap().is_empty());
    assert_eq!((ranking.ordinal, batches[0].ordinal), (1, 1));
    drop(batches);
    let second = kernel
        .rank_spans(batch(&original))?
        .context("second batch")?;
    assert_eq!(second.ordinal, 2, "each batch gets the next ordinal");
    Ok(())
}

#[test]
fn a_released_ordinal_is_returned_only_when_nothing_followed_it() {
    let mut held = Attempt {
        loop_ordinal: 2,
        loop_pairs: 6,
        loop_tokens: 96,
        ..Attempt::default()
    };
    rollback(&mut held, 1, 3, 48);
    assert_eq!(
        (held.loop_ordinal, held.loop_pairs, held.loop_tokens),
        (2, 3, 48),
        "a used hole"
    );
    rollback(&mut held, 2, 3, 48);
    assert_eq!(
        (held.loop_ordinal, held.loop_pairs, held.loop_tokens),
        (1, 0, 0)
    );
}

#[test]
fn batched_focus_keeps_the_whole_answer_and_per_part_budgets() -> Result<()> {
    let (_temp, kernel, owner, original) = fixture(Mode::Ranked)?;
    kernel.bind_answer_loop(Instant::now() + Duration::from_secs(5))?;
    let mut small = batch(&original);
    small.budget.focus_pairs = 4;
    small.budget.focus_calls = 2;
    assert!(kernel.rank_spans(small)?.is_some());
    let mut over = batch(&original);
    over.budget.focus_pairs = 4;
    over.budget.focus_calls = 2;
    assert!(
        kernel.rank_spans(over)?.is_none(),
        "cumulative pairs exceed the budget"
    );
    let mut tight = batch(&original);
    tight.round_left.insert((1, 1), 40);
    let error = kernel
        .rank_spans(tight)
        .err()
        .context("over a part's allowance")?;
    assert!(error.to_string().contains("part 1 round 1"), "{error}");
    let requests = owner.batches.lock().unwrap();
    assert!(
        requests[0].remaining_ms <= 5_000,
        "clipped to the original deadline"
    );
    Ok(())
}

#[test]
fn unknown_batched_focus_outcome_keeps_its_reservation_and_blocks_more() -> Result<()> {
    let (_temp, kernel, _owner, original) = fixture(Mode::Unknown)?;
    kernel.bind_answer_loop(Instant::now() + Duration::from_secs(30))?;
    let ranking = kernel.rank_spans(batch(&original))?.context("submitted")?;
    assert!(ranking.unknown && !ranking.receipt.applied);
    assert_eq!(ranking.receipt.observed_input_tokens, None);
    assert_eq!(ranking.receipt.reserved_input_tokens, 96);
    assert!(ranking.relevance.iter().all(Option::is_none));
    assert!(
        kernel.rank_spans(batch(&original))?.is_none(),
        "no replay after unknown work"
    );
    Ok(())
}
