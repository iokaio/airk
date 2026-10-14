// SPDX-License-Identifier: Apache-2.0
//! Explicit offline tokenizer pipelines. Neither model nor framing is guessed.
use crate::usage::{LocalByteBpe, read_usage_input};
use airk_core::usage::{TokenizerAlgorithm, TokenizerIdentity, validate_hex};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

const MAX_INTERMEDIATE_BYTES: usize = 4 * 1024 * 1024;
const MAX_PIPELINE_STAGES: usize = 32;

/// Conservative normalization/pretokenization byte growth and postprocessor
/// insertion work, including prefixes inserted per split. Every admitted
/// transform is monotonic in this estimate. This is not an RSS/time limit.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Expansion {
    multiplier: usize,
    extra: usize,
}

impl Default for Expansion {
    fn default() -> Self {
        Self {
            multiplier: 1,
            extra: 0,
        }
    }
}

impl Expansion {
    fn then(&mut self, multiplier: usize, extra: usize) -> Result<()> {
        self.multiplier = self
            .multiplier
            .checked_mul(multiplier)
            .context("tokenizer expansion overflow")?;
        self.extra = self
            .extra
            .checked_mul(multiplier)
            .and_then(|value| value.checked_add(extra))
            .context("tokenizer expansion overflow")?;
        self.check(1)?;
        Ok(())
    }

    fn check(self, bytes: usize) -> Result<usize> {
        let estimate = bytes
            .checked_mul(self.multiplier)
            .and_then(|value| value.checked_add(self.extra))
            .context("tokenizer expansion overflow")?;
        ensure!(
            estimate <= MAX_INTERMEDIATE_BYTES,
            "tokenizer conservative intermediate-work estimate exceeds 4 MiB"
        );
        Ok(estimate)
    }
}

#[derive(Clone, Copy)]
enum Stage {
    Normalizer,
    PreTokenizer,
    PostProcessor,
}

fn bounded_string(value: &Value, maximum: usize) -> Result<&str> {
    let value = value.as_str().context("tokenizer string missing")?;
    ensure!(value.len() <= maximum, "tokenizer string exceeds limit");
    Ok(value)
}

fn template_budget(value: &Value) -> Result<usize> {
    let specials = value["special_tokens"]
        .as_object()
        .context("tokenizer template special tokens missing")?;
    ensure!(specials.len() <= 64, "too many template special tokens");
    for (name, token) in specials {
        ensure!(name.len() <= 128, "template token name exceeds limit");
        bounded_string(&token["id"], 128)?;
        let ids = token["ids"]
            .as_array()
            .context("special token IDs missing")?;
        let tokens = token["tokens"]
            .as_array()
            .context("special token strings missing")?;
        ensure!(
            !ids.is_empty() && ids.len() <= 16 && ids.len() == tokens.len(),
            "template special-token expansion exceeds limit"
        );
        for text in tokens {
            bounded_string(text, 128)?;
        }
    }
    let mut largest = 0;
    for (name, pair) in [("single", false), ("pair", true)] {
        let pieces = value[name]
            .as_array()
            .context("tokenizer templates must be explicit arrays")?;
        ensure!(pieces.len() <= 16, "tokenizer template exceeds 16 pieces");
        let (mut a, mut b, mut added) = (0, 0, 0);
        for piece in pieces {
            ensure!(
                piece.as_object().is_some_and(|object| object.len() == 1),
                "ambiguous tokenizer template piece"
            );
            if let Some(sequence) = piece.get("Sequence") {
                match sequence["id"].as_str() {
                    Some("A") => a += 1,
                    Some("B") => b += 1,
                    _ => anyhow::bail!("unknown template sequence"),
                }
            } else if let Some(token) = piece.get("SpecialToken") {
                let id = bounded_string(&token["id"], 128)?;
                added += specials
                    .get(id)
                    .and_then(|entry| entry["tokens"].as_array())
                    .context("undefined template special token")?
                    .iter()
                    .map(|token| token.as_str().map_or(0, |text| text.len().max(1)))
                    .sum::<usize>();
            } else {
                anyhow::bail!("unsupported tokenizer template piece");
            }
        }
        ensure!(
            a == 1 && b == usize::from(pair),
            "tokenizer template must preserve each input sequence exactly once"
        );
        largest = largest.max(added);
    }
    Ok(largest)
}

