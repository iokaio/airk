// SPDX-License-Identifier: Apache-2.0
//! Durable typed lifecycle receipts in the native registry transaction.
use super::NativeServiceRegistry;
use airk_core::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    service_lifecycle::{LifecycleAction, LifecyclePhase, LifecycleRecord, LifecycleRequest},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LifecycleState {
    stopped: bool,
    instance: Option<Incarnation>,
    sequence: u64,
    records: BTreeMap<String, (u64, LifecycleRecord)>,
}
fn id(request: &LifecycleRequest) -> Sha256Digest {
    framed_digest(
        b"airk/lifecycle-request/1",
        &[
            request.key.namespace.as_bytes(),
            &request.key.epoch.to_be_bytes(),
            request.key.nonce.as_bytes(),
            request.caller.as_bytes(),
        ],
    )
}
fn pending(phase: LifecyclePhase) -> bool {
    matches!(phase, LifecyclePhase::Draining | LifecyclePhase::Starting)
}
impl LifecycleState {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(self.records.len() <= 64, "lifecycle receipt bound exceeded");
        ensure!(
            self.records
                .values()
                .filter(|(_, record)| pending(record.phase))
                .count()
                <= 1,
            "multiple active lifecycle receipts"
        );
        for (key, (sequence, record)) in &self.records {
            ensure!(
                key == &record.id.to_string()
                    && record.id == id(&record.request)
                    && *sequence <= self.sequence
                    && serde_json::to_vec(&record.request)?.len() <= 2048,
                "invalid lifecycle receipt"
            );
        }
        Ok(())
    }
}
impl NativeServiceRegistry {
    pub fn lifecycle_by_id(&self, identity: Sha256Digest) -> Result<Option<LifecycleRecord>> {
        self.read(|state| {
            Ok(state
                .lifecycle
                .records
                .get(&identity.to_string())
                .map(|(_, record)| record.clone()))
        })
    }
    pub fn lifecycle_record(&self, request: &LifecycleRequest) -> Result<Option<LifecycleRecord>> {
        let record = self.lifecycle_by_id(id(request))?;
        ensure!(
            record
                .as_ref()
                .is_none_or(|record| record.request == *request),
            "lifecycle_request_key_conflict"
        );
        Ok(record)
    }
    pub fn pending_lifecycle(&self) -> Result<Option<LifecycleRecord>> {
        self.read(|state| {
            Ok(state
                .lifecycle
                .records
                .values()
                .find(|(_, record)| pending(record.phase))
                .map(|(_, record)| record.clone()))
        })
    }
    pub fn record_service_instance(
        &mut self,
        lease: &crate::service::ServiceLease,
        instance: Incarnation,
    ) -> Result<()> {
        ensure!(
            lease.root() == self.root,
            "service instance ownership differs"
        );
        self.write(|state| {
            ensure!(!state.lifecycle.stopped, "service_explicitly_stopped");
            state.lifecycle.instance = Some(instance);
            state.bump()
        })
    }
    pub fn recorded_service_instance(&self) -> Result<Option<Incarnation>> {
        self.read(|state| Ok(state.lifecycle.instance))
    }
    pub fn begin_lifecycle(&mut self, request: &LifecycleRequest) -> Result<LifecycleRecord> {
        let identity = id(request);
        ensure!(
            serde_json::to_vec(request)?.len() <= 2048,
            "lifecycle request exceeds budget"
        );
        self.write(|state| {
            // Retained exact receipts remain replayable after epoch rotation.
            if let Some((_, record)) = state.lifecycle.records.get(&identity.to_string()) {
                ensure!(record.request == *request, "lifecycle_request_key_conflict");
                return Ok(record.clone());
            }
            ensure!(
                request.key.namespace == state.request_namespace
                    && request.key.epoch == state.request_epoch,
                "lifecycle_request_key_expired"
            );
            ensure!(
                !state
                    .lifecycle
                    .records
                    .values()
                    .any(|(_, record)| pending(record.phase)),
                "another_lifecycle_request_is_pending"
            );
            if state.lifecycle.records.len() >= 64 {
                let mut completed: Vec<_> = state
                    .lifecycle
                    .records
                    .iter()
                    .filter(|(_, (_, record))| !pending(record.phase))
                    .map(|(key, (sequence, _))| (*sequence, key.clone()))
                    .collect();
                completed.sort_unstable();
                for (_, key) in completed.into_iter().take(32) {
                    state.lifecycle.records.remove(&key);
                }
                state.request_epoch = state
                    .request_epoch
                    .checked_add(1)
                    .context("service request epoch exhausted")?;
            }
            state.lifecycle.sequence = state
                .lifecycle
                .sequence
                .checked_add(1)
                .context("lifecycle sequence exhausted")?;
            let record = LifecycleRecord {
                id: identity,
                request: request.clone(),
                phase: LifecyclePhase::Draining,
            };
            state.lifecycle.records.insert(
                identity.to_string(),
                (state.lifecycle.sequence, record.clone()),
            );
            state.lifecycle.stopped = true;
            state.bump()?;
            Ok(record)
        })
    }
    pub fn advance_lifecycle(
        &mut self,
        record: &LifecycleRecord,
        next: LifecyclePhase,
    ) -> Result<LifecycleRecord> {
        self.write(|state| {
            let (_, current) = state
                .lifecycle
                .records
                .get_mut(&record.id.to_string())
                .context("unknown lifecycle receipt")?;
            ensure!(
                current.request == record.request,
                "lifecycle request changed"
            );
            if current.phase == next {
                return Ok(current.clone());
            }
            ensure!(current.phase == record.phase, "lifecycle phase changed");
            let valid = matches!(
                (current.request.action, current.phase, next),
                (
                    LifecycleAction::Stop,
                    LifecyclePhase::Draining,
                    LifecyclePhase::Complete
                ) | (
                    LifecycleAction::Restart,
                    LifecyclePhase::Draining,
                    LifecyclePhase::Starting
                ) | (
                    LifecycleAction::Restart,
                    LifecyclePhase::Starting,
                    LifecyclePhase::Complete
                )
            );
            let abandon = next == LifecyclePhase::Abandoned && pending(current.phase);
            ensure!(valid || abandon, "invalid lifecycle transition");
            current.phase = next;
            let result = current.clone();
            if abandon {
                state.lifecycle.stopped = true;
            } else if current.request.action == LifecycleAction::Restart {
                state.lifecycle.stopped = false;
            }
            state.bump()?;
            Ok(result)
        })
    }
    pub fn service_explicitly_stopped(&self) -> Result<bool> {
        self.read(|state| Ok(state.lifecycle.stopped))
    }
    pub fn clear_service_stop(&mut self) -> Result<()> {
        self.write(|state| {
            ensure!(
                !state
                    .lifecycle
                    .records
                    .values()
                    .any(|(_, record)| pending(record.phase)),
                "resume pending lifecycle before explicit start"
            );
            state.lifecycle.stopped = false;
            state.bump()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::service::RequestKey;
    fn request(
        registry: &mut NativeServiceRegistry,
        nonce: u8,
        action: LifecycleAction,
    ) -> Result<LifecycleRequest> {
        let (namespace, epoch) = registry.request_epoch()?;
        Ok(LifecycleRequest {
            caller: Incarnation::from_bytes([2; 16]),
            key: RequestKey {
                namespace,
                epoch,
                nonce: Incarnation::from_bytes([nonce; 16]),
            },
            action,
            expected_instance: Incarnation::from_bytes([3; 16]),
            runtime: Sha256Digest::from_bytes([4; 32]),
            manager: false,
        })
    }
    #[test]
    fn restart_and_explicit_stop_survive_reopen_and_abandoned_work_stays_stopped() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut registry =
            NativeServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
        let input = request(&mut registry, 1, LifecycleAction::Restart)?;
        let accepted = registry.begin_lifecycle(&input)?;
        assert!(registry.service_explicitly_stopped()?);
        assert!(registry.clear_service_stop().is_err());
        assert!(
            registry
                .advance_lifecycle(&accepted, LifecyclePhase::Complete)
                .is_err()
        );
        let starting = registry.advance_lifecycle(&accepted, LifecyclePhase::Starting)?;
        assert!(!registry.service_explicitly_stopped()?);
        drop(registry);
        let mut registry =
            NativeServiceRegistry::open(root.path(), Incarnation::from_bytes([9; 16]))?;
        assert_eq!(registry.incarnation(), input.key.namespace);
        assert_eq!(registry.pending_lifecycle()?.unwrap().id, accepted.id);
        registry.advance_lifecycle(&starting, LifecyclePhase::Abandoned)?;
        assert!(registry.service_explicitly_stopped()?);
        assert_eq!(
            registry.begin_lifecycle(&input)?.phase,
            LifecyclePhase::Abandoned
        );
        assert!(
            registry
                .advance_lifecycle(&starting, LifecyclePhase::Starting)
                .is_err()
        );
        registry.clear_service_stop()?;
        Ok(())
    }
    #[test]
    fn lifecycle_retention_rotates_unknown_keys_and_keeps_exact_pending_receipts() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut registry =
            NativeServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
        let first = request(&mut registry, 0, LifecycleAction::Stop)?;
        for nonce in 0..70 {
            let input = request(&mut registry, nonce, LifecycleAction::Stop)?;
            let accepted = registry.begin_lifecycle(&input)?;
            registry.advance_lifecycle(&accepted, LifecyclePhase::Complete)?;
        }
        assert!(registry.lifecycle_record(&first)?.is_none());
        assert!(registry.begin_lifecycle(&first).is_err());
        let input = request(&mut registry, 71, LifecycleAction::Stop)?;
        let accepted = registry.begin_lifecycle(&input)?;
        let mut conflict = input.clone();
        conflict.action = LifecycleAction::Restart;
        assert!(registry.begin_lifecycle(&conflict).is_err());
        assert_eq!(registry.begin_lifecycle(&input)?.id, accepted.id);
        assert!(registry.read(|state| Ok(state.lifecycle.records.len()))? <= 64);
        Ok(())
    }
}
