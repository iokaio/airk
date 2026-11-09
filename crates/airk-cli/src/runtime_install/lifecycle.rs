// SPDX-License-Identifier: Apache-2.0
//! Instance-fenced stop/restart, durable in the service registry rather than a client window.
use super::*;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::RequestKey,
    service_lifecycle::{LifecycleAction, LifecyclePhase, LifecycleRecord, LifecycleRequest},
};
use airk_store::service::registry::ServiceRegistry;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    caller: Incarnation,
    key: RequestKey,
    action: LifecycleAction,
    expected_instance: Incarnation,
    runtime: Sha256Digest,
    #[serde(default)]
    force: bool,
}

fn registry(root: &Path) -> Result<ServiceRegistry> {
    let control = layout::control(root)?;
    layout::validate_control(&control)?;
    // Lifecycle never creates a replacement registry in order to stop something.
    ensure!(
        control.join("service-1.4.sqlite3").try_exists()?,
        "service_registry_unavailable"
    );
    ServiceRegistry::open_saved(&control)
}

fn with_lifecycle<T>(
    authority: &InstallerAuthority<'_>,
    action: impl FnOnce(&mut airk_store::service::registry::ExistingLifecycleLedger) -> Result<T>,
) -> Result<T> {
    let saved = registry(authority.root)?;
    let control = saved.control_root().to_path_buf();
    let namespace = saved.incarnation()?;
    drop(saved);
    ServiceRegistry::with_existing_lifecycle(&control, namespace, action)
}
fn owner(root: &Path) -> Result<bool> {
    if let Some(policy) = managed::saved_policy_for_lifecycle(root)? {
        ensure!(
            policy.lifecycle == ManagedLifecycle::UserOnDemand,
            "lifecycle_managed_externally"
        );
        return Ok(false);
    }
    crate::service_manager::ServiceManager::for_runtime(root)?.owns_restart()
}
pub(super) fn inspect(root: &Path) -> Result<serde_json::Value> {
    let control = layout::control(root)?;
    if !control.join("service-1.4.sqlite3").try_exists()? {
        ensure!(
            !control.join("service-1.4.owner.json").try_exists()?,
            "initialized_service_registry_is_missing"
        );
        return Ok(serde_json::json!({"explicitly_stopped":false,"pending":null}));
    }
    layout::validate_control(&control)?;
    let registry = ServiceRegistry::open_saved(&control)?;
    Ok(
        serde_json::json!({"explicitly_stopped":registry.service_explicitly_stopped()?,"pending":registry.pending_lifecycle()?,
            "recorded_instance":registry.recorded_service_instance()?,"namespace":registry.incarnation()?,"epoch":registry.saved_request_epoch()?.1}),
    )
}

pub(super) fn require_start_allowed(root: &Path) -> Result<()> {
    let state = inspect(root)?;
    ensure!(
        state["explicitly_stopped"] == false,
        "service_explicitly_stopped"
    );
    Ok(())
}
pub(super) fn require_settled(root: &Path) -> Result<()> {
    ensure!(
        inspect(root)?["pending"].is_null(),
        "resume_or_abandon_pending_lifecycle_first"
    );
    Ok(())
}
pub(super) fn abandon(root: &Path, identity: Sha256Digest) -> Result<LifecycleRecord> {
    let lock = install_lock(root)?;
    let authority = InstallerAuthority { root, _lock: &lock };
    let record = registry(root)?
        .lifecycle_by_id(identity)?
        .context("lifecycle request not found")?;
    if matches!(
        record.phase,
        LifecyclePhase::Complete | LifecyclePhase::Abandoned
    ) {
        return Ok(record);
    }
    with_lifecycle(&authority, |ledger| {
        ledger.advance_lifecycle(&record, LifecyclePhase::Abandoned)
    })
}

/// Only installed_control's held-lock connection route uses this witness.
/// Direct ensure_running keeps saved-identity STOP settlement in its old order.
pub(super) enum ConnectionSettlement {
    NoEffects,
    EffectsRequired,
}
pub(super) fn connection_settlement(root: &Path) -> Result<ConnectionSettlement> {
    let state = match inspect(root) {
        Ok(state) => state,
        // An unreadable/older/migration-required observation is NOT a no-op.
        // Drop the proof before the existing authoritative settlement path.
        Err(_) => return Ok(ConnectionSettlement::EffectsRequired),
    };
    if !state["pending"].is_null() {
        return Ok(ConnectionSettlement::EffectsRequired);
    }
    ensure!(
        state["explicitly_stopped"] == false,
        "service_explicitly_stopped"
    );
    Ok(ConnectionSettlement::NoEffects)
}

