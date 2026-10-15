// SPDX-License-Identifier: Apache-2.0
//! Path-free continuity candidates. Assigned persistent handles belong to kernel.
use crate::{
    DocumentLocation,
    changes::ChangeDimension,
    extraction::{LocalChunk, LocalSpan, LocalSyntaxId},
    identity::Sha256Digest,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CONTINUITY_VERSION: u32 = 1;
pub const MAX_CONTINUITY_UNITS: usize = 20_000;
pub const MAX_CONTINUITY_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CONTINUITY_HASH_BYTES: usize = 64 * 1024 * 1024;

/// Computed while native syntax is available, before display truncation.
/// Missing fields are unsupported; a supported absent value hashes empty bytes.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeFingerprints {
    pub version: u32,
    pub name: Option<Sha256Digest>,
    pub qualified_name: Option<Sha256Digest>,
    pub signature: Option<Sha256Digest>,
    pub body: Option<Sha256Digest>,
    #[serde(default)]
    pub body_informative: bool,
    pub documentation: Option<Sha256Digest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuityFamily {
    Code,
    Documents,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuityCoverage {
    pub supported: bool,
    pub complete: bool,
}

/// IDs are local to one payload; they never establish continuity themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ContinuitySubject {
    Syntax(LocalSyntaxId),
    Fact(u32),
    Unit(u32),
    Region(u32),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuityDescriptor {
    pub algorithm_version: u32,
    pub subject: ContinuitySubject,
    pub owner: Option<ContinuitySubject>,
    pub region: Option<u32>,
    pub family: ContinuityFamily,
    pub category: String,
    /// Complete name/qualified name, never truncated display text.
    pub name: Option<Sha256Digest>,
    /// Explicit written requirement/decision/cell ID when available.
    pub explicit_identity: Option<Sha256Digest>,
    pub profile: Sha256Digest,
    pub provenance: Sha256Digest,
    pub dimensions: BTreeMap<ChangeDimension, Sha256Digest>,
    /// An absent digest is unknown, not proof of an unchanged dimension.
    pub unsupported_dimensions: Vec<ChangeDimension>,
    pub span: LocalSpan,
    pub location: DocumentLocation,
    pub complete: bool,
    pub body_informative: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "key",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ContinuityKey {
    Symbol(crate::identity::SymbolKey),
    Document(crate::identity::DocumentKey),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuityRecord {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ambiguous_predecessor: bool,
    pub key: ContinuityKey,
    pub descriptor: ContinuityDescriptor,
    pub first_generation: u64,
    pub changed: BTreeMap<ChangeDimension, u64>,
}

/// Bind only native facts to an unambiguous descriptor from their own file.
pub fn fact_key(
    records: &[ContinuityRecord],
    ordinal: usize,
    fact: &crate::Fact,
) -> Option<ContinuityKey> {
    FactKeyIndex::new(records).key(ordinal, fact)
}

/// Construct once per bounded file; lookups never scan all descriptors.
pub struct FactKeyIndex {
    facts: BTreeMap<usize, Option<ContinuityKey>>,
    syntax: BTreeMap<(usize, usize), Option<ContinuityKey>>,
}

impl FactKeyIndex {
    pub fn new(records: &[ContinuityRecord]) -> Self {
        let mut index = Self {
            facts: BTreeMap::new(),
            syntax: BTreeMap::new(),
        };
        for record in records {
            match record.descriptor.subject {
                ContinuitySubject::Fact(id) => {
                    index
                        .facts
                        .entry(id as usize)
                        .and_modify(|key| *key = None)
                        .or_insert(Some(record.key));
                }
                ContinuitySubject::Syntax(_)
                    if record.descriptor.family == ContinuityFamily::Code =>
                {
                    index
                        .syntax
                        .entry((record.descriptor.span.start, record.descriptor.span.end))
                        .and_modify(|key| *key = None)
                        .or_insert(Some(record.key));
                }
                _ => {}
            }
        }
        index
    }

    pub fn key(&self, ordinal: usize, fact: &crate::Fact) -> Option<ContinuityKey> {
        if fact
            .code
            .as_ref()
            .is_some_and(|code| code.basis == crate::EvidenceBasis::Imported)
        {
            return None;
        }
        let written = self.facts.get(&ordinal);
        let syntax = fact
            .code
            .as_ref()
            .filter(|code| code.basis == crate::EvidenceBasis::Syntax)
            .and_then(|code| code.span.as_ref())
            .and_then(|span| self.syntax.get(&(span.start_byte, span.end_byte)));
        match (written, syntax) {
            (Some(key), None) | (None, Some(key)) => *key,
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ContinuityDraft {
    pub descriptors: Vec<ContinuityDescriptor>,
    /// Nonoverlapping final partition is constructed by the shared orchestrator.
    pub chunk_hints: Vec<LocalChunk>,
    pub supported: bool,
    pub partial: bool,
    pub diagnostics: Vec<String>,
}

/// One shared charge ledger per host, including nested and failed descriptors.
#[derive(Clone, Debug, Default)]
pub struct ContinuityBudget {
    pub units: usize,
    pub hash_bytes: usize,
}
impl ContinuityBudget {
    pub fn charge(&mut self, bytes: usize) -> anyhow::Result<()> {
        self.units = self
            .units
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("continuity unit overflow"))?;
        self.hash_bytes = self
            .hash_bytes
            .checked_add(bytes)
            .ok_or_else(|| anyhow::anyhow!("continuity hash overflow"))?;
        anyhow::ensure!(
            self.units <= MAX_CONTINUITY_UNITS && self.hash_bytes <= MAX_CONTINUITY_HASH_BYTES,
            "continuity work budget exceeded"
        );
        Ok(())
    }
}
