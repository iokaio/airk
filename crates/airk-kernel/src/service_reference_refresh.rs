// SPDX-License-Identifier: Apache-2.0
//! Explicit native refresh, separate from evidence verification and requery.
use crate::Kernel;
use airk_core::{
    catalog_refresh::{
        RefreshFallbackReason, RefreshMode, RefreshReviewRequired, RefreshSelection,
    },
    identity::RevisionToken,
    scoped_reads::SourceDescriptor,
    service::ProjectBinding,
    service_configuration::{ConfigurationRequest, ProjectConfiguration},
};
use anyhow::{Context, Result, ensure};

fn control_drift_review(error: anyhow::Error) -> anyhow::Error {
    match error.downcast_ref::<airk_ingest::control_snapshot::ControlDrift>() {
        Some(drift) => RefreshReviewRequired {
            reasons: vec![match drift.kind {
                airk_ingest::control_snapshot::ControlDriftKind::Inputs => {
                    RefreshFallbackReason::ControlInputsChanged
                }
                airk_ingest::control_snapshot::ControlDriftKind::Context => {
                    RefreshFallbackReason::ControlContextChanged
                }
            }],
        }
        .into(),
        None => error,
    }
}

impl Kernel {
    /// Read-only qualification under an independently authenticated read scope.
    /// Legacy metadata contains no prior ignore/control provenance. Such an
    /// index can only use an explicit, separately reviewed whole-index fallback.
    pub fn qualify_reference_refresh(
        &self,
        selection: &RefreshSelection,
        sources: &[SourceDescriptor],
    ) -> Result<RevisionToken> {
        selection.validate()?;
        ensure!(
            selection.members.len() == 1,
            "refresh currently admits one explicitly selected member per saved operation"
        );
        let read = self.current_reader()?;
        self.require_index_metadata(&read)?;
        let configured = self.configured_roots()?;
        self.require_configured_index_metadata(&read, &configured)?;
        let publication = crate::current::runtime::revision(read.token()?);
        ensure!(
            publication == selection.members[0].original_publication,
            "refresh original publication changed; new review required"
        );

        let state = read
            .collection_state()?
            .context("refresh collection metadata absent")?;
        if selection.mode == RefreshMode::SelectedOnly {
            self.reference_refresh_baseline(&read)?;
        }
        for source in &selection.members[0].sources {
            let descriptor = sources
                .iter()
                .find(|entry| entry.source_root_id == source.source_root_id)
                .context("refresh source root is outside the authenticated selection")?;
            let path = if descriptor.original_root == self.root_id() {
                source.source_relative_path.clone()
            } else {
                let alias = state
                    .configuration
                    .roots
                    .iter()
                    .find(|(_, root)| **root == descriptor.original_root)
                    .map(|(alias, _)| alias)
                    .context("refresh source alias changed")?;
                format!("@{alias}/{}", source.source_relative_path)
            };
            let location = airk_store::roots::source_location(
                &self.root_id(),
                &state.configuration.roots,
                &path,
            )?;
            ensure!(
                location.root == descriptor.original_root
                    && source.canonical_root == descriptor.original_root
                    && location.relative_path == source.source_relative_path,
                "refresh source alias namespace is ambiguous"
            );
            let artifact = read
                .artifact(&path)?
                .context("refresh original source is not retained in the selected publication")?;
            ensure!(
                artifact.content_hash == source.indexed_hash.to_string(),
                "refresh original source hash changed; new review required"
            );
        }
        read.token()?;
        Ok(publication)
    }

