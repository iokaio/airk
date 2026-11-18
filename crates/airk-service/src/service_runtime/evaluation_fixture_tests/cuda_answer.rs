// SPDX-License-Identifier: Apache-2.0
//! Private exact-graph input for the real Windows CUDA Answer timing fixture.

use super::*;
use airk_inference::inference_engine::pool::arbiter::EvaluationAnswerGraph;

const MAX_GRAPH_BYTES: u64 = 16 << 20;

pub(super) struct Input {
    pub(super) manifest: Sha256Digest,
    pub(super) adapter: [u8; 16],
    pub(super) graph: EvaluationAnswerGraph,
}

impl Input {
    pub(super) fn read(artifacts: &Path, prepared: &[Prepared], expires: Instant) -> Result<Self> {
        let graph_path = PathBuf::from(
            std::env::var_os("AIRK_ARBITER_EVALUATION_CUDA_GRAPH")
                .context("private CUDA Answer requires a reviewed graph result")?,
        );
        let expected: Sha256Digest =
            std::env::var("AIRK_ARBITER_EVALUATION_CUDA_GRAPH_SHA256")?.parse()?;
        let adapter: [u8; 16] =
            serde_json::from_str(&std::env::var("AIRK_ARBITER_EVALUATION_CUDA_ADAPTER_UUID")?)?;
        ensure!(
            adapter != [0; 16],
            "private CUDA Answer adapter identity missing"
        );
        let (_directory, file) = source_file(&graph_path, artifacts)?;
        let mut graph = Vec::new();
        file.take(MAX_GRAPH_BYTES + 1).read_to_end(&mut graph)?;
        ensure!(
            !graph.is_empty() && graph.len() as u64 <= MAX_GRAPH_BYTES && sha(&graph) == expected,
            "private CUDA Answer graph size or SHA differs"
        );
        let result: serde_json::Value = serde_json::from_slice(&graph)?;
        let manifest = serde_json::from_value(result["profile"].clone())?;
        ensure!(
            prepared.iter().any(|item| {
                item.profile.manifest == manifest
                    && item.profile.model == ModelChoice::ContextQwenCompact
                    && item.profile.device_execution.is_some()
            }),
            "exact CUDA Compact graph profile missing from the isolated fixture"
        );
        Ok(Self {
            manifest,
            adapter,
            graph: EvaluationAnswerGraph::new(graph.into(), expected, expires),
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "private Windows CUDA Answer timing with exact reviewed fixture inputs"]
async fn actual_private_arbiter_fixture_cuda_answer() -> Result<()> {
    let consent = std::env::var("AIRK_ARBITER_EVALUATION_RESOURCE_CONSENT")
        .context("current CUDA fixture requires explicit resource configuration consent")?;
    let resources = software_v2_resource_configuration(&consent)?;
    run_actual_private_arbiter_fixture(software_v2_answer_template()?, Some(resources), true).await
}
