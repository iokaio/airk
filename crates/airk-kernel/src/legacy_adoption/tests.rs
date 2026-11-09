// SPDX-License-Identifier: Apache-2.0
use super::*;

fn saved_text(bytes: &[u8]) -> Result<SavedLegacyArtifact> {
    Ok(SavedLegacyArtifact {
        artifact: Artifact {
            path: "saved.txt".into(),
            content_hash: format!("{:x}", Sha256::digest(bytes)),
            bytes: bytes.len(),
            chunks: airk_ingest::chunk(std::str::from_utf8(bytes)?),
            language: "text".into(),
            extraction_method: "utf8".into(),
            parse_status: "not-applicable".into(),
            facts: vec![],
            retained: None,
        },
        raw_source: None,
        profile: None,
        payload: None,
    })
}

#[test]
fn saved_native_text_reconstructs_proven_source_and_preserves_exact_chunks() -> Result<()> {
    let original = b"original first line\nsecond saved line\n";
    let saved = saved_text(original)?;
    let exact_artifact = serde_json::to_value(&saved.artifact)?;
    let prepared = prepare_artifact(saved, &PreparationLimits::default(), &mut 0)?;
    let PreparedLegacyArtifact::Native {
        artifact,
        payload,
        profile,
        source,
        source_origin,
    } = prepared
    else {
        anyhow::bail!("proved native evidence was not converted")
    };
    assert_eq!(source, original);
    assert_eq!(
        source_origin,
        SavedSourceOrigin::ReconstructedUtf8DigestVerified
    );
    assert_eq!(serde_json::to_value(&artifact)?, exact_artifact);
    let bound = bind_payload(
        &payload,
        &profile,
        ArtifactBinding {
            path: &artifact.path,
            source_hash: artifact.content_hash.parse()?,
            source_bytes: artifact.bytes,
        },
    )?;
    assert_eq!(serde_json::to_value(bound)?, exact_artifact);
    Ok(())
}

#[test]
fn missing_binary_source_retains_extraction_without_inventing_profile_or_raw_bytes() -> Result<()> {
    let mut saved = saved_text(b"retained extracted page\n")?;
    saved.artifact.extraction_method = "pdf-text".into();
    saved.artifact.language = "pdf".into();
    saved.artifact.bytes = 4123;
    saved.artifact.content_hash = "11".repeat(32);
    let original = serde_json::to_value(&saved.artifact)?;
    let prepared = prepare_artifact(saved, &PreparationLimits::default(), &mut 0)?;
    let PreparedLegacyArtifact::RetainedLegacy {
        artifact,
        source,
        profile,
        payload,
        source_origin,
        reason,
    } = prepared
    else {
        anyhow::bail!("missing binary source was invented")
    };
    assert_eq!(serde_json::to_value(artifact)?, original);
    assert!(source.is_none() && profile.is_none() && payload.is_none());
    assert_eq!(source_origin, None);
    assert_eq!(reason, LegacyRetentionReason::SourceUnavailable);
    Ok(())
}

#[test]
fn incorrect_saved_text_digest_is_preserved_as_legacy_not_blessed_as_raw() -> Result<()> {
    let mut saved = saved_text(b"retained bytes\n")?;
    saved.artifact.content_hash = "12".repeat(32);
    let original = serde_json::to_value(&saved.artifact)?;
    let prepared = prepare_artifact(saved, &PreparationLimits::default(), &mut 0)?;
    let PreparedLegacyArtifact::RetainedLegacy {
        artifact,
        source,
        reason,
        ..
    } = prepared
    else {
        anyhow::bail!("mismatched text became native source")
    };
    assert_eq!(serde_json::to_value(artifact)?, original);
    assert!(source.is_none());
    assert_eq!(reason, LegacyRetentionReason::SourceUnavailable);
    Ok(())
}

#[test]
fn inconsistent_chunk_locations_keep_original_evidence_outside_native_capture() -> Result<()> {
    let bytes = b"retained bytes\n";
    let mut saved = saved_text(bytes)?;
    saved.raw_source = Some(bytes.to_vec());
    saved.artifact.chunks[0].start_line = 7;
    saved.artifact.chunks[0].end_line = 7;
    let original = serde_json::to_value(&saved.artifact)?;
    let prepared = prepare_artifact(saved, &PreparationLimits::default(), &mut 0)?;
    let PreparedLegacyArtifact::RetainedLegacy {
        artifact,
        source,
        profile,
        reason,
        ..
    } = prepared
    else {
        anyhow::bail!("inconsistent locations became native extraction")
    };
    assert_eq!(serde_json::to_value(artifact)?, original);
    assert_eq!(source.as_deref(), Some(bytes.as_slice()));
    assert!(profile.is_none());
    assert_eq!(reason, LegacyRetentionReason::UnsupportedInterpretation);
    Ok(())
}

