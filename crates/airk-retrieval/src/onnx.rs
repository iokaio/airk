// SPDX-License-Identifier: Apache-2.0
//! Bounded CPU ONNX shapes, shared plans and masked mean pooling.
use crate::model::{self, EmbeddingCacheStats, MAX_MODEL_BYTES, MAX_TOKENIZER_BYTES, OnnxManifest};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    io::Cursor,
    path::Path,
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};
use tokenizers::{Encoding, PostProcessor, Tokenizer, TruncationDirection};
use tract_onnx::{pb, prelude::*};

const MAX_PLAN_CACHE_BYTES: usize = 1 << 30;
const MAX_CACHED_PLANS: usize = 8;
const EXECUTION_RESERVE_BYTES: usize = 8 << 20;

struct CompiledPlan {
    runnable: Arc<TypedRunnableModel>,
    declared_bytes: usize,
    workspace_bytes: usize,
}

struct PlanEntry {
    plan: Arc<CompiledPlan>,
    used: u64,
}

struct PlanState {
    entries: BTreeMap<(usize, usize), PlanEntry>,
    declared_bytes: usize,
    peak_bytes: usize,
    active_bytes: usize,
    tick: u64,
    maximum_bytes: usize,
    maximum_plans: usize,
}

struct PlanStore {
    state: Mutex<PlanState>,
    available: Condvar,
}

struct PlanLease<'a> {
    store: &'a PlanStore,
    plan: Option<Arc<CompiledPlan>>,
    workspace_bytes: usize,
}

impl PlanLease<'_> {
    fn reserve_workspace(&mut self, budget: usize) -> Result<()> {
        let needed = self
            .plan
            .as_ref()
            .context("ONNX plan missing")?
            .workspace_bytes
            .checked_add(EXECUTION_RESERVE_BYTES)
            .context("ONNX workspace allocation overflow")?;
        ensure!(
            needed <= budget,
            "ONNX execution exceeds working-memory budget"
        );
        let mut state = self
            .store
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("ONNX plan cache lock poisoned"))?;
        while state.active_bytes > budget - needed {
            state = self
                .store
                .available
                .wait(state)
                .map_err(|_| anyhow::anyhow!("ONNX workspace lock poisoned"))?;
        }
        state.active_bytes += needed;
        self.workspace_bytes = needed;
        Ok(())
    }
}

impl Drop for PlanLease<'_> {
    fn drop(&mut self) {
        // Release the Arc before waking cache/workspace waiters.
        drop(self.plan.take());
        if let Ok(mut state) = self.store.state.lock() {
            state.active_bytes = state.active_bytes.saturating_sub(self.workspace_bytes);
            self.store.available.notify_all();
        }
    }
}

struct EncodedInput {
    encoding: Encoding,
    input_tokens: usize,
    truncated: bool,
}

struct BatchTask {
    indices: Vec<usize>,
    batch: usize,
    length: usize,
}

use crate::model::qualified_batch_four;

