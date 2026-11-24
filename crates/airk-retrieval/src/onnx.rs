// SPDX-License-Identifier: Apache-2.0
//! Pinned INT8 CPU inference. One text per forward keeps dynamic quantization
//! independent of unrelated inputs and makes exact-input cache reuse sound.
//! A batch spreads its texts over a pool of identical one-thread sessions; each
//! text still runs alone, so its vector does not depend on the session used.
use crate::model::{self, EmbeddingCacheStats, MAX_MODEL_BYTES, MAX_TOKENIZER_BYTES, OnnxManifest};
use anyhow::{Context, Result, ensure};
use ort::{
    session::{Session, builder::GraphOptimizationLevel},
    value::Tensor,
};

#[cfg(target_os = "macos")]
#[path = "onnx_macos.rs"]
mod process_environment;
#[cfg(target_os = "macos")]
pub(crate) use process_environment::SessionLease as SessionEnvironmentLease;
#[cfg(not(target_os = "macos"))]
pub(crate) struct SessionEnvironmentLease;

pub(crate) fn acquire_session_environment() -> Result<SessionEnvironmentLease> {
    #[cfg(target_os = "macos")]
    {
        process_environment::SessionLease::acquire()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(SessionEnvironmentLease)
    }
}
use std::{path::Path, sync::Mutex};
use tokenizers::{Encoding, PostProcessor, Tokenizer, TruncationDirection};

