// SPDX-License-Identifier: Apache-2.0
use airk_core::SearchQuery;
use airk_kernel::Kernel;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn default_vectors_opt_out_is_durable_isolated_and_rebuilds_without_vectors() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
    fs::write(root.path().join("auth.txt"), "authenticate")?;
    let directory = data.path().join("models/local");
    fs::create_dir_all(&directory)?;
    let vocabulary = br#"{"authenticate":[1.0,0.0],"login":[0.99,0.01]}"#;
    fs::write(directory.join("vocabulary.json"), vocabulary)?;
    fs::write(
        directory.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1, "name":"original-vector-default-fixture", "license":"Apache-2.0",
            "dimensions":2, "sha256":format!("{:x}", Sha256::digest(vocabulary))
        }))?,
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    assert!(kernel.vector_status()?.enabled);
    assert!(kernel.vector_status()?.pending_reindex);
    kernel.index()?;
    let query = SearchQuery {
        text: "login".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    };
    assert!(kernel.search(&query)?.hits.is_empty());
    assert_eq!(
        kernel.default_search(&query)?.hits[0].citation.path,
        "auth.txt"
    );
    let generation = kernel.status()?.generation;
    let disabled = kernel.configure_vectors(false)?;
    assert!(disabled.pending_reindex);
    assert_eq!(disabled.default_search_mode, "lexical");
    // The existing MCP/kernel instance observes the setting immediately.
    assert!(kernel.default_search(&query)?.hits.is_empty());
    assert!(kernel.hybrid_search(&query).is_err());
    assert_eq!(kernel.status()?.generation, generation);
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert!(!reopened.vectors_enabled()?);
    let other = Kernel::open_collection(root.path(), Some(data.path().into()), "other")?;
    assert!(other.vector_status()?.enabled);
    kernel.index()?;
    assert_eq!(kernel.status()?.indexed_vectors, 0);
    assert!(!kernel.vector_status()?.pending_reindex);
    assert!(directory.join("model.json").exists());
    kernel.configure_vectors(true)?;
    kernel.index()?;
    assert_eq!(
        kernel.default_search(&query)?.hits[0].citation.path,
        "auth.txt"
    );
    // Failed replacement retains both the old publication and its usable model.
    let generation = kernel.status()?.generation;
    fs::write(directory.join("vocabulary.json"), b"corrupt")?;
    assert!(kernel.index().is_err());
    assert_eq!(kernel.status()?.generation, generation);
    assert!(!kernel.default_search(&query)?.hits.is_empty());
    Ok(())
}

#[test]
fn missing_model_never_silently_disables_default_vectors_and_foreign_policy_is_rejected()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    assert!(kernel.vector_status()?.enabled);
    assert!(kernel.vector_status()?.pending_reindex);
    assert_eq!(kernel.vector_status()?.default_search_mode, "unavailable");
    let query = SearchQuery {
        text: "fixture".into(),
        path_prefix: None,
        limit: 1,
        max_bytes: 100,
    };
    assert!(kernel.default_search(&query).is_err());
    kernel.configure_vectors(false)?;
    assert!(kernel.default_search(&query).is_ok());
    let foreign = tempfile::tempdir()?;
    assert!(Kernel::open(foreign.path(), Some(data.path().into())).is_err());
    // Corrupt configuration cannot reset a saved opt-out to the enabled default.
    fs::write(
        data.path()
            .join("collections-1.2/default/vector-policy.json"),
        b"{}",
    )
    .unwrap();
    assert!(kernel.vectors_enabled().is_err());
    Ok(())
}

#[test]
fn model_installation_respects_quota_and_never_overwrites_existing_inputs() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let source = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let vocabulary = br#"{"login":[1.0,0.0]}"#;
    fs::write(source.path().join("vocabulary.json"), vocabulary)?;
    fs::write(
        source.path().join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1,"name":"original-install-fixture","license":"Apache-2.0","dimensions":2,
            "sha256":format!("{:x}", Sha256::digest(vocabulary))
        }))?,
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.install_vector_model(source.path())?;
    assert_eq!(
        fs::read(data.path().join("models/local/vocabulary.json"))?,
        vocabulary
    );
    assert!(kernel.install_vector_model(source.path()).is_err());
    let disabled = tempfile::tempdir()?;
    let opted_out = Kernel::open(root.path(), Some(disabled.path().into()))?;
    opted_out.configure_vectors(false)?;
    assert!(opted_out.install_vector_model(source.path()).is_err());
    assert!(!disabled.path().join("models/local").exists());
    let limited = tempfile::tempdir()?;
    let kernel = Kernel::open(root.path(), Some(limited.path().into()))?;
    kernel.initialize()?;
    let quota = airk_store::quota::Quota::open(limited.path())?;
    let mut policy = quota.policy()?;
    policy.root_bytes = quota.status()?.used_bytes;
    quota.configure(policy)?;
    assert!(kernel.install_vector_model(source.path()).is_err());
    assert!(!limited.path().join("models/local").exists());
    Ok(())
}

