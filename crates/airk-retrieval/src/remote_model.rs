// SPDX-License-Identifier: Apache-2.0
//! Lazy direct model session; metadata stays local and inference stays in the pool.
use crate::{
    embedding_worker::{self, ModelLocation, RemoteSession, Request, Response},
    model::{self, EmbeddingCacheStats, ModelInfo, PlanningTokens},
};
use airk_core::{
    embedding::EmbeddingDescriptor,
    model_objects::{ModelAssetName, ModelAssetSource, ModelObjectSet},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

pub struct RemoteModel {
    pub(crate) info: ModelInfo,
    pub(crate) descriptor: EmbeddingDescriptor,
    objects: ModelObjectSet,
    session: Mutex<Option<Arc<dyn RemoteSession>>>,
}
impl RemoteModel {
    pub(crate) fn open(source: &dyn ModelAssetSource, objects: &ModelObjectSet) -> Result<Self> {
        ensure!(objects.shared.is_some(), "shared model reference required");
        let descriptor = model::descriptor_from_asset_metadata(source, objects)?;
        let bytes = source.read_asset(ModelAssetName::ModelJson, 16384)?;
        let reference = &objects.assets[&ModelAssetName::ModelJson];
        ensure!(
            bytes.len() as u64 == reference.bytes
                && format!("{:x}", Sha256::digest(&bytes)) == reference.sha256.to_string(),
            "shared model metadata changed"
        );
        let schema = serde_json::from_slice::<serde_json::Value>(&bytes)?["schema"].as_u64();
        let info = match schema {
            Some(1) => {
                let manifest: crate::vector::ModelManifest = serde_json::from_slice(&bytes)?;
                ModelInfo {
                    identity: manifest.sha256,
                    name: manifest.name,
                    license: manifest.license,
                    backend: "local-word-vectors/mean/l2-normalized",
                    dimensions: manifest.dimensions,
                    max_tokens: None,
                    max_input_bytes: None,
                }
            }
            Some(2) => {
                let (manifest, identity) = model::onnx_manifest(&bytes)?;
                ModelInfo {
                    identity,
                    name: manifest.name,
                    license: manifest.license,
                    backend: model::ONNX_RUNTIME,
                    dimensions: manifest.dimensions,
                    max_tokens: Some(manifest.max_tokens),
                    max_input_bytes: Some(model::MAX_INPUT_BYTES),
                }
            }
            _ => anyhow::bail!("unsupported shared model schema"),
        };
        Ok(Self {
            info,
            descriptor,
            objects: objects.clone(),
            session: Mutex::new(None),
        })
    }
    pub(crate) fn is_onnx(&self) -> bool {
        self.info.max_tokens.is_some()
    }
    fn request(&self, request: Request) -> Result<Response> {
        let session = {
            let mut slot = self
                .session
                .lock()
                .map_err(|_| anyhow::anyhow!("shared model session unavailable"))?;
            if slot.is_none() {
                *slot = Some(embedding_worker::connector()?.open(
                    ModelLocation::Shared {
                        objects: self.objects.clone(),
                    },
                    &self.info.identity,
                )?);
            }
            slot.as_ref()
                .context("shared model session missing")?
                .clone()
        };
        session.request(request)
    }
    pub(crate) fn planning_tokens(&self, text: &str) -> Result<PlanningTokens> {
        ensure!(
            text.len() <= model::MAX_INPUT_BYTES,
            "shared planning input exceeds bound"
        );
        let Response::Planned { tokens } = self.request(Request::Plan { text: text.into() })?
        else {
            anyhow::bail!("invalid shared planning response")
        };
        ensure!(
            tokens.special_tokens <= 16 && tokens.offsets.len() <= model::MAX_INPUT_BYTES,
            "shared token plan exceeds bound"
        );
        let mut previous = 0;
        for &(start, end) in &tokens.offsets {
            ensure!(
                start >= previous
                    && start <= end
                    && end <= text.len()
                    && text.is_char_boundary(start)
                    && text.is_char_boundary(end),
                "invalid shared token offsets"
            );
            previous = start;
        }
        Ok(tokens)
    }
    fn batch(
        &self,
        texts: &[&str],
        workers: usize,
        memory_bytes: usize,
    ) -> Result<(Vec<Option<Vec<f32>>>, EmbeddingCacheStats)> {
        ensure!(
            !texts.is_empty()
                && texts.len() <= crate::embedding_batch::MAX_BATCH_CHUNKS
                && texts
                    .iter()
                    .all(|text| text.len() <= model::MAX_INPUT_BYTES),
            "shared embedding input exceeds bound"
        );
        let Response::Embedded { vectors, execution } = self.request(Request::Embed {
            texts: texts.iter().map(|text| (*text).into()).collect(),
            workers,
            memory_bytes,
        })?
        else {
            anyhow::bail!("invalid shared embedding response")
        };
        ensure!(
            vectors.len() == texts.len()
                && vectors
                    .iter()
                    .flatten()
                    .all(|v| v.len() == self.info.dimensions && v.iter().all(|n| n.is_finite())),
            "shared embedding dimensions/count mismatch"
        );
        Ok((vectors, *execution))
    }
    pub(crate) fn embed_batch_measured(
        &self,
        texts: &[&str],
        workers: usize,
        memory: usize,
    ) -> Result<(Vec<Vec<f32>>, EmbeddingCacheStats)> {
        let (vectors, stats) = self.batch(texts, workers, memory)?;
        Ok((
            vectors
                .into_iter()
                .map(|v| v.context("shared ONNX model returned no vector"))
                .collect::<Result<_>>()?,
            stats,
        ))
    }
    pub(crate) fn embed_text_measured(
        &self,
        text: &str,
        memory: usize,
    ) -> Result<(Option<Vec<f32>>, EmbeddingCacheStats)> {
        let text = if self.is_onnx() {
            if model::onnx_input_is_blank(text) {
                return Ok((None, EmbeddingCacheStats::default()));
            }
            model::onnx_input_prefix(text)
        } else {
            text
        };
        let (mut vectors, stats) = self.batch(&[text], 1, memory)?;
        Ok((vectors.pop().flatten(), stats))
    }
    pub(crate) fn embed_text(&self, text: &str) -> Result<Option<Vec<f32>>> {
        Ok(self.embed_text_measured(text, 1 << 30)?.0)
    }
    pub(crate) fn plan_cache_peak_bytes(&self) -> Result<u64> {
        match self.request(Request::Peak)? {
            Response::Peak { bytes } => Ok(bytes),
            _ => anyhow::bail!("invalid shared model memory response"),
        }
    }
}