#[test]
fn a_claimed_retained_source_digest_mismatch_fails_closed() -> Result<()> {
    let mut saved = saved_text(b"original bytes\n")?;
    saved.raw_source = Some(b"altered bytes!\n".to_vec());
    assert!(prepare_artifact(saved, &PreparationLimits::default(), &mut 0).is_err());
    Ok(())
}

#[test]
fn incompatible_binding_keeps_the_original_authoritative_payload_and_profile() -> Result<()> {
    let bytes = b"retained original bytes\n";
    let mut saved = saved_text(bytes)?;
    let profile = airk_ingest::adapters::legacy::legacy_profile(&saved.artifact)?;
    let payload =
        airk_ingest::adapters::legacy::convert_native_legacy(&saved.artifact, bytes, &profile)?;
    let original_payload = serde_json::to_value(&payload)?;
    let original_profile = serde_json::to_value(&profile)?;
    saved.profile = Some(profile);
    saved.payload = Some(payload);
    saved.raw_source = Some(bytes.to_vec());
    saved.artifact.parse_status = "historical interpretation differs".into();
    let original = serde_json::to_value(&saved.artifact)?;
    let prepared = prepare_artifact(saved, &PreparationLimits::default(), &mut 0)?;
    let PreparedLegacyArtifact::RetainedLegacy {
        artifact,
        source,
        profile,
        payload,
        reason,
        ..
    } = prepared
    else {
        anyhow::bail!("different saved binding became native evidence")
    };
    assert_eq!(serde_json::to_value(artifact)?, original);
    assert_eq!(
        serde_json::to_value(profile.context("profile lost")?)?,
        original_profile
    );
    assert_eq!(
        serde_json::to_value(payload.context("payload lost")?)?,
        original_payload
    );
    assert_eq!(source.as_deref(), Some(bytes.as_slice()));
    assert_eq!(reason, LegacyRetentionReason::PayloadBindingDiffers);
    Ok(())
}

#[test]
fn preparation_counts_reconstructed_source_against_original_aggregate_budget() -> Result<()> {
    let saved = saved_text(b"retained bytes\n")?;
    let metadata = serde_json::to_vec(&saved.artifact)?.len() as u64;
    let limits = PreparationLimits {
        max_artifacts: 1,
        max_artifact_bytes: metadata,
        max_total_bytes: metadata,
    };
    assert!(prepare_artifact(saved, &limits, &mut 0).is_err());
    Ok(())
}

// This positive journey deliberately requires the real native preparation
// issuer. It is not a passing integration gate until that issuer is supplied.
#[allow(dead_code)]
fn actual_current_adoption_reads_saved_extraction_after_original_source_disappears(
    issue_native_permit: impl FnOnce(
        &crate::Kernel,
        &VerifiedLegacyArchive,
    )
        -> Result<airk_store::legacy_adoption::LegacyAdoptionWritePermit>,
) -> Result<()> {
    use airk_core::SnapshotStore;
    use airk_store::legacy_adoption::{CaptureLimits, LegacyScope, capture_tree};
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let validation = tempfile::tempdir()?;
    let root_path = root.path().canonicalize()?;
    let mut saved = saved_text(b"retained extracted evidence\n")?;
    saved.artifact.extraction_method = "pdf-text".into();
    saved.artifact.language = "pdf".into();
    saved.artifact.bytes = 4123;
    saved.artifact.content_hash = "11".repeat(32);
    let exact = serde_json::to_value(&saved.artifact)?;
    let mut snapshot = airk_core::Snapshot::empty(root_path.to_str().context("root UTF8")?.into());
    snapshot.collection = "adopted".into();
    snapshot.generation = 7;
    snapshot.embedding_model = Some("historical-saved-pipeline".into());
    snapshot
        .artifacts
        .insert(saved.artifact.path.clone(), saved.artifact);
    airk_store::FileStore::new(validation.path()).save(&snapshot)?;
    let capture_limits = CaptureLimits {
        max_files: 100,
        max_bytes: 16 << 20,
        max_file_bytes: 16 << 20,
        max_depth: 16,
        max_snapshot_bytes: 16 << 20,
    };
    let inventory = capture_tree(
        validation.path(),
        capture_limits,
        Instant::now() + Duration::from_secs(10),
    )?;
    let archive = VerifiedLegacyArchive::open_validation_copy(
        validation.path(),
        LegacyScope {
            root: snapshot.root.clone(),
            collection: snapshot.collection.clone(),
            roots: Default::default(),
        },
        &inventory,
        capture_limits,
        Instant::now() + Duration::from_secs(10),
    )?;
    let kernel =
        crate::Kernel::open_service_collection(&root_path, Some(data.path().into()), "adopted")?;
    let permit = issue_native_permit(&kernel, &archive)?;
    root.close()?;
    let receipt = kernel.adopt_saved_legacy(&archive, &PreparationLimits::default(), &permit)?;
    assert!(!root_path.exists(), "adoption recreated original source");
    assert_eq!(receipt.original.generation, 7);
    assert_eq!(receipt.compatibility.retained_legacy_artifacts, 1);
    assert!(!receipt.compatibility.retrieval_converted);
    assert_eq!(
        receipt.compatibility.original_embedding_model.as_deref(),
        Some("historical-saved-pipeline")
    );
    let read = kernel.current_reader()?;
    let compact = read
        .artifact("saved.txt")?
        .context("adopted artifact missing")?;
    let origin = compact
        .retained
        .as_ref()
        .context("retained missing")?
        .legacy_origin
        .as_ref()
        .context("legacy origin missing")?;
    assert!(!origin.source_available && !origin.profile_available);
    assert_eq!(
        serde_json::to_value(kernel.store.load_legacy_saved_artifact(&compact)?)?,
        exact
    );
    drop(read);
    let lines = kernel.read_file("saved.txt", 1, 1, 4096)?;
    assert_eq!(lines.hits[0].text, "retained extracted evidence\n");
    let info = kernel.file_info("saved.txt")?;
    let compatibility = info
        .legacy_saved_evidence
        .context("saved file compatibility missing")?;
    assert!(!compatibility.origin.source_available);
    assert!(!compatibility.vector_selection_available && !compatibility.profile_families_available);
    assert_eq!(
        info.line_basis,
        "saved-extracted-text; cite original location"
    );
    let search = kernel.search(&airk_core::SearchQuery {
        text: "retained".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    })?;
    assert_eq!(search.hits.len(), 1);
    assert_eq!(search.hits[0].citation.path, "saved.txt");
    assert_eq!(search.hits[0].text, "retained extracted evidence\n");
    assert!(!root_path.exists());
    Ok(())
}