fn stage_budget(
    value: &Value,
    stage: Stage,
    depth: usize,
    stages: &mut usize,
    growth: &mut Expansion,
) -> Result<()> {
    if value.is_null() {
        return Ok(());
    }
    *stages += 1;
    ensure!(
        depth <= 4 && *stages <= MAX_PIPELINE_STAGES,
        "tokenizer pipeline exceeds depth/stage limit"
    );
    let kind = value["type"]
        .as_str()
        .context("tokenizer stage type missing")?;
    if kind == "Sequence" {
        let key = match stage {
            Stage::Normalizer => "normalizers",
            Stage::PreTokenizer => "pretokenizers",
            Stage::PostProcessor => "processors",
        };
        let children = value[key]
            .as_array()
            .context("tokenizer sequence missing")?;
        ensure!(children.len() <= 16, "tokenizer sequence exceeds 16 stages");
        for child in children {
            ensure!(!child.is_null(), "null tokenizer sequence stage");
            stage_budget(child, stage, depth + 1, stages, growth)?;
        }
        return Ok(());
    }
    match (stage, kind) {
        // Unicode decomposition can produce up to 18 scalars per input scalar;
        // 72 allows four UTF-8 bytes for each. Lowercase can produce three scalars.
        (Stage::Normalizer, "NFC" | "NFD" | "NFKC" | "NFKD") => growth.then(72, 0)?,
        (Stage::Normalizer, "Lowercase") => growth.then(12, 0)?,
        (Stage::Normalizer, "BertNormalizer" | "Bert") => {
            // Conservative byte-ratio bounds for canonical Unicode transforms:
            // canonical decomposition <=4x, lowercase <=3x, spacing <=3x.
            growth.then(36, 0)?;
        }
        (Stage::Normalizer, "Strip" | "StripAccents" | "Nmt") => {}
        (Stage::Normalizer, "ByteLevel") => growth.then(2, 0)?,
        (Stage::Normalizer, "Replace") => {
            let pattern = value["pattern"]
                .as_object()
                .context("replace pattern missing")?;
            ensure!(pattern.len() == 1, "ambiguous replace pattern");
            let text = bounded_string(&value["pattern"]["String"], 128)?;
            let replacement = bounded_string(&value["content"], 128)?;
            ensure!(
                !text.is_empty() && replacement.len() <= text.len(),
                "expanding or empty-pattern tokenizer replacement is unsupported"
            );
        }
        (Stage::Normalizer, "Prepend") => {
            let prefix = bounded_string(&value["prepend"], 32)?;
            // Added tokens can split the input before normalization, so allow a
            // prefix for every input byte plus the empty-input fragment.
            growth.then(1 + prefix.len(), prefix.len())?;
        }
        (
            Stage::PreTokenizer,
            "BertPreTokenizer" | "Whitespace" | "WhitespaceSplit" | "Punctuation" | "Digits"
            | "UnicodeScripts" | "CharDelimiterSplit" | "Delimiter",
        ) => {}
        (Stage::PreTokenizer, "FixedLength") => ensure!(
            value.get("length").is_none() || value["length"].as_u64().is_some_and(|v| v > 0),
            "fixed-length tokenizer requires a positive length"
        ),
        (Stage::PreTokenizer, "Split") => {
            let pattern = value["pattern"]
                .as_object()
                .context("split pattern missing")?;
            ensure!(pattern.len() == 1, "ambiguous split pattern");
            bounded_string(
                pattern.values().next().context("split pattern missing")?,
                1024,
            )?;
        }
        (Stage::PreTokenizer, "ByteLevel") => growth.then(4, 2)?,
        (Stage::PreTokenizer, "Metaspace") => growth.then(8, 4)?,
        (Stage::PostProcessor, "ByteLevel") => {}
        (Stage::PostProcessor, "BertProcessing" | "RobertaProcessing") => {
            let sep = bounded_string(&value["sep"][0], 128)?;
            let cls = bounded_string(&value["cls"][0], 128)?;
            growth.then(1, 4 * sep.len().max(cls.len()).max(1))?;
        }
        (Stage::PostProcessor, "TemplateProcessing") => growth.then(1, template_budget(value)?)?,
        _ => anyhow::bail!("unsupported bounded tokenizer stage: {kind}"),
    }
    Ok(())
}

