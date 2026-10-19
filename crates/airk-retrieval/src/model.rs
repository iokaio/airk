// SPDX-License-Identifier: Apache-2.0
//! Deliberately provisioned local embedding adapters; no runtime asset downloads.
use crate::{onnx::OnnxModel, vector::LocalModel};
use airk_core::{
    embedding::{EmbeddingCache, EmbeddingDescriptor, lookup_embedding, store_embedding},
    identity::{Sha256Digest, framed_digest},
    model_objects::{ModelAssetName, ModelAssetSource, ModelObjectRef, ModelObjectSet},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::Path,
};

pub(crate) struct DirectoryAssets<'a>(pub &'a Path);
impl ModelAssetSource for DirectoryAssets<'_> {
    fn read_asset(&self, name: ModelAssetName, maximum: u64) -> Result<Vec<u8>> {
        read_asset(self.0, name.filename(), maximum.min(name.maximum_bytes()))
    }
}

struct ObjectAssets<'a> {
    collection: &'a Path,
    set: &'a ModelObjectSet,
}
impl ModelAssetSource for ObjectAssets<'_> {
    fn read_asset(&self, name: ModelAssetName, maximum: u64) -> Result<Vec<u8>> {
        let reference = self
            .set
            .assets
            .get(&name)
            .context("model object asset missing")?;
        ensure!(
            reference.bytes <= maximum.min(name.maximum_bytes()),
            "model object asset exceeds read budget"
        );
        let directory = self.collection.join("objects").join("models");
        for ancestor in directory
            .ancestors()
            .filter(|path| !path.as_os_str().is_empty())
        {
            let metadata = fs::symlink_metadata(ancestor)?;
            ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "linked model object ancestor"
            );
        }
        let bytes = read_asset(&directory, &reference.sha256.to_string(), reference.bytes)?;
        ensure!(
            bytes.len() as u64 == reference.bytes
                && Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == reference.sha256,
            "model object digest or length mismatch"
        );
        Ok(bytes)
    }
}

pub(crate) fn verified_source_asset(
    source: &dyn ModelAssetSource,
    name: ModelAssetName,
    maximum: u64,
    digest: &str,
) -> Result<Vec<u8>> {
    ensure!(valid_digest(digest), "invalid asset SHA-256");
    let bytes = source.read_asset(name, maximum)?;
    ensure!(
        bytes.len() as u64 <= maximum && format!("{:x}", Sha256::digest(&bytes)) == digest,
        "local model asset digest mismatch: {}",
        name.filename()
    );
    Ok(bytes)
}

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

#[derive(Clone, Debug, Default, Serialize)]
pub struct EmbeddingCacheStats {
    /// Logical inputs presented, including cache hits and blank inputs.
    pub inputs: u64,
    /// Inputs whose effective model text is whitespace-only.
    pub blank_inputs: u64,
    /// Repeated nonblank exact inputs within bounded ONNX batches.
    pub duplicate_inputs: u64,
    /// Successful ONNX plan executions, excluding cache hits and blank inputs.
    pub actual_forwards: u64,
    pub hits: u64,
    pub misses: u64,
    /// Legacy logical uncached evaluations, including blanks and duplicate misses.
    /// This is not a count of neural model executions; use actual_forwards.
    pub inferences: u64,
    pub no_vector: u64,
    /// Wall time for each disjoint phase; inference includes worker scheduling.
    pub cache_lookup_us: u64,
    pub cache_store_us: u64,
    pub inference_us: u64,
    pub graph_insert_us: u64,
    pub model_load_us: u64,
    pub workers: usize,
    pub batches: u64,
}

pub struct EmbeddingCacheContext<'a> {
    pub cache: Option<&'a dyn EmbeddingCache>,
    /// Full rebuild skips lookup, but verifies/stores recomputed entries.
    pub bypass: bool,
    pub stats: &'a mut EmbeddingCacheStats,
    pub max_workers: usize,
    pub working_memory_bytes: usize,
}

