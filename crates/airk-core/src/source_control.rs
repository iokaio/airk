// SPDX-License-Identifier: Apache-2.0
//! Bounded retained control witnesses, never an authorization credential.
use crate::identity::{Incarnation, Sha256Digest, framed_digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_CONTROL_INPUTS: usize = 4096;
pub const MAX_CONTROL_INPUT_BYTES: u64 = 1 << 20;
pub const MAX_CONTROL_BASELINE_BYTES: usize = 1 << 20;
pub const CONTROL_BASELINE_RESOURCE: &str = "source-controls/1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlInputState {
    File { sha256: Sha256Digest, bytes: u64 },
    Absent,
    Directory,
}
/// Exact publication carrying the witness. Its own resource-map digest is
/// deliberately excluded: including it would make the witness self-referential.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlPublication {
    pub incarnation: Incarnation,
    pub sequence: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceControlBaseline {
    pub schema: u32,
    /// None only while capturing controls, or in an older unqualified witness.
    /// Only the owning locked publication path assigns the actual successor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication: Option<ControlPublication>,
    /// Complete serialized CollectionConfiguration digest, including future
    /// P11 retained-profile fields, roots, permanent policy and model identity.
    pub configuration: Sha256Digest,
    /// Complete native configuration-input identity captured for this scan.
    pub settings: Sha256Digest,
    /// Eligibility implementation version + canonical source roots + actual
    /// computed global control locators/CWD/home, not caller-selected values.
    pub contexts: BTreeMap<String, Sha256Digest>,
    pub inputs: BTreeMap<String, ControlInputState>,
}
impl SourceControlBaseline {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1 && !self.contexts.is_empty() && self.contexts.len() <= 17,
            "unsupported source control baseline"
        );
        ensure!(
            self.inputs.len() <= MAX_CONTROL_INPUTS,
            "source control entry budget exceeded"
        );
        ensure!(
            self.publication
                .is_none_or(|publication| publication.sequence > 0
                    && publication.sequence <= i64::MAX as u64),
            "invalid control publication sequence"
        );
        let mut bytes = 0u64;
        for (path, state) in &self.inputs {
            ensure!(
                !path.is_empty() && path.len() <= 32768 && !path.chars().any(char::is_control),
                "invalid retained control locator"
            );
            if let ControlInputState::File {
                bytes: captured, ..
            } = state
            {
                bytes = bytes
                    .checked_add(*captured)
                    .ok_or_else(|| anyhow::anyhow!("source control byte overflow"))?;
                ensure!(
                    bytes <= MAX_CONTROL_INPUT_BYTES,
                    "source control byte budget exceeded"
                );
            }
        }
        for root in self.contexts.keys() {
            ensure!(
                !root.is_empty() && root.len() <= 32768 && !root.chars().any(char::is_control),
                "invalid retained control context locator"
            );
        }
        crate::index_catalog::encoded_size(self, MAX_CONTROL_BASELINE_BYTES)?;
        Ok(())
    }
    pub fn matches_publication(&self, incarnation: Incarnation, sequence: u64) -> bool {
        self.publication
            == Some(ControlPublication {
                incarnation,
                sequence,
            })
    }
    pub fn bind_publication(&mut self, incarnation: Incarnation, sequence: u64) -> Result<()> {
        self.publication = Some(ControlPublication {
            incarnation,
            sequence,
        });
        self.validate()
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/source-controls/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn control_publication_binding_rejects_historical_cross_incarnation_and_successor() -> Result<()>
    {
        let mut witness = SourceControlBaseline {
            schema: 1,
            publication: None,
            configuration: Sha256Digest::from_bytes([1; 32]),
            settings: Sha256Digest::from_bytes([2; 32]),
            contexts: BTreeMap::from([("root".into(), Sha256Digest::from_bytes([3; 32]))]),
            inputs: BTreeMap::new(),
        };
        let incarnation = Incarnation::from_bytes([4; 16]);
        assert!(!witness.matches_publication(incarnation, 1));
        witness.bind_publication(incarnation, 1)?;
        assert!(witness.matches_publication(incarnation, 1));
        assert!(!witness.matches_publication(incarnation, 2));
        assert!(!witness.matches_publication(Incarnation::from_bytes([5; 16]), 1));
        assert!(witness.bind_publication(incarnation, 0).is_err());
        assert!(
            witness
                .bind_publication(incarnation, i64::MAX as u64 + 1)
                .is_err()
        );
        Ok(())
    }
}
