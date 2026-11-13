// SPDX-License-Identifier: Apache-2.0
//! Exact native checkpoint/exclusion for a journal-owned same-image issuer change.
use super::*;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service_checkpoint::UpgradeCheckpointObservation,
};
use airk_store::service::registry::checkpoint as store;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StopBoundary {
    GracefulNativeCheckpoint,
    NoRunningServiceCommittedStoresAfterInterruption,
    CommittedSuccessorNativeExclusion,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StopProof {
    schema: u32,
    operation: Incarnation,
    selected_runtime: String,
    expected_instance: Incarnation,
    approved_execution_sha256: Sha256Digest,
    boundary: StopBoundary,
    checkpoint: Option<UpgradeCheckpointObservation>,
    canonical_state_sha256: Sha256Digest,
    manager_owned: bool,
    manager_definition_sha256: Sha256Digest,
    native_owner_exclusion: bool,
    // A ServiceLease excludes the actual listener/model/source/write owners.
    // It is not a fabricated cross-platform OS process-reap observation.
    independent_os_process_reap: bool,
}

pub(super) struct StoppedOwner {
    root: PathBuf,
    control: PathBuf,
    main: PathBuf,
    approval: Approval,
    selected: Selection,
    proof: StopProof,
    exclusion: Quiescence,
}
impl StoppedOwner {
    pub(super) fn service_lease(&self) -> &airk_store::service::ServiceLease {
        &self.exclusion._service
    }
    pub(super) fn control(&self) -> &Path {
        &self.control
    }
    pub(super) fn proof(&self) -> &StopProof {
        &self.proof
    }
}

fn plan_digest(manager: &crate::service_manager::ServiceManager) -> Result<Sha256Digest> {
    Ok(Sha256Digest::from_bytes(
        Sha256::digest(serde_json::to_vec(&manager.plan()?)?).into(),
    ))
}

fn require_recorded_owner(control: &Path, expected: Incarnation) -> Result<()> {
    ensure!(
        airk_store::service::registry::ServiceRegistry::open_saved(control)?
            .recorded_service_instance()?
            == Some(expected),
        "issuer rotation recorded service owner changed; preserve the existing state"
    );
    Ok(())
}

/// A recorded incarnation alone is not approved-image proof. An interrupted
/// committed replacement needs its own protected native endpoint and actual
/// exclusion; it must never be relabeled as the original checkpointed owner.
fn committed_successor(
    control: &Path,
    record: &crate::http_endpoint::rotation::Record,
    exclusion: &airk_store::service::ServiceLease,
) -> Result<Option<Incarnation>> {
    ensure!(
        exclusion.root().canonicalize()? == control.canonicalize()?,
        "issuer successor exclusion crossed control root"
    );
    crate::http_endpoint::rotation::require_committed_restart(
        control,
        record.operation_id,
        record.old_endpoint.executable_sha256,
    )?;
    let recorded = airk_store::service::registry::ServiceRegistry::open_saved(control)?
        .recorded_service_instance()?
        .context("issuer successor recorded owner unavailable")?;
    if recorded == record.old_endpoint.service_instance {
        return Ok(None);
    }
    let endpoint = crate::http_endpoint::load_descriptor(&control.join("http-1"), &record.binding())?
        .context("committed successor has no protected endpoint; explicit Start and authenticated review are required")?;
    ensure!(
        endpoint.service_instance == recorded
            && endpoint.service_instance != record.old_endpoint.service_instance
            && endpoint.ca_sha256 == record.new_ca_sha256
            && endpoint.executable_sha256 == record.old_endpoint.executable_sha256
            && endpoint.generation > record.old_endpoint.generation,
        "recorded issuer successor lacks exact committed endpoint proof; explicit Start and authenticated review are required"
    );
    Ok(Some(recorded))
}

/// Resume only an absent, evidenced committed successor. Its committed work may
/// differ from the OLD Ready packet; preserve that packet and capture current
/// same-engine state under the new native exclusion instead.
pub(super) async fn recover_committed_successor(
    root: &Path,
    record: &crate::http_endpoint::rotation::Record,
) -> Result<Option<ServiceHealth>> {
    let root = root.canonicalize()?;
    managed::require_unmanaged(&root)?;
    ensure!(
        activation::pending(&root)?.is_none(),
        "pending activation owns runtime maintenance"
    );
    lifecycle::require_settled(&root)?;
    lifecycle::require_start_allowed(&root)?;
    let control = layout::control(&root)?;
    let exclusion = quiescent(&root)?;
    let Some(successor) = committed_successor(&control, record, &exclusion._service)? else {
        return Ok(None);
    };
    let selected = selection_record(&root)?;
    let (main, approval) = verify_runtime(&root, &selected.active)?;
    let original: StopProof = serde_json::from_value(
        record
            .stop_proof
            .clone()
            .context("issuer original stop proof missing")?,
    )?;
    ensure!(
        original.schema == 1
            && original.operation == record.operation_id
            && original.expected_instance == record.old_endpoint.service_instance
            && original.approved_execution_sha256 == record.old_endpoint.executable_sha256
            && original.selected_runtime == selected.active
            && original.native_owner_exclusion
            && runtime_pair::single_executable(&main)?
            && runtime_pair::digest_file(&main)?
                == record.old_endpoint.executable_sha256.to_string(),
        "issuer committed successor changed its approved image or original stop proof"
    );
    let manager = crate::service_manager::ServiceManager::for_runtime(&root)?;
    ensure!(
        manager.owns_restart()? == original.manager_owned
            && plan_digest(&manager)? == original.manager_definition_sha256,
        "issuer committed successor manager policy changed"
    );
    let engine = runtime_pair::engine_identity_at_version(&main, &approval.version)?;
    store::snapshot(&control)?.require_compatible(Some(engine), engine)?;
    // Existing foreign fences fail before any manager action. This recovery
    // anchor makes no historical Ready/service-incarnation assertion.
    crate::checkpoint_packet::arm_recovery(
        &control,
        record.operation_id,
        record.old_endpoint.executable_sha256,
    )?;
    if original.manager_owned {
        manager.stop()?;
    }
    manager.require_stopped_for_maintenance()?;
    require_recorded_owner(&control, successor)?;
    let proof = StopProof {
        schema: 1,
        operation: record.operation_id,
        selected_runtime: selected.active.clone(),
        expected_instance: successor,
        approved_execution_sha256: record.old_endpoint.executable_sha256,
        boundary: StopBoundary::CommittedSuccessorNativeExclusion,
        checkpoint: None,
        canonical_state_sha256: store::snapshot(&control)?.digest()?,
        manager_owned: original.manager_owned,
        manager_definition_sha256: original.manager_definition_sha256,
        native_owner_exclusion: true,
        independent_os_process_reap: false,
    };
    let owner = StoppedOwner {
        root,
        control,
        main,
        approval,
        selected,
        proof,
        exclusion,
    };
    start_after_commit(owner).await.map(Some)
}

/// Caller holds the original installer lock and accepted issuer journal. A
/// refusal never starts a service, invents an activation, or forces a worker.
pub(super) async fn prepare_stop(
    root: &Path,
    operation: Incarnation,
    expected_instance: Incarnation,
    expected_execution: Sha256Digest,
) -> Result<StoppedOwner> {
    let root = root.canonicalize()?;
    managed::require_unmanaged(&root)?;
    ensure!(
        activation::pending(&root)?.is_none(),
        "pending activation owns runtime maintenance"
    );
    lifecycle::require_settled(&root)?;
    lifecycle::require_start_allowed(&root)?;
    let control = layout::control(&root)?;
    crate::http_endpoint::rotation::require_stop_owner(
        &control,
        operation,
        expected_instance,
        expected_execution,
    )?;
    let selected = selection_record(&root)?;
    let (main, approval) = verify_runtime(&root, &selected.active)?;
    ensure!(
        runtime_pair::single_executable(&main)?
            && runtime_pair::digest_file(&main)? == expected_execution.to_string(),
        "issuer rotation approved main changed"
    );
    let manager = crate::service_manager::ServiceManager::for_runtime(&root)?;
    let manager_owned = manager.owns_restart()?;
    let manager_definition_sha256 = plan_digest(&manager)?;
    store::preflight_capacity(&control)?;
    let mut checkpoint = match health(&root, &approval, &expected_execution.to_string()).await {
        Ok(observed) => {
            ensure!(
                observed.instance == expected_instance,
                "issuer rotation service instance changed; preserve the live owner"
            );
            Some(
                activation::prepare_native_checkpoint(
                    &control,
                    operation,
                    &approval.version,
                    &observed,
                    expected_execution,
                )
                .await?,
            )
        }
        Err(error) => {
            // A damaged/unavailable health endpoint does not prove exit. Only
            // acquisition of every actual native owner below can qualify this
            // explicit interrupted-operation recovery boundary.
            let exclusion = quiescent(&root).context(error)?;
            require_recorded_owner(&control, expected_instance)?;
            // Bind/refuse the exact operation while native exclusion is held,
            // before unregistering any OS manager. A foreign saved fence is
            // never cleared or bypassed by interrupted-rotation recovery.
            crate::checkpoint_packet::arm_recovery(&control, operation, expected_execution)?;
            drop(exclusion);
            None
        }
    };
    ensure!(
        manager.owns_restart()? == manager_owned
            && plan_digest(&manager)? == manager_definition_sha256,
        "issuer rotation captured manager changed"
    );
    if manager_owned {
        manager.stop()?;
    }
    drain(&root, &main, &approval).await?;
    manager.require_stopped_for_maintenance()?;
    let exclusion = quiescent(&root)?;
    ensure!(
        selection_record(&root)? == selected
            && runtime_pair::digest_file(&main)? == expected_execution.to_string(),
        "issuer rotation selection changed before native exclusion"
    );
    let snapshot = store::snapshot(&control)?;
    require_recorded_owner(&control, expected_instance)?;
    let engine = runtime_pair::engine_identity_at_version(&main, &approval.version)?;
    snapshot.require_compatible(Some(engine), engine)?;
    let state_sha = snapshot.digest()?;
    let packet_path = control.join(crate::checkpoint_packet::packet_name(operation));
    if packet_path.try_exists()? {
        let packet_sha = crate::checkpoint_packet::packet_digest(&control, operation)?;
        let packet = crate::checkpoint_packet::read_packet(&control, operation, packet_sha)?;
        ensure!(
            packet.service_instance == expected_instance
                && packet.execution_sha256 == expected_execution
                && packet.candidate_execution_sha256 == expected_execution
                && packet.state.digest()? == state_sha,
            "issuer rotation retained checkpoint crossed its native owner/state"
        );
        if let Some(observed) = &checkpoint {
            ensure!(
                observed.native_packet_sha256 == Some(packet_sha)
                    && observed.state_sha256 == Some(state_sha),
                "issuer rotation live checkpoint packet changed"
            );
        } else {
            let observed = UpgradeCheckpointObservation {
                schema: 1,
                operation_id: operation,
                service_instance: packet.service_instance,
                execution_sha256: packet.execution_sha256,
                candidate_execution_sha256: packet.candidate_execution_sha256,
                state: airk_core::service_checkpoint::CheckpointState::Ready,
                pending_operations: Some(0),
                sealed_workers: packet.workers.len(),
                state_sha256: Some(state_sha),
                native_packet_sha256: Some(packet_sha),
                provider: Some(packet.provider),
                blocked_code: None,
                blocked_details: None,
            };
            observed.validate()?;
            checkpoint = Some(observed);
        }
    } else {
        ensure!(
            checkpoint.is_none(),
            "issuer rotation native Ready packet disappeared"
        );
    }
    crate::checkpoint_packet::arm_recovery(&control, operation, expected_execution)?;
    let proof = StopProof {
        schema: 1,
        operation,
        selected_runtime: selected.active.clone(),
        expected_instance,
        approved_execution_sha256: expected_execution,
        boundary: if checkpoint.is_some() {
            StopBoundary::GracefulNativeCheckpoint
        } else {
            StopBoundary::NoRunningServiceCommittedStoresAfterInterruption
        },
        checkpoint,
        canonical_state_sha256: state_sha,
        manager_owned,
        manager_definition_sha256,
        native_owner_exclusion: true,
        independent_os_process_reap: false,
    };
    Ok(StoppedOwner {
        root,
        control,
        main,
        approval,
        selected,
        proof,
        exclusion,
    })
}

/// No old sealed owner is reopened. The accepted journal must already bind the
/// durably published new TLS issuer before its exact fence can be removed.
pub(super) async fn start_after_commit(owner: StoppedOwner) -> Result<ServiceHealth> {
    let StoppedOwner {
        root,
        control,
        main,
        approval,
        selected,
        proof,
        exclusion,
    } = owner;
    crate::http_endpoint::rotation::require_committed_restart(
        &control,
        proof.operation,
        proof.approved_execution_sha256,
    )?;
    ensure!(
        selection_record(&root)? == selected
            && runtime_pair::digest_file(&main)? == proof.approved_execution_sha256.to_string(),
        "issuer rotation selected image changed before restart"
    );
    let manager = crate::service_manager::ServiceManager::for_runtime(&root)?;
    ensure!(
        manager.owns_restart()? == proof.manager_owned
            && plan_digest(&manager)? == proof.manager_definition_sha256,
        "issuer rotation manager changed before restart"
    );
    require_recorded_owner(&control, proof.expected_instance)?;
    ensure!(
        store::snapshot(&control)?.digest()? == proof.canonical_state_sha256,
        "issuer rotation changed canonical work while native ownership was excluded"
    );
    crate::checkpoint_packet::settle(&control, proof.operation)?;
    drop(exclusion);
    let observed =
        activation::start_selected(&root, &approval, &main, Some(proof.manager_owned)).await?;
    ensure!(
        observed.instance != proof.expected_instance
            && observed.executable_sha256 == proof.approved_execution_sha256.to_string(),
        "issuer rotation did not start a fresh exact service owner"
    );
    Ok(observed)
}

#[cfg(test)]
mod tests;
