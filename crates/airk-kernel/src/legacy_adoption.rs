// SPDX-License-Identifier: Apache-2.0
//! Preparation of authenticated saved evidence for explicit native adoption.
//!
//! No source path is opened, producer run, Current publication committed or
//! authority inferred here. Unsupported evidence remains explicitly legacy.
use airk_core::{
    Artifact,
    binding::{ArtifactBinding, bind_payload},
    extraction::{ExtractionPayload, ExtractionProfile},
};
use airk_store::{
    current::analysis::AnalysisEvidence,
    legacy_adoption::{LegacyPublicationIdentity, SavedLegacyArtifact, VerifiedLegacyArchive},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub struct PreparationLimits {
    pub max_artifacts: usize,
    pub max_artifact_bytes: u64,
    pub max_total_bytes: u64,
}

impl Default for PreparationLimits {
    fn default() -> Self {
        Self {
            max_artifacts: 100_000,
            max_artifact_bytes: 64 << 20,
            max_total_bytes: 8 << 30,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedSourceOrigin {
    RetainedOriginal,
    /// The original source digest and length verify the concatenated saved
    /// UTF-8 chunks. No live source or extractor participates.
    ReconstructedUtf8DigestVerified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyRetentionReason {
    SourceUnavailable,
    UnsupportedInterpretation,
    UnsupportedCurrentProfile,
    PayloadBindingDiffers,
}

pub enum PreparedLegacyArtifact {
    Native {
        artifact: Artifact,
        payload: ExtractionPayload,
        profile: ExtractionProfile,
        source: Vec<u8>,
        source_origin: SavedSourceOrigin,
    },
    RetainedLegacy {
        artifact: Artifact,
        source: Option<Vec<u8>>,
        profile: Option<ExtractionProfile>,
        payload: Option<ExtractionPayload>,
        source_origin: Option<SavedSourceOrigin>,
        reason: LegacyRetentionReason,
    },
}

use airk_store::legacy_adoption::AdoptionResource;
pub use airk_store::legacy_adoption::{LegacyCompatibility, VerifiedLegacyPublication};

pub struct PreparedLegacyEvidence {
    pub origin: LegacyPublicationIdentity,
    pub root: String,
    pub collection: String,
    pub roots: BTreeMap<String, String>,
    pub artifacts: Vec<PreparedLegacyArtifact>,
    /// Original producer assertions. Install as a Current analysis resource only
    /// when analysis_requires_legacy_retention is false.
    pub analysis: Option<AnalysisEvidence>,
    pub compatibility: LegacyCompatibility,
}

#[derive(serde::Serialize)]
pub struct LegacyAdoptionReceipt {
    pub original: LegacyPublicationIdentity,
    pub current: airk_store::current::CurrentToken,
    pub compatibility: LegacyCompatibility,
}

#[derive(serde::Serialize)]
pub struct LegacySavedFileCompatibility {
    pub origin: airk_core::legacy_adoption::LegacySavedOrigin,
    pub vector_selection_available: bool,
    pub profile_families_available: bool,
}

pub(crate) struct CurrentLegacyAdoption {
    pub observation: serde_json::Value,
    semantic_unavailable: bool,
}

impl CurrentLegacyAdoption {
    pub fn qualify(&self, readiness: &mut airk_core::readiness::SemanticReadiness) {
        if self.semantic_unavailable {
            readiness.state = airk_core::readiness::SemanticState::Unavailable;
            readiness.completed_artifacts = 0;
            readiness.completed_units = 0;
            readiness.first_ready_unix_ms = None;
            readiness.fully_ready_unix_ms = None;
            readiness.selection_digest = None;
            readiness.pipeline_digest = None;
            readiness.failure = Some("legacy_saved_semantic_conversion_unavailable".into());
        }
    }
}

/// One canonical observation for status, diagnostics and search readiness. The
/// complete original resource stays visible, including explicit old/new mapping.
pub(crate) fn current_observation(
    store: &airk_store::FileStore,
    read: &airk_store::current::CurrentRead,
) -> Result<Option<CurrentLegacyAdoption>> {
    let Some(reference) = read.resource_reference("legacy-adoption")? else {
        return Ok(None);
    };
    ensure!(
        reference.bytes <= 2 << 20,
        "legacy adoption observation exceeds bound"
    );
    let bytes = store.read_current_resource(&reference)?;
    let resource: AdoptionResource = serde_json::from_slice(&bytes)?;
    let state = read
        .collection_state()?
        .context("legacy adoption configuration missing")?;
    ensure!(
        serde_json::to_vec(&resource)? == bytes
            && resource.schema == 1
            && resource.root == read.head().scope.root
            && resource.collection == read.head().scope.collection
            && resource.roots == state.configuration.roots,
        "legacy adoption observation scope differs"
    );
    let semantic_unavailable = resource.compatibility.original_embedding_model.is_some()
        && !resource.compatibility.retrieval_converted
        && state.configuration.model.is_none();
    let matches_current = resource.target_incarnation == read.head().scope.incarnation
        && resource.target_sequence == read.head().sequence;
    let mut observation = serde_json::to_value(&resource)?;
    observation["matches_current_publication"] = matches_current.into();
    Ok(Some(CurrentLegacyAdoption {
        observation,
        semantic_unavailable,
    }))
}

impl crate::Kernel {
    /// Open only the destination reserved by an actual native adoption intent.
    /// Source labels come from verified saved evidence; their directories need
    /// not exist. This grants neither ordinary Ready membership nor read grants.
    pub fn open_service_collection_for_legacy_adoption(
        permit: &airk_store::legacy_adoption::LegacyAdoptionWritePermit,
        archive: &VerifiedLegacyArchive,
    ) -> Result<Self> {
        permit.require_archive(archive)?;
        let registration = permit.prepared_registration()?;
        ensure!(
            registration.root == archive.scope().root
                && registration.collection == archive.scope().collection,
            "native adoption destination changes saved source labels"
        );
        let data_root = std::path::PathBuf::from(&registration.data_root);
        let collection = airk_store::service::collection::Collection::existing(
            &data_root,
            &registration.root,
            &registration.collection,
        )?;
        let store = collection.select_for_diagnostics()?;
        // This proves the selected physical incarnation is exactly the native
        // preparation operation, under the pinned target/control namespace.
        permit.require_store(&store)?;
        let source_policy = crate::source_policy::PolicyRuntime::shared_load(
            data_root
                .join("collections-1.4")
                .join(&registration.collection),
        )?;
        Ok(Self {
            source_policy,
            read_scope: None,
            verification_cancellation: None,
            progress: None,
            root: std::path::PathBuf::from(&archive.scope().root),
            collection: registration.collection.clone(),
            data_root,
            store,
            service_collection: Some(collection),
            service_selection: None,
            service_freshness: None,
            split_authority: None,
            native_owner: std::sync::Arc::new(std::sync::Mutex::new(None)),
        })
    }

    pub fn verify_legacy_adoption_publication(&self) -> Result<VerifiedLegacyPublication> {
        self.require_unrestricted_read()?;
        let read = self.current_reader()?;
        let proof = airk_store::legacy_adoption::verify_publication(&self.store, &read)?;
        ensure!(
            proof.root() == self.root_id() && proof.collection() == self.collection,
            "legacy proof differs from requested Kernel scope"
        );
        Ok(proof)
    }

    /// Publish saved evidence into a fresh, privately staged service destination.
    /// The native installer owns admission/cutover and must retain the original
    /// archive through failure. This never extracts, watches or scans sources.
    pub fn adopt_saved_legacy(
        &self,
        archive: &VerifiedLegacyArchive,
        limits: &PreparationLimits,
        permit: &airk_store::legacy_adoption::LegacyAdoptionWritePermit,
    ) -> Result<LegacyAdoptionReceipt> {
        // The native catalog issues this only after its real schema-13
        // prewrite fence. Verified saved bytes alone grant no write authority.
        permit.require_store(&self.store)?;
        permit.require_archive(archive)?;
        let prepared = prepare(archive, limits)?;
        ensure!(
            self.root_id() == prepared.root && self.collection == prepared.collection,
            "legacy adoption destination differs from saved scope"
        );
        ensure!(
            self.service_collection.is_some(),
            "legacy adoption requires service storage"
        );
        if let Some(scope) = self.current_scope()? {
            let read = self.store.current_read(&scope)?;
            ensure!(
                !read.has_prepared_update()?,
                "legacy adoption requires native prepared-update recovery before resume"
            );
            if read.resource_reference("legacy-adoption")?.is_some() {
                let proof = airk_store::legacy_adoption::verify_publication(&self.store, &read)?;
                let compatibility_sha256 = airk_core::identity::Sha256Digest::from_bytes(
                    Sha256::digest(serde_json::to_vec(&prepared.compatibility)?).into(),
                );
                ensure!(
                    proof.original_publication_sha256() == prepared.origin.original_root_sha256
                        && proof.archive_inventory_sha256()
                            == prepared.origin.archive_inventory_sha256
                        && proof.compatibility_sha256() == compatibility_sha256
                        && proof.root() == prepared.root
                        && proof.collection() == prepared.collection
                        && proof.roots() == &prepared.roots,
                    "completed legacy adoption differs from this exact frozen archive"
                );
                let token = proof.current_revision_token();
                return Ok(LegacyAdoptionReceipt {
                    original: prepared.origin,
                    current: airk_store::current::CurrentToken {
                        incarnation: token.incarnation,
                        sequence: token.generation,
                        digest: token.revision,
                    },
                    compatibility: prepared.compatibility,
                });
            }
        }
        let writer = self.store.lock()?;
        let mut configuration = airk_store::current::collection::CollectionConfiguration {
            retained_profile: None,
            source_policy_digest: None,
            roots: prepared.roots.clone(),
            extractor_version: archive.snapshot().extractor_version.clone(),
            // This domain records unavailable historical policy; it does not
            // assert equivalence to today's extraction defaults.
            extraction_policy_digest: airk_core::identity::framed_digest(
                b"airk/legacy-adoption-saved-policy/1",
                &[prepared.origin.original_root_sha256.as_bytes()],
            ),
            index_detail: airk_core::extraction::IndexDetail::Detailed,
            vector_selection: Default::default(),
            model: None,
        };
        preserve_saved_configuration(&mut configuration, &prepared.origin);
        let existing = if let Some(scope) = self.current_scope()? {
            let read = self.store.current_build_read(&scope, &writer)?;
            ensure!(
                !read.has_prepared_update()?,
                "legacy adoption requires native prepared-update recovery before resume"
            );
            ensure!(
                read.resource_reference("legacy-adoption")?.is_none(),
                "legacy destination completed concurrently; repeat exact adoption observation"
            );
            let state = read
                .collection_state()?
                .context("legacy bootstrap configuration missing")?;
            ensure!(
                read.head().sequence == 1
                    && read.head().graph_head.live_nodes == 0
                    && state.counts.files == 0
                    && serde_json::to_vec(&state.configuration)?
                        == serde_json::to_vec(&configuration)?
                    && read
                        .lookup_paths(airk_store::current::postings::Lookup::Files, "", None, 1)?
                        .paths
                        .is_empty(),
                "legacy adoption resume requires the exact empty frozen bootstrap"
            );
            Some(scope)
        } else {
            None
        };
        let quota = self.collection_quota()?;
        let reservation = self.publication_reservation_with_limit(limits.max_total_bytes)?;
        let scope = if let Some(scope) = existing {
            scope
        } else {
            self.bootstrap_legacy_adoption(&writer, &quota, &configuration, &reservation)?
        };
        let mut changes = Vec::with_capacity(prepared.artifacts.len());
        for artifact in prepared.artifacts {
            let compact = match artifact {
                PreparedLegacyArtifact::Native {
                    artifact,
                    payload,
                    profile,
                    source,
                    ..
                } => self.store.retain_payload_with_budget_owned(
                    &writer,
                    &artifact,
                    &payload,
                    &profile,
                    Some(&source),
                    Some(&reservation),
                )?,
                PreparedLegacyArtifact::RetainedLegacy {
                    artifact,
                    source,
                    profile,
                    ..
                } => {
                    let origin = airk_core::legacy_adoption::LegacySavedOrigin {
                        schema: 1,
                        archive_inventory_sha256: prepared.origin.archive_inventory_sha256,
                        original_root_sha256: prepared.origin.original_root_sha256,
                        original_path: artifact.path.clone(),
                        source_available: source.is_some(),
                        profile_available: profile.is_some(),
                    };
                    self.store.retain_legacy_artifact_owned(
                        &writer,
                        &artifact,
                        source.as_deref(),
                        profile.as_ref(),
                        origin,
                        &reservation,
                    )?
                }
            };
            changes.push(airk_store::current::ArtifactChange::Upsert(Box::new(
                airk_store::current::CapturedInput::new(compact),
            )));
        }
        let mut resources = Vec::new();
        if !prepared.compatibility.analysis_requires_legacy_retention
            && let Some(analysis) = &prepared.analysis
        {
            let reference = self.store.retain_current_resource_owned(
                &writer,
                &analysis.canonical_bytes()?,
                &reservation,
            )?;
            resources.push(airk_store::current::resources::ResourceChange {
                key: "analysis".into(),
                reference: Some(reference),
            });
        }
        let base = self.store.current_build_read(&scope, &writer)?;
        let target_sequence = base
            .head()
            .sequence
            .checked_add(1)
            .context("legacy target sequence overflow")?;
        drop(base);
        let resource = AdoptionResource {
            schema: 1,
            original: prepared.origin.clone(),
            target_incarnation: scope.incarnation,
            target_sequence,
            root: prepared.root,
            collection: prepared.collection,
            roots: prepared.roots,
            data_root: self
                .data_root
                .canonicalize()?
                .to_str()
                .context("legacy data root is not UTF8")?
                .into(),
            compatibility: prepared.compatibility.clone(),
        };
        let reference = self.store.retain_current_resource_owned(
            &writer,
            &serde_json::to_vec(&resource)?,
            &reservation,
        )?;
        resources.push(airk_store::current::resources::ResourceChange {
            key: "legacy-adoption".into(),
            reference: Some(reference),
        });
        let result = crate::current::CurrentUpdater {
            store: &self.store,
            quota: &quota,
            scope: &scope,
            writer: &writer,
            reservation: &reservation,
            configuration: Some(&configuration),
        }
        .apply_with_resources(&changes, None, &resources)?;
        ensure!(
            result.commit.token.incarnation == scope.incarnation
                && result.commit.token.sequence == target_sequence,
            "legacy adoption publication changed target"
        );
        Ok(LegacyAdoptionReceipt {
            original: prepared.origin,
            current: result.commit.token,
            compatibility: prepared.compatibility,
        })
    }
}

/// The only public input is a Store-authenticated archive capability. A caller
/// cannot substitute a Snapshot and assert that it has already been verified.
pub fn prepare(
    archive: &VerifiedLegacyArchive,
    limits: &PreparationLimits,
) -> Result<PreparedLegacyEvidence> {
    ensure!(
        limits.max_artifacts > 0
            && limits.max_artifact_bytes > 0
            && limits.max_total_bytes >= limits.max_artifact_bytes,
        "invalid legacy preparation limits"
    );
    let snapshot = archive.snapshot();
    let scope = archive.scope();
    ensure!(
        snapshot.root == scope.root
            && snapshot.collection == scope.collection
            && snapshot.roots == scope.roots,
        "legacy archive scope changed"
    );
    ensure!(
        snapshot.artifacts.len() <= limits.max_artifacts,
        "legacy artifact count exceeds preparation limit"
    );
    let mut artifacts = Vec::with_capacity(snapshot.artifacts.len());
    let mut total = 0u64;
    let mut retained_legacy_artifacts = 0usize;
    for path in snapshot.artifacts.keys() {
        let saved = archive.artifact(path, limits.max_artifact_bytes)?;
        ensure!(
            saved.artifact.path == *path,
            "legacy archive artifact crossed path"
        );
        let prepared = prepare_artifact(saved, limits, &mut total)?;
        retained_legacy_artifacts += usize::from(matches!(
            &prepared,
            PreparedLegacyArtifact::RetainedLegacy { .. }
        ));
        artifacts.push(prepared);
    }
    let analysis = snapshot.analysis.as_ref().map(|manifest| AnalysisEvidence {
        manifest: manifest.clone(),
        facts: snapshot.imported_facts.clone(),
        relationships: snapshot.imported_relationships.clone(),
    });
    // An old incomplete producer assertion remains in the authenticated archive.
    // It cannot become a current native producer assertion merely by copying it.
    let analysis_supported = analysis.as_ref().is_none_or(|value| {
        value.manifest.root == scope.root
            && value.manifest.collection == scope.collection
            && value.manifest.extractor_version == snapshot.extractor_version
            && value.manifest.source_hashes == airk_ingest::semantic::source_inventory(snapshot)
            && value.canonical_bytes().is_ok()
    });
    let compatibility = LegacyCompatibility {
        retained_legacy_artifacts,
        unmanifested_imported_facts: if snapshot.analysis.is_none() {
            snapshot.imported_facts.len()
        } else {
            0
        },
        unmanifested_imported_relationships: if snapshot.analysis.is_none() {
            snapshot.imported_relationships.len()
        } else {
            0
        },
        original_embedding_model: snapshot.embedding_model.clone(),
        original_persistent_indexes: snapshot.persistent_indexes,
        retrieval_converted: false,
        requires_archive_retention: true,
        analysis_requires_legacy_retention: !analysis_supported,
    };
    Ok(PreparedLegacyEvidence {
        origin: archive.identity().clone(),
        root: scope.root.clone(),
        collection: scope.collection.clone(),
        roots: scope.roots.clone(),
        artifacts,
        analysis,
        compatibility,
    })
}

fn prepare_artifact(
    saved: SavedLegacyArtifact,
    limits: &PreparationLimits,
    total: &mut u64,
) -> Result<PreparedLegacyArtifact> {
    ensure!(
        saved.artifact.retained.is_none(),
        "legacy archive artifact not hydrated"
    );
    let metadata = serde_json::to_vec(&saved.artifact)?;
    let payload_bytes = saved.payload.as_ref().map(serde_json::to_vec).transpose()?;
    let profile_bytes = saved.profile.as_ref().map(serde_json::to_vec).transpose()?;
    let cost = (metadata.len() as u64)
        .checked_add(
            saved
                .raw_source
                .as_ref()
                .map_or(0, |bytes| bytes.len() as u64),
        )
        .and_then(|bytes| {
            bytes.checked_add(payload_bytes.as_ref().map_or(0, |bytes| bytes.len() as u64))
        })
        .and_then(|bytes| {
            bytes.checked_add(profile_bytes.as_ref().map_or(0, |bytes| bytes.len() as u64))
        })
        .context("legacy preparation size overflow")?;
    ensure!(
        metadata.len() as u64 <= limits.max_artifact_bytes,
        "legacy artifact exceeds byte limit"
    );
    ensure!(
        payload_bytes
            .as_ref()
            .is_none_or(|bytes| bytes.len() as u64 <= limits.max_artifact_bytes),
        "legacy payload exceeds byte limit"
    );
    ensure!(
        profile_bytes
            .as_ref()
            .is_none_or(|bytes| bytes.len() as u64 <= limits.max_artifact_bytes),
        "legacy profile exceeds byte limit"
    );
    ensure!(
        saved
            .raw_source
            .as_ref()
            .is_none_or(|bytes| bytes.len() as u64 <= limits.max_artifact_bytes),
        "legacy source exceeds byte limit"
    );
    *total = total
        .checked_add(cost)
        .context("legacy preparation total overflow")?;
    ensure!(
        *total <= limits.max_total_bytes,
        "legacy preparation aggregate byte limit exceeded"
    );
    let mut source = saved.raw_source.clone();
    let mut source_origin = SavedSourceOrigin::RetainedOriginal;
    if let Some(bytes) = &source {
        verify_source(&saved.artifact, bytes)?;
    } else if saved.artifact.extraction_method == "utf8"
        && saved.artifact.bytes as u64 <= limits.max_artifact_bytes
    {
        let mut bytes = Vec::with_capacity(saved.artifact.bytes);
        for chunk in &saved.artifact.chunks {
            if !bytes
                .len()
                .checked_add(chunk.text.len())
                .is_some_and(|size| size <= saved.artifact.bytes)
            {
                return Ok(retained(
                    saved,
                    None,
                    LegacyRetentionReason::SourceUnavailable,
                ));
            }
            bytes.extend_from_slice(chunk.text.as_bytes());
        }
        if verify_source(&saved.artifact, &bytes).is_ok() {
            *total = total
                .checked_add(bytes.len() as u64)
                .context("legacy preparation total overflow")?;
            ensure!(
                *total <= limits.max_total_bytes,
                "legacy preparation aggregate byte limit exceeded"
            );
            source = Some(bytes);
            source_origin = SavedSourceOrigin::ReconstructedUtf8DigestVerified;
        }
    }
    let Some(bytes) = source else {
        return Ok(retained(
            saved,
            None,
            LegacyRetentionReason::SourceUnavailable,
        ));
    };
    let converted = match (&saved.profile, &saved.payload) {
        (Some(profile), Some(payload)) => Some((profile.clone(), payload.clone())),
        (None, None) => airk_ingest::adapters::legacy::legacy_profile(&saved.artifact)
            .and_then(|profile| {
                let payload = airk_ingest::adapters::legacy::convert_native_legacy(
                    &saved.artifact,
                    &bytes,
                    &profile,
                )?;
                Ok((profile, payload))
            })
            .ok(),
        _ => None,
    };
    let Some((profile, payload)) = converted else {
        return Ok(retained(
            saved,
            Some(bytes),
            LegacyRetentionReason::UnsupportedInterpretation,
        ));
    };
    if profile.chunk_version != 3 || profile.identity().is_err() {
        return Ok(retained(
            saved,
            Some(bytes),
            LegacyRetentionReason::UnsupportedCurrentProfile,
        ));
    }
    if saved.payload.is_none() {
        let payload_size = serde_json::to_vec(&payload)?.len() as u64;
        let profile_size = serde_json::to_vec(&profile)?.len() as u64;
        ensure!(
            payload_size <= limits.max_artifact_bytes && profile_size <= limits.max_artifact_bytes,
            "converted legacy payload exceeds byte limit"
        );
        let generated = payload_size
            .checked_add(profile_size)
            .context("legacy converted size overflow")?;
        *total = total
            .checked_add(generated)
            .context("legacy preparation total overflow")?;
        ensure!(
            *total <= limits.max_total_bytes,
            "legacy preparation aggregate byte limit exceeded"
        );
    }
    let rebound = bind_payload(
        &payload,
        &profile,
        ArtifactBinding {
            path: &saved.artifact.path,
            source_hash: saved.artifact.content_hash.parse()?,
            source_bytes: saved.artifact.bytes,
        },
    );
    if rebound
        .and_then(|artifact| Ok(serde_json::to_vec(&artifact)? == metadata))
        .unwrap_or(false)
    {
        Ok(PreparedLegacyArtifact::Native {
            artifact: saved.artifact,
            payload,
            profile,
            source: bytes,
            source_origin,
        })
    } else {
        Ok(retained(
            saved,
            Some(bytes),
            LegacyRetentionReason::PayloadBindingDiffers,
        ))
    }
}

fn verify_source(artifact: &Artifact, source: &[u8]) -> Result<()> {
    ensure!(
        source.len() == artifact.bytes
            && format!("{:x}", Sha256::digest(source)) == artifact.content_hash,
        "legacy source digest or length mismatch"
    );
    Ok(())
}

fn retained(
    saved: SavedLegacyArtifact,
    source: Option<Vec<u8>>,
    reason: LegacyRetentionReason,
) -> PreparedLegacyArtifact {
    let source_origin = source.as_ref().map(|_| {
        if saved.raw_source.is_some() {
            SavedSourceOrigin::RetainedOriginal
        } else {
            SavedSourceOrigin::ReconstructedUtf8DigestVerified
        }
    });
    PreparedLegacyArtifact::RetainedLegacy {
        artifact: saved.artifact,
        source,
        profile: saved.profile,
        payload: saved.payload,
        source_origin,
        reason,
    }
}

fn preserve_saved_configuration(
    configuration: &mut airk_store::current::collection::CollectionConfiguration,
    origin: &airk_store::legacy_adoption::LegacyPublicationIdentity,
) {
    if let Some(original) = &origin.catalog {
        // Historical inventory predates vector-selection authority. Preserve
        // its actual saved coverage/policy without adopting old model state.
        configuration.index_detail = original.index_detail.unwrap_or_default();
        if let Some(policy) = original.extraction_policy_digest {
            configuration.extraction_policy_digest = policy;
        }
    }
    if let Some(original) = &origin.current {
        // Preserve actual saved coverage/selection and extraction policy;
        // the old model/graph and source-policy admission are only archived,
        // never adopted as new service authority or semantic readiness.
        *configuration = original.collection.configuration.clone();
        configuration.model = None;
        configuration.source_policy_digest = None;
    }
}

#[cfg(test)]
#[path = "legacy_adoption_tests.rs"]
mod tests;
