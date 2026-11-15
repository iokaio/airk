// SPDX-License-Identifier: Apache-2.0
//! Canonical Focus owner tests with an injected scorer; no model-quality claim.
use super::*;
use airk_core::{
    Citation, DocumentLocation,
    arbiter::{
        FocusApplication, FocusApplicationReceipt, FocusIdentity, FocusPresentation,
        FocusProfileLimits, FocusScore,
    },
    grant_provenance::GrantProvenance,
    identity::Incarnation,
    index_catalog::*,
    read_attempt::{ReadAttemptBudget, ReadClock},
    scoped_reads::SourceDescriptor,
    search_execution::{RequestedMode, SearchExecution},
    service::{ScopedReadRef, WorkerIdentity},
};
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
    Refused,
    Foreign,
}
struct Controller {
    mode: Mode,
    revision: AtomicU64,
    requests: Mutex<Vec<NativeFocusRequest>>,
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
    let authority = crate::scoped_reads::ScopedReadAuthority {
        schema: 1,
        backing: airk_core::service::ProjectRegistration::new(
            kernel.root().to_str().unwrap().into(),
            kernel.data_dir().to_str().unwrap().into(),
            kernel.collection().into(),
        )?
        .binding,
        index_id: index,
        project_ids: vec![project],
        provenance: GrantProvenance::UserReviewedProjectGrant,
        grant: ReviewedGrant {
            id: GrantId::from_identity(Incarnation::from_bytes([4; 16])),
            host: digest(5),
            revision: CatalogRevision::INITIAL,
            revoked: false,
            memberships: vec![GrantedMembership {
                membership: Membership {
                    project_id: project,
                    index_id: index,
                    revision: CatalogRevision::INITIAL,
                    scope: vec![ScopeAtom {
                        source_root_id: source_id,
                        kind: PathScope::Subtree,
                        path: "public".into(),
                    }],
                },
                source_revision: CatalogRevision::INITIAL,
                policy_epoch: 0,
                policy_digest: None,
            }],
        },
        sources: vec![SourceDescriptor {
            source_root_id: source_id,
            original_root: kernel.root().to_str().unwrap().into(),
        }],
    };
    let scoped = kernel.with_scoped_reads(authority.clone())?;
    let reference = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: index,
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
    let controller = Arc::new(Controller {
        mode,
        revision: AtomicU64::new(1),
        requests: Mutex::new(vec![]),
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
