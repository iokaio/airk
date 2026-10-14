// SPDX-License-Identifier: Apache-2.0
//! Deliberately provisioned local embedding adapters; no runtime asset downloads.
use crate::{onnx::OnnxModel, vector::LocalModel};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};

pub const ONNX_RUNTIME: &str = "tract-cpu-onnx/0.23.8/airk-1";
pub const MAX_MODEL_BYTES: u64 = 268_435_456;
pub const MAX_TOKENIZER_BYTES: u64 = 4_194_304;
pub const MAX_INPUT_BYTES: usize = 16_384;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OnnxManifest {
    pub schema: u32,
    pub name: String,
    pub license: String,
    pub source: String,
    pub revision: String,
    pub model_sha256: String,
    pub tokenizer_sha256: String,
    pub license_sha256: String,
    pub dimensions: usize,
    pub max_tokens: usize,
    pub pooling: String,
    pub input_ids: String,
    pub attention_mask: String,
    pub token_type_ids: Option<String>,
    pub output: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ModelInfo {
    pub identity: String,
    pub name: String,
    pub license: String,
    pub backend: &'static str,
    pub dimensions: usize,
    pub max_tokens: Option<usize>,
    pub max_input_bytes: Option<usize>,
}

pub enum EmbeddingModel {
    WordVectors(LocalModel),
    Onnx(Box<OnnxModel>),
}

pub(crate) fn read_asset(directory: &Path, name: &str, maximum: u64) -> Result<Vec<u8>> {
    let path = directory.join(name);
    let metadata = fs::symlink_metadata(&path).context("local model asset missing")?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "local model asset must be a regular unlinked file"
    );
    ensure!(
        metadata.len() <= maximum,
        "local model asset exceeds byte limit"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "local model asset exceeds byte limit"
    );
    Ok(bytes)
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) fn verified_asset(
    directory: &Path,
    name: &str,
    maximum: u64,
    digest: &str,
) -> Result<Vec<u8>> {
    ensure!(valid_digest(digest), "invalid asset SHA-256");
    let bytes = read_asset(directory, name, maximum)?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == digest,
        "local model asset digest mismatch: {name}"
    );
    Ok(bytes)
}

pub(crate) fn onnx_manifest(bytes: &[u8]) -> Result<(OnnxManifest, String)> {
    let manifest: OnnxManifest =
        serde_json::from_slice(bytes).context("invalid ONNX model manifest")?;
    ensure!(
        manifest.schema == 2
            && (1..=2048).contains(&manifest.dimensions)
            && (8..=512).contains(&manifest.max_tokens),
        "unsupported ONNX embedding dimensions or context size"
    );
    ensure!(
        manifest.pooling == "attention-mask-mean-l2",
        "unsupported ONNX pooling specification"
    );
    for value in [
        &manifest.name,
        &manifest.license,
        &manifest.source,
        &manifest.revision,
    ] {
        ensure!(
            !value.trim().is_empty() && value.len() <= 2048,
            "model provenance fields are required and bounded"
        );
    }
    for digest in [
        &manifest.model_sha256,
        &manifest.tokenizer_sha256,
        &manifest.license_sha256,
    ] {
        ensure!(valid_digest(digest), "invalid model asset SHA-256");
    }
    let mut names = std::collections::BTreeSet::new();
    for name in [&manifest.input_ids, &manifest.attention_mask]
        .into_iter()
        .chain(manifest.token_type_ids.iter())
    {
        ensure!(
            !name.is_empty() && name.len() <= 128 && names.insert(name),
            "invalid or duplicate ONNX input name"
        );
    }
    ensure!(
        !manifest.output.is_empty() && manifest.output.len() <= 128,
        "invalid ONNX output name"
    );
    let mut digest = Sha256::new();
    digest.update(ONNX_RUNTIME.as_bytes());
    digest.update(serde_json::to_vec(&manifest)?);
    Ok((manifest, format!("{:x}", digest.finalize())))
}

/// Inspect and verify all assets without compiling an inference plan.
pub fn inspect(directory: &Path) -> Result<ModelInfo> {
    let bytes = read_asset(directory, "model.json", 16_384)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    if value["schema"] == 1 {
        let model = LocalModel::open(directory)?;
        return Ok(ModelInfo {
            identity: model.manifest.sha256,
            name: model.manifest.name,
            license: model.manifest.license,
            backend: "local-word-vectors/mean/l2-normalized",
            dimensions: model.manifest.dimensions,
            max_tokens: None,
            max_input_bytes: None,
        });
    }
    let (manifest, identity) = onnx_manifest(&bytes)?;
    verified_asset(
        directory,
        "model.onnx",
        MAX_MODEL_BYTES,
        &manifest.model_sha256,
    )?;
    verified_asset(
        directory,
        "tokenizer.json",
        MAX_TOKENIZER_BYTES,
        &manifest.tokenizer_sha256,
    )?;
    verified_asset(directory, "LICENSE", 65_536, &manifest.license_sha256)?;
    Ok(ModelInfo {
        identity,
        name: manifest.name,
        license: manifest.license,
        backend: ONNX_RUNTIME,
        dimensions: manifest.dimensions,
        max_tokens: Some(manifest.max_tokens),
        max_input_bytes: Some(MAX_INPUT_BYTES),
    })
}

