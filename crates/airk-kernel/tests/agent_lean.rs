// SPDX-License-Identifier: Apache-2.0
use airk_core::{
    SearchQuery, compression::CompressionMode, extraction::IndexDetail, readiness::DiscoveryState,
};
use airk_kernel::Kernel;
use anyhow::Result;
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

fn query(path: &str) -> SearchQuery {
    SearchQuery {
        text: "authenticate".into(),
        path_prefix: Some(path.into()),
        limit: 5,
        max_bytes: 8192,
    }
}

fn unlinked_tempdir() -> Result<tempfile::TempDir> {
    // Model guards require real ancestors; macOS exposes temporary paths through /var.
    Ok(tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?)
}

#[test]
fn lean_preserves_declarations_text_and_policy_omissions() -> Result<()> {
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    fs::write(
        root.path().join("auth.rs"),
        "fn authenticate(name: &str) -> bool { println!(\"login\"); !name.is_empty() }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.configure_index_detail(IndexDetail::AgentLean)?;
    kernel.index()?;
    assert_eq!(kernel.status()?.indexing.level, IndexDetail::AgentLean);
    let facts = kernel.outline("auth.rs", 100, 65536)?;
    assert!(facts.facts.iter().any(|f| f.name == "authenticate"
        && f.code.as_ref().is_some_and(|c| {
            c.signature.as_ref().is_some_and(|s| s.contains("-> bool")) && c.fingerprints.is_none()
        })));
    assert!(!facts.facts.iter().any(|f| f.kind == "call"));
    assert!(!kernel.search(&query("auth.rs"))?.hits.is_empty());
    assert_eq!(kernel.index()?.current.unwrap().changed_artifacts, 0);
    kernel.configure_index_detail(IndexDetail::Essential)?;
    kernel.index()?;
    assert!(kernel.outline("auth.rs", 10, 8192)?.facts.is_empty());
    assert!(!kernel.search(&query("auth.rs"))?.hits.is_empty());
    kernel.configure_index_detail(IndexDetail::Balanced)?;
    kernel.index()?;
    assert!(
        kernel
            .outline("auth.rs", 100, 65536)?
            .facts
            .iter()
            .any(|f| f.code.as_ref().is_some_and(|c| c.fingerprints.is_some()))
    );
    Ok(())
}

#[test]
fn repack_preserves_evidence_and_is_idempotent_in_both_directions() -> Result<()> {
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    let source = format!(
        "fn authenticate() -> bool {{\n{}true\n}}\n",
        "// original retained login evidence\r\n".repeat(3000)
    );
    fs::write(root.path().join("auth.rs"), &source)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.configure_index_detail(IndexDetail::AgentLean)?;
    kernel.index()?;
    let before = kernel.search(&query("auth.rs"))?.hits[0].citation.clone();
    let raw = kernel.compression_status()?;
    kernel.configure_compression(CompressionMode::Automatic)?;
    assert_eq!(kernel.repack_current(None, 32)?.changed, 1);
    let encoded = kernel.compression_status()?;
    assert!(encoded.compressed_objects > 0);
    assert!(encoded.unique_encoded_bytes < raw.unique_encoded_bytes);
    assert_eq!(
        kernel.search(&query("auth.rs"))?.hits[0]
            .citation
            .content_hash,
        before.content_hash
    );
    assert_eq!(kernel.repack_current(None, 32)?.changed, 0);
    drop(kernel);
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert!(
        reopened.read_file("auth.rs", 1, 1, 8192)?.hits[0]
            .text
            .starts_with("fn authenticate")
    );
    reopened.configure_compression(CompressionMode::Off)?;
    assert_eq!(reopened.repack_current(None, 32)?.changed, 1);
    assert_eq!(reopened.compression_status()?.compressed_objects, 0);
    assert_eq!(fs::read_to_string(root.path().join("auth.rs"))?, source);
    Ok(())
}

#[test]
fn compressed_continuity_metadata_survives_reopen_update_and_cleanup() -> Result<()> {
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    let mut source: String = (0..80)
        .map(|n| format!("pub fn authenticate_{n}() -> usize {{ {n} }}\n"))
        .collect();
    fs::write(root.path().join("auth.rs"), &source)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.configure_index_detail(IndexDetail::Balanced)?;
    kernel.configure_compression(CompressionMode::Automatic)?;
    kernel.index()?;
    let metadata = fs::read_dir(data.path().join("collections-1.2/default/current-metadata"))?
        .collect::<std::io::Result<Vec<_>>>()?;
    assert!(
        metadata.iter().any(|entry| {
            fs::read(entry.path()).is_ok_and(|bytes| bytes.starts_with(b"AIRKCF01"))
        })
    );
    drop(kernel);
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(kernel.index()?.current.unwrap().changed_artifacts, 0);
    source.insert_str(0, "pub fn authenticate_added() -> bool { true }\n");
    fs::write(root.path().join("auth.rs"), source)?;
    let updated = kernel.index()?;
    assert!(updated.maintenance_error.is_none());
    assert!(
        kernel
            .outline("auth.rs", 100, 65536)?
            .facts
            .iter()
            .any(|f| f.name == "authenticate_added")
    );
    fs::remove_file(root.path().join("auth.rs"))?;
    assert!(kernel.index()?.maintenance_error.is_none());
    assert_eq!(kernel.status()?.files, 0);
    Ok(())
}

#[test]
fn paused_update_keeps_prior_paths_until_a_complete_rescan() -> Result<()> {
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    for index in 0..40 {
        fs::write(
            root.path().join(format!("file_{index:02}.rs")),
            format!("fn authenticate_{index}() {{}}\n"),
        )?;
    }
    fs::write(
        root.path().join("old_only.txt"),
        "authenticate prior evidence\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.configure_index_detail(IndexDetail::AgentLean)?;
    kernel.index()?;
    fs::remove_file(root.path().join("old_only.txt"))?;
    for index in 0..40 {
        fs::write(
            root.path().join(format!("file_{index:02}.rs")),
            format!("fn authenticate_{index}() {{ /* changed */ }}\n"),
        )?;
    }
    let control = kernel.clone();
    let paused = Arc::new(AtomicBool::new(false));
    let signal = paused.clone();
    let observed = kernel.clone().with_index_progress(move |event| {
        if event.phase == "base_committed" && !signal.swap(true, Ordering::SeqCst) {
            control.schedule_pause(true).expect("pause scheduler");
        }
    });
    assert!(observed.scheduled_index(false, false).is_err());
    assert!(paused.load(Ordering::SeqCst));
    assert_eq!(kernel.run_scheduled_once()?.state, "idle");
    assert_eq!(
        kernel.status()?.discovery_readiness.state,
        DiscoveryState::Paused
    );
    assert!(
        !kernel
            .read_file("old_only.txt", 1, 1, 4096)?
            .hits
            .is_empty()
    );
    kernel.schedule_pause(false)?;
    kernel.scheduled_index(false, false)?;
    assert_eq!(
        kernel.status()?.discovery_readiness.state,
        DiscoveryState::Ready
    );
    assert_eq!(kernel.status()?.files, 40);
    assert!(kernel.read_file("old_only.txt", 1, 1, 4096).is_err());
    Ok(())
}

#[test]
fn repack_crash_child() -> Result<()> {
    let Ok(root) = std::env::var("AIRK_REPACK_TEST_ROOT") else {
        return Ok(());
    };
    let data = std::env::var("AIRK_REPACK_TEST_DATA")?;
    let stage = std::env::var("AIRK_REPACK_TEST_STAGE")?;
    let kernel = Kernel::open(root, Some(data.into()))?;
    kernel.repack_current_observed(None, 32, &mut |current| {
        if format!("{current:?}") == stage {
            std::process::exit(73)
        }
    })?;
    Ok(())
}

#[test]
fn repack_keeps_graph_and_lexical_identity_with_vectors() -> Result<()> {
    use sha2::{Digest, Sha256};
    let root = unlinked_tempdir()?;
    let data = unlinked_tempdir()?;
    let model = data.path().join("models/local");
    fs::create_dir_all(&model)?;
    let vocabulary = br#"{"authenticate":[1.0,0.0],"login":[0.0,1.0]}"#;
    fs::write(model.join("vocabulary.json"), vocabulary)?;
    fs::write(
        model.join("model.json"),
        serde_json::to_vec(
            &serde_json::json!({"schema":1,"name":"original-repack-fixture","license":"Apache-2.0","dimensions":2,"sha256":format!("{:x}",Sha256::digest(vocabulary))}),
        )?,
    )?;
    fs::write(
        root.path().join("auth.txt"),
        "authenticate login original evidence\n".repeat(500),
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(IndexDetail::AgentLean)?;
    kernel.index()?;
    let store = airk_store::FileStore::new(data.path().join("collections-1.2/default"));
    let scope = store
        .current_scope(&kernel.root().to_string_lossy(), "default")?
        .unwrap();
    let before = store.current_read(&scope)?.head().clone();
    assert!(kernel.status()?.indexed_vectors > 0);
    kernel.configure_compression(CompressionMode::Automatic)?;
    kernel.repack_current(None, 32)?;
    let after = store.current_read(&scope)?.head().clone();
    assert_eq!(before.graph_head, after.graph_head);
    assert_eq!(before.lexical_commit, after.lexical_commit);
    assert_eq!(before.lexical_sequence, after.lexical_sequence);
    assert!(!kernel.semantic_search(&query("auth.txt"))?.hits.is_empty());
    Ok(())
}

#[test]
fn repack_recovers_twice_at_each_journal_boundary() -> Result<()> {
    for stage in ["Prepared", "Applying", "Committed"] {
        let root = unlinked_tempdir()?;
        let data = unlinked_tempdir()?;
        fs::write(
            root.path().join("auth.rs"),
            format!(
                "fn authenticate() {{\n{} }}\n",
                "// original bounded recovery\n".repeat(500)
            ),
        )?;
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.configure_vectors(false)?;
        kernel.configure_index_detail(IndexDetail::AgentLean)?;
        kernel.index()?;
        kernel.configure_compression(CompressionMode::Automatic)?;
        drop(kernel);
        let status = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "repack_crash_child", "--nocapture"])
            .env("AIRK_REPACK_TEST_ROOT", root.path())
            .env("AIRK_REPACK_TEST_DATA", data.path())
            .env("AIRK_REPACK_TEST_STAGE", stage)
            .status()?;
        assert_eq!(status.code(), Some(73), "{stage}");
        for _ in 0..2 {
            let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
            assert!(!kernel.search(&query("auth.rs"))?.hits.is_empty());
            kernel.repack_current(None, 32)?;
            kernel.maintain_current()?;
        }
    }
    Ok(())
}
