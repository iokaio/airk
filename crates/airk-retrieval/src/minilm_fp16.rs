// SPDX-License-Identifier: Apache-2.0
//! Native CUDA view of the approved shared-weight MiniLM encoder.
mod asset;
mod cuda;
mod placement;

#[cfg(feature = "test-support")]
pub use cuda::MiniLmCudaProbe;
pub use cuda::{CudaBatchFailure, MiniLmCudaAdmission, MiniLmCudaSession};

use crate::{
    embedding_batch::MAX_BATCH_CHUNKS,
    model::{self, EmbeddingCacheStats, MAX_MODEL_BYTES, MAX_TOKENIZER_BYTES, OnnxManifest},
};
use airk_core::model_objects::{ModelAssetName, ModelAssetSource, ModelObjectSet};
use anyhow::{Context, Result, ensure};
use std::path::Path;
use tokenizers::{Encoding, PostProcessor, Tokenizer, TruncationDirection};

pub struct MiniLmCudaModel {
    model_sha256: String,
    tokenizer: Tokenizer,
    raw_tokenizer: Tokenizer,
    special_tokens: usize,
    pad_id: u32,
    max_tokens: usize,
    graph: Vec<u8>,
    cuda_graph: Vec<u8>,
}

struct BatchInputs {
    ids: Vec<i64>,
    mask: Vec<i64>,
    types: Vec<i64>,
    width: usize,
    count: usize,
    stats: EmbeddingCacheStats,
}

impl MiniLmCudaModel {
    pub fn open_assets(source: &dyn ModelAssetSource, manifest_bytes: &[u8]) -> Result<Self> {
        crate::native_inference::require_service_compute()?;
        let (manifest, _) = model::onnx_manifest(manifest_bytes)?;
        #[cfg(feature = "test-support")]
        if crate::native_inference::evaluation_model_lease(manifest_bytes).is_none() {
            crate::model_catalog::approved_manifest(&manifest)?;
        }
        #[cfg(not(feature = "test-support"))]
        crate::model_catalog::approved_manifest(&manifest)?;
        Self::from_verified_source(source, manifest)
    }

    pub fn open_directory(directory: &Path) -> Result<Self> {
        let source = model::DirectoryAssets(directory);
        let manifest = source.read_asset(ModelAssetName::ModelJson, 16_384)?;
        Self::open_assets(&source, &manifest)
    }

    pub fn open_retained(collection: &Path, set: &ModelObjectSet) -> Result<Self> {
        set.validate()?;
        let source = model::ObjectAssets { collection, set };
        let manifest = source.read_asset(ModelAssetName::ModelJson, 16_384)?;
        Self::open_assets(&source, &manifest)
    }

    pub fn model_asset_sha256(&self) -> &str {
        &self.model_sha256
    }

    fn from_verified_source(source: &dyn ModelAssetSource, manifest: OnnxManifest) -> Result<Self> {
        ensure!(
            manifest.dimensions == 384
                && manifest.max_tokens == 256
                && manifest.input_ids == "input_ids"
                && manifest.attention_mask == "attention_mask"
                && manifest.token_type_ids.as_deref() == Some("token_type_ids")
                && manifest.output == "last_hidden_state",
            "unsupported MiniLM FP16 ABI"
        );
        model::verified_source_asset(
            source,
            ModelAssetName::License,
            65_536,
            &manifest.license_sha256,
        )?;
        let graph = model::verified_source_bytes(
            source,
            ModelAssetName::ModelOnnx,
            MAX_MODEL_BYTES,
            &manifest.model_sha256,
        )?
        .into_owned();
        let cuda_graph = asset::cuda_graph(&graph)?;
        let tokenizer_bytes = model::verified_source_asset(
            source,
            ModelAssetName::TokenizerJson,
            MAX_TOKENIZER_BYTES,
            &manifest.tokenizer_sha256,
        )?;
        let mut tokenizer = Tokenizer::from_bytes(&tokenizer_bytes)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        tokenizer
            .with_truncation(None)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        tokenizer.with_padding(None);
        let mut raw_tokenizer = tokenizer.clone();
        raw_tokenizer.with_post_processor(None::<tokenizers::processors::PostProcessorWrapper>);
        let special_tokens = tokenizer
            .get_post_processor()
            .map_or(0, |processor| processor.added_tokens(false));
        ensure!(
            (1..manifest.max_tokens).contains(&special_tokens),
            "invalid MiniLM special token template"
        );
        let pad_id = tokenizer
            .token_to_id("[PAD]")
            .context("missing MiniLM padding token")?;
        crate::onnx::initialize_runtime()?;
        Ok(Self {
            model_sha256: manifest.model_sha256,
            tokenizer,
            raw_tokenizer,
            special_tokens,
            pad_id,
            max_tokens: manifest.max_tokens,
            graph,
            cuda_graph,
        })
    }

