// SPDX-License-Identifier: Apache-2.0
//! V2 extraction contracts; independent of parser, filesystem and storage.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const EXTRACTION_CONTRACT_VERSION: u32 = 2;
/// Preserves legacy 40-line chunks across the bounded native source ceiling.
/// The independent encoded payload limit remains authoritative.
pub const MAX_EXTRACTION_CHUNKS: usize = 500_000;
pub const MAX_SOURCE_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_TEXT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 256 * 1024 * 1024;

/// A source slice admitted under the native input ceiling. Document adapters
/// must additionally enforce their lower extraction/text limits.
pub struct AdapterInput<'a> {
    bytes: &'a [u8],
    pub profile: &'a ExtractionProfile,
    pub region: Option<AdapterRegionContext<'a>>,
}

pub struct AdapterRegionContext<'a> {
    pub id: u32,
    pub depth: usize,
    pub host_text: &'a str,
    pub source_map: &'a crate::location::SourceMap,
}

impl<'a> AdapterInput<'a> {
    pub fn new(
        bytes: &'a [u8],
        profile: &'a ExtractionProfile,
        region: Option<AdapterRegionContext<'a>>,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            bytes.len() <= MAX_SOURCE_BYTES,
            "adapter source exceeds native ceiling"
        );
        profile.identity()?;
        if let Some(context) = &region {
            anyhow::ensure!(
                (1..=4).contains(&context.depth),
                "invalid input region depth"
            );
            context
                .source_map
                .validate(std::str::from_utf8(bytes)?, context.host_text)?;
        }
        Ok(Self {
            bytes,
            profile,
            region,
        })
    }
    pub fn bytes(&self) -> &[u8] {
        self.bytes
    }
}

/// Implementations charge all visited/failed/empty work to the same host
/// budget, including nested adapters, and return only validated local output.
pub trait NativeAdapter {
    fn extract(
        &self,
        input: AdapterInput<'_>,
        budget: &mut crate::location::HostBudget,
    ) -> anyhow::Result<ExtractionPayload>;
}

/// Collection-level evidence selection. Missing legacy settings mean Detailed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexDetail {
    Essential,
    AgentLean,
    Balanced,
    #[default]
    Detailed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexCoverage {
    pub level: IndexDetail,
    pub note: String,
}
impl From<IndexDetail> for IndexCoverage {
    fn from(level: IndexDetail) -> Self {
        Self {
            level,
            note: level.coverage().into(),
        }
    }
}
impl Default for IndexCoverage {
    fn default() -> Self {
        IndexDetail::Detailed.into()
    }
}
impl IndexDetail {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Essential => "essential",
            Self::AgentLean => "agent_lean",
            Self::Balanced => "balanced",
            Self::Detailed => "detailed",
        }
    }
    pub fn from_profile(profile: &ExtractionProfile) -> anyhow::Result<Self> {
        match profile.options.get("index_detail").map(String::as_str) {
            None | Some("detailed") => Ok(Self::Detailed),
            Some("essential") => Ok(Self::Essential),
            Some("agent_lean") => Ok(Self::AgentLean),
            Some("balanced") => Ok(Self::Balanced),
            _ => anyhow::bail!("unsupported index detail"),
        }
    }
    pub fn coverage(self) -> &'static str {
        match self {
            Self::AgentLean => {
                "Selected declarations, imports, call occurrences, local structure, entry points and document facts. Broad framework facts, standalone dates and fine-grained continuity are deliberately omitted. Call targets are written syntax candidates, not resolved; exact text and semantic inputs remain independently available."
            }
            Self::Essential => {
                "Native structured facts, relationships and fine-grained continuity are not indexed at Essential detail. Text and exact citations remain available. Explicit imported analysis is separate."
            }
            Self::Balanced => {
                "Native call occurrences, declarations, imports, other structural facts and declaration continuity are adapter-dependent. Call targets are written syntax candidates, not resolved. Document-unit continuity is not indexed at Balanced detail. Explicit imported analysis is separate."
            }
            Self::Detailed => {
                "Selected adapter-supported facts, relationships and continuity; not complete language understanding or resolved references."
            }
        }
    }

    pub fn retains_fact(self, kind: &str) -> bool {
        match self {
            Self::Essential => false,
            Self::AgentLean => matches!(
                kind,
                "definition"
                    | "call"
                    | "dynamic_call"
                    | "import"
                    | "inherits"
                    | "key"
                    | "section"
                    | "requirement"
                    | "decision"
                    | "acceptance"
                    | "architecture"
                    | "design"
                    | "route_registration"
                    | "route_annotation"
                    | "test_registration"
                    | "test_declaration"
                    | "table"
                    | "table_row"
            ),
            Self::Balanced => true,
            Self::Detailed => true,
        }
    }

    pub fn tracks_continuity(self) -> bool {
        matches!(self, Self::Balanced | Self::Detailed)
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Essential => "Search Only",
            Self::AgentLean => "Agent Lean",
            Self::Balanced => "Rich Structure",
            Self::Detailed => "Detailed",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterCapabilities {
    pub lexical: bool,
    pub definitions: bool,
    pub imports: bool,
    pub call_candidates: bool,
    pub structural_facts: bool,
    pub embedded_regions: bool,
    pub native_locations: bool,
    pub continuity: bool,
    pub producer_assertions: bool,
}

/// Only effective inputs belong here, never the entire registry or policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionProfile {
    pub contract_version: u32,
    pub adapter_id: String,
    pub adapter_version: String,
    pub language: String,
    pub dialect: Option<String>,
    pub grammar_identity: Option<String>,
    pub mapping_version: u32,
    pub chunk_version: u32,
    pub options: BTreeMap<String, String>,
    /// Hashes of only the captured local configuration inputs actually used.
    pub configuration_inputs: BTreeMap<String, crate::identity::Sha256Digest>,
}

