// SPDX-License-Identifier: Apache-2.0
//! Bounded metadata actor keeps registry I/O off independent native health handlers.
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse, JobError, ProjectBinding},
};
use airk_store::service::registry::ServiceRegistry;
use anyhow::Result;
use std::{
    path::PathBuf,
    sync::mpsc::{SyncSender, sync_channel},
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::oneshot;

enum ManagementReply {
    Control(oneshot::Sender<ControlResponse>),
    Scoped(oneshot::Sender<Result<crate::project_supervisor::ScopedConnectionDelivery>>),
}
impl ManagementReply {
    fn send(self, response: ControlResponse) -> std::result::Result<(), Box<ControlResponse>> {
        match self {
            Self::Control(sender) => sender.send(response).map_err(Box::new),
            Self::Scoped(sender) => {
                let _ = sender.send(Err(anyhow::anyhow!(
                    "scoped connection requires direct delivery"
                )));
                Err(Box::new(response))
            }
        }
    }
}
enum Envelope {
    Request(Box<ControlMethod>, ManagementReply),
    Missing(crate::source_supervisor::NativeMissingSourceEvent),
    AdministrativeRead(
        ProjectBinding,
        oneshot::Sender<Result<crate::project_supervisor::NativeAdministrativeRead>>,
    ),
}

struct MissingReporter {
    sender: Weak<SyncSender<Envelope>>,
    pending: Arc<AtomicUsize>,
    missing: Arc<AtomicUsize>,
}
impl crate::source_supervisor::MissingSourceReporter for MissingReporter {
    fn try_report(
        &self,
        event: crate::source_supervisor::NativeMissingSourceEvent,
    ) -> std::result::Result<(), crate::source_supervisor::NativeMissingSourceEvent> {
        let Some(sender) = self.sender.upgrade() else {
            return Err(event);
        };
        if self
            .missing
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 32).then_some(n + 1)
            })
            .is_err()
        {
            return Err(event);
        }
        self.pending.fetch_add(1, Ordering::AcqRel);
        match sender.try_send(Envelope::Missing(event)) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.pending.fetch_sub(1, Ordering::AcqRel);
                self.missing.fetch_sub(1, Ordering::AcqRel);
                let envelope = match error {
                    std::sync::mpsc::TrySendError::Full(envelope)
                    | std::sync::mpsc::TrySendError::Disconnected(envelope) => envelope,
                };
                let Envelope::Missing(event) = envelope else {
                    unreachable!("missing-only reporter");
                };
                Err(event)
            }
        }
    }
}
struct MissingWork {
    event: crate::source_supervisor::NativeMissingSourceEvent,
    nonce: Option<Incarnation>,
    draft: Option<airk_core::catalog_source_draft::SourceMutationDraft>,
    next_attempt: std::time::Instant,
}
impl MissingWork {
    fn advance(
        &mut self,
        registry: &mut ServiceRegistry,
        root: &std::path::Path,
        native: &mut CatalogRuntime,
    ) -> Result<()> {
        if self.draft.is_none() {
            native.reconcile(registry)?;
            if !registry.is_registered(self.event.binding())? {
                return Err(crate::catalog_source_change_control::NativeMissingSourceObsolete::RegistrationRetired.into());
            }
            let current = registry.watch(self.event.binding())?;
            anyhow::ensure!(
                current.project.fencing_generation == self.event.registration_epoch()
                    && current.configuration_epoch == self.event.configuration_epoch()
                    && current.observation.enabled,
                crate::catalog_source_change_control::NativeMissingSourceObsolete::EpochChanged
            );
            let state = registry
                .catalog_state()?
                .ok_or_else(|| anyhow::anyhow!("native missing-source catalog unavailable"))?;
            // Waiting behind an existing fence must not manufacture a second
            // draft that will become stale as that accepted operation commits.
            native.gate.begin_issue(state.namespace, state.revision)?;
            let index = registry
                .catalog_index_for_binding(self.event.binding())?
                .ok_or_else(|| anyhow::anyhow!("native missing-source index unavailable"))?;
            let (namespace, epoch) = registry.saved_request_epoch()?;
            let nonce = match self.nonce {
                Some(nonce) => nonce,
                None => {
                    let nonce = airk_kernel::service_broker::new_instance_id()?;
                    self.nonce = Some(nonce);
                    nonce
                }
            };
            let selection = airk_core::catalog_source_draft::SourceMutationDraftSelection {
                key: airk_core::service::RequestKey {
                    namespace,
                    epoch,
                    nonce,
                },
                expected_catalog_revision: state.revision,
                index_id: index.id,
                change: airk_core::catalog_source_draft::SourceMutationSelection::RetireIndex,
            };
            // Until this succeeds, there is no canonical request and no Store
            // mutation/fence effect. Snapshot refresh retains the event nonce.
            self.draft = Some(
                crate::catalog_source_change_control::draft_missing_from_event(
                    registry,
                    root,
                    &selection,
                    &self.event,
                )?,
            );
        }
        let draft = self
            .draft
            .as_ref()
            .expect("retained canonical native draft");
        if matches!(
            registry.catalog_fence_observation(draft.request.caller, &draft.request.key)?,
            Some(airk_core::catalog_fences::FenceObservation::Pending(_))
        ) {
            anyhow::bail!("native accepted fence continuation pending");
        }
        native
            .coordinator
            .apply_fenced(registry, &draft.request, draft.preview_digest)?;
        native.reconcile(registry)?;
        Ok(())
    }
}

pub(crate) struct CatalogBinding {
    pub gate: crate::catalog_admission::NativeCatalogAdmission,
    pub control: crate::project_supervisor::CatalogControlHandle,
}
struct CatalogRuntime {
    gate: crate::catalog_admission::NativeCatalogAdmission,
    control: crate::project_supervisor::CatalogControlHandle,
    coordinator: crate::catalog_coordinator::NativeCatalogCoordinator,
    policy_completions_pending: bool,
}

/// A saved terminal receipt describes an immutable completed operation, not
/// current live admission. Keep its complete validated proof when unrelated
/// reconciliation fails; every ordinary read/mutation remains fenced below.
fn finalize_catalog_response(
    response: ControlResponse,
    reconcile: impl FnOnce() -> Result<()>,
) -> ControlResponse {
    if matches!(
        response,
        ControlResponse::Failure { .. }
            | ControlResponse::CatalogOutcomeUnknown { .. }
            | ControlResponse::SourceChangeBusy { .. }
            | ControlResponse::SourceChangeResolutionOutcomeUnknown { .. }
    ) {
        return response;
    }
    let saved_terminal = matches!(
        &response,
        ControlResponse::CatalogReceipt {
            receipt: Some(metadata),
            fence: Some(airk_core::catalog_fences::FenceObservation::Applied(saved)),
        } if metadata == &saved.metadata && saved.validate().is_ok()
    );
    if reconcile().is_err() && !saved_terminal {
        return ControlResponse::Failure {
            code: "catalog_metadata_saved_outcome_unknown".into(),
        };
    }
    response
}

#[cfg(test)]
#[path = "service_management_receipt_tests.rs"]
mod receipt_tests;

impl CatalogRuntime {
    fn advance_split(
        &mut self,
        registry: &mut ServiceRegistry,
        root: &std::path::Path,
        engine: airk_core::identity::Sha256Digest,
        operation: Incarnation,
    ) -> Result<bool> {
        match crate::catalog_split_control::advance_pending(registry, root, engine, operation)? {
            crate::catalog_split_control::SplitAdvance::Waiting(snapshot) => {
                snapshot.validate()?;
                Ok(false)
            }
            crate::catalog_split_control::SplitAdvance::Advanced(snapshot) => {
                snapshot.validate()?;
                Ok(true)
            }
            crate::catalog_split_control::SplitAdvance::NeedOriginalFence(snapshot) => {
                self.reconcile(registry)?;
                // Resume an accepted native fence before constructing another
                // request. The coordinator retains the original exact key.
                if self.gate.pending_fence_key()?.is_some() {
                    return Ok(false);
                }
                let (request, preview) =
                    crate::catalog_source_change_control::split_cutover(registry, root, &snapshot)?;
                self.coordinator.apply_fenced(registry, &request, preview)?;
                self.reconcile(registry)?;
                Ok(true)
            }
        }
    }
    fn initialize(
        mut binding: CatalogBinding,
        registry: &mut ServiceRegistry,
        source: crate::source_supervisor::SourceWake,
    ) -> Result<Self> {
        binding.control = binding.control.with_source_wake(source);
        let state = registry
            .catalog_state()?
            .ok_or_else(|| anyhow::anyhow!("native catalog unavailable"))?;
        anyhow::ensure!(
            state.activated
                && state.authority_contract == 1
                && state.policy_contract
                    <= airk_store::service::registry::catalog::SUPPORTED_POLICY_CONTRACT,
            "native catalog authority unavailable"
        );
        binding.gate.begin_issue(state.namespace, state.revision)?;
        let mut coordinator = crate::catalog_coordinator::CatalogCoordinator::new(
            binding.gate.clone(),
            binding.control.clone(),
        )?;
        coordinator.restore_pending(registry)?;
        let policy_completions_pending =
            registry.catalog_reconcile_policy_completions(16)?.len() == 16;
        Ok(Self {
            gate: binding.gate,
            control: binding.control,
            coordinator,
            policy_completions_pending,
        })
    }
    /// Native accepted-result bookkeeping belongs to the existing metadata
    /// owner, never a status read or a replayed frontend mutation. A failed turn
    /// retains ownership until exact saved results can be reconciled.
    fn reconcile_policy_completions(&mut self, registry: &mut ServiceRegistry) -> Result<bool> {
        self.policy_completions_pending = true;
        let completed = registry.catalog_reconcile_policy_completions(16)?;
        self.policy_completions_pending = completed.len() == 16;
        Ok(!completed.is_empty())
    }
    fn connect(
        &self,
        registry: &mut ServiceRegistry,
        request: &airk_core::service::ScopedConnectRequest,
    ) -> Result<crate::project_supervisor::ScopedConnectionDelivery> {
        self.reconcile(registry)?;
        anyhow::ensure!(
            self.gate
                .catalog_clock()
                .is_some_and(|clock| clock.namespace == request.namespace
                    && clock.revision == request.expected_catalog_revision),
            crate::catalog_coordinator::CatalogRevisionChanged
        );
        let selection = crate::catalog_coordinator::native_scoped_selection(registry, request)?;
        self.control
            .issue_scoped(selection, Duration::from_secs(300))
    }
    fn connect_legacy(
        &self,
        registry: &mut ServiceRegistry,
        binding: airk_core::service::ProjectBinding,
    ) -> Result<crate::project_supervisor::ScopedConnectionDelivery> {
        self.reconcile(registry)?;
        let state = registry
            .catalog_state()?
            .ok_or_else(|| anyhow::anyhow!("native catalog unavailable"))?;
        let key = airk_core::identity::framed_digest(
            b"airk/native-legacy-connection-context/1",
            &[binding.as_bytes()],
        );
        let context = crate::catalog_host_context::NativeHostContext::load(
            registry,
            key,
            Some(state.revision),
        )?;
        let descriptor = context.descriptor();
        anyhow::ensure!(
            descriptor.namespace == state.namespace
                && descriptor.catalog_revision == state.revision,
            "native compatibility context changed"
        );
        self.control.issue_legacy(
            crate::project_supervisor::NativeLegacyAuthoritySelection {
                namespace: descriptor.namespace,
                host: descriptor.host,
                catalog_revision: descriptor.catalog_revision,
                binding,
            },
            Duration::from_secs(300),
        )
    }
    fn reconcile(&self, registry: &ServiceRegistry) -> Result<()> {
        let state = registry
            .catalog_state()?
            .ok_or_else(|| anyhow::anyhow!("native catalog unavailable"))?;
        anyhow::ensure!(
            state.activated
                && state.authority_contract == 1
                && state.policy_contract
                    <= airk_store::service::registry::catalog::SUPPORTED_POLICY_CONTRACT,
            "native catalog authority changed"
        );
        self.gate
            .reconcile_catalog_snapshot(state.namespace, state.revision)
    }
}
pub struct Management {
    checkpoint_pause: Arc<AtomicBool>,
    checkpoint_observed: Arc<AtomicBool>,
    sender: Option<Arc<SyncSender<Envelope>>>,
    thread: Option<std::thread::JoinHandle<()>>,
    pending: Arc<AtomicUsize>,
    plans: Arc<AtomicBool>,
    missing: Arc<AtomicUsize>,
}