    fn encode(&self, original: &str) -> Result<(Encoding, usize, bool)> {
        let text = model::onnx_input_prefix(original);
        ensure!(!text.trim().is_empty(), "blank MiniLM execution input");
        let mut tokens = self
            .raw_tokenizer
            .encode(text, false)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        ensure!(
            tokens.len() <= 65_536,
            "MiniLM tokenizer exceeds output-token budget"
        );
        let untruncated = tokens.len() + self.special_tokens;
        tokens.truncate(
            self.max_tokens - self.special_tokens,
            0,
            TruncationDirection::Right,
        );
        tokens.set_overflowing(Vec::new());
        let tokens = self
            .tokenizer
            .post_process(tokens, None, true)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        ensure!(
            !tokens.is_empty() && tokens.len() <= self.max_tokens,
            "MiniLM tokenizer exceeded context bound"
        );
        Ok((
            tokens,
            untruncated,
            text.len() < original.len() || untruncated > self.max_tokens,
        ))
    }

    fn inputs(&self, texts: &[&str]) -> Result<BatchInputs> {
        ensure!(
            !texts.is_empty() && texts.len() <= MAX_BATCH_CHUNKS,
            "invalid MiniLM batch size"
        );
        let encodings = texts
            .iter()
            .map(|text| self.encode(text))
            .collect::<Result<Vec<_>>>()?;
        let longest = encodings
            .iter()
            .map(|(tokens, _, _)| tokens.len())
            .max()
            .context("empty MiniLM batch")?;
        let width = [32, 64, 128, 256]
            .into_iter()
            .find(|width| *width >= longest)
            .context("MiniLM input exceeds model width")?;
        let count = encodings.len();
        let mut batch = BatchInputs {
            ids: vec![i64::from(self.pad_id); count * width],
            mask: vec![0; count * width],
            types: vec![0; count * width],
            width,
            count,
            stats: EmbeddingCacheStats {
                tokenized_inputs: count as u64,
                actual_forwards: 1,
                workers: 1,
                ..Default::default()
            },
        };
        for (row, (tokens, untruncated, truncated)) in encodings.into_iter().enumerate() {
            let offset = row * width;
            for column in 0..tokens.len() {
                batch.ids[offset + column] = i64::from(tokens.get_ids()[column]);
                batch.mask[offset + column] = i64::from(tokens.get_attention_mask()[column]);
                batch.types[offset + column] = i64::from(tokens.get_type_ids()[column]);
            }
            let embedded: u64 = tokens
                .get_attention_mask()
                .iter()
                .map(|value| u64::from(*value))
                .sum();
            batch.stats.input_tokens += untruncated as u64;
            batch.stats.embedded_tokens += embedded;
            batch.stats.padding_tokens += width as u64 - embedded;
            batch.stats.truncated_inputs += u64::from(truncated);
        }
        Ok(batch)
    }

    #[cfg(feature = "test-support")]
    pub fn evaluation_inputs(&self, text: &str) -> Result<serde_json::Value> {
        let input = self.inputs(&[text])?;
        Ok(serde_json::json!({
            "input_ids": [input.ids],
            "attention_mask": [input.mask],
            "token_type_ids": [input.types]
        }))
    }
}

fn pool(
    output: &ort::value::DynValue,
    mask: &[i64],
    count: usize,
    width: usize,
) -> Result<Vec<Vec<f32>>> {
    const DIMENSIONS: usize = 384;
    let (shape, values) = output.try_extract_tensor::<f32>()?;
    ensure!(
        shape.as_ref() == [count as i64, width as i64, DIMENSIONS as i64]
            && mask.len() == count * width,
        "MiniLM output does not match [batch, tokens, dimensions]"
    );
    values
        .chunks_exact(width * DIMENSIONS)
        .zip(mask.chunks_exact(width))
        .map(|(row, mask)| {
            let mut sum = vec![0.0_f64; DIMENSIONS];
            for (token, active) in row.as_chunks::<DIMENSIONS>().0.iter().zip(mask) {
                if *active == 0 {
                    continue;
                }
                for (sum, value) in sum.iter_mut().zip(token) {
                    ensure!(value.is_finite(), "MiniLM produced a nonfinite output");
                    *sum += f64::from(*value);
                }
            }
            let norm = sum.iter().map(|value| value * value).sum::<f64>().sqrt();
            ensure!(
                norm.is_finite() && norm > 0.0,
                "MiniLM produced an empty embedding"
            );
            Ok(sum.into_iter().map(|value| (value / norm) as f32).collect())
        })
        .collect()
}

#[cfg(all(test, feature = "test-support"))]
mod tests;