    /// Called only by the existing admitted native Configure worker, after the
    /// same current host/grant/backing checks used at acceptance. This starts no
    /// nested scheduler and never requests an extra watcher-wide source pass.
    pub fn refresh_service_references(
        &self,
        project: ProjectBinding,
        request: &ConfigurationRequest,
    ) -> Result<airk_ingest::IndexReport> {
        self.refresh_service_references_owned(project, request)
            .map_err(control_drift_review)
    }
    fn refresh_service_references_owned(
        &self,
        project: ProjectBinding,
        request: &ConfigurationRequest,
    ) -> Result<airk_ingest::IndexReport> {
        ensure!(
            self.is_service_collection(),
            "refresh requires its native project owner"
        );
        let ProjectConfiguration::RefreshReferences { selection } = &request.configuration else {
            anyhow::bail!("refresh configuration missing");
        };
        selection.validate()?;
        ensure!(
            selection.members.len() == 1,
            "refresh requires one exact native member"
        );
        if selection.mode == RefreshMode::SelectedOnly {
            // Selected locators have already been translated and bound at native
            // admission. Re-derive from the authenticated applied alias map.
            let read = self.current_reader()?;
            let state = read
                .collection_state()?
                .context("refresh collection metadata absent")?;
            let policy = self.source_policy_for_work()?;
            let mut paths = std::collections::BTreeSet::new();
            for source in &selection.members[0].sources {
                let root = policy.as_ref().and_then(|policy| {
                    policy
                        .roots
                        .iter()
                        .find(|root| root.id == source.source_root_id)
                });
                // Canonical native admission supplies source-root identity. For
                // policy-less legacy roots use the exact original descriptor
                // path sealed into the new operation by that admission.
                let native = source.canonical_root.as_str();
                if let Some(root) = root {
                    ensure!(root.canonical_path == native, "refresh source root rebound");
                }
                let path = if native == self.root_id() {
                    source.source_relative_path.clone()
                } else {
                    let alias = state
                        .configuration
                        .roots
                        .iter()
                        .find(|(_, root)| root.as_str() == native)
                        .map(|(alias, _)| alias)
                        .context("refresh alias removed")?;
                    format!("@{alias}/{}", source.source_relative_path)
                };
                let baseline = self.reference_refresh_baseline(&read)?;
                let absolute = std::path::Path::new(native).join(&source.source_relative_path);
                ensure!(
                    !baseline
                        .inputs
                        .contains_key(absolute.to_str().context("non-UTF8 selected locator")?),
                    RefreshReviewRequired {
                        reasons: vec![RefreshFallbackReason::ControlInput]
                    }
                );
                paths.insert(path);
            }
            drop(read);
            self.validate_service_configuration(project, request)?;
            return crate::service_execution::with_database_admission(|| {
                self.observe_service_index(airk_core::usage::UsageOperation::Index, || {
                    self.index_current_reviewed_refresh_paths(project, request, &paths)
                })
            });
        }
        self.validate_service_configuration(project, request)?;
        crate::service_execution::with_database_admission(|| {
            self.observe_service_index(airk_core::usage::UsageOperation::Index, || {
                self.index_current_reviewed_refresh(project, request)
            })
        })
    }

