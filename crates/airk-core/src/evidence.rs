// SPDX-License-Identifier: Apache-2.0
//! Exact publication envelopes and representation-specific conditional reads.
use crate::{changes::ContinuityBasis, identity::*};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionEnvelope {
    pub collection: String,
    pub incarnation: Incarnation,
    pub generation: u64,
    pub revision: Sha256Digest,
    pub token: RevisionToken,
}

impl RevisionEnvelope {
    pub fn new(collection: String, token: RevisionToken) -> Self {
        Self {
            collection,
            incarnation: token.incarnation,
            generation: token.generation,
            revision: token.revision,
            token,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EvidenceReadOptions {
    /// Exact retained publication; never a generation-only or live-source fallback.
    pub revision: Option<RevisionToken>,
    pub if_evidence: Option<Sha256Digest>,
    pub follow_moves: bool,
    /// Source-byte identity only; never substitutes for an evidence validator.
    pub known_hash: Option<Sha256Digest>,
    pub artifact_id: Option<ArtifactId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceReadStatus {
    HistoryUnavailable,
    Content,
    NotModified,
    MoveCandidate,
    RemovedFromIndex,
    Ambiguous,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadEvidence {
    pub publication: Option<RevisionEnvelope>,
    #[serde(default)]
    pub current_publication: Option<RevisionEnvelope>,
    #[serde(default)]
    pub historical: bool,
    #[serde(default)]
    pub original_root: Option<String>,
    pub status: EvidenceReadStatus,
    pub requested_path: String,
    pub resolved_path: Option<String>,
    pub validator: Option<Sha256Digest>,
    pub basis: ContinuityBasis,
    pub reason: Option<String>,
}

/// Existing payload fields stay top-level; not-modified/missing responses omit
/// the payload and retain an exact revision and explicit bounded outcome.
#[derive(Clone, Debug, Serialize)]
pub struct ConditionalRead<T> {
    pub read: ReadEvidence,
    #[serde(flatten)]
    pub content: Option<T>,
}