// Conservative admission reservation for the two pinned B1/<=256 graphs, not a
// measurement or a hard RSS limit. Charged for each one-thread session.
const EXECUTION_RESERVE_BYTES: usize = crate::embedding_batch::SESSION_MEMORY_BYTES;
struct EncodedInput {
    encoding: Encoding,
    input_tokens: usize,
    truncated: bool,
}
struct InputTensors {
    ids: Vec<i64>,
    mask: Vec<i64>,
    types: Vec<i64>,
}
pub struct OnnxModel {
    pub(crate) manifest: OnnxManifest,
    pub(crate) identity: String,
    tokenizer: Tokenizer,
    raw_tokenizer: Tokenizer,
    // Graph bytes verified once at open. Further sessions load from them
    // without reading or verifying the model file again.
    graph: Vec<u8>,
    // Never empty. Grown lazily to the admitted worker count and reused.
    sessions: Mutex<Vec<Session>>,
    // Struct fields drop in declaration order: release the native sessions
    // before allowing process-global environment teardown.
    #[cfg(target_os = "macos")]
    _environment_user: process_environment::SessionLease,
    pad_id: u32,
    special_tokens: usize,
    // Declared after Session: retain the actual private native owner through
    // synchronous execution and real session teardown, even after scope exit.
    #[cfg(feature = "test-support")]
    _evaluation_lease: Option<std::sync::Arc<dyn Send + Sync>>,
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
        crate::native_inference::require_service_compute()?;
        use airk_core::model_objects::ModelAssetName;
        let (manifest, identity) = model::onnx_manifest(manifest_bytes)?;
        #[cfg(feature = "test-support")]
        let evaluation_lease = crate::native_inference::evaluation_model_lease(manifest_bytes);
        #[cfg(feature = "test-support")]
        if evaluation_lease.is_none() {
            crate::model_catalog::approved_manifest(&manifest)?;
        }
        #[cfg(not(feature = "test-support"))]
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
        let bytes = model::verified_source_bytes(
            source,
            ModelAssetName::ModelOnnx,
            MAX_MODEL_BYTES,
            &manifest.model_sha256,
        )?;
        initialize_runtime()?;
        #[cfg(target_os = "macos")]
        let environment_user = process_environment::SessionLease::acquire()?;
        let session = new_session(&bytes, &manifest)?;
        Ok(Self {
            manifest,
            identity,
            tokenizer,
            raw_tokenizer,
            graph: bytes.into_owned(),
            sessions: Mutex::new(vec![session]),
            #[cfg(target_os = "macos")]
            _environment_user: environment_user,
            pad_id,
            special_tokens,
            #[cfg(feature = "test-support")]
            _evaluation_lease: evaluation_lease,
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

    fn input_tensors(&self, input: &EncodedInput) -> Result<InputTensors> {
        let length = shape_length(input.encoding.len(), self.manifest.max_tokens)?;
        let mut ids = vec![i64::from(self.pad_id); length];
        let mut mask = vec![0i64; length];
        let mut types = vec![0i64; length];
        for index in 0..input.encoding.len() {
            ids[index] = i64::from(input.encoding.get_ids()[index]);
            mask[index] = i64::from(input.encoding.get_attention_mask()[index]);
            types[index] = i64::from(input.encoding.get_type_ids()[index]);
        }
        Ok(InputTensors { ids, mask, types })
    }
    #[cfg(feature = "test-support")]
    pub(crate) fn evaluation_inputs(&self, text: &str) -> Result<serde_json::Value> {
        crate::native_inference::require_service_compute()?;
        let encoded = self.encode_input(text)?.context("blank evaluation input")?;
        let InputTensors { ids, mask, types } = self.input_tensors(&encoded)?;
        Ok(serde_json::json!({"input_ids":[ids],"attention_mask":[mask],"token_type_ids":[types]}))
    }
    fn run_encoded(
        &self,
        session: &mut Session,
        input: &EncodedInput,
        memory_budget: usize,
    ) -> Result<(Vec<f32>, EmbeddingCacheStats)> {
        crate::native_inference::require_service_compute()?;
        ensure!(
            memory_budget >= EXECUTION_RESERVE_BYTES,
            "INT8 execution exceeds working-memory budget"
        );
        let InputTensors { ids, mask, types } = self.input_tensors(input)?;
        let length = ids.len();
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
    fn embed_on(
        &self,
        session: &mut Session,
        text: &str,
        memory_budget: usize,
    ) -> Result<(Option<Vec<f32>>, EmbeddingCacheStats)> {
        let Some(input) = self.encode_input(text)? else {
            return Ok((None, EmbeddingCacheStats::default()));
        };
        let (vector, stats) = self.run_encoded(session, &input, memory_budget)?;
        Ok((Some(vector), stats))
    }
    fn lock_sessions(&self) -> Result<std::sync::MutexGuard<'_, Vec<Session>>> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("INT8 session lock poisoned"))?;
        ensure!(!sessions.is_empty(), "INT8 session pool is empty");
        Ok(sessions)
    }
    pub(crate) fn embed_text_measured(
        &self,
        text: &str,
        memory_budget: usize,
    ) -> Result<(Option<Vec<f32>>, EmbeddingCacheStats)> {
        let mut sessions = self.lock_sessions()?;
        self.embed_on(&mut sessions[0], text, memory_budget)
    }
    /// Spreads `texts` round-robin over the sessions that the caller's worker
    /// count and the memory budget admit, one text per forward, and returns the
    /// vectors in input order. Missing sessions are created here, on the
    /// calling compute thread, and kept for later batches.
    pub(crate) fn embed_batch_measured(
        &self,
        texts: &[&str],
        requested_workers: usize,
        memory_budget: usize,
    ) -> Result<(Vec<Vec<f32>>, EmbeddingCacheStats)> {
        ensure!(
            !texts.is_empty() && texts.len() <= crate::embedding_batch::MAX_BATCH_CHUNKS,
            "invalid INT8 batch size"
        );
        crate::native_inference::require_service_compute()?;
        ensure!(
            memory_budget >= EXECUTION_RESERVE_BYTES,
            "INT8 execution exceeds working-memory budget"
        );
        let count =
            crate::embedding_batch::session_count(requested_workers, texts.len(), memory_budget);
        let mut sessions = self.lock_sessions()?;
        // Idle sessions that a smaller budget no longer admits are released.
        sessions.truncate(crate::embedding_batch::retained_sessions(memory_budget).max(count));
        while sessions.len() < count {
            let session = new_session(&self.graph, &self.manifest)?;
            sessions.push(session);
        }
        let run_lane = |(session, lane): (&mut Session, Vec<(usize, &str)>)| {
            lane.into_iter()
                .map(|(index, text)| {
                    let (vector, execution) = self.embed_on(session, text, memory_budget)?;
                    let vector = vector.context("blank input passed to INT8 execution batch")?;
                    Ok((index, vector, execution))
                })
                .collect::<Result<Vec<_>>>()
        };
        let lanes = sessions
            .iter_mut()
            .take(count)
            .enumerate()
            .map(|(lane, session)| {
                let texts = texts
                    .iter()
                    .copied()
                    .enumerate()
                    .skip(lane)
                    .step_by(count)
                    .collect::<Vec<_>>();
                (session, texts)
            })
            .collect::<Vec<_>>();
        let lanes = if count == 1 {
            lanes
                .into_iter()
                .map(run_lane)
                .collect::<Result<Vec<_>>>()?
        } else {
            crate::native_inference::scoped_compute(lanes, run_lane)?
        };
        let mut computed = lanes.into_iter().flatten().collect::<Vec<_>>();
        computed.sort_unstable_by_key(|(index, _, _)| *index);
        ensure!(
            computed.len() == texts.len()
                && computed
                    .iter()
                    .enumerate()
                    .all(|(index, (position, _, _))| index == *position),
            "INT8 session pool lost an input"
        );
        let mut stats = EmbeddingCacheStats {
            workers: count,
            plan_cache_peak_bytes: (sessions.len() * EXECUTION_RESERVE_BYTES) as u64,
            ..Default::default()
        };
        let vectors = computed
            .into_iter()
            .map(|(_, vector, execution)| {
                stats.add_execution(&execution);
                vector
            })
            .collect();
        Ok((vectors, stats))
    }
    pub(crate) fn embed_text(&self, text: &str) -> Result<Option<Vec<f32>>> {
        Ok(self.embed_text_measured(text, 1 << 30)?.0)
    }
    pub(crate) fn plan_cache_peak_bytes(&self) -> Result<u64> {
        Ok((self.lock_sessions()?.len() * EXECUTION_RESERVE_BYTES) as u64)
    }
    #[cfg(test)]
    fn pooled_sessions(&self) -> Result<usize> {
        Ok(self.lock_sessions()?.len())
    }
}

/// One single-threaded session of the pinned graph, checked against the
/// manifest's input and output names. Every pooled session is built this way.
fn new_session(bytes: &[u8], manifest: &OnnxManifest) -> Result<Session> {
    crate::native_inference::require_service_compute()?;
    let session = Session::builder()?
        .with_intra_threads(1)?
        .with_inter_threads(1)?
        .with_parallel_execution(false)?
        .with_memory_pattern(false)?
        .with_config_entry("session.intra_op.allow_spinning", "0")?
        .with_config_entry("session.inter_op.allow_spinning", "0")?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .commit_from_memory(bytes)
        .context("load pinned INT8 graph")?;
    let expected: std::collections::BTreeSet<_> = [&manifest.input_ids, &manifest.attention_mask]
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
    Ok(session)
}

/// Initialize the engine linked into the authenticated service image. No
/// ambient search path or external ORT image is read. Optional G5 math settings
/// are observed before initialization; this path never changes the environment.
pub(crate) fn initialize_runtime() -> Result<()> {
    crate::native_inference::require_service_compute()?;
    static READY: std::sync::OnceLock<std::result::Result<(), String>> = std::sync::OnceLock::new();
    READY
        .get_or_init(|| {
            (|| -> Result<()> {
                #[cfg(feature = "gpu-cuda")]
                crate::arbiter::runtime::cuda::capture_at_runtime_init();
                // SAFETY: the pinned static ONNX Runtime API base and its version
                // string have process lifetime. Neither call creates an OrtEnv.
                let version = unsafe {
                    let base = ort::sys::OrtGetApiBase();
                    ensure!(!base.is_null(), "linked INT8 runtime API base unavailable");
                    let text = ((*base).GetVersionString)();
                    ensure!(!text.is_null(), "linked INT8 runtime version unavailable");
                    std::ffi::CStr::from_ptr(text).to_str()?
                };
                ensure!(
                    version == model::ONNX_RUNTIME_VERSION,
                    "linked INT8 runtime version differs from approved static runtime"
                );
                ensure!(
                    ort::init().with_name("AIRK INT8").commit()?,
                    "INT8 runtime was initialized outside AIRK"
                );
                #[cfg(target_os = "macos")]
                process_environment::install()?;
                Ok(())
            })()
            .map_err(|error| format!("{error:#}"))
        })
        .as_ref()
        .map_err(|error| anyhow::anyhow!(error.clone()))?;
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
        let _native_owner = crate::native_inference::fixture::NativeCompute::admit()?;
        let root = std::env::var_os("AIRK_BUNDLED_MODEL_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/local-vector-model")
            });
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
            assert_eq!(stats.workers, 3, "one pooled session for each text");
            assert_eq!(stats.truncated_inputs, 1);
            assert!(model.embed_text("   ")?.is_none());
            assert!(model.embed_text_measured(short, 1024).is_err());
        }
        Ok(())
    }

    /// Deterministic prose of about `words` words from a fixed vocabulary, so
    /// each text is distinct and runs to roughly 200 to 256 tokens.
    fn realistic_texts(count: usize, words: usize) -> Vec<String> {
        const VOCABULARY: &[&str] = &[
            "the",
            "index",
            "retrieval",
            "kernel",
            "reads",
            "artifact",
            "revision",
            "chapter",
            "parser",
            "returns",
            "bounded",
            "evidence",
            "for",
            "each",
            "query",
            "and",
            "keeps",
            "original",
            "document",
            "locations",
            "while",
            "workers",
            "embed",
            "vectors",
            "cache",
            "service",
            "configuration",
            "memory",
            "budget",
            "admission",
            "of",
            "a",
            "session",
            "tokenizer",
            "graph",
            "publishes",
            "failure",
            "retry",
            "policy",
            "pipeline",
            "café",
            "naïve",
            "résumé",
            "with",
            "under",
            "through",
            "builds",
            "verifies",
            "manifest",
            "digest",
            "readiness",
            "coverage",
            "continuation",
            "lexical",
            "semantic",
            "is",
            "not",
            "when",
            "every",
            "file",
            "changes",
            "then",
            "only",
            "rows",
            "are",
            "written",
        ];
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        (0..count)
            .map(|item| {
                let mut text = format!("Section {item}:");
                for word in 0..words {
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    text.push(' ');
                    text.push_str(VOCABULARY[(state >> 33) as usize % VOCABULARY.len()]);
                    if word % 17 == 16 {
                        text.push('.');
                    }
                }
                text
            })
            .collect()
    }

    fn bits(vectors: &[Vec<f32>]) -> Vec<Vec<u32>> {
        vectors
            .iter()
            .map(|vector| vector.iter().map(|value| value.to_bits()).collect())
            .collect()
    }

    fn test_model() -> Result<OnnxModel> {
        let directory = std::path::PathBuf::from(
            std::env::var_os("AIRK_TEST_MODEL_DIR").context("AIRK_TEST_MODEL_DIR is required")?,
        );
        OnnxModel::open(&directory, &std::fs::read(directory.join("model.json"))?)
    }

    #[test]
    #[ignore = "requires AIRK_TEST_MODEL_DIR with verified local ONNX assets and native runtime"]
    fn pooled_sessions_reproduce_single_session_vectors_bit_for_bit() -> Result<()> {
        let _native_owner = crate::native_inference::fixture::NativeCompute::admit()?;
        let model = test_model()?;
        assert_eq!(model.pooled_sessions()?, 1, "open creates one session");
        let mut owned = realistic_texts(29, 190);
        owned.push("Find the chapter generation pipeline and its retry policy.".into());
        owned.push("long unrelated content ".repeat(1024));
        owned.push("Reset an account password.".into());
        let texts = owned.iter().map(String::as_str).collect::<Vec<_>>();
        // The reference is the single-session path, one text at a time.
        let reference = texts
            .iter()
            .map(|text| model.embed_text(text)?.context("reference vector"))
            .collect::<Result<Vec<_>>>()?;
        // A second, separately opened model checks that the reference does not
        // depend on the first model's session history.
        let fresh = test_model()?;
        let (fresh_vectors, _) = fresh.embed_batch_measured(&texts, 1, 1 << 30)?;
        assert_eq!(bits(&fresh_vectors), bits(&reference));
        drop(fresh);
        let budget = crate::embedding_batch::pooled_model_bytes(32) as usize;
        let mut created = 1;
        for workers in [1, 2, 4, 8, 16, 32, 4] {
            let (pooled, stats) = model.embed_batch_measured(&texts, workers, budget)?;
            assert_eq!(
                bits(&pooled),
                bits(&reference),
                "{workers} sessions changed a vector or its position"
            );
            assert_eq!(stats.workers, workers);
            assert_eq!(stats.actual_forwards, texts.len() as u64);
            created = created.max(workers);
            assert_eq!(
                model.pooled_sessions()?,
                created,
                "sessions are created lazily and reused"
            );
        }
        // A subset in another order keeps each text's exact vector.
        let order = [31, 0, 17, 30, 3, 29, 8];
        let subset = order.iter().map(|index| texts[*index]).collect::<Vec<_>>();
        let (shuffled, _) = model.embed_batch_measured(&subset, 16, budget)?;
        assert_eq!(
            bits(&shuffled),
            bits(
                &order
                    .iter()
                    .map(|index| reference[*index].clone())
                    .collect::<Vec<_>>()
            )
        );
        // Memory admission: 1 GiB admits five sessions and releases the rest.
        let (bounded, stats) = model.embed_batch_measured(&texts, 16, 1 << 30)?;
        assert_eq!(bits(&bounded), bits(&reference));
        assert_eq!(stats.workers, 5);
        assert_eq!(model.pooled_sessions()?, 5);
        assert_eq!(
            model.plan_cache_peak_bytes()?,
            5 * EXECUTION_RESERVE_BYTES as u64
        );
        assert!(
            model
                .embed_batch_measured(&texts, 16, EXECUTION_RESERVE_BYTES - 1)
                .is_err()
        );
        assert!(model.embed_batch_measured(&["   "], 4, budget).is_err());
        Ok(())
    }

    /// Embedding rate for 1, 4, 8 and 16 sessions on 512 texts of about 200 to
    /// 256 tokens, in batches of the largest size the batch path allows.
    #[test]
    #[ignore = "measurement; requires AIRK_TEST_MODEL_DIR and native runtime"]
    fn session_pool_embedding_rate() -> Result<()> {
        let _native_owner = crate::native_inference::fixture::NativeCompute::admit()?;
        let model = test_model()?;
        let owned = realistic_texts(512, 190);
        let texts = owned.iter().map(String::as_str).collect::<Vec<_>>();
        let tokens = texts
            .iter()
            .map(|text| {
                Ok(model
                    .encode_input(text)?
                    .context("blank text")?
                    .input_tokens)
            })
            .collect::<Result<Vec<_>>>()?;
        eprintln!(
            "texts={} input_tokens min={} mean={} max={} logical_cpus={}",
            texts.len(),
            tokens.iter().min().unwrap_or(&0),
            tokens.iter().sum::<usize>() / tokens.len(),
            tokens.iter().max().unwrap_or(&0),
            std::thread::available_parallelism().map_or(1, usize::from)
        );
        let budget = crate::embedding_batch::pooled_model_bytes(32) as usize;
        let mut reference = None;
        for workers in [1, 4, 8, 16] {
            // Warm: create the sessions outside the timed run.
            model.embed_batch_measured(&texts[..workers], workers, budget)?;
            let started = std::time::Instant::now();
            let mut vectors = Vec::with_capacity(texts.len());
            for batch in texts.chunks(crate::embedding_batch::MAX_BATCH_CHUNKS) {
                vectors.extend(model.embed_batch_measured(batch, workers, budget)?.0);
            }
            let seconds = started.elapsed().as_secs_f64();
            let vectors = bits(&vectors);
            match &reference {
                None => reference = Some(vectors),
                Some(expected) => assert_eq!(&vectors, expected),
            }
            eprintln!(
                "sessions={workers} seconds={seconds:.3} texts_per_second={:.1}",
                texts.len() as f64 / seconds
            );
        }
        Ok(())
    }
}