impl EmbeddingModel {
    pub fn open(directory: &Path) -> Result<Self> {
        let bytes = read_asset(directory, "model.json", 16_384)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        match value["schema"].as_u64() {
            Some(1) => Ok(Self::WordVectors(LocalModel::open(directory)?)),
            Some(2) => Ok(Self::Onnx(Box::new(OnnxModel::open(directory, &bytes)?))),
            _ => anyhow::bail!("unsupported local embedding model schema"),
        }
    }
    pub fn identity(&self) -> &str {
        match self {
            Self::WordVectors(model) => &model.manifest.sha256,
            Self::Onnx(model) => &model.identity,
        }
    }
    pub fn dimensions(&self) -> usize {
        match self {
            Self::WordVectors(model) => model.manifest.dimensions,
            Self::Onnx(model) => model.manifest.dimensions,
        }
    }
    pub fn specification(&self) -> &'static str {
        match self {
            Self::WordVectors(_) => "local-word-vectors/mean/l2-normalized",
            Self::Onnx(_) => ONNX_RUNTIME,
        }
    }
    pub fn asset_names(&self) -> &'static [&'static str] {
        match self {
            Self::WordVectors(_) => &["model.json", "vocabulary.json"],
            Self::Onnx(_) => &["model.json", "model.onnx", "tokenizer.json", "LICENSE"],
        }
    }
    pub fn embed_text(&self, text: &str) -> Result<Option<Vec<f32>>> {
        match self {
            Self::WordVectors(model) => model.embed_text(text),
            Self::Onnx(model) => model.embed_text(text),
        }
    }
}

impl airk_core::EmbeddingProvider for EmbeddingModel {
    fn specification(&self) -> &str {
        self.identity()
    }
    fn embed(&self, text: &[String]) -> Result<Vec<Vec<f32>>> {
        ensure!(text.len() <= 1000, "embedding batch exceeds 1000 texts");
        text.iter()
            .map(|text| {
                self.embed_text(text)?
                    .context("local embedding has no usable input")
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> OnnxManifest {
        OnnxManifest {
            schema: 2,
            name: "original fixture".into(),
            license: "Apache-2.0".into(),
            source: "https://example.invalid/original-fixture".into(),
            revision: "original-v1".into(),
            model_sha256: "a".repeat(64),
            tokenizer_sha256: "b".repeat(64),
            license_sha256: "c".repeat(64),
            dimensions: 2,
            max_tokens: 8,
            pooling: "attention-mask-mean-l2".into(),
            input_ids: "input_ids".into(),
            attention_mask: "attention_mask".into(),
            token_type_ids: None,
            output: "last_hidden_state".into(),
        }
    }
    #[test]
    fn model_identity_binds_tokenizer_context_pooling_provenance_and_runtime() -> Result<()> {
        let mut spec = manifest();
        let (_, original) = onnx_manifest(&serde_json::to_vec(&spec)?)?;
        spec.tokenizer_sha256 = "d".repeat(64);
        assert_ne!(onnx_manifest(&serde_json::to_vec(&spec)?)?.1, original);
        spec = manifest();
        spec.max_tokens = 16;
        assert_ne!(onnx_manifest(&serde_json::to_vec(&spec)?)?.1, original);
        spec = manifest();
        spec.license = "different license declaration".into();
        assert_ne!(onnx_manifest(&serde_json::to_vec(&spec)?)?.1, original);
        spec.pooling = "unqualified pooling".into();
        assert!(onnx_manifest(&serde_json::to_vec(&spec)?).is_err());
        spec = manifest();
        spec.max_tokens = 513;
        assert!(onnx_manifest(&serde_json::to_vec(&spec)?).is_err());
        spec = manifest();
        spec.attention_mask = spec.input_ids.clone();
        assert!(onnx_manifest(&serde_json::to_vec(&spec)?).is_err());
        Ok(())
    }
    #[test]
    fn asset_verification_is_bounded_and_does_not_accept_changed_bytes() -> Result<()> {
        let directory = tempfile::tempdir()?;
        fs::write(directory.path().join("model.onnx"), b"original fixture")?;
        let hash = format!("{:x}", Sha256::digest(b"original fixture"));
        assert!(verified_asset(directory.path(), "model.onnx", 15, &hash).is_err());
        assert_eq!(
            verified_asset(directory.path(), "model.onnx", 16, &hash)?,
            b"original fixture"
        );
        fs::write(directory.path().join("model.onnx"), b"altered fixture!")?;
        assert!(verified_asset(directory.path(), "model.onnx", 16, &hash).is_err());
        Ok(())
    }
}
