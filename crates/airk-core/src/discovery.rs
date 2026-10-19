// SPDX-License-Identifier: Apache-2.0
//! Revision-bound discovery continuation and compact outline contracts.
use crate::{FactHit, continuity::ContinuityKey, identity::framed_digest};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DiscoveryOptions {
    pub cursor: Option<String>,
    #[serde(default)]
    pub compact: bool,
}

/// The budget counts each complete serialized record; envelope/array framing is additional.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DiscoveryPage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Minimum budget for the next record when no record could be returned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_record_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_bytes: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompactFact {
    pub name: String,
    pub kind: String,
    pub syntax_kind: String,
    pub citation: crate::Citation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuity_key: Option<ContinuityKey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<crate::CodeSpan>,
}

impl From<&FactHit> for CompactFact {
    fn from(hit: &FactHit) -> Self {
        Self {
            name: hit.name.clone(),
            kind: hit.kind.clone(),
            syntax_kind: hit.syntax_kind.clone(),
            citation: hit.citation.clone(),
            continuity_key: hit.continuity_key,
            span: hit.code.as_ref().and_then(|code| code.span.clone()),
        }
    }
}

/// The ordinal is only a final tie breaker within one immutable publication.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryKey {
    pub path: String,
    pub line: usize,
    pub kind: String,
    pub name: String,
    pub ordinal: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    binding: String,
    after: DiscoveryKey,
    checksum: String,
}

/// Cursors are consistency selectors, not authorization tokens. All returned records
/// still pass the collection's normal authorization and publication verification.
pub fn binding(revision: &str, tool: &str, filters: impl Serialize) -> Result<String> {
    Ok(framed_digest(
        b"discovery-binding/1",
        &[
            revision.as_bytes(),
            tool.as_bytes(),
            &serde_json::to_vec(&filters)?,
        ],
    )
    .to_string())
}

pub fn decode(cursor: Option<&str>, binding: &str) -> Result<Option<DiscoveryKey>> {
    let Some(encoded) = cursor else {
        return Ok(None);
    };
    ensure!(encoded.len() <= 32_768, "discovery cursor exceeds 32 KiB");
    let cursor: Cursor = serde_json::from_str(
        encoded
            .strip_prefix("airk-d1:")
            .context("invalid discovery cursor version")?,
    )
    .context("malformed discovery cursor")?;
    ensure!(
        cursor.binding == binding,
        "discovery cursor expired or filters/representation changed; restart discovery"
    );
    ensure!(
        cursor.checksum == checksum(binding, &cursor.after)?,
        "discovery cursor checksum mismatch"
    );
    ensure!(
        cursor.after.path.len() <= 4096
            && cursor.after.name.is_empty()
            && cursor.after.kind.is_empty()
            && cursor.after.line == 0
            && cursor.after.ordinal >= 0,
        "invalid discovery cursor key"
    );
    Ok(Some(cursor.after))
}

pub fn encode(binding: &str, after: &DiscoveryKey) -> Result<String> {
    // Recover ordering columns from the immutable row on continuation. Long
    // source names never inflate the cursor or prevent a client from resuming.
    let after = DiscoveryKey {
        path: after.path.clone(),
        ordinal: after.ordinal,
        ..Default::default()
    };
    ensure!(
        after.path.len() <= 4096,
        "discovery cursor path exceeds bound"
    );
    Ok(format!(
        "airk-d1:{}",
        serde_json::to_string(&Cursor {
            binding: binding.into(),
            after: after.clone(),
            checksum: checksum(binding, &after)?
        })?
    ))
}

fn checksum(binding: &str, after: &DiscoveryKey) -> Result<String> {
    Ok(framed_digest(
        b"discovery-cursor/1",
        &[binding.as_bytes(), &serde_json::to_vec(after)?],
    )
    .to_string())
}

pub fn append_fact(
    response: &mut crate::FactsResponse,
    hit: FactHit,
    compact: bool,
    limit: usize,
    max_bytes: usize,
) -> Result<bool> {
    let compact_hit = compact.then(|| CompactFact::from(&hit));
    let bytes = match &compact_hit {
        Some(hit) => serde_json::to_vec(hit)?.len(),
        None => serde_json::to_vec(&hit)?.len(),
    };
    let count = response.facts.len() + response.outline.len();
    let used = response.page.record_bytes.get_or_insert(0);
    if count >= limit || bytes > max_bytes.saturating_sub(*used) {
        response.truncated = true;
        if count == 0 {
            response.page.required_record_bytes = Some(bytes);
        }
        return Ok(false);
    }
    *used += bytes;
    if let Some(hit) = compact_hit {
        response.outline.push(hit);
    } else {
        response.facts.push(hit);
    }
    Ok(true)
}

pub fn paginate_facts(
    response: &mut crate::FactsResponse,
    facts: Vec<FactHit>,
    options: &DiscoveryOptions,
    binding: &str,
    limit: usize,
    max_bytes: usize,
) -> Result<()> {
    let after = decode(options.cursor.as_deref(), binding)?;
    response.page.record_bytes = Some(0);
    let mut records = facts
        .into_iter()
        .enumerate()
        .map(|(ordinal, hit)| {
            let key = DiscoveryKey {
                path: hit.citation.path.clone(),
                line: hit.citation.start_line,
                kind: hit.kind.clone(),
                name: hit.name.clone(),
                ordinal: ordinal as i64,
            };
            (key, hit)
        })
        .collect::<Vec<_>>();
    records.sort_by(|a, b| a.0.cmp(&b.0));
    let after = after
        .map(|after| {
            records
                .iter()
                .find(|(key, _)| key.path == after.path && key.ordinal == after.ordinal)
                .map(|(key, _)| key.clone())
                .context("discovery cursor position is unavailable")
        })
        .transpose()?;
    let mut last = after.clone();
    for (key, hit) in records {
        if after.as_ref().is_some_and(|after| key <= *after) {
            continue;
        }
        if !append_fact(response, hit, options.compact, limit, max_bytes)? {
            break;
        }
        last = Some(key);
    }
    if response.truncated {
        response.page.cursor = last.as_ref().map(|key| encode(binding, key)).transpose()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursors_bound_argument_size_without_copying_large_source_names() -> Result<()> {
        let key = DiscoveryKey {
            path: "\u{1}".repeat(4096),
            name: "large written name".repeat(4000),
            kind: "definition".into(),
            line: 17,
            ordinal: 123,
        };
        let binding = binding("fictional revision", "outline", true)?;
        let encoded = encode(&binding, &key)?;
        assert!(encoded.len() < 32_768);
        // Nested JSON escaping also fits the CLI's 64 KiB argument file cap.
        assert!(serde_json::to_vec(&encoded)?.len() < 65_536);
        let decoded = decode(Some(&encoded), &binding)?.context("cursor")?;
        assert_eq!(decoded.path, key.path);
        assert_eq!(decoded.ordinal, key.ordinal);
        assert!(decoded.name.is_empty());
        Ok(())
    }
}