/// Match the ONNX adapter's bounded UTF-8 prefix before its early blank return.
pub(crate) fn onnx_input_is_blank(text: &str) -> bool {
    let mut end = text.len().min(MAX_INPUT_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].trim().is_empty()
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
    let mut input = File::open(path)?;
    ensure!(
        input.metadata()?.len() == metadata.len(),
        "local model asset changed before read"
    );
    (&mut input).take(maximum + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "local model asset exceeds byte limit"
    );
    ensure!(
        bytes.len() as u64 == metadata.len() && input.metadata()?.len() == metadata.len(),
        "local model asset changed while reading"
    );
    Ok(bytes)
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
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

enum VerifiedManifest {
    Word(crate::vector::ModelManifest),
    Onnx(Box<OnnxManifest>, String),
}

fn inspect_assets(directory: &Path) -> Result<VerifiedManifest> {
    inspect_source_assets(&DirectoryAssets(directory))
}

fn inspect_source_assets(source: &dyn ModelAssetSource) -> Result<VerifiedManifest> {
    let bytes = source.read_asset(ModelAssetName::ModelJson, 16_384)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    if value["schema"] == 1 {
        let model = LocalModel::open_assets(source)?;
        return Ok(VerifiedManifest::Word(model.manifest));
    }
    let (manifest, identity) = onnx_manifest(&bytes)?;
    verified_source_asset(
        source,
        ModelAssetName::ModelOnnx,
        MAX_MODEL_BYTES,
        &manifest.model_sha256,
    )?;
    verified_source_asset(
        source,
        ModelAssetName::TokenizerJson,
        MAX_TOKENIZER_BYTES,
        &manifest.tokenizer_sha256,
    )?;
    verified_source_asset(
        source,
        ModelAssetName::License,
        65_536,
        &manifest.license_sha256,
    )?;
    Ok(VerifiedManifest::Onnx(Box::new(manifest), identity))
}

/// Inspect and verify all assets without compiling an inference plan.
pub fn inspect(directory: &Path) -> Result<ModelInfo> {
    Ok(match inspect_assets(directory)? {
        VerifiedManifest::Word(manifest) => ModelInfo {
            identity: manifest.sha256,
            name: manifest.name,
            license: manifest.license,
            backend: "local-word-vectors/mean/l2-normalized",
            dimensions: manifest.dimensions,
            max_tokens: None,
            max_input_bytes: None,
        },
        VerifiedManifest::Onnx(manifest, identity) => ModelInfo {
            identity,
            name: manifest.name,
            license: manifest.license,
            backend: ONNX_RUNTIME,
            dimensions: manifest.dimensions,
            max_tokens: Some(manifest.max_tokens),
            max_input_bytes: Some(MAX_INPUT_BYTES),
        },
    })
}

/// Verify the complete cache pipeline without compiling ONNX inference.
/// Returns the same descriptor as an opened model with these verified assets.
pub fn inspect_cache_descriptor(directory: &Path) -> Result<EmbeddingDescriptor> {
    match inspect_assets(directory)? {
        VerifiedManifest::Word(manifest) => {
            descriptor_from_manifest(DescriptorManifest::Word(&manifest))
        }
        VerifiedManifest::Onnx(manifest, _) => {
            descriptor_from_manifest(DescriptorManifest::Onnx(&manifest))
        }
    }
}

pub fn inspect_object_plan(directory: &Path) -> Result<ModelObjectSet> {
    for ancestor in directory
        .ancestors()
        .filter(|path| !path.as_os_str().is_empty())
    {
        let metadata = fs::symlink_metadata(ancestor)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "linked model source ancestor"
        );
    }
    let descriptor = inspect_cache_descriptor(directory)?;
    let info = inspect(directory)?;
    let manifest = read_asset(directory, "model.json", 16_384)?;
    let schema = serde_json::from_slice::<serde_json::Value>(&manifest)?["schema"].as_u64();
    let names: &[ModelAssetName] = match schema {
        Some(1) => &[ModelAssetName::ModelJson, ModelAssetName::VocabularyJson],
        Some(2) => &[
            ModelAssetName::ModelJson,
            ModelAssetName::ModelOnnx,
            ModelAssetName::TokenizerJson,
            ModelAssetName::License,
        ],
        _ => anyhow::bail!("unsupported model object schema"),
    };
    let mut assets = BTreeMap::new();
    for name in names {
        let bytes = DirectoryAssets(directory).read_asset(*name, name.maximum_bytes())?;
        assets.insert(
            *name,
            ModelObjectRef {
                sha256: Sha256Digest::from_bytes(Sha256::digest(&bytes).into()),
                bytes: bytes.len() as u64,
            },
        );
    }
    ensure!(
        inspect_cache_descriptor(directory)? == descriptor
            && inspect(directory)?.identity == info.identity,
        "model changed during object planning"
    );
    let set = ModelObjectSet {
        version: 1,
        pipeline_digest: descriptor.identity()?,
        legacy_identity: info.identity,
        assets,
    };
    set.validate()?;
    Ok(set)
}

