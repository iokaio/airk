// SPDX-License-Identifier: Apache-2.0
//! Bounded combined consistency cursors. These are not authorization tokens;
//! every resumed member still requires live native scope and publication checks.
use airk_core::{
    combined_reads::PublicationVector,
    evidence::RevisionEnvelope,
    identity::{RevisionToken, Sha256Digest, framed_digest},
    index_catalog::{IndexId, MAX_SELECTION, encoded_size, unique_bounded},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

const PREFIX: &str = "airk-cq1:";
const MAX_CURSOR_BYTES: usize = 32_768;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Continuation {
    schema: u32,
    tool: String,
    arguments: Sha256Digest,
    scope_identity: Sha256Digest,
    vector: PublicationVector,
    /// Every member is represented, including exhausted members whose exact
    /// publication must still be checked before resuming any selected member.
    children: BTreeMap<IndexId, Option<String>>,
    checksum: Sha256Digest,
}

fn arguments_identity(tool: &str, args: &Map<String, Value>) -> Result<Sha256Digest> {
    ensure!(
        !tool.is_empty() && tool.len() <= 128,
        "invalid combined cursor tool"
    );
    // Response mode/status repetition affects transport observations only.
    // Projection, filters, modes, count/text/verification budgets all bind.
    let mut semantic = args.clone();
    for key in ["cursor", "response_mode", "include_status"] {
        semantic.remove(key);
    }
    encoded_size(&semantic, airk_core::MAX_RESPONSE_BYTES)?;
    Ok(framed_digest(
        b"airk/combined-cursor-arguments/1",
        &[tool.as_bytes(), &serde_json::to_vec(&semantic)?],
    ))
}

impl Continuation {
    pub(super) fn new(
        tool: &str,
        args: &Map<String, Value>,
        scope_identity: Sha256Digest,
        vector: PublicationVector,
        child_cursors: BTreeMap<IndexId, Option<String>>,
    ) -> Result<Self> {
        let mut cursor = Self {
            schema: 1,
            tool: tool.into(),
            arguments: arguments_identity(tool, args)?,
            scope_identity,
            vector,
            children: child_cursors,
            checksum: Sha256Digest::from_bytes([0; 32]),
        };
        cursor.validate()?;
        cursor.checksum = cursor.expected_checksum()?;
        // Establish the wire bound before returning a retained cursor object.
        cursor.encode()?;
        Ok(cursor)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1 && !self.tool.is_empty() && self.tool.len() <= 128,
            "unsupported combined cursor"
        );
        self.vector.validate()?;
        ensure!(
            self.vector.authority == self.scope_identity,
            "combined cursor authority mismatch"
        );
        ensure!(
            self.children.keys().copied().eq(self
                .vector
                .members
                .iter()
                .map(|member| member.index_id)),
            "combined cursor must retain every selected member"
        );
        ensure!(
            self.vector
                .members
                .iter()
                .all(|member| member.publication.is_some()),
            "unavailable member publication prevents combined continuation"
        );
        for child in self.children.values().flatten() {
            ensure!(
                !child.is_empty() && child.len() <= MAX_CURSOR_BYTES,
                "invalid native child cursor"
            );
        }
        Ok(())
    }

    fn expected_checksum(&self) -> Result<Sha256Digest> {
        // Include original observation dates in the payload checksum. Resume
        // compares exact publications, never those dates with a new interval.
        Ok(framed_digest(
            b"airk/combined-cursor/1",
            &[&serde_json::to_vec(&(
                self.schema,
                &self.tool,
                self.arguments,
                self.scope_identity,
                &self.vector,
                &self.children,
            ))?],
        ))
    }

    pub(super) fn decode(
        encoded: &str,
        tool: &str,
        args: &Map<String, Value>,
        scope_identity: Sha256Digest,
        selected_ids: &[IndexId],
    ) -> Result<Self> {
        ensure!(
            encoded.len() <= MAX_CURSOR_BYTES,
            "combined cursor exceeds 32 KiB"
        );
        unique_bounded(selected_ids, MAX_SELECTION)?;
        ensure!(
            !selected_ids.is_empty() && selected_ids.windows(2).all(|pair| pair[0] < pair[1]),
            "combined cursor selected indexes are not canonical"
        );
        let cursor: Self = serde_json::from_str(
            encoded
                .strip_prefix(PREFIX)
                .context("unsupported combined cursor version")?,
        )
        .context("malformed combined cursor")?;
        cursor.validate()?;
        ensure!(
            cursor.tool == tool
                && cursor.arguments == arguments_identity(tool, args)?
                && cursor.scope_identity == scope_identity
                && cursor
                    .children
                    .keys()
                    .copied()
                    .eq(selected_ids.iter().copied()),
            "combined_cursor_expired"
        );
        ensure!(
            cursor.checksum == cursor.expected_checksum()?,
            "combined cursor checksum mismatch"
        );
        // Native emission is canonical. This also rejects duplicate JSON keys,
        // silently overwritten map entries and noncanonical identity spellings.
        ensure!(cursor.encode()? == encoded, "noncanonical combined cursor");
        Ok(cursor)
    }

    pub(super) fn encode(&self) -> Result<String> {
        self.validate()?;
        ensure!(
            self.checksum == self.expected_checksum()?,
            "combined cursor checksum mismatch"
        );
        encoded_size(self, MAX_CURSOR_BYTES - PREFIX.len())?;
        Ok(format!("{PREFIX}{}", serde_json::to_string(self)?))
    }

    pub(super) fn cursor(&self, index: IndexId) -> Option<&str> {
        self.children.get(&index).and_then(Option::as_deref)
    }

    pub(super) fn is_exhausted(&self, index: IndexId) -> bool {
        self.children.get(&index).is_some_and(Option::is_none)
    }

    pub(super) fn publication(&self, index: IndexId) -> Option<&RevisionEnvelope> {
        self.vector
            .members
            .iter()
            .find(|member| member.index_id == index)
            .and_then(|member| member.publication.as_ref())
    }

    pub(super) fn has_more(&self) -> bool {
        self.children.values().any(Option::is_some)
    }

    /// Called for *every* selected index, including exhausted members, using
    /// an actual same-authority native observation in this resumed request.
    pub(super) fn validate_publication(&self, index: IndexId, token: &str) -> Result<()> {
        let expected = self
            .publication(index)
            .context("combined cursor contains no selected member publication")?;
        let current: RevisionToken = token.parse()?;
        ensure!(current == expected.token, "combined_cursor_expired");
        Ok(())
    }
}

