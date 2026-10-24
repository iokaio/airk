// SPDX-License-Identifier: Apache-2.0
//! Read-only project observations; cached snapshots never authorize maintenance.
use crate::Kernel;
use airk_core::{
    service::ProjectBinding,
    service_inspection::{
        DiscoverySummary, IndexState, InspectionFailure, InspectionSnapshot, SemanticSummary,
        SourceState,
    },
};
use anyhow::{Context, Result, ensure};
use sha2::Digest;

#[derive(Debug)]
pub struct InspectionChanged;
impl std::fmt::Display for InspectionChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("project settings changed during inspection; refresh its observation")
    }
}
impl std::error::Error for InspectionChanged {}

impl Kernel {
    pub fn service_storage_summary(&self) -> Result<airk_core::service_inspection::StorageSummary> {
        let quota = self.collection_quota()?;
        let status = quota.status()?;
        let collection = status.collections.get(&self.collection);
        let used = collection.map_or(0, |entry| entry.used_bytes);
        let reserved = collection.map_or(0, |entry| entry.reserved_bytes);
        let policy = quota.policy()?;
        let limit = collection.map_or_else(
            || {
                policy
                    .collections
                    .get(&self.collection)
                    .copied()
                    .unwrap_or(policy.default_collection_bytes)
            },
            |entry| entry.limit_bytes,
        );
        // A comparable existing incarnation is the only measured corpus size.
        // The estimate deliberately does not claim to bound future source growth.
        let replacement_estimate_bytes = used
            .checked_add((64 << 20) + (128 << 10) + (192 << 10))
            .context("replacement storage estimate overflow")?;
        Ok(airk_core::service_inspection::StorageSummary {
            collection_used_bytes: used,
            collection_reserved_bytes: reserved,
            collection_limit_bytes: limit,
            root_available_bytes: status
                .root_limit_bytes
                .saturating_sub(status.used_bytes)
                .saturating_sub(status.reserved_bytes),
            physical_available_growth_bytes: status.physical_capacity.available_growth_bytes,
            replacement_estimate_bytes,
        })
    }

