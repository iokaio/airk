// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::identity::Sha256Digest;
use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf, sync::Arc};

#[test]
#[ignore = "reads the authenticated private MiniLM shared-weight artifact and uses the real CPU runtime"]
fn fp16_cuda_tokenizer_matches_existing_cpu_tokenizer() -> Result<()> {
    let model_dir = PathBuf::from(
        env::var_os("AIRK_MINILM_FP16_PROBE_MODEL_DIR")
            .context("private model directory is required")?,
    );
    let control = PathBuf::from(
        env::var_os("AIRK_MINILM_FP16_PROBE_CONTROL")
            .context("private native lease directory is required")?,
    );
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    lease.admit_inference_owner()?;
    let cpu_manifest = fs::read(model_dir.join("model.json"))?;
    let (manifest, _) = model::onnx_manifest(&cpu_manifest)?;
    assert_eq!(
        manifest.model_sha256,
        "f3c3a4eed373650ba1e2f4d3d53437f1ecf92d4c50e5994e07e163751923ee4a"
    );
    // SAFETY: this native test retains its real exclusive service lease and
    // binds exactly the private manifest for both CPU and CUDA adapters.
    let _compute = unsafe { crate::native_inference::ServiceComputeScope::enter()? };
    let _catalog = unsafe {
        crate::native_inference::EvaluationScope::enter(
            lease.clone(),
            Sha256Digest::from_bytes(Sha256::digest(&cpu_manifest).into()),
        )?
    };
    let cpu = crate::onnx::OnnxModel::open(&model_dir, &cpu_manifest)?;
    let cuda = MiniLmCudaModel::open_directory(&model_dir)?;
    let long = "a thread owns UTF-8 text, index boundaries, and a stable token mask. ".repeat(350);
    for text in [
        "  Sparse indexing — naïve τ, 世界.  ",
        "a straight identifier BINARY_OP::retry from file-1",
        long.as_str(),
    ] {
        assert_eq!(cuda.evaluation_inputs(text)?, cpu.evaluation_inputs(text)?);
    }
    Ok(())
}
