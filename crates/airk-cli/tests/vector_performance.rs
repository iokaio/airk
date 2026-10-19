// SPDX-License-Identifier: Apache-2.0
//! Explicit real-model qualification; no model downloads during tests.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn run(root: &Path, data: &Path, workers: &str, args: &[&str]) -> Result<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_airk"))
        .arg("--root")
        .arg(root)
        .arg("--data-dir")
        .arg(data)
        .env("AIRK_USAGE_MODE", "disabled")
        .env("AIRK_INDEX_WORKERS", workers)
        .args(args)
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
#[ignore = "requires AIRK_TEST_MODEL_DIR with verified local ONNX assets"]
fn serial_parallel_and_incremental_vectors_preserve_retrieval() -> Result<()> {
    let base = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let root = base.path().join("workspace");
    let data = base.path().join("state");
    fs::create_dir(&root)?;
    let source: String = (0..36).map(|n| format!(
        "/// Recover account {n} using a time-limited password reset link.\npub fn recover_{n}() -> usize {{ {n} }}\n\n"
    )).collect();
    fs::write(root.join("recovery.rs"), &source)?;
    let model = std::env::var("AIRK_TEST_MODEL_DIR")?;
    run(
        &root,
        &data,
        "1",
        &["collections", "vectors", "install", &model],
    )?;
    let serial = run(&root, &data, "1", &["index"])?;
    ensure!(
        serial["embedding_workers"] == 1 && serial["embedding_batches"].as_u64().unwrap_or(0) >= 2,
        "fixture did not exercise multiple batches: {serial}"
    );
    let query = ["search", "recover account password", "--mode", "vector"];
    let before = run(&root, &data, "1", &query)?;
    let count = run(&root, &data, "1", &["status"])?["indexed_vectors"].clone();
    let parallel = run(&root, &data, "4", &["index", "--full"])?;
    ensure!(
        parallel["embedding_cache_hits"] == 0,
        "full reindex used cached inference"
    );
    ensure!(
        parallel["embedding_inference_us"].as_u64().unwrap_or(0) > 0
            && parallel["vector_graph_insert_us"].as_u64().unwrap_or(0) > 0,
        "phase measurements missing"
    );
    ensure!(
        run(&root, &data, "4", &["status"])?["indexed_vectors"] == count,
        "vector count changed"
    );
    ensure!(
        run(&root, &data, "4", &query)?["hits"] == before["hits"],
        "parallel graph changed evidence"
    );
    fs::write(
        root.join("bread.txt"),
        "Mix flour and yeast to bake a loaf of bread.",
    )?;
    let incremental = run(&root, &data, "4", &["index"])?;
    ensure!(
        incremental["embedding_cache_hits"]
            .as_u64()
            .context("hits missing")?
            >= count.as_u64().context("count missing")?,
        "unchanged vectors did not reuse cache"
    );
    ensure!(
        incremental["embedding_inferences"]
            .as_u64()
            .unwrap_or(u64::MAX)
            < parallel["embedding_inferences"].as_u64().unwrap_or(0),
        "incremental inference did not decrease"
    );
    ensure!(
        fs::read_to_string(root.join("recovery.rs"))? == source,
        "source changed"
    );
    eprintln!("serial={serial}\nparallel={parallel}\nincremental={incremental}");
    Ok(())
}
