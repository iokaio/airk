// SPDX-License-Identifier: Apache-2.0
//! Pinned INT8 CPU inference. One text per forward keeps dynamic quantization
//! independent of unrelated inputs and makes exact-input cache reuse sound.
use crate::model::{self, EmbeddingCacheStats, MAX_MODEL_BYTES, MAX_TOKENIZER_BYTES, OnnxManifest};
use anyhow::{Context, Result, ensure};
use ort::{
    session::{Session, builder::GraphOptimizationLevel},
    value::Tensor,
};
use std::{path::Path, sync::Mutex};
use tokenizers::{Encoding, PostProcessor, Tokenizer, TruncationDirection};

// Conservative admission reservation for the two pinned B1/<=256 graphs, not a
// measurement or a hard RSS limit. One session and one inference thread.
const EXECUTION_RESERVE_BYTES: usize = 192 << 20;
struct EncodedInput {
    encoding: Encoding,
    input_tokens: usize,
    truncated: bool,
}
pub struct OnnxModel {
    pub(crate) manifest: OnnxManifest,
    pub(crate) identity: String,
    tokenizer: Tokenizer,
    raw_tokenizer: Tokenizer,
    session: Mutex<Session>,
    pad_id: u32,
    special_tokens: usize,
}

fn shape_length(tokens: usize, maximum: usize) -> Result<usize> {
    ensure!(
        tokens > 0 && tokens <= maximum && maximum <= 512,
        "invalid ONNX token count"
    );
    Ok([32, 64, 128, 256, 512]
        .into_iter()
        .find(|length| *length >= tokens && *length <= maximum)
        .unwrap_or(maximum))
}

fn validate_single_template(processor: &serde_json::Value) -> Result<()> {
    let pieces = processor["single"]
        .as_array()
        .context("single tokenizer template missing")?;
    ensure!(
        (1..=8).contains(&pieces.len())
            && pieces.iter().all(
                |piece| piece.as_object().is_some_and(|value| value.len() == 1)
                    && (piece.get("Sequence").is_some() || piece.get("SpecialToken").is_some())
            ),
        "unsupported single tokenizer template pieces"
    );
    let sequences: Vec<_> = pieces
        .iter()
        .filter_map(|piece| piece.get("Sequence"))
        .collect();
    ensure!(
        sequences.len() == 1 && sequences[0]["id"] == "A",
        "unsupported tokenizer template: single input must preserve sequence A exactly once"
    );
    Ok(())
}

