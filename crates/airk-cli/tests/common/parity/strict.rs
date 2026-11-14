// SPDX-License-Identifier: Apache-2.0
//! Strict v2 parity for new transport observations. Never reads or rewrites U95
//! v1 captures, whose blanket hash normalization lost stable digest bytes.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, io::Write, path::Path};

/// Each identity is an exact JSON pointer and the observed transport-owned value.
/// Stable evidence/model/publication/revision fields cannot be registered.
#[derive(Default)]
pub struct Normalization {
    pub identities: BTreeMap<String, String>,
    pub timing: Vec<String>,
}

// Exact outer observation fields only. Nested native evidence is never a
// transport wrapper. Extend only after reviewing an actual caller.
const IDENTITY_POINTERS: &[&str] = &[
    "/request_id",
    "/service_instance",
    "/window_id",
    "/connection_id",
];
const TIMING_POINTERS: &[&str] = &[
    "/observed_unix_ms",
    "/observed_unix_secs",
    "/elapsed_ms",
    "/uptime_ms",
    "/cache_age_ms",
];

pub fn normalize(value: &Value, rules: &Normalization) -> Result<Value, String> {
    let mut result = value.clone();
    for (pointer, expected) in &rules.identities {
        if !IDENTITY_POINTERS.contains(&pointer.as_str()) {
            return Err(format!("unreviewed transport identity pointer: {pointer}"));
        }
        let field = result
            .pointer_mut(pointer)
            .ok_or_else(|| format!("missing transport identity: {pointer}"))?;
        if expected.is_empty() || field.as_str() != Some(expected.as_str()) {
            return Err(format!(
                "transport identity differs from captured context: {pointer}"
            ));
        }
        *field = Value::String(format!("<transport:{pointer}>"));
    }
    for pointer in &rules.timing {
        if !TIMING_POINTERS.contains(&pointer.as_str()) {
            return Err(format!("unreviewed observation timing pointer: {pointer}"));
        }
        let field = result
            .pointer_mut(pointer)
            .ok_or_else(|| format!("missing observation timing: {pointer}"))?;
        if !field.is_u64() {
            return Err(format!("invalid observation timing: {pointer}"));
        }
        *field = Value::String(format!("<timing:{pointer}>"));
    }
    Ok(result)
}

pub fn equivalent(
    left: &Value,
    left_rules: &Normalization,
    right: &Value,
    right_rules: &Normalization,
) -> Result<bool, String> {
    Ok(normalize(left, left_rules)? == normalize(right, right_rules)?)
}

/// Actual capture callers supply digests observed from their identified matched
/// pair. These fields identify evidence; constructing a record is not runtime
/// qualification and does not authenticate a caller's self-asserted identity.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub runtime_manifest_sha256: String,
    pub service_sha256: String,
    pub cli_sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema: String,
    pub source: Source,
    pub value: Value,
}

impl Snapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != "airk.split.parity/2" {
            return Err("strict parity requires an actual v2 raw capture".into());
        }
        for digest in [
            &self.source.runtime_manifest_sha256,
            &self.source.service_sha256,
            &self.source.cli_sha256,
        ] {
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err("strict parity source digest is invalid".into());
            }
        }
        Ok(())
    }
}

/// Preserve complete raw values and source identity. Never overwrites a prior
/// observation and never reconstructs hashes from a legacy canonical fixture.
pub fn capture(
    path: &Path,
    source: Source,
    value: Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = Snapshot {
        schema: "airk.split.parity/2".into(),
        source,
        value,
    };
    snapshot.validate()?;
    let bytes = serde_json::to_vec_pretty(&snapshot)?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    output.write_all(&bytes)?;
    output.write_all(b"\n")?;
    output.sync_all()?;
    Ok(())
}
