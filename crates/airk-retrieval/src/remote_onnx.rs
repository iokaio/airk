// SPDX-License-Identifier: Apache-2.0
//! ONNX facade without local inference dependencies. The native host supplies transport.
use crate::{
    embedding_worker::{self, ModelLocation, RemoteSession, Request, Response},
    model::{self, EmbeddingCacheStats, OnnxManifest, PlanningTokens},
};
use airk_core::model_objects::ModelObjectSet;
use anyhow::{Result, bail, ensure};
use std::{path::Path, sync::Arc};

pub struct OnnxModel {
    pub(crate) manifest: OnnxManifest,
    pub(crate) identity: String,
    session: Arc<dyn RemoteSession>,
}

impl OnnxModel {
    fn connect(location: ModelLocation, bytes: &[u8]) -> Result<Self> {
        let (manifest, identity) = model::onnx_manifest(bytes)?;
        crate::model_catalog::approved_manifest(&manifest)?;
        let session = embedding_worker::connector()?.open(location, &identity)?;
        Ok(Self {
            manifest,
            identity,
            session,
        })
    }

    pub(crate) fn open(directory: &Path, bytes: &[u8]) -> Result<Self> {
        Self::connect(
            ModelLocation::Directory {
                path: directory.canonicalize()?,
            },
            bytes,
        )
    }

    pub(crate) fn open_retained(
        collection: &Path,
        set: &ModelObjectSet,
        bytes: &[u8],
    ) -> Result<Self> {
        set.validate()?;
        Self::connect(
            ModelLocation::Retained {
                collection: collection.canonicalize()?,
                objects: set.clone(),
            },
            bytes,
        )
    }

    pub(crate) fn planning_tokens(&self, text: &str) -> Result<PlanningTokens> {
        ensure!(
            text.len() <= model::MAX_INPUT_BYTES,
            "vector planning input exceeds byte bound"
        );
        match self.session.request(Request::Plan { text: text.into() })? {
            Response::Planned { tokens } => {
                ensure!(
                    tokens.special_tokens <= 16 && tokens.offsets.len() <= model::MAX_INPUT_BYTES,
                    "embedding worker returned oversized token plan"
                );
                let mut previous = 0;
                for &(start, end) in &tokens.offsets {
                    ensure!(
                        start >= previous
                            && start <= end
                            && end <= text.len()
                            && text.is_char_boundary(start)
                            && text.is_char_boundary(end),
                        "invalid worker token offsets"
                    );
                    previous = start;
                }
                Ok(tokens)
            }
            _ => bail!("invalid embedding worker planning response"),
        }
    }

    pub(crate) fn embed_batch_measured(
        &self,
        texts: &[&str],
        workers: usize,
        memory_bytes: usize,
    ) -> Result<(Vec<Vec<f32>>, EmbeddingCacheStats)> {
        ensure!(
            !texts.is_empty()
                && texts.len() <= crate::embedding_batch::MAX_BATCH_CHUNKS
                && texts.iter().all(|t| t.len() <= model::MAX_INPUT_BYTES),
            "invalid embedding worker batch"
        );
        match self.session.request(Request::Embed {
            texts: texts.iter().map(|s| (*s).into()).collect(),
            workers,
            memory_bytes,
        })? {
            Response::Embedded { vectors, execution } => {
                ensure!(
                    vectors.len() == texts.len(),
                    "embedding worker result count mismatch"
                );
                let vectors = vectors
                    .into_iter()
                    .map(|v| {
                        let v = v.ok_or_else(|| {
                            anyhow::anyhow!("embedding worker returned no vector")
                        })?;
                        ensure!(
                            v.len() == self.manifest.dimensions,
                            "embedding worker dimensions mismatch"
                        );
                        ensure!(
                            v.iter().all(|v| v.is_finite()),
                            "embedding worker returned nonfinite vector"
                        );
                        Ok(v)
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok((vectors, *execution))
            }
            _ => bail!("invalid embedding worker response"),
        }
    }

    pub(crate) fn embed_text_measured(
        &self,
        text: &str,
        memory: usize,
    ) -> Result<(Option<Vec<f32>>, EmbeddingCacheStats)> {
        if model::onnx_input_is_blank(text) {
            return Ok((None, EmbeddingCacheStats::default()));
        }
        let bounded = model::onnx_input_prefix(text);
        let (mut vectors, execution) = self.embed_batch_measured(&[bounded], 1, memory)?;
        Ok((vectors.pop(), execution))
    }

    pub(crate) fn embed_text(&self, text: &str) -> Result<Option<Vec<f32>>> {
        Ok(self.embed_text_measured(text, 1 << 30)?.0)
    }

    pub(crate) fn plan_cache_peak_bytes(&self) -> Result<u64> {
        match self.session.request(Request::Peak)? {
            Response::Peak { bytes } => Ok(bytes),
            _ => bail!("invalid embedding worker peak response"),
        }
    }
}