impl OnnxModel {
    pub(crate) fn open_retained(
        collection: &Path,
        set: &airk_core::model_objects::ModelObjectSet,
        bytes: &[u8],
    ) -> Result<Self> {
        Self::open_assets(&model::ObjectAssets { collection, set }, bytes)
    }
    pub(crate) fn open(directory: &Path, manifest_bytes: &[u8]) -> Result<Self> {
        Self::open_assets(&model::DirectoryAssets(directory), manifest_bytes)
    }
    pub(crate) fn open_assets(
        source: &dyn airk_core::model_objects::ModelAssetSource,
        manifest_bytes: &[u8],
    ) -> Result<Self> {
        use airk_core::model_objects::ModelAssetName;
        let (manifest, identity) = model::onnx_manifest(manifest_bytes)?;
        crate::model_catalog::approved_manifest(&manifest)?;
        model::verified_source_asset(
            source,
            ModelAssetName::License,
            65_536,
            &manifest.license_sha256,
        )?;
        let tokenizer_bytes = model::verified_source_asset(
            source,
            ModelAssetName::TokenizerJson,
            MAX_TOKENIZER_BYTES,
            &manifest.tokenizer_sha256,
        )?;
        let tokenizer_spec: serde_json::Value = serde_json::from_slice(&tokenizer_bytes)?;
        // This adapter qualifies BERT-family encoders. Reject arbitrary regular
        // expression pipelines and expansion-heavy custom processors explicitly.
        ensure!(
            tokenizer_spec["model"]["type"] == "WordPiece"
                && tokenizer_spec["normalizer"]["type"] == "BertNormalizer"
                && tokenizer_spec["pre_tokenizer"]["type"] == "BertPreTokenizer",
            "ONNX adapter requires a BERT WordPiece tokenizer"
        );
        ensure!(
            tokenizer_spec["post_processor"]["type"] == "TemplateProcessing"
                || tokenizer_spec["post_processor"]["type"] == "BertProcessing",
            "unsupported BERT postprocessor"
        );
        if tokenizer_spec["post_processor"]["type"] == "TemplateProcessing" {
            validate_single_template(&tokenizer_spec["post_processor"])?;
            for name in ["single", "pair"] {
                ensure!(
                    tokenizer_spec["post_processor"][name]
                        .as_array()
                        .is_some_and(|items| items.len() <= 8),
                    "tokenizer special-token template exceeds bound"
                );
            }
            let tokens = tokenizer_spec["post_processor"]["special_tokens"]
                .as_object()
                .context("tokenizer special tokens missing")?;
            ensure!(
                tokens.len() <= 16
                    && tokens
                        .values()
                        .all(|token| token["ids"].as_array().is_some_and(|ids| ids.len() <= 2)),
                "tokenizer special token expansion exceeds bound"
            );
        }
        ensure!(
            tokenizer_spec["added_tokens"]
                .as_array()
                .is_some_and(|tokens| tokens.len() <= 512),
            "tokenizer added-token budget exceeded"
        );
        let vocabulary = tokenizer_spec["model"]["vocab"]
            .as_object()
            .context("WordPiece vocabulary missing")?;
        ensure!(
            vocabulary.len() <= 100_000
                && vocabulary
                    .iter()
                    .all(|(token, id)| token.len() <= 1024
                        && id.as_u64().is_some_and(|id| id < 100_000)),
            "WordPiece vocabulary exceeds supported bounds"
        );
        let mut tokenizer =
            Tokenizer::from_bytes(&tokenizer_bytes).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        ensure!(
            tokenizer.get_vocab_size(true) <= 100_000,
            "tokenizer vocabulary exceeds limit"
        );
        tokenizer
            .with_truncation(None)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        tokenizer.with_padding(None);
        // encode(false) still applies sequence/type transformations. Remove the
        // processor entirely from raw tokenization and apply it exactly once below.
        let mut raw_tokenizer = tokenizer.clone();
        raw_tokenizer.with_post_processor(None::<tokenizers::processors::PostProcessorWrapper>);
        let special_tokens = tokenizer
            .get_post_processor()
            .map_or(0, |processor| processor.added_tokens(false));
        ensure!(
            special_tokens < manifest.max_tokens,
            "ONNX context cannot fit special tokens"
        );
        let pad_id = tokenizer
            .token_to_id("[PAD]")
            .context("BERT padding token missing")?;
        let bytes = model::verified_source_asset(
            source,
            ModelAssetName::ModelOnnx,
            MAX_MODEL_BYTES,
            &manifest.model_sha256,
        )?;
        initialize_runtime()?;
        let session = Session::builder()?
            .with_intra_threads(1)?
            .with_inter_threads(1)?
            .with_parallel_execution(false)?
            .with_memory_pattern(false)?
            .with_config_entry("session.intra_op.allow_spinning", "0")?
            .with_config_entry("session.inter_op.allow_spinning", "0")?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .commit_from_memory(&bytes)
            .context("load pinned INT8 graph")?;
        let expected: std::collections::BTreeSet<_> =
            [&manifest.input_ids, &manifest.attention_mask]
                .into_iter()
                .chain(manifest.token_type_ids.iter())
                .map(String::as_str)
                .collect();
        ensure!(
            session.inputs.len() == 3
                && session
                    .inputs
                    .iter()
                    .map(|i| i.name.as_str())
                    .collect::<std::collections::BTreeSet<_>>()
                    == expected,
            "INT8 input names differ from manifest"
        );
        ensure!(
            session.outputs.len() == 1 && session.outputs[0].name == manifest.output,
            "INT8 output differs from manifest"
        );
        Ok(Self {
            manifest,
            identity,
            tokenizer,
            raw_tokenizer,
            session: Mutex::new(session),
            pad_id,
            special_tokens,
        })
    }