impl ExtractionProfile {
    pub fn identity(&self) -> anyhow::Result<crate::identity::Sha256Digest> {
        anyhow::ensure!(
            matches!(self.contract_version, 1 | EXTRACTION_CONTRACT_VERSION),
            "unsupported profile contract"
        );
        let bytes = serde_json::to_vec(self)?;
        anyhow::ensure!(
            bytes.len() <= 131_072,
            "profile exceeds identity input limit"
        );
        Ok(crate::identity::framed_digest(
            b"extraction-profile-v1",
            &[&bytes],
        ))
    }
}

pub fn extraction_key(
    source: &crate::identity::Sha256Digest,
    profile: &crate::identity::Sha256Digest,
) -> crate::identity::Sha256Digest {
    crate::identity::framed_digest(
        b"extraction-key-v1",
        &[source.as_bytes(), profile.as_bytes()],
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterDescriptor {
    pub profile: ExtractionProfile,
    pub capabilities: AdapterCapabilities,
    pub limitations: Vec<String>,
}

/// Byte span in decoded payload text. Host binding is a separate source map.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalFact {
    pub id: u32,
    pub owner: Option<u32>,
    pub kind: String,
    pub name: String,
    pub span: LocalSpan,
    pub syntax_kind: String,
    pub start_line: usize,
    pub end_line: usize,
    pub location: crate::DocumentLocation,
    pub document: Option<crate::DocumentEvidence>,
    pub code: Option<LocalCodeEvidence>,
    pub region: Option<u32>,
    /// Written attributes only; no resolved cross-artifact targets.
    pub attributes: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocalSyntaxId(pub u32);

/// Syntax nodes can own valid emitted children without emitting a fact, and
/// several framework facts can describe the same node. These identities are
/// therefore independent of LocalFact record IDs.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalSyntaxIdentity {
    pub id: LocalSyntaxId,
    pub span: LocalSpan,
    pub syntax_kind: String,
    pub region: Option<u32>,
}

/// All legacy native code metadata with typed local symbol/owner references.
/// The finalizer binds these syntax identities to public paths.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCodeEvidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprints: Option<crate::continuity::CodeFingerprints>,
    pub syntax_id: LocalSyntaxId,
    pub owner_id: Option<LocalSyntaxId>,
    pub qualified_name: String,
    pub scope: Vec<String>,
    pub signature: Option<String>,
    pub span: Option<crate::CodeSpan>,
    pub signature_span: Option<crate::CodeSpan>,
    pub parameters: Option<String>,
    pub return_type: Option<String>,
    pub modifiers: Vec<String>,
    pub documentation: Option<String>,
    pub target: Option<String>,
    pub basis: crate::EvidenceBasis,
    pub producer: String,
    pub producer_version: String,
    pub coverage: String,
    pub details_truncated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalChunk {
    pub span: LocalSpan,
    pub start_line: usize,
    pub end_line: usize,
    pub location: crate::DocumentLocation,
    pub region: Option<u32>,
}

/// Written semantic boundaries captured before optional evidence selection.
/// Coordinates remain local to the host or mapped region; no public path is stored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticBoundary {
    pub region: Option<u32>,
    pub span: LocalSpan,
    pub group: u32,
    pub context: Option<LocalSpan>,
    /// A leaf's own written signature, used only when its body needs splitting.
    pub split_context: Option<LocalSpan>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProjection {
    pub version: u32,
    pub boundaries: Vec<SemanticBoundary>,
}

impl Default for SemanticProjection {
    fn default() -> Self {
        Self {
            version: 1,
            boundaries: Vec::new(),
        }
    }
}

pub const MAX_SEMANTIC_BOUNDARIES: usize = 100_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalRelationshipTarget {
    Local { id: u32 },
    Written { spelling: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalRelationship {
    pub source: u32,
    pub target: LocalRelationshipTarget,
    pub kind: String,
    pub basis: crate::EvidenceBasis,
    pub producer: String,
    pub producer_version: String,
    pub span: LocalSpan,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalRegion {
    pub id: u32,
    pub parent: Option<u32>,
    pub profile: ExtractionProfile,
    pub text: String,
    pub source_map: crate::location::SourceMap,
    pub location: crate::location::RegionLocation,
}

pub use crate::continuity::ContinuityDescriptor;

/// Artifact-level provenance remains present even when extraction emits no facts.
/// Attributes preserve bounded producer options/coverage; immutable assets use
/// canonical digests, never display names as an identity substitute.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionProvenance {
    pub basis: crate::EvidenceBasis,
    pub producer: String,
    pub producer_version: String,
    pub attributes: BTreeMap<String, String>,
    pub asset_digests: BTreeMap<String, crate::identity::Sha256Digest>,
}

/// Neutral draft output. Never embeds public path-bearing CodeEvidence IDs.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionPayload {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub continuity_coverage:
        BTreeMap<crate::continuity::ContinuityFamily, crate::continuity::ContinuityCoverage>,
    pub contract_version: u32,
    pub extraction_method: String,
    /// Exact original parse status, including legacy serialized OCR provenance.
    pub parse_status: String,
    pub provenance: Vec<ExtractionProvenance>,
    pub text: String,
    pub facts: Vec<LocalFact>,
    pub syntax_identities: Vec<LocalSyntaxIdentity>,
    pub chunks: Vec<LocalChunk>,
    /// Required in serialized payloads. Obsolete indexes must be rebuilt.
    pub semantic_projection: SemanticProjection,
    pub regions: Vec<LocalRegion>,
    pub relationships: Vec<LocalRelationship>,
    pub continuity: Vec<ContinuityDescriptor>,
    pub diagnostics: Vec<String>,
    pub partial: bool,
}

impl ExtractionPayload {
    pub fn validate(&self) -> anyhow::Result<()> {
        self.validate_with_text_limit(MAX_TEXT_BYTES)
    }

    /// Adapters may impose a lower bound than the shared 32 MiB text ceiling.
    pub fn validate_with_text_limit(&self, text_limit: usize) -> anyhow::Result<()> {
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
                && self.continuity.len() <= crate::continuity::MAX_CONTINUITY_UNITS
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
        anyhow::ensure!(
            self.semantic_projection.version == 1
                && self.semantic_projection.boundaries.len() <= MAX_SEMANTIC_BOUNDARIES,
            "unsupported or excessive semantic projection; rebuild the index with airk index --full"
        );
        let mut prior_boundary = None;
        let mut groups = BTreeMap::new();
        for boundary in &self.semantic_projection.boundaries {
            let text = local_text(boundary.region)?;
            anyhow::ensure!(
                boundary.span.start < boundary.span.end,
                "empty semantic boundary"
            );
            for span in std::iter::once(&boundary.span)
                .chain(boundary.context.iter())
                .chain(boundary.split_context.iter())
            {
                crate::location::ByteSpan {
                    start: span.start,
                    end: span.end,
                }
                .validate(text)?;
            }
            if let Some((region, end)) = prior_boundary {
                anyhow::ensure!(
                    boundary.region > region
                        || (boundary.region == region && boundary.span.start >= end),
                    "semantic boundaries overlap or are unordered"
                );
            }
            prior_boundary = Some((boundary.region, boundary.span.end));
            let identity = (boundary.region, boundary.context.clone());
            if let Some(prior) = groups.insert(boundary.group, identity.clone()) {
                anyhow::ensure!(prior == identity, "semantic group context changed");
            }
        }
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
            crate::location::ByteSpan {
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
            crate::location::ByteSpan {
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
                    code.basis != crate::EvidenceBasis::Imported,
                    "imported code belongs to a bound producer domain"
                );
                for span in [&code.span, &code.signature_span].into_iter().flatten() {
                    crate::location::ByteSpan {
                        start: span.start_byte,
                        end: span.end_byte,
                    }
                    .validate(text)?;
                }
            }
        }
        for chunk in &self.chunks {
            crate::location::ByteSpan {
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
                relation.basis != crate::EvidenceBasis::Imported,
                "imported relationships require bound producer domain"
            );
            crate::location::ByteSpan {
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
            use crate::continuity::ContinuitySubject;
            anyhow::ensure!(
                descriptor.algorithm_version == crate::continuity::CONTINUITY_VERSION
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
            crate::location::ByteSpan {
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
            serde_json::to_vec(&self.continuity)?.len() <= crate::continuity::MAX_CONTINUITY_BYTES,
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

/// Kernel supplies a collection-scoped verified implementation to ingestion.
/// A hit must verify key/profile/content binding and payload integrity first.
pub trait ExtractionCache {
    fn lookup(
        &self,
        source_hash: &crate::identity::Sha256Digest,
        profile_id: &crate::identity::Sha256Digest,
    ) -> anyhow::Result<Option<ExtractionPayload>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_payload() -> ExtractionPayload {
        ExtractionPayload {
            continuity_coverage: BTreeMap::new(),
            semantic_projection: Default::default(),
            contract_version: 1,
            extraction_method: "ocr-tesseract".into(),
            parse_status: "{\"schema\":1,\"pages_with_text\":0,\"first_page\":2,\"last_page\":2}"
                .into(),
            provenance: vec![ExtractionProvenance {
                basis: crate::EvidenceBasis::Imported,
                producer: "tesseract".into(),
                producer_version: "fixture-v1".into(),
                attributes: BTreeMap::from([("language".into(), "eng".into())]),
                asset_digests: BTreeMap::from([(
                    "traineddata/eng".into(),
                    crate::identity::Sha256Digest::from_bytes([9; 32]),
                )]),
            }],
            text: String::new(),
            facts: vec![],
            syntax_identities: vec![],
            chunks: vec![],
            regions: vec![],
            relationships: vec![],
            continuity: vec![],
            diagnostics: vec![],
            partial: false,
        }
    }

    #[test]
    fn empty_ocr_provenance_and_parse_status_survive_roundtrip() -> anyhow::Result<()> {
        let original = empty_payload();
        original.validate()?;
        let bytes = serde_json::to_vec(&original)?;
        let mut decoded: ExtractionPayload = serde_json::from_slice(&bytes)?;
        decoded.validate()?;
        assert!(decoded.facts.is_empty());
        assert_eq!(decoded.extraction_method, original.extraction_method);
        assert_eq!(decoded.parse_status, original.parse_status);
        assert_eq!(serde_json::to_vec(&decoded)?, bytes);
        decoded.parse_status = "x".repeat(65_537);
        assert!(decoded.validate().is_err());
        Ok(())
    }

    #[test]
    fn shared_syntax_identity_and_nonemitted_owner_preserve_native_evidence() -> anyhow::Result<()>
    {
        let mut payload = empty_payload();
        payload.text = "class A { run() {} }".into();
        let child = LocalSpan { start: 10, end: 18 };
        payload.syntax_identities = vec![
            LocalSyntaxIdentity {
                id: LocalSyntaxId(1),
                span: LocalSpan {
                    start: 0,
                    end: payload.text.len(),
                },
                syntax_kind: "class_declaration".into(),
                region: None,
            },
            LocalSyntaxIdentity {
                id: LocalSyntaxId(2),
                span: child.clone(),
                syntax_kind: "method_definition".into(),
                region: None,
            },
        ];
        let code = LocalCodeEvidence {
            fingerprints: None,
            syntax_id: LocalSyntaxId(2),
            owner_id: Some(LocalSyntaxId(1)),
            qualified_name: "A::run".into(),
            scope: vec!["A".into()],
            signature: Some("run()".into()),
            span: Some(crate::CodeSpan {
                start_byte: child.start,
                end_byte: child.end,
            }),
            signature_span: Some(crate::CodeSpan {
                start_byte: 10,
                end_byte: 15,
            }),
            parameters: Some("()".into()),
            return_type: None,
            modifiers: vec![],
            documentation: None,
            target: None,
            basis: crate::EvidenceBasis::Syntax,
            producer: "airk-tree-sitter/javascript".into(),
            producer_version: "native-facts/4".into(),
            coverage: "Selected node".into(),
            details_truncated: false,
        };
        payload.facts.push(LocalFact {
            id: 10,
            owner: None,
            kind: "definition".into(),
            name: "run".into(),
            span: child,
            syntax_kind: "method_definition".into(),
            start_line: 1,
            end_line: 1,
            location: crate::DocumentLocation::Lines { start: 1, end: 1 },
            document: None,
            code: Some(code),
            region: None,
            attributes: BTreeMap::new(),
        });
        let mut cue = payload.facts[0].clone();
        cue.id = 11;
        cue.kind = "test_declaration".into();
        cue.code.as_mut().unwrap().basis = crate::EvidenceBasis::StructuralRule;
        payload.facts.push(cue);
        payload.validate()?;
        assert!(
            payload
                .facts
                .iter()
                .all(|f| f.code.as_ref().unwrap().syntax_id == LocalSyntaxId(2))
        );
        assert!(
            !payload
                .facts
                .iter()
                .any(|f| f.syntax_kind == "class_declaration")
        );
        let serialized = serde_json::to_vec(&payload)?;
        let decoded: ExtractionPayload = serde_json::from_slice(&serialized)?;
        decoded.validate()?;
        assert_eq!(serde_json::to_vec(&decoded)?, serialized);
        payload.syntax_identities.remove(0);
        assert!(payload.validate().is_err());
        payload
            .syntax_identities
            .push(payload.syntax_identities[0].clone());
        assert!(payload.validate().is_err());
        Ok(())
    }

    #[test]
    fn child_profile_versions_and_configuration_hashes_fail_closed() -> anyhow::Result<()> {
        let mut payload = empty_payload();
        payload.text = "abc".into();
        let profile = ExtractionProfile {
            contract_version: 1,
            adapter_id: "text".into(),
            adapter_version: "fixture-v1".into(),
            language: "text".into(),
            dialect: None,
            grammar_identity: None,
            mapping_version: 1,
            chunk_version: 1,
            options: BTreeMap::new(),
            configuration_inputs: BTreeMap::from([(
                "configuration".into(),
                crate::identity::Sha256Digest::from_bytes([1; 32]),
            )]),
        };
        let encoded = serde_json::to_string(&profile)?;
        let malformed = encoded.replace(
            &crate::identity::Sha256Digest::from_bytes([1; 32]).to_string(),
            "not-a-canonical-digest",
        );
        assert!(serde_json::from_str::<ExtractionProfile>(&malformed).is_err());
        let span = crate::location::ByteSpan { start: 0, end: 3 };
        payload.regions.push(LocalRegion {
            id: 1,
            parent: None,
            profile,
            text: "abc".into(),
            source_map: crate::location::SourceMap {
                version: 1,
                segments: vec![crate::location::MapSegment {
                    local: span,
                    mapping: crate::location::Mapping::Verbatim { host: span },
                }],
            },
            location: crate::location::RegionLocation::Embedded {
                region_id: "child".into(),
                local_span: span,
                original_spans: vec![span],
            },
        });
        payload.validate()?;
        payload.regions[0].profile.contract_version = 99;
        assert!(payload.validate().is_err());
        Ok(())
    }

    #[test]
    fn semantic_projection_is_required_and_rejects_invalid_bindings() -> anyhow::Result<()> {
        let mut payload = empty_payload();
        payload.text = "é body".into();
        let mut old = serde_json::to_value(&payload)?;
        old.as_object_mut().unwrap().remove("semantic_projection");
        assert!(serde_json::from_value::<ExtractionPayload>(old).is_err());
        payload.semantic_projection.boundaries = vec![SemanticBoundary {
            region: None,
            span: LocalSpan {
                start: 0,
                end: payload.text.len(),
            },
            group: 0,
            context: None,
            split_context: None,
        }];
        payload.validate()?;
        payload.semantic_projection.boundaries[0].span.start = 1;
        assert!(payload.validate().is_err());
        payload.semantic_projection.boundaries[0].span.start = 0;
        let mut overlapping = payload.semantic_projection.boundaries[0].clone();
        overlapping.span.start = 2;
        payload.semantic_projection.boundaries.push(overlapping);
        assert!(payload.validate().is_err());
        payload.semantic_projection.boundaries.pop();
        payload.semantic_projection.boundaries[0].region = Some(99);
        assert!(payload.validate().is_err());
        payload.semantic_projection.boundaries[0].region = None;
        payload.semantic_projection.version = 99;
        assert!(payload.validate().is_err());
        Ok(())
    }

    #[test]
    fn payload_rejects_invalid_utf8_spans_and_owner_cycles() {
        let mut p = ExtractionPayload {
            continuity_coverage: BTreeMap::new(),
            semantic_projection: Default::default(),
            contract_version: 1,
            extraction_method: "fixture".into(),
            parse_status: "syntax".into(),
            provenance: vec![],
            text: "é".into(),
            facts: vec![],
            syntax_identities: vec![],
            chunks: vec![LocalChunk {
                span: LocalSpan { start: 0, end: 1 },
                start_line: 1,
                end_line: 1,
                location: crate::DocumentLocation::Lines { start: 1, end: 1 },
                region: None,
            }],
            regions: vec![],
            relationships: vec![],
            continuity: vec![],
            diagnostics: vec![],
            partial: false,
        };
        assert!(p.validate().is_err());
        p.chunks[0].span.end = 2;
        assert!(p.validate().is_ok());
        p.facts.push(LocalFact {
            id: 1,
            owner: Some(1),
            kind: "definition".into(),
            name: "x".into(),
            span: LocalSpan { start: 0, end: 2 },
            syntax_kind: "function".into(),
            start_line: 1,
            end_line: 1,
            location: crate::DocumentLocation::Lines { start: 1, end: 1 },
            document: None,
            code: None,
            region: None,
            attributes: BTreeMap::new(),
        });
        assert!(p.validate().is_err());
        p.facts[0].owner = None;
        p.facts[0].location = crate::DocumentLocation::WorksheetRow {
            sheet: "Receipts".into(),
            row: 4,
        };
        p.facts[0].document = Some(crate::DocumentEvidence {
            parser_version: "baseline".into(),
            table: Some(crate::TableEvidence {
                row: 4,
                sheet: Some("Receipts".into()),
                cells: vec![crate::TableCellEvidence {
                    column: 2,
                    address: Some("B4".into()),
                    value: "12".into(),
                    formula: Some("SUM(B1:B3)".into()),
                    cached_value: true,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        });
        let encoded = serde_json::to_vec(&p).unwrap();
        let decoded: ExtractionPayload = serde_json::from_slice(&encoded).unwrap();
        decoded.validate().unwrap();
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), encoded);
    }
}
