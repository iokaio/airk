// SPDX-License-Identifier: Apache-2.0
//! Canonical bounded split metadata controls. Reads never advance accepted work.
use airk_core::{
    catalog_management::{CatalogEntity, CatalogItem, catalog_operation_id},
    catalog_source_change::new_source_id,
    catalog_split::*,
    catalog_split_draft::*,
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{IndexId, ScopeAtom},
    service::{ControlMethod, ControlResponse, JobState, RequestKey},
    service_configuration::{ConfigurationRequest, ConfigurationWork, ProjectConfiguration},
    source_policy::SplitDestination,
};
use airk_store::{
    quota::{
        Quota,
        funding::{FundingBudget, FundingRequest},
    },
    roots::RootConfig,
    service::{
        collection::{Collection, inspect_settings_digest},
        data_scope,
        registry::ServiceRegistry,
    },
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
#[cfg(test)]
#[path = "catalog_split_control_tests.rs"]
pub(crate) mod tests;

#[derive(Debug)]
pub(crate) enum SplitAdvance {
    Waiting(SplitSnapshot),
    Advanced(SplitSnapshot),
    NeedOriginalFence(SplitSnapshot),
}

pub(crate) fn execute(
    registry: &mut ServiceRegistry,
    control: &Path,
    method: &ControlMethod,
) -> Result<Option<ControlResponse>> {
    let caller = match method {
        ControlMethod::CatalogSplitDraft { .. }
        | ControlMethod::CatalogSplitSourceState { .. }
        | ControlMethod::CatalogSplitApply { .. }
        | ControlMethod::CatalogSplitStatus { .. }
        | ControlMethod::CatalogSplitReceipt { .. }
        | ControlMethod::CatalogSplitRecover { .. }
        | ControlMethod::CatalogSplitRecoveryReceipt { .. } => crate::local_ipc::caller_identity()?,
        _ => return Ok(None),
    };
    Ok(Some(match method {
        ControlMethod::CatalogSplitSourceState {
            index_id,
            expected_catalog_revision,
        } => ControlResponse::CatalogSplitSourceState {
            state: Box::new(
                registry.catalog_split_source_state(*index_id, *expected_catalog_revision)?,
            ),
            capabilities: airk_store::service::registry::catalog::capabilities(),
        },
        ControlMethod::CatalogSplitDraft { selection } => ControlResponse::CatalogSplitDraft {
            draft: Box::new(draft_native(registry, control, selection, caller)?),
        },
        ControlMethod::CatalogSplitApply { draft } => {
            draft.validate()?;
            ensure!(draft.request.caller == caller, "foreign split caller");
            if let Some(saved) = registry.catalog_split_receipt(caller, &draft.request.key)? {
                ensure!(
                    saved.request == draft.request
                        && saved.baseline == draft.before_policy
                        && saved.plan == draft.plan,
                    "split request key conflict"
                );
                ControlResponse::CatalogSplitAccepted {
                    snapshot: Box::new(saved),
                }
            } else {
                let selection = selection_from_draft(draft)?;
                ensure!(
                    draft_native(registry, control, &selection, caller)? == **draft,
                    "split review changed; prepare a new review"
                );
                // No destination setup, quota admission, source read or job is
                // attempted before the exact Accepted intent is durable.
                match registry.catalog_reserve_split(
                    &draft.request,
                    &draft.before_policy,
                    &draft.plan,
                ) {
                    Ok(snapshot) => ControlResponse::CatalogSplitAccepted {
                        snapshot: Box::new(snapshot),
                    },
                    Err(_) => ControlResponse::CatalogOutcomeUnknown {
                        code: "catalog_split_apply_outcome_unknown".into(),
                        caller,
                        key: draft.request.key.clone(),
                        request_digest: draft.request_digest,
                        observation: registry
                            .catalog_split_receipt(caller, &draft.request.key)
                            .ok()
                            .flatten()
                            .filter(|saved| saved.request == draft.request)
                            .map(|snapshot| {
                                airk_core::service::CatalogOutcomeObservation::IndexSplit {
                                    snapshot: Box::new(snapshot),
                                }
                            }),
                    },
                }
            }
        }
        ControlMethod::CatalogSplitStatus { operation_id } => ControlResponse::CatalogSplitStatus {
            operation_id: *operation_id,
            snapshot: registry.catalog_split_status(*operation_id)?.map(Box::new),
        },
        ControlMethod::CatalogSplitReceipt {
            caller: requested,
            key,
        } => {
            ensure!(*requested == caller, "foreign split receipt caller");
            let snapshot = registry.catalog_split_receipt(caller, key)?;
            ControlResponse::CatalogSplitReceipt {
                caller,
                key: key.clone(),
                expired: snapshot.is_none() && expired(registry, key)?,
                snapshot: snapshot.map(Box::new),
            }
        }
        ControlMethod::CatalogSplitRecoveryReceipt {
            operation_id,
            caller: requested,
            key,
        } => {
            ensure!(*requested == caller, "foreign split recovery caller");
            let receipt = registry.catalog_split_recovery_receipt(caller, key)?;
            if let Some(receipt) = &receipt {
                ensure!(
                    receipt.request.operation_id == *operation_id,
                    "split recovery key belongs to another operation"
                );
            }
            let snapshot = registry.catalog_split_status(*operation_id)?;
            ControlResponse::CatalogSplitRecoveryReceipt {
                operation_id: *operation_id,
                caller,
                key: key.clone(),
                expired: receipt.is_none() && expired(registry, key)?,
                receipt: receipt.map(Box::new),
                snapshot: snapshot.map(Box::new),
            }
        }
        ControlMethod::CatalogSplitRecover { request } => {
            ensure!(request.caller == caller, "foreign split recovery caller");
            let receipt = recover(registry, control, request)?;
            let snapshot = registry
                .catalog_split_status(request.operation_id)?
                .context("split recovery operation unavailable")?;
            ControlResponse::CatalogSplitRecovered {
                receipt: Box::new(receipt),
                snapshot: Box::new(snapshot),
            }
        }
        _ => unreachable!("split method classified before work"),
    }))
}
fn expired(registry: &ServiceRegistry, key: &RequestKey) -> Result<bool> {
    let (namespace, epoch) = registry.saved_request_epoch()?;
    Ok(namespace != key.namespace || epoch != key.epoch)
}
fn unfinished_finalization(snapshot: &SplitSnapshot) -> bool {
    snapshot
        .finalization
        .as_ref()
        .is_some_and(|status| status.progress != SplitFinalizationProgress::Complete)
}

fn destination_id(operation: Incarnation) -> Result<IndexId> {
    Ok(IndexId::from_identity(
        framed_digest(b"airk/split-destination-index/1", &[operation.as_bytes()]).to_string()[..32]
            .parse()?,
    ))
}
fn draft_native(
    registry: &mut ServiceRegistry,
    control: &Path,
    selection: &SplitDraftSelection,
    caller: Incarnation,
) -> Result<SplitDraft> {
    selection.validate()?;
    crate::runtime_install::validate_control_root(control)?;
    ensure!(
        !expired(registry, &selection.key)?,
        "split draft request epoch expired"
    );
    let source = registry
        .catalog_split_source_state(selection.index_id, selection.expected_catalog_revision)?;
    let original = &source.index.backing;
    airk_kernel::service_projects::validate_retained(original)?;
    data_scope::check(
        Path::new(&original.data_root),
        control,
        selection.key.namespace,
    )?;
    let selected = Collection::existing(
        Path::new(&original.data_root),
        &original.root,
        &original.collection,
    )?
    .select()?;
    let settings = inspect_settings_digest(&selected.store, &original.root)?;
    let incarnation = selected
        .store
        .service_incarnation()?
        .context("split source lacks native incarnation")?;
    ensure!(
        RootConfig::load(selected.store.directory(), &original.root)?.roots
            == source.source_configuration.aliases,
        "split catalog and native source configuration differ"
    );
    let destination = crate::service_registration::prepare(
        Path::new(&original.root),
        Path::new(&selection.destination_data_root),
        &selection.destination_collection,
    )?;
    let operation = catalog_operation_id(caller, &selection.key)?;
    let index_id = destination_id(operation)?;
    let source_ids = source
        .policy
        .roots
        .iter()
        .map(|root| {
            Ok((
                root.id,
                new_source_id(index_id, operation, &root.canonical_path)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let target = SplitDestination {
        index_id,
        owner_binding: destination.binding,
        collection: destination.collection.clone(),
        source_ids,
    };
    let plan = source.policy.compile_split(
        source.policy.identity()?,
        operation,
        &selection.selectors,
        target.clone(),
    )?;
    let affected = source
        .affected_projects
        .iter()
        .map(|effect| effect.project_id)
        .collect::<BTreeSet<_>>();
    ensure!(
        selection
            .projects
            .iter()
            .map(|effect| effect.project_id)
            .collect::<BTreeSet<_>>()
            == affected,
        "review every affected Project explicitly; additional Project fences are unavailable"
    );
    let mut projects = Vec::new();
    for choice in &selection.projects {
        let mut effect = source
            .affected_projects
            .iter()
            .find(|effect| effect.project_id == choice.project_id)
            .unwrap()
            .clone();
        effect.retain_original = choice.retain_original;
        let original_scope = match &choice.membership {
            SplitMembershipSelection::Omit => None,
            SplitMembershipSelection::Selected { scope } => Some(scope.clone()),
            SplitMembershipSelection::PreserveReviewedScope => {
                let detail = registry.catalog_detail(
                    &CatalogEntity::Membership {
                        project_id: choice.project_id,
                        index_id: selection.index_id,
                    },
                    Some(selection.expected_catalog_revision),
                )?;
                let Some(CatalogItem::Membership(membership)) = detail.item else {
                    anyhow::bail!("reviewed Project membership unavailable")
                };
                ensure!(
                    Some(membership.revision) == effect.expected_original_membership,
                    "Project membership review changed"
                );
                Some(membership.scope)
            }
        };
        effect.destination_scope = original_scope
            .map(|scope| remap_scope(&scope, &target.source_ids))
            .transpose()?;
        projects.push(effect);
    }
    projects.sort_by_key(|effect| effect.project_id);
    let original_quota = Quota::review_service_funding_policy(Path::new(&original.data_root))?;
    let destination_quota =
        Quota::review_service_funding_policy(Path::new(&destination.data_root))?;
    let request = SplitRequest {
        schema: 1,
        required_policy_contract: SPLIT_CONTRACT,
        caller,
        key: selection.key.clone(),
        expected_catalog_revision: selection.expected_catalog_revision,
        original_index: selection.index_id,
        original_registration: original.clone(),
        expected_definition_revision: source.index.definition_revision,
        expected_source_revision: source.index.source_revision,
        original_incarnation: incarnation,
        original_settings_digest: settings,
        source_configuration: source.source_configuration,
        baseline: source.baseline,
        before_policy_digest: source.policy.identity()?,
        selectors: plan.selectors.clone(),
        destination_name: selection.destination_name.clone(),
        destination_registration: destination,
        destination: target,
        compiled_plan_digest: framed_digest(
            b"airk/catalog-split-plan/1",
            &[&serde_json::to_vec(&plan)?],
        ),
        projects,
        capacity: selection
            .capacity
            .with_policies(original_quota.digest, destination_quota.digest)?,
        require_semantic_ready: selection.require_semantic_ready,
        original_watch_digest: source.watch_digest,
        destination_watch_enabled: selection.destination_watch_enabled,
    };
    request.validate_plan(&source.policy, &plan)?;
    validate_snapshot_admission(&request, &source.policy, &plan)?;
    let metadata = airk_store::service::registry::catalog::split::native_metadata_capacity(
        &request,
        &source.policy,
        &plan,
    )?;
    ensure!(
        request.capacity.cleanup_allowance >= metadata.total,
        "split cleanup allowance cannot cover native metadata lifecycle"
    );
    let funding = FundingRequest::for_split(&request, participant_budgets(&request)?)?;
    let review = crate::catalog_index_setup::review_split(control, &request)?;
    ensure!(
        inspect_settings_digest(&selected.store, &original.root)? == settings
            && RootConfig::load(selected.store.directory(), &original.root)?.roots
                == request.source_configuration.aliases,
        "split source settings changed during preview"
    );
    let mut draft = SplitDraft {
        schema: 1,
        request_digest: request.identity()?,
        request,
        before_policy: source.policy,
        plan,
        destination_storage_action: review.storage_action(),
        control_inputs: serde_json::json!({
            "schema":1,"behavior":"protected native capture enforces permanent policy at use",
            "rules":["nested ignore ancestry","extractor configuration","tokenizer and custom model assets","preparation and import inputs"],
            "filesystem_review":review.review_digest()?.to_string(),
            "funding_budgets":funding.budgets
            ,"native_metadata_minimum":{"snapshot_upper_bound":metadata.snapshot_upper_bound,"per_participant":metadata.per_participant,"total":metadata.total}
        }),
        review_digest: Sha256Digest::from_bytes([0; 32]),
    };
    draft.review_digest = draft.identity()?;
    draft.validate()?;
    Ok(draft)
}
fn remap_scope(
    scope: &[ScopeAtom],
    ids: &BTreeMap<airk_core::index_catalog::SourceRootId, airk_core::index_catalog::SourceRootId>,
) -> Result<Vec<ScopeAtom>> {
    let mut mapped = scope
        .iter()
        .map(|atom| {
            let mut atom = atom.clone();
            atom.source_root_id = *ids
                .get(&atom.source_root_id)
                .context("Project scope references an unreviewed source")?;
            Ok(atom)
        })
        .collect::<Result<Vec<_>>>()?;
    mapped.sort();
    mapped.dedup();
    airk_core::index_catalog::atoms(&mapped)?;
    Ok(mapped)
}
fn selection_from_draft(draft: &SplitDraft) -> Result<SplitDraftSelection> {
    let request = &draft.request;
    let reverse = request
        .destination
        .source_ids
        .iter()
        .map(|(old, new)| (*new, *old))
        .collect::<BTreeMap<_, _>>();
    Ok(SplitDraftSelection {
        key: request.key.clone(),
        expected_catalog_revision: request.expected_catalog_revision,
        index_id: request.original_index,
        selectors: request.selectors.clone(),
        destination_name: request.destination_name.clone(),
        destination_data_root: request.destination_registration.data_root.clone(),
        destination_collection: request.destination_registration.collection.clone(),
        projects: request
            .projects
            .iter()
            .map(|effect| {
                Ok(SplitProjectSelection {
                    project_id: effect.project_id,
                    retain_original: effect.retain_original,
                    membership: match &effect.destination_scope {
                        None => SplitMembershipSelection::Omit,
                        Some(scope) => SplitMembershipSelection::Selected {
                            scope: remap_scope(scope, &reverse)?,
                        },
                    },
                })
            })
            .collect::<Result<Vec<_>>>()?,
        capacity: SplitCapacitySelection {
            destination_growth: request.capacity.destination_growth,
            destination_temporary: request.capacity.destination_temporary,
            original_rewrite: request.capacity.original_rewrite,
            cleanup_allowance: request.capacity.cleanup_allowance,
        },
        require_semantic_ready: request.require_semantic_ready,
        destination_watch_enabled: request.destination_watch_enabled,
    })
}

fn collection(snapshot: &SplitSnapshot, destination: bool) -> Result<Collection> {
    let registration = if destination {
        &snapshot.request.destination_registration
    } else {
        &snapshot.request.original_registration
    };
    Collection::existing(
        Path::new(&registration.data_root),
        &registration.root,
        &registration.collection,
    )
}
fn destination_work(purpose: SplitJobPurpose) -> bool {
    matches!(
        purpose,
        SplitJobPurpose::DestinationBuild | SplitJobPurpose::DestinationCleanup
    )
}
fn recovery_configuration(
    registry: &ServiceRegistry,
    snapshot: &SplitSnapshot,
    purpose: SplitJobPurpose,
    engine: Option<Sha256Digest>,
) -> Result<(Sha256Digest, ConfigurationRequest)> {
    if let Some(work) =
        registry.catalog_split_saved_work_configuration(snapshot.receipt.operation_id, purpose)?
    {
        return Ok((work.engine, work.request));
    }
    if purpose == SplitJobPurpose::DestinationCleanup
        && let Some(mut work) = registry.catalog_split_saved_work_configuration(
            snapshot.receipt.operation_id,
            SplitJobPurpose::DestinationBuild,
        )?
    {
        let ProjectConfiguration::SplitWork {
            purpose,
            require_semantic_ready,
            ..
        } = &mut work.request.configuration
        else {
            anyhow::bail!("saved destination work is not native split work")
        };
        *purpose = SplitJobPurpose::DestinationCleanup;
        *require_semantic_ready = false;
        work.request.validate()?;
        return Ok((work.engine, work.request));
    }
    let configuration = collection(snapshot, destination_work(purpose))?.split_work_configuration(
        &registry.catalog_split_state(snapshot.receipt.operation_id)?,
        purpose,
    )?;
    Ok((
        match engine {
            Some(engine) => engine,
            None => crate::runtime_pair::engine_identity()?,
        },
        configuration,
    ))
}
fn recover(
    registry: &mut ServiceRegistry,
    _control: &Path,
    request: &SplitRecoveryRequest,
) -> Result<SplitRecoveryReceipt> {
    if let Some(saved) = registry.catalog_split_recovery_receipt(request.caller, &request.key)? {
        ensure!(saved.request == *request, "split recovery key conflict");
        return Ok(saved);
    }
    let snapshot = registry
        .catalog_split_status(request.operation_id)?
        .context("split operation unavailable")?;
    request.validate_for(&snapshot.request, &snapshot.receipt)?;
    if request.action == SplitRecoveryAction::Resume && unfinished_finalization(&snapshot) {
        return registry.catalog_resume_split_finalization(request);
    }
    ensure!(
        !unfinished_finalization(&snapshot),
        "split finalization is still pending; resume the exact saved operation"
    );
    if snapshot.receipt.stage == SplitStage::Complete
        || matches!(
            snapshot.receipt.state,
            SplitRunState::CanceledBeforeFence {
                cleanup_pending: false
            }
        )
    {
        return registry.catalog_record_split_recovery(&SplitRecoveryReceipt {
            request: request.clone(),
            request_digest: framed_digest(
                b"airk/catalog-split-recovery/1",
                &[&serde_json::to_vec(request)?],
            ),
            outcome: SplitRecoveryOutcome::AlreadyComplete {
                receipt_digest: snapshot.receipt_digest,
            },
        });
    }
    if request.action == SplitRecoveryAction::Cancel
        && snapshot.receipt.stage >= SplitStage::OriginalFenced
    {
        return registry.catalog_record_split_recovery(&SplitRecoveryReceipt {
            request: request.clone(),
            request_digest: framed_digest(
                b"airk/catalog-split-recovery/1",
                &[&serde_json::to_vec(request)?],
            ),
            outcome: SplitRecoveryOutcome::RestrictiveCompletionRequired {
                receipt_digest: snapshot.receipt_digest,
            },
        });
    }
    if snapshot.finalization.is_some() {
        return registry.catalog_resume_split_finalization(request);
    }
    if request.action == SplitRecoveryAction::Cancel {
        return registry.catalog_cancel_split_action(request);
    }
    // Successful native work may outlive a lost metadata acknowledgment. Route
    // that exact saved result before any new worker/settings preparation.
    if registry.catalog_split_resume_requires_continuation(request)? {
        return registry.catalog_resume_split_continuation(request);
    }
    if snapshot.receipt.stage == SplitStage::OriginalFenced {
        return registry.catalog_resume_split_policy_action(request);
    }
    if snapshot.receipt.stage == SplitStage::DestinationValidated {
        let (engine, configuration) =
            recovery_configuration(registry, &snapshot, SplitJobPurpose::OriginalWitness, None)?;
        return registry.catalog_resume_split_job_action(request, engine, &configuration);
    }
    let failed = snapshot
        .receipt
        .jobs
        .last()
        .map(|job| {
            registry
                .job(job.job)
                .map(|record| (job.purpose, record.state))
        })
        .transpose()?;
    if let Some((purpose, JobState::Failed | JobState::Canceled)) = failed {
        let (engine, configuration) = recovery_configuration(registry, &snapshot, purpose, None)?;
        return registry.catalog_resume_split_job_action(request, engine, &configuration);
    }
    if snapshot.receipt.supports_metadata_continuation() {
        return registry.catalog_resume_split_continuation(request);
    }
    anyhow::bail!(
        "split waits for its exact accepted child work; inspect its saved job before recovery"
    )
}

fn funding_request(snapshot: &SplitSnapshot) -> Result<FundingRequest> {
    FundingRequest::for_split(&snapshot.request, participant_budgets(&snapshot.request)?)
}
fn participant_budgets(request: &SplitRequest) -> Result<Vec<FundingBudget>> {
    let capacity = &request.capacity;
    let original_cleanup = capacity.cleanup_allowance / 2;
    let destination_cleanup = capacity.cleanup_allowance - original_cleanup;
    let original = capacity
        .original_rewrite
        .checked_add(original_cleanup)
        .context("original funding overflow")?;
    let destination = capacity
        .destination_growth
        .checked_add(capacity.destination_temporary)
        .and_then(|bytes| bytes.checked_add(destination_cleanup))
        .context("destination funding overflow")?;
    Ok(vec![
        FundingBudget {
            index_id: request.original_index,
            bytes: original,
        },
        FundingBudget {
            index_id: request.destination.index_id,
            bytes: destination,
        },
    ])
}
fn job_key(
    registry: &ServiceRegistry,
    snapshot: &SplitSnapshot,
    purpose: SplitJobPurpose,
) -> Result<RequestKey> {
    let nonce = framed_digest(
        b"airk/split-internal-job-key/1",
        &[
            snapshot.receipt.operation_id.as_bytes(),
            &serde_json::to_vec(&purpose)?,
        ],
    )
    .to_string()[..32]
        .parse()?;
    let (namespace, epoch) = registry.saved_request_epoch()?;
    ensure!(
        namespace == snapshot.request.key.namespace,
        "split registry namespace changed"
    );
    Ok(RequestKey {
        namespace,
        epoch,
        nonce,
    })
}
fn enqueue(
    registry: &mut ServiceRegistry,
    snapshot: &SplitSnapshot,
    purpose: SplitJobPurpose,
    engine: Sha256Digest,
) -> Result<SplitSnapshot> {
    let (saved_engine, configuration) =
        recovery_configuration(registry, snapshot, purpose, Some(engine))?;
    let engine = if purpose == SplitJobPurpose::DestinationCleanup {
        saved_engine
    } else {
        engine
    };
    registry.enqueue_split_job(
        snapshot.receipt.operation_id,
        snapshot.receipt_digest,
        snapshot.request.caller,
        &job_key(registry, snapshot, purpose)?,
        purpose,
        &ConfigurationWork {
            engine,
            request: configuration,
        },
    )
}

/// One durable metadata step per wake. Waiting/read observations do not launch
/// jobs; proof-bearing native owner actions are returned to the owning actor.
pub(crate) fn advance_pending(
    registry: &mut ServiceRegistry,
    control: &Path,
    engine: Sha256Digest,
    operation: Incarnation,
) -> Result<SplitAdvance> {
    let snapshot = registry
        .catalog_split_status(operation)?
        .context("split operation unavailable")?;
    if snapshot.receipt.cleanup_attention.is_some()
        || snapshot
            .finalization
            .as_ref()
            .is_some_and(|finalization| finalization.attention.is_some())
        || matches!(snapshot.receipt.state, SplitRunState::NeedsAttention { .. })
        || (!unfinished_finalization(&snapshot)
            && matches!(
                snapshot.receipt.state,
                SplitRunState::Complete
                    | SplitRunState::CanceledBeforeFence {
                        cleanup_pending: false
                    }
            ))
    {
        return Ok(SplitAdvance::Waiting(snapshot));
    }
    let advanced = match advance_one(registry, control, engine, &snapshot) {
        Ok(value) => return Ok(value),
        Err(error) => {
            if error.is::<airk_store::current::CurrentGateContention>()
                || error.is::<airk_store::quota::QuotaContention>()
                || error.is::<airk_core::catalog_source_change::SourceChangeBusy>()
                || error.chain().any(|cause| {
                    matches!(
                        cause.downcast_ref::<std::fs::TryLockError>(),
                        Some(std::fs::TryLockError::WouldBlock)
                    )
                })
            {
                return Ok(SplitAdvance::Waiting(
                    registry
                        .catalog_split_status(operation)?
                        .context("split intent disappeared during contention")?,
                ));
            }
            let current = registry
                .catalog_split_status(operation)?
                .context("split intent disappeared after failure")?;
            let code = if error.is::<airk_store::quota::QuotaPressure>() {
                "storage_quota_exhausted".into()
            } else {
                airk_store::diagnostics::failure_code(&error)
                    .unwrap_or_else(|| "split_metadata_needs_attention".into())
            };
            registry.catalog_note_split_failure(operation, current.receipt_digest, &code)?
        }
    };
    Ok(SplitAdvance::Advanced(advanced))
}
fn advance_one(
    registry: &mut ServiceRegistry,
    control: &Path,
    engine: Sha256Digest,
    snapshot: &SplitSnapshot,
) -> Result<SplitAdvance> {
    let operation = snapshot.receipt.operation_id;
    if unfinished_finalization(snapshot) {
        return advance_release(registry, snapshot);
    }
    if matches!(
        snapshot.receipt.state,
        SplitRunState::CanceledBeforeFence {
            cleanup_pending: true
        }
    ) {
        return advance_canceled(registry, control, engine, snapshot);
    }
    Ok(match snapshot.receipt.stage {
        SplitStage::Accepted => {
            let detail = registry.catalog_detail(
                &CatalogEntity::Index {
                    id: snapshot.request.destination.index_id,
                },
                None,
            )?;
            if detail.item.is_none() {
                SplitAdvance::Advanced(
                    registry
                        .catalog_reserve_split_destination(operation, snapshot.receipt_digest)?,
                )
            } else if snapshot.receipt.destination_storage.is_none() {
                let state = registry.catalog_split_state(operation)?;
                SplitAdvance::Advanced(
                    crate::catalog_index_setup::prepare_split(registry, control, &state)?
                        .record(registry)?,
                )
            } else {
                let roots = [
                    snapshot.request.original_registration.data_root.clone(),
                    snapshot.request.destination_registration.data_root.clone(),
                ]
                .into_iter()
                .collect::<BTreeSet<_>>();
                let root = roots
                    .iter()
                    .find(|root| !snapshot.receipt.funding_receipts.contains_key(*root))
                    .context("Accepted split has no remaining funding action")?;
                let quota = Quota::open_service(Path::new(root))?;
                let funding = quota.reserve_funding(&funding_request(snapshot)?)?;
                SplitAdvance::Advanced(registry.catalog_record_split_funding(
                    operation,
                    snapshot.receipt_digest,
                    &funding,
                )?)
            }
        }
        SplitStage::DestinationPrepared => SplitAdvance::Advanced(enqueue(
            registry,
            snapshot,
            SplitJobPurpose::DestinationBuild,
            engine,
        )?),
        SplitStage::Building => {
            complete_or_wait(registry, snapshot, SplitJobPurpose::DestinationBuild)?
        }
        SplitStage::DestinationValidated => {
            if let Some(job) = snapshot
                .receipt
                .jobs
                .iter()
                .rev()
                .find(|job| job.purpose == SplitJobPurpose::OriginalWitness)
            {
                let state = registry.job(job.job)?.state;
                let result = registry
                    .configuration_result(job.job)?
                    .map(serde_json::from_value::<SplitJobResult>)
                    .transpose()?;
                if state == JobState::Succeeded
                    && result.as_ref().is_some_and(|result| {
                        result
                            .remainder_witness
                            .as_ref()
                            .is_some_and(|witness| witness.job == Some(job.job))
                            && result.remainder_witness == snapshot.receipt.remainder_witness
                    })
                {
                    SplitAdvance::NeedOriginalFence(snapshot.clone())
                } else {
                    complete_or_wait(registry, snapshot, SplitJobPurpose::OriginalWitness)?
                }
            } else {
                SplitAdvance::Advanced(enqueue(
                    registry,
                    snapshot,
                    SplitJobPurpose::OriginalWitness,
                    engine,
                )?)
            }
        }
        SplitStage::OriginalFenced => {
            complete_or_wait(registry, snapshot, SplitJobPurpose::OriginalRemoval)?
        }
        SplitStage::RemainderPublished => SplitAdvance::Advanced(
            registry.catalog_apply_split_memberships(operation, snapshot.receipt_digest)?,
        ),
        SplitStage::MembershipApplied => advance_membership_cleanup(registry, snapshot, engine)?,
        SplitStage::CleanupPending => advance_release(registry, snapshot)?,
        SplitStage::Complete => SplitAdvance::Waiting(snapshot.clone()),
    })
}
fn complete_or_wait(
    registry: &mut ServiceRegistry,
    snapshot: &SplitSnapshot,
    purpose: SplitJobPurpose,
) -> Result<SplitAdvance> {
    let job = snapshot
        .receipt
        .jobs
        .iter()
        .rev()
        .find(|job| job.purpose == purpose)
        .context("split stage has no exact child job")?;
    Ok(match registry.job(job.job)?.state {
        JobState::Succeeded => SplitAdvance::Advanced(registry.catalog_complete_split_job(
            snapshot.receipt.operation_id,
            snapshot.receipt_digest,
            job.job,
        )?),
        JobState::Failed | JobState::Canceled => {
            if matches!(
                snapshot.receipt.state,
                SplitRunState::CanceledBeforeFence { .. }
            ) {
                SplitAdvance::Waiting(snapshot.clone())
            } else {
                SplitAdvance::Advanced(registry.catalog_note_split_failure(
                    snapshot.receipt.operation_id,
                    snapshot.receipt_digest,
                    "split_child_needs_attention",
                )?)
            }
        }
        JobState::Pending | JobState::Running => SplitAdvance::Waiting(snapshot.clone()),
    })
}

fn advance_membership_cleanup(
    registry: &mut ServiceRegistry,
    snapshot: &SplitSnapshot,
    engine: Sha256Digest,
) -> Result<SplitAdvance> {
    for destination in [false, true] {
        let collection = collection(snapshot, destination)?;
        let registration = if destination {
            &snapshot.request.destination_registration
        } else {
            &snapshot.request.original_registration
        };
        let index = if destination {
            snapshot.request.destination.index_id
        } else {
            snapshot.request.original_index
        };
        let quota = Quota::open_service(Path::new(&registration.data_root))?;
        let funding = quota.observe_funding(snapshot.receipt.operation_id)?;
        let participant = funding
            .participants
            .iter()
            .find(|participant| participant.index_id == index)
            .context("split membership funding participant missing")?;
        let settled = participant
            .phase
            .as_ref()
            .is_some_and(|phase| participant.settled_revision == Some(phase.revision));
        if collection.required_split()?.is_none_or(|reference| {
            reference.activation_receipt != snapshot.receipt.membership_receipt
        }) || !settled
        {
            collection.retain_split_state_and_reconcile(
                &registry.catalog_split_state(snapshot.receipt.operation_id)?,
                &funding,
            )?;
            return Ok(SplitAdvance::Advanced(snapshot.clone()));
        }
    }
    let policy = registry
        .catalog_policy_status(snapshot.request.original_index)?
        .context("split remainder policy unavailable")?;
    let report = policy
        .application
        .context("split remainder application unavailable")?;
    if !report.physical_cleanup_pending {
        return Ok(SplitAdvance::Advanced(
            registry.catalog_authorize_split_cleanup(
                snapshot.receipt.operation_id,
                snapshot.receipt_digest,
            )?,
        ));
    }
    if snapshot
        .receipt
        .jobs
        .iter()
        .any(|job| job.purpose == SplitJobPurpose::OriginalCleanup)
    {
        return complete_or_wait(registry, snapshot, SplitJobPurpose::OriginalCleanup);
    }
    Ok(SplitAdvance::Advanced(enqueue(
        registry,
        snapshot,
        SplitJobPurpose::OriginalCleanup,
        engine,
    )?))
}

fn advance_canceled(
    registry: &mut ServiceRegistry,
    _control: &Path,
    engine: Sha256Digest,
    snapshot: &SplitSnapshot,
) -> Result<SplitAdvance> {
    if snapshot.receipt.physical_cleanup_authorization.is_some() {
        return advance_release(registry, snapshot);
    }
    // A still-running builder retains its worker/lease; cancellation is an
    // explicit request, not proof that its captures or journal have stopped.
    for child in snapshot
        .receipt
        .jobs
        .iter()
        .filter(|job| job.purpose == SplitJobPurpose::DestinationBuild)
    {
        if matches!(
            registry.job(child.job)?.state,
            JobState::Pending | JobState::Running
        ) {
            return Ok(SplitAdvance::Waiting(snapshot.clone()));
        }
    }
    if snapshot.receipt.destination_storage.is_none() && snapshot.receipt.jobs.is_empty() {
        let directory = Path::new(&snapshot.request.destination_registration.data_root)
            .join("collections-1.4")
            .join(&snapshot.request.destination_registration.collection);
        match std::fs::symlink_metadata(&directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let state = registry.catalog_split_state(snapshot.receipt.operation_id)?;
                let proof=airk_store::service::registry::catalog::split::NativeSplitAbsentDestination::capture(&state)?;
                let lease = airk_store::service::ProjectLease::acquire(
                    _control,
                    snapshot.request.destination_registration.binding,
                )?;
                return Ok(SplitAdvance::Advanced(
                    registry.catalog_authorize_split_absent_cleanup(&lease, proof)?,
                ));
            }
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
    }
    if snapshot
        .receipt
        .jobs
        .iter()
        .any(|job| job.purpose == SplitJobPurpose::DestinationCleanup)
    {
        return complete_or_wait(registry, snapshot, SplitJobPurpose::DestinationCleanup);
    }
    Ok(SplitAdvance::Advanced(enqueue(
        registry,
        snapshot,
        SplitJobPurpose::DestinationCleanup,
        engine,
    )?))
}
fn advance_release(
    registry: &mut ServiceRegistry,
    snapshot: &SplitSnapshot,
) -> Result<SplitAdvance> {
    Ok(SplitAdvance::Advanced(
        registry.catalog_advance_split_finalization(
            snapshot.receipt.operation_id,
            snapshot.receipt_digest,
        )?,
    ))
}

/// Bridge an ACTUAL claimed native job to its exact paid store. Metadata is
/// published under the still-held prior phase before the new permit is used;
/// pending journal recovery follows takeover and never precedes its funding.
pub(crate) fn prepare_claimed_funding(
    registry: &ServiceRegistry,
    work: &airk_store::service::registry::catalog::split::ClaimedSplitWork,
) -> Result<Option<airk_store::quota::funding::FundingUse>> {
    use airk_store::quota::funding::FundingPhase;
    if work.job.purpose == SplitJobPurpose::OriginalWitness {
        return Ok(None);
    }
    let snapshot = &work.snapshot;
    let state = registry.catalog_split_state(snapshot.receipt.operation_id)?;
    ensure!(
        state.snapshot() == snapshot,
        "split claimed metadata phase changed before funding handoff"
    );
    let destination = destination_work(work.job.purpose);
    let registration = if destination {
        &snapshot.request.destination_registration
    } else {
        &snapshot.request.original_registration
    };
    let index = if destination {
        snapshot.request.destination.index_id
    } else {
        snapshot.request.original_index
    };
    let collection = collection(snapshot, destination)?;
    let quota = Quota::open_service(Path::new(&registration.data_root))?;
    if !snapshot
        .receipt
        .funding_receipts
        .contains_key(&registration.data_root)
    {
        ensure!(
            work.job.purpose == SplitJobPurpose::DestinationCleanup,
            "split mutation cannot run before its root funding admission"
        );
        // Cancellation may own a metadata-only shell from preparation before
        // any root funding succeeded. Its finite native cleanup metadata uses
        // ordinary setup/storage admission; no capture or evidence is allowed.
        collection.retain_split_state(&state, &quota)?;
        collection
            .select_for_diagnostics()?
            .with_split_access(work.permit.clone())?;
        return Ok(None);
    }
    let mut funding = quota.observe_funding(snapshot.receipt.operation_id)?;
    funding.validate_for(&snapshot.request)?;
    ensure!(
        snapshot
            .receipt
            .funding_receipts
            .get(&registration.data_root)
            == Some(&funding.identity()?),
        "split worker funding was not admitted"
    );
    let phase = FundingPhase {
        revision: snapshot.receipt.revision.get(),
        purpose: work.job.purpose,
        job: work.job.job,
    };
    let prior = funding
        .participants
        .iter()
        .find(|participant| participant.index_id == index)
        .context("split worker funding participant missing")?
        .phase
        .clone();
    let owner = match prior {
        None => {
            let owner = quota.begin_funding_phase(&funding, index, phase.clone())?;
            collection.retain_split_state(&state, &quota.clone().with_funding(owner.clone())?)?;
            owner
        }
        Some(previous) if previous == phase => {
            let owner = quota.adopt_funding_phase(&funding, index, &phase)?;
            collection.retain_split_state(&state, &quota.clone().with_funding(owner.clone())?)?;
            owner
        }
        Some(previous)
            if previous.purpose != phase.purpose
                && phase.purpose != SplitJobPurpose::DestinationCleanup =>
        {
            funding = collection.retain_split_state_and_reconcile(&state, &funding)?;
            let owner = quota.begin_funding_phase(&funding, index, phase.clone())?;
            collection.retain_split_state(&state, &quota.clone().with_funding(owner.clone())?)?;
            owner
        }
        Some(previous) => {
            let prior = quota.adopt_funding_phase(&funding, index, &previous)?;
            collection.retain_split_state(&state, &quota.clone().with_funding(prior)?)?;
            funding = quota.observe_funding(snapshot.receipt.operation_id)?;
            let store = collection
                .select_for_diagnostics()?
                .with_split_access(work.permit.clone())?;
            let writer = store.lock()?;
            let (store, _) = if phase.purpose == SplitJobPurpose::DestinationCleanup {
                store.takeover_operation_funding_for_cleanup(
                    &writer,
                    &quota,
                    &funding,
                    phase.clone(),
                )?
            } else {
                store.takeover_operation_funding_for_retry(
                    &writer,
                    &quota,
                    &funding,
                    phase.clone(),
                )?
            };
            let funded = Quota::for_store(&store)?;
            funded
                .funding_use()
                .cloned()
                .context("split takeover lost its exact paid use")?
        }
    };
    // Actual stable capsule + claimed permit check before returning a capability
    // to Kernel. No evidence/model/source pass is performed by this bridge.
    let store = collection
        .select_for_diagnostics()?
        .with_split_access(work.permit.clone())?
        .with_operation_funding(owner.clone())?;
    let writer = store.lock()?;
    // Exact takeover (or the exact same claimed phase) precedes this custody
    // step. It closes dead OS handles only; existing recovery/Ready/disposal
    // boundaries still own staging cleanup and all capacity release.
    store.close_abandoned_operation_reservations(&writer, &Quota::for_store(&store)?)?;
    Ok(Some(owner))
}