    pub(crate) fn planning_tokens(&self, text: &str) -> Result<model::PlanningTokens> {
        ensure!(
            text.len() <= model::MAX_INPUT_BYTES,
            "vector planning input exceeds byte bound"
        );
        let encoding = self
            .raw_tokenizer
            .encode(text, false)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        ensure!(
            encoding.len() <= 65_536,
            "ONNX tokenizer exceeds output-token budget"
        );
        Ok(model::PlanningTokens {
            offsets: encoding.get_offsets().to_vec(),
            special_tokens: self.special_tokens,
        })
    }

    fn encode_input(&self, original: &str) -> Result<Option<EncodedInput>> {
        let text = model::onnx_input_prefix(original);
        if text.trim().is_empty() {
            return Ok(None);
        }
        // Tokenize once without the tokenizer's early truncation optimization so
        // pre-truncation counts are observed, not inferred from partial overflow.
        let mut encoding = self
            .raw_tokenizer
            .encode(text, false)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        ensure!(
            encoding.len() <= 65_536,
            "ONNX tokenizer exceeds output-token budget"
        );
        let input_tokens = encoding
            .len()
            .checked_add(self.special_tokens)
            .context("ONNX input-token overflow")?;
        encoding.truncate(
            self.manifest.max_tokens - self.special_tokens,
            0,
            TruncationDirection::Right,
        );
        encoding.set_overflowing(Vec::new());
        let encoding = self
            .tokenizer
            .post_process(encoding, None, true)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        ensure!(
            !encoding.is_empty() && encoding.len() <= self.manifest.max_tokens,
            "tokenizer did not respect context bound"
        );
        Ok(Some(EncodedInput {
            encoding,
            input_tokens,
            truncated: text.len() < original.len() || input_tokens > self.manifest.max_tokens,
        }))
    }

    fn run_encoded(
        &self,
        input: &EncodedInput,
        memory_budget: usize,
    ) -> Result<(Vec<f32>, EmbeddingCacheStats)> {
        ensure!(
            memory_budget >= EXECUTION_RESERVE_BYTES,
            "INT8 execution exceeds working-memory budget"
        );
        let length = shape_length(input.encoding.len(), self.manifest.max_tokens)?;
        let mut ids = vec![i64::from(self.pad_id); length];
        let mut mask = vec![0i64; length];
        let mut types = vec![0i64; length];
        for index in 0..input.encoding.len() {
            ids[index] = i64::from(input.encoding.get_ids()[index]);
            mask[index] = i64::from(input.encoding.get_attention_mask()[index]);
            types[index] = i64::from(input.encoding.get_type_ids()[index]);
        }
        let mut session = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("INT8 session lock poisoned"))?;
        let outputs = session
            .run(ort::inputs![
                "input_ids" => Tensor::from_array(([1, length], ids))?,
                "attention_mask" => Tensor::from_array(([1, length], mask.clone()))?,
                "token_type_ids" => Tensor::from_array(([1, length], types))?,
            ])
            .context("local INT8 inference failed")?;
        let (shape, values) = outputs[self.manifest.output.as_str()].try_extract_tensor::<f32>()?;
        ensure!(
            shape.as_ref() == [1, length as i64, self.manifest.dimensions as i64],
            "INT8 output shape changed"
        );
        let mut sum = vec![0.0f64; self.manifest.dimensions];
        for (token, mask) in values.chunks_exact(self.manifest.dimensions).zip(&mask) {
            if *mask == 0 {
                continue;
            }
            for (total, value) in sum.iter_mut().zip(token) {
                ensure!(value.is_finite(), "INT8 output contains nonfinite values");
                *total += f64::from(*value);
            }
        }
        let norm = sum.iter().map(|v| v * v).sum::<f64>().sqrt();
        ensure!(
            norm.is_finite() && norm > 0.0,
            "INT8 model produced an empty embedding"
        );
        let embedded = mask.iter().sum::<i64>() as u64;
        Ok((
            sum.into_iter().map(|v| (v / norm) as f32).collect(),
            EmbeddingCacheStats {
                tokenized_inputs: 1,
                input_tokens: input.input_tokens as u64,
                embedded_tokens: embedded,
                truncated_inputs: u64::from(input.truncated),
                padding_tokens: length as u64 - embedded,
                actual_forwards: 1,
                workers: 1,
                plan_cache_peak_bytes: EXECUTION_RESERVE_BYTES as u64,
                ..Default::default()
            },
        ))
    }
    pub(crate) fn embed_text_measured(
        &self,
        text: &str,
        memory_budget: usize,
    ) -> Result<(Option<Vec<f32>>, EmbeddingCacheStats)> {
        let Some(input) = self.encode_input(text)? else {
            return Ok((None, EmbeddingCacheStats::default()));
        };
        let (vector, stats) = self.run_encoded(&input, memory_budget)?;
        Ok((Some(vector), stats))
    }
    pub(crate) fn embed_batch_measured(
        &self,
        texts: &[&str],
        _requested_workers: usize,
        memory_budget: usize,
    ) -> Result<(Vec<Vec<f32>>, EmbeddingCacheStats)> {
        ensure!(
            !texts.is_empty() && texts.len() <= crate::embedding_batch::MAX_BATCH_CHUNKS,
            "invalid INT8 batch size"
        );
        let mut stats = EmbeddingCacheStats {
            workers: 1,
            ..Default::default()
        };
        let vectors = texts
            .iter()
            .map(|text| {
                let (vector, execution) = self.embed_text_measured(text, memory_budget)?;
                stats.add_execution(&execution);
                vector.context("blank input passed to INT8 execution batch")
            })
            .collect::<Result<_>>()?;
        Ok((vectors, stats))
    }
    pub(crate) fn embed_text(&self, text: &str) -> Result<Option<Vec<f32>>> {
        Ok(self.embed_text_measured(text, 1 << 30)?.0)
    }
    pub(crate) fn plan_cache_peak_bytes(&self) -> Result<u64> {
        Ok(EXECUTION_RESERVE_BYTES as u64)
    }
}

