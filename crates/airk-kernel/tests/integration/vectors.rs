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
    let other = Kernel::open(foreign.path(), Some(data.path().into()))?;
    assert!(other.vectors_enabled().is_err());
    assert!(other.configure_vectors(true).is_err());
    // Corrupt configuration cannot reset a saved opt-out to the enabled default.
    fs::write(
        data.path().join("collections/default/vector-policy.json"),
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
