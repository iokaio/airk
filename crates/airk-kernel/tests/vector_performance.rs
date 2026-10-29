// SPDX-License-Identifier: Apache-2.0
//! Kernel inference/cache qualification, separate from native service-client tests.
//! Run with local-inference and verified AIRK_TEST_MODEL_DIR; tests never download assets.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{fs, path::Path, process::Command};

// These counters describe the kernel's local inference/cache implementation. Run
// it in an isolated test process; public CLI work is brokered and asynchronous.
fn run(root: &Path, data: &Path, workers: &str, args: &[&str]) -> Result<Value> {
    let result = tempfile::NamedTempFile::new()?;
    let output = Command::new(std::env::current_exe()?)
        .args(["--exact", "component_adapter", "--nocapture"])
        .env(
            "AIRK_PERF_REQUEST",
            serde_json::to_string(&(root, data, args))?,
        )
        .env("AIRK_PERF_RESULT", result.path())
        .env("AIRK_USAGE_MODE", "disabled")
        .env("AIRK_INDEX_WORKERS", workers)
        .output()?;
    ensure!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    Ok(serde_json::from_slice(&fs::read(result.path())?)?)
}

#[test]
fn component_adapter() -> Result<()> {
    let Some(request) = std::env::var_os("AIRK_PERF_REQUEST") else {
        return Ok(());
    };
    let (root, data, args): (std::path::PathBuf, std::path::PathBuf, Vec<String>) =
        serde_json::from_str(request.to_str().context("invalid component request")?)?;
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    let kernel = airk_kernel::Kernel::open_collection(root, Some(data), "component-performance")?;
    let value = match args.as_slice() {
        ["collections", "vectors", "install", model] => {
            serde_json::to_value(kernel.install_vector_model(Path::new(model))?)?
        }
        [
            "collections",
            "vectors",
            "selection",
            "set",
            "--file",
            policy,
        ] => serde_json::to_value(
            kernel.configure_vector_selection(serde_json::from_slice(&fs::read(policy)?)?)?,
        )?,
        ["index"] => serde_json::to_value(kernel.index()?)?,
        ["index", "--full"] => serde_json::to_value(kernel.reindex()?)?,
        ["status"] => serde_json::to_value(kernel.status()?)?,
        ["info", path] => serde_json::to_value(kernel.file_info(path)?)?,
        [
            "search",
            text,
            "--mode",
            mode @ ("vector" | "lexical"),
            rest @ ..,
        ] => {
            let prefix = match rest {
                [] => None,
                ["--path-prefix", value] => Some((*value).to_owned()),
                _ => anyhow::bail!("unsupported component search"),
            };
            let query = airk_core::SearchQuery {
                text: (*text).into(),
                path_prefix: prefix,
                limit: 10,
                max_bytes: 4096,
            };
            serde_json::to_value(if *mode == "vector" {
                kernel.semantic_search(&query)?
            } else {
                kernel.search(&query)?
            })?
        }
        _ => anyhow::bail!("unsupported component operation: {args:?}"),
    };
    fs::write(
        std::env::var_os("AIRK_PERF_RESULT").context("component result missing")?,
        serde_json::to_vec(&value)?,
    )?;
    Ok(())
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
    fs::write(root.join("initial-blank.txt"), " \n")?;
    let model = std::env::var("AIRK_TEST_MODEL_DIR")?;
    run(
        &root,
        &data,
        "1",
        &["collections", "vectors", "install", &model],
    )?;
    let serial = run(&root, &data, "1", &["index"])?;
    ensure!(
        serial["embedding_inputs"]
            .as_u64()
            .context("inputs missing")?
            == serial["embedding_cache_hits"]
                .as_u64()
                .context("hits missing")?
                + serial["embedding_inferences"]
                    .as_u64()
                    .context("evaluations missing")?,
        "per-input work counters do not reconcile"
    );
    ensure!(
        serial["embedding_actual_forwards"]
            .as_u64()
            .context("forwards missing")?
            > 0
            && serial["embedding_blank_inputs"]
                .as_u64()
                .context("blanks missing")?
                > 0,
        "fixture must distinguish neural work from the blank source"
    );
    ensure!(
        serial["embedding_tokenized_inputs"]
            .as_u64()
            .context("tokenized inputs missing")?
            >= serial["embedding_actual_forwards"]
                .as_u64()
                .context("forwards missing")?
            && serial["embedding_input_tokens"]
                .as_u64()
                .context("input tokens missing")?
                >= serial["embedding_embedded_tokens"]
                    .as_u64()
                    .context("embedded tokens missing")?
            && serial["embedding_padding_tokens"]
                .as_u64()
                .context("padding missing")?
                > 0
            && serial["embedding_plan_compilations"]
                .as_u64()
                .context("compilations missing")?
                == 0,
        "token and plan measurements do not reconcile"
    );
    ensure!(
        serial["embedding_workers"] == 1 && serial["embedding_batches"].as_u64().unwrap_or(0) > 0,
        "fixture did not exercise serial batching: {serial}"
    );
    ensure!(
        serial["vector_units"] == serial["embedding_inputs"]
            && serial["vector_body_bytes"].as_u64() == Some((source.len() + 2) as u64)
            && serial["embedding_inputs"].as_u64().unwrap_or(u64::MAX) < 72
            && serial["embedding_truncated_inputs"] == 0,
        "semantic planning must cover the complete body with fewer inputs and no truncation: {serial}"
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
        incremental["embedding_actual_forwards"]
            .as_u64()
            .context("forwards missing")?
            < parallel["embedding_actual_forwards"]
                .as_u64()
                .context("forwards missing")?,
        "incremental neural work did not decrease"
    );
    ensure!(
        incremental["current"]["changed_artifacts"] == 1
            && incremental["current"]["lexical_documents"] == 1
            && incremental["embedding_cache_hits"] == 0,
        "unchanged files must do no embedding-cache work: {incremental}"
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
    fs::write(root.join("blank.txt"), " \n")?;
    let cached = run(&root, &data, "4", &["index"])?;
    ensure!(
        cached["embedding_actual_forwards"] == 0
            && cached["embedding_plan_compilations"] == 0
            && cached["embedding_plan_cache_peak_bytes"]
                .as_u64()
                .unwrap_or(0)
                > 0,
        "a cached update must report retained template memory without inventing plan work"
    );
    eprintln!("serial={serial}\nparallel={parallel}\nincremental={incremental}\ncached={cached}");
    Ok(())
}

#[test]
#[ignore = "requires AIRK_TEST_MODEL_DIR with verified local ONNX assets"]
fn exact_alias_inputs_share_one_native_forward_and_graph_node_with_explicit_selection() -> Result<()>
{
    let base = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let root = base.path().join("workspace");
    let data = base.path().join("state");
    fs::create_dir_all(root.join("aliases"))?;
    let text = "Reset the account password using a temporary recovery link.\n";
    for index in 0..40 {
        fs::write(root.join(format!("aliases/copy{index:03}.txt")), text)?;
    }
    let model = std::env::var("AIRK_TEST_MODEL_DIR")?;
    run(
        &root,
        &data,
        "4",
        &["collections", "vectors", "install", &model],
    )?;
    let mut reports = Vec::new();
    for full in [false, true] {
        let started = std::time::Instant::now();
        let report = run(
            &root,
            &data,
            "4",
            if full {
                &["index", "--full"]
            } else {
                &["index"]
            },
        )?;
        let elapsed = started.elapsed().as_micros();
        ensure!(
            report["vector_units"] == 40
                && report["vector_reused_inputs"] == 39
                && report["vector_graph_nodes"] == 1
                && report["embedding_inputs"] == 1
                && report["embedding_actual_forwards"] == 1
                && report["embedding_cache_hits"] == 0,
            "cold/full exact input accounting differs: {report}"
        );
        ensure!(
            report["vector_registry_rows"] == 1
                && report["vector_registry_bytes"] == 0
                && report["current"]["graph_records"]
                    .as_u64()
                    .is_some_and(|n| (2..=4).contains(&n)),
            "bounded semantic batches must share one live node with bounded graph work: {report}"
        );
        ensure!(
            run(&root, &data, "4", &["status"])?["indexed_vectors"] == 1,
            "exact aliases should share one graph node"
        );
        let scoped = run(
            &root,
            &data,
            "4",
            &[
                "search",
                "recover a forgotten login password",
                "--mode",
                "vector",
                "--path-prefix",
                "aliases/copy039.txt",
            ],
        )?;
        ensure!(
            scoped["hits"][0]["citation"]["path"] == "aliases/copy039.txt"
                && scoped["hits"][0]["text"] == text
                && scoped["truncated"] == false,
            "last alias lost behind the first graph occurrence: {scoped}"
        );
        reports.push((if full { "full" } else { "cold" }, elapsed, report));
    }
    // A changed corpus adds another alias while retaining every original alias.
    fs::write(root.join("aliases/copy040.txt"), text)?;
    let started = std::time::Instant::now();
    let warm = run(&root, &data, "4", &["index"])?;
    let elapsed = started.elapsed().as_micros();
    ensure!(
        warm["vector_units"] == 1
            && warm["vector_reused_inputs"] == 1
            && warm["vector_graph_nodes"] == 0
            && warm["embedding_inputs"] == 0
            && warm["embedding_cache_hits"] == 0
            && warm["embedding_actual_forwards"] == 0,
        "warm alias addition performed redundant inference: {warm}"
    );
    reports.push(("warm-alias-addition", elapsed, warm));
    let policy = base.path().join("selection.json");
    fs::write(
        &policy,
        br#"{"version":1,"exclusions":[{"id":"excluded-copy","basename":"copy039.txt"}]}"#,
    )?;
    run(
        &root,
        &data,
        "4",
        &[
            "collections",
            "vectors",
            "selection",
            "set",
            "--file",
            policy.to_str().context("policy path is not UTF8")?,
        ],
    )?;
    let started = std::time::Instant::now();
    let selected = run(&root, &data, "4", &["index"])?;
    let elapsed = started.elapsed().as_micros();
    ensure!(
        selected["vector_eligible_artifacts"] == 40
            && selected["vector_excluded_artifacts"] == 1
            && selected["vector_units"] == 40
            && selected["vector_reused_inputs"] == 40
            && selected["vector_graph_nodes"] == 0
            && selected["embedding_actual_forwards"] == 0,
        "explicit selection counters differ: {selected}"
    );
    let excluded = run(
        &root,
        &data,
        "4",
        &[
            "search",
            "account password",
            "--mode",
            "vector",
            "--path-prefix",
            "aliases/copy039.txt",
        ],
    )?;
    ensure!(
        excluded["hits"]
            .as_array()
            .context("vector hits missing")?
            .is_empty(),
        "excluded alias retained vectors"
    );
    let lexical = run(
        &root,
        &data,
        "4",
        &[
            "search",
            "account password",
            "--mode",
            "lexical",
            "--path-prefix",
            "aliases/copy039.txt",
        ],
    )?;
    ensure!(
        lexical["hits"][0]["citation"]["path"] == "aliases/copy039.txt"
            && lexical["hits"][0]["text"] == text,
        "selection removed exact lexical evidence: {lexical}"
    );
    let info = run(&root, &data, "4", &["info", "aliases/copy039.txt"])?;
    ensure!(
        info["vector_selection"]["eligible"] == false
            && info["vector_selection"]["excluded_by"] == "excluded-copy",
        "committed selection reason missing: {info}"
    );
    reports.push(("selected", elapsed, selected));
    for (phase, wall_us, report) in reports {
        eprintln!("exact_aliases phase={phase} wall_us={wall_us} report={report}");
    }
    for index in 0..41 {
        ensure!(
            fs::read_to_string(root.join(format!("aliases/copy{index:03}.txt")))? == text,
            "indexing changed an original alias source"
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires AIRK_TEST_MODEL_DIR with verified local ONNX assets"]
fn distinct_native_inputs_use_packs_and_preserve_warm_and_full_retrieval() -> Result<()> {
    let base = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let root = base.path().join("workspace");
    let data = base.path().join("state");
    fs::create_dir_all(root.join("notes"))?;
    let source = |index: usize| {
        format!(
            "Recover account {index:03} with a temporary password reset link sent to its verified email address.\n"
        )
    };
    for index in 0..64 {
        fs::write(
            root.join(format!("notes/account{index:03}.txt")),
            source(index),
        )?;
    }
    let model = std::env::var("AIRK_TEST_MODEL_DIR")?;
    run(
        &root,
        &data,
        "4",
        &["collections", "vectors", "install", &model],
    )?;
    let counter = |report: &Value, name: &str| -> Result<u64> {
        report[name]
            .as_u64()
            .with_context(|| format!("missing counter {name}"))
    };
    let mut reports = Vec::new();
    let started = std::time::Instant::now();
    let cold = run(&root, &data, "4", &["index"])?;
    let elapsed = started.elapsed().as_micros();
    ensure!(
        cold["embedding_inputs"] == 64
            && cold["vector_graph_nodes"] == 64
            && cold["embedding_inferences"] == 64
            && cold["embedding_cache_hits"] == 0
            && cold["embedding_cache_packs_written"] == 2,
        "distinct inputs did not produce one cache pack per semantic batch: {cold}"
    );
    for field in ["embedding_cache_file_opens", "embedding_cache_syncs"] {
        ensure!(
            (1..64).contains(&counter(&cold, field)?),
            "packing did not reduce {field}: {cold}"
        );
    }
    ensure!(
        counter(&cold, "embedding_cache_written_bytes")? > 0
            && counter(&cold, "embedding_cache_full_pack_verification_bytes")? > 0
            && counter(&cold, "embedding_cache_finalize_us")? > 0,
        "cold pack persistence measurements missing: {cold}"
    );
    reports.push(("cold", elapsed, cold));

    fs::rename(
        root.join("notes/account063.txt"),
        root.join("notes/renamed-last.txt"),
    )?;
    let started = std::time::Instant::now();
    let moved = run(&root, &data, "4", &["index"])?;
    let elapsed = started.elapsed().as_micros();
    ensure!(
        moved["embedding_cache_hits"] == 0
            && moved["vector_reused_inputs"] == 1
            && moved["embedding_actual_forwards"] == 0
            && moved["embedding_cache_packs_written"] == 0
            && moved["current"]["changed_artifacts"] == 2,
        "rename must reuse the exact graph node without cache/inference: {moved}"
    );
    ensure!(
        counter(&moved, "embedding_cache_record_verification_bytes")? == 0,
        "unchanged graph vector must not require a cache-record read: {moved}"
    );
    reports.push(("warm-move", elapsed, moved));

    let edited = "Recover account 063 using a temporary password reset link. Verify the recovery email before restoring access.\n";
    fs::write(root.join("notes/renamed-last.txt"), edited)?;
    let started = std::time::Instant::now();
    let changed = run(&root, &data, "4", &["index"])?;
    let elapsed = started.elapsed().as_micros();
    ensure!(
        changed["embedding_cache_hits"] == 0
            && changed["embedding_inferences"] == 1
            && changed["embedding_actual_forwards"] == 1
            && changed["vector_graph_nodes"] == 1
            && changed["embedding_cache_packs_written"] == 1
            && changed["embedding_cache_packs_reused"] == 0,
        "one-file edit did not preserve unchanged packed vectors: {changed}"
    );
    reports.push(("warm-edit", elapsed, changed));
    let query = [
        "search",
        "recover forgotten login access",
        "--mode",
        "vector",
        "--path-prefix",
        "notes/renamed-last.txt",
    ];
    let before = run(&root, &data, "4", &query)?;
    ensure!(
        before["hits"][0]["citation"]["path"] == "notes/renamed-last.txt"
            && before["hits"][0]["text"] == edited
            && before["truncated"] == false,
        "scoped packed evidence differs from the edited source: {before}"
    );
    let started = std::time::Instant::now();
    let full = run(&root, &data, "4", &["index", "--full"])?;
    let elapsed = started.elapsed().as_micros();
    ensure!(
        full["embedding_cache_hits"] == 0
            && full["embedding_inferences"] == 64
            && full["vector_graph_nodes"] == 64
            && counter(&full, "embedding_cache_packs_written")?
                + counter(&full, "embedding_cache_packs_reused")?
                == 2
            && counter(&full, "embedding_cache_record_verification_bytes")? > 0,
        "full rebuild must recompute and verify packed values across semantic batches: {full}"
    );
    ensure!(
        run(&root, &data, "4", &query)?["hits"] == before["hits"],
        "full rebuild changed the packed native evidence"
    );
    reports.push(("full", elapsed, full));
    for (phase, wall_us, report) in reports {
        let persistence_us = counter(&report, "embedding_cache_store_us")?
            .saturating_add(counter(&report, "embedding_cache_finalize_us")?);
        eprintln!(
            "distinct_packs phase={phase} wall_us={wall_us} persistence_us={persistence_us} report={report}"
        );
    }
    for index in 0..63 {
        ensure!(
            fs::read_to_string(root.join(format!("notes/account{index:03}.txt")))? == source(index),
            "indexing changed source {index}"
        );
    }
    ensure!(
        fs::read_to_string(root.join("notes/renamed-last.txt"))? == edited,
        "indexing changed edited source"
    );
    Ok(())
}
