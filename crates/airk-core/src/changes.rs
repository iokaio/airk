// SPDX-License-Identifier: Apache-2.0
//! Bounded observed-change contracts; no live-source or ancestry inference.
use crate::identity::{
    ArtifactId, DocumentKey, Incarnation, RevisionToken, Sha256Digest, SymbolKey,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CHANGE_CONTRACT_VERSION: u32 = 1;
pub const LINEAGE_CONTRACT_VERSION: u32 = 2;
pub const MAX_REMEMBERED_SELECTORS: usize = 64;
pub const MAX_SELECTOR_BYTES: usize = 32 * 1024;
pub const MAX_CHANGE_PATH_BYTES: usize = 4096;
pub const MAX_LINEAGE_RECORDS: usize = 10_000;
pub const MAX_LINEAGE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_LINEAGE_AGE_SECONDS: u64 = 90 * 24 * 60 * 60;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChangesRequest {
    pub since: Option<RevisionToken>,
    pub selectors: Vec<RememberedSelector>,
    pub path_prefix: String,
    pub include_symbols: bool,
    pub include_documents: bool,
    pub include_suggestions: bool,
    pub limit: usize,
    pub max_bytes: usize,
    pub cursor: Option<String>,
}

impl Default for ChangesRequest {
    fn default() -> Self {
        Self {
            since: None,
            selectors: vec![],
            path_prefix: String::new(),
            include_symbols: false,
            include_documents: false,
            include_suggestions: false,
            limit: 10,
            max_bytes: 8192,
            cursor: None,
        }
    }
}

impl ChangesRequest {
    pub fn validate(&self) -> Result<()> {
        validate_selectors(self.since.as_ref(), &self.selectors, &self.path_prefix)?;
        ensure!(
            (1..=100).contains(&self.limit),
            "change limit must be 1..100"
        );
        ensure!(
            (1..=65_536).contains(&self.max_bytes),
            "change max_bytes must be 1..65536"
        );
        ensure!(
            self.cursor
                .as_ref()
                .is_none_or(|cursor| cursor.len() <= 4096),
            "change cursor exceeds 4096 bytes"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineStatus {
    NotRequested,
    Verified,
    BaselineUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangesStatus {
    Ready,
    CursorExpired,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoalescedChange {
    pub subject: ChangeSubject,
    pub first_generation: u64,
    pub last: ChangePosition,
    pub observations: usize,
    pub before: Option<ChangeEndpoint>,
    pub after: Option<ChangeEndpoint>,
    pub before_detail: Option<ChangeDetail>,
    pub after_detail: Option<ChangeDetail>,
    pub dimensions: Vec<ChangeDimension>,
    pub causes: Vec<ChangeCause>,
    pub basis: ContinuityBasis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationOutcome {
    Current,
    Unchanged,
    Modified,
    MoveCandidate,
    RemovedFromIndex,
    PathReused,
    Ambiguous,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceValidation {
    pub ordinal: usize,
    pub selector: RememberedSelector,
    pub outcome: ValidationOutcome,
    pub artifact_id: Option<ArtifactId>,
    pub current: Option<FileState>,
    pub requested_path: Option<String>,
    pub resolved_path: Option<String>,
    pub basis: ContinuityBasis,
    pub validated_dimensions: Vec<ChangeDimension>,
    pub candidates: Vec<ChangeEndpoint>,
    pub candidates_truncated: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessState {
    Unregistered,
    Unavailable,
    Idle,
    Pending,
    Running,
    Paused,
    Blocked,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreshnessObservation {
    pub observed_unix_ms: u64,
    /// Latest observed service source attempt; distinct from an index token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_job: Option<Sha256Digest>,
    pub state: FreshnessState,
    pub requested_watermark: Option<u64>,
    pub completed_watermark: Option<u64>,
    pub running_watermark: Option<u64>,
    pub last_success_unix_ms: Option<u64>,
    pub error: Option<String>,
    pub live_source_verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_inventory_unix_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_index: Option<IndexTimingObservation>,
}

/// Latest completed source pass, independent of live source freshness.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IndexTimingObservation {
    #[serde(default)]
    pub completed_unix_ms: u64,
    #[serde(default)]
    pub path_scoped: bool,
    #[serde(default)]
    pub scan_extract_us: u64,
    #[serde(default)]
    pub publish_us: u64,
    pub first_ready_us: Option<u64>,
    pub fully_ready_us: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangesResponse {
    pub status: ChangesStatus,
    pub publication: crate::evidence::RevisionEnvelope,
    pub from: Option<RevisionToken>,
    pub to: RevisionToken,
    pub current: RevisionToken,
    pub baseline_status: BaselineStatus,
    pub coverage: Option<ChangeCoverage>,
    pub changes: Vec<CoalescedChange>,
    pub invalidations: Vec<CoalescedChange>,
    pub validations: Vec<EvidenceValidation>,
    pub suggestions: Vec<ChangeSuggestion>,
    pub suggestion_coverage: Option<SuggestionCoverage>,
    pub freshness: FreshnessObservation,
    /// Exact sum of serialized change/invalidation/validation records.
    pub record_bytes: usize,
    pub truncated: bool,
    pub cursor: Option<String>,
}

/// Optional hints only: never identity, resolved movement, or read redirects.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChangeSuggestion {
    File {
        before: ChangeEndpoint,
        after: ChangeEndpoint,
        score_basis_points: u32,
        shared_shingles: usize,
        union_shingles: usize,
    },
    Directory {
        root: String,
        /// Root-relative directory paths; children retain collection citation paths.
        before: String,
        after: String,
        children: Vec<(String, String)>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionStatus {
    Complete,
    Partial,
    Unavailable,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestionCoverage {
    pub version: u32,
    pub status: SuggestionStatus,
    pub baseline_representation: Option<String>,
    pub compared_pairs: usize,
    pub examined_files: usize,
    pub examined_text_bytes: usize,
    pub reason: Option<String>,
}

/// Count events, resolver records and ancestry together, including detail records.
/// The store supplies actual encoded bytes, not an estimated payload size.
pub fn validate_lineage_budget(total_records: usize, encoded_bytes: usize) -> Result<()> {
    ensure!(
        total_records <= MAX_LINEAGE_RECORDS,
        "lineage record budget exceeded"
    );
    ensure!(
        encoded_bytes <= MAX_LINEAGE_BYTES,
        "lineage byte budget exceeded"
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileStamps {
    pub first_generation: u64,
    pub path_generation: u64,
    pub content_generation: u64,
    pub evidence_generation: u64,
}

impl FileStamps {
    pub fn validate(&self, generation: u64) -> Result<()> {
        ensure!(
            [
                self.path_generation,
                self.content_generation,
                self.evidence_generation
            ]
            .iter()
            .all(|value| *value >= self.first_generation && *value <= generation),
            "invalid file generation stamps"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeDimension {
    Path,
    Content,
    Evidence,
    CodeName,
    CodeSignature,
    CodeBody,
    Documentation,
    Location,
    Statement,
    Quantity,
    Date,
    Modality,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeCause {
    Added,
    Modified,
    DeletedObserved,
    RemovedUnknown,
    Ignored,
    Unsupported,
    Oversize,
    ExtractionOmitted,
    AliasRemoved,
    AliasRebound,
    ProfileChanged,
    ProviderInvalidated,
    ModelChanged,
    MetadataMaintenance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuityBasis {
    SamePathObservation,
    UniqueCompatibleContent,
    EquivalentContentCandidate,
    SimilarityCandidate,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeEndpoint {
    /// Original collection-relative citation, including the then-bound alias.
    /// Absent in lineage contract 1; physical root/path remain authoritative.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citation_path: Option<String>,
    pub root: String,
    pub path: String,
    pub source_hash: Sha256Digest,
    pub profile_id: Sha256Digest,
}

impl ChangeEndpoint {
    fn validate(&self) -> Result<()> {
        validate_root(&self.root)?;
        if let Some(path) = &self.citation_path {
            validate_path(path, false)?;
            ensure!(
                path == &self.path
                    || path
                        .strip_prefix('@')
                        .and_then(|path| path.split_once('/'))
                        .is_some_and(|(_, relative)| relative == self.path),
                "citation path differs from endpoint location"
            );
        }
        validate_path(&self.path, false)
    }
}

/// One bounded record, without excerpts or unbounded child events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChangeSubject {
    Global {
        domain: String,
    },
    Artifact {
        artifact_id: ArtifactId,
    },
    Symbol {
        artifact_id: ArtifactId,
        symbol_key: SymbolKey,
    },
    Document {
        artifact_id: ArtifactId,
        document_key: DocumentKey,
    },
}

/// Full dimension fingerprints and intrinsic location, never source excerpts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeDetail {
    pub digests: BTreeMap<ChangeDimension, Sha256Digest>,
    pub location: Option<crate::DocumentLocation>,
    /// Binding to complete embedded/notebook map and location metadata, if applicable.
    pub region_location_digest: Option<Sha256Digest>,
    /// Complete extraction/producer-domain provenance identity, not a display label.
    pub provenance_digest: Sha256Digest,
}

impl ChangeDetail {
    fn validate(&self) -> Result<()> {
        ensure!(self.digests.len() <= 14, "too many detail dimensions");
        if let Some(location) = &self.location {
            use crate::DocumentLocation;
            let valid = match location {
                DocumentLocation::Mapped { region } => region.validate_shape().is_ok(),
                DocumentLocation::Lines { start, end } => *start > 0 && end >= start,
                DocumentLocation::Page { number }
                | DocumentLocation::Block { number }
                | DocumentLocation::Slide { number } => *number > 0,
                DocumentLocation::TableRow { table, row } => *table > 0 && *row > 0,
                DocumentLocation::WorksheetRow { sheet, row } => {
                    !sheet.is_empty() && sheet.len() <= 4096 && *row > 0
                }
            };
            ensure!(valid, "invalid intrinsic change location");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeEvent {
    pub generation: u64,
    pub event_id: u64,
    pub subject: ChangeSubject,
    pub before: Option<ChangeEndpoint>,
    pub after: Option<ChangeEndpoint>,
    pub before_detail: Option<ChangeDetail>,
    pub after_detail: Option<ChangeDetail>,
    pub dimensions: Vec<ChangeDimension>,
    pub cause: ChangeCause,
    pub basis: ContinuityBasis,
}

impl ChangeEvent {
    pub fn validate(&self, target_generation: u64) -> Result<()> {
        ensure!(self.generation <= target_generation, "future change event");
        ensure!(self.dimensions.len() <= 14, "too many change dimensions");
        for (index, dimension) in self.dimensions.iter().enumerate() {
            ensure!(
                !self.dimensions[..index].contains(dimension),
                "duplicate change dimension"
            );
        }
        for endpoint in [&self.before, &self.after].into_iter().flatten() {
            endpoint.validate()?;
        }
        for detail in [&self.before_detail, &self.after_detail]
            .into_iter()
            .flatten()
        {
            detail.validate()?;
        }
        if let ChangeSubject::Global { domain } = &self.subject {
            ensure!(
                !domain.is_empty()
                    && domain.len() <= 128
                    && domain
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._/-".contains(&b)),
                "invalid global change domain"
            );
            ensure!(
                self.before.is_none() && self.after.is_none(),
                "global change cannot impersonate a file endpoint"
            );
        }
        Ok(())
    }
}

/// Current entry deliberately has no self revision or manifest digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationObservation {
    pub generation: u64,
    pub parent: Option<RevisionToken>,
    pub observed_unix_seconds: u64,
    pub reason: PublicationReason,
}

/// Completed older publication, retaining the observation timestamp for age expiry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedPublicationHeader {
    pub token: RevisionToken,
    pub parent: Option<RevisionToken>,
    pub observed_unix_seconds: u64,
    pub reason: PublicationReason,
}

impl RetainedPublicationHeader {
    /// A backwards clock does not manufacture age or change publication order.
    pub fn observed_age_seconds(&self, now_unix_seconds: u64) -> u64 {
        now_unix_seconds.saturating_sub(self.observed_unix_seconds)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationReason {
    Initialization,
    Index,
    FullReindex,
    ScipImport,
    Lsp,
    Ctags,
    Ocr,
    PolicyChange,
    LineageCompaction,
    LineagePurge,
    Migration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageReason {
    BaselineUnavailable,
    Expired,
    Overflow,
    Unsupported,
    Omitted,
    Purged,
}

/// `complete_after` is an exclusive lower floor: changes in (floor,target] are covered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageFloor {
    pub complete_after: u64,
    pub reason: Option<CoverageReason>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeCoverage {
    pub files: CoverageFloor,
    pub code: CoverageFloor,
    pub documents: CoverageFloor,
    pub ancestry: CoverageFloor,
    pub resolution: CoverageFloor,
}

/// Persisted file binding; payload is path-free, evidence is bound to this root/path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileState {
    pub artifact_id: ArtifactId,
    pub stamps: FileStamps,
    pub source_hash: Sha256Digest,
    pub profile_id: Sha256Digest,
    pub payload_digest: Sha256Digest,
    pub evidence_digest: Sha256Digest,
    pub root: String,
    pub path: String,
}

impl FileState {
    pub fn validate(&self, generation: u64) -> Result<()> {
        self.stamps.validate(generation)?;
        validate_root(&self.root)?;
        validate_path(&self.path, false)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionState {
    ExactMove,
    DeletedObserved,
    RemovedFromIndex,
    Ambiguous,
    Unknown,
}

/// Immutable projection for one remembered endpoint; candidates are separate bounded records.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolverRecord {
    pub artifact_id: ArtifactId,
    pub observed_generation: u64,
    pub from: ChangeEndpoint,
    pub to: Option<ChangeEndpoint>,
    pub state: ResolutionState,
    pub basis: ContinuityBasis,
}

/// Does not embed its enclosing revision/manifest digest; ancestry contains older tokens only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineageEnvelope {
    pub version: u32,
    pub incarnation: Incarnation,
    pub observation: PublicationObservation,
    pub coverage: ChangeCoverage,
    pub ancestry: Vec<RetainedPublicationHeader>,
    pub events: Vec<ChangeEvent>,
    pub resolvers: Vec<ResolverRecord>,
}

impl LineageEnvelope {
    /// Structural admission, not a proof that persisted ancestors or roots are authentic.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=LINEAGE_CONTRACT_VERSION).contains(&self.version),
            "unsupported lineage contract"
        );
        let records = self
            .ancestry
            .len()
            .checked_add(self.events.len())
            .and_then(|count| count.checked_add(self.resolvers.len()))
            .and_then(|count| count.checked_add(6))
            .ok_or_else(|| anyhow::anyhow!("lineage record count overflow"))?;
        validate_lineage_budget(records, 0)?;
        validate_lineage_budget(records, serde_json::to_vec(self)?.len())?;
        let generation = self.observation.generation;
        if let Some(parent) = &self.observation.parent {
            ensure!(
                parent.incarnation == self.incarnation && parent.next_generation()? == generation,
                "invalid publication parent"
            );
        }
        for floor in [
            &self.coverage.files,
            &self.coverage.code,
            &self.coverage.documents,
            &self.coverage.ancestry,
            &self.coverage.resolution,
        ] {
            ensure!(floor.complete_after <= generation, "future coverage floor");
        }
        let mut previous = None;
        let mut headers = BTreeMap::new();
        for ancestor in &self.ancestry {
            ensure!(
                ancestor.token.incarnation == self.incarnation
                    && ancestor.token.generation < generation
                    && previous.is_none_or(|value| ancestor.token.generation > value),
                "invalid ancestry order/scope"
            );
            previous = Some(ancestor.token.generation);
            headers.insert(ancestor.token.generation, ancestor.token);
        }
        for (child_generation, parent) in self
            .ancestry
            .iter()
            .map(|header| (header.token.generation, header.parent.as_ref()))
            .chain(std::iter::once((
                generation,
                self.observation.parent.as_ref(),
            )))
        {
            if let Some(parent) = parent {
                ensure!(
                    parent.incarnation == self.incarnation
                        && parent.next_generation()? == child_generation,
                    "invalid retained publication parent"
                );
                if let Some(retained) = headers.get(&parent.generation) {
                    ensure!(parent == retained, "contradictory retained parent token");
                }
            } else {
                ensure!(
                    headers.range(..child_generation).next().is_none(),
                    "parentless publication has earlier retained ancestry"
                );
            }
        }
        let mut previous = None;
        for event in &self.events {
            event.validate(generation)?;
            ensure!(
                event.generation == generation || headers.contains_key(&event.generation),
                "event lacks retained observation header"
            );
            let key = (event.generation, event.event_id);
            ensure!(
                previous.is_none_or(|value| key > value),
                "invalid event order"
            );
            previous = Some(key);
        }
        for resolver in &self.resolvers {
            ensure!(
                resolver.observed_generation <= generation,
                "future resolver"
            );
            resolver.from.validate()?;
            if let Some(to) = &resolver.to {
                to.validate()?;
                ensure!(
                    resolver.from.source_hash == to.source_hash
                        && resolver.from.profile_id == to.profile_id
                        && (resolver.from.path != to.path || resolver.from.root != to.root),
                    "invalid exact move endpoints"
                );
            }
            ensure!(
                (resolver.state == ResolutionState::ExactMove) == resolver.to.is_some(),
                "invalid resolver target state"
            );
            if resolver.state == ResolutionState::ExactMove {
                ensure!(
                    resolver.basis == ContinuityBasis::UniqueCompatibleContent,
                    "unqualified automatic move"
                );
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RememberedSelector {
    Path {
        path: String,
    },
    Citation {
        path: String,
        source_hash: Sha256Digest,
    },
    Artifact {
        artifact_id: ArtifactId,
    },
    Symbol {
        symbol_key: SymbolKey,
    },
    Document {
        document_key: DocumentKey,
    },
}

fn validate_root(root: &str) -> Result<()> {
    ensure!(
        !root.is_empty() && root.len() <= MAX_CHANGE_PATH_BYTES && !root.contains('\0'),
        "invalid root binding"
    );
    Ok(())
}

fn validate_path(path: &str, empty_allowed: bool) -> Result<()> {
    ensure!(
        path.len() <= MAX_CHANGE_PATH_BYTES && (empty_allowed || !path.is_empty()),
        "change path exceeds bounds or is empty"
    );
    ensure!(
        !path.starts_with('/')
            && !path.contains(['\\', '\0', ':'])
            && !path.split('/').any(|part| part == "." || part == ".."),
        "unsafe change path"
    );
    Ok(())
}

/// Bounds apply to the complete serialized selector array, including framing.
pub fn validate_selectors(
    since: Option<&RevisionToken>,
    selectors: &[RememberedSelector],
    path_prefix: &str,
) -> Result<()> {
    ensure!(
        selectors.len() <= MAX_REMEMBERED_SELECTORS,
        "too many remembered selectors"
    );
    ensure!(
        since.is_some() || !selectors.is_empty(),
        "since or remembered selector required"
    );
    validate_path(path_prefix, true)?;
    for selector in selectors {
        if let RememberedSelector::Path { path } | RememberedSelector::Citation { path, .. } =
            selector
        {
            validate_path(path, false)?;
        }
    }
    ensure!(
        serde_json::to_vec(selectors)?.len() <= MAX_SELECTOR_BYTES,
        "serialized remembered selectors exceed 32 KiB"
    );
    Ok(())
}

/// Internal cursor payload. Encoding/authentication and retained-target lookup belong to kernel.
/// Changes are emitted first; validation results follow in request-selector order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorStage {
    Changes,
    Validations,
    Suggestions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangePosition {
    pub generation: u64,
    pub event_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeCursor {
    pub version: u32,
    pub target: RevisionToken,
    pub baseline: Option<RevisionToken>,
    /// Binds filters, selectors, detail flags and other query-shaping inputs.
    pub request_digest: Sha256Digest,
    pub stage: CursorStage,
    /// Exclusive last emitted coalesced change key; None starts the list.
    pub after_change: Option<ChangePosition>,
    /// Next selector ordinal. One bounded validation record per input selector.
    pub next_validation: usize,
    #[serde(default)]
    pub next_suggestion: usize,
    #[serde(default)]
    pub suggestion_digest: Option<Sha256Digest>,
}

impl ChangeCursor {
    /// Does not prove retention, receipt validity or baseline ancestry.
    pub fn validate_binding(
        &self,
        target: &RevisionToken,
        baseline: Option<&RevisionToken>,
        request_digest: &Sha256Digest,
        selector_count: usize,
    ) -> Result<()> {
        ensure!(
            self.version == CHANGE_CONTRACT_VERSION,
            "unsupported change cursor"
        );
        ensure!(
            &self.target == target
                && self.baseline.as_ref() == baseline
                && &self.request_digest == request_digest,
            "change cursor binding mismatch"
        );
        ensure!(
            self.after_change
                .is_none_or(|position| position.generation <= target.generation),
            "future cursor position"
        );
        ensure!(
            selector_count <= MAX_REMEMBERED_SELECTORS && self.next_validation <= selector_count,
            "invalid validation cursor position"
        );
        ensure!(
            self.stage != CursorStage::Changes || self.next_validation == 0,
            "validation position before change list completion"
        );
        ensure!(
            self.stage != CursorStage::Validations || self.next_validation < selector_count,
            "completed validation list must not emit a cursor"
        );
        ensure!(
            self.next_suggestion <= 128,
            "invalid suggestion cursor position"
        );
        ensure!(
            self.stage == CursorStage::Suggestions || self.next_suggestion == 0,
            "suggestion position before prior lists completed"
        );
        ensure!(
            self.stage != CursorStage::Suggestions || self.next_validation == selector_count,
            "suggestion cursor skipped validations"
        );
        if let Some(baseline) = baseline {
            baseline.validate_against(target)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Incarnation;

    #[test]
    fn selectors_bound_count_bytes_paths_and_empty_requests() {
        assert!(validate_selectors(None, &[], "").is_err());
        let selector = RememberedSelector::Path {
            path: "@spec/requirements.md".into(),
        };
        assert!(validate_selectors(None, std::slice::from_ref(&selector), "@spec/").is_ok());
        assert!(validate_selectors(None, &vec![selector; 65], "").is_err());
        for path in ["../secret", "/absolute", "C:/source", "a\\b", "a/../b", ""] {
            assert!(
                validate_selectors(None, &[RememberedSelector::Path { path: path.into() }], "")
                    .is_err()
            );
        }
        let large = RememberedSelector::Path {
            path: "x".repeat(4096),
        };
        assert!(validate_selectors(None, &vec![large; 9], "").is_err());
        let unicode = RememberedSelector::Path {
            path: "é".repeat(2049),
        };
        assert!(validate_selectors(None, &[unicode], "").is_err());
        // JSON escaping consumes bytes beyond the literal path's UTF-8 bytes.
        let escaped = RememberedSelector::Path {
            path: "\"".repeat(4096),
        };
        assert!(validate_selectors(None, &vec![escaped; 4], "").is_err());
    }

    #[test]
    fn stamps_are_independent_and_bounded_by_observation() {
        let mut stamps = FileStamps {
            first_generation: 2,
            path_generation: 5,
            content_generation: 3,
            evidence_generation: 4,
        };
        assert!(stamps.validate(5).is_ok());
        assert!(stamps.validate(4).is_err());
        stamps.content_generation = 1;
        assert!(stamps.validate(5).is_err());
    }

    #[test]
    fn lineage_admission_counts_all_records_and_actual_bytes() {
        assert!(validate_lineage_budget(MAX_LINEAGE_RECORDS, MAX_LINEAGE_BYTES).is_ok());
        assert!(validate_lineage_budget(MAX_LINEAGE_RECORDS + 1, 0).is_err());
        assert!(validate_lineage_budget(0, MAX_LINEAGE_BYTES + 1).is_err());
    }

    #[test]
    fn lineage_envelope_rejects_foreign_parents_and_inconsistent_moves() -> Result<()> {
        let incarnation = Incarnation::from_bytes([1; 16]);
        let digest = Sha256Digest::from_bytes([2; 32]);
        let floor = CoverageFloor {
            complete_after: 1,
            reason: Some(CoverageReason::BaselineUnavailable),
        };
        let parent = RevisionToken {
            incarnation,
            generation: 1,
            revision: digest,
        };
        let mut lineage = LineageEnvelope {
            version: 1,
            incarnation,
            observation: PublicationObservation {
                generation: 2,
                parent: Some(parent),
                observed_unix_seconds: 1,
                reason: PublicationReason::Index,
            },
            coverage: ChangeCoverage {
                files: floor.clone(),
                code: floor.clone(),
                documents: floor.clone(),
                ancestry: floor.clone(),
                resolution: floor,
            },
            ancestry: vec![RetainedPublicationHeader {
                token: parent,
                parent: None,
                observed_unix_seconds: 10,
                reason: PublicationReason::Migration,
            }],
            events: vec![],
            resolvers: vec![],
        };
        lineage.validate()?;
        let artifact_id = ArtifactId::from_bytes([3; 16]);
        let subjects = [
            ChangeSubject::Global {
                domain: "provider/scip".into(),
            },
            ChangeSubject::Artifact { artifact_id },
            ChangeSubject::Symbol {
                artifact_id,
                symbol_key: SymbolKey::from_bytes([4; 16]),
            },
            ChangeSubject::Document {
                artifact_id,
                document_key: DocumentKey::from_bytes([5; 16]),
            },
        ];
        for (index, subject) in subjects.into_iter().enumerate() {
            lineage.events.push(ChangeEvent {
                generation: 1,
                event_id: index as u64,
                subject,
                before: None,
                after: None,
                before_detail: None,
                after_detail: Some(ChangeDetail {
                    digests: BTreeMap::from([(ChangeDimension::Evidence, digest)]),
                    location: None,
                    region_location_digest: None,
                    provenance_digest: digest,
                }),
                dimensions: vec![ChangeDimension::Evidence],
                cause: ChangeCause::Modified,
                basis: ContinuityBasis::Unknown,
            });
        }
        lineage.validate()?;
        let encoded = serde_json::to_vec(&lineage)?;
        assert_eq!(
            serde_json::from_slice::<LineageEnvelope>(&encoded)?,
            lineage
        );
        let decoded: LineageEnvelope = serde_json::from_slice(&encoded)?;
        assert_eq!(decoded.ancestry[0].observed_age_seconds(25), 15);
        assert_eq!(decoded.ancestry[0].observed_age_seconds(1), 0);
        lineage.observation.parent.as_mut().unwrap().revision = Sha256Digest::from_bytes([8; 32]);
        assert!(lineage.validate().is_err());
        lineage.observation.parent = Some(parent);
        lineage.events[0].generation = 0;
        assert!(lineage.validate().is_err());
        lineage.events[0].generation = 1;
        lineage.observation.parent.as_mut().unwrap().incarnation = Incarnation::from_bytes([9; 16]);
        assert!(lineage.validate().is_err());
        lineage.observation.parent = Some(parent);
        let from = ChangeEndpoint {
            citation_path: Some("old.rs".into()),
            root: "/fixture".into(),
            path: "old.rs".into(),
            source_hash: digest,
            profile_id: digest,
        };
        let to = ChangeEndpoint {
            citation_path: Some("new.rs".into()),
            path: "new.rs".into(),
            ..from.clone()
        };
        lineage.resolvers.push(ResolverRecord {
            artifact_id: ArtifactId::from_bytes([3; 16]),
            observed_generation: 2,
            from,
            to: Some(to),
            state: ResolutionState::ExactMove,
            basis: ContinuityBasis::UniqueCompatibleContent,
        });
        lineage.validate()?;
        lineage.resolvers[0].to.as_mut().unwrap().source_hash = Sha256Digest::from_bytes([4; 32]);
        assert!(lineage.validate().is_err());
        Ok(())
    }

    #[test]
    fn cursor_never_silently_changes_target_or_filters() {
        let target = RevisionToken {
            incarnation: Incarnation::from_bytes([1; 16]),
            generation: 3,
            revision: Sha256Digest::from_bytes([2; 32]),
        };
        let digest = Sha256Digest::from_bytes([3; 32]);
        let cursor = ChangeCursor {
            version: 1,
            target,
            baseline: None,
            request_digest: digest,
            stage: CursorStage::Changes,
            after_change: Some(ChangePosition {
                generation: 2,
                event_id: 9,
            }),
            next_validation: 0,
            next_suggestion: 0,
            suggestion_digest: None,
        };
        assert!(cursor.validate_binding(&target, None, &digest, 2).is_ok());
        let newer = RevisionToken {
            generation: 4,
            ..target
        };
        assert!(cursor.validate_binding(&newer, None, &digest, 2).is_err());
        assert!(
            cursor
                .validate_binding(&target, None, &Sha256Digest::from_bytes([0; 32]), 2)
                .is_err()
        );
        let mut validations = ChangeCursor {
            stage: CursorStage::Validations,
            next_validation: 1,
            ..cursor.clone()
        };
        assert!(
            validations
                .validate_binding(&target, None, &digest, 2)
                .is_ok()
        );
        validations.next_validation = 2;
        assert!(
            validations
                .validate_binding(&target, None, &digest, 2)
                .is_err()
        );
        validations.stage = CursorStage::Changes;
        assert!(
            validations
                .validate_binding(&target, None, &digest, 2)
                .is_err()
        );
    }

    #[test]
    fn endpoint_migration_preserves_absent_and_historic_citations() -> Result<()> {
        let legacy = serde_json::json!({
            "root": "/original", "path": "notes.txt",
            "source_hash": "01".repeat(32), "profile_id": "02".repeat(32)
        });
        let mut endpoint: ChangeEndpoint = serde_json::from_value(legacy.clone())?;
        endpoint.validate()?;
        assert!(endpoint.citation_path.is_none());
        assert_eq!(serde_json::to_value(&endpoint)?, legacy);
        endpoint.citation_path = Some("@removed/notes.txt".into());
        endpoint.validate()?;
        let decoded: ChangeEndpoint = serde_json::from_slice(&serde_json::to_vec(&endpoint)?)?;
        assert_eq!(decoded, endpoint);
        endpoint.citation_path = Some("@removed/other.txt".into());
        assert!(endpoint.validate().is_err());
        endpoint.citation_path = Some("../notes.txt".into());
        assert!(endpoint.validate().is_err());
        Ok(())
    }
}
