// SPDX-License-Identifier: Apache-2.0
//! Adoption-only path-free extraction from AIRK inventory 2/3 publications.
//! Preserves the historical payload contract without inventing semantic boundaries.
//! Field layout and validation follow AIRK v1.1.4 (also the v1.0 payload layout).
use airk_core::extraction::{
    ContinuityDescriptor, ExtractionProvenance, LocalChunk, LocalFact, LocalRegion,
    LocalRelationship, LocalRelationshipTarget, LocalSyntaxIdentity,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const EXTRACTION_CONTRACT_VERSION: u32 = 2;
const MAX_TEXT_BYTES: usize = 32 * 1024 * 1024;
const MAX_PAYLOAD_BYTES: usize = 256 * 1024 * 1024;
const MAX_EXTRACTION_CHUNKS: usize = 500_000;

/// Neutral draft output. Never embeds public path-bearing CodeEvidence IDs.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LegacyPayload {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(super) continuity_coverage: BTreeMap<
        airk_core::continuity::ContinuityFamily,
        airk_core::continuity::ContinuityCoverage,
    >,
    pub(super) contract_version: u32,
    pub(super) extraction_method: String,
    /// Exact original parse status, including legacy serialized OCR provenance.
    pub(super) parse_status: String,
    pub(super) provenance: Vec<ExtractionProvenance>,
    pub(super) text: String,
    pub(super) facts: Vec<LocalFact>,
    pub(super) syntax_identities: Vec<LocalSyntaxIdentity>,
    pub(super) chunks: Vec<LocalChunk>,
    pub(super) regions: Vec<LocalRegion>,
    pub(super) relationships: Vec<LocalRelationship>,
    pub(super) continuity: Vec<ContinuityDescriptor>,
    pub(super) diagnostics: Vec<String>,
    pub(super) partial: bool,
}

