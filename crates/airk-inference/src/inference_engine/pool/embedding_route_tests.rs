// SPDX-License-Identifier: Apache-2.0
//! Private native MiniLM CUDA placement, numeric and settlement smoke.
use airk_core::identity::Sha256Digest;
use airk_kernel::inference_owner::NativeInferenceOwner;
use airk_retrieval::{
    arbiter::runtime::cuda::process_poisoned,
    minilm_fp16::{MiniLmCudaModel, MiniLmCudaProbe},
    model::{EmbeddingCacheContext, EmbeddingCacheStats, EmbeddingModel},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{Read, Write},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

const ASSET: &str = "f3c3a4eed373650ba1e2f4d3d53437f1ecf92d4c50e5994e07e163751923ee4a";
const GRAPH: &str = "8d6ee9ccbeeb7a2c9a5dca313cb7b63ba7eb860fdfeab00bed7cc2fb3056bcb4";
const PLACEMENT: &str = "3761c7dc65d393a8ac2593b87578fe2bb7a2d714318aad902de2577ec4878801";
const RUNTIME: &str = "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5";
const ADAPTER: [u8; 16] = [
    0x08, 0xe2, 0xb3, 0xf6, 0x6a, 0x39, 0x34, 0x56, 0x77, 0xcb, 0xa5, 0x72, 0x87, 0xa7, 0x02, 0xec,
];
const HOST_BUDGET: usize = 512 << 20;

fn absolute_env(key: &str) -> Result<PathBuf> {
    let path = PathBuf::from(env::var_os(key).with_context(|| format!("missing {key}"))?);
    ensure!(path.is_absolute(), "{key} must be absolute");
    Ok(path)
}

fn authenticated_query() -> Result<Option<(Sha256Digest, String)>> {
    const PATH: &str = "AIRK_MINILM_FP16_PROBE_QUERY";
    const HASH: &str = "AIRK_MINILM_FP16_PROBE_QUERY_SHA256";
    const MAX_BYTES: usize = 16 * 1024;

    let (path, expected) = match (env::var_os(PATH), env::var_os(HASH)) {
        (None, None) => return Ok(None),
        (Some(path), Some(expected)) => (PathBuf::from(path), expected),
        _ => anyhow::bail!("{PATH} and {HASH} must be supplied together"),
    };
    ensure!(path.is_absolute(), "{PATH} must be absolute");
    let expected = expected
        .to_str()
        .and_then(|value| value.parse::<Sha256Digest>().ok())
        .context("invalid expected MiniLM probe query SHA-256")?;
    let file = fs::File::open(&path).context("open authenticated MiniLM probe query")?;
    ensure!(
        file.metadata()?.is_file(),
        "{PATH} must name a regular file"
    );
    let mut bytes = Vec::new();
    file.take(u64::try_from(MAX_BYTES + 1)?)
        .read_to_end(&mut bytes)
        .context("read authenticated MiniLM probe query")?;
    ensure!(bytes.len() <= MAX_BYTES, "{PATH} exceeds 16 KiB");
    let actual = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
    ensure!(actual == expected, "MiniLM probe query SHA-256 differs");
    let query = String::from_utf8(bytes)
        .map_err(|_| anyhow::anyhow!("MiniLM probe query must be valid UTF-8"))?;
    ensure!(!query.trim().is_empty(), "MiniLM probe query is blank");
    Ok(Some((actual, query)))
}

#[test]
#[ignore = "private Windows A6000 MiniLM FP16 probe against the exact authenticated static core, asset and CUDA runtime"]
fn minilm_cuda_native_placement_and_settled_batch() -> Result<()> {
    let start = Instant::now();
    let deadline = start + Duration::from_secs(120);
    let model_path = absolute_env("AIRK_MINILM_FP16_PROBE_MODEL_DIR")?;
    let control = absolute_env("AIRK_MINILM_FP16_PROBE_CONTROL")?;
    let output = absolute_env("AIRK_MINILM_FP16_PROBE_OUTPUT")?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let probe = MiniLmCudaProbe {
        asset_sha256: ASSET.parse()?,
        graph_sha256: GRAPH.parse()?,
        runtime_manifest_sha256: RUNTIME.parse()?,
        placement_sha256: PLACEMENT.parse()?,
        adapter_uuid: ADAPTER,
        cold_peak_bytes: 2 << 30,
        warm_extra_bytes: 1 << 30,
        headroom_bytes: 1 << 30,
        host_bytes: HOST_BUDGET,
    };
    let mut receipt = json!({
        "schema": "airk.private.minilm-cuda-smoke/1",
        "pid": std::process::id(),
        "model": ASSET,
        "gpu_graph": GRAPH,
        "runtime": RUNTIME,
        "adapter_uuid": ADAPTER,
        "limits": {
            "declared_only": true,
            "cold_peak_bytes": probe.cold_peak_bytes,
            "warm_extra_bytes": probe.warm_extra_bytes,
            "headroom_bytes": probe.headroom_bytes,
            "host_bytes": probe.host_bytes,
        },
        "qualification": false,
        "settled_cuda_batches": [],
    });
    let result = (|| -> Result<()> {
        let query = authenticated_query()?;
        let manifest = fs::read(model_path.join("model.json"))?;
        let digest = Sha256Digest::from_bytes(Sha256::digest(&manifest).into());
        let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
        let owner = NativeInferenceOwner::admit(lease.clone())?;
        owner.run_encoder_evaluation(digest, || {
            let cpu = EmbeddingModel::open(&model_path).context("open canonical CPU MiniLM")?;
            let gpu = MiniLmCudaModel::open_directory(&model_path)
                .context("open canonical CUDA MiniLM view")?;
            ensure!(
                gpu.model_asset_sha256() == ASSET,
                "private GPU asset pin differs"
            );
            let (root, path) = airk_host::gpu_runtime::installed_paths()?;
            let runtime = airk_host::gpu_runtime::admit(lease, &root, &path, deadline)
                .context("admit exact installed native CUDA runtime")?;
            ensure!(
                runtime.manifest_identity() == probe.runtime_manifest_sha256,
                "runtime pin differs"
            );
            let device = runtime.device(ADAPTER, deadline)?;
            receipt["adapter_before_open"] = serde_json::to_value(device.observation(deadline)?)?;
            // This observation cannot run the graph. It retains failure
            // evidence even if the exact reviewed placement refuses the graph.
            let placed = gpu
                .observe_cuda_placement(&runtime, &probe, deadline)
                .context("observe native final MiniLM placement")?;
            receipt["observed_placement"] = serde_json::to_value(placed)?;
            let mut session = gpu
                .probe_cuda(&runtime, &probe, deadline)
                .context("open checked native MiniLM CUDA session")?;
            receipt["placement_sha256"] = serde_json::to_value(session.placement_sha256()?)?;
            receipt["adapter_session_loaded"] =
                serde_json::to_value(device.observation(deadline)?)?;
            let text1 = "Thread cancellation settles CUDA work before releasing a source snapshot.";
            let text2 = "  Sparse indexing — naïve τ, 世界.  ";
            let long =
                "Index snapshots keep document provenance and original byte spans. ".repeat(48);
            for case in [
                vec![text1, text2],
                (0..32)
                    .map(|i| if i % 2 == 0 { text2 } else { long.as_str() })
                    .collect(),
            ] {
                let _ = execute_batch(
                    &mut receipt,
                    &cpu,
                    &gpu,
                    &mut session,
                    &case,
                    None,
                    deadline,
                )?;
            }
            let query_embedding = if let Some((sha256, query)) = query {
                execute_batch(
                    &mut receipt,
                    &cpu,
                    &gpu,
                    &mut session,
                    &[query.as_str()],
                    Some(sha256),
                    deadline,
                )?
            } else {
                None
            };
            receipt["adapter_after_batch"] = serde_json::to_value(device.observation(deadline)?)?;
            session.close()?;
            receipt["adapter_after_session_close"] =
                serde_json::to_value(device.observation(deadline)?)?;
            drop(device);
            drop(runtime);
            ensure!(!process_poisoned(), "MiniLM CUDA runtime did not settle");
            if let Some(query_embedding) = query_embedding {
                receipt["query_embedding"] = query_embedding;
            }
            Ok(())
        })?
    })();
    receipt["ok"] = json!(result.is_ok());
    receipt["error"] = json!(result.as_ref().err().map(|error| format!("{error:#}")));
    receipt["runtime_poisoned"] = json!(process_poisoned());
    receipt["elapsed_us"] = json!(u64::try_from(start.elapsed().as_micros()).ok());
    receipt["memory_observation"] = json!("point-in-time native free VRAM, not peak");
    serde_json::to_writer_pretty(&mut file, &receipt)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    result
}

fn execute_batch(
    receipt: &mut Value,
    cpu: &EmbeddingModel,
    gpu: &MiniLmCudaModel,
    session: &mut airk_retrieval::minilm_fp16::MiniLmCudaSession,
    texts: &[&str],
    query_sha256: Option<Sha256Digest>,
    deadline: Instant,
) -> Result<Option<Value>> {
    let mut cpu_stats = EmbeddingCacheStats::default();
    let expected = cpu.embed_batch(
        texts,
        &mut EmbeddingCacheContext {
            cache: None,
            bypass: true,
            stats: &mut cpu_stats,
            max_workers: 1,
            working_memory_bytes: HOST_BUDGET,
        },
    );
    let expected = expected.map_err(|error| {
        if query_sha256.is_some() {
            anyhow::anyhow!("canonical CPU MiniLM failed to embed the authenticated query")
        } else {
            error
        }
    })?;
    let start = Instant::now();
    let (actual, cuda_stats) = gpu
        .embed_cuda(session, texts, HOST_BUDGET, deadline)
        .map_err(|error| {
            if query_sha256.is_some() {
                anyhow::anyhow!("native CUDA MiniLM failed to embed the authenticated query")
            } else {
                error.into()
            }
        })?;
    let elapsed_us = u64::try_from(start.elapsed().as_micros())?;
    ensure!(
        expected.len() == actual.len()
            && actual.len() == texts.len()
            && cuda_stats.actual_forwards == 1,
        "native CUDA did not return one full batch in order"
    );
    let mut max_abs = 0.0_f32;
    let mut min_dot = 1.0_f64;
    let mut query_embedding = None;
    for (expected, actual) in expected.into_iter().zip(actual) {
        let expected = expected.context("reference embedding missing")?;
        ensure!(
            expected.len() == 384 && actual.len() == 384,
            "MiniLM vector dimension differs"
        );
        if let Some(sha256) = &query_sha256 {
            ensure!(
                expected
                    .iter()
                    .chain(&actual)
                    .all(|value| value.is_finite()),
                "MiniLM query vector contains a non-finite coordinate"
            );
            query_embedding =
                Some(json!({"input_sha256": sha256, "cpu": &expected, "cuda": &actual}));
        }
        let mut dot = 0.0;
        for (expected, actual) in expected.into_iter().zip(actual) {
            max_abs = max_abs.max((expected - actual).abs());
            dot += f64::from(expected) * f64::from(actual);
        }
        min_dot = min_dot.min(dot);
    }
    receipt["settled_cuda_batches"]
        .as_array_mut()
        .context("invalid local CUDA smoke receipt")?
        .push(json!({
            "rows": texts.len(),
            "native_forwards": cuda_stats.actual_forwards,
            "cpu_reference_max_abs": max_abs,
            "cpu_reference_min_dot": min_dot,
            "elapsed_cuda_us": elapsed_us
        }));
    ensure!(
        max_abs <= 0.005 && min_dot >= 0.9995,
        "MiniLM CUDA smoke numeric tolerance exceeded"
    );
    Ok(query_embedding)
}