/// Installation lock is held by the caller. A stopped service stays stopped
/// across automatic bridge reconnects and extension reloads.
pub(super) async fn settle(authority: &InstallerAuthority<'_>) -> Result<()> {
    let root = authority.root;
    let control = layout::control(root)?;
    if !control.join("service-1.4.sqlite3").try_exists()? {
        return require_start_allowed(root);
    }
    if let Some(record) = registry(root)?.pending_lifecycle()? {
        resume(authority, record, false).await?;
    }
    require_start_allowed(root)
}
pub(super) async fn explicit_start(root: &Path) -> Result<ServiceHealth> {
    let lock = install_lock(root)?;
    let authority = InstallerAuthority { root, _lock: &lock };
    payload_repair::require_settled(root)?;
    let reset_required = state_contract::required(root)?;
    let accepted_reset = activation::rebuild::pending(root)?;
    ensure!(
        !reset_required || accepted_reset,
        "clean_break_required: Install or Repair CLI must accept the fresh baseline before Start"
    );
    managed::catalog::resume_before_lifecycle(&authority, None)?;
    owner(root)?;
    let control = layout::control(root)?;
    if control.join("service-1.4.sqlite3").try_exists()? {
        if let Some(record) = registry(root)?.pending_lifecycle()? {
            resume(&authority, record, false).await?;
        }
        ensure!(
            managed::read(root)?.is_some() || layout::managed_origin(root)?.is_none(),
            "repeat_same_approved_managed_configure_to_resume"
        );
        with_lifecycle(&authority, |ledger| ledger.clear_service_stop())?;
    } else {
        require_start_allowed(root)?;
    }
    if accepted_reset {
        activation::rebuild::resume(root).await?;
        let (_, main, approval) = selected(root)?;
        return activation::start_selected(root, &approval, &main, None).await;
    }
    if managed::read(root)?.is_some() {
        return managed::ensure_checked(&authority, None, || Ok(())).await;
    }
    if activation::pending(root)?.is_some() {
        activation::resume(root).await?;
    }
    launcher_repair::require_settled(root)?;
    let (_, main, approval) = selected(root)?;
    activation::start_selected(root, &approval, &main, None).await
}
pub(super) async fn run(root: &Path, input: Input) -> Result<LifecycleRecord> {
    run_with_admission_budget(root, input, Duration::from_secs(60)).await
}