impl Drop for Management {
    fn drop(&mut self) {
        // Do not surrender the service ownership lock while metadata work survives.
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn execute_request(registry: &mut ServiceRegistry, method: ControlMethod) -> ControlResponse {
    let read_only = request_is_read_only(&method);
    execute(registry, method).unwrap_or_else(|error| failure_response(error, read_only))
}
fn request_is_read_only(method: &ControlMethod) -> bool {
    // A failed observation has no effects to replay. RequestEpoch prunes receipts,
    // and all mutations remain outside this whitelist, even when receipt-keyed.
    matches!(
        method,
        ControlMethod::CatalogPage { .. }
            | ControlMethod::CatalogDetail { .. }
            | ControlMethod::CatalogContext { .. }
            | ControlMethod::CatalogHostContext { .. }
            | ControlMethod::CatalogLegacyBinding { .. }
            | ControlMethod::CatalogGrantDraft { .. }
            | ControlMethod::CatalogSourceMutationDraft { .. }
            | ControlMethod::CatalogSourceChangeObservation { .. }
            | ControlMethod::CatalogPendingSourceChange { .. }
            | ControlMethod::CatalogPolicyStatus { .. }
            | ControlMethod::CatalogPolicyResumeReceipt { .. }
            | ControlMethod::CatalogIndexRetirementEvidence { .. }
            | ControlMethod::CatalogSplitSourceState { .. }
            | ControlMethod::CatalogSplitDraft { .. }
            | ControlMethod::CatalogSplitStatus { .. }
            | ControlMethod::CatalogSplitReceipt { .. }
            | ControlMethod::CatalogSplitRecoveryReceipt { .. }
            | ControlMethod::CatalogIndexSetupDraft { .. }
            | ControlMethod::CatalogIndexSetupReceipt { .. }
            | ControlMethod::CatalogReceipt { .. }
            | ControlMethod::Job { .. }
            | ControlMethod::RequestReceipt { .. }
            | ControlMethod::ConfigurationResult { .. }
            | ControlMethod::ReferenceRefreshReceipt { .. }
            | ControlMethod::Plan { .. }
            | ControlMethod::PlanItems { .. }
            | ControlMethod::Projects { .. }
            | ControlMethod::Events { .. }
            | ControlMethod::Maintenance { .. }
            | ControlMethod::Watch { .. }
            | ControlMethod::Progress { .. }
            | ControlMethod::Inspection { .. }
            | ControlMethod::Console { .. }
            | ControlMethod::InactiveConsole { .. }
    )
}
fn failure_response(error: anyhow::Error, read_only: bool) -> ControlResponse {
    if let Some(unknown) =
        error.downcast_ref::<crate::catalog_coordinator::CatalogApplyOutcomeUnknown>()
    {
        return unknown.response();
    }
    if error
        .is::<airk_store::service::registry::catalog::source_change::SourceChangeReviewRequired>()
    {
        return ControlResponse::Failure {
            code: "source_change_review_required".into(),
        };
    }
    if let Some(busy) = error.downcast_ref::<airk_core::catalog_source_change::SourceChangeBusy>() {
        return ControlResponse::SourceChangeBusy {
            code: "source_change_jobs_pending".into(),
            busy: *busy,
        };
    }
    if error
        .downcast_ref::<crate::catalog_coordinator::CatalogApplyRejectedNoEffects>()
        .is_some()
        || error
            .downcast_ref::<airk_core::catalog_fences::FencePrepareRejected>()
            .is_some()
    {
        return ControlResponse::Failure {
            code: if error.is::<crate::catalog_admission::NativeCatalogRetentionFull>() {
                "catalog_native_reconciliation_capacity"
            } else {
                "catalog_preparation_rejected"
            }
            .into(),
        };
    }
    // Source reviews can meet a short collection writer handoff before preview.
    // Only a known read may acquire this retry contract, never a mutation.
    let code = if read_only
        && (airk_store::service::registry::is_read_contention(&error)
            || matches!(
                error.downcast_ref::<std::fs::TryLockError>(),
                Some(std::fs::TryLockError::WouldBlock)
            )) {
        "registry_busy".into()
    } else {
        error
            .downcast_ref::<JobError>()
            .map(|code| format!("{code:?}"))
            .or_else(|| {
                error
                    .downcast_ref::<airk_core::service_events::EventError>()
                    .map(ToString::to_string)
            })
            .or_else(|| {
                error
                    .downcast_ref::<airk_core::service_maintenance::PlanError>()
                    .map(ToString::to_string)
            })
            .or_else(|| {
                (error.is::<airk_store::service::data_scope::ForeignDataRoot>()
                    || error.is::<airk_store::service::data_scope::UnboundDataRoot>())
                .then(|| "data_root_scope_conflict".into())
            })
            .unwrap_or_else(|| "invalid_registry_request".into())
    };
    ControlResponse::Failure { code }
}

fn catalog_request(
    registry: &mut ServiceRegistry,
    control: &std::path::Path,
    method: &ControlMethod,
    catalog: Option<&mut CatalogRuntime>,
) -> Result<Option<ControlResponse>> {
    match method {
        ControlMethod::CatalogPolicyResume {
            index_id,
            operation_id,
            caller,
            key,
        } => {
            let snapshot =
                registry.catalog_resume_policy_job(*index_id, *operation_id, *caller, key)?;
            // An old resume key may be replayed after another deliberate retry.
            // Its exact receipt remains the accepted identity; the accompanying
            // snapshot and job describe current policy state, not that old job.
            let (accepted, _) = registry.request_receipt(*caller, key)?;
            let accepted = accepted
                .ok_or_else(|| anyhow::anyhow!("accepted policy resume receipt unavailable"))?;
            anyhow::ensure!(
                accepted.kind == airk_core::service::JobKind::Configure
                    && accepted.project == snapshot.policy.owner_binding,
                "accepted policy resume receipt binding changed"
            );
            let job = registry.job(snapshot.job)?;
            Ok(Some(ControlResponse::CatalogPolicyResumed {
                index_id: *index_id,
                accepted_job_id: accepted.id,
                status_basis: airk_core::service::PolicyStatusBasis::CurrentPolicy,
                snapshot,
                job,
            }))
        }
        ControlMethod::CatalogPolicyResumeReceipt {
            index_id,
            operation_id,
            caller,
            key,
        } => {
            let (receipt, expired) =
                registry.catalog_policy_resume_receipt(*index_id, *operation_id, *caller, key)?;
            Ok(Some(ControlResponse::CatalogPolicyResumeReceipt {
                index_id: *index_id,
                operation_id: *operation_id,
                caller: *caller,
                key: key.clone(),
                receipt,
                expired,
            }))
        }
        ControlMethod::CatalogPolicyStatus { index_id } => {
            let snapshot = registry.catalog_policy_status(*index_id)?;
            let job = snapshot
                .as_ref()
                .map(|snapshot| registry.job(snapshot.job))
                .transpose()?;
            Ok(Some(ControlResponse::CatalogPolicyStatus {
                index_id: *index_id,
                snapshot,
                job,
            }))
        }
        ControlMethod::CatalogIndexRetirementEvidence { index_id } => {
            Ok(Some(ControlResponse::CatalogIndexRetirementEvidence {
                index_id: *index_id,
                evidence: registry.catalog_index_retirement_evidence(*index_id)?,
            }))
        }
        ControlMethod::CatalogPendingSourceChange { index_id } => {
            Ok(Some(ControlResponse::CatalogPendingSourceChange {
                index_id: *index_id,
                observation: registry.catalog_pending_source_change(*index_id)?,
            }))
        }
        ControlMethod::CatalogSourceChangeObservation { begin_operation } => {
            Ok(Some(ControlResponse::CatalogSourceChangeObservation {
                observation: registry.catalog_source_change_observation(*begin_operation)?,
            }))
        }
        ControlMethod::CatalogSourceChangeResolve { begin_operation } => {
            let known = registry
                .catalog_source_change_observation(*begin_operation)
                .ok();
            let gate = catalog.as_ref().map(|native| native.gate.clone());
            let result = (|| -> Result<_> {
                let observed = known.clone().ok_or_else(|| {
                    anyhow::anyhow!("native source begin observation unavailable")
                })?;
                if observed.completion.is_some() {
                    return Ok(observed);
                }
                let native =
                    catalog.ok_or_else(|| anyhow::anyhow!("native source owner unavailable"))?;
                native.reconcile(registry)?;
                let airk_core::catalog_management::CatalogOperation::BeginIndexSourceChange {
                    plan,
                    ..
                } = &observed.begin.request.operation
                else {
                    anyhow::bail!("native source begin required");
                };
                native
                    .control
                    .resolve_source_change(plan.registration.binding, *begin_operation)
            })();
            let fresh = registry
                .catalog_source_change_observation(*begin_operation)
                .ok();
            if let Some(gate) = gate
                && let Ok(Some(state)) = registry.catalog_state()
            {
                let _ = gate.reconcile_catalog_snapshot(state.namespace, state.revision);
            }
            Ok(Some(match result {
                Ok(observation) => ControlResponse::CatalogSourceChangeResolved { observation },
                Err(_) => ControlResponse::SourceChangeResolutionOutcomeUnknown {
                    code: "source_change_resolution_outcome_unknown".into(),
                    begin_operation: *begin_operation,
                    observation: fresh.or(known),
                },
            }))
        }
        ControlMethod::CatalogSourceMutationDraft { selection } => {
            Ok(Some(ControlResponse::CatalogSourceMutationDraft {
                draft: crate::catalog_source_change_control::draft(registry, control, selection)?,
            }))
        }
        ControlMethod::CatalogPreview { request }
            if matches!(
                request.operation,
                airk_core::catalog_management::CatalogOperation::BeginIndexSourceChange { .. }
                    | airk_core::catalog_management::CatalogOperation::ChangeIndexPolicy { .. }
                    | airk_core::catalog_management::CatalogOperation::CompleteIndexSourceChange { .. }
                    | airk_core::catalog_management::CatalogOperation::RetireIndex { .. }
            ) =>
        {
            let draft = crate::catalog_source_change_control::review_request(
                registry, control, request, None,
            )?;
            Ok(Some(ControlResponse::CatalogPreview {
                preview: draft.preview,
                preview_digest: draft.preview_digest,
            }))
        }
        ControlMethod::CatalogApply {
            request,
            preview_digest,
        } => {
            if matches!(
                request.operation,
                airk_core::catalog_management::CatalogOperation::BeginIndexSourceChange { .. }
                    | airk_core::catalog_management::CatalogOperation::ChangeIndexPolicy { .. }
                    | airk_core::catalog_management::CatalogOperation::CompleteIndexSourceChange { .. }
                    | airk_core::catalog_management::CatalogOperation::RetireIndex { .. }
            ) && matches!(
                registry.catalog_fence_observation(request.caller, &request.key),
                Ok(None)
            ) && registry
                .catalog_receipt(request.caller, &request.key)?
                .is_none()
            {
                // Only first public admission is rebuilt from native state.
                // Exact saved event/completion replays remain authoritative.
                crate::catalog_source_change_control::review_request(
                    registry,
                    control,
                    request,
                    Some(*preview_digest),
                )
                .map_err(|error| {
                    if error
                        .downcast_ref::<airk_core::catalog_source_change::SourceChangeBusy>()
                        .is_some()
                    {
                        error.context(crate::catalog_coordinator::CatalogApplyRejectedNoEffects)
                    } else {
                        error
                    }
                })?;
            }
            if request.operation.requires_authority_fence() {
                let Some(native) = catalog else {
                    return Ok(Some(ControlResponse::Failure {
                        code: "catalog_authority_fence_unavailable".into(),
                    }));
                };
                let receipt =
                    native
                        .coordinator
                        .apply_fenced(registry, request, *preview_digest)?;
                return Ok(Some(ControlResponse::CatalogApplied {
                    receipt: receipt.metadata.clone(),
                    fenced_receipt: Some(receipt),
                }));
            }
            if let Some(native) = catalog {
                // Complete saved replays precede stale input checks. New writes
                // may not advance around an unresolved native authority fence.
                if let Some(receipt) = registry.catalog_receipt(request.caller, &request.key)? {
                    anyhow::ensure!(
                        receipt.request_digest == request.identity()?
                            && !receipt.authority_fence_required,
                        "catalog request key conflict"
                    );
                    return Ok(Some(ControlResponse::CatalogApplied {
                        receipt,
                        fenced_receipt: None,
                    }));
                }
                native.reconcile(registry)?;
                let state = registry
                    .catalog_state()?
                    .ok_or_else(|| anyhow::anyhow!("catalog unavailable"))?;
                native.gate.begin_issue(state.namespace, state.revision)?;
            }
            Ok(Some(ControlResponse::CatalogApplied {
                receipt: registry.catalog_apply(request, *preview_digest)?,
                fenced_receipt: None,
            }))
        }
        ControlMethod::CatalogReceipt { caller, key } => Ok(Some(
            crate::catalog_coordinator::NativeCatalogCoordinator::observe_receipt(
                registry,
                catalog.map(|native| &mut native.coordinator),
                *caller,
                key,
            )?,
        )),
        ControlMethod::ConnectScoped { .. } => Ok(Some(ControlResponse::Failure {
            code: "scoped_connection_requires_direct_delivery".into(),
        })),
        _ => Ok(None),
    }
}

fn execute(registry: &mut ServiceRegistry, method: ControlMethod) -> Result<ControlResponse> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .try_into()?;
    Ok(match method {
        // Only the native Runtime's actual guarded socket writer can make a
        // final combined delivery observation; the metadata actor cannot.
        ControlMethod::ValidateCombinedDelivery { .. } => ControlResponse::Failure {
            code: "combined_delivery_native_owner_required".into(),
        },
        ControlMethod::CatalogPage { request } => ControlResponse::CatalogPage {
            page: registry.catalog_page(&request)?,
            capabilities: airk_store::service::registry::catalog::capabilities(),
        },
        ControlMethod::CatalogDetail {
            entity,
            expected_catalog_revision,
        } => ControlResponse::CatalogDetail {
            detail: registry.catalog_detail(&entity, expected_catalog_revision)?,
            capabilities: airk_store::service::registry::catalog::capabilities(),
        },
        ControlMethod::CatalogContext {
            host,
            key,
            expected_catalog_revision,
        } => ControlResponse::CatalogDetail {
            detail: registry.catalog_detail(
                &airk_core::catalog_management::CatalogEntity::ContextKey { host, key },
                expected_catalog_revision,
            )?,
            capabilities: airk_store::service::registry::catalog::capabilities(),
        },
        ControlMethod::CatalogLegacyBinding {
            project,
            expected_catalog_revision,
        } => ControlResponse::CatalogLegacyBinding {
            observation: crate::catalog_host_context::legacy_binding(
                registry,
                project,
                expected_catalog_revision,
            )?,
        },
        ControlMethod::CatalogHostContext {
            key,
            expected_catalog_revision,
        } => ControlResponse::CatalogHostContext {
            descriptor: crate::catalog_host_context::NativeHostContext::load(
                registry,
                key,
                expected_catalog_revision,
            )?
            .descriptor()
            .clone(),
            capabilities: airk_store::service::registry::catalog::capabilities(),
        },
        ControlMethod::CatalogGrantDraft {
            context_key,
            selection,
        } => {
            selection.validate()?;
            let context = crate::catalog_host_context::NativeHostContext::load(
                registry,
                context_key,
                Some(selection.expected_catalog_revision),
            )?
            .for_review(&selection)?;
            ControlResponse::CatalogGrantDraft {
                draft: crate::catalog_grant_draft::build_reviewed_grant_draft(
                    registry, &context, &selection,
                )?,
            }
        }
        ControlMethod::CatalogPreview { request } => {
            let preview = registry.catalog_preview(&request)?;
            let preview_digest = preview.identity()?;
            ControlResponse::CatalogPreview {
                preview,
                preview_digest,
            }
        }
        ControlMethod::CatalogReceipt { caller, key } => ControlResponse::CatalogReceipt {
            receipt: registry.catalog_receipt(caller, &key)?,
            fence: None,
        },
        ControlMethod::CatalogApply {
            request,
            preview_digest,
        } => {
            // A durable replay is checked before stale revision/epoch guards.
            // Until native admission AND delivery fences are wired, metadata
            // commits requiring those fences must never be reported as complete.
            let saved = registry.catalog_receipt(request.caller, &request.key)?;
            let requires_fence = match saved {
                Some(ref receipt) => receipt.authority_fence_required,
                None => registry.catalog_preview(&request)?.authority_fence_required,
            };
            if requires_fence {
                ControlResponse::Failure {
                    code: "catalog_authority_fence_unavailable".into(),
                }
            } else {
                ControlResponse::CatalogApplied {
                    receipt: registry.catalog_apply(&request, preview_digest)?,
                    fenced_receipt: None,
                }
            }
        }
        ControlMethod::PlanCreate {
            caller,
            key,
            request,
        } => ControlResponse::Plan {
            plan: registry.create_maintenance_plan(
                caller,
                &key,
                &request,
                now_ms,
                crate::runtime_pair::engine_identity()?,
            )?,
        },
        ControlMethod::Plan { id } => ControlResponse::Plan {
            plan: registry.maintenance_plan(id)?,
        },
        ControlMethod::PlanItems { id, after, limit } => ControlResponse::PlanItems {
            page: registry.maintenance_plan_page(id, after, limit)?,
        },
        ControlMethod::PlanExecute {
            id,
            preview,
            allow_fallback,
        } => ControlResponse::Plan {
            plan: registry.approve_maintenance_plan(id, preview, allow_fallback, now_ms)?,
        },
        ControlMethod::PlanPause { id, paused } => ControlResponse::Plan {
            plan: registry.pause_maintenance_plan(id, paused, now_ms)?,
        },
        ControlMethod::PlanCancel { id } => ControlResponse::Plan {
            plan: registry.cancel_maintenance_plan(id, now_ms)?,
        },
        ControlMethod::ReserveProject {
            project,
            setup_watch,
            only_if_missing,
        } => {
            airk_kernel::service_projects::validate_planned(&project)?;
            if only_if_missing {
                registry.register_missing_with_setup(&project, setup_watch)?;
            } else {
                registry.register_with_setup(&project, setup_watch)?;
            }
            ControlResponse::Registered {
                project: registry.get(project.binding)?,
            }
        }
        ControlMethod::Register { project } => {
            airk_kernel::service_projects::validate(&project)?;
            registry.register(&project)?;
            ControlResponse::Registered {
                project: registry.get(project.binding)?,
            }
        }
        ControlMethod::Console {
            after,
            revision,
            limit,
        } => ControlResponse::Console {
            page: console_page(registry, after, revision, limit, now_ms)?,
        },
        ControlMethod::Projects {
            after,
            revision,
            limit,
        } => ControlResponse::Projects {
            page: registry.list(after, revision, limit)?,
        },
        ControlMethod::InactiveConsole {
            after,
            revision,
            limit,
        } => ControlResponse::Console {
            page: console_page_role(registry, after, revision, limit, now_ms, true)?,
        },
        ControlMethod::Maintenance { project } => {
            registry.get(project)?;
            ControlResponse::Maintenance {
                project,
                observation: registry.maintenance_observation(project)?,
                retrying: registry.maintenance_retrying(project)?,
            }
        }
        ControlMethod::Watch { project } => ControlResponse::Watch {
            project,
            observation: registry.watch(project)?.observation,
        },
        ControlMethod::Progress { project } => {
            let paused = registry.get(project)?.paused;
            let pending_job = registry.pending_job_observation(project)?;
            let mut queued_jobs = registry.queued_jobs_observation(project, 17)?;
            let queue_truncated = queued_jobs.len() > 16;
            queued_jobs.truncate(16);
            let pending_paused = paused
                && pending_job
                    .as_ref()
                    .map(|job| registry.job_runs_while_paused(job.id))
                    .transpose()?
                    .is_some_and(|runs| !runs);
            ControlResponse::Progress {
                project,
                observation: registry.progress_observation(project)?.map(Box::new),
                pending_job,
                queued_jobs,
                queue_truncated: Some(queue_truncated),
                project_paused: Some(paused),
                pending_paused: Some(pending_paused),
                observed_unix_ms: Some(now_ms),
            }
        }
        ControlMethod::Inspection { project } => ControlResponse::Inspection {
            project,
            observation: registry.inspection(project)?.map(Box::new),
            cached: true,
        },
        ControlMethod::ConfigureWatch { project, enabled } => {
            registry.configure_watch(project, enabled)?;
            ControlResponse::Watch {
                project,
                observation: registry.watch(project)?.observation,
            }
        }
        ControlMethod::RequestEpoch => {
            let (namespace, epoch) = registry.request_epoch()?;
            ControlResponse::RequestEpoch { namespace, epoch }
        }
        ControlMethod::RequestReceipt {
            caller,
            key,
            project,
        } => {
            let (job, expired) = registry.request_receipt(caller, &key)?;
            if let (Some(project), Some(job)) = (project, job.as_ref()) {
                anyhow::ensure!(project == job.project, "crossed_request_project");
            }
            let removed_project = match project {
                Some(project)
                    if registry.project_was_removed(project)?
                        || !registry.is_registered(project)?
                            && registry.retained_project(project).is_ok() =>
                {
                    Some(project)
                }
                _ => None,
            };
            ControlResponse::RequestReceipt {
                job,
                expired,
                removed_project,
            }
        }
        ControlMethod::Events {
            project,
            cursor,
            limit,
        } => ControlResponse::Events {
            page: registry.events(project, cursor.as_ref(), limit)?,
        },
        ControlMethod::ReferenceRefreshReceipt { caller, key } => {
            crate::reference_refresh_control::receipt(registry, caller, &key)?
        }
        ControlMethod::ConfigurationResult { id } => ControlResponse::Configuration {
            job: registry.job(id)?,
            result: registry.configuration_result(id)?,
        },
        ControlMethod::ConfigureProject {
            project,
            caller,
            key,
            request,
        } => {
            if matches!(
                request.configuration,
                airk_core::service_configuration::ProjectConfiguration::RefreshReferences { .. }
            ) {
                crate::reference_refresh_control::admit_response(
                    registry, project, caller, &key, &request,
                )?
            } else {
                ControlResponse::Job {
                    job: registry.enqueue_configuration(
                        project,
                        caller,
                        &key,
                        &request,
                        crate::runtime_pair::engine_identity()?,
                    )?,
                }
            }
        }
        ControlMethod::Enqueue {
            project,
            caller,
            key,
            kind,
        } => ControlResponse::Job {
            job: registry.enqueue(project, caller, &key, kind)?,
        },
        ControlMethod::Job { id } => ControlResponse::Job {
            job: registry.job(id)?,
        },
        ControlMethod::Retry { id, caller, key } => ControlResponse::Job {
            job: registry.retry_job(id, caller, &key)?,
        },
        ControlMethod::Pause { project, paused } => {
            registry.pause(project, paused)?;
            ControlResponse::Registered {
                project: registry.get(project)?,
            }
        }
        ControlMethod::Cancel { id } => ControlResponse::Job {
            job: registry.request_cancellation(id)?,
        },
        ControlMethod::CancelPending { id } => ControlResponse::Job {
            job: registry.cancel_pending(id)?,
        },
        ControlMethod::Logs { .. }
        | ControlMethod::Health
        | ControlMethod::Shutdown
        | ControlMethod::PrepareUpgradeCheckpoint { .. }
        | ControlMethod::ObserveUpgradeCheckpoint { .. }
        | ControlMethod::UpgradeRecoveryProbe { .. }
        | ControlMethod::ReleaseUpgradeAdmission { .. }
        | ControlMethod::HttpEndpoint
        | ControlMethod::HttpEnroll { .. }
        | ControlMethod::HttpRevoke { .. }
        | ControlMethod::Connect { .. }
        | ControlMethod::ConnectScoped { .. }
        | ControlMethod::NativeRead { .. }
        | ControlMethod::CatalogIndexSetupDraft { .. }
        | ControlMethod::CatalogSourceMutationDraft { .. }
        | ControlMethod::CatalogSourceChangeObservation { .. }
        | ControlMethod::CatalogPendingSourceChange { .. }
        | ControlMethod::CatalogPolicyStatus { .. }
        | ControlMethod::CatalogPolicyResumeReceipt { .. }
        | ControlMethod::CatalogPolicyResume { .. }
        | ControlMethod::CatalogIndexRetirementEvidence { .. }
        | ControlMethod::CatalogSourceChangeResolve { .. }
        | ControlMethod::CatalogIndexSetupApply { .. }
        | ControlMethod::CatalogIndexSetupReceipt { .. }
        | ControlMethod::CatalogSplitSourceState { .. }
        | ControlMethod::CatalogSplitDraft { .. }
        | ControlMethod::CatalogSplitApply { .. }
        | ControlMethod::CatalogSplitStatus { .. }
        | ControlMethod::CatalogSplitReceipt { .. }
        | ControlMethod::CatalogSplitRecover { .. }
        | ControlMethod::CatalogSplitRecoveryReceipt { .. }
        | ControlMethod::EmbeddingStatus
        | ControlMethod::SourceStatus
        | ControlMethod::Resources
        | ControlMethod::Workers { .. }
        | ControlMethod::Worker { .. }
        | ControlMethod::RestartProject { .. }
        | ControlMethod::StartEmbedding
        | ControlMethod::StopEmbedding { .. }
        | ControlMethod::ConfigureResources { .. } => {
            anyhow::bail!("not a registry method")
        }
    })
}

fn observed<T>(
    value: Result<T>,
    kind: airk_core::service_console::ObservationKind,
    failed: &mut Vec<airk_core::service_console::ObservationKind>,
) -> Option<T> {
    match value {
        Ok(value) => Some(value),
        Err(_) => {
            failed.push(kind);
            None
        }
    }
}
pub(crate) fn console_page(
    registry: &mut ServiceRegistry,
    after: Option<airk_core::service::ProjectBinding>,
    revision: Option<u64>,
    limit: usize,
    now_ms: u64,
) -> Result<airk_core::service_console::ConsolePage> {
    console_page_role(registry, after, revision, limit, now_ms, false)
}
fn console_page_role(
    registry: &mut ServiceRegistry,
    after: Option<airk_core::service::ProjectBinding>,
    revision: Option<u64>,
    limit: usize,
    now_ms: u64,
    inactive: bool,
) -> Result<airk_core::service_console::ConsolePage> {
    use airk_core::service_console::{
        ConsolePage, MAX_PAGE_BYTES, ObservationKind as Kind, ProjectObservation,
    };
    let source = if inactive {
        registry.list_inactive(after, revision, limit)?
    } else {
        registry.list(after, revision, limit)?
    };
    let mut page = ConsolePage {
        revision: source.revision,
        observed_unix_ms: now_ms,
        cached: true,
        projects: Vec::new(),
        after: source.after,
    };
    let source_after = source.after;
    let mut overflow = false;
    for project in source.projects {
        let binding = project.registration.binding;
        let mut failed = Vec::new();
        let pending_job = observed(
            registry.pending_job_observation(binding),
            Kind::PendingJob,
            &mut failed,
        )
        .flatten();
        let pending_paused = project.paused
            && pending_job.as_ref().is_some_and(|job| {
                !observed(
                    registry.job_runs_while_paused(job.id),
                    Kind::PendingJob,
                    &mut failed,
                )
                .unwrap_or(true)
            });
        let configuration = observed(
            registry.latest_configuration(binding),
            Kind::Configuration,
            &mut failed,
        )
        .flatten();
        let preference = configuration
            .as_ref()
            .filter(|job| {
                matches!(
                    job.state,
                    airk_core::service::JobState::Pending | airk_core::service::JobState::Running
                )
            })
            .and_then(|job| {
                let work = observed(
                    registry.configuration_work(job.id),
                    Kind::Configuration,
                    &mut failed,
                )
                .flatten()?;
                use airk_core::service_configuration::ProjectConfiguration;
                let (vectors_enabled, detail) = match work.request.configuration {
                    ProjectConfiguration::Vectors { enabled } => (Some(enabled), None),
                    ProjectConfiguration::Detail { level } => (None, Some(level)),
                    _ => return None,
                };
                Some(airk_core::service_console::IndexingPreference {
                    job: job.id,
                    vectors_enabled,
                    detail,
                })
            });
        let row = ProjectObservation {
            pending_gate: observed(
                registry.pending_job_gate(binding),
                Kind::PendingJob,
                &mut failed,
            )
            .flatten(),
            preference,
            recovery: observed(
                registry.recovery_observations(binding),
                Kind::Recovery,
                &mut failed,
            )
            .unwrap_or_default(),
            pending_job,
            pending_paused,
            configuration,
            watch: observed(
                registry.watch(binding).map(|watch| watch.observation),
                Kind::Watch,
                &mut failed,
            ),
            progress: observed(
                registry.progress_observation(binding),
                Kind::Progress,
                &mut failed,
            )
            .flatten(),
            inspection: observed(registry.inspection(binding), Kind::Inspection, &mut failed)
                .flatten(),
            maintenance: observed(
                registry.maintenance_observation(binding),
                Kind::Maintenance,
                &mut failed,
            )
            .flatten(),
            maintenance_retrying: observed(
                registry.maintenance_retrying(binding),
                Kind::MaintenanceRetry,
                &mut failed,
            ),
            failed_observations: failed,
            project,
        };
        page.projects.push(row);
        page.after = Some(binding);
        if serde_json::to_vec(&page)?.len() > MAX_PAGE_BYTES {
            page.projects.pop();
            overflow = true;
            break;
        }
    }
    anyhow::ensure!(
        !overflow || !page.projects.is_empty(),
        "console project observation exceeds page budget"
    );
    page.after = if overflow || source_after.is_some() {
        page.projects
            .last()
            .map(|row| row.project.registration.binding)
    } else {
        None
    };
    Ok(page)
}

impl Management {
    pub(crate) fn start_with_catalog(
        root: PathBuf,
        instance: Incarnation,
        source: crate::source_supervisor::SourceWake,
        log: crate::service_logs::Log,
        ownership: Arc<airk_store::service::ServiceLease>,
        catalog_binding: Option<CatalogBinding>,
    ) -> Result<Self> {
        let strict_catalog = catalog_binding.is_some();
        let (sender, receiver) = sync_channel::<Envelope>(32);
        let pending = Arc::new(AtomicUsize::new(0));
        let missing = Arc::new(AtomicUsize::new(0));
        let accepted_missing = missing.clone();
        let active = pending.clone();
        let plans = Arc::new(AtomicBool::new(true));
        let runnable_plans = plans.clone();
        let checkpoint_pause = Arc::new(AtomicBool::new(false));
        let checkpoint_observed = Arc::new(AtomicBool::new(false));
        let checkpoint_freeze = checkpoint_pause.clone();
        let checkpoint_ack = checkpoint_observed.clone();
        let recovery_fenced = crate::service_runtime::checkpoint::admission_closed(&root)?;
        let (ready, initialized) = sync_channel::<std::result::Result<(), String>>(1);
        let thread = std::thread::Builder::new()
            .name("airk-registry".into())
            .spawn(move || {
                let _ownership = ownership;
                let mut registry =
                    ServiceRegistry::open(&root, instance).and_then(|mut registry| {
                        registry.record_service_instance(&_ownership, instance)?;
                        crate::runtime_install::record_registry_root(
                            &_ownership,
                            registry.incarnation()?,
                        )?;
                        Ok(registry)
                    });
                let (mut catalog, bootstrap) = match catalog_binding {
                    None => (None, Ok(())),
                    Some(binding) => match registry.as_mut() {
                        Ok(registry) => match CatalogRuntime::initialize(binding, registry, source.clone()) {
                            Ok(native) => (Some(native), Ok(())),
                            Err(_) => (None, Err("catalog_bootstrap_unavailable".into())),
                        },
                        Err(_) => (None, Err("catalog_registry_unavailable".into())),
                    },
                };
                let _ = ready.send(bootstrap);
                source.changed();
                let mut next_tick = std::time::Instant::now();
                let mut metadata_failed = false;
                let mut missing_work = std::collections::VecDeque::<MissingWork>::new();
                let mut next_catalog_resume=std::time::Instant::now();
                let split_engine = crate::runtime_pair::engine_identity();
                let mut split_cursor = None;
                let mut clean_break = crate::runtime_install::clean_break_background::Continuation::new(
                    std::env::current_exe().ok(), std::time::Instant::now());
                let mut clean_break_failed = false;
                loop {
                    if recovery_fenced && crate::service_runtime::checkpoint::admission_closed(&root).unwrap_or(true)
                        || checkpoint_freeze.load(Ordering::Acquire)
                        && active.load(Ordering::Acquire) == 0
                        && !runnable_plans.load(Ordering::Acquire)
                        && missing_work.is_empty()
                    {
                        checkpoint_ack.store(true, Ordering::Release);
                        // All accepted operations have settled. Keep ownership
                        // alive, without advancing/pruning metadata after capture.
                        match receiver.recv_timeout(Duration::from_millis(250)) {
                            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                            Ok(envelope) => {
                                checkpoint_ack.store(false, Ordering::Release);
                                match envelope {
                                    Envelope::Request(_, reply) => { active.fetch_sub(1,Ordering::AcqRel); let _=reply.send(ControlResponse::Failure {code:"runtime_upgrade_in_progress".into()}); },
                                    Envelope::AdministrativeRead(_, reply) => { active.fetch_sub(1,Ordering::AcqRel); let _=reply.send(Err(anyhow::anyhow!("runtime_upgrade_in_progress"))); },
                                    Envelope::Missing(_) => { active.fetch_sub(1,Ordering::AcqRel); accepted_missing.fetch_sub(1,Ordering::AcqRel); },
                                }
                                continue;
                            }
                        }
                    }
                    checkpoint_ack.store(false, Ordering::Release);
                    if std::time::Instant::now() >= next_tick {
                        let running = registry.as_mut().ok().and_then(|registry| {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .ok()?;
                            registry
                                .advance_maintenance_plans(now.as_millis().try_into().ok()?)
                                .ok()
                        });
                        if running.is_none() && !metadata_failed {
                            log.record(
                                airk_core::service_logs::LogKind::MetadataUnavailable,
                                None,
                                None,
                            );
                        }
                        metadata_failed = running.is_none();
                        // Unknown metadata state retains service ownership until a
                        // later bounded turn can establish whether work remains.
                        let fence_pending=catalog.as_ref().is_some_and(|native|native.policy_completions_pending||native.coordinator.has_pending_cleanup()||native.gate.pending_fence_key().map_or(true,|key|key.is_some()));
                        let pending_splits = registry.as_ref().ok()
                            .map(|registry| registry.catalog_pending_splits(8));
                        let split_pending = pending_splits.as_ref()
                            .is_some_and(|result| result.as_ref().map_or(true, |items| !items.is_empty()));
                        runnable_plans.store(running.unwrap_or(true)||fence_pending||split_pending, Ordering::Release);
                        next_tick = std::time::Instant::now()
                            + Duration::from_millis(if running.is_some() { 250 } else { 1000 });
                        if std::time::Instant::now()>=next_catalog_resume {
                            let progressed=match (registry.as_mut(),catalog.as_mut()) {
                                (Ok(registry),Some(native))=>native.coordinator.resume_pending(registry).and_then(|progressed| {
                                    native.reconcile_policy_completions(registry)?;
                                    Ok(progressed)
                                }),
                                _=>Ok(None),
                            };
                            next_catalog_resume=std::time::Instant::now()+Duration::from_millis(if progressed.is_err(){1000}else{250});
                            if matches!(progressed,Ok(Some(_))) {source.changed();}
                        }
                        if let (Some(Ok(items)), Ok(engine), Ok(registry), Some(native)) =
                            (pending_splits, split_engine.as_ref(), registry.as_mut(), catalog.as_mut())
                        {
                            let selected = items.iter().find(|snapshot| split_cursor.is_none_or(
                                |cursor| snapshot.receipt.operation_id > cursor,
                            )).or_else(|| items.first());
                            if let Some(snapshot) = selected {
                                split_cursor = Some(snapshot.receipt.operation_id);
                                match native.advance_split(registry, &root, *engine, snapshot.receipt.operation_id) {
                                    Ok(true) => source.changed(),
                                    Ok(false) => {},
                                    Err(_) => {
                                        // Persist attention rather than spin on
                                        // an unaccepted cutover failure. An
                                        // already accepted fence retains its
                                        // existing native resume owner.
                                        if native.gate.pending_fence_key().is_ok_and(|key| key.is_none()) {
                                            let _ = registry.catalog_note_split_failure(
                                                snapshot.receipt.operation_id, snapshot.receipt_digest,
                                                "split_native_advance_needs_attention",
                                            );
                                        }
                                    },
                                }
                            }
                        }
                        if let Some(mut work) = missing_work.pop_front() {
                            if std::time::Instant::now()<work.next_attempt {missing_work.push_back(work);continue;}
                            let outcome = match (registry.as_mut(),catalog.as_mut()) {
                                (Ok(registry),Some(native)) => work.advance(registry,&root,native),
                                _ => Err(anyhow::anyhow!("native missing-source owner unavailable")),
                            };
                            let settled = outcome.is_ok() || (work.draft.is_none() && outcome.as_ref().is_err_and(|error| error.downcast_ref::<crate::catalog_source_change_control::NativeMissingSourceObsolete>().is_some()));
                            if settled {
                                active.fetch_sub(1,Ordering::AcqRel);
                                accepted_missing.fetch_sub(1,Ordering::AcqRel);
                                source.changed();
                            } else {
                                // Accepted intent, its key, and its resource
                                // charge survive transport cancellation/errors.
                                work.next_attempt=std::time::Instant::now()+Duration::from_secs(1);
                                missing_work.push_back(work);
                            }
                        }
                        // No registry transaction/borrow survives this point.
                        // The existing recovery/checkpoint early continue above
                        // suppresses reset mutations while admission is fenced.
                        let cleanup = registry.as_ref()
                            .map_err(|_| anyhow::anyhow!("clean-break registry owner unavailable"))
                            .and_then(|registry| clean_break.tick(
                                &_ownership, registry.incarnation()?, std::time::Instant::now()));
                        if clean_break.pending() { runnable_plans.store(true, Ordering::Release); }
                        if cleanup.is_err() && !clean_break_failed {
                            log.record(airk_core::service_logs::LogKind::ControlFailed, None, None);
                        }
                        clean_break_failed = cleanup.is_err();
                    }
                    let envelope = match receiver.recv_timeout(
                        next_tick.saturating_duration_since(std::time::Instant::now()),
                    ) {
                        Ok(envelope) => envelope,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                            if missing_work.is_empty() {break;}
                            std::thread::sleep(next_tick.saturating_duration_since(std::time::Instant::now()));
                            continue;
                        },
                    };
                    let (method, reply) = match envelope {
                        Envelope::AdministrativeRead(project,sender) => {
                            let result = match (registry.as_ref(),catalog.as_ref()) {
                                (Ok(registry),Some(native)) => native.reconcile(registry).and_then(|_| native.control.administrative_read(project)),
                                _ => Err(anyhow::anyhow!("native administrative catalog unavailable")),
                            };
                            active.fetch_sub(1,Ordering::AcqRel);
                            let _ = sender.send(result);
                            continue;
                        }
                        Envelope::Request(method,reply) => (*method,reply),
                        Envelope::Missing(event) => {
                            if missing_work.iter().any(|work| work.event.binding() == event.binding()
                                && work.event.registration_epoch() == event.registration_epoch()
                                && work.event.configuration_epoch() == event.configuration_epoch()) {
                                active.fetch_sub(1,Ordering::AcqRel);
                                accepted_missing.fetch_sub(1,Ordering::AcqRel);
                            } else {
                                missing_work.push_back(MissingWork {event,nonce:None,draft:None,next_attempt:std::time::Instant::now()});
                            }
                            continue;
                        }
                    };
                    if let ManagementReply::Scoped(sender) = reply {
                        let result = match (registry.as_mut(), catalog.as_ref()) {
                            (Ok(registry), Some(native)) => match method {
                                ControlMethod::ConnectScoped { request } => {
                                    native.connect(registry, &request)
                                }
                                ControlMethod::Connect { project } => {
                                    native.connect_legacy(registry, project)
                                }
                                _ => Err(anyhow::anyhow!("invalid scoped connection route")),
                            },
                            _ => Err(anyhow::anyhow!("catalog_authority_unavailable")),
                        };
                        active.fetch_sub(1, Ordering::AcqRel);
                        match result {
                            Ok(delivery) => delivery.handoff_native(sender),
                            Err(error) => {
                                let _ = sender.send(Err(error));
                            }
                        }
                        continue;
                    }
                    let source_changed = matches!(
                        method,
                        ControlMethod::ConfigureWatch { .. }
                            | ControlMethod::CatalogIndexSetupApply { .. }
                            | ControlMethod::CatalogSourceChangeResolve { .. }
                            | ControlMethod::Register { .. }
                            | ControlMethod::ReserveProject { .. }
                    );
                    let plan_changed = matches!(
                        method,
                        ControlMethod::PlanCreate { .. }
                            | ControlMethod::PlanExecute { .. }
                            | ControlMethod::PlanPause { .. }
                            | ControlMethod::PlanCancel { .. }
                    );
                    let response = match method {
                        ControlMethod::Logs { limit } => {
                            match crate::service_logs::recent(&root, limit) {
                                Ok(page) => ControlResponse::Logs { page },
                                Err(_) => ControlResponse::Failure {
                                    code: "diagnostic_logs_unavailable".into(),
                                },
                            }
                        }
                        method => match registry.as_mut() {
                            Ok(registry) => {
                                let routed = crate::catalog_split_control::execute(
                                    registry, &root, &method,
                                )
                                .and_then(|split| match split {
                                    Some(response) => Ok(Some(response)),
                                    None => crate::catalog_index_setup_control::execute(
                                        registry, &root, &method,
                                    ),
                                })
                                .and_then(|setup| match setup {
                                    Some(response) => Ok(Some(response)),
                                    None => {
                                        catalog_request(registry, &root, &method, catalog.as_mut())
                                    }
                                });
                                let response = match routed {
                                    Ok(Some(response)) => response,
                                    Ok(None) => execute_request(registry, method),
                                    Err(error) => {
                                        let error=match &method {ControlMethod::CatalogApply{request,..}=>crate::catalog_coordinator::CatalogApplyOutcomeUnknown::capture(registry,request,error),_=>error};
                                        failure_response(error, request_is_read_only(&method))
                                    },
                                };
                                finalize_catalog_response(response, || match catalog.as_ref() {
                                    Some(native) => native.reconcile(registry),
                                    None => Ok(()),
                                })
                            }
                            Err(_) => ControlResponse::Failure {
                                code: "registry_unavailable".into(),
                            },
                        },
                    };
                    if source_changed {
                        source.changed();
                    }
                    if plan_changed {
                        // Accepted plan work must prevent idle exit immediately,
                        // including the interval before its first dispatch turn.
                        runnable_plans.store(true, Ordering::Release);
                        next_tick = std::time::Instant::now();
                    }
                    // A timed-out/disconnected caller cannot release this charge
                    // while its accepted metadata operation can still commit.
                    if catalog.as_ref().is_some_and(|native|native.policy_completions_pending||native.coordinator.has_pending_cleanup()||native.gate.pending_fence_key().map_or(true,|key|key.is_some())) {runnable_plans.store(true,Ordering::Release);}
                    active.fetch_sub(1, Ordering::AcqRel);
                    let _ = reply.send(response);
                }
            })?;
        let management = Self {
            checkpoint_pause,
            checkpoint_observed,
            sender: Some(Arc::new(sender)),
            thread: Some(thread),
            pending,
            plans,
            missing,
        };
        if strict_catalog {
            initialized
                .recv_timeout(Duration::from_secs(30))
                .map_err(|_| anyhow::anyhow!("catalog_bootstrap_unknown"))?
                .map_err(|code| anyhow::anyhow!(code))?;
        } else {
            let _ = initialized.recv_timeout(Duration::from_secs(5));
        }
        Ok(management)
    }