#[test]
fn selection_configuration_preserves_saved_enable_setting_and_collection_scope() -> Result<()> {
    use airk_core::vector_selection::VectorSelectionPolicy;
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    let policy: VectorSelectionPolicy = serde_json::from_value(serde_json::json!({
        "version":1,"exclusions":[{"id":"generated-notes","path_prefix":"generated/","suffix":".txt"}]
    }))?;
    let initial = kernel.vector_selection_status()?;
    let preview = kernel.validate_vector_selection(&policy)?;
    assert_eq!(preview.origin, "preview");
    assert_eq!(kernel.vector_selection_status()?.digest, initial.digest);
    let configured = kernel.configure_vector_selection(policy.clone())?;
    assert_eq!(configured.origin, "explicit");
    assert!(configured.pending_reindex);
    assert!(!kernel.vectors_enabled()?);
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(
        reopened.vector_selection()?.canonical_bytes()?,
        policy.canonical_bytes()?
    );
    assert!(!reopened.vector_status()?.enabled);
    reopened.configure_vectors(true)?;
    assert_eq!(
        reopened.vector_selection_status()?.digest,
        configured.digest
    );
    let other = Kernel::open_collection(root.path(), Some(data.path().into()), "other")?;
    assert!(other.vector_selection()?.exclusions.is_empty());
    let wrong_root = tempfile::tempdir()?;
    let wrong = Kernel::open(wrong_root.path(), Some(data.path().into()))?;
    assert!(wrong.vector_selection_status().is_err());
    assert!(
        wrong
            .configure_vector_selection(VectorSelectionPolicy::default())
            .is_err()
    );
    assert_eq!(
        reopened.vector_selection_status()?.digest,
        configured.digest
    );
    Ok(())
}

#[test]
fn maximum_valid_selection_policy_survives_its_saved_configuration_wrapper() -> Result<()> {
    use airk_core::vector_selection::{
        MAX_POLICY_BYTES, VectorExclusionRule, VectorSelectionPolicy,
    };
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    let mut policy = VectorSelectionPolicy {
        version: 1,
        exclusions: (0..32)
            .map(|index| VectorExclusionRule {
                id: format!("rule-{index:02}"),
                path_prefix: Some("x".repeat(if index < 31 { 4096 } else { 1 })),
                ..Default::default()
            })
            .collect(),
    };
    let remaining = MAX_POLICY_BYTES - serde_json::to_vec(&policy)?.len();
    assert!(remaining < 4096);
    policy.exclusions.last_mut().unwrap().path_prefix = Some("x".repeat(remaining + 1));
    assert_eq!(policy.canonical_bytes()?.len(), MAX_POLICY_BYTES);
    let preview = kernel.validate_vector_selection(&policy)?;
    let saved = kernel.configure_vector_selection(policy)?;
    assert_eq!(saved.digest, preview.digest);
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(reopened.vector_selection_status()?.digest, saved.digest);
    assert!(!reopened.vectors_enabled()?);
    Ok(())
}