pub fn inspect_objects_cache_descriptor(
    collection: &Path,
    set: &ModelObjectSet,
) -> Result<EmbeddingDescriptor> {
    set.validate()?;
    let source = ObjectAssets { collection, set };
    let (descriptor, identity) = match inspect_source_assets(&source)? {
        VerifiedManifest::Word(manifest) => (
            descriptor_from_manifest(DescriptorManifest::Word(&manifest))?,
            manifest.sha256,
        ),
        VerifiedManifest::Onnx(manifest, identity) => (
            descriptor_from_manifest(DescriptorManifest::Onnx(&manifest))?,
            identity,
        ),
    };
    ensure!(
        descriptor.identity()? == set.pipeline_digest && identity == set.legacy_identity,
        "model objects disagree with authenticated pipeline"
    );
    Ok(descriptor)
}

impl EmbeddingModel {
    pub fn open_objects(collection: &Path, set: &ModelObjectSet) -> Result<Self> {
        set.validate()?;
        let source = ObjectAssets { collection, set };
        let bytes = source.read_asset(ModelAssetName::ModelJson, 16_384)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        let model = match value["schema"].as_u64() {
            Some(1) => Self::WordVectors(LocalModel::open_assets(&source)?),
            Some(2) => Self::Onnx(Box::new(OnnxModel::open_assets(&source, &bytes)?)),
            _ => anyhow::bail!("unsupported local embedding model schema"),
        };
        ensure!(
            model.identity() == set.legacy_identity
                && model.cache_descriptor()?.identity()? == set.pipeline_digest,
            "model objects disagree with authenticated pipeline"
        );
        Ok(model)
    }
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

    /// Complete pipeline identity, separate from the legacy model display identity.
    pub fn cache_descriptor(&self) -> Result<EmbeddingDescriptor> {
        descriptor_from_manifest(match self {
            Self::WordVectors(model) => DescriptorManifest::Word(&model.manifest),
            Self::Onnx(model) => DescriptorManifest::Onnx(&model.manifest),
        })
    }

    pub fn embed_cached(
        &self,
        text: &str,
        descriptor: &EmbeddingDescriptor,
        cache: Option<&dyn EmbeddingCache>,
        bypass: bool,
        stats: &mut EmbeddingCacheStats,
    ) -> Result<Option<Vec<f32>>> {
        let started = std::time::Instant::now();
        airk_core::embedding::embedding_key(descriptor, text.as_bytes())?;
        stats.inputs += 1;
        let is_onnx = matches!(self, Self::Onnx(_));
        let blank = if is_onnx {
            onnx_input_is_blank(text)
        } else {
            text.trim().is_empty()
        };
        stats.blank_inputs += u64::from(blank);
        if is_onnx && blank {
            stats.inferences += 1;
            stats.no_vector += 1;
            return Ok(None);
        }
        if let Some(cache) = cache.filter(|_| !bypass) {
            if let Some(vector) = lookup_embedding(cache, descriptor, text.as_bytes())? {
                stats.hits += 1;
                stats.cache_lookup_us += crate::embedding_batch::elapsed_us(started);
                return Ok(Some(vector));
            }
            stats.misses += 1;
        }
        stats.cache_lookup_us += crate::embedding_batch::elapsed_us(started);
        stats.inferences += 1;
        let started = std::time::Instant::now();
        let vector = self.embed_text(text)?;
        stats.actual_forwards += u64::from(is_onnx);
        stats.inference_us += crate::embedding_batch::elapsed_us(started);
        let started = std::time::Instant::now();
        if let Some(vector) = &vector {
            if let Some(cache) = cache {
                store_embedding(cache, descriptor, text.as_bytes(), vector.clone())?;
            }
        } else {
            stats.no_vector += 1;
        }
        stats.cache_store_us += crate::embedding_batch::elapsed_us(started);
        Ok(vector)
    }
}

