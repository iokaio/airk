// SPDX-License-Identifier: Apache-2.0
//! Small fixed intent scorer. No embeddings, network, training or generated text at runtime.
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, sync::OnceLock};

pub const LABELS: [&str; 13] = [
    "search",
    "find_definition",
    "references",
    "read_file",
    "outline",
    "list_files",
    "file_info",
    "imports",
    "facts",
    "changes",
    "status",
    "diagnostics",
    "help",
];
const BUCKETS: usize = 65536;
const HEADER: usize = 96;
const ASSET: &[u8] = include_bytes!("../assets/intent-v1.bin");

pub fn features(text: &str) -> Vec<usize> {
    let lower = text.to_lowercase();
    let words: Vec<_> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
        .collect();
    let mut features = BTreeSet::new();
    let hash = |text: &str| {
        let mut hash = 14695981039346656037u64;
        for byte in text.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(1099511628211);
        }
        (hash as usize) & (BUCKETS - 1)
    };
    for word in &words {
        features.insert(hash(&format!("w:{word}")));
        let chars: Vec<_> = format!("<{word}>").chars().collect();
        for n in 3..=5 {
            for part in chars.windows(n) {
                features.insert(hash(&format!("c:{}", part.iter().collect::<String>())));
            }
        }
    }
    for pair in words.windows(2) {
        features.insert(hash(&format!("b:{} {}", pair[0], pair[1])));
    }
    for (present, flag) in [
        (text.contains('/') || text.contains('\\'), "path"),
        (text.contains(':'), "range"),
        (text.contains('"') || text.contains('\''), "quote"),
    ] {
        if present {
            features.insert(hash(flag));
        }
    }
    features.into_iter().collect()
}
fn validate() -> Result<()> {
    ensure!(
        ASSET.len() == HEADER + BUCKETS * 13 + 13 * 4 && &ASSET[..8] == b"AIRKINT1",
        "invalid intent asset length or magic"
    );
    let u32_at = |offset| u32::from_le_bytes(ASSET[offset..offset + 4].try_into().unwrap());
    ensure!(
        u32_at(8) == 1 && u32_at(12) == 65536 && u32_at(16) == 13 && u32_at(20) == 1,
        "unsupported intent feature specification"
    );
    ensure!(
        ASSET[32..64] == Sha256::digest(LABELS.join("\n").as_bytes())[..],
        "intent catalog digest mismatch"
    );
    ensure!(
        ASSET[64..96] == Sha256::digest(&ASSET[HEADER..])[..],
        "intent checksum mismatch"
    );
    Ok(())
}
pub struct Prediction {
    pub intent: &'static str,
    pub logit: i32,
    pub margin: i32,
    pub accepted: bool,
}
pub fn predict(text: &str) -> Result<Prediction> {
    static VALID: OnceLock<bool> = OnceLock::new();
    ensure!(
        *VALID.get_or_init(|| validate().is_ok()),
        "intent asset validation failed"
    );
    ensure!(text.len() <= 4096, "intent input too large");
    let indices = features(text);
    let mut scores = [0i32; 13];
    for (label, score) in scores.iter_mut().enumerate() {
        let at = HEADER + BUCKETS * 13 + label * 4;
        *score = i32::from_le_bytes(ASSET[at..at + 4].try_into().unwrap());
        for index in &indices {
            *score += i32::from(ASSET[HEADER + index * 13 + label] as i8);
        }
    }
    let mut order: Vec<_> = (0..13).collect();
    order.sort_by_key(|i| (std::cmp::Reverse(scores[*i]), *i));
    let top = order[0];
    let margin = scores[top] - scores[order[1]];
    let threshold = i32::from_le_bytes(ASSET[24..28].try_into().unwrap());
    let min_margin = i32::from_le_bytes(ASSET[28..32].try_into().unwrap());
    Ok(Prediction {
        intent: LABELS[top],
        logit: scores[top],
        margin,
        accepted: scores[top] >= threshold && margin >= min_margin,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn asset_and_shared_features() -> Result<()> {
        validate()?;
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../tools/parser-train/fixtures.json"))?;
        for row in fixture.as_array().unwrap() {
            let actual = features(row["text"].as_str().unwrap());
            assert_eq!(serde_json::to_value(actual)?, row["features"]);
        }
        Ok(())
    }
    #[test]
    fn inference_is_fixed() -> Result<()> {
        let a = predict("find references to QueueClient")?;
        let b = predict("find references to QueueClient")?;
        assert_eq!((a.intent, a.logit, a.margin), (b.intent, b.logit, b.margin));
        Ok(())
    }

    #[test]
    fn reviewed_model_matches_python_logits_and_retained_accuracy_floor() -> Result<()> {
        let goldens: serde_json::Value =
            serde_json::from_str(include_str!("../../../tools/parser-train/inference.json"))?;
        for row in goldens.as_array().unwrap() {
            let prediction = predict(row["text"].as_str().unwrap())?;
            assert_eq!(prediction.intent, row["intent"]);
            assert_eq!(prediction.logit, row["logit"].as_i64().unwrap() as i32);
            assert_eq!(prediction.margin, row["margin"].as_i64().unwrap() as i32);
            assert_eq!(prediction.accepted, row["accepted"].as_bool().unwrap());
        }
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../tools/parser-train/evaluation.json"))?;
        let mut correct = 0;
        for row in fixture.as_array().unwrap() {
            let prediction = predict(row["text"].as_str().unwrap())?;
            if prediction.intent == row["intent"] {
                correct += 1;
            } else {
                assert!(
                    !prediction.accepted,
                    "wrong accepted supported regression request: {}",
                    row["text"]
                );
            }
        }
        assert_eq!(fixture.as_array().unwrap().len(), 150);
        assert!(
            correct >= 138,
            "retained 150-utterance intent floor: {correct}/150"
        );
        Ok(())
    }
}