fn schedule_shapes(lengths: &[usize], use_batch_four: bool) -> Vec<BatchTask> {
    let mut groups = BTreeMap::<usize, Vec<usize>>::new();
    for (index, length) in lengths.iter().copied().enumerate() {
        groups.entry(length).or_default().push(index);
    }
    let mut tasks = Vec::new();
    for (length, indices) in groups {
        let mut chunks = indices.chunks_exact(if use_batch_four && length == 32 { 4 } else { 1 });
        for chunk in &mut chunks {
            tasks.push(BatchTask {
                indices: chunk.to_vec(),
                batch: chunk.len(),
                length,
            });
        }
        for index in chunks.remainder() {
            tasks.push(BatchTask {
                indices: vec![*index],
                batch: 1,
                length,
            });
        }
    }
    tasks
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

pub struct OnnxModel {
    pub(crate) manifest: OnnxManifest,
    pub(crate) identity: String,
    tokenizer: Tokenizer,
    raw_tokenizer: Tokenizer,
    template: InferenceModel,
    plans: PlanStore,
    pad_id: u32,
    special_tokens: usize,
    inputs: Vec<String>,
}

struct NoExternalData;
impl tract_onnx::data_resolver::ModelDataResolver for NoExternalData {
    fn read_bytes_from_path(
        &self,
        _: &mut Vec<u8>,
        _: &Path,
        _: usize,
        _: Option<usize>,
    ) -> TractResult<()> {
        anyhow::bail!("external ONNX tensor files are unsupported")
    }
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

fn validate_tensor(tensor: &pb::TensorProto) -> Result<()> {
    ensure!(
        tensor.external_data.is_empty() && tensor.data_location.unwrap_or(0) == 0,
        "external ONNX tensor data is forbidden"
    );
    ensure!(tensor.dims.len() <= 8, "ONNX tensor rank exceeds limit");
    let elements = tensor.dims.iter().try_fold(1u64, |size, dim| {
        size.checked_mul(u64::try_from(*dim)?)
            .context("ONNX tensor size overflow")
    })?;
    ensure!(elements <= 33_554_432, "ONNX tensor exceeds element budget");
    ensure!(
        [1, 6, 7, 9, 10, 11].contains(&tensor.data_type),
        "unsupported ONNX tensor type"
    );
    Ok(())
}

fn validate_graph(proto: &pb::ModelProto) -> Result<usize> {
    ensure!(
        proto.functions.is_empty() && proto.training_info.is_empty(),
        "ONNX functions/training graphs are unsupported"
    );
    let graph = proto.graph.as_ref().context("ONNX graph missing")?;
    ensure!(
        graph.node.len() <= 4096
            && graph.initializer.len() <= 4096
            && graph.sparse_initializer.is_empty(),
        "ONNX graph exceeds supported bounds"
    );
    ensure!(
        graph.input.len() <= 3 && graph.output.len() == 1,
        "ONNX graph requires two or three inputs and one output"
    );
    let mut tensor_elements = 0u64;
    let mut constant_bytes = 0usize;
    let mut check_tensor = |tensor: &pb::TensorProto| -> Result<()> {
        validate_tensor(tensor)?;
        let elements = tensor.dims.iter().try_fold(1u64, |size, dim| {
            size.checked_mul(u64::try_from(*dim)?)
                .context("ONNX constant size overflow")
        })?;
        tensor_elements = tensor_elements
            .checked_add(elements)
            .context("ONNX constant budget overflow")?;
        let width = match tensor.data_type {
            1 | 6 => 4,
            7 | 11 => 8,
            9 => 1,
            10 => 2,
            _ => anyhow::bail!("unsupported ONNX tensor type"),
        };
        constant_bytes = constant_bytes
            .checked_add(
                usize::try_from(elements)?
                    .checked_mul(width)
                    .context("ONNX constant byte overflow")?,
            )
            .context("ONNX constant byte overflow")?;
        ensure!(
            tensor_elements <= 67_108_864,
            "ONNX constant tensors exceed cumulative element budget"
        );
        Ok(())
    };
    for tensor in &graph.initializer {
        check_tensor(tensor)?;
    }
    for node in &graph.node {
        ensure!(
            node.domain.is_empty() || node.domain == "ai.onnx",
            "custom ONNX operator domains are unsupported"
        );
        ensure!(
            [
                "Constant",
                "Identity",
                "Add",
                "Shape",
                "Gather",
                "Unsqueeze",
                "MatMul",
                "Mul",
                "Sqrt",
                "ReduceMean",
                "Div",
                "Concat",
                "Reshape",
                "Transpose",
                "Cast",
                "Sub",
                "Pow",
                "Slice",
                "Softmax",
                "Erf",
                "Where",
                "ConstantOfShape",
                "Equal",
                "Expand",
                "Squeeze",
                "Gemm",
                "Relu",
                "Tanh"
            ]
            .contains(&node.op_type.as_str()),
            "ONNX operator is outside the supported BERT profile"
        );
        ensure!(
            node.attribute.len() <= 64 && node.input.len() <= 64 && node.output.len() <= 16,
            "ONNX operator exceeds structural bounds"
        );
        for attribute in &node.attribute {
            ensure!(
                attribute.g.is_none()
                    && attribute.graphs.is_empty()
                    && attribute.sparse_tensor.is_none()
                    && attribute.sparse_tensors.is_empty(),
                "nested ONNX graphs/sparse tensors are unsupported"
            );
            for tensor in attribute.t.iter().chain(attribute.tensors.iter()) {
                check_tensor(tensor)?;
            }
        }
    }
    constant_bytes
        .checked_add(EXECUTION_RESERVE_BYTES)
        .context("ONNX template allocation overflow")
}

fn validate_plan(model: &TypedModel) -> Result<usize> {
    let mut total = 0usize;
    for node in model.nodes() {
        for output in &node.outputs {
            let shape = output
                .fact
                .shape
                .as_concrete()
                .context("ONNX intermediate tensor has dynamic shape")?;
            ensure!(
                shape.len() <= 8,
                "ONNX intermediate tensor rank exceeds limit"
            );
            let elements = shape.iter().try_fold(1usize, |size, dim| {
                size.checked_mul(*dim)
                    .context("ONNX intermediate tensor overflow")
            })?;
            let bytes = elements
                .checked_mul(output.fact.datum_type.size_of())
                .context("ONNX intermediate byte overflow")?;
            ensure!(
                bytes <= 134_217_728,
                "ONNX intermediate tensor exceeds 128 MiB"
            );
            total = total
                .checked_add(bytes)
                .context("ONNX graph allocation overflow")?;
        }
    }
    ensure!(
        total <= 1_073_741_824,
        "ONNX graph declared tensor sizes exceed 1 GiB"
    );
    Ok(total)
}

fn workspace_bytes(model: &TypedModel) -> Result<usize> {
    model
        .nodes()
        .iter()
        .flat_map(|node| &node.outputs)
        .filter(|output| output.fact.konst.is_none())
        .try_fold(0usize, |total, output| {
            let shape = output
                .fact
                .shape
                .as_concrete()
                .context("dynamic ONNX workspace")?;
            let bytes = shape
                .iter()
                .try_fold(output.fact.datum_type.size_of(), |size, dim| {
                    size.checked_mul(*dim).context("ONNX workspace overflow")
                })?;
            total.checked_add(bytes).context("ONNX workspace overflow")
        })
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
        let mut framework = tract_onnx::onnx();
        framework.provider = Arc::new(NoExternalData);
        let proto = framework.proto_model_for_read(&mut Cursor::new(bytes))?;
        let template_bytes = validate_graph(&proto)?;
        ensure!(
            template_bytes <= MAX_PLAN_CACHE_BYTES,
            "ONNX template exceeds plan cache budget"
        );
        let graph = proto.graph.as_ref().context("ONNX graph missing")?;
        let inputs: Vec<_> = graph.input.iter().map(|input| input.name.clone()).collect();
        let expected: std::collections::BTreeSet<_> =
            [&manifest.input_ids, &manifest.attention_mask]
                .into_iter()
                .chain(manifest.token_type_ids.iter())
                .cloned()
                .collect();
        ensure!(
            inputs
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
                == expected
                && inputs.len() == expected.len(),
            "ONNX input names differ from manifest"
        );
        ensure!(
            graph.output[0].name == manifest.output,
            "ONNX output differs from manifest"
        );
        let template = framework.model_for_proto_model(&proto)?;
        drop(proto);
        Ok(Self {
            manifest,
            identity,
            tokenizer,
            raw_tokenizer,
            template,
            plans: PlanStore {
                state: Mutex::new(PlanState {
                    entries: BTreeMap::new(),
                    declared_bytes: template_bytes,
                    peak_bytes: template_bytes,
                    active_bytes: 0,
                    tick: 0,
                    maximum_bytes: MAX_PLAN_CACHE_BYTES,
                    maximum_plans: MAX_CACHED_PLANS,
                }),
                available: Condvar::new(),
            },
            pad_id,
            special_tokens,
            inputs,
        })
    }

    fn compile_plan(&self, batch: usize, length: usize) -> Result<CompiledPlan> {
        let mut inference = self.template.clone();
        for input in 0..self.inputs.len() {
            inference.set_input_fact(input, i64::fact([batch, length]).into())?;
        }
        let typed = inference
            .into_typed()
            .context("infer bounded ONNX shapes")?;
        validate_plan(&typed)?;
        let output = typed.output_fact(0)?;
        ensure!(
            output.datum_type == f32::datum_type()
                && output.shape.as_concrete()
                    == Some(&[batch, length, self.manifest.dimensions][..]),
            "ONNX output must be float32 [batch,tokens,dimensions]"
        );
        let optimized = typed.into_optimized()?;
        let declared_bytes = validate_plan(&optimized)?;
        let workspace_bytes = workspace_bytes(&optimized)?;
        Ok(CompiledPlan {
            runnable: optimized.into_runnable()?,
            declared_bytes,
            workspace_bytes,
        })
    }

    fn acquire_plan(
        &self,
        batch: usize,
        length: usize,
    ) -> Result<(PlanLease<'_>, EmbeddingCacheStats)> {
        ensure!(
            [1, 4, 8].contains(&batch) && (1..=self.manifest.max_tokens).contains(&length),
            "unsupported ONNX execution shape"
        );
        let mut state = self
            .plans
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("ONNX plan cache lock poisoned"))?;
        state.tick = state.tick.saturating_add(1);
        let tick = state.tick;
        let mut stats = EmbeddingCacheStats::default();
        let key = (batch, length);
        let plan = if let Some(entry) = state.entries.get_mut(&key) {
            entry.used = tick;
            entry.plan.clone()
        } else {
            // One compilation at a time. Template constant tensors are shared;
            // validation bounds both the temporary typed graph and retained plans.
            let started = Instant::now();
            let compiled = self.compile_plan(batch, length).with_context(|| {
                format!("ONNX model cannot execute shape [{batch},{length}]; use a supported dynamic BERT export")
            })?;
            stats.plan_compile_us = crate::embedding_batch::elapsed_us(started);
            while state.entries.len() >= state.maximum_plans
                || compiled.declared_bytes
                    > state.maximum_bytes.saturating_sub(state.declared_bytes)
            {
                let victim = state
                    .entries
                    .iter()
                    .filter(|(_, entry)| Arc::strong_count(&entry.plan) == 1)
                    .min_by_key(|(_, entry)| entry.used)
                    .map(|(key, _)| *key)
                    .context("ONNX plan cache budget exhausted; reduce concurrent shape work")?;
                let removed = state
                    .entries
                    .remove(&victim)
                    .context("ONNX plan eviction failed")?;
                state.declared_bytes -= removed.plan.declared_bytes;
            }
            state.declared_bytes += compiled.declared_bytes;
            state.peak_bytes = state.peak_bytes.max(state.declared_bytes);
            let plan = Arc::new(compiled);
            state.entries.insert(
                key,
                PlanEntry {
                    plan: plan.clone(),
                    used: tick,
                },
            );
            stats.plan_compilations = 1;
            plan
        };
        stats.plan_cache_peak_bytes = state.peak_bytes as u64;
        Ok((
            PlanLease {
                store: &self.plans,
                plan: Some(plan),
                workspace_bytes: 0,
            },
            stats,
        ))
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
        rows: &[(usize, &EncodedInput)],
        batch: usize,
        length: usize,
        memory_budget: usize,
    ) -> Result<(Vec<Vec<f32>>, EmbeddingCacheStats)> {
        ensure!(
            !rows.is_empty() && rows.len() <= batch,
            "invalid ONNX batch occupancy"
        );
        let mut occupied = std::collections::BTreeSet::new();
        for (row, input) in rows {
            ensure!(
                *row < batch && occupied.insert(*row) && input.encoding.len() <= length,
                "invalid ONNX input row"
            );
        }
        let (mut lease, mut stats) = self.acquire_plan(batch, length)?;
        lease.reserve_workspace(memory_budget)?;
        let mut ids = vec![i64::from(self.pad_id); batch * length];
        let mut mask = vec![0i64; batch * length];
        let mut types = vec![0i64; batch * length];
        for (row, input) in rows {
            let encoding = &input.encoding;
            let offset = row * length;
            for index in 0..encoding.len() {
                ids[offset + index] = i64::from(encoding.get_ids()[index]);
                mask[offset + index] = i64::from(encoding.get_attention_mask()[index]);
                types[offset + index] = i64::from(encoding.get_type_ids()[index]);
            }
            stats.tokenized_inputs += 1;
            stats.input_tokens += input.input_tokens as u64;
            stats.embedded_tokens += encoding
                .get_attention_mask()
                .iter()
                .map(|mask| u64::from(*mask))
                .sum::<u64>();
            stats.truncated_inputs += u64::from(input.truncated);
        }
        stats.padding_tokens = (batch * length) as u64 - stats.embedded_tokens;
        let inputs: TVec<TValue> = self
            .inputs
            .iter()
            .map(|name| {
                let values = if name == &self.manifest.input_ids {
                    &ids
                } else if name == &self.manifest.attention_mask {
                    &mask
                } else {
                    &types
                };
                Ok(Tensor::from_shape(&[batch, length], values)?.into())
            })
            .collect::<Result<_>>()?;
        let outputs =
            multithread::multithread_tract_scope(multithread::Executor::SingleThread, || {
                lease
                    .plan
                    .as_ref()
                    .context("ONNX plan missing")?
                    .runnable
                    .run(inputs)
            })
            .context("local ONNX inference failed")?;
        ensure!(
            outputs.len() == 1 && outputs[0].shape() == [batch, length, self.manifest.dimensions],
            "ONNX output shape changed"
        );
        let view = outputs[0].to_plain_array_view::<f32>()?;
        let values = view.as_slice().context("ONNX output is not contiguous")?;
        let mut vectors = Vec::with_capacity(rows.len());
        for (row, _) in rows {
            let start = row * length;
            let values = &values
                [start * self.manifest.dimensions..(start + length) * self.manifest.dimensions];
            let mut sum = vec![0.0f64; self.manifest.dimensions];
            for (token, mask) in values
                .chunks_exact(self.manifest.dimensions)
                .zip(&mask[start..start + length])
            {
                if *mask == 0 {
                    continue;
                }
                for (total, value) in sum.iter_mut().zip(token) {
                    ensure!(value.is_finite(), "ONNX output contains nonfinite values");
                    *total += f64::from(*value);
                }
            }
            let norm = sum.iter().map(|value| value * value).sum::<f64>().sqrt();
            ensure!(
                norm.is_finite() && norm > 0.0,
                "ONNX model produced an empty embedding"
            );
            vectors.push(sum.into_iter().map(|value| (value / norm) as f32).collect());
        }
        stats.actual_forwards = 1;
        Ok((vectors, stats))
    }

    pub(crate) fn embed_text_measured(
        &self,
        text: &str,
        memory_budget: usize,
    ) -> Result<(Option<Vec<f32>>, EmbeddingCacheStats)> {
        let Some(input) = self.encode_input(text)? else {
            return Ok((None, EmbeddingCacheStats::default()));
        };
        let length = shape_length(input.encoding.len(), self.manifest.max_tokens)?;
        let (mut vectors, stats) = self.run_encoded(&[(0, &input)], 1, length, memory_budget)?;
        Ok((vectors.pop(), stats))
    }

    pub(crate) fn embed_batch_measured(
        &self,
        texts: &[&str],
        requested_workers: usize,
        memory_budget: usize,
    ) -> Result<(Vec<Vec<f32>>, EmbeddingCacheStats)> {
        ensure!(
            !texts.is_empty() && texts.len() <= crate::embedding_batch::MAX_BATCH_CHUNKS,
            "invalid ONNX execution batch size"
        );
        let inputs: Vec<_> = texts
            .iter()
            .map(|text| {
                self.encode_input(text)?
                    .context("blank input passed to ONNX execution batch")
            })
            .collect::<Result<_>>()?;
        let lengths = inputs
            .iter()
            .map(|input| shape_length(input.encoding.len(), self.manifest.max_tokens))
            .collect::<Result<Vec<_>>>()?;
        let tasks = schedule_shapes(&lengths, qualified_batch_four(&self.manifest));
        let workers = requested_workers.clamp(1, 8).min(tasks.len());
        let workers = tasks.len().div_ceil(tasks.len().div_ceil(workers));
        let computed = crate::embedding_batch::parallel_map(&tasks, workers, |task| {
            let rows = task
                .indices
                .iter()
                .enumerate()
                .map(|(row, index)| (row, &inputs[*index]))
                .collect::<Vec<_>>();
            self.run_encoded(&rows, task.batch, task.length, memory_budget)
        })?;
        let mut vectors = vec![None; inputs.len()];
        let mut stats = EmbeddingCacheStats {
            workers,
            ..Default::default()
        };
        for (task, (results, execution)) in tasks.into_iter().zip(computed) {
            stats.add_execution(&execution);
            for (index, vector) in task.indices.into_iter().zip(results) {
                vectors[index] = Some(vector);
            }
        }
        Ok((
            vectors
                .into_iter()
                .map(|vector| vector.context("ONNX batch result missing"))
                .collect::<Result<_>>()?,
            stats,
        ))
    }

    pub(crate) fn embed_text(&self, text: &str) -> Result<Option<Vec<f32>>> {
        Ok(self.embed_text_measured(text, 1 << 30)?.0)
    }

    pub(crate) fn plan_cache_peak_bytes(&self) -> Result<u64> {
        Ok(self
            .plans
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("ONNX plan cache lock poisoned"))?
            .peak_bytes as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_full_qualified_short_groups_use_batch_four() {
        let lengths = [32, 64, 32, 32, 128, 32, 32];
        let tasks = schedule_shapes(&lengths, true);
        assert_eq!(
            tasks
                .iter()
                .map(|task| (task.batch, task.length, task.indices.clone()))
                .collect::<Vec<_>>(),
            vec![
                (4, 32, vec![0, 2, 3, 5]),
                (1, 32, vec![6]),
                (1, 64, vec![1]),
                (1, 128, vec![4])
            ]
        );
        assert!(
            schedule_shapes(&lengths, false)
                .iter()
                .all(|task| task.batch == 1)
        );
        assert!(
            schedule_shapes(&[64; 8], true)
                .iter()
                .all(|task| task.batch == 1)
        );
    }

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

    fn difference(left: &[f32], right: &[f32]) -> (f64, f32) {
        let cosine = left
            .iter()
            .zip(right)
            .map(|(a, b)| f64::from(*a) * f64::from(*b))
            .sum();
        let maximum = left
            .iter()
            .zip(right)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        (cosine, maximum)
    }

    fn execute_fixture(
        model: &OnnxModel,
        inputs: &[EncodedInput],
        batch: usize,
        length: usize,
        workers: usize,
    ) -> Result<Vec<Vec<f32>>> {
        std::thread::scope(|scope| {
            let handles: Vec<_> = inputs
                .chunks(inputs.len().div_ceil(workers))
                .map(|part| {
                    scope.spawn(move || {
                        let mut output = Vec::new();
                        for chunk in part.chunks(batch) {
                            let rows: Vec<_> = chunk.iter().enumerate().collect();
                            output.extend(model.run_encoded(&rows, batch, length, 1 << 30)?.0);
                        }
                        Ok(output)
                    })
                })
                .collect();
            let results: Vec<Result<Vec<Vec<f32>>>> = handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .map_err(|_| anyhow::anyhow!("qualification worker panicked"))?
                })
                .collect();
            Ok(results
                .into_iter()
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect())
        })
    }

    #[test]
    #[ignore = "requires AIRK_TEST_MODEL_DIR with verified local ONNX assets"]
    fn real_model_shape_policy_qualification() -> Result<()> {
        use tokenizers::{PaddingParams, PaddingStrategy, TruncationParams};
        let opened =
            model::EmbeddingModel::open(Path::new(&std::env::var("AIRK_TEST_MODEL_DIR")?))?;
        let model::EmbeddingModel::Onnx(model) = opened else {
            anyhow::bail!("ONNX model required")
        };
        let mut changed = model.manifest.clone();
        changed.model_sha256 = "0".repeat(64);
        assert!(!qualified_batch_four(&changed));
        changed = model.manifest.clone();
        changed.tokenizer_sha256 = "0".repeat(64);
        assert!(!qualified_batch_four(&changed));
        let mut old_tokenizer = model.tokenizer.clone();
        old_tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: model.manifest.max_tokens,
                stride: 0,
                ..Default::default()
            }))
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        old_tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::Fixed(model.manifest.max_tokens),
            pad_id: model.pad_id,
            pad_token: "[PAD]".into(),
            ..Default::default()
        }));
        let mut texts: Vec<_> = [1, 29, 30, 31, 61, 62, 63, 125, 126, 127, 253, 254, 255, 600]
            .into_iter()
            .map(|length| "token ".repeat(length))
            .collect();
        texts.extend([
            "Unicode café 中文 русский العربية 🦀\r\n".into(),
            "Literal [CLS] and [SEP] text".into(),
        ]);
        let mut maximum_delta = 0.0f32;
        let mut minimum_cosine = 1.0f64;
        let mut batch_exact = BTreeMap::from([(4, true), (8, true)]);
        let mut batch_errors = BTreeMap::<usize, String>::new();
        for text in &texts {
            let input = model
                .encode_input(text)?
                .context("qualification input is empty")?;
            let old = old_tokenizer
                .encode(model::onnx_input_prefix(text), true)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let mut ids = input.encoding.get_ids().to_vec();
            let mut mask = input.encoding.get_attention_mask().to_vec();
            let mut types = input.encoding.get_type_ids().to_vec();
            ids.resize(model.manifest.max_tokens, model.pad_id);
            mask.resize(model.manifest.max_tokens, 0);
            types.resize(model.manifest.max_tokens, 0);
            assert_eq!(ids, old.get_ids(), "token IDs changed");
            assert_eq!(mask, old.get_attention_mask(), "attention masks changed");
            assert_eq!(types, old.get_type_ids(), "type IDs changed");
            let fixed = model
                .run_encoded(&[(0, &input)], 1, model.manifest.max_tokens, 1 << 30)?
                .0
                .remove(0);
            let length = shape_length(input.encoding.len(), model.manifest.max_tokens)?;
            let bucketed = model
                .run_encoded(&[(0, &input)], 1, length, 1 << 30)?
                .0
                .remove(0);
            let (cosine, delta) = difference(&fixed, &bucketed);
            maximum_delta = maximum_delta.max(delta);
            minimum_cosine = minimum_cosine.min(cosine);
            ensure!(
                cosine > 0.99999 && delta < 0.0001,
                "length bucket changed vector beyond tolerance"
            );
            for batch in [4, 8] {
                if batch_errors.contains_key(&batch) {
                    continue;
                }
                for row in 0..batch {
                    match model.run_encoded(&[(row, &input)], batch, length, 1 << 30) {
                        Ok((vectors, stats)) => {
                            *batch_exact.get_mut(&batch).unwrap() &= vectors[0] == bucketed;
                            assert_eq!(stats.actual_forwards, 1);
                            assert_eq!(stats.tokenized_inputs, 1);
                            assert_eq!(
                                stats.embedded_tokens + stats.padding_tokens,
                                (batch * length) as u64
                            );
                        }
                        Err(error) => {
                            batch_errors.insert(batch, error.to_string());
                            break;
                        }
                    }
                }
            }
        }
        let short: Vec<_> = (0..32)
            .map(|index| {
                model
                    .encode_input(&format!("Reuse unchanged local embedding number {index}."))?
                    .context("empty benchmark input")
            })
            .collect::<Result<_>>()?;
        ensure!(
            short.iter().all(|input| input.encoding.len() <= 32),
            "benchmark shape exceeded"
        );
        let reference = execute_fixture(&model, &short, 1, 32, 4)?;
        let mut observations = Vec::new();
        for (batch, length) in [(1, 256), (1, 32), (4, 32), (8, 32)] {
            let first = execute_fixture(&model, &short, batch, length, 4)?;
            if batch > 1 {
                *batch_exact.get_mut(&batch).unwrap() &= first == reference;
            }
            let mut samples = Vec::new();
            for _ in 0..3 {
                let started = Instant::now();
                let result = execute_fixture(&model, &short, batch, length, 4)?;
                samples.push(crate::embedding_batch::elapsed_us(started));
                assert_eq!(first, result, "repeated shape executions differ");
            }
            samples.sort_unstable();
            observations.push(serde_json::json!({"batch":batch,"length":length,"workers":4,"inputs":32,"median_us":samples[1],"samples_us":samples}));
        }
        for batch in [4, 8] {
            for count in [1, 3, 5, 7, 9, 31] {
                let reordered: Vec<_> = short.iter().rev().take(count).collect();
                let mut actual = Vec::new();
                for part in reordered.chunks(batch) {
                    let rows: Vec<_> = part.iter().copied().enumerate().collect();
                    actual.extend(model.run_encoded(&rows, batch, 32, 1 << 30)?.0);
                }
                let expected: Vec<_> = reference.iter().rev().take(count).cloned().collect();
                *batch_exact.get_mut(&batch).unwrap() &= actual == expected;
            }
        }
        // Cold/warm query plan reuse and deterministic bounded eviction.
        {
            let mut state = model.plans.state.lock().unwrap();
            let removed: usize = state
                .entries
                .values()
                .map(|entry| entry.plan.declared_bytes)
                .sum();
            state.entries.clear();
            state.declared_bytes -= removed;
            state.maximum_plans = 1;
        }
        let query = "Recover an account password.";
        let (first, cold) = model.embed_text_measured(query, 1 << 30)?;
        let (second, warm) = model.embed_text_measured(query, 1 << 30)?;
        assert_eq!(first, second);
        assert_eq!((cold.plan_compilations, warm.plan_compilations), (1, 0));
        let input = model.encode_input(query)?.unwrap();
        assert!(model.run_encoded(&[(0, &input)], 1, 32, 0).is_err());
        assert_eq!(model.plans.state.lock().unwrap().active_bytes, 0);
        let (pinned, _) = model.acquire_plan(1, 32)?;
        assert!(
            model.acquire_plan(1, 64).is_err(),
            "cannot evict a live plan"
        );
        drop(pinned);
        let (_, eviction) = model.run_encoded(&[(0, &input)], 1, 64, 1 << 30)?;
        assert_eq!(eviction.plan_compilations, 1);
        assert_eq!(model.plans.state.lock().unwrap().entries.len(), 1);
        let (_, recompiled) = model.embed_text_measured(query, 1 << 30)?;
        assert_eq!(recompiled.plan_compilations, 1);
        println!(
            "{}",
            serde_json::json!({"native_shape_qualification":{
                "runtime":model::ONNX_RUNTIME,"tokenizer_cases":texts.len(),"minimum_cosine":minimum_cosine,
                "maximum_component_delta":maximum_delta,"batch_exact":batch_exact,"batch_errors":batch_errors,
                "cold_query_compile_us":cold.plan_compile_us,"plan_cache_peak_bytes":cold.plan_cache_peak_bytes,
                "warm_observations":observations,"scope":"Small native Windows fixtures; declared memory is not RSS and timings are not corpus-scale claims"
            }})
        );
        Ok(())
    }
    #[test]
    fn external_tensor_data_and_excessive_shapes_are_rejected_before_loading() {
        let mut tensor = pb::TensorProto {
            data_type: 1,
            dims: vec![2, 2],
            ..Default::default()
        };
        assert!(validate_tensor(&tensor).is_ok());
        tensor.data_location = Some(1);
        assert!(validate_tensor(&tensor).is_err());
        tensor.data_location = None;
        tensor.external_data.push(pb::StringStringEntryProto {
            key: "location".into(),
            value: "../../private-source".into(),
        });
        assert!(validate_tensor(&tensor).is_err());
        tensor.external_data.clear();
        tensor.dims = vec![i64::MAX, i64::MAX];
        assert!(validate_tensor(&tensor).is_err());
    }
    #[test]
    fn unsupported_control_flow_and_custom_operators_fail_closed() {
        let mut proto = pb::ModelProto {
            graph: Some(pb::GraphProto {
                output: vec![pb::ValueInfoProto::default()],
                node: vec![pb::NodeProto {
                    op_type: "Loop".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(validate_graph(&proto).is_err());
        proto.graph.as_mut().unwrap().node[0].op_type = "Identity".into();
        assert!(validate_graph(&proto).is_ok());
        proto.graph.as_mut().unwrap().node[0].domain = "custom.execution".into();
        assert!(validate_graph(&proto).is_err());
    }
    #[test]
    fn static_intermediate_tensor_budget_rejects_oversized_graph() -> Result<()> {
        let mut model = TypedModel::default();
        model.add_source("large", f32::fact([1, 134_217_729]))?;
        assert!(validate_plan(&model).is_err());
        let mut model = TypedModel::default();
        model.add_source("small", f32::fact([1, 8, 4]))?;
        assert!(validate_plan(&model).is_ok());
        Ok(())
    }
}