/// Validate before constructing the third-party tokenizer. Input-file limits alone
/// cannot bound recursive replacement or whole-encoding template duplication.
fn pipeline_budget(config: &Value) -> Result<Expansion> {
    let mut growth = Expansion::default();
    let mut stages = 0;
    stage_budget(
        &config["normalizer"],
        Stage::Normalizer,
        0,
        &mut stages,
        &mut growth,
    )?;
    if let Some(added) = config["added_tokens"].as_array() {
        ensure!(added.len() <= 4096, "too many added tokenizer tokens");
        let mut bytes = 0;
        for token in added {
            bytes += bounded_string(&token["content"], 128)?.len();
        }
        // Added-token normalization happens when loading the tokenizer, before
        // encode. Include per-token prefix overhead in this separate admission.
        growth.check(bytes.saturating_add(added.len()))?;
    }
    for key in [
        "unk_token",
        "continuing_subword_prefix",
        "end_of_word_suffix",
    ] {
        if !config["model"][key].is_null() {
            bounded_string(&config["model"][key], 32)?;
        }
    }
    if let Some(vocabulary) = config["model"]["vocab"].as_object() {
        for token in vocabulary.keys() {
            ensure!(
                token.len() <= 256,
                "tokenizer vocabulary string exceeds 256 bytes"
            );
        }
    } else if let Some(vocabulary) = config["model"]["vocab"].as_array() {
        for token in vocabulary {
            bounded_string(&token[0], 256)?;
        }
        if let Some(id) = config["model"]["unk_id"].as_u64() {
            let unknown = usize::try_from(id)
                .ok()
                .and_then(|id| vocabulary.get(id))
                .context("Unigram unknown token is outside vocabulary")?;
            bounded_string(&unknown[0], 32)?;
        }
    }
    stage_budget(
        &config["pre_tokenizer"],
        Stage::PreTokenizer,
        0,
        &mut stages,
        &mut growth,
    )?;
    stage_budget(
        &config["post_processor"],
        Stage::PostProcessor,
        0,
        &mut stages,
        &mut growth,
    )?;
    Ok(growth)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    schema: u32,
    model_revision_sha256: String,
    tokenizer_file: String,
    tokenizer_sha256: String,
    add_special_tokens: bool,
    framing: Framing,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(directory: &Path, model: Value) -> Result<std::path::PathBuf> {
        let pipeline = json!({"version":"1.0","truncation":{"direction":"Right","max_length":1,"strategy":"LongestFirst","stride":0},
            "padding":{"strategy":{"Fixed":20},"direction":"Right","pad_to_multiple_of":null,"pad_id":0,"pad_type_id":0,"pad_token":"<unk>"},
            "added_tokens":[],"normalizer":{"type":"Lowercase"},"pre_tokenizer":{"type":"Whitespace"},
            "post_processor":null,"decoder":null,"model":model});
        let bytes = serde_json::to_vec(&pipeline)?;
        std::fs::write(directory.join("tokenizer.json"), &bytes)?;
        let path = directory.join("calibration.json");
        std::fs::write(
            &path,
            serde_json::to_vec(&json!({"schema":2,"model_revision_sha256":"a".repeat(64),
            "tokenizer_file":"tokenizer.json","tokenizer_sha256":format!("{:x}",Sha256::digest(&bytes)),
            "add_special_tokens":false,"framing":{"prefix":"hi ","suffix":" hi"}}))?,
        )?;
        Ok(path)
    }

    fn change_pipeline(path: &Path, change: impl FnOnce(&mut Value)) -> Result<()> {
        let model = path
            .parent()
            .context("fixture parent")?
            .join("tokenizer.json");
        let mut config: Value = serde_json::from_slice(&std::fs::read(&model)?)?;
        change(&mut config);
        let bytes = serde_json::to_vec(&config)?;
        std::fs::write(model, &bytes)?;
        let mut manifest: Value = serde_json::from_slice(&std::fs::read(path)?)?;
        manifest["tokenizer_sha256"] = json!(format!("{:x}", Sha256::digest(bytes)));
        std::fs::write(path, serde_json::to_vec(&manifest)?)?;
        Ok(())
    }

    #[test]
    fn expanding_normalizers_and_recursive_pipelines_fail_before_loading() -> Result<()> {
        let malicious = [
            json!({"type":"Replace","pattern":{"String":"a"},"content":"aa"}),
            json!({"type":"Replace","pattern":{"Regex":".*"},"content":" "}),
            json!({"type":"Precompiled","precompiled_charsmap":"AAAA"}),
            json!({"type":"Sequence","normalizers":vec![json!({"type":"Lowercase"}); 17]}),
        ];
        let temp = tempfile::tempdir()?;
        let path = fixture(
            temp.path(),
            json!({"type":"WordLevel","vocab":{"<unk>":0,"hi":1},"unk_token":"<unk>"}),
        )?;
        for normalizer in malicious {
            change_pipeline(&path, |config| config["normalizer"] = normalizer)?;
            assert!(LocalTokenizer::open(&path).is_err());
        }
        let mut nested = json!({"type":"Lowercase"});
        for _ in 0..6 {
            nested = json!({"type":"Sequence","normalizers":[nested]});
        }
        change_pipeline(&path, |config| config["normalizer"] = nested)?;
        assert!(LocalTokenizer::open(&path).is_err());
        change_pipeline(&path, |config| {
            config["normalizer"] = Value::Null;
            config["pre_tokenizer"] = json!({"type":"FixedLength","length":0});
        })?;
        assert!(LocalTokenizer::open(&path).is_err());
        Ok(())
    }

    #[test]
    fn template_duplication_and_oversized_special_token_expansion_are_rejected() -> Result<()> {
        let template = json!({"type":"TemplateProcessing",
            "single":[{"SpecialToken":{"id":"[CLS]","type_id":0}},{"Sequence":{"id":"A","type_id":0}},{"SpecialToken":{"id":"[SEP]","type_id":0}}],
            "pair":[{"Sequence":{"id":"A","type_id":0}},{"Sequence":{"id":"B","type_id":1}}],
            "special_tokens":{"[CLS]":{"id":"[CLS]","ids":[2],"tokens":["[CLS]"]},"[SEP]":{"id":"[SEP]","ids":[3],"tokens":["[SEP]"]}}});
        let temp = tempfile::tempdir()?;
        let path = fixture(
            temp.path(),
            json!({"type":"WordPiece","vocab":{"<unk>":0,"hi":1,"[CLS]":2,"[SEP]":3},"unk_token":"<unk>","continuing_subword_prefix":"##","max_input_chars_per_word":100}),
        )?;
        change_pipeline(&path, |config| {
            config["normalizer"] = json!({"type":"BertNormalizer","clean_text":true,"handle_chinese_chars":true,"strip_accents":null,"lowercase":true});
            config["pre_tokenizer"] = json!({"type":"BertPreTokenizer"});
            config["post_processor"] = template.clone();
        })?;
        let tokenizer = LocalTokenizer::open(&path)?;
        assert_eq!(tokenizer.count("HI hi")?, 4); // explicit framing; specials disabled
        assert_eq!(tokenizer.count(&"hi ".repeat(12_000))?, 12_002);
        change_pipeline(&path, |config| {
            config["post_processor"]["single"] =
                json!([{"Sequence":{"id":"A","type_id":0}},{"Sequence":{"id":"A","type_id":0}}]);
        })?;
        assert!(LocalTokenizer::open(&path).is_err());
        change_pipeline(&path, |config| {
            config["post_processor"] = template;
            config["post_processor"]["special_tokens"]["[CLS]"]["ids"] = json!(vec![2; 17]);
            config["post_processor"]["special_tokens"]["[CLS]"]["tokens"] =
                json!(vec!["[CLS]"; 17]);
        })?;
        assert!(LocalTokenizer::open(&path).is_err());
        Ok(())
    }

    #[test]
    fn composed_work_bound_rejects_before_encode_and_unused_stages_are_discarded() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = fixture(
            temp.path(),
            json!({"type":"WordLevel","vocab":{"<unk>":0,"hi":1},"unk_token":"<unk>"}),
        )?;
        change_pipeline(&path, |config| {
            config["normalizer"] =
                json!({"type":"Sequence","normalizers":vec![json!({"type":"ByteLevel"}); 5]});
            config["decoder"] = json!({"type":"an unused unsupported decoder"});
            config["padding"]["strategy"]["Fixed"] = json!(usize::MAX);
        })?;
        let tokenizer = LocalTokenizer::open(&path)?;
        let error = tokenizer
            .count(&"a".repeat(262_144))
            .expect_err("bounded input can still exceed intermediate budget");
        assert!(error.to_string().contains("intermediate-work"));
        assert!(tokenizer.count("hi").is_ok());
        let mut overflow = Expansion {
            multiplier: usize::MAX,
            extra: 0,
        };
        assert!(overflow.then(2, 0).is_err());
        Ok(())
    }

    #[test]
    fn all_pipeline_algorithms_preserve_full_input_and_explicit_framing() -> Result<()> {
        let models = [
            json!({"type":"WordLevel","vocab":{"<unk>":0,"hi":1},"unk_token":"<unk>"}),
            json!({"type":"WordPiece","vocab":{"<unk>":0,"hi":1},"unk_token":"<unk>","continuing_subword_prefix":"##","max_input_chars_per_word":100}),
            json!({"type":"Unigram","vocab":[["<unk>",0.0],["hi",-1.0],["h",-2.0],["i",-2.0]],"unk_id":0,"byte_fallback":false}),
            json!({"type":"BPE","dropout":null,"unk_token":"<unk>","continuing_subword_prefix":null,"end_of_word_suffix":null,
                "fuse_unk":false,"byte_fallback":false,"ignore_merges":false,"vocab":{"<unk>":0,"h":1,"i":2,"hi":3},"merges":[["h","i"]]}),
        ];
        for model in models {
            let temp = tempfile::tempdir()?;
            let path = fixture(temp.path(), model)?;
            let tokenizer = LocalTokenizer::open(&path)?;
            assert_eq!(tokenizer.count("HI hi hi")?, 5);
            assert_eq!(tokenizer.count("HI")?, 3);
            assert_eq!(tokenizer.count(&"HI ".repeat(1024))?, 1026);
            let original_identity = tokenizer.identity().definition_sha256.clone();
            let mut manifest: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
            manifest["framing"]["suffix"] = json!("");
            std::fs::write(&path, serde_json::to_vec(&manifest)?)?;
            let changed = LocalTokenizer::open(&path)?;
            assert_ne!(original_identity, changed.identity().definition_sha256);
            assert_eq!(changed.count("hi")?, 2);
            std::fs::write(temp.path().join("tokenizer.json"), b"{}")?;
            assert!(LocalTokenizer::open(&path).is_err());
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Framing {
    prefix: String,
    suffix: String,
}

pub(crate) enum LocalTokenizer {
    Byte(LocalByteBpe),
    Pipeline {
        tokenizer: Box<tokenizers::Tokenizer>,
        identity: TokenizerIdentity,
        add_special_tokens: bool,
        framing: Framing,
        expansion: Expansion,
    },
}

impl LocalTokenizer {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let bytes = read_usage_input(path, 1_048_576)?;
        let value: Value = serde_json::from_slice(&bytes)?;
        if value["schema"] == 1 {
            return Ok(Self::Byte(LocalByteBpe::parse(&bytes)?));
        }
        let definition: Definition = serde_json::from_value(value)?;
        ensure!(
            definition.schema == 2,
            "unsupported tokenizer manifest schema"
        );
        validate_hex(&definition.model_revision_sha256, 32)?;
        validate_hex(&definition.tokenizer_sha256, 32)?;
        ensure!(
            !definition.tokenizer_file.is_empty()
                && definition.tokenizer_file.len() <= 128
                && definition
                    .tokenizer_file
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                && !matches!(definition.tokenizer_file.as_str(), "." | ".."),
            "tokenizer_file must be a sibling filename"
        );
        ensure!(
            definition.framing.prefix.len() + definition.framing.suffix.len() <= 16_384,
            "tokenizer framing exceeds 16 KiB"
        );
        let model = read_usage_input(
            &path
                .parent()
                .unwrap_or(Path::new("."))
                .join(&definition.tokenizer_file),
            33_554_432,
        )?;
        ensure!(
            format!("{:x}", Sha256::digest(&model))
                .eq_ignore_ascii_case(&definition.tokenizer_sha256),
            "tokenizer digest mismatch"
        );
        let mut config: Value = serde_json::from_slice(&model)?;
        let algorithm = match config["model"]["type"].as_str() {
            Some("BPE") => TokenizerAlgorithm::HuggingFaceBpe,
            Some("WordPiece") => TokenizerAlgorithm::HuggingFaceWordPiece,
            Some("Unigram") => TokenizerAlgorithm::HuggingFaceUnigram,
            Some("WordLevel") => TokenizerAlgorithm::HuggingFaceWordLevel,
            _ => anyhow::bail!("unsupported local tokenizer algorithm"),
        };
        // Disable stochastic segmentation: otherwise identical inputs need not
        // produce identical counts or comparable baseline/returned samples.
        ensure!(
            config["model"]["dropout"].is_null() || config["model"]["dropout"] == 0.0,
            "stochastic BPE dropout is unsupported for calibration"
        );
        let expansion = pipeline_budget(&config)?;
        // These stages do not participate in counting. Remove them before loading
        // so unused decoder recursion or padding allocations cannot bypass checks.
        config["decoder"] = Value::Null;
        config["padding"] = Value::Null;
        config["truncation"] = Value::Null;
        let mut tokenizer = tokenizers::Tokenizer::from_bytes(serde_json::to_vec(&config)?)
            .map_err(|e| anyhow::anyhow!("invalid tokenizer pipeline: {e}"))?;
        tokenizer
            .with_truncation(None)
            .map_err(|e| anyhow::anyhow!("invalid truncation: {e}"))?;
        tokenizer.with_padding(None);
        let mut hash = Sha256::new();
        hash.update(b"airk-local-calibration/2\0tokenizers/0.23.2\0");
        hash.update(&bytes);
        let identity = TokenizerIdentity {
            algorithm,
            definition_sha256: format!("{:x}", hash.finalize()),
            model_revision_sha256: definition.model_revision_sha256.to_ascii_lowercase(),
        };
        Ok(Self::Pipeline {
            tokenizer: Box::new(tokenizer),
            identity,
            add_special_tokens: definition.add_special_tokens,
            framing: definition.framing,
            expansion,
        })
    }

    pub(crate) fn identity(&self) -> &TokenizerIdentity {
        match self {
            Self::Byte(value) => &value.identity,
            Self::Pipeline { identity, .. } => identity,
        }
    }

    pub(crate) fn count(&self, text: &str) -> Result<u64> {
        ensure!(text.len() <= 262_144, "tokenizer text exceeds bound");
        match self {
            Self::Byte(value) => value.count(text),
            Self::Pipeline {
                tokenizer,
                add_special_tokens,
                framing,
                expansion,
                ..
            } => {
                expansion.check(text.len() + framing.prefix.len() + framing.suffix.len())?;
                let framed = format!("{}{}{}", framing.prefix, text, framing.suffix);
                let encoding = tokenizer
                    .encode(framed, *add_special_tokens)
                    .map_err(|e| anyhow::anyhow!("local tokenization failed: {e}"))?;
                ensure!(
                    encoding.get_overflowing().is_empty(),
                    "tokenizer unexpectedly truncated input"
                );
                Ok(encoding.len() as u64)
            }
        }
    }

    pub(crate) fn description(&self) -> Value {
        match self {
            Self::Byte(_) => {
                json!({"runtime":"utf8-byte-bpe/1", "framing":"none", "special_tokens":false})
            }
            Self::Pipeline {
                add_special_tokens,
                framing,
                ..
            } => json!({"runtime":"tokenizers/0.23.2",
                "framing":"explicit_prefix_suffix", "framing_utf8_bytes":framing.prefix.len()+framing.suffix.len(),
                "special_tokens":add_special_tokens,"truncation":false,"padding":false,
                "max_intermediate_work_bytes":MAX_INTERMEDIATE_BYTES,"pipeline_stage_limit":MAX_PIPELINE_STAGES}),
        }
    }
}