/// Native discovery uses a top-level cursor; references owns its page cursor.
/// A truncated page without a cursor is a declared budget cutoff, not exhaustion.
pub(super) fn child_cursor(body: &Value) -> Result<Option<String>> {
    let body = body.get("structuredContent").unwrap_or(body);
    let top = body.get("cursor").filter(|value| !value.is_null());
    let page = body
        .pointer("/page/cursor")
        .filter(|value| !value.is_null());
    ensure!(
        top.is_none() || page.is_none(),
        "ambiguous native continuation fields"
    );
    if let Some(cursor) = top.or(page) {
        let cursor = cursor
            .as_str()
            .context("native continuation is not a string")?;
        ensure!(
            !cursor.is_empty() && cursor.len() <= MAX_CURSOR_BYTES,
            "native continuation exceeds bound"
        );
        return Ok(Some(cursor.into()));
    }
    let more = body
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || body
            .pointer("/evidence/truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || body
            .pointer("/page/more")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    ensure!(
        !more,
        "native page cannot resume within the selected budget"
    );
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{combined_reads::MemberPublication, identity::Incarnation};
    use serde_json::json;
    fn index(n: u8) -> IndexId {
        IndexId::from_identity(Incarnation::from_bytes([n; 16]))
    }
    fn publication(sequence: u64) -> RevisionEnvelope {
        RevisionEnvelope::new(
            "fixture".into(),
            RevisionToken {
                incarnation: Incarnation::from_bytes([1; 16]),
                generation: sequence,
                revision: Sha256Digest::from_bytes([sequence as u8; 32]),
            },
        )
    }
    fn authority() -> Sha256Digest {
        Sha256Digest::from_bytes([7; 32])
    }
    fn args() -> Map<String, Value> {
        json!({"limit":4,"max_bytes":8192,"reply_detail":"lean","verification":"report","verification_max_sources":8}).as_object().unwrap().clone()
    }
    fn vector() -> PublicationVector {
        PublicationVector {
            authority: authority(),
            observed_from_unix_ms: 10,
            observed_to_unix_ms: 20,
            members: vec![
                MemberPublication {
                    index_id: index(1),
                    publication: Some(publication(1)),
                },
                MemberPublication {
                    index_id: index(2),
                    publication: Some(publication(2)),
                },
            ],
        }
    }
    fn cursor() -> Result<Continuation> {
        Continuation::new(
            "airk_symbols",
            &args(),
            authority(),
            vector(),
            BTreeMap::from([(index(1), Some("native-one".into())), (index(2), None)]),
        )
    }

    #[test]
    fn roundtrip_keeps_exhausted_member_in_the_whole_publication_vector() -> Result<()> {
        let cursor = cursor()?;
        let encoded = cursor.encode()?;
        let decoded = Continuation::decode(
            &encoded,
            "airk_symbols",
            &args(),
            authority(),
            &[index(1), index(2)],
        )?;
        assert_eq!(decoded.cursor(index(1)), Some("native-one"));
        assert!(!decoded.is_exhausted(index(1)));
        assert!(decoded.is_exhausted(index(2)));
        assert!(!decoded.is_exhausted(index(3)));
        assert!(decoded.has_more());
        decoded.validate_publication(index(1), &publication(1).token.to_string())?;
        decoded.validate_publication(index(2), &publication(2).token.to_string())?;
        assert!(
            decoded
                .validate_publication(index(2), &publication(3).token.to_string())
                .is_err()
        );
        assert!(
            decoded
                .validate_publication(index(3), &publication(1).token.to_string())
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn tool_projection_filters_budgets_scope_and_selected_set_are_bound() -> Result<()> {
        let encoded = cursor()?.encode()?;
        for (key, value) in [
            ("reply_detail", json!("rich")),
            ("limit", json!(5)),
            ("max_bytes", json!(8193)),
            ("path_prefix", json!("src")),
            ("verification", json!("off")),
            ("verification_max_sources", json!(9)),
        ] {
            let mut changed = args();
            changed.insert(key.into(), value);
            assert!(
                Continuation::decode(
                    &encoded,
                    "airk_symbols",
                    &changed,
                    authority(),
                    &[index(1), index(2)]
                )
                .is_err()
            );
        }
        assert!(
            Continuation::decode(
                &encoded,
                "airk_code_facts",
                &args(),
                authority(),
                &[index(1), index(2)]
            )
            .is_err()
        );
        assert!(
            Continuation::decode(
                &encoded,
                "airk_symbols",
                &args(),
                Sha256Digest::from_bytes([8; 32]),
                &[index(1), index(2)]
            )
            .is_err()
        );
        assert!(
            Continuation::decode(&encoded, "airk_symbols", &args(), authority(), &[index(1)])
                .is_err()
        );
        let mut display = args();
        display.insert("cursor".into(), json!(&encoded));
        display.insert("response_mode".into(), json!("full"));
        display.insert("include_status".into(), json!(true));
        Continuation::decode(
            &encoded,
            "airk_symbols",
            &display,
            authority(),
            &[index(1), index(2)],
        )?;
        Ok(())
    }

    #[test]
    fn malformed_overlong_tampered_or_noncanonical_cursors_are_refused() -> Result<()> {
        let encoded = cursor()?.encode()?;
        assert!(
            Continuation::decode(
                &"x".repeat(MAX_CURSOR_BYTES + 1),
                "airk_symbols",
                &args(),
                authority(),
                &[index(1), index(2)]
            )
            .is_err()
        );
        assert!(
            Continuation::decode(
                "airk-cq1:{malformed}",
                "airk_symbols",
                &args(),
                authority(),
                &[index(1), index(2)]
            )
            .is_err()
        );
        let changed = encoded.replace("native-one", "native-two");
        assert!(
            Continuation::decode(
                &changed,
                "airk_symbols",
                &args(),
                authority(),
                &[index(1), index(2)]
            )
            .is_err()
        );
        let pretty = format!(
            "{PREFIX}{}",
            serde_json::to_string_pretty(&serde_json::from_str::<Value>(
                encoded.strip_prefix(PREFIX).unwrap()
            )?)?
        );
        assert!(
            Continuation::decode(
                &pretty,
                "airk_symbols",
                &args(),
                authority(),
                &[index(1), index(2)]
            )
            .is_err()
        );
        let mut unavailable = vector();
        unavailable.members[1].publication = None;
        assert!(
            Continuation::new(
                "airk_symbols",
                &args(),
                authority(),
                unavailable,
                BTreeMap::from([(index(1), Some("native".into())), (index(2), None)])
            )
            .is_err()
        );
        assert!(
            Continuation::new(
                "airk_symbols",
                &args(),
                authority(),
                vector(),
                BTreeMap::from([(index(1), None)])
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn child_pagination_never_confuses_budget_cutoff_with_exhaustion() -> Result<()> {
        assert_eq!(
            child_cursor(&json!({"cursor":"one","truncated":true}))?,
            Some("one".into())
        );
        assert_eq!(
            child_cursor(
                &json!({"page":{"cursor":"two","more":true},"evidence":{"truncated":true}})
            )?,
            Some("two".into())
        );
        assert_eq!(
            child_cursor(&json!({"cursor":null,"truncated":false}))?,
            None
        );
        assert!(child_cursor(&json!({"truncated":true})).is_err());
        assert!(child_cursor(&json!({"evidence":{"truncated":true}})).is_err());
        assert!(child_cursor(&json!({"cursor":42})).is_err());
        assert!(child_cursor(&json!({"cursor":"one","page":{"cursor":"two"}})).is_err());
        Ok(())
    }
}