    pub fn inspect_service(&self, project: ProjectBinding) -> Result<InspectionSnapshot> {
        crate::service_execution::checkpoint()?;
        ensure!(
            self.service_collection.is_some(),
            "service collection required"
        );
        let selected = self.refresh_service_snapshot()?;
        let expected = airk_core::service::ProjectRegistration::new(
            selected.root_id(),
            selected
                .data_root
                .to_str()
                .context("non-UTF-8 service data root")?
                .into(),
            selected.collection.clone(),
        )?;
        ensure!(
            expected.binding == project,
            "inspection registration differs from selected project"
        );
        let incarnation = selected
            .store
            .service_incarnation()?
            .context("service incarnation missing")?;
        let settings =
            airk_store::service::collection::settings_digest(&selected.store, &selected.root_id());
        let recovery = selected.service_recovery_required();
        let status = selected.status();
        let after =
            airk_store::service::collection::settings_digest(&selected.store, &selected.root_id());
        ensure!(
            settings.as_ref().ok() == after.as_ref().ok(),
            InspectionChanged
        );
        crate::service_execution::progress_verified(1);
        let mut snapshot = InspectionSnapshot {
            project,
            incarnation,
            publication: None,
            settings_digest: settings.as_ref().ok().copied(),
            quota_policy_digest: selected
                .quota_policy()
                .and_then(|policy| {
                    Ok(airk_core::identity::Sha256Digest::from_bytes(
                        sha2::Sha256::digest(serde_json::to_vec(&policy)?).into(),
                    ))
                })
                .ok(),
            index_state: IndexState::Unavailable,
            source_state: match selected
                .service_source_observation()
                .map(|observation| observation.state)
            {
                Some(crate::service_projects::SourceState::Available) => SourceState::Available,
                Some(crate::service_projects::SourceState::Unavailable) => SourceState::Unavailable,
                _ => SourceState::Unknown,
            },
            recovery_required: recovery.as_ref().ok().copied(),
            indexed_files: None,
            indexed_vectors: None,
            vectors_enabled: None,
            semantic: None,
            discovery: None,
            failure: Some(InspectionFailure::IndexObservationFailed),
            storage: selected.service_storage_summary().ok(),
        };
        if let Ok(status) = status {
            snapshot.publication = status.publication.map(|revision| revision.token);
            snapshot.indexed_files = Some(status.files.try_into()?);
            snapshot.indexed_vectors = Some(status.indexed_vectors.try_into()?);
            snapshot.vectors_enabled = Some(status.vectors_enabled);
            snapshot.semantic = Some(SemanticSummary {
                state: status.semantic_readiness.state,
                selected_artifacts: status.semantic_readiness.selected_artifacts,
                completed_artifacts: status.semantic_readiness.completed_artifacts,
                selected_units: status.semantic_readiness.selected_units,
                completed_units: status.semantic_readiness.completed_units,
                failed: status.semantic_readiness.failure.is_some(),
            });
            snapshot.discovery = Some(DiscoverySummary {
                state: status.discovery_readiness.state,
                processed_artifacts: status.discovery_readiness.processed_artifacts,
                committed_artifacts: status.discovery_readiness.committed_artifacts,
                total_artifacts: status.discovery_readiness.total_artifacts,
                failed: status.discovery_readiness.failure.is_some(),
            });
            snapshot.index_state = if snapshot.publication.is_some() {
                IndexState::Ready
            } else {
                IndexState::Empty
            };
            snapshot.failure = None;
        } else if snapshot.recovery_required == Some(true) {
            snapshot.index_state = IndexState::RecoveryRequired;
            snapshot.failure = Some(InspectionFailure::IndexRequiresRecovery);
        }
        if settings.is_err() || recovery.is_err() {
            snapshot.index_state = IndexState::Unavailable;
            snapshot.failure = Some(if settings.is_err() {
                InspectionFailure::SettingsObservationFailed
            } else {
                InspectionFailure::RecoveryObservationFailed
            });
        }
        snapshot.validate()?;
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspection_guards_bind_settings_and_revision_without_capturing_sources_or_installing_models()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let source = owner.path().join("source");
        let data = owner.path().join("data");
        std::fs::create_dir(&source)?;
        std::fs::create_dir(&data)?;
        let bytes = b"# Fictional copper harbor\nOriginal private fixture evidence.\n";
        std::fs::write(source.join("memo.md"), bytes)?;
        let kernel = Kernel::open_service_collection(&source, Some(data.clone()), "fixture")?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let project = crate::service_projects::registration(&source, &data, "fixture")?;
        let first = kernel.inspect_service(project.binding)?;
        assert_eq!(first.index_state, IndexState::Ready);
        assert_eq!(first.source_state, SourceState::Available);
        assert_eq!(
            first.publication,
            kernel.status()?.publication.map(|p| p.token)
        );
        assert!(
            kernel
                .inspect_service(airk_core::identity::Sha256Digest::from_bytes([42; 32]))
                .is_err()
        );
        kernel.configure_vectors(true)?;
        let changed = kernel.inspect_service(project.binding)?;
        assert_eq!(first.publication, changed.publication);
        assert_ne!(first.guard_digest()?, changed.guard_digest()?);
        let moved = owner.path().join("temporarily-unavailable");
        std::fs::rename(&source, &moved)?;
        let missing = kernel.inspect_service(project.binding)?;
        assert_eq!(missing.source_state, SourceState::Unavailable);
        assert_eq!(missing.index_state, IndexState::Ready);
        assert_eq!(missing.guard_digest()?, changed.guard_digest()?);
        assert!(!serde_json::to_string(&missing)?.contains("memo.md"));
        assert!(!serde_json::to_string(&missing)?.contains("copper harbor"));
        assert!(!data.join("models/local").exists());
        assert_eq!(std::fs::read(moved.join("memo.md"))?, bytes);
        Ok(())
    }
}