/// Load only pinned offline libraries alongside the helper. Never search PATH,
/// the current directory or ORT_DYLIB_PATH. Debug builds can use prepared inputs.
fn initialize_runtime() -> Result<()> {
    static READY: std::sync::OnceLock<std::result::Result<(), String>> = std::sync::OnceLock::new();
    READY
        .get_or_init(|| {
            (|| -> Result<()> {
                let catalog: serde_json::Value =
                    serde_json::from_str(include_str!("ort_runtime_catalog.json"))?;
                let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
                let assets = catalog["platforms"][&platform]
                    .as_array()
                    .context("unsupported INT8 runtime platform")?;
                let executable = std::env::current_exe()?;
                #[allow(unused_mut)]
                let mut directory = executable
                    .parent()
                    .context("helper directory missing")?
                    .to_path_buf();
                #[cfg(debug_assertions)]
                if !directory.join(assets[0]["name"].as_str().unwrap()).exists() {
                    directory = std::env::var_os("AIRK_BUNDLED_RUNTIME_DIR")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|| {
                            Path::new(env!("CARGO_MANIFEST_DIR"))
                                .join("../../target/local-onnx-runtime")
                        })
                        .join(platform);
                }
                for asset in assets {
                    #[cfg(target_os = "macos")]
                    if let Some(expected) = asset["macho_payload_sha256"].as_str() {
                        let bytes = model::read_asset(
                            &directory,
                            asset["name"].as_str().unwrap(),
                            64 << 20,
                        )?;
                        ensure!(
                            crate::macho_identity::payload_digest(&bytes)? == expected,
                            "signed inference library content differs"
                        );
                        continue;
                    }
                    model::verified_asset(
                        &directory,
                        asset["name"].as_str().unwrap(),
                        asset["bytes"].as_u64().unwrap(),
                        asset["sha256"].as_str().unwrap(),
                    )?;
                }
                let library = directory
                    .join(assets[0]["name"].as_str().unwrap())
                    .canonicalize()?;
                ensure!(
                    ort::init_from(library.to_string_lossy())
                        .with_name("AIRK INT8")
                        .commit()?,
                    "INT8 runtime was initialized outside AIRK"
                );
                Ok(())
            })()
            .map_err(|e| format!("{e:#}"))
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!(e.clone()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_tokenization_has_no_processor_and_single_templates_cannot_duplicate_input() -> Result<()>
    {
        use serde_json::json;
        let processor = json!({"type":"TemplateProcessing",
            "single":[{"SpecialToken":{"id":"[CLS]","type_id":0}},{"Sequence":{"id":"A","type_id":1}},{"SpecialToken":{"id":"[SEP]","type_id":0}}],
            "pair":[{"Sequence":{"id":"A","type_id":0}},{"Sequence":{"id":"B","type_id":1}}],
            "special_tokens":{"[CLS]":{"id":"[CLS]","ids":[1],"tokens":["[CLS]"]},"[SEP]":{"id":"[SEP]","ids":[2],"tokens":["[SEP]"]}}});
        validate_single_template(&processor)?;
        for single in [
            json!([{"Sequence":{"id":"A","type_id":0}},{"Sequence":{"id":"A","type_id":0}}]),
            json!([{"Sequence":{"id":"B","type_id":0}}]),
            json!([{"SpecialToken":{"id":"[CLS]","type_id":0}}]),
        ] {
            let mut rejected = processor.clone();
            rejected["single"] = single;
            assert!(validate_single_template(&rejected).is_err());
        }
        let spec = json!({"version":"1.0","truncation":null,"padding":null,"added_tokens":[],
            "normalizer":{"type":"BertNormalizer","clean_text":true,"handle_chinese_chars":true,"strip_accents":null,"lowercase":true},
            "pre_tokenizer":{"type":"BertPreTokenizer"},"post_processor":processor,"decoder":null,
            "model":{"type":"WordPiece","vocab":{"[UNK]":0,"[CLS]":1,"[SEP]":2,"hello":3,"[PAD]":4},"unk_token":"[UNK]","continuing_subword_prefix":"##","max_input_chars_per_word":100}});
        let tokenizer = Tokenizer::from_bytes(serde_json::to_vec(&spec)?)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let mut raw = tokenizer.clone();
        raw.with_post_processor(None::<tokenizers::processors::PostProcessorWrapper>);
        let encoding = raw
            .encode("HELLO hello", false)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        assert_eq!(encoding.get_ids(), &[3, 3]);
        assert_eq!(
            encoding.get_type_ids(),
            &[0, 0],
            "raw stage must not apply the template"
        );
        let final_encoding = tokenizer
            .post_process(encoding, None, true)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        assert_eq!(final_encoding.get_ids(), &[1, 3, 3, 2]);
        assert_eq!(final_encoding.get_type_ids(), &[0, 1, 1, 0]);
        Ok(())
    }
    #[test]
    fn shape_buckets_are_deterministic_and_respect_nonstandard_contexts() -> Result<()> {
        for (tokens, expected) in [
            (1, 32),
            (32, 32),
            (33, 64),
            (64, 64),
            (65, 128),
            (128, 128),
            (129, 256),
            (256, 256),
        ] {
            assert_eq!(shape_length(tokens, 256)?, expected);
        }
        assert_eq!(shape_length(7, 8)?, 8);
        assert_eq!(shape_length(65, 100)?, 100);
        assert_eq!(shape_length(257, 384)?, 384);
        assert!(shape_length(0, 256).is_err());
        assert!(shape_length(257, 256).is_err());
        Ok(())
    }

    #[test]
    #[ignore = "requires explicitly provisioned INT8 assets and native runtime"]
    fn bundled_int8_is_normalized_bounded_and_batch_independent() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/local-vector-model");
        for profile in crate::model_catalog::inspect_catalog()?.catalog.profiles {
            let directory = root.join(&profile.id);
            let model = OnnxModel::open(&directory, &std::fs::read(directory.join("model.json"))?)?;
            let short = "Find the chapter generation pipeline and its retry policy.";
            let long = "long unrelated content ".repeat(1024);
            let expected = model.embed_text(short)?.unwrap();
            assert_eq!(expected.len(), 384);
            assert!(
                (expected.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() - 1.0).abs() < 1e-5
            );
            let (actual, stats) = model.embed_batch_measured(&[short, &long, short], 8, 1 << 30)?;
            assert_eq!(actual[0], expected);
            assert_eq!(actual[2], expected);
            assert_eq!(stats.actual_forwards, 3);
            assert_eq!(stats.workers, 1);
            assert_eq!(stats.truncated_inputs, 1);
            assert!(model.embed_text("   ")?.is_none());
            assert!(model.embed_text_measured(short, 1024).is_err());
        }
        Ok(())
    }
}
