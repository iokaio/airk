// SPDX-License-Identifier: Apache-2.0
//! Private upgrade admission and native committed-boundary observations.
use super::*;
use crate::checkpoint_fence::{FENCE, Fence, FenceOrigin, fence};
// The packet format and the installer's fence writer/remover moved to
// `checkpoint_packet` (split step 2b); these paths keep their callers.
#[cfg(test)]
use crate::checkpoint_fence::bytes;
use crate::checkpoint_packet::immutable;
#[cfg(test)]
pub(crate) use crate::checkpoint_packet::{MAX_PACKET, arm_recovery};
pub(crate) use crate::checkpoint_packet::{
    Packet, packet_digest, packet_name, read_packet, settle,
};
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service_checkpoint::{CheckpointState, UpgradeCheckpointObservation},
};
use airk_store::service::registry::checkpoint as store;
use anyhow::Context;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

pub(crate) struct Coordinator {
    control: PathBuf,
    active: AtomicUsize,
    transition: Mutex<()>,
    observation: tokio::sync::Mutex<()>,
    closed: AtomicBool,
    cached_fence: Mutex<Option<Fence>>,
}
pub(crate) struct AdmissionGuard(Arc<Coordinator>);
impl Drop for AdmissionGuard {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::AcqRel);
    }
}
impl Coordinator {
    pub(crate) fn new(control: &Path) -> Result<Self> {
        let control = control.canonicalize()?;
        let saved = fence(&control)?;
        Ok(Self {
            control,
            active: AtomicUsize::new(0),
            transition: Mutex::new(()),
            observation: tokio::sync::Mutex::new(()),
            closed: AtomicBool::new(saved.is_some()),
            cached_fence: Mutex::new(saved),
        })
    }
    pub(crate) fn admission_closed(&self) -> Result<bool> {
        Ok(self.closed.load(Ordering::Acquire))
    }
    pub(crate) fn has_active_requests(&self) -> bool {
        self.active.load(Ordering::Acquire) != 0
    }
    pub(super) fn recovery_probe(&self, instance: Incarnation) -> Result<bool> {
        Ok(self
            .cached_fence
            .lock()
            .map_err(|_| anyhow::anyhow!("checkpoint admission state unavailable"))?
            .as_ref()
            .is_some_and(|saved| saved.service_instance != Some(instance)))
    }
    pub(super) fn validate_recovery(
        &self,
        state: &State,
        operation: Incarnation,
    ) -> Result<ControlResponse> {
        let saved = fence(&self.control)?.context("upgrade recovery fence missing")?;
        ensure!(
            saved.operation_id == operation
                && saved.service_instance != Some(state.instance)
                && saved.candidate_execution_sha256.to_string() == state.executable_sha256,
            "upgrade recovery candidate identity changed"
        );
        let snapshot = store::snapshot(&self.control)?;
        ensure!(
            snapshot.unfinished.is_empty(),
            "upgrade recovery found unresolved accepted work"
        );
        Ok(ControlResponse::UpgradeRecoveryReady {
            operation_id: operation,
            service_instance: state.instance,
            execution_sha256: state.executable_sha256.parse()?,
            namespace: snapshot.namespace,
        })
    }
    pub(crate) fn admit(self: &Arc<Self>) -> Result<AdmissionGuard> {
        ensure!(!self.admission_closed()?, "runtime_upgrade_in_progress");
        self.active.fetch_add(1, Ordering::AcqRel);
        let guard = AdmissionGuard(self.clone());
        ensure!(!self.admission_closed()?, "runtime_upgrade_in_progress");
        Ok(guard)
    }
    pub(super) async fn prepare(
        self: &Arc<Self>,
        state: Arc<State>,
        operation_id: Incarnation,
        candidate: Sha256Digest,
    ) -> Result<()> {
        let owner = self.clone();
        tokio::task::spawn_blocking(move || {
            owner.prepare_blocking(&state, operation_id, candidate)
        })
        .await??;
        Ok(())
    }
    fn prepare_blocking(
        &self,
        state: &State,
        operation_id: Incarnation,
        candidate: Sha256Digest,
    ) -> Result<()> {
        let _transition = self
            .transition
            .lock()
            .map_err(|_| anyhow::anyhow!("checkpoint transition unavailable"))?;
        let next = Fence {
            schema: 1,
            control: self.control.clone(),
            operation_id,
            service_instance: Some(state.instance),
            origin: FenceOrigin::RunningService,
            execution_sha256: state.executable_sha256.parse()?,
            candidate_execution_sha256: candidate,
        };
        if let Some(saved) = fence(&self.control)? {
            ensure!(
                serde_json::to_vec(&saved)? == serde_json::to_vec(&next)?,
                "another checkpoint operation or service owns admission"
            );
        } else {
            immutable(&self.control, FENCE, &serde_json::to_vec(&next)?)?;
        }
        *self
            .cached_fence
            .lock()
            .map_err(|_| anyhow::anyhow!("checkpoint admission state unavailable"))? = Some(next);
        self.closed.store(true, Ordering::Release);
        state
            .policy
            .lock()
            .map_err(|_| anyhow::anyhow!("service policy unavailable"))?
            .pause_evidence();
        state.sources.begin_checkpoint(operation_id)?;
        state.supervisor.begin_checkpoint();
        Ok(())
    }
    pub(super) async fn release(
        self: &Arc<Self>,
        state: Arc<State>,
        operation: Incarnation,
    ) -> Result<()> {
        let owner = self.clone();
        tokio::task::spawn_blocking(move||->Result<()> {
            let _transition=owner.transition.lock().map_err(|_|anyhow::anyhow!("checkpoint release unavailable"))?;
            if let Some(saved)=owner.cached_fence.lock().map_err(|_|anyhow::anyhow!("checkpoint admission state unavailable"))?.as_ref() {
                ensure!(saved.operation_id==operation,"another checkpoint owns admission release");
                ensure!(saved.service_instance!=Some(state.instance),"checkpoint-prepared owner must restart before reopening sealed source and provider admission");
            }
            let current=std::env::current_exe()?.canonicalize()?;
            let payload=crate::runtime_pair::payload_root(&current)?;
            let identity=payload.file_name().and_then(|name|name.to_str()).context("admission release runtime identity missing")?;
            let runtimes=payload.parent().context("admission release runtime parent missing")?;
            ensure!(runtimes.file_name().is_some_and(|name|name=="runtimes"),"admission release is not an installed runtime");
            let install=runtimes.parent().context("admission release install root missing")?;
            crate::runtime_install::verify_admission_release(install,&owner.control,operation,identity)?;
            settle(&owner.control,operation)?;
            *owner.cached_fence.lock().map_err(|_|anyhow::anyhow!("checkpoint admission state unavailable"))?=None;
            owner.closed.store(false,Ordering::Release);
            drop(state); // Retain the real native owner through durable release.
            Ok(())
        }).await??;
        Ok(())
    }
    pub(super) fn prepared(
        &self,
        state: &State,
        operation_id: Incarnation,
    ) -> Result<UpgradeCheckpointObservation> {
        let saved = fence(&self.control)?.context("checkpoint fence missing")?;
        ensure!(
            saved.operation_id == operation_id && saved.service_instance == Some(state.instance),
            "checkpoint preparation owner changed"
        );
        Ok(UpgradeCheckpointObservation {
            schema: 1,
            operation_id,
            service_instance: state.instance,
            execution_sha256: saved.execution_sha256,
            candidate_execution_sha256: saved.candidate_execution_sha256,
            state: CheckpointState::Pending,
            pending_operations: None,
            sealed_workers: 0,
            state_sha256: None,
            native_packet_sha256: None,
            provider: None,
            blocked_code: Some("checkpoint_prepared".into()),
            blocked_details: None,
        })
    }
    pub(super) async fn observe(
        &self,
        state: &State,
        operation_id: Incarnation,
    ) -> Result<UpgradeCheckpointObservation> {
        let _observation = self.observation.lock().await;
        let saved = fence(&self.control)?.context("checkpoint has not been prepared")?;
        ensure!(
            saved.operation_id == operation_id
                && saved.service_instance == Some(state.instance)
                && saved.execution_sha256.to_string() == state.executable_sha256,
            "checkpoint observation crossed its service owner"
        );
        let mut observation = UpgradeCheckpointObservation {
            schema: 1,
            operation_id,
            service_instance: state.instance,
            execution_sha256: saved.execution_sha256,
            candidate_execution_sha256: saved.candidate_execution_sha256,
            state: CheckpointState::Pending,
            pending_operations: None,
            sealed_workers: 0,
            state_sha256: None,
            native_packet_sha256: None,
            provider: None,
            blocked_code: None,
            blocked_details: None,
        };
        let packet_path = self.control.join(packet_name(operation_id));
        if packet_path.try_exists()? {
            let digest = packet_digest(&self.control, operation_id)?;
            let packet = read_packet(&self.control, operation_id, digest)?;
            ensure!(
                packet.service_instance == state.instance
                    && packet.execution_sha256 == saved.execution_sha256
                    && packet.candidate_execution_sha256 == saved.candidate_execution_sha256,
                "saved checkpoint packet changed service binding"
            );
            observation.state = CheckpointState::Ready;
            observation.pending_operations = Some(0);
            observation.state_sha256 = Some(packet.state.digest()?);
            observation.native_packet_sha256 = Some(digest);
            observation.sealed_workers = packet.workers.len();
            observation.provider = Some(packet.provider);
            observation.validate()?;
            return Ok(observation);
        }
        let active_requests = self.active.load(Ordering::Acquire);
        let management_can_idle_exit = state.management.can_idle_exit();
        let source_admission_observed = state.sources.checkpoint_admission_observed();
        if active_requests != 0 || !management_can_idle_exit || !source_admission_observed {
            observation.blocked_code =
                Some("accepted_metadata_or_source_coordination_pending".into());
            observation.blocked_details = Some(
                serde_json::json!({
                    "active_requests": active_requests,
                    "management_can_idle_exit": management_can_idle_exit,
                    "source_admission_observed": source_admission_observed,
                })
                .to_string(),
            );
            return Ok(observation);
        }
        let workers = match state
            .supervisor
            .observe_checkpoint(operation_id, false)
            .await
        {
            Ok(workers) => workers,
            Err(error) if error.is::<crate::project_supervisor::CheckpointResourcesPending>() => {
                observation.blocked_code = Some("checkpoint_predecessor_resources_pending".into());
                return Ok(observation);
            }
            Err(error) => return Err(error),
        };
        if workers.iter().any(|worker| !worker.durable_boundary) {
            observation.blocked_code = Some("accepted_native_work_pending".into());
            return Ok(observation);
        }
        let snapshot = match store::snapshot(&self.control) {
            Ok(snapshot) => snapshot,
            Err(error) if store::is_read_contention(&error) => {
                observation.blocked_code = Some("checkpoint_committed_state_read_busy".into());
                return Ok(observation);
            }
            Err(error) => return Err(error),
        };
        observation.pending_operations = Some(snapshot.unfinished.len());
        if !snapshot.unfinished.is_empty() {
            observation.blocked_code = Some("accepted_native_work_pending".into());
            return Ok(observation);
        }
        state.sources.seal_checkpoint(operation_id)?;
        let source = match state.sources.checkpoint_result() {
            Ok(Some(source)) if state.sources.checkpoint_observed() => source,
            Ok(_) => {
                observation.blocked_code = Some("native_source_checkpoint_barrier_pending".into());
                return Ok(observation);
            }
            Err(error) => {
                observation.blocked_code = Some("native_source_checkpoint_barrier_failed".into());
                observation.blocked_details =
                    Some(format!("{error:#}").chars().take(2048).collect());
                return Ok(observation);
            }
        };
        source.validate()?;
        ensure!(
            source.operation_id == operation_id && source.service == state.instance,
            "source checkpoint receipt crossed owner"
        );
        state.management.begin_checkpoint_freeze();
        if !state.management.checkpoint_observed() {
            observation.blocked_code = Some("native_metadata_checkpoint_ack_pending".into());
            return Ok(observation);
        }
        let workers = state
            .supervisor
            .observe_checkpoint(operation_id, true)
            .await?;
        if workers
            .iter()
            .any(|worker| !worker.sealed || !worker.durable_boundary)
        {
            observation.blocked_code = Some("native_worker_checkpoint_ack_pending".into());
            return Ok(observation);
        }
        observation.sealed_workers = workers.len();
        let identities = workers.iter().map(|worker| worker.identity).collect();
        let Some(provider) = state
            .supervisor
            .observe_inference_checkpoint(operation_id, identities)
            .await?
        else {
            observation.blocked_code = Some("native_inference_completion_pending".into());
            return Ok(observation);
        };
        provider.validate()?;
        ensure!(
            provider.captured_workers
                == workers
                    .iter()
                    .map(|worker| worker.identity)
                    .collect::<Vec<_>>(),
            "native inference proof crossed sealed worker roster"
        );
        if provider.state != airk_core::inference_provider::CompletionState::Complete {
            observation.provider = Some(provider);
            observation.blocked_code = Some("native_inference_completion_pending".into());
            return Ok(observation);
        }
        let snapshot = store::snapshot(&self.control)?;
        ensure!(
            snapshot.unfinished.is_empty(),
            "new durable work appeared after worker checkpoint seal"
        );
        let state_digest = snapshot.digest()?;
        let packet = Packet {
            schema: 1,
            captured_unix_ms: u64::try_from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_millis(),
            )?,
            operation_id,
            service_instance: state.instance,
            execution_sha256: saved.execution_sha256,
            candidate_execution_sha256: saved.candidate_execution_sha256,
            state: snapshot,
            workers,
            source,
            provider: provider.clone(),
        };
        let packet_bytes = serde_json::to_vec(&packet)?;
        immutable(&self.control, &packet_name(operation_id), &packet_bytes)?;
        observation.state = CheckpointState::Ready;
        observation.state_sha256 = Some(state_digest);
        observation.native_packet_sha256 = Some(packet_digest(&self.control, operation_id)?);
        observation.provider = Some(provider);
        observation.validate()?;
        Ok(observation)
    }
}

#[cfg(test)]
mod tests;