pub(super) async fn run_with_admission_budget(
    root: &Path,
    input: Input,
    budget: Duration,
) -> Result<LifecycleRecord> {
    // Launcher admission may briefly own the same installer lock. Wait before
    // accepting lifecycle work; never replay a submitted Stop/Restart operation.
    // Keep this lock through every authority check and the complete operation.
    let lock = connection_install_lock(root, budget).await?;
    let authority = InstallerAuthority { root, _lock: &lock };
    activation::require_settled(root)?;
    launcher_repair::require_settled(root)?;
    ensure!(
        !root.join("uninstall.json").try_exists()?,
        "runtime_uninstall_is_pending"
    );
    ensure!(
        input.action != LifecycleAction::Restart || !state_contract::required(root)?,
        "clean_break_required: Restart cannot reopen predecessor state"
    );
    let manager = owner(root)?;
    let request = LifecycleRequest {
        caller: input.caller,
        key: input.key,
        action: input.action,
        expected_instance: input.expected_instance,
        runtime: input.runtime,
        manager,
    };
    let ledger = registry(root)?;
    if let Some(record) = ledger.lifecycle_record(&request)? {
        drop(ledger);
        return resume(&authority, record, input.force).await;
    }
    let saved = snapshot::identity(root)?;
    ensure!(
        saved.selection.active == request.runtime.to_string(),
        "lifecycle_runtime_selection_changed"
    );
    if request.action == LifecycleAction::Restart {
        payload_repair::require_settled(root)?;
        verify_runtime(root, &saved.selection.active)?;
    }
    match observation(root, &saved.approval, &saved.executable_sha256).await {
        Ok(current) => ensure!(
            current.instance == request.expected_instance,
            "service_instance_changed"
        ),
        Err(error) => {
            ensure!(
                input.force && manager,
                "lifecycle_health_unavailable: {error:#}"
            );
            ensure!(
                ledger.recorded_service_instance()? == Some(request.expected_instance),
                "service_instance_changed"
            );
        }
    }
    drop(ledger);
    let record = with_lifecycle(&authority, |ledger| ledger.begin_lifecycle(&request))?;
    resume(&authority, record, input.force).await
}
async fn observation(
    root: &Path,
    approval: &Approval,
    expected_sha256: &str,
) -> Result<ServiceHealth> {
    let ControlResponse::Health { observation } = service_runtime::request_for_version(
        &layout::control(root)?,
        ControlMethod::Health,
        &approval.version,
    )
    .await?
    else {
        bail!("lifecycle_health_unavailable")
    };
    ensure!(
        observation.executable_sha256 == expected_sha256
            && observation.runtime_sha256.as_deref() == Some(approval.manifest_sha256.as_str()),
        "lifecycle_runtime_identity_mismatch"
    );
    Ok(*observation)
}
async fn resume(
    authority: &InstallerAuthority<'_>,
    mut record: LifecycleRecord,
    force: bool,
) -> Result<LifecycleRecord> {
    let root = authority.root;
    if matches!(
        record.phase,
        LifecyclePhase::Complete | LifecyclePhase::Abandoned
    ) {
        return Ok(record);
    }
    ensure!(
        owner(root)? == record.request.manager,
        "lifecycle_startup_ownership_changed"
    );
    let saved = snapshot::identity(root)?;
    ensure!(
        saved.selection.active == record.request.runtime.to_string(),
        "lifecycle_runtime_selection_changed"
    );
    let main = if record.request.action == LifecycleAction::Restart {
        verify_runtime(root, &saved.selection.active)?.0
    } else {
        saved.executable
    };
    let approval = saved.approval;
    let control = layout::control(root)?;
    let manager = record
        .request
        .manager
        .then(|| crate::service_manager::ServiceManager::for_runtime(root))
        .transpose()?;
    if record.phase == LifecyclePhase::Draining {
        match observation(root, &approval, &saved.executable_sha256).await {
            Ok(current) => {
                ensure!(
                    current.instance == record.request.expected_instance,
                    "service_instance_changed"
                );
                let result = service_runtime::request_for_version_and_instance(
                    &control,
                    ControlMethod::Shutdown,
                    &approval.version,
                    record.request.expected_instance,
                )
                .await;
                ensure!(
                    matches!(result, Ok(ControlResponse::Stopping)),
                    "lifecycle_shutdown_outcome_unknown_retry_exact_request"
                );
            }
            Err(error) => {
                if airk_store::service::ServiceLease::acquire(&control).is_err() {
                    ensure!(
                        force && manager.is_some(),
                        "lifecycle_unresponsive_requires_owned_force_stop: {error:#}"
                    );
                }
            }
        }
        let graceful = wait_quiescent(root).await;
        if graceful.is_err() {
            ensure!(
                force,
                "lifecycle_graceful_drain_incomplete_retry_or_approve_force"
            );
            let manager = manager
                .as_ref()
                .context("lifecycle_force_stop_ownership_unavailable")?;
            manager.stop()?;
            wait_quiescent(root).await?;
        } else if let Some(manager) = &manager {
            manager.stop()?;
        }
        if let Some(manager) = &manager {
            manager.require_stopped_for_maintenance()?;
        }
        let _quiescence = quiescent(root)?;
        if record.request.action == LifecycleAction::Stop {
            return with_lifecycle(authority, |ledger| {
                ledger.advance_lifecycle(&record, LifecyclePhase::Complete)
            });
        }
        replace_json(
            root,
            &format!("startup-{}.json", approval.manifest_sha256),
            &StartupAttempts::default(),
        )?;
        record = with_lifecycle(authority, |ledger| {
            ledger.advance_lifecycle(&record, LifecyclePhase::Starting)
        })?;
    }
    let healthy =
        activation::start_selected(root, &approval, &main, Some(record.request.manager)).await?;
    ensure!(
        healthy.instance != record.request.expected_instance,
        "lifecycle_predecessor_is_still_active"
    );
    let result = service_runtime::request_for_version_and_instance(
        &control,
        ControlMethod::RequestEpoch,
        &approval.version,
        healthy.instance,
    )
    .await?;
    ensure!(
        matches!(result, ControlResponse::RequestEpoch { .. }),
        "lifecycle_restarted_registry_unavailable"
    );
    with_lifecycle(authority, |ledger| {
        ledger.advance_lifecycle(&record, LifecyclePhase::Complete)
    })
}

#[cfg(test)]
mod tests;