impl LegacyPayload {
    /// The owning archive loader authenticates the original bytes before decoding.
    pub(super) fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        anyhow::ensure!(
            bytes.len() <= MAX_PAYLOAD_BYTES,
            "historical payload exceeds byte bound"
        );
        let payload: Self = serde_json::from_slice(bytes)?;
        payload.validate()?;
        anyhow::ensure!(
            serde_json::to_vec(&payload)? == bytes,
            "historical payload is not canonical"
        );
        Ok(payload)
    }

    pub(super) fn validate(&self) -> anyhow::Result<()> {
        self.validate_with_text_limit(MAX_TEXT_BYTES)
    }

    /// Adapters may impose a lower bound than the shared 32 MiB text ceiling.
    pub(super) fn validate_with_text_limit(&self, text_limit: usize) -> anyhow::Result<()> {
        anyhow::ensure!(
            text_limit <= MAX_TEXT_BYTES && self.text.len() <= text_limit,
            "adapter text limit exceeded"
        );
        anyhow::ensure!(
            matches!(self.contract_version, 1 | EXTRACTION_CONTRACT_VERSION),
            "unsupported extraction contract"
        );
        anyhow::ensure!(
            self.extraction_method.len() <= 256
                && self.parse_status.len() <= 65_536
                && self.provenance.len() <= 16,
            "artifact provenance exceeds limits"
        );
        for provenance in &self.provenance {
            anyhow::ensure!(
                provenance.producer.len() <= 256
                    && provenance.producer_version.len() <= 256
                    && provenance.attributes.len() <= 64
                    && provenance.asset_digests.len() <= 256,
                "producer provenance exceeds limits"
            );
            anyhow::ensure!(
                provenance
                    .attributes
                    .iter()
                    .all(|(k, v)| k.len() <= 128 && v.len() <= 4096)
                    && provenance.asset_digests.keys().all(|k| k.len() <= 256),
                "producer provenance field exceeds limits"
            );
        }
        anyhow::ensure!(
            self.text.len() <= MAX_TEXT_BYTES && self.facts.len() <= 10_000,
            "extraction payload exceeds limits"
        );
        anyhow::ensure!(
            self.chunks.len() <= MAX_EXTRACTION_CHUNKS && self.diagnostics.len() <= 256,
            "extraction metadata exceeds limits"
        );
        anyhow::ensure!(
            self.regions.len() <= 256
                && self.relationships.len() <= 10_000
                && self.continuity.len() <= airk_core::continuity::MAX_CONTINUITY_UNITS
                && self.syntax_identities.len() <= 20_000,
            "payload metadata exceeds limits"
        );
        let mut owners = BTreeMap::new();
        let mut regions = BTreeMap::new();
        let mut decoded_bytes = self.text.len();
        let mut segments = 0usize;
        for region in &self.regions {
            anyhow::ensure!(
                regions.insert(region.id, region).is_none(),
                "duplicate region id"
            );
            decoded_bytes = decoded_bytes
                .checked_add(region.text.len())
                .ok_or_else(|| anyhow::anyhow!("decoded byte overflow"))?;
            segments = segments
                .checked_add(region.source_map.segments.len())
                .ok_or_else(|| anyhow::anyhow!("map segment overflow"))?;
        }
        anyhow::ensure!(
            decoded_bytes <= text_limit && segments <= 100_000,
            "shared region budget exceeded"
        );
        for region in &self.regions {
            let host = match region.parent {
                Some(id) => {
                    &regions
                        .get(&id)
                        .ok_or_else(|| anyhow::anyhow!("missing parent region"))?
                        .text
                }
                None => &self.text,
            };
            anyhow::ensure!(
                matches!(region.profile.contract_version, 1 | 2),
                "unsupported historical region profile"
            );
            region.profile.identity()?;
            region.source_map.validate(&region.text, host)?;
            region
                .location
                .validate(&region.text, host, &region.source_map)?;
            let mut parent = region.parent;
            let mut depth = 1;
            while let Some(id) = parent {
                depth += 1;
                anyhow::ensure!(
                    id != region.id && depth <= 4,
                    "cyclic or excessive region depth"
                );
                parent = regions
                    .get(&id)
                    .ok_or_else(|| anyhow::anyhow!("missing parent region"))?
                    .parent;
            }
        }
        let local_text = |region: Option<u32>| -> anyhow::Result<&str> {
            match region {
                None => Ok(&self.text),
                Some(id) => Ok(&regions
                    .get(&id)
                    .ok_or_else(|| anyhow::anyhow!("missing fact/chunk region"))?
                    .text),
            }
        };
        let mut syntax_identities = BTreeMap::new();
        let mut syntax_tuples = std::collections::BTreeSet::new();
        for identity in &self.syntax_identities {
            anyhow::ensure!(
                syntax_identities.insert(identity.id, identity).is_none(),
                "duplicate local syntax id"
            );
            anyhow::ensure!(
                !identity.syntax_kind.is_empty() && identity.syntax_kind.len() <= 128,
                "invalid syntax identity kind"
            );
            airk_core::location::ByteSpan {
                start: identity.span.start,
                end: identity.span.end,
            }
            .validate(local_text(identity.region)?)?;
            anyhow::ensure!(
                syntax_tuples.insert((
                    identity.region,
                    identity.span.start,
                    identity.span.end,
                    &identity.syntax_kind
                )),
                "duplicate syntax identity tuple"
            );
        }
        for fact in &self.facts {
            let text = local_text(fact.region)?;
            airk_core::location::ByteSpan {
                start: fact.span.start,
                end: fact.span.end,
            }
            .validate(text)?;
            if let Some(code) = &fact.code {
                let identity = syntax_identities
                    .get(&code.syntax_id)
                    .ok_or_else(|| anyhow::anyhow!("missing code syntax identity"))?;
                anyhow::ensure!(
                    identity.region == fact.region && identity.syntax_kind == fact.syntax_kind,
                    "code syntax identity context mismatch"
                );
                if let Some(span) = &code.span {
                    anyhow::ensure!(
                        span.start_byte == identity.span.start
                            && span.end_byte == identity.span.end,
                        "code syntax identity span mismatch"
                    );
                }
                if let Some(owner_id) = code.owner_id {
                    let owner = syntax_identities
                        .get(&owner_id)
                        .ok_or_else(|| anyhow::anyhow!("missing code owner syntax identity"))?;
                    anyhow::ensure!(
                        owner_id != code.syntax_id
                            && owner.region == identity.region
                            && owner.span.start <= identity.span.start
                            && owner.span.end >= identity.span.end,
                        "invalid code owner syntax identity"
                    );
                }
                anyhow::ensure!(
                    code.basis != airk_core::EvidenceBasis::Imported,
                    "imported code belongs to a bound producer domain"
                );
                for span in [&code.span, &code.signature_span].into_iter().flatten() {
                    airk_core::location::ByteSpan {
                        start: span.start_byte,
                        end: span.end_byte,
                    }
                    .validate(text)?;
                }
            }
        }
        for chunk in &self.chunks {
            airk_core::location::ByteSpan {
                start: chunk.span.start,
                end: chunk.span.end,
            }
            .validate(local_text(chunk.region)?)?;
        }
        for fact in &self.facts {
            anyhow::ensure!(
                owners.insert(fact.id, fact).is_none(),
                "duplicate local fact id"
            );
            anyhow::ensure!(
                fact.kind.len() <= 128 && fact.name.len() <= 4096 && fact.attributes.len() <= 64,
                "fact metadata exceeds limits"
            );
            anyhow::ensure!(
                fact.attributes
                    .iter()
                    .all(|(k, v)| k.len() <= 128 && v.len() <= 4096),
                "fact attribute exceeds limits"
            );
        }
        for span in self
            .facts
            .iter()
            .filter(|f| f.region.is_none())
            .map(|f| &f.span)
            .chain(
                self.chunks
                    .iter()
                    .filter(|c| c.region.is_none())
                    .map(|c| &c.span),
            )
        {
            anyhow::ensure!(
                span.start <= span.end
                    && span.end <= self.text.len()
                    && self.text.is_char_boundary(span.start)
                    && self.text.is_char_boundary(span.end),
                "invalid local UTF-8 span"
            );
        }
        for fact in &self.facts {
            let mut next = fact.owner;
            let mut depth = 0;
            while let Some(id) = next {
                depth += 1;
                anyhow::ensure!(
                    depth <= 128 && id != fact.id,
                    "cyclic or excessive local owner chain"
                );
                next = owners
                    .get(&id)
                    .ok_or_else(|| anyhow::anyhow!("missing local owner"))?
                    .owner;
            }
        }
        anyhow::ensure!(
            self.diagnostics.iter().all(|d| d.len() <= 4096),
            "diagnostic exceeds limits"
        );
        let metadata_bytes: usize = self
            .facts
            .iter()
            .map(|f| {
                f.kind.len()
                    + f.name.len()
                    + f.attributes
                        .iter()
                        .map(|(k, v)| k.len() + v.len())
                        .sum::<usize>()
            })
            .sum();
        anyhow::ensure!(
            metadata_bytes <= 16_777_216,
            "aggregate fact metadata exceeds limits"
        );
        for relation in &self.relationships {
            anyhow::ensure!(
                owners.contains_key(&relation.source),
                "missing relationship source"
            );
            anyhow::ensure!(
                relation.basis != airk_core::EvidenceBasis::Imported,
                "imported relationships require bound producer domain"
            );
            airk_core::location::ByteSpan {
                start: relation.span.start,
                end: relation.span.end,
            }
            .validate(local_text(owners[&relation.source].region)?)?;
            if let LocalRelationshipTarget::Local { id } = relation.target {
                anyhow::ensure!(
                    owners.contains_key(&id),
                    "missing local relationship target"
                );
            }
        }
        let mut descriptors = BTreeMap::new();
        for descriptor in &self.continuity {
            use airk_core::continuity::ContinuitySubject;
            anyhow::ensure!(
                descriptor.algorithm_version == airk_core::continuity::CONTINUITY_VERSION
                    && descriptors.insert(descriptor.subject, descriptor).is_none()
                    && !descriptor.category.is_empty()
                    && descriptor.category.len() <= 256
                    && descriptor.dimensions.len() <= 12
                    && descriptor.unsupported_dimensions.len() <= 12
                    && descriptor
                        .unsupported_dimensions
                        .iter()
                        .all(|dimension| !descriptor.dimensions.contains_key(dimension)),
                "invalid continuity descriptor"
            );
            anyhow::ensure!(
                match descriptor.subject {
                    ContinuitySubject::Syntax(id) => syntax_identities
                        .get(&id)
                        .is_some_and(|item| item.region == descriptor.region),
                    ContinuitySubject::Fact(id) => owners
                        .get(&id)
                        .is_some_and(|item| item.region == descriptor.region),
                    ContinuitySubject::Region(id) => regions.contains_key(&id),
                    ContinuitySubject::Unit(_) => true,
                },
                "continuity subject is missing or in a foreign region"
            );
            airk_core::location::ByteSpan {
                start: descriptor.span.start,
                end: descriptor.span.end,
            }
            .validate(local_text(descriptor.region)?)?;
        }
        for descriptor in &self.continuity {
            let mut owner = descriptor.owner;
            let mut depth = 0;
            while let Some(id) = owner {
                depth += 1;
                anyhow::ensure!(
                    id != descriptor.subject && depth <= 128,
                    "cyclic continuity owner"
                );
                owner = descriptors
                    .get(&id)
                    .ok_or_else(|| anyhow::anyhow!("missing continuity owner"))?
                    .owner;
            }
        }
        anyhow::ensure!(
            serde_json::to_vec(&self.continuity)?.len()
                <= airk_core::continuity::MAX_CONTINUITY_BYTES,
            "continuity descriptor byte budget exceeded"
        );
        // Includes nested document/table/code evidence in the durable bound.
        anyhow::ensure!(
            serde_json::to_vec(self)?.len() <= MAX_PAYLOAD_BYTES,
            "serialized extraction payload exceeds 256 MiB"
        );
        Ok(())
    }
}