#[derive(Clone, Copy)]
enum DescriptorManifest<'a> {
    Word(&'a crate::vector::ModelManifest),
    Onnx(&'a OnnxManifest),
}

fn descriptor_from_manifest(source: DescriptorManifest<'_>) -> Result<EmbeddingDescriptor> {
    let (manifest, tokenizer, preprocessing, pooling, runtime, dimensions) = match source {
        DescriptorManifest::Word(manifest) => (
            serde_json::to_vec(manifest)?,
            serde_json::to_vec(&(
                "unicode-alphanumeric-or-underscore/split/unicode-lowercase/1",
                std::char::UNICODE_VERSION,
            ))?,
            b"exact-utf8/no-truncation/unknown-token-skip/1".to_vec(),
            b"sum-f64/l2-f64/cast-f32/zero-or-empty-none/1".to_vec(),
            b"airk-local-word-vectors/1".to_vec(),
            manifest.dimensions,
        ),
        DescriptorManifest::Onnx(manifest) => (
            serde_json::to_vec(manifest)?,
            manifest.tokenizer_sha256.as_bytes().to_vec(),
            serde_json::to_vec(&(
                MAX_INPUT_BYTES,
                manifest.max_tokens,
                "tokenizers-0.23.2/fancy-regex/utf8-boundary-prefix/trim-empty-none/add-special-tokens/right-longest-first-truncation-stride0/right-fixed-padding-PAD-type0/1",
            ))?,
            manifest.pooling.as_bytes().to_vec(),
            ONNX_RUNTIME.as_bytes().to_vec(),
            manifest.dimensions,
        ),
    };
    let digest = |domain: &[u8], value: &[u8]| framed_digest(domain, &[value]);
    let descriptor = EmbeddingDescriptor {
        version: 1,
        model: digest(b"airk-model-manifest/1", &manifest),
        tokenizer: digest(b"airk-tokenizer/1", &tokenizer),
        preprocessing: digest(b"airk-preprocessing/1", &preprocessing),
        pooling: digest(b"airk-pooling/1", &pooling),
        normalization: digest(
            b"airk-normalization/1",
            match source {
                DescriptorManifest::Word(_) => b"l2-f64-to-f32/zero-norm-none/1",
                DescriptorManifest::Onnx(_) => b"l2-f64-to-f32/zero-norm-error/1",
            },
        ),
        inference: framed_digest(
            b"airk-inference/1",
            &[
                &runtime,
                std::env::consts::ARCH.as_bytes(),
                std::env::consts::OS.as_bytes(),
            ],
        ),
        dimensions: dimensions as u32,
    };
    descriptor.validate()?;
    Ok(descriptor)
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
    use airk_core::{embedding::CachedEmbedding, identity::Sha256Digest};
    use std::{collections::BTreeMap, sync::Mutex};

    fn unlinked_tempdir() -> Result<tempfile::TempDir> {
        // macOS presents the default temporary directory through /var.
        // Object planning intentionally rejects symlinked model ancestors.
        Ok(tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?)
    }

    #[test]
    fn immutable_objects_preserve_pipeline_without_source_directory_or_scratch_copies() -> Result<()>
    {
        let source = unlinked_tempdir()?;
        let collection = unlinked_tempdir()?;
        let vocabulary = br#"{"original":[1.0,0.0],"beacon":[0.0,1.0]}"#;
        fs::write(source.path().join("vocabulary.json"), vocabulary)?;
        fs::write(
            source.path().join("model.json"),
            serde_json::to_vec(&crate::vector::ModelManifest {
                schema: 1,
                name: "original immutable fixture".into(),
                license: "Apache-2.0".into(),
                sha256: format!("{:x}", Sha256::digest(vocabulary)),
                dimensions: 2,
            })?,
        )?;
        let expected = EmbeddingModel::open(source.path())?.embed_text("original beacon")?;
        let set = inspect_object_plan(source.path())?;
        let directory = collection.path().join("objects/models");
        fs::create_dir_all(&directory)?;
        for (name, reference) in &set.assets {
            fs::copy(
                source.path().join(name.filename()),
                directory.join(reference.sha256.to_string()),
            )?;
        }
        source.close()?;
        assert_eq!(
            inspect_objects_cache_descriptor(collection.path(), &set)?.identity()?,
            set.pipeline_digest
        );
        assert_eq!(
            EmbeddingModel::open_objects(collection.path(), &set)?.embed_text("original beacon")?,
            expected
        );
        assert!(!collection.path().join("model").exists());
        let mut forged = set.clone();
        forged.pipeline_digest = Sha256Digest::from_bytes([8; 32]);
        assert!(EmbeddingModel::open_objects(collection.path(), &forged).is_err());
        let reference = &set.assets[&ModelAssetName::VocabularyJson];
        fs::write(
            directory.join(reference.sha256.to_string()),
            vec![b'x'; reference.bytes as usize],
        )?;
        assert!(EmbeddingModel::open_objects(collection.path(), &set).is_err());
        let other = tempfile::tempdir()?;
        assert!(EmbeddingModel::open_objects(other.path(), &set).is_err());
        Ok(())
    }

    #[derive(Default)]
    struct Cache(Mutex<BTreeMap<Sha256Digest, CachedEmbedding>>);
    impl EmbeddingCache for Cache {
        fn lookup(&self, key: &Sha256Digest) -> Result<Option<CachedEmbedding>> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        fn store(&self, entry: &CachedEmbedding) -> Result<()> {
            self.0.lock().unwrap().insert(entry.key, entry.clone());
            Ok(())
        }
    }

    #[test]
    fn actual_model_cache_avoids_inference_and_full_bypass_matches() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let vocabulary = br#"{"original":[1.0,0.0],"fixture":[0.0,1.0]}"#;
        fs::write(directory.path().join("vocabulary.json"), vocabulary)?;
        let mut manifest = crate::vector::ModelManifest {
            schema: 1,
            name: "original fixture".into(),
            license: "Apache-2.0".into(),
            sha256: format!("{:x}", Sha256::digest(vocabulary)),
            dimensions: 2,
        };
        fs::write(
            directory.path().join("model.json"),
            serde_json::to_vec(&manifest)?,
        )?;
        let model = EmbeddingModel::open(directory.path())?;
        let descriptor = model.cache_descriptor()?;
        let tokenizer_contract = serde_json::to_vec(&(
            "unicode-alphanumeric-or-underscore/split/unicode-lowercase/1",
            std::char::UNICODE_VERSION,
        ))?;
        assert_eq!(
            descriptor.tokenizer,
            framed_digest(b"airk-tokenizer/1", &[&tokenizer_contract]),
            "cache identity binds the actual Rust Unicode tables"
        );
        assert_eq!(inspect_cache_descriptor(directory.path())?, descriptor);
        assert_eq!(inspect(directory.path())?.identity, model.identity());
        let cache = Cache::default();
        let mut stats = EmbeddingCacheStats::default();
        let cold = model.embed_cached(
            "original fixture",
            &descriptor,
            Some(&cache),
            false,
            &mut stats,
        )?;
        assert!(cold.is_some());
        assert_eq!((stats.hits, stats.misses, stats.inferences), (0, 1, 1));
        assert_eq!(
            model.embed_cached(
                "original fixture",
                &descriptor,
                Some(&cache),
                false,
                &mut stats
            )?,
            cold
        );
        assert_eq!((stats.hits, stats.misses, stats.inferences), (1, 1, 1));
        assert_eq!(
            model.embed_cached(
                "original fixture",
                &descriptor,
                Some(&cache),
                true,
                &mut stats
            )?,
            cold
        );
        assert_eq!(stats.inferences, 2);
        model.embed_cached(
            "original fixture\n",
            &descriptor,
            Some(&cache),
            false,
            &mut stats,
        )?;
        assert_eq!(stats.misses, 2, "exact byte differences cannot hit");
        for _ in 0..2 {
            assert!(
                model
                    .embed_cached("unknown", &descriptor, Some(&cache), false, &mut stats)?
                    .is_none()
            );
        }
        assert_eq!(stats.no_vector, 2);
        assert_eq!(
            stats.inferences, 5,
            "no-vector outcome is not fabricated or cached as a vector"
        );
        assert_eq!(stats.inputs, 6);
        assert_eq!(
            stats.actual_forwards, 0,
            "word vectors execute no ONNX plan"
        );
        assert_eq!(stats.blank_inputs, 0);
        assert_eq!(stats.duplicate_inputs, 0);
        manifest.name = "different manifest".into();
        fs::write(
            directory.path().join("model.json"),
            serde_json::to_vec(&manifest)?,
        )?;
        assert_ne!(
            EmbeddingModel::open(directory.path())?.cache_descriptor()?,
            descriptor
        );
        assert_eq!(
            inspect_cache_descriptor(directory.path())?,
            EmbeddingModel::open(directory.path())?.cache_descriptor()?
        );
        assert_eq!(
            EmbeddingModel::open(directory.path())?.identity(),
            model.identity(),
            "legacy word identity remains vocabulary digest"
        );
        let renamed = inspect_cache_descriptor(directory.path())?;
        manifest.license = "Original additional provenance".into();
        fs::write(
            directory.path().join("model.json"),
            serde_json::to_vec(&manifest)?,
        )?;
        assert_ne!(inspect_cache_descriptor(directory.path())?, renamed);
        cache
            .0
            .lock()
            .unwrap()
            .values_mut()
            .for_each(|entry| entry.vector[0] = 123.0);
        assert!(
            model
                .embed_cached(
                    "original fixture",
                    &descriptor,
                    Some(&cache),
                    false,
                    &mut stats
                )
                .is_err()
        );
        Ok(())
    }
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
    fn onnx_pipeline_inspection_verifies_assets_without_compiling_fake_model() -> Result<()> {
        let directory = tempfile::tempdir()?;
        // Original opaque assets exercise inspection only, not ONNX execution.
        let assets: [(&str, &[u8]); 3] = [
            ("model.onnx", b"original non-ONNX inspection fixture"),
            (
                "tokenizer.json",
                b"original non-tokenizer inspection fixture",
            ),
            ("LICENSE", b"original fixture license"),
        ];
        for (name, bytes) in assets {
            fs::write(directory.path().join(name), bytes)?;
        }
        let mut spec = manifest();
        spec.model_sha256 = format!("{:x}", Sha256::digest(assets[0].1));
        spec.tokenizer_sha256 = format!("{:x}", Sha256::digest(assets[1].1));
        spec.license_sha256 = format!("{:x}", Sha256::digest(assets[2].1));
        let write = |value: &OnnxManifest| -> Result<()> {
            fs::write(
                directory.path().join("model.json"),
                serde_json::to_vec(value)?,
            )?;
            Ok(())
        };
        write(&spec)?;
        let original = inspect_cache_descriptor(directory.path())?;
        assert_eq!(
            original,
            descriptor_from_manifest(DescriptorManifest::Onnx(&spec))?
        );
        assert_eq!(
            inspect(directory.path())?.identity,
            onnx_manifest(&serde_json::to_vec(&spec)?)?.1
        );
        let mut changed = spec.clone();
        changed.max_tokens = 16;
        write(&changed)?;
        let context = inspect_cache_descriptor(directory.path())?;
        assert_ne!(context.preprocessing, original.preprocessing);
        assert_eq!(context.tokenizer, original.tokenizer);
        for changed in [
            OnnxManifest {
                name: "changed name".into(),
                ..spec.clone()
            },
            OnnxManifest {
                revision: "changed revision".into(),
                ..spec.clone()
            },
            OnnxManifest {
                source: "https://example.invalid/changed".into(),
                ..spec.clone()
            },
            OnnxManifest {
                token_type_ids: Some("token_types".into()),
                ..spec.clone()
            },
            OnnxManifest {
                output: "different_output".into(),
                ..spec.clone()
            },
        ] {
            write(&changed)?;
            assert_ne!(inspect_cache_descriptor(directory.path())?, original);
            assert_eq!(
                inspect_cache_descriptor(directory.path())?,
                descriptor_from_manifest(DescriptorManifest::Onnx(&changed))?
            );
        }
        write(&spec)?;
        for (name, bytes) in assets {
            fs::write(directory.path().join(name), b"corrupt")?;
            assert!(
                inspect_cache_descriptor(directory.path()).is_err(),
                "asset {name} must verify before a cache identity is trusted"
            );
            fs::write(directory.path().join(name), bytes)?;
        }
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