    pub async fn request(&self, method: ControlMethod) -> ControlResponse {
        let (sender, receiver) = oneshot::channel();
        self.pending.fetch_add(1, Ordering::AcqRel);
        if self.sender.as_ref().is_none_or(|queue| {
            queue
                .try_send(Envelope::Request(
                    Box::new(method),
                    ManagementReply::Control(sender),
                ))
                .is_err()
        }) {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            return ControlResponse::Failure {
                code: "registry_busy".into(),
            };
        }
        match tokio::time::timeout(Duration::from_secs(3), receiver).await {
            Ok(Ok(response)) => response,
            _ => ControlResponse::Failure {
                code: "registry_response_unknown_retry_with_key".into(),
            },
        }
    }
    pub(crate) async fn request_scoped(
        &self,
        request: airk_core::service::ScopedConnectRequest,
    ) -> Result<crate::project_supervisor::ScopedConnectionDelivery> {
        self.request_connection(ControlMethod::ConnectScoped { request })
            .await
    }
    pub(crate) async fn request_administrative_read(
        &self,
        project: ProjectBinding,
    ) -> Result<crate::project_supervisor::NativeAdministrativeRead> {
        let (sender, receiver) = oneshot::channel();
        self.pending.fetch_add(1, Ordering::AcqRel);
        if self.sender.as_ref().is_none_or(|queue| {
            queue
                .try_send(Envelope::AdministrativeRead(project, sender))
                .is_err()
        }) {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            anyhow::bail!("native administrative service busy");
        }
        tokio::time::timeout(Duration::from_secs(15), receiver)
            .await
            .map_err(|_| anyhow::anyhow!("native administrative admission outcome unknown"))?
            .map_err(|_| anyhow::anyhow!("native administrative owner unavailable"))?
    }
    pub(crate) async fn request_legacy(
        &self,
        project: airk_core::service::ProjectBinding,
    ) -> Result<crate::project_supervisor::ScopedConnectionDelivery> {
        self.request_connection(ControlMethod::Connect { project })
            .await
    }
    async fn request_connection(
        &self,
        method: ControlMethod,
    ) -> Result<crate::project_supervisor::ScopedConnectionDelivery> {
        let (sender, receiver) = oneshot::channel();
        self.pending.fetch_add(1, Ordering::AcqRel);
        let sent = self.sender.as_ref().map(|queue| {
            queue.try_send(Envelope::Request(
                Box::new(method),
                ManagementReply::Scoped(sender),
            ))
        });
        match sent {
            Some(Ok(())) => {}
            Some(Err(std::sync::mpsc::TrySendError::Full(_))) => {
                self.pending.fetch_sub(1, Ordering::AcqRel);
                return Err(crate::project_supervisor::ConnectionSetupPending::Queue.into());
            }
            _ => {
                self.pending.fetch_sub(1, Ordering::AcqRel);
                anyhow::bail!("catalog_connection_owner_closed");
            }
        }
        tokio::time::timeout(Duration::from_secs(15), receiver)
            .await
            .map_err(|_| anyhow::anyhow!("catalog_connection_outcome_unknown"))?
            .map_err(|_| anyhow::anyhow!("catalog_connection_owner_closed"))?
    }
    pub(crate) fn begin_checkpoint_freeze(&self) {
        self.checkpoint_pause.store(true, Ordering::Release);
    }
    pub(crate) fn checkpoint_observed(&self) -> bool {
        self.checkpoint_observed.load(Ordering::Acquire)
    }
    pub fn can_idle_exit(&self) -> bool {
        self.pending.load(Ordering::Acquire) == 0 && !self.plans.load(Ordering::Acquire)
    }
    pub(crate) fn missing_reporter(
        &self,
    ) -> Arc<dyn crate::source_supervisor::MissingSourceReporter> {
        Arc::new(MissingReporter {
            sender: self.sender.as_ref().map(Arc::downgrade).unwrap_or_default(),
            pending: self.pending.clone(),
            missing: self.missing.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        identity::Incarnation,
        service::ProjectRegistration,
        service_console::{MAX_PAGE_BYTES, ObservationKind},
    };
    #[tokio::test]
    async fn actual_connection_queue_distinguishes_full_from_closed_before_acceptance() -> Result<()>
    {
        let (sender, receiver) = sync_channel(0);
        let owner = Management {
            checkpoint_pause: Arc::new(AtomicBool::new(false)),
            checkpoint_observed: Arc::new(AtomicBool::new(false)),
            sender: Some(Arc::new(sender)),
            thread: None,
            pending: Arc::new(AtomicUsize::new(0)),
            plans: Arc::new(AtomicBool::new(false)),
            missing: Arc::new(AtomicUsize::new(0)),
        };
        let request = || ControlMethod::Connect {
            project: airk_core::identity::Sha256Digest::from_bytes([3; 32]),
        };
        let full = owner
            .request_connection(request())
            .await
            .err()
            .expect("full queue must reject");
        assert!(full.is::<crate::project_supervisor::ConnectionSetupPending>());
        assert_eq!(owner.pending.load(Ordering::Acquire), 0);
        drop(receiver);
        let closed = owner
            .request_connection(request())
            .await
            .err()
            .expect("closed queue must reject");
        assert!(!closed.is::<crate::project_supervisor::ConnectionSetupPending>());
        assert_eq!(owner.pending.load(Ordering::Acquire), 0);
        Ok(())
    }
    #[test]
    fn catalog_reads_do_not_activate_legacy_registry() -> Result<()> {
        use airk_core::catalog_management::{CatalogKind, CatalogPageRequest, MAX_PAGE_BYTES};
        let root = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([18; 16]))?;
        let response = execute_request(
            &mut registry,
            ControlMethod::CatalogPage {
                request: CatalogPageRequest {
                    kind: CatalogKind::Projects,
                    index_id: None,
                    project_id: None,
                    after: None,
                    expected_catalog_revision: None,
                    limit: 1,
                    max_bytes: MAX_PAGE_BYTES,
                },
            },
        );
        assert!(matches!(response, ControlResponse::Failure { .. }));
        assert!(registry.catalog_state()?.is_none());
        Ok(())
    }

    #[test]
    fn registry_read_contention_preserves_receipts_and_other_failures() -> Result<()> {
        use airk_core::service::{JobKind, RequestKey};
        for code in [
            rusqlite::ffi::SQLITE_CORRUPT,
            rusqlite::ffi::SQLITE_IOERR,
            rusqlite::ffi::SQLITE_CONSTRAINT,
        ] {
            let error =
                rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), None).into();
            assert!(!airk_store::service::registry::is_read_contention(&error));
        }
        // The source-review adapter can contend with the collection writer
        // before reaching SQL. Retry remains limited to an established read.
        let locks = tempfile::tempdir()?;
        let file = std::fs::File::create(locks.path().join("native-writer"))?;
        file.try_lock()?;
        let second = std::fs::File::open(locks.path().join("native-writer"))?;
        let error = second.try_lock().unwrap_err();
        assert!(
            matches!(failure_response(anyhow::Error::new(error),true),ControlResponse::Failure{code} if code=="registry_busy")
        );
        let error = second.try_lock().unwrap_err();
        assert!(
            matches!(failure_response(anyhow::Error::new(error),false),ControlResponse::Failure{code} if code=="invalid_registry_request")
        );
        assert!(
            matches!(failure_response(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into(),true),ControlResponse::Failure{code} if code=="invalid_registry_request")
        );
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        crate::local_ipc::prepare_root(&root)?;
        let caller = Incarnation::from_bytes([92; 16]);
        let mut registry = ServiceRegistry::open(&root, caller)?;
        let project = ProjectRegistration::new(
            owner.path().join("source").to_str().unwrap().into(),
            owner.path().join("data").to_str().unwrap().into(),
            "read-contention".into(),
        )?;
        registry.register(&project)?;
        let (namespace, epoch) = registry.request_epoch()?;
        let job = registry.enqueue(
            project.binding,
            caller,
            &RequestKey {
                namespace,
                epoch,
                nonce: caller,
            },
            JobKind::Inspect,
        )?;
        let sql = rusqlite::Connection::open(root.join("service-1.4.sqlite3"))?;
        sql.execute_batch("BEGIN EXCLUSIVE")?;
        let error = execute(&mut registry, ControlMethod::Job { id: job.id })
            .err()
            .expect("held registry must block a read");
        assert!(airk_store::service::registry::is_read_contention(&error));
        assert!(
            matches!(execute_request(&mut registry, ControlMethod::Job { id: job.id }),
            ControlResponse::Failure { code } if code == "registry_busy")
        );
        // A mutation must not acquire the read-only retry contract.
        assert!(
            matches!(execute_request(&mut registry, ControlMethod::Pause { project: project.binding, paused: true }),
            ControlResponse::Failure { code } if code == "invalid_registry_request")
        );
        assert!(
            matches!(execute_request(&mut registry, ControlMethod::RequestEpoch),
            ControlResponse::Failure { code } if code == "invalid_registry_request")
        );
        sql.execute_batch("ROLLBACK")?;
        assert_eq!(registry.saved_request_epoch()?, (namespace, epoch));
        let ControlResponse::Job { job: observed } =
            execute_request(&mut registry, ControlMethod::Job { id: job.id })
        else {
            panic!("same receipt unavailable after releasing the registry")
        };
        assert_eq!(observed.id, job.id);
        assert_eq!(observed.attempts, 0);
        assert!(!registry.get(project.binding)?.paused);
        // Incoherent stored state is still a failure, never retried as contention.
        sql.execute(
            "UPDATE service_jobs SET state='succeeded' WHERE id=?1",
            [job.id.to_string()],
        )?;
        assert!(
            matches!(execute_request(&mut registry, ControlMethod::Job { id: job.id }),
            ControlResponse::Failure { code } if code == "invalid_registry_request")
        );
        assert!(!owner.path().join("source").exists());
        assert!(!owner.path().join("data").exists());
        Ok(())
    }

    #[test]
    fn timed_out_metadata_keeps_ownership_and_exact_key_before_and_after_commit() -> Result<()> {
        use airk_core::service::{JobKind, RequestKey};
        for commit_before_timeout in [false, true] {
            let owner = tempfile::tempdir()?;
            let root = owner.path().join("runtime");
            crate::local_ipc::prepare_root(&root)?;
            let caller = Incarnation::from_bytes([91; 16]);
            let mut registry = ServiceRegistry::open(&root, caller)?;
            let project = ProjectRegistration::new(
                owner.path().join("source").to_str().unwrap().into(),
                owner.path().join("data").to_str().unwrap().into(),
                "unknown-receipt".into(),
            )?;
            registry.register(&project)?;
            let (namespace, epoch) = registry.request_epoch()?;
            let request = ControlMethod::Enqueue {
                project: project.binding,
                caller,
                key: RequestKey {
                    namespace,
                    epoch,
                    nonce: caller,
                },
                kind: JobKind::Index,
            };
            let (sender, receiver) = sync_channel::<Envelope>(2);
            let (release, gate) = sync_channel(1);
            let (committed, receipt) = sync_channel(1);
            let pending = Arc::new(AtomicUsize::new(0));
            let active = pending.clone();
            // Gate the actual registry operation or its acknowledgement. This
            // forces both ambiguous outcomes without relying on disk speed.
            let thread = std::thread::spawn(move || {
                for attempt in 0..2 {
                    let Envelope::Request(method, reply) =
                        receiver.recv_timeout(Duration::from_secs(15)).unwrap()
                    else {
                        panic!("unexpected native event");
                    };
                    if attempt == 0 && !commit_before_timeout {
                        gate.recv_timeout(Duration::from_secs(15)).unwrap();
                    }
                    let response = execute(&mut registry, *method).unwrap();
                    if attempt == 0 {
                        let ControlResponse::Job { job } = &response else {
                            panic!("missing committed job")
                        };
                        committed.send(job.id).unwrap();
                        if commit_before_timeout {
                            gate.recv_timeout(Duration::from_secs(15)).unwrap();
                        }
                    }
                    active.fetch_sub(1, Ordering::AcqRel);
                    let _ = reply.send(response);
                }
            });
            let management = Management {
                checkpoint_pause: Arc::new(AtomicBool::new(false)),
                checkpoint_observed: Arc::new(AtomicBool::new(false)),
                sender: Some(Arc::new(sender)),
                thread: Some(thread),
                pending,
                plans: Arc::new(AtomicBool::new(false)),
                missing: Arc::new(AtomicUsize::new(0)),
            };
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let unknown = runtime.block_on(management.request(request.clone()));
            assert!(
                matches!(unknown, ControlResponse::Failure { code } if code == "registry_response_unknown_retry_with_key")
            );
            assert!(
                !management.can_idle_exit(),
                "unknown work released ownership"
            );
            let before = receipt.try_recv();
            assert_eq!(before.is_ok(), commit_before_timeout);
            release.send(())?;
            let original = before.or_else(|_| receipt.recv_timeout(Duration::from_secs(15)))?;
            let ControlResponse::Job { job } = runtime.block_on(management.request(request)) else {
                panic!("same-key retry lost its receipt")
            };
            assert_eq!(job.id, original, "same-key retry duplicated native work");
            assert!(management.can_idle_exit());
            assert!(!owner.path().join("source").exists());
            assert!(!owner.path().join("data").exists());
        }
        Ok(())
    }
    #[test]
    fn console_explains_paused_maintenance_owner_without_pausing_the_project() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        crate::local_ipc::prepare_root(&root)?;
        let service = Incarnation::from_bytes([76; 16]);
        let mut registry = ServiceRegistry::open(&root, service)?;
        let project =
            ProjectRegistration::new("/source".into(), "/data".into(), "plan-wait".into())?;
        registry.register(&project)?;
        let (namespace, epoch) = registry.request_epoch()?;
        let plan = registry.create_maintenance_plan(
            service,
            &airk_core::service::RequestKey {
                namespace,
                epoch,
                nonce: service,
            },
            &airk_core::service_maintenance::RepairRequest {
                selection: airk_core::service_maintenance::RepairSelection::Selected(vec![
                    project.binding,
                ]),
                operation: airk_core::service_maintenance::RepairOperation::Verify,
            },
            1000,
            airk_core::identity::Sha256Digest::from_bytes([2; 32]),
        )?;
        registry.advance_maintenance_plans(1100)?;
        registry.pause_maintenance_plan(plan.id, true, 1200)?;
        let page = console_page(&mut registry, None, None, 1, 1300)?;
        let row = &page.projects[0];
        assert!(!row.project.paused);
        assert!(!row.pending_paused);
        let gate = row.pending_gate.as_ref().unwrap();
        assert_eq!(gate.plan, Some(plan.id));
        assert_eq!(gate.code, "maintenance_plan_paused");
        assert_eq!(gate.job, row.pending_job.as_ref().unwrap().id);
        let encoded = serde_json::to_value(&page)?;
        assert_eq!(
            encoded["projects"][0]["pending_gate"]["code"],
            "maintenance_plan_paused"
        );
        assert!(registry.next_pending_job(project.binding)?.is_none());
        Ok(())
    }
    #[test]
    fn console_observes_paused_queue_without_admitting_or_opening_project_state() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        crate::local_ipc::prepare_root(&root)?;
        let service = Incarnation::from_bytes([77; 16]);
        let mut registry = ServiceRegistry::open(&root, service)?;
        let project = ProjectRegistration::new("/source".into(), "/data".into(), "queued".into())?;
        registry.register(&project)?;
        registry.pause(project.binding, true)?;
        let (namespace, epoch) = registry.request_epoch()?;
        let job = registry.enqueue(
            project.binding,
            service,
            &airk_core::service::RequestKey {
                namespace,
                epoch,
                nonce: service,
            },
            airk_core::service::JobKind::Index,
        )?;
        let page = console_page(&mut registry, None, None, 1, 100)?;
        let row = &page.projects[0];
        assert_eq!(row.pending_job.as_ref().unwrap().id, job.id);
        assert!(row.pending_paused);
        assert!(row.progress.is_none());
        assert!(registry.next_pending_job(project.binding)?.is_none());
        assert!(row.failed_observations.is_empty());
        Ok(())
    }
    #[test]
    fn console_pages_bound_complete_rows_and_isolate_damaged_cached_observations() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        crate::local_ipc::prepare_root(&root)?;
        let service = Incarnation::from_bytes([77; 16]);
        let mut registry = ServiceRegistry::open(&root, service)?;
        let sql = rusqlite::Connection::open(root.join("service-1.4.sqlite3"))?;
        let mut bindings = Vec::new();
        for n in 0..32u8 {
            let project = ProjectRegistration::new(
                owner
                    .path()
                    .join("sources")
                    .join("s".repeat(160))
                    .join(format!("p-{n}"))
                    .to_str()
                    .unwrap()
                    .into(),
                owner
                    .path()
                    .join("data")
                    .join("d".repeat(160))
                    .join(format!("p-{n}"))
                    .to_str()
                    .unwrap()
                    .into(),
                format!("fixture-{n:02}-{}", "x".repeat(100)),
            )?;
            registry.register(&project)?;
            let mut lease = airk_store::service::ProjectLease::acquire(&root, project.binding)?;
            let epoch = registry.admit_worker(&mut lease)?;
            let worker = airk_core::service::WorkerIdentity {
                service,
                worker: Incarnation::from_bytes([n + 1; 16]),
                project: project.binding,
                fencing_generation: epoch,
            };
            let observation = serde_json::json!({"worker":worker,"current_fencing_generation":epoch,"job":project.binding,"observed_unix_ms":1,
                "snapshot":{"project":project.binding,"incarnation":worker.worker,"publication":null,"settings_digest":null,
                "index_state":"unavailable","source_state":"unknown","recovery_required":null,
                "indexed_files":u64::MAX,"indexed_vectors":u64::MAX,"vectors_enabled":true,
                "semantic":{"state":"partial","selected_artifacts":u64::MAX,"completed_artifacts":u64::MAX-1,
                    "selected_units":u64::MAX,"completed_units":u64::MAX-1,"failed":false},
                "discovery":{"state":"ready","processed_artifacts":u64::MAX,"committed_artifacts":u64::MAX,"total_artifacts":u64::MAX,"failed":false},
                "failure":"index_observation_failed","storage":{"collection_used_bytes":u64::MAX,"collection_reserved_bytes":u64::MAX,
                    "collection_limit_bytes":u64::MAX,"root_available_bytes":u64::MAX,"physical_available_growth_bytes":u64::MAX,"replacement_estimate_bytes":u64::MAX}}});
            let parsed: airk_core::service_inspection::InspectionObservation =
                serde_json::from_value(observation)?;
            parsed.snapshot.validate()?;
            sql.execute(
                "INSERT INTO service_inspections(project,payload) VALUES(?1,?2)",
                rusqlite::params![project.binding.to_string(), serde_json::to_vec(&parsed)?],
            )?;
            bindings.push(project.binding);
        }
        let mut after = None;
        let mut revision = None;
        let mut seen = std::collections::BTreeSet::new();
        let mut pages = 0;
        loop {
            let page = console_page(&mut registry, after, revision, 32, 100)?;
            pages += 1;
            assert!(page.cached);
            assert_eq!(page.observed_unix_ms, 100);
            assert!(serde_json::to_vec(&page)?.len() <= MAX_PAGE_BYTES);
            for row in &page.projects {
                assert!(seen.insert(row.project.registration.binding));
                assert!(row.inspection.is_some());
            }
            revision = Some(page.revision);
            match page.after {
                Some(next) => {
                    assert!(after.is_none_or(|previous| next > previous));
                    after = Some(next);
                }
                None => break,
            }
            assert!(pages < 32);
        }
        assert_eq!(seen.len(), 32);
        assert!(pages > 1, "fixture must exercise byte-budget continuation");
        sql.execute(
            "UPDATE service_inspections SET payload=?1 WHERE project=?2",
            rusqlite::params![b"{broken".as_slice(), bindings[0].to_string()],
        )?;
        let mut after = None;
        let mut found = false;
        loop {
            let page = console_page(&mut registry, after, revision, 32, 200)?;
            if let Some(row) = page
                .projects
                .iter()
                .find(|row| row.project.registration.binding == bindings[0])
            {
                assert!(row.inspection.is_none());
                assert!(
                    row.failed_observations
                        .iter()
                        .any(|kind| matches!(kind, ObservationKind::Inspection))
                );
                found = true;
            }
            match page.after {
                Some(next) => after = Some(next),
                None => break,
            }
        }
        assert!(found);
        assert!(
            !owner.path().join("sources").exists() && !owner.path().join("data").exists(),
            "console metadata created/opened project namespaces"
        );
        registry.register(&ProjectRegistration::new(
            owner.path().join("fresh-source").to_str().unwrap().into(),
            owner.path().join("fresh-data").to_str().unwrap().into(),
            "fresh".into(),
        )?)?;
        assert!(
            console_page(&mut registry, None, revision, 32, 300).is_err(),
            "changed registry accepted old pagination identity"
        );
        Ok(())
    }
}
