// SPDX-License-Identifier: Apache-2.0
//! Bounded single-document CPU ONNX execution and masked mean pooling.
use crate::model::{self, MAX_INPUT_BYTES, MAX_MODEL_BYTES, MAX_TOKENIZER_BYTES, OnnxManifest};
use anyhow::{Context, Result, ensure};
use std::{io::Cursor, path::Path, sync::Arc};
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};
use tract_onnx::{pb, prelude::*};

pub struct OnnxModel {
    pub(crate) manifest: OnnxManifest,
    pub(crate) identity: String,
    tokenizer: Tokenizer,
    plan: Arc<TypedRunnableModel>,
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

fn validate_graph(proto: &pb::ModelProto) -> Result<()> {
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
    let mut check_tensor = |tensor: &pb::TensorProto| -> Result<()> {
        validate_tensor(tensor)?;
        let elements = tensor.dims.iter().try_fold(1u64, |size, dim| {
            size.checked_mul(u64::try_from(*dim)?)
                .context("ONNX constant size overflow")
        })?;
        tensor_elements = tensor_elements
            .checked_add(elements)
            .context("ONNX constant budget overflow")?;
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
    Ok(())
}

fn validate_plan(model: &TypedModel) -> Result<()> {
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
    Ok(())
}

impl OnnxModel {
    pub(crate) fn open(directory: &Path, manifest_bytes: &[u8]) -> Result<Self> {
        let (manifest, identity) = model::onnx_manifest(manifest_bytes)?;
        model::verified_asset(directory, "LICENSE", 65_536, &manifest.license_sha256)?;
        let tokenizer_bytes = model::verified_asset(
            directory,
            "tokenizer.json",
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
            .with_truncation(Some(TruncationParams {
                max_length: manifest.max_tokens,
                stride: 0,
                ..Default::default()
            }))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let pad_id = tokenizer
            .token_to_id("[PAD]")
            .context("BERT padding token missing")?;
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::Fixed(manifest.max_tokens),
            pad_id,
            pad_token: "[PAD]".into(),
            ..Default::default()
        }));
        let bytes = model::verified_asset(
            directory,
            "model.onnx",
            MAX_MODEL_BYTES,
            &manifest.model_sha256,
        )?;
        let mut framework = tract_onnx::onnx();
        framework.provider = Arc::new(NoExternalData);
        let proto = framework.proto_model_for_read(&mut Cursor::new(bytes))?;
        validate_graph(&proto)?;
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
        let mut inference = framework.model_for_proto_model(&proto)?;
        drop(proto);
        for input in 0..inputs.len() {
            inference.set_input_fact(input, i64::fact([1, manifest.max_tokens]).into())?;
        }
        let typed = inference
            .into_typed()
            .context("infer bounded ONNX shapes")?;
        validate_plan(&typed)?;
        let output = typed.output_fact(0)?;
        ensure!(
            output.datum_type == f32::datum_type()
                && output.shape.as_concrete()
                    == Some(&[1, manifest.max_tokens, manifest.dimensions][..]),
            "ONNX output must be float32 [1,tokens,dimensions]"
        );
        let optimized = typed.into_optimized()?;
        validate_plan(&optimized)?;
        let plan = optimized.into_runnable()?;
        Ok(Self {
            manifest,
            identity,
            tokenizer,
            plan,
            inputs,
        })
    }

    pub(crate) fn embed_text(&self, text: &str) -> Result<Option<Vec<f32>>> {
        let mut end = text.len().min(MAX_INPUT_BYTES);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let text = &text[..end];
        if text.trim().is_empty() {
            return Ok(None);
        }
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let length = self.manifest.max_tokens;
        ensure!(
            encoding.len() == length,
            "tokenizer did not respect fixed context bound"
        );
        let ids: Vec<i64> = encoding.get_ids().iter().map(|id| i64::from(*id)).collect();
        let mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|id| i64::from(*id))
            .collect();
        let types: Vec<i64> = encoding
            .get_type_ids()
            .iter()
            .map(|id| i64::from(*id))
            .collect();
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
                Ok(Tensor::from_shape(&[1, length], values)?.into())
            })
            .collect::<Result<_>>()?;
        let outputs = self
            .plan
            .run(inputs)
            .context("local ONNX inference failed")?;
        ensure!(
            outputs.len() == 1 && outputs[0].shape() == [1, length, self.manifest.dimensions],
            "ONNX output shape changed"
        );
        let view = outputs[0].to_plain_array_view::<f32>()?;
        let values = view.as_slice().context("ONNX output is not contiguous")?;
        let mut sum = vec![0.0f64; self.manifest.dimensions];
        for (row, mask) in values.chunks_exact(self.manifest.dimensions).zip(mask) {
            if mask == 0 {
                continue;
            }
            for (total, value) in sum.iter_mut().zip(row) {
                ensure!(value.is_finite(), "ONNX output contains nonfinite values");
                *total += f64::from(*value);
            }
        }
        let norm = sum.iter().map(|v| v * v).sum::<f64>().sqrt();
        ensure!(
            norm.is_finite() && norm > 0.0,
            "ONNX model produced an empty embedding"
        );
        Ok(Some(sum.into_iter().map(|v| (v / norm) as f32).collect()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