    /// Must run while the existing collection writer is held and BEFORE any
    /// bootstrap/journal recovery, model capture or source discovery. It returns
    /// no authority; the caller retains the real writer through publication.
    pub(crate) fn require_refresh_base_before_recovery(
        &self,
        project: ProjectBinding,
        request: &ConfigurationRequest,
        writer: &airk_store::WriterLock,
    ) -> Result<Option<std::sync::Arc<airk_ingest::control_snapshot::ControlSnapshot>>> {
        let ProjectConfiguration::RefreshReferences { selection } = &request.configuration else {
            anyhow::bail!("refresh base guard lacks exact request");
        };
        ensure!(
            self.store.runtime_bootstrap()?.is_none(),
            RefreshReviewRequired {
                reasons: vec![RefreshFallbackReason::PendingRecovery]
            }
        );
        let read = self.current_reader()?;
        ensure!(
            !read.has_prepared_update()?,
            RefreshReviewRequired {
                reasons: vec![RefreshFallbackReason::PendingRecovery]
            }
        );
        ensure!(
            crate::current::runtime::revision(read.token()?)
                == selection.members[0].original_publication,
            "refresh original publication changed; new review required"
        );
        let controls = if selection.mode == RefreshMode::SelectedOnly {
            let baseline = self.reference_refresh_baseline(&read)?;
            ensure!(
                baseline.settings == request.settings,
                RefreshReviewRequired {
                    reasons: vec![RefreshFallbackReason::ConfigurationChanged]
                }
            );
            let snapshot = std::sync::Arc::new(
                airk_ingest::control_snapshot::ControlSnapshot::revalidate(baseline)?,
            );
            snapshot.revalidate_inputs()?;
            Some(snapshot)
        } else if self
            .rebuild_extraction_policy()?
            .policy
            .declared_configurations()
            .is_empty()
        {
            Some(std::sync::Arc::new(
                airk_ingest::control_snapshot::ControlSnapshot::capture(),
            ))
        } else {
            None
        };
        drop(read);
        // Runtime-bootstrap absence and actual Ready/no-pending state were
        // checked above under this same writer. Recheck saved settings without
        // recursively acquiring its advisory through general inspection.
        self.validate_service_configuration_with_writer(project, request, writer)?;
        Ok(controls)
    }
    fn reference_refresh_baseline(
        &self,
        read: &airk_store::current::CurrentRead,
    ) -> Result<airk_core::source_control::SourceControlBaseline> {
        let missing = || RefreshReviewRequired {
            reasons: vec![RefreshFallbackReason::UnqualifiedControlProvenance],
        };
        let reference = read
            .resource_reference(airk_core::source_control::CONTROL_BASELINE_RESOURCE)?
            .ok_or_else(missing)?;
        ensure!(
            reference.bytes <= airk_core::source_control::MAX_CONTROL_BASELINE_BYTES,
            missing()
        );
        let bytes = self.store.read_current_resource(&reference)?;
        let baseline: airk_core::source_control::SourceControlBaseline =
            serde_json::from_slice(&bytes)?;
        baseline.validate()?;
        ensure!(
            serde_json::to_vec(&baseline)? == bytes,
            "noncanonical retained control baseline"
        );
        ensure!(
            baseline.matches_publication(read.head().scope.incarnation, read.head().sequence),
            missing()
        );
        let state = read
            .collection_state()?
            .context("refresh collection metadata absent")?;
        ensure!(
            baseline.configuration
                == airk_core::identity::framed_digest(
                    b"airk/refresh-applied-configuration/1",
                    &[&serde_json::to_vec(&state.configuration)?]
                ),
            RefreshReviewRequired {
                reasons: vec![RefreshFallbackReason::ConfigurationChanged]
            }
        );
        let policy = self.rebuild_extraction_policy()?.policy;
        // Local mutable model directories lack a small immutable configuration
        // identity. They require explicit full review, never selected capture.
        ensure!(
            !self.vectors_enabled()? || self.store.shared_model_runtime().is_some(),
            RefreshReviewRequired {
                reasons: vec![RefreshFallbackReason::ExtractorChanged]
            }
        );
        ensure!(
            serde_json::to_vec(&self.configured_model_objects(false)?)?
                == serde_json::to_vec(&state.configuration.model)?,
            RefreshReviewRequired {
                reasons: vec![RefreshFallbackReason::ConfigurationChanged]
            }
        );
        ensure!(
            policy.declared_configurations().is_empty()
                && crate::extraction::policy_digest(&policy)?
                    == state.configuration.extraction_policy_digest
                && state.configuration.extractor_version == airk_core::EXTRACTOR_VERSION
                && state.configuration.roots == self.configured_roots()?
                && state.configuration.source_policy_digest == self.source_policy_digest()?
                && serde_json::to_vec(&state.configuration.vector_selection)?
                    == serde_json::to_vec(&self.vector_selection_for_index_metadata(read)?)?,
            RefreshReviewRequired {
                reasons: vec![RefreshFallbackReason::ConfigurationChanged]
            }
        );
        Ok(baseline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        catalog_refresh::{RefreshMember, RefreshSource},
        identity::{Incarnation, Sha256Digest},
        index_catalog::{CatalogRevision, GrantId, IndexId, QueryProjectId, SourceRootId},
        service::{CombinedConnectRequest, RequestKey},
    };
    use sha2::Digest;
    fn request(
        kernel: &Kernel,
        project: ProjectBinding,
        mode: RefreshMode,
    ) -> Result<ConfigurationRequest> {
        request_path(kernel, project, mode, "one.txt")
    }
    fn request_path(
        kernel: &Kernel,
        project: ProjectBinding,
        mode: RefreshMode,
        path: &str,
    ) -> Result<ConfigurationRequest> {
        let observed = kernel.inspect_service(project)?;
        let file = kernel.file_info(path)?;
        let id = Incarnation::from_bytes([1; 16]);
        let hash = Sha256Digest::from_bytes([2; 32]);
        let index = IndexId::from_identity(id);
        let key = RequestKey {
            namespace: id,
            epoch: 1,
            nonce: id,
        };
        Ok(ConfigurationRequest {
            prepare: false,
            incarnation: observed.incarnation,
            settings: observed.settings_digest.context("settings absent")?,
            configuration: ProjectConfiguration::RefreshReferences {
                selection: Box::new(RefreshSelection {
                    schema: 1,
                    caller: id,
                    key,
                    original_result_digest: hash,
                    original_query_digest: hash,
                    original_evidence_selection_digest: hash,
                    mode,
                    selection: CombinedConnectRequest {
                        schema: 1,
                        context_key: hash,
                        namespace: id,
                        expected_catalog_revision: CatalogRevision::INITIAL,
                        grant_id: GrantId::from_identity(id),
                        expected_grant_revision: CatalogRevision::INITIAL,
                        project_ids: vec![QueryProjectId::from_identity(id)],
                        index_ids: Some(vec![index]),
                    },
                    members: vec![RefreshMember {
                        index_id: index,
                        original_publication: file.publication.context("publication absent")?.token,
                        sources: vec![RefreshSource {
                            source_root_id: SourceRootId::from_identity(id),
                            source_relative_path: path.into(),
                            canonical_root: kernel.root_id(),
                            indexed_hash: file.content_hash.parse()?,
                        }],
                    }],
                }),
            },
        })
    }
    #[test]
    fn reviewed_full_fallback_updates_both_files_and_refuses_old_publication_reexecution()
    -> Result<()> {
        let source = crate::unlinked_tempdir()?;
        let data = crate::unlinked_tempdir()?;
        std::fs::write(source.path().join("one.txt"), b"one old")?;
        std::fs::write(source.path().join("two.txt"), b"two old")?;
        let kernel =
            Kernel::open_service_collection(source.path(), Some(data.path().into()), "refresh")?;
        kernel.configure_vectors(false)?;
        kernel.index_service_sources()?;
        let project =
            crate::service_projects::registration(source.path(), data.path(), "refresh")?.binding;
        let selected = request(&kernel, project, RefreshMode::SelectedOnly)?;
        let token = kernel.current_reader()?.token()?;
        assert!(
            kernel
                .refresh_service_references(project, &selected)
                .unwrap_err()
                .is::<RefreshReviewRequired>()
        );
        assert_eq!(
            kernel.current_reader()?.token()?,
            token,
            "unknown controls must never silently widen"
        );
        let full = request(&kernel, project, RefreshMode::FullReconcileReviewed)?;
        std::fs::write(source.path().join("one.txt"), b"one current")?;
        std::fs::write(source.path().join("two.txt"), b"two current")?;
        kernel.refresh_service_references(project, &full)?;
        assert_ne!(kernel.current_reader()?.token()?, token);
        for (path, bytes) in [
            ("one.txt", b"one current".as_slice()),
            ("two.txt", b"two current".as_slice()),
        ] {
            assert_eq!(
                kernel.file_info(path)?.content_hash,
                airk_core::identity::Sha256Digest::from_bytes(sha2::Sha256::digest(bytes).into())
                    .to_string()
            );
            assert_eq!(std::fs::read(source.path().join(path))?, bytes);
        }
        let after = kernel.current_reader()?.token()?;
        assert!(
            kernel.refresh_service_references(project, &full).is_err(),
            "old accepted intent is not permission for a second pass"
        );
        assert_eq!(kernel.current_reader()?.token()?, after);
        Ok(())
    }
    #[test]
    fn qualified_selected_refresh_preserves_unselected_payload_and_changed_controls_refuse_before_publish()
    -> Result<()> {
        let source = crate::unlinked_tempdir()?;
        let data = crate::unlinked_tempdir()?;
        std::fs::write(source.path().join("one.txt"), b"one baseline")?;
        std::fs::write(source.path().join("two.txt"), b"two baseline")?;
        let kernel = Kernel::open_service_collection(
            source.path(),
            Some(data.path().into()),
            "refresh-selected",
        )?;
        kernel.configure_vectors(false)?;
        kernel.index_service_sources()?;
        let project =
            crate::service_projects::registration(source.path(), data.path(), "refresh-selected")?
                .binding;
        // Legacy ordinary scans cannot be retroactively certified. The user
        // first reviews a full pass which records actual matcher inputs.
        let full = request(&kernel, project, RefreshMode::FullReconcileReviewed)?;
        kernel.refresh_service_references(project, &full)?;
        let before = kernel.current_reader()?;
        assert!(
            before
                .resource_reference(airk_core::source_control::CONTROL_BASELINE_RESOURCE)?
                .is_some()
        );
        let untouched = serde_json::to_value(
            before
                .artifact("two.txt")?
                .context("second artifact absent")?,
        )?;
        drop(before);
        let selected = request(&kernel, project, RefreshMode::SelectedOnly)?;
        std::fs::write(source.path().join("one.txt"), b"one selected change")?;
        std::fs::write(source.path().join("two.txt"), b"two unselected change")?;
        let report = kernel.refresh_service_references(project, &selected)?;
        assert!(report.path_scoped);
        let after = kernel.current_reader()?;
        assert_eq!(
            serde_json::to_value(
                after
                    .artifact("two.txt")?
                    .context("second artifact absent")?
            )?,
            untouched,
            "unselected retained payload/profile must remain exact"
        );
        assert_eq!(
            kernel.file_info("one.txt")?.content_hash,
            Sha256Digest::from_bytes(sha2::Sha256::digest(b"one selected change").into())
                .to_string()
        );
        assert_eq!(
            std::fs::read(source.path().join("two.txt"))?,
            b"two unselected change"
        );
        let publication = after.token()?;
        drop(after);
        let stale_controls = request(&kernel, project, RefreshMode::SelectedOnly)?;
        std::fs::write(source.path().join(".airkignore"), b"*.never\n")?;
        std::fs::write(source.path().join("one.txt"), b"must stay unpublished")?;
        let drift = kernel
            .refresh_service_references(project, &stale_controls)
            .unwrap_err();
        assert_eq!(
            drift
                .downcast_ref::<RefreshReviewRequired>()
                .context("known absence drift lost typed fallback")?
                .reasons,
            vec![RefreshFallbackReason::ControlInputsChanged]
        );
        assert_eq!(
            kernel.current_reader()?.token()?,
            publication,
            "changed absent control must refuse without publication"
        );
        let full = request(&kernel, project, RefreshMode::FullReconcileReviewed)?;
        kernel.refresh_service_references(project, &full)?;
        let new_selected = request(&kernel, project, RefreshMode::SelectedOnly)?;
        std::fs::write(
            source.path().join("one.txt"),
            b"fresh qualification succeeds",
        )?;
        kernel.refresh_service_references(project, &new_selected)?;
        assert_eq!(
            kernel.file_info("one.txt")?.content_hash,
            Sha256Digest::from_bytes(sha2::Sha256::digest(b"fresh qualification succeeds").into())
                .to_string()
        );
        Ok(())
    }
    /// Exercise the predecessor's actual Store artifact journal contract: an
    /// ordinary artifact deletion updates the authenticated map but preserves
    /// unknown resources. No test-only admission override is installed.
    #[test]
    fn predecessor_artifact_publication_cannot_requalify_restored_controls() -> Result<()> {
        use airk_core::graph_storage::PreparedGraphDelta;
        use airk_store::current::{
            ArtifactChange, ArtifactVectorChanges,
            graph::{GraphOverlay, GraphOverlayLimits},
        };
        for historical_unbound in [false, true] {
            let source = crate::unlinked_tempdir()?;
            let data = crate::unlinked_tempdir()?;
            std::fs::write(source.path().join("one.txt"), b"one baseline")?;
            std::fs::write(source.path().join("two.txt"), b"two baseline")?;
            let kernel = Kernel::open_service_collection(
                source.path(),
                Some(data.path().into()),
                "refresh-predecessor",
            )?;
            kernel.configure_vectors(false)?;
            kernel.index_service_sources()?;
            let project = crate::service_projects::registration(
                source.path(),
                data.path(),
                "refresh-predecessor",
            )?
            .binding;
            let full = request(&kernel, project, RefreshMode::FullReconcileReviewed)?;
            kernel.refresh_service_references(project, &full)?;
            let read = kernel.current_reader()?;
            let prior_reference = read
                .resource_reference(airk_core::source_control::CONTROL_BASELINE_RESOURCE)?
                .context("qualified resource absent")?;
            let mut prior: airk_core::source_control::SourceControlBaseline =
                serde_json::from_slice(&kernel.store.read_current_resource(&prior_reference)?)?;
            assert!(prior.matches_publication(read.head().scope.incarnation, read.head().sequence));
            let scope = read.head().scope.clone();
            let base = read.head().clone();
            drop(read);
            // B changes eligibility; the predecessor publishes a genuine source
            // deletion while retaining the unknown A witness map reference.
            std::fs::write(source.path().join(".airkignore"), b"two.txt\n")?;
            std::fs::remove_file(source.path().join("two.txt"))?;
            let quota = kernel.collection_quota()?;
            let writer = kernel.store.lock()?;
            let reservation = kernel.publication_reservation()?;
            let mut expected_reference = prior_reference.clone();
            let resources = if historical_unbound {
                prior.publication = None;
                expected_reference = kernel.store.retain_current_resource_owned(
                    &writer,
                    &serde_json::to_vec(&prior)?,
                    &reservation,
                )?;
                vec![airk_store::current::resources::ResourceChange {
                    key: airk_core::source_control::CONTROL_BASELINE_RESOURCE.into(),
                    reference: Some(expected_reference.clone()),
                }]
            } else {
                vec![]
            };
            let overlay = GraphOverlay::new(
                &reservation,
                kernel.store.current_build_read(&scope, &writer)?,
                GraphOverlayLimits::for_indexing(&reservation),
            )?;
            let sealed = overlay.seal()?;
            let mut target = base.clone();
            target.sequence += 1;
            target.lexical_sequence = target.sequence;
            target.graph_head = sealed.descriptor().target.clone();
            let capacity = kernel.store.current_database_capacity(&scope)?;
            let mut current = kernel
                .store
                .current_write(&writer, &quota, &scope, capacity)?;
            let (pending, _) = current.prepare_artifacts_with_fts(
                &base.token()?,
                target,
                &sealed,
                ArtifactVectorChanges {
                    artifacts: &[ArtifactChange::Delete("two.txt".into())],
                    vector_inputs: None,
                    occurrences: &[],
                    configuration: None,
                },
                None,
                &[],
                &resources,
            )?;
            current.mark_applying(&pending)?;
            current.commit(&pending, pending.target.lexical_commit)?;
            drop(current);
            drop(writer);
            drop(reservation);
            std::fs::remove_file(source.path().join(".airkignore"))?;
            let read = kernel.current_reader()?;
            assert!(read.artifact("two.txt")?.is_none());
            assert_eq!(
                read.resource_reference(airk_core::source_control::CONTROL_BASELINE_RESOURCE)?
                    .context("predecessor dropped unknown resource")?
                    .object_digest()?,
                expected_reference.object_digest()?
            );
            let after = read.token()?;
            assert!(!prior.matches_publication(after.incarnation, after.sequence));
            let error = kernel.reference_refresh_baseline(&read).unwrap_err();
            assert!(error.downcast_ref::<RefreshReviewRequired>().is_some());
            drop(read);
            // A newly reviewed full pass owns a new witness; then selected work
            // may resume. Restoring the control bytes alone never authorizes it.
            let full = request(&kernel, project, RefreshMode::FullReconcileReviewed)?;
            kernel.refresh_service_references(project, &full)?;
            let read = kernel.current_reader()?;
            kernel.reference_refresh_baseline(&read)?;
            drop(read);
            let selected = request(&kernel, project, RefreshMode::SelectedOnly)?;
            std::fs::write(source.path().join("one.txt"), b"one selected successor")?;
            kernel.refresh_service_references(project, &selected)?;
            let read = kernel.current_reader()?;
            kernel.reference_refresh_baseline(&read)?;
        }
        Ok(())
    }

    #[test]
    fn pending_runtime_bootstrap_is_refused_before_recovery_or_publication() -> Result<()> {
        let source = crate::unlinked_tempdir()?;
        let data = crate::unlinked_tempdir()?;
        std::fs::write(source.path().join("one.txt"), b"original")?;
        let kernel = Kernel::open_service_collection(
            source.path(),
            Some(data.path().into()),
            "refresh-pending",
        )?;
        kernel.configure_vectors(false)?;
        kernel.index_service_sources()?;
        let project =
            crate::service_projects::registration(source.path(), data.path(), "refresh-pending")?
                .binding;
        let request = request(&kernel, project, RefreshMode::FullReconcileReviewed)?;
        let token = kernel.current_reader()?.token()?;
        let held = b"intentionally opaque pending bootstrap; do not parse or replay";
        {
            let writer = kernel.store.lock()?;
            kernel.require_refresh_base_before_recovery(project, &request, &writer)?;
            let mut changed = request.clone();
            changed.settings = Sha256Digest::from_bytes([81; 32]);
            let error = kernel
                .require_refresh_base_before_recovery(project, &changed, &writer)
                .err()
                .context("changed settings were accepted")?;
            assert!(error.is::<crate::service_configuration::ConfigurationChanged>());
            changed = request.clone();
            changed.incarnation = Incarnation::from_bytes([82; 16]);
            assert!(
                kernel
                    .validate_service_configuration_with_writer(project, &changed, &writer)
                    .unwrap_err()
                    .is::<crate::service_configuration::ConfigurationChanged>()
            );
            assert!(
                kernel
                    .validate_service_configuration_with_writer(
                        Sha256Digest::from_bytes([83; 32]),
                        &request,
                        &writer
                    )
                    .is_err()
            );
            assert_eq!(kernel.current_reader()?.token()?, token);
            kernel.store.write_runtime_bootstrap(&writer, held)?;
        }
        std::fs::write(source.path().join("one.txt"), b"unpublished edit")?;
        assert!(
            kernel
                .refresh_service_references(project, &request)
                .is_err()
        );
        assert_eq!(
            kernel.store.runtime_bootstrap()?.as_deref(),
            Some(held.as_slice())
        );
        assert_eq!(kernel.current_reader()?.token()?, token);
        assert_eq!(
            std::fs::read(source.path().join("one.txt"))?,
            b"unpublished edit"
        );
        Ok(())
    }

    /// Real native collection + actual reviewed profile migration and Current
    /// publication. This is not a native grant/CLI/MCP transport admission test.
    #[test]
    fn selected_refresh_preserves_applied_lean_rich_identity_and_unselected_payloads() -> Result<()>
    {
        use airk_core::retention::{AppliedRetention, RetainedProfile};
        let source = crate::unlinked_tempdir()?;
        let data = crate::unlinked_tempdir()?;
        std::fs::write(source.path().join("one.md"), b"# One baseline\n")?;
        std::fs::write(source.path().join("two.md"), b"# Two baseline\n")?;
        let kernel = Kernel::open_service_collection(
            source.path(),
            Some(data.path().into()),
            "refresh-retention",
        )?;
        kernel.configure_vectors(false)?;
        kernel.index_service_sources()?;
        let project =
            crate::service_projects::registration(source.path(), data.path(), "refresh-retention")?
                .binding;
        for target in [RetainedProfile::Lean, RetainedProfile::Rich] {
            let migration = kernel.preview_retained_migration(target)?;
            kernel.configure_retained_migration(&migration)?;
            kernel.index_service_sources()?;
            let full = request_path(
                &kernel,
                project,
                RefreshMode::FullReconcileReviewed,
                "one.md",
            )?;
            kernel.refresh_service_references(project, &full)?;
            let before = kernel.current_reader()?;
            let state = before
                .collection_state()?
                .context("applied configuration missing")?;
            assert_eq!(state.configuration.retained_profile, Some(target));
            assert_eq!(
                kernel.extraction_policy()?.published_retained_profile,
                Some(target)
            );
            let untouched = serde_json::to_value(
                before
                    .artifact("two.md")?
                    .context("unselected artifact absent")?,
            )?;
            let previous = before.token()?;
            drop(before);
            let selected = request_path(&kernel, project, RefreshMode::SelectedOnly, "one.md")?;
            let one = format!("# One selected {target:?}\nThe system must retain 200 ms.\n");
            let two = format!("# Two unselected live edit {target:?}\n");
            std::fs::write(source.path().join("one.md"), one.as_bytes())?;
            std::fs::write(source.path().join("two.md"), two.as_bytes())?;
            let report = kernel.refresh_service_references(project, &selected)?;
            assert!(report.path_scoped);
            let after = kernel.current_reader()?;
            assert_ne!(
                after.token()?,
                previous,
                "actual selected publication did not advance"
            );
            assert_eq!(
                serde_json::to_value(
                    after
                        .artifact("two.md")?
                        .context("unselected artifact lost")?
                )?,
                untouched
            );
            assert_eq!(
                after
                    .collection_state()?
                    .context("published config missing")?
                    .configuration
                    .retained_profile,
                Some(target)
            );
            let artifact = after
                .artifact("one.md")?
                .context("selected artifact missing")?;
            let profile = artifact
                .retained
                .as_ref()
                .and_then(|reference| reference.profile.as_ref())
                .context("effective selected profile missing")?;
            assert!(
                matches!(AppliedRetention::from_profile(profile)?, AppliedRetention::Selected { profile, .. } if profile == target)
            );
            assert_eq!(
                artifact.content_hash,
                Sha256Digest::from_bytes(sha2::Sha256::digest(one.as_bytes()).into()).to_string()
            );
            assert_eq!(std::fs::read(source.path().join("one.md"))?, one.as_bytes());
            assert_eq!(std::fs::read(source.path().join("two.md"))?, two.as_bytes());
            drop(after);
        }
        // A pending desired migration is not permission to silently refresh
        // under the old applied retained profile/control witness.
        let selected = request_path(&kernel, project, RefreshMode::SelectedOnly, "one.md")?;
        let pending = kernel.preview_retained_migration(RetainedProfile::Lean)?;
        kernel.configure_retained_migration(&pending)?;
        let publication = kernel.current_reader()?.token()?;
        assert!(
            kernel
                .refresh_service_references(project, &selected)
                .is_err()
        );
        assert_eq!(kernel.current_reader()?.token()?, publication);
        Ok(())
    }

    #[test]
    fn only_known_control_drift_maps_to_review_and_real_errors_keep_their_cause() -> Result<()> {
        for (kind, reason) in [
            (
                airk_ingest::control_snapshot::ControlDriftKind::Inputs,
                RefreshFallbackReason::ControlInputsChanged,
            ),
            (
                airk_ingest::control_snapshot::ControlDriftKind::Context,
                RefreshFallbackReason::ControlContextChanged,
            ),
        ] {
            let error =
                control_drift_review(airk_ingest::control_snapshot::ControlDrift { kind }.into());
            assert_eq!(
                error
                    .downcast_ref::<RefreshReviewRequired>()
                    .context("typed drift missing")?
                    .reasons,
                vec![reason]
            );
        }
        let error =
            control_drift_review(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into());
        assert!(error.downcast_ref::<RefreshReviewRequired>().is_none());
        assert_eq!(
            error
                .downcast_ref::<std::io::Error>()
                .context("permission cause lost")?
                .kind(),
            std::io::ErrorKind::PermissionDenied
        );
        let error =
            serde_json::from_slice::<airk_core::source_control::SourceControlBaseline>(b"not-json")
                .unwrap_err();
        let error = control_drift_review(error.into());
        assert!(error.downcast_ref::<RefreshReviewRequired>().is_none());
        assert!(error.downcast_ref::<serde_json::Error>().is_some());
        Ok(())
    }
}