#[test]
fn selection_changes_only_vectors_and_file_info_describes_its_committed_revision() -> Result<()> {
    use airk_core::evidence::{EvidenceReadOptions, EvidenceReadStatus};
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
    fs::create_dir(root.path().join("generated"))?;
    let original = b"# Login recovery\nAuthenticate with the recovery key.\n";
    fs::write(root.path().join("generated/auth.txt"), original)?;
    fs::write(root.path().join("keep.txt"), "An invoice payment note.\n")?;
    let directory = data.path().join("models/local");
    fs::create_dir_all(&directory)?;
    let vocabulary = br#"{"authenticate":[1.0,0.0],"recovery":[1.0,0.0],"login":[1.0,0.0],"invoice":[0.0,1.0],"payment":[0.0,1.0]}"#;
    fs::write(directory.join("vocabulary.json"), vocabulary)?;
    fs::write(
        directory.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1,"name":"original-selection-fixture","license":"Apache-2.0","dimensions":2,
            "sha256":format!("{:x}", Sha256::digest(vocabulary))
        }))?,
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let before = kernel.file_info_conditional("generated/auth.txt", &Default::default())?;
    let old_token = before.read.publication.as_ref().unwrap().token;
    let old_info = before.content.as_ref().unwrap();
    assert!(
        old_info.vector_selection.eligible,
        "generated names are not automatically excluded"
    );
    let mut policy: airk_core::vector_selection::VectorSelectionPolicy =
        serde_json::from_value(serde_json::json!({
            "version":1,"exclusions":[
                {"id":"generated-notes","path_prefix":"generated/","suffix":".txt"},
                {"id":"vendor-notes","path_prefix":"vendor/","suffix":".txt"}
            ]
        }))?;
    let configured = kernel.configure_vector_selection(policy.clone())?;
    assert!(configured.pending_reindex);
    assert!(kernel.vector_status()?.selection_pending_reindex);
    let condition = EvidenceReadOptions {
        if_evidence: before.read.validator,
        ..Default::default()
    };
    assert_eq!(
        kernel
            .file_info_conditional("generated/auth.txt", &condition)?
            .read
            .status,
        EvidenceReadStatus::NotModified
    );
    // Failed indexing cannot apply saved eligibility or alter the publication.
    fs::write(directory.join("vocabulary.json"), b"corrupt")?;
    assert!(kernel.index().is_err());
    assert_eq!(kernel.status()?.publication.unwrap().token, old_token);
    assert!(
        kernel
            .file_info("generated/auth.txt")?
            .vector_selection
            .eligible
    );
    assert!(kernel.vector_selection_status()?.pending_reindex);
    kernel.maintain_current()?;
    assert_eq!(kernel.status()?.publication.unwrap().token, old_token);
    fs::write(directory.join("vocabulary.json"), vocabulary)?;
    let report = kernel.index()?;
    assert_eq!(report.indexed, 0);
    assert_eq!(report.reused, 2);
    assert!(!kernel.vector_selection_status()?.pending_reindex);
    let after = kernel.file_info_conditional("generated/auth.txt", &condition)?;
    assert_eq!(after.read.status, EvidenceReadStatus::Content);
    assert_ne!(after.read.validator, before.read.validator);
    let info = after.content.unwrap();
    assert!(!info.vector_selection.eligible);
    assert_eq!(
        info.vector_selection.excluded_by.as_deref(),
        Some("generated-notes")
    );
    assert_eq!(info.vector_selection_digest, configured.digest);
    assert_eq!(
        (info.chunks, info.facts, info.content_hash),
        (
            old_info.chunks,
            old_info.facts,
            old_info.content_hash.clone()
        )
    );
    let query = SearchQuery {
        text: "recovery".into(),
        path_prefix: Some("generated/".into()),
        limit: 10,
        max_bytes: 4096,
    };
    assert!(!kernel.search(&query)?.hits.is_empty());
    assert!(kernel.semantic_search(&query)?.hits.is_empty());
    assert!(
        kernel.read_file("generated/auth.txt", 1, 2, 4096)?.hits[0]
            .text
            .contains("recovery")
    );
    assert!(
        kernel
            .file_info_conditional(
                "generated/auth.txt",
                &EvidenceReadOptions {
                    revision: Some(old_token),
                    ..Default::default()
                }
            )
            .is_err()
    );
    // Rule order is not policy identity and must not trigger a replacement.
    let applied = kernel.status()?.publication.unwrap().token;
    policy.exclusions.reverse();
    let reordered = kernel.configure_vector_selection(policy)?;
    assert_eq!(reordered.digest, configured.digest);
    assert!(!reordered.pending_reindex);
    kernel.index()?;
    assert_eq!(kernel.status()?.publication.unwrap().token, applied);
    assert_eq!(fs::read(root.path().join("generated/auth.txt"))?, original);
    Ok(())
}

#[test]
fn selection_can_be_saved_before_initial_index_and_matches_aliased_paths() -> Result<()> {
    let root = tempfile::tempdir()?;
    let external = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("same.txt"), "original local note")?;
    fs::write(external.path().join("same.txt"), "original external note")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.add_root("extra", external.path())?;
    let policy = serde_json::from_value(serde_json::json!({
        "version":1,"exclusions":[{"id":"extra-only","path_prefix":"@extra/","basename":"same.txt"}]
    }))?;
    let configured = kernel.configure_vector_selection(policy)?;
    assert!(configured.pending_reindex);
    assert!(!kernel.vector_status()?.enabled);
    assert!(kernel.vector_status()?.applied_selection_digest.is_none());
    kernel.reindex()?;
    assert!(!kernel.vectors_enabled()?);
    assert!(!kernel.vector_selection_status()?.pending_reindex);
    assert!(kernel.file_info("same.txt")?.vector_selection.eligible);
    assert_eq!(
        kernel
            .file_info("@extra/same.txt")?
            .vector_selection
            .excluded_by
            .as_deref(),
        Some("extra-only")
    );
    assert_eq!(
        fs::read_to_string(external.path().join("same.txt"))?,
        "original external note"
    );
    Ok(())
}