#[test]
fn fresh_service_storage_cannot_forge_a_verified_legacy_publication() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let kernel =
        crate::Kernel::open_service_collection(root.path(), Some(data.path().into()), "adopted")?;
    assert!(kernel.verify_legacy_adoption_publication().is_err());
    assert!(kernel.current_scope()?.is_none());
    Ok(())
}

#[test]
fn legacy_adoption_historical_configuration_preserves_coverage_and_full_original_metadata()
-> Result<()> {
    use airk_core::{
        extraction::IndexDetail,
        identity::{Incarnation, PublicationReceipt, RevisionToken, Sha256Digest},
    };
    use airk_store::legacy_adoption::{LegacyPublicationIdentity, OriginalCatalogPublication};
    let digest = Sha256Digest::from_bytes([7; 32]);
    let mut configuration = airk_store::current::collection::CollectionConfiguration {
        retained_profile: None,
        source_policy_digest: None,
        roots: Default::default(),
        extractor_version: "historical".into(),
        extraction_policy_digest: digest,
        index_detail: IndexDetail::Detailed,
        vector_selection: Default::default(),
        model: None,
    };
    let mut origin = LegacyPublicationIdentity {
        header: None,
        current: None,
        catalog: None,
        generation: 1,
        original_root_sha256: digest,
        archive_inventory_sha256: digest,
    };
    preserve_saved_configuration(&mut configuration, &origin);
    assert_eq!(configuration.index_detail, IndexDetail::Detailed);
    let policy = Sha256Digest::from_bytes([8; 32]);
    origin.catalog = Some(OriginalCatalogPublication {
        receipt: PublicationReceipt {
            token: RevisionToken {
                incarnation: Incarnation::from_bytes([9; 16]),
                generation: 1,
                revision: digest,
            },
            inventory_digest: digest,
            manifest_digest: digest,
        },
        manifest_schema: 3,
        engine: "historical-engine".into(),
        inventory_version: 3,
        index_detail: Some(IndexDetail::Balanced),
        extraction_policy_digest: Some(policy),
        embedding_pipeline_digest: Some(digest),
        embedding_model: Some(digest.to_string()),
        components: Default::default(),
        resource_conversion: "original model archived only".into(),
    });
    let encoded = serde_json::to_value(&origin)?;
    let decoded: LegacyPublicationIdentity = serde_json::from_value(encoded.clone())?;
    assert_eq!(serde_json::to_value(&decoded)?, encoded);
    preserve_saved_configuration(&mut configuration, &decoded);
    assert_eq!(configuration.index_detail, IndexDetail::Balanced);
    assert_eq!(configuration.extraction_policy_digest, policy);
    assert!(configuration.model.is_none());
    assert!(configuration.source_policy_digest.is_none());
    assert_eq!(
        encoded["catalog"]["embedding_pipeline_digest"],
        digest.to_string()
    );
    Ok(())
}
