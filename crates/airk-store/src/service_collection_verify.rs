// SPDX-License-Identifier: Apache-2.0
//! Off-query dated observations. No method in this module authorizes a query.
use super::*;
use crate::current::postings::Lookup;
use crate::current::query_projection::QueryProducerCertificate;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectionPendingComponent {
    certificate: QueryProducerCertificate,
    next_page: usize,
    logical_certified_unix_ms: u64,
    physical_first_unix_ms: Option<u64>,
    physical_through_unix_ms: Option<u64>,
}
impl ProjectionPendingComponent {
    fn new(certificate: QueryProducerCertificate, now: u64) -> Self {
        Self {
            certificate,
            next_page: 0,
            logical_certified_unix_ms: now,
            physical_first_unix_ms: Some(now),
            physical_through_unix_ms: Some(now),
        }
    }
    fn validate(&self) -> Result<()> {
        self.certificate.validate()?;
        ensure!(
            self.next_page <= self.certificate.summary.pages
                && self.physical_first_unix_ms.is_some() == self.physical_through_unix_ms.is_some(),
            "invalid query projection continuation"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingTextRanges {
    certificate: crate::current::text_range::TextRangeCertificate,
    next_page: usize,
    first_unix_ms: u64,
    through_unix_ms: u64,
}
impl PendingTextRanges {
    fn new(certificate: crate::current::text_range::TextRangeCertificate, now: u64) -> Self {
        Self {
            certificate,
            next_page: 0,
            first_unix_ms: now,
            through_unix_ms: now,
        }
    }
    fn validate(&self) -> Result<()> {
        self.certificate.validate()?;
        ensure!(
            self.next_page <= self.certificate.pages,
            "invalid text range continuation"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportedProjectionAudit {
    evaluated: bool,
    #[serde(default)]
    authoritative_digest: Option<Sha256Digest>,
    #[serde(default)]
    authoritative_first_unix_ms: Option<u64>,
    #[serde(default)]
    authoritative_through_unix_ms: Option<u64>,
    coverage: ProjectionComponentCoverage,
    pending: Option<ProjectionPendingComponent>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionComponentCoverage {
    pub checked_files: u64,
    pub missing_files: u64,
    pub first_observed_unix_ms: Option<u64>,
    pub through_unix_ms: Option<u64>,
}
impl ProjectionComponentCoverage {
    fn observe(&mut self, available: bool, now: u64) -> Result<()> {
        if available {
            self.checked_files = self
                .checked_files
                .checked_add(1)
                .context("projection coverage overflow")?;
            self.first_observed_unix_ms.get_or_insert(now);
            self.through_unix_ms = Some(now);
        } else {
            self.missing_files = self
                .missing_files
                .checked_add(1)
                .context("missing projection coverage overflow")?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionPendingFile {
    scalars: bool,
    logical_certified_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_observed_unix_ms: Option<u64>,
    certificate: crate::current::native_projection::NativeProducerCertificate,
    next_page: usize,
    #[serde(default)]
    physical_first_unix_ms: Option<u64>,
    #[serde(default)]
    physical_through_unix_ms: Option<u64>,
    #[serde(default)]
    native_edges: Option<ProjectionPendingComponent>,
    #[serde(default)]
    native_edges_empty: bool,
    #[serde(default)]
    native_lines: bool,
    #[serde(default)]
    text_ranges: Option<PendingTextRanges>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionOversized {
    pub component: Sha256Digest,
    pub required_bytes: u64,
    pub admitted_bytes: u64,
    pub observed_unix_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionAuditProgress {
    #[serde(default)]
    schema: u32,
    #[serde(default)]
    preserved_v1_dates: bool,
    #[serde(default)]
    preserved_v2_dates: bool,
    #[serde(skip)]
    coverage_skipped: bool,
    pub artifact_root: Sha256Digest,
    pub started_unix_ms: u64,
    pub after: Option<String>,
    pub visited_files: u64,
    pub scalars: ProjectionComponentCoverage,
    pub native_facts: ProjectionComponentCoverage,
    #[serde(default)]
    pub native_edges: ProjectionComponentCoverage,
    #[serde(default)]
    pub read_ranges: ProjectionComponentCoverage,
    #[serde(default)]
    imported: ImportedProjectionAudit,
    pub exhausted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recent_observed_unix_ms: Option<u64>,
    #[serde(default)]
    pub recent_checked_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_pending: Option<crate::current::native_projection::SourceAuditWitness>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending: Option<ProjectionPendingFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_oversized: Option<ProjectionOversized>,
    #[serde(default)]
    pub logical_certification_bytes: u64,
    #[serde(default)]
    pub leaf_checked_pages: u64,
    #[serde(default)]
    pub leaf_checked_bytes: u64,
    #[serde(default)]
    pub manifest_loads: u64,
    #[serde(default)]
    pub physical_metadata_bytes: u64,
}
impl ProjectionAuditProgress {
    fn new(root: Sha256Digest, now: u64) -> Self {
        Self {
            schema: 4,
            preserved_v1_dates: false,
            preserved_v2_dates: false,
            coverage_skipped: false,
            artifact_root: root,
            started_unix_ms: now,
            after: None,
            visited_files: 0,
            scalars: Default::default(),
            native_facts: Default::default(),
            native_edges: Default::default(),
            read_ranges: Default::default(),
            imported: Default::default(),
            exhausted: false,
            recent_observed_unix_ms: None,
            recent_checked_bytes: 0,
            source_pending: None,
            pending: None,
            first_oversized: None,
            logical_certification_bytes: 0,
            leaf_checked_pages: 0,
            leaf_checked_bytes: 0,
            manifest_loads: 0,
            physical_metadata_bytes: 0,
        }
    }
    fn upgrade(&mut self) {
        if self.schema == 4 {
            return;
        }
        let mut next = Self::new(self.artifact_root, self.started_unix_ms);
        next.preserved_v1_dates = true;
        next.preserved_v2_dates = self.schema >= 2;
        if self.schema >= 2 {
            next.native_edges.first_observed_unix_ms = self.native_edges.first_observed_unix_ms;
            next.native_edges.through_unix_ms = self.native_edges.through_unix_ms;
            // The imported component set is unchanged by retained-text ranges.
            next.imported = self.imported.clone();
        }
        if self.schema >= 3 {
            next.read_ranges.first_observed_unix_ms = self.read_ranges.first_observed_unix_ms;
            next.read_ranges.through_unix_ms = self.read_ranges.through_unix_ms;
        }
        next.first_oversized = self.first_oversized.clone();
        next.scalars.first_observed_unix_ms = self.scalars.first_observed_unix_ms;
        next.scalars.through_unix_ms = self.scalars.through_unix_ms;
        next.native_facts.first_observed_unix_ms = self.native_facts.first_observed_unix_ms;
        next.native_facts.through_unix_ms = self.native_facts.through_unix_ms;
        if let Some(pending) = &self.pending {
            if pending.scalars {
                next.scalars
                    .first_observed_unix_ms
                    .get_or_insert(pending.logical_certified_unix_ms);
                next.scalars.through_unix_ms = Some(pending.logical_certified_unix_ms);
            }
            next.native_facts.first_observed_unix_ms = next
                .native_facts
                .first_observed_unix_ms
                .or(pending.physical_first_unix_ms);
            next.native_facts.through_unix_ms = pending
                .physical_through_unix_ms
                .or(next.native_facts.through_unix_ms);
        }
        if self.schema >= 2
            && let Some(pending) = &self.pending
        {
            if let Some(edges) = &pending.native_edges {
                next.native_edges.first_observed_unix_ms = next
                    .native_edges
                    .first_observed_unix_ms
                    .or(edges.physical_first_unix_ms);
                next.native_edges.through_unix_ms = edges
                    .physical_through_unix_ms
                    .or(next.native_edges.through_unix_ms);
            } else if pending.native_edges_empty {
                next.native_edges
                    .first_observed_unix_ms
                    .get_or_insert(pending.logical_certified_unix_ms);
                next.native_edges.through_unix_ms = Some(pending.logical_certified_unix_ms);
            }
        }
        *self = next;
    }
    fn record(
        &mut self,
        path: &str,
        scalars: bool,
        native: bool,
        edges: bool,
        ranges: bool,
        (logical_now, physical_now): (u64, u64),
    ) -> Result<()> {
        self.scalars.observe(scalars, logical_now)?;
        self.native_facts.observe(native, physical_now)?;
        self.native_edges.observe(edges, physical_now)?;
        self.read_ranges.observe(ranges, physical_now)?;
        self.visited_files = self
            .visited_files
            .checked_add(1)
            .context("projection visited count overflow")?;
        self.after = Some(path.into());
        Ok(())
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            matches!(self.schema, 0 | 2 | 3 | 4),
            "unsupported projection continuation schema"
        );
        ensure!(
            self.after
                .as_ref()
                .is_none_or(|path| !path.is_empty() && path.len() <= 8192 && !path.contains('\0')),
            "invalid projection audit continuation"
        );
        if let Some(source) = &self.source_pending {
            source.validate()?;
            ensure!(
                self.schema == 4
                    && !self.exhausted
                    && self.after.is_some()
                    && self.pending.is_none(),
                "invalid pending source audit"
            );
        }
        if let Some(pending) = &self.pending {
            ensure!(
                !self.exhausted
                    && self.after.is_some()
                    && pending.certificate.version == 1
                    && pending.certificate.facts <= 10_000
                    && pending.certificate.pages <= 10_000
                    && pending.next_page <= pending.certificate.pages,
                "invalid pending projection audit"
            );
            ensure!(
                !pending.native_edges_empty || pending.native_edges.is_none(),
                "conflicting pending empty edge declaration"
            );
            ensure!(
                !pending.native_lines || pending.text_ranges.is_none(),
                "conflicting pending read range components"
            );
            if let Some(text) = &pending.text_ranges {
                text.validate()?;
            }
            if let Some(edges) = &pending.native_edges {
                edges.validate()?;
            }
        }
        let mut components = vec![&self.scalars, &self.native_facts];
        if self.schema >= 2 {
            components.push(&self.native_edges);
        }
        if self.schema >= 3 {
            components.push(&self.read_ranges);
        }
        for (component_index, component) in components.into_iter().enumerate() {
            ensure!(
                component.checked_files.checked_add(component.missing_files)
                    == Some(self.visited_files)
                    && ((component.checked_files == 0
                        && component.first_observed_unix_ms.is_none()
                        && component.through_unix_ms.is_none())
                        || ((component.checked_files > 0
                            || ((self.preserved_v1_dates && component_index < 2)
                                || (self.preserved_v2_dates && component_index < 4)))
                            && component.first_observed_unix_ms.is_some()
                            && component.through_unix_ms.is_some())),
                "invalid projection audit coverage counters"
            );
        }
        if self.schema >= 2 {
            ensure!(
                self.imported
                    .coverage
                    .checked_files
                    .checked_add(self.imported.coverage.missing_files)
                    .is_some_and(|count| count <= 1)
                    && (!self.imported.evaluated || self.imported.pending.is_none()),
                "invalid imported audit coverage"
            );
            let imported = &self.imported;
            let records = imported
                .coverage
                .checked_files
                .checked_add(imported.coverage.missing_files)
                .context("import coverage count overflow")?;
            ensure!(
                (!imported.evaluated && records == 0)
                    || (imported.evaluated
                        && records == u64::from(imported.authoritative_digest.is_some())),
                "imported coverage settled before component completion"
            );
            ensure!(
                imported.authoritative_digest.is_some()
                    == imported.authoritative_first_unix_ms.is_some()
                    && imported.authoritative_digest.is_some()
                        == imported.authoritative_through_unix_ms.is_some()
                    && (imported.coverage.checked_files == 0
                        || (imported.coverage.first_observed_unix_ms.is_some()
                            && imported.coverage.through_unix_ms.is_some()))
                    && (imported.coverage.checked_files > 0
                        || (imported.coverage.first_observed_unix_ms.is_none()
                            && imported.coverage.through_unix_ms.is_none())),
                "invalid imported component dates"
            );
            if let Some(pending) = &self.imported.pending {
                pending.validate()?;
                ensure!(
                    (self.exhausted || self.preserved_v2_dates)
                        && imported.authoritative_digest.is_some(),
                    "import continuation precedes component admission"
                );
            }
        }
        Ok(())
    }
    fn declared_complete(&self) -> bool {
        !self.coverage_skipped
            && self.schema == 4
            && self.exhausted
            && self.pending.is_none()
            && self.source_pending.is_none()
            && self.imported.evaluated
            && self.imported.pending.is_none()
            && self.recent_observed_unix_ms.is_some()
    }
    /// Historical component coverage only, never private-root/query authority.
    pub fn complete_coverage(&self) -> bool {
        !self.coverage_skipped
            && self.schema == 4
            && self.exhausted
            && self.imported.evaluated
            && self.recent_observed_unix_ms.is_some()
            && self.imported.coverage.missing_files == 0
            && self.scalars.missing_files == 0
            && self.native_facts.missing_files == 0
            && self.native_edges.missing_files == 0
            && self.scalars.checked_files == self.visited_files
            && self.native_facts.checked_files == self.visited_files
            && self.native_edges.checked_files == self.visited_files
            && self.read_ranges.missing_files == 0
            && self.read_ranges.checked_files == self.visited_files
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyProgress {
    version: u32,
    job: Sha256Digest,
    scope: crate::current::CurrentScope,
    artifact_root: Sha256Digest,
    lexical_commit: Sha256Digest,
    lexical_observed_unix_ms: Option<u64>,
    started_unix_ms: u64,
    through_unix_ms: u64,
    after: Option<String>,
    checked_files: u64,
    checked_bytes: u64,
    objects_complete: bool,
    /// Includes uncertain I/O; not a declaration of physical corruption.
    /// Metadata/partial observation cannot clear it; actual full clean physical
    /// completion may settle uncertainty after all declared components finish.
    unresolved: bool,
    // Separate bounded receipt: do not add another path cursor to ordinary or
    // exceptional history. Only the current turn assembles this derived view.
    #[serde(skip)]
    projections: Option<ProjectionAuditProgress>,
}
impl VerifyProgress {
    pub fn requires_repair(&self) -> bool {
        self.unresolved
    }
    fn reconcile(old: Option<Self>, job: Sha256Digest, head: &CurrentHead, now: u64) -> Self {
        let unresolved = old.as_ref().is_some_and(|old| old.unresolved);
        let Some(mut old) =
            old.filter(|old| old.version == 1 && old.job == job && old.scope == head.scope)
        else {
            return Self {
                version: 1,
                job,
                scope: head.scope.clone(),
                artifact_root: head.artifact_root,
                lexical_commit: head.lexical_commit,
                lexical_observed_unix_ms: None,
                started_unix_ms: now,
                through_unix_ms: now,
                after: None,
                checked_files: 0,
                checked_bytes: 0,
                objects_complete: false,
                unresolved,
                projections: Some(ProjectionAuditProgress::new(head.artifact_root, now)),
            };
        };
        // Metadata/semantic/graph-only publication need not discard unrelated
        // observed object work. These are authenticated component identities.
        if old.artifact_root != head.artifact_root {
            old.artifact_root = head.artifact_root;
            old.after = None;
            old.checked_files = 0;
            old.checked_bytes = 0;
            old.objects_complete = false;
            old.started_unix_ms = now;
        }
        if old.lexical_commit != head.lexical_commit {
            old.lexical_commit = head.lexical_commit;
            old.lexical_observed_unix_ms = None;
        }
        if old
            .projections
            .as_ref()
            .is_none_or(|projection| projection.artifact_root != head.artifact_root)
        {
            old.projections = Some(ProjectionAuditProgress::new(head.artifact_root, now));
        }
        old
    }
    pub fn projection_coverage(&self) -> Option<&ProjectionAuditProgress> {
        self.projections.as_ref()
    }
    pub fn complete(&self) -> bool {
        self.objects_complete && self.lexical_observed_unix_ms.is_some() && !self.unresolved
    }
}

/// Non-Clone/non-serializable: constructed only after this module actually
/// verifies the declared lexical/retained-input components, drains owned reads
/// and persists dated progress, including the declared projection checks. This
/// does not certify graph/vector quality or every file in a private data root.
pub struct AuditCompletionProof {
    drained: crate::service::registry::AuditDrainProof,
    head: CurrentHead,
}
impl AuditCompletionProof {
    pub(crate) fn matches(
        &self,
        connection: &rusqlite::Connection,
        job: &airk_core::service::JobRecord,
    ) -> Result<bool> {
        self.drained.matches(connection, job)
    }
    pub fn revision(&self) -> Result<airk_core::identity::RevisionToken> {
        let token = self.head.token()?;
        Ok(airk_core::identity::RevisionToken {
            incarnation: token.incarnation,
            generation: token.sequence,
            revision: token.digest,
        })
    }
}
/// Dated retained-data coverage. Missing optional components stay explicit;
/// serializing this observation never serializes the physical authority below.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedAuditObservation {
    pub version: u32,
    pub policy: Sha256Digest,
    pub head: CurrentToken,
    pub lexical_unix_ms: u64,
    #[serde(default)]
    pub recent_unix_ms: Option<u64>,
    pub source_first_unix_ms: u64,
    pub source_through_unix_ms: u64,
    pub checked_files: u64,
    pub checked_bytes: u64,
    pub scalars: ProjectionComponentCoverage,
    pub native_facts: ProjectionComponentCoverage,
    pub native_edges: ProjectionComponentCoverage,
    pub read_ranges: ProjectionComponentCoverage,
    pub imported: ProjectionComponentCoverage,
    pub authoritative_analysis: Option<Sha256Digest>,
    pub analysis_first_unix_ms: Option<u64>,
    pub analysis_through_unix_ms: Option<u64>,
}
impl RetainedAuditObservation {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "unsupported retained audit observation");
        for component in [
            &self.scalars,
            &self.native_facts,
            &self.native_edges,
            &self.read_ranges,
        ] {
            ensure!(
                component.checked_files.checked_add(component.missing_files)
                    == Some(self.checked_files)
                    && component.first_observed_unix_ms.is_some()
                        == component.through_unix_ms.is_some()
                    && (component.checked_files == 0 || component.first_observed_unix_ms.is_some()),
                "invalid retained audit component observation"
            );
        }
        ensure!(
            self.authoritative_analysis.is_some() == self.analysis_first_unix_ms.is_some()
                && self.authoritative_analysis.is_some() == self.analysis_through_unix_ms.is_some()
                && self
                    .imported
                    .checked_files
                    .checked_add(self.imported.missing_files)
                    == Some(u64::from(self.authoritative_analysis.is_some()))
                && self.imported.first_observed_unix_ms.is_some()
                    == self.imported.through_unix_ms.is_some()
                && (self.imported.checked_files == 0
                    || self.imported.first_observed_unix_ms.is_some()),
            "invalid retained audit import observation"
        );
        Ok(())
    }
}
/// The fixed retained/declared profile checks lexical representation, retained
/// source/extraction and every declared projection. It does not certify graph,
/// vector/posting completeness or confer native fast-query/recovery authority.
/// No constructor, Clone or Deserialize exists for this physical evidence.
pub struct RetainedAuditCompletionProof {
    drained: crate::service::registry::AuditDrainProof,
    observation: RetainedAuditObservation,
}
impl RetainedAuditCompletionProof {
    pub fn observation(&self) -> &RetainedAuditObservation {
        &self.observation
    }
    pub fn revision(&self) -> airk_core::identity::RevisionToken {
        let head = self.observation.head;
        airk_core::identity::RevisionToken {
            incarnation: head.incarnation,
            generation: head.sequence,
            revision: head.digest,
        }
    }
    pub(crate) fn job(&self) -> Sha256Digest {
        self.drained.job()
    }
    pub(crate) fn matches(
        &self,
        connection: &rusqlite::Connection,
        job: &airk_core::service::JobRecord,
    ) -> Result<bool> {
        Ok(self.observation.policy == declared_audit_policy()
            && self.drained.matches(connection, job)?)
    }
}

/// Actual fixed private-consumer coverage. Posting/graph/vector/occurrence reads remain
/// strict and are intentionally outside this policy. Every declared projection
/// is audited; absent optional legacy projections select strict fallback.
/// Neither saved observations nor ordinary historical completion can mint this.
pub struct QueryComponentAuditProof {
    drained: crate::service::registry::AuditDrainProof,
    observation: RetainedAuditObservation,
}
impl QueryComponentAuditProof {
    pub fn observation(&self) -> &RetainedAuditObservation {
        &self.observation
    }
    pub fn revision(&self) -> airk_core::identity::RevisionToken {
        let head = self.observation.head;
        airk_core::identity::RevisionToken {
            incarnation: head.incarnation,
            generation: head.sequence,
            revision: head.digest,
        }
    }
    pub(crate) fn drain(&self) -> &crate::service::registry::AuditDrainProof {
        &self.drained
    }
    pub(crate) fn job(&self) -> Sha256Digest {
        self.drained.job()
    }
    pub(crate) fn matches(
        &self,
        connection: &rusqlite::Connection,
        job: &airk_core::service::JobRecord,
    ) -> Result<bool> {
        Ok(self.observation.policy == declared_audit_policy()
            && self.drained.matches(connection, job)?)
    }
}

/// Fixed private-consumer policy; posting/graph/vector/occurrence reads retain strict
/// per-query checks. Missing optional projections select strict fallback.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecoveryProfile {
    BaselineV1,
    ProjectedV1,
}
pub(crate) const NATIVE_RECOVERY_PROFILE: RecoveryProfile = RecoveryProfile::ProjectedV1;

/// Opaque owning recovery purpose. Serialized only as saved audit context, never
/// accepted as authority from a caller. Its live instance comes from the owner.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecoveryContext {
    pub(crate) namespace: Incarnation,
    pub(crate) project: airk_core::service::ProjectBinding,
    pub(crate) worker: Incarnation,
    pub(crate) worker_epoch: u64,
    pub(crate) job: Sha256Digest,
    pub(crate) inherited: Sha256Digest,
    pub(crate) head: CurrentToken,
    pub(crate) root: Sha256Digest,
    pub(crate) selected: Sha256Digest,
    pub(crate) profile: RecoveryProfile,
}
/// Live purpose has no Deserialize constructor. Persisted context is only
/// continuation data and must be checked against this newly minted live value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryEpoch {
    pub(crate) context: RecoveryContext,
}
impl RecoveryEpoch {
    /// Actual durably admitted job; observing identity grants no recovery authority.
    pub fn job(&self) -> Sha256Digest {
        self.context.job
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryReceipt {
    version: u32,
    owner: Sha256Digest,
    namespace: Incarnation,
    epoch: RecoveryContext,
    progress: VerifyProgress,
}
const RECOVERY_RECEIPT: &str = "integrity-recovery.json";
const PROJECTION_RECEIPT: &str = "integrity-projection.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectionReceipt {
    version: u32,
    owner: Sha256Digest,
    namespace: Incarnation,
    job: Sha256Digest,
    incarnation: Incarnation,
    progress: ProjectionAuditProgress,
}
impl ProjectionReceipt {
    fn validate(&self, owner: Sha256Digest) -> Result<()> {
        ensure!(
            matches!(
                (self.version, self.progress.schema),
                (1, 0) | (2, 2) | (3, 3) | (4, 4)
            ) && self.owner == owner,
            "foreign projection audit receipt"
        );
        self.progress.validate()
    }
}
pub(crate) fn owned_projection_scratch(path: &Path, owner: Sha256Digest) -> bool {
    json::<ProjectionReceipt>(path).is_ok_and(|receipt| receipt.validate(owner).is_ok())
}
impl IntegrityOwner<'_> {
    fn load_projection(
        &self,
        namespace: Incarnation,
        job: Sha256Digest,
        head: &CurrentHead,
    ) -> Result<Option<ProjectionAuditProgress>> {
        let path = self.collection.directory.join(PROJECTION_RECEIPT);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        let receipt: ProjectionReceipt = json(&path)?;
        receipt.validate(self.collection.binding)?;
        ensure!(
            receipt.namespace == namespace,
            "foreign projection audit namespace"
        );
        Ok((receipt.job == job
            && receipt.incarnation == head.scope.incarnation
            && receipt.progress.artifact_root == head.artifact_root)
            .then_some(receipt.progress)
            .map(|mut progress| {
                progress.upgrade();
                progress
            }))
    }
}
impl RecoveryReceipt {
    fn validate(&self, owner: Sha256Digest) -> Result<()> {
        ensure!(
            self.version == 1
                && self.owner == owner
                && self.namespace == self.epoch.namespace
                && self.epoch.worker_epoch > 0
                && self.epoch.head.sequence <= i64::MAX as u64
                && self.progress.job == self.epoch.job,
            "foreign or unsupported recovery continuation"
        );
        self.progress.scope.validate()?;
        ensure!(
            self.progress.version == 1
                && self.progress.scope.incarnation == self.epoch.head.incarnation
                && self
                    .progress
                    .after
                    .as_ref()
                    .is_none_or(|path| !path.is_empty()
                        && path.len() <= 8192
                        && !path.contains('\0')),
            "unsupported or malformed recovery continuation"
        );
        Ok(())
    }
}
pub(crate) fn owned_recovery_scratch(path: &Path, owner: Sha256Digest) -> bool {
    json::<RecoveryReceipt>(path).is_ok_and(|receipt| receipt.validate(owner).is_ok())
}
impl IntegrityOwner<'_> {
    fn load_recovery(&self, namespace: Incarnation) -> Result<Option<RecoveryReceipt>> {
        let path = self.collection.directory.join(RECOVERY_RECEIPT);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        let receipt: RecoveryReceipt = json(&path)?;
        receipt.validate(self.collection.binding)?;
        ensure!(
            receipt.namespace == namespace
                && receipt.progress.scope.root == self.collection.owner.root
                && receipt.progress.scope.collection == self.collection.owner.collection,
            "foreign recovery namespace or publication scope"
        );
        Ok(Some(receipt))
    }
}
/// Actual post-epoch coverage, not ordinary historical/global job completion.
/// No constructor, Clone or Deserialize is exposed for physical evidence.
pub struct EnabledComponentAuditProof {
    drained: crate::service::registry::AuditDrainProof,
    epoch: RecoveryContext,
    head: CurrentHead,
}
impl EnabledComponentAuditProof {
    pub(crate) fn exact_head(&self) -> &CurrentHead {
        &self.head
    }
    pub(crate) fn epoch(&self) -> &RecoveryContext {
        &self.epoch
    }
    pub(crate) fn matches(
        &self,
        connection: &rusqlite::Connection,
        job: &airk_core::service::JobRecord,
    ) -> Result<bool> {
        Ok(self.drained.matches(connection, job)?
            && self.epoch.job == job.id
            && self.epoch.profile == NATIVE_RECOVERY_PROFILE
            && self.epoch.head == self.head.token()?
            && crate::service::registry::owner_session::recovery_matches(connection, &self.epoch)?)
    }
}
/// Versioned scheduling identity, not native query authority. F14 must select
/// its complete enabled-component policy before relaxing any strict reader.
pub fn declared_audit_policy() -> Sha256Digest {
    airk_core::identity::framed_digest(
        b"declared-integrity-audit-policy/1",
        &[
            &airk_core::storage_format::NATIVE_QUERY_PROJECTION_VERSION.to_be_bytes(),
            &5u32.to_be_bytes(),
        ],
    )
}
pub struct VerifyTurn {
    pub verified_files: u64,
    pub completed: Option<AuditCompletionProof>,
    pub recovery_completed: Option<EnabledComponentAuditProof>,
    pub progress: VerifyProgress,
    pub blocked_large: bool,
    pub required_bytes: Option<u64>,
    pub admitted_bytes: u64,
    /// Bytes debited to this turn's retained/projection/recent admission. The
    /// existing indivisible lexical integrity pass and fixed map observations
    /// are separate; this is not a claim of total filesystem I/O.
    pub charged_bytes: u64,
    pub drained: Option<crate::service::registry::AuditDrainProof>,
    pub blocked_input: Option<crate::current::audit_witness::AuditInputWitness>,
    pub retained_completed: Option<RetainedAuditCompletionProof>,
    pub query_completed: Option<QueryComponentAuditProof>,
}
impl VerifyTurn {
    // A completed campaign can honestly report unavailable projection coverage.
    // Historical VerifyProgress::complete retains its original component meaning.
    pub fn work_complete(&self) -> bool {
        self.progress.objects_complete
            && self.progress.lexical_observed_unix_ms.is_some()
            && self
                .progress
                .projections
                .as_ref()
                .is_some_and(|projection| {
                    projection.exhausted
                        && projection.imported.evaluated
                        && projection.source_pending.is_none()
                        && projection.pending.is_none()
                        && projection.recent_observed_unix_ms.is_some()
                })
    }
    pub fn projection_coverage_complete(&self) -> bool {
        self.progress
            .projections
            .as_ref()
            .is_some_and(ProjectionAuditProgress::complete_coverage)
    }
}
#[allow(clippy::too_many_arguments)]
fn audit_pending_projection(
    progress: &mut ProjectionAuditProgress,
    native: &crate::current::native_projection::NativeFactRead<'_>,
    read: &crate::current::CurrentRead,
    store: &FileStore,
    used: &mut u64,
    maximum: u64,
    pages: &mut usize,
    now: u64,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Option<u64>> {
    let pending = progress
        .pending
        .as_mut()
        .context("pending projection audit missing")?;
    native.check_certificate(&pending.certificate)?;
    let path = progress
        .after
        .as_deref()
        .context("pending projection path missing")?;
    let declared_edges = read.native_edge_reference(path)?;
    ensure!(
        pending.native_edges_empty == read.native_edges_declared_empty(path)?
            && pending
                .native_edges
                .as_ref()
                .map(|component| component.certificate.manifest)
                == declared_edges
                    .as_ref()
                    .map(|(reference, _)| reference.digest),
        "native edge continuation differs from selected components"
    );
    if let (Some(component), Some((reference, _))) = (&pending.native_edges, &declared_edges) {
        ensure!(
            reference.projection.as_ref() == Some(&component.certificate.summary),
            "native edge continuation summary changed"
        );
    }
    let declared_text = read.text_range_reference(path)?;
    ensure!(
        pending.native_lines == read.native_line_reference(path)?.is_some()
            && pending
                .text_ranges
                .as_ref()
                .map(|text| text.certificate.manifest)
                == declared_text
                    .as_ref()
                    .map(|reference| reference.root.digest),
        "read range continuation differs from selected components"
    );
    if let (Some(text), Some(reference)) = (&pending.text_ranges, &declared_text) {
        ensure!(
            text.certificate.pages == reference.pages
                && text.certificate.data_bytes == reference.data_bytes,
            "text range continuation summary changed"
        );
    }
    while pending.next_page < pending.certificate.pages && *pages < 128 {
        checkpoint()?;
        let size = native.header().pages[pending.next_page].resource.bytes as u64;
        if used.saturating_add(size) > maximum {
            let required = (native.manifest_bytes() as u64)
                .checked_add(size)
                .context("projection audit size overflow")?;
            return Ok((required > maximum).then_some(required));
        }
        native.page(pending.next_page)?;
        pending.next_page += 1;
        *pages += 1;
        *used += size;
        pending.physical_first_unix_ms.get_or_insert(now);
        pending.physical_through_unix_ms = Some(now);
        progress.leaf_checked_pages = progress
            .leaf_checked_pages
            .checked_add(1)
            .context("projection page count overflow")?;
        progress.leaf_checked_bytes = progress
            .leaf_checked_bytes
            .checked_add(size)
            .context("projection physical byte overflow")?;
        checkpoint()?;
    }
    if pending.next_page == pending.certificate.pages
        && let Some(edges) = pending.native_edges.as_mut()
        && edges.next_page < edges.certificate.summary.pages
    {
        if *pages == 128 {
            return Ok(None);
        }
        let path = progress
            .after
            .as_deref()
            .context("pending native edge path missing")?;
        let (reference, _) = read
            .native_edge_reference(path)?
            .context("pending native edge component disappeared")?;
        let prefix = native.manifest_bytes() as u64;
        let minimum = prefix
            .checked_add(reference.bytes as u64)
            .context("native edge audit size overflow")?;
        if minimum > maximum {
            return Ok(Some(minimum));
        }
        if used.saturating_add(reference.bytes as u64) > maximum {
            return Ok(None);
        }
        let projection = read
            .native_edge_projection(store, path)?
            .context("pending native edge component disappeared")?;
        let turn = projection.audit_physical(
            &edges.certificate,
            edges.next_page,
            128 - *pages,
            maximum - *used,
            &mut checkpoint,
        )?;
        *used = used
            .checked_add(turn.checked_bytes)
            .context("projection physical bytes overflow")?;
        *pages += turn.checked_pages;
        progress.manifest_loads = progress
            .manifest_loads
            .checked_add(1)
            .context("projection manifest count overflow")?;
        progress.leaf_checked_pages = progress
            .leaf_checked_pages
            .checked_add(turn.checked_pages as u64)
            .context("projection page count overflow")?;
        progress.leaf_checked_bytes = progress
            .leaf_checked_bytes
            .checked_add(turn.data_bytes)
            .context("projection leaf byte overflow")?;
        progress.physical_metadata_bytes = progress
            .physical_metadata_bytes
            .checked_add(turn.checked_bytes - turn.data_bytes)
            .context("projection metadata byte overflow")?;
        edges.next_page = turn.next_page;
        if turn.checked_bytes > 0 {
            edges.physical_through_unix_ms = Some(now);
        }
        if let Some(required) = turn.required_bytes {
            let required = prefix
                .checked_add(required)
                .context("projection next turn size overflow")?;
            if required > maximum {
                return Ok(Some(required));
            }
        }
        if !turn.complete {
            return Ok(None);
        }
    }
    if pending.next_page == pending.certificate.pages
        && let Some(text) = pending.text_ranges.as_mut()
        && text.next_page < text.certificate.pages
    {
        if *pages == 128 {
            return Ok(None);
        }
        let reference = declared_text
            .as_ref()
            .context("pending text range disappeared")?;
        let prefix = native.manifest_bytes() as u64;
        let minimum = prefix
            .checked_add(reference.root.bytes as u64)
            .context("text audit size overflow")?;
        if minimum > maximum {
            return Ok(Some(minimum));
        }
        if used.saturating_add(reference.root.bytes as u64) > maximum {
            return Ok(None);
        }
        let projection = read
            .text_range_projection(store, path)?
            .context("pending text range disappeared")?;
        let turn = projection.audit_physical(
            &text.certificate,
            text.next_page,
            128 - *pages,
            maximum - *used,
            &mut checkpoint,
        )?;
        *used = used
            .checked_add(turn.checked_bytes)
            .context("text audit size overflow")?;
        *pages += turn.checked_pages;
        progress.manifest_loads = progress
            .manifest_loads
            .checked_add(1)
            .context("text audit count overflow")?;
        progress.leaf_checked_pages = progress
            .leaf_checked_pages
            .checked_add(turn.checked_pages as u64)
            .context("text audit count overflow")?;
        progress.leaf_checked_bytes = progress
            .leaf_checked_bytes
            .checked_add(turn.data_bytes)
            .context("text audit size overflow")?;
        progress.physical_metadata_bytes = progress
            .physical_metadata_bytes
            .checked_add(turn.checked_bytes - turn.data_bytes)
            .context("text audit metadata overflow")?;
        text.next_page = turn.next_page;
        if turn.checked_bytes > 0 {
            text.through_unix_ms = now;
        }
        if let Some(required) = turn.required_bytes {
            let required = prefix
                .checked_add(required)
                .context("text audit next turn overflow")?;
            if required > maximum {
                return Ok(Some(required));
            }
        }
        if !turn.complete {
            return Ok(None);
        }
    }
    if pending.next_page == pending.certificate.pages {
        let completed = progress
            .pending
            .take()
            .context("completed projection missing")?;
        let path = progress
            .after
            .clone()
            .context("pending projection path missing")?;
        let previous_edge_first = progress.native_edges.first_observed_unix_ms;
        let previous_range_first = progress.read_ranges.first_observed_unix_ms;
        progress.record(
            &path,
            completed.scalars,
            true,
            completed.native_edges.is_some() || completed.native_edges_empty,
            completed.native_lines || completed.text_ranges.is_some(),
            (
                completed.logical_certified_unix_ms,
                completed.physical_first_unix_ms.unwrap_or(now),
            ),
        )?;
        if completed.scalars
            && let Some(source_time) = completed.source_observed_unix_ms
        {
            progress.scalars.first_observed_unix_ms = Some(
                progress
                    .scalars
                    .first_observed_unix_ms
                    .unwrap_or(source_time)
                    .min(source_time),
            );
        }
        progress.native_facts.through_unix_ms =
            Some(completed.physical_through_unix_ms.unwrap_or(now));
        if completed.native_lines {
            if progress.read_ranges.checked_files == 1 {
                progress.read_ranges.first_observed_unix_ms = previous_range_first.or(Some(
                    completed
                        .source_observed_unix_ms
                        .unwrap_or(completed.logical_certified_unix_ms),
                ));
            }
            progress.read_ranges.through_unix_ms = Some(
                completed
                    .source_observed_unix_ms
                    .unwrap_or(completed.logical_certified_unix_ms),
            );
        }
        if let Some(text) = completed.text_ranges {
            if progress.read_ranges.checked_files == 1 {
                progress.read_ranges.first_observed_unix_ms =
                    previous_range_first.or(Some(text.first_unix_ms));
            }
            progress.read_ranges.through_unix_ms = Some(text.through_unix_ms);
        }
        if completed.native_edges_empty {
            if progress.native_edges.checked_files == 1 {
                progress.native_edges.first_observed_unix_ms =
                    previous_edge_first.or(Some(completed.logical_certified_unix_ms));
            }
            progress.native_edges.through_unix_ms = Some(completed.logical_certified_unix_ms);
        }
        if let Some(edges) = completed.native_edges {
            if progress.native_edges.checked_files == 1 {
                progress.native_edges.first_observed_unix_ms =
                    previous_edge_first.or(edges.physical_first_unix_ms);
            }
            progress.native_edges.through_unix_ms = edges.physical_through_unix_ms;
        }
    }
    Ok(None)
}
fn record_projection_oversized(
    progress: &mut ProjectionAuditProgress,
    path: &str,
    required: u64,
    admitted: u64,
    now: u64,
) {
    if progress.first_oversized.is_none() {
        progress.first_oversized = Some(ProjectionOversized {
            component: airk_core::identity::framed_digest(
                b"projection-audit-component/1",
                &[path.as_bytes()],
            ),
            required_bytes: required,
            admitted_bytes: admitted,
            observed_unix_ms: now,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn audit_imported_projection(
    progress: &mut ProjectionAuditProgress,
    read: &crate::current::CurrentRead,
    store: &FileStore,
    used: &mut u64,
    maximum: u64,
    pages: &mut usize,
    now: u64,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Option<u64>> {
    if progress.imported.evaluated {
        let analysis = read.resource_reference("analysis")?;
        let projection = read.resource_reference("analysis-projection")?.is_some();
        let coverage = &progress.imported.coverage;
        ensure!(
            (analysis.is_none()
                && !projection
                && coverage.checked_files == 0
                && coverage.missing_files == 0
                && progress.imported.authoritative_digest.is_none())
                || (analysis.is_some()
                    && progress.imported.authoritative_digest
                        == analysis.as_ref().map(|reference| reference.digest)
                    && ((projection
                        && coverage.checked_files == 1
                        && coverage.missing_files == 0)
                        || (!projection
                            && coverage.checked_files == 0
                            && coverage.missing_files == 1))),
            "imported audit coverage differs from selected components"
        );
        return Ok(None);
    }
    if progress.imported.pending.is_none() {
        let Some(required) = read.import_certification_bytes()? else {
            progress.imported.evaluated = true;
            return Ok(None);
        };
        if required > maximum {
            return Ok(Some(required));
        }
        if used.saturating_add(required) > maximum {
            return Ok(None);
        }
        checkpoint()?;
        let analysis = read
            .resource_reference("analysis")?
            .context("analysis disappeared during audit")?;
        let certificate = if read.resource_reference("analysis-projection")?.is_some() {
            Some(
                read.certify_imported_projection(store, &mut checkpoint)?
                    .context("import projection disappeared during certification")?,
            )
        } else {
            // Compatible omission still audits the authoritative resource. Missing
            // acceleration is incomplete coverage, never permission to skip bytes.
            read.analysis(store)?
                .context("analysis disappeared during audit")?;
            None
        };
        *used = used
            .checked_add(required)
            .context("import certification byte overflow")?;
        progress.logical_certification_bytes = progress
            .logical_certification_bytes
            .checked_add(required)
            .context("projection logical byte overflow")?;
        progress.imported.authoritative_digest = Some(analysis.digest);
        progress
            .imported
            .authoritative_first_unix_ms
            .get_or_insert(now);
        progress.imported.authoritative_through_unix_ms = Some(now);
        if let Some(certificate) = certificate {
            progress.manifest_loads = progress
                .manifest_loads
                .checked_add(1)
                .context("projection manifest count overflow")?;
            progress.imported.pending = Some(ProjectionPendingComponent::new(certificate, now));
        } else {
            progress.imported.coverage.observe(false, now)?;
            progress.imported.evaluated = true;
            return Ok(None);
        }
    }
    let pending = progress
        .imported
        .pending
        .as_mut()
        .context("import projection continuation missing")?;
    let selected = read
        .resource_reference("analysis-projection")?
        .context("pending import projection missing")?;
    ensure!(
        selected.digest == pending.certificate.manifest
            && selected.projection.as_ref() == Some(&pending.certificate.summary),
        "import continuation differs from selected component"
    );
    if pending.next_page < pending.certificate.summary.pages {
        if *pages == 128 {
            return Ok(None);
        }
        let reference = read
            .resource_reference("analysis-projection")?
            .context("pending import projection missing")?;
        if reference.bytes as u64 > maximum {
            return Ok(Some(reference.bytes as u64));
        }
        if used.saturating_add(reference.bytes as u64) > maximum {
            return Ok(None);
        }
        let projection = read
            .imported_projection(store)?
            .context("pending import projection missing")?;
        let turn = projection.audit_physical(
            &pending.certificate,
            pending.next_page,
            128 - *pages,
            maximum - *used,
            &mut checkpoint,
        )?;
        *used = used
            .checked_add(turn.checked_bytes)
            .context("projection physical byte overflow")?;
        *pages += turn.checked_pages;
        progress.manifest_loads = progress
            .manifest_loads
            .checked_add(1)
            .context("projection manifest count overflow")?;
        progress.leaf_checked_pages = progress
            .leaf_checked_pages
            .checked_add(turn.checked_pages as u64)
            .context("projection page count overflow")?;
        progress.leaf_checked_bytes = progress
            .leaf_checked_bytes
            .checked_add(turn.data_bytes)
            .context("projection leaf byte overflow")?;
        progress.physical_metadata_bytes = progress
            .physical_metadata_bytes
            .checked_add(turn.checked_bytes - turn.data_bytes)
            .context("projection metadata byte overflow")?;
        pending.next_page = turn.next_page;
        if turn.checked_bytes > 0 {
            pending.physical_through_unix_ms = Some(now);
        }
        if let Some(required) = turn.required_bytes
            && required > maximum
        {
            return Ok(Some(required));
        }
        if !turn.complete {
            return Ok(None);
        }
    }
    let completed = progress
        .imported
        .pending
        .take()
        .context("completed import projection missing")?;
    progress.imported.coverage.observe(
        true,
        completed
            .physical_first_unix_ms
            .unwrap_or(completed.logical_certified_unix_ms),
    )?;
    progress.imported.coverage.through_unix_ms = completed.physical_through_unix_ms;
    progress.imported.evaluated = true;
    Ok(None)
}

impl IntegrityOwner<'_> {
    /// One optional legacy lexical component and one bounded retained-object page.
    /// Acquire the existing writer before entry, release it between turns. No
    /// CurrentRead, transaction, writer or object lease survives this call.
    /// `checkpoint` is the existing service job cancellation/admission hook.
    pub fn verify_turn(
        &self,
        writer: &mut crate::WriterLock,
        job: Sha256Digest,
        max_files: usize,
        max_bytes: u64,
        now: u64,
        checkpoint: impl Fn() -> Result<()>,
    ) -> Result<VerifyTurn> {
        self.verify_turn_inner(
            writer,
            (job, job),
            (max_files, max_bytes),
            now,
            checkpoint,
            None,
        )
    }
    /// One bounded automatic campaign turn. Its opaque registry context owns
    /// both the actual job and a crash-fresh continuation identity.
    pub fn verify_automatic_turn(
        &self,
        writer: &mut crate::WriterLock,
        context: &crate::service::registry::AutomaticAuditContext,
        max_files: usize,
        max_bytes: u64,
        now: u64,
        checkpoint: impl Fn() -> Result<()>,
    ) -> Result<VerifyTurn> {
        // Opaque context is a live cohort identity, not indefinitely reusable
        // authority. Reobserve the fenced running job before selecting its
        // continuation namespace; a crash or promotion invalidates old context.
        let current = self
            .registry
            .automatic_audit_context(self.lease, context.job())?
            .context("automatic audit cohort no longer current")?;
        ensure!(
            current.job() == context.job()
                && current.epoch() == context.epoch()
                && current.campaign() == context.campaign()
                && current.manual() == context.manual(),
            "stale automatic audit context"
        );
        self.verify_turn_inner(
            writer,
            (context.job(), context.progress_key()),
            (max_files, max_bytes),
            now,
            checkpoint,
            None,
        )
    }
    /// Exceptional owner recovery only. Ordinary Verify retains its dated cursor.
    pub fn verify_recovery_turn(
        &self,
        writer: &mut crate::WriterLock,
        epoch: &RecoveryEpoch,
        max_files: usize,
        max_bytes: u64,
        now: u64,
        checkpoint: impl Fn() -> Result<()>,
    ) -> Result<VerifyTurn> {
        self.verify_turn_inner(
            writer,
            (
                epoch.context.job,
                airk_core::identity::framed_digest(
                    b"projected-recovery-progress/1",
                    &[&serde_json::to_vec(&epoch.context)?],
                ),
            ),
            (max_files, max_bytes),
            now,
            checkpoint,
            Some(epoch),
        )
    }
    fn verify_turn_inner(
        &self,
        writer: &mut crate::WriterLock,
        identity: (Sha256Digest, Sha256Digest),
        limits: (usize, u64),
        now: u64,
        checkpoint: impl Fn() -> Result<()>,
        recovery: Option<&RecoveryEpoch>,
    ) -> Result<VerifyTurn> {
        let (job, progress_key) = identity;
        let (max_files, max_bytes) = limits;
        let recovery = recovery.map(|epoch| &epoch.context);
        ensure!(
            (1..=128).contains(&max_files) && (1..=512 << 20).contains(&max_bytes),
            "invalid off-query verification bounds"
        );
        if let Err(error) = checkpoint() {
            return Err(
                match crate::service::registry::AuditDrainProof::new(
                    self.registry,
                    self.lease,
                    self.bound,
                    job,
                ) {
                    Ok(proof) => error.context(proof),
                    Err(_) => error,
                },
            );
        }
        let fence = self.check()?;
        ensure!(
            self.registry.integrity_audit_veto(self.lease)? == Some(job),
            "Verify requires its durably admitted audit veto"
        );
        let selected = self.collection.select()?;
        crate::maintenance::check_writer(
            &selected.store,
            writer,
            &Quota::open_service(Path::new(&self.collection.owner.data_root))?,
        )?;
        let generation = writer.census_mutation_epoch();
        let scope = selected
            .store
            .current_owned_scope()?
            .context("verification scope missing")?;
        let read = selected.store.current_read(&scope)?;
        let head = read.head().clone();
        let mut observation = {
            let _lock = manifest_lock(&self.collection.directory)?;
            self.load(fence.0)?
        };
        if let Some(epoch) = recovery {
            self.registry.require_recovery_epoch(self.lease, epoch)?;
            ensure!(epoch.head == head.token()?, "recovery publication changed");
        }
        // Recovery never enlarges or rewrites the ordinary receipt. Only an
        // exactly matching live purpose can resume the separate bounded cursor.
        let old = if let Some(epoch) = recovery {
            self.load_recovery(fence.0)?
                .filter(|saved| &saved.epoch == epoch)
                .map(|saved| saved.progress)
        } else {
            observation.as_mut().and_then(|value| value.verify.take())
        };
        if let Some(saved) = &old {
            saved.scope.validate()?;
            if let Some(projection) = &saved.projections {
                projection.validate()?;
            }
            ensure!(
                saved.version == 1
                    && saved.after.as_ref().is_none_or(|path| !path.is_empty()
                        && path.len() <= 8192
                        && !path.contains('\0')),
                "unsupported or malformed Verify continuation"
            );
        }
        let mut progress = VerifyProgress::reconcile(
            old,
            if recovery.is_some() {
                job
            } else {
                progress_key
            },
            &head,
            now,
        );
        let audit_projections =
            recovery.is_none_or(|epoch| epoch.profile == RecoveryProfile::ProjectedV1);
        if audit_projections {
            progress.projections = Some(
                self.load_projection(fence.0, progress_key, &head)?
                    .unwrap_or_else(|| ProjectionAuditProgress::new(head.artifact_root, now)),
            );
        } else {
            // Historical baseline recovery is supported only for strict readers.
            // Projected recovery audits every actually declared component.
            let mut skipped = ProjectionAuditProgress::new(head.artifact_root, now);
            skipped.exhausted = true;
            skipped.coverage_skipped = true;
            skipped.imported.evaluated = true;
            skipped.recent_observed_unix_ms = Some(now);
            progress.projections = Some(skipped);
        }
        if observation.as_ref().is_some_and(|value| {
            value.known_damage.len() >= MAX_DAMAGE
                || value
                    .known_damage
                    .iter()
                    .any(|damage| damage.incarnation == head.scope.incarnation)
        }) {
            progress.unresolved = true;
        }
        let recorded = self.registry.recorded_integrity_damage(self.lease)?;
        if recorded.len() >= MAX_DAMAGE
            || recorded
                .iter()
                .any(|damage| damage.incarnation == head.scope.incarnation)
        {
            progress.unresolved = true;
        }
        let state_head_for_completion = head.clone();
        let files_before = progress.checked_files;
        let mut blocked_large = false;
        let mut blocked_input = None;
        let mut required_bytes = None;
        let mut interrupted = false;
        let mut check = || match checkpoint() {
            Ok(()) => Ok(()),
            Err(error) => {
                interrupted = true;
                Err(error)
            }
        };
        let mut used = 0u64;
        let work = (|| -> Result<()> {
            let mut leaf_pages = 0usize;
            (|| -> Result<()> {
                if progress.lexical_observed_unix_ms.is_none() {
                    check()?;
                    // Existing v2 auxiliary SHA streams are indivisible; this can
                    // take longer than a turn. Never called once per object page.
                    read.verify_lexical_integrity()?;
                    progress.lexical_observed_unix_ms = Some(now);
                }
                check()?;
                // A fresh CurrentRead reauthenticates the immutable manifest once;
                // only the fixed-size logical certificate/cursor survived the turn.
                if progress
                    .projections
                    .as_ref()
                    .and_then(|projection| projection.pending.as_ref())
                    .is_some()
                {
                    let pending_path = progress
                        .projections
                        .as_ref()
                        .and_then(|p| p.after.clone())
                        .context("pending projection path missing")?;
                    let manifest = read
                        .native_fact_manifest_bytes(&pending_path)?
                        .context("pending native projection disappeared")?
                        as u64;
                    if manifest > max_bytes {
                        blocked_large = true;
                        required_bytes = Some(manifest);
                        record_projection_oversized(
                            progress
                                .projections
                                .as_mut()
                                .context("projection progress missing")?,
                            &pending_path,
                            manifest,
                            max_bytes,
                            now,
                        );
                        blocked_input = Some(read.file_audit_witness(
                            &pending_path,
                            declared_audit_policy(),
                            max_bytes,
                        )?);
                        return Ok(());
                    }
                    let native = read
                        .native_fact_projection(&selected.store, &pending_path)?
                        .context("pending native projection disappeared")?;
                    used = manifest;
                    let projection = progress
                        .projections
                        .as_mut()
                        .context("projection progress missing")?;
                    projection.manifest_loads = projection
                        .manifest_loads
                        .checked_add(1)
                        .context("projection manifest count overflow")?;
                    if let Some(required) = audit_pending_projection(
                        projection,
                        &native,
                        &read,
                        &selected.store,
                        &mut used,
                        max_bytes,
                        &mut leaf_pages,
                        now,
                        &mut check,
                    )? {
                        blocked_large = true;
                        required_bytes = Some(required);
                        record_projection_oversized(
                            projection,
                            &pending_path,
                            required,
                            max_bytes,
                            now,
                        );
                        blocked_input = Some(read.file_audit_witness(
                            &pending_path,
                            declared_audit_policy(),
                            max_bytes,
                        )?);
                        return Ok(());
                    }
                    if projection.pending.is_some() {
                        return Ok(());
                    }
                }
                let projection = progress
                    .projections
                    .as_ref()
                    .context("projection progress missing")?;
                if progress.objects_complete && projection.exhausted {
                    return Ok(());
                }
                let cursor = match (progress.objects_complete, projection.exhausted) {
                    (true, false) => projection.after.clone(),
                    (false, true) => progress.after.clone(),
                    (false, false) => {
                        std::cmp::min(progress.after.clone(), projection.after.clone())
                    }
                    (true, true) => return Ok(()),
                };
                let source_path = progress
                    .projections
                    .as_ref()
                    .filter(|projection| projection.source_pending.is_some())
                    .and_then(|projection| projection.after.clone());
                let page = if let Some(path) = source_path {
                    crate::current::postings::PathPage {
                        paths: vec![path.clone()],
                        next_after: Some(path),
                    }
                } else {
                    read.lookup_paths(Lookup::Files, "", cursor.as_deref(), max_files + 1)?
                };
                let mut scanned = 0usize;
                let mut historical_observed = false;
                for path in page.paths.iter().take(max_files) {
                    check()?;
                    let historical = !progress.objects_complete
                        && progress.after.as_ref().is_none_or(|after| path > after);
                    let projection_needed = {
                        let projection = progress
                            .projections
                            .as_ref()
                            .context("projection progress missing")?;
                        !projection.exhausted
                            && (projection.source_pending.is_some()
                                || projection.after.as_ref().is_none_or(|after| path > after))
                    };
                    if !historical && !projection_needed {
                        scanned += 1;
                        continue;
                    }
                    let artifact = read
                        .artifact(path)?
                        .context("verification artifact missing")?;
                    let retained = artifact
                        .retained
                        .as_ref()
                        .context("verification extraction missing")?;
                    let object_size = (artifact.bytes as u64)
                        .checked_add(retained.object_bytes as u64)
                        .context("verification byte overflow")?;
                    let combined = if projection_needed {
                        read.projection_certification_bytes(path)?
                    } else {
                        object_size
                    };
                    let source_bytes = read.projection_source_audit_bytes(path)?;
                    let source_pending = progress
                        .projections
                        .as_ref()
                        .and_then(|projection| projection.source_pending.as_ref());
                    let required = if source_pending.is_some() {
                        combined
                            .checked_sub(source_bytes)
                            .context("source audit admission underflow")?
                    } else {
                        combined
                    };
                    if projection_needed
                        && read.file_scalars(path)?.is_some()
                        && source_pending.is_none()
                        && required > max_bytes
                        && source_bytes <= max_bytes
                        && combined.saturating_sub(source_bytes) <= max_bytes
                    {
                        if used.saturating_add(source_bytes) > max_bytes {
                            break;
                        }
                        let witness = read.certify_projection_source(&selected.store, path, now)?;
                        used = used
                            .checked_add(source_bytes)
                            .context("source audit byte overflow")?;
                        let projection = progress
                            .projections
                            .as_mut()
                            .context("projection progress missing")?;
                        projection.logical_certification_bytes = projection
                            .logical_certification_bytes
                            .checked_add(source_bytes)
                            .context("projection logical byte overflow")?;
                        projection.after = Some(path.clone());
                        projection.source_pending = Some(witness);
                        break;
                    }
                    if required > max_bytes {
                        blocked_large = true;
                        required_bytes = Some(required);
                        record_projection_oversized(
                            progress
                                .projections
                                .as_mut()
                                .context("projection progress missing")?,
                            path,
                            required,
                            max_bytes,
                            now,
                        );
                        blocked_input = Some(read.file_audit_witness(
                            path,
                            declared_audit_policy(),
                            max_bytes,
                        )?);
                        break;
                    }
                    if used.saturating_add(required) > max_bytes {
                        break;
                    }
                    if projection_needed {
                        let native = read.native_fact_projection(&selected.store, path)?;
                        let source_observed_unix_ms =
                            source_pending.map(|source| source.observed_unix_ms);
                        let certified = if let Some(source) = source_pending {
                            read.certify_file_projections_after_source(
                                &selected.store,
                                path,
                                native.as_ref(),
                                source,
                            )?
                        } else {
                            read.certify_file_projections_with_native(
                                &selected.store,
                                path,
                                native.as_ref(),
                            )?
                        };
                        used += required;
                        let projection = progress
                            .projections
                            .as_mut()
                            .context("projection progress missing")?;
                        projection.source_pending = None;
                        projection.logical_certification_bytes = projection
                            .logical_certification_bytes
                            .checked_add(required)
                            .context("projection logical byte overflow")?;
                        if native.is_some() {
                            projection.manifest_loads = projection
                                .manifest_loads
                                .checked_add(1)
                                .context("projection manifest count overflow")?;
                        }
                        if let Some(certificate) = certified.native {
                            let empty_native = certificate.pages == 0;
                            projection.after = Some(path.clone());
                            projection.pending = Some(ProjectionPendingFile {
                                scalars: certified.scalars,
                                native_lines: certified.native_lines,
                                text_ranges: certified
                                    .text_ranges
                                    .map(|certificate| PendingTextRanges::new(certificate, now)),
                                logical_certified_unix_ms: now,
                                source_observed_unix_ms,
                                certificate,
                                next_page: 0,
                                physical_first_unix_ms: empty_native.then_some(now),
                                physical_through_unix_ms: empty_native.then_some(now),
                                native_edges: certified.native_edges.map(|certificate| {
                                    ProjectionPendingComponent::new(certificate, now)
                                }),
                                native_edges_empty: certified.native_edges_empty,
                            });
                        } else {
                            projection.record(
                                path,
                                certified.scalars,
                                false,
                                false,
                                certified.native_lines,
                                (now, now),
                            )?;
                            if let Some(source_time) = source_observed_unix_ms {
                                if certified.scalars {
                                    projection.scalars.first_observed_unix_ms = Some(
                                        projection
                                            .scalars
                                            .first_observed_unix_ms
                                            .unwrap_or(source_time)
                                            .min(source_time),
                                    );
                                }
                                if certified.native_lines {
                                    projection.read_ranges.first_observed_unix_ms = Some(
                                        projection
                                            .read_ranges
                                            .first_observed_unix_ms
                                            .unwrap_or(source_time)
                                            .min(source_time),
                                    );
                                    projection.read_ranges.through_unix_ms = Some(source_time);
                                }
                            }
                        }
                        // The source/extraction checks above are also the historical
                        // object audit; do not reread them for this same file.
                        if historical {
                            progress.checked_files = progress
                                .checked_files
                                .checked_add(1)
                                .context("verification count overflow")?;
                            progress.checked_bytes = progress
                                .checked_bytes
                                .checked_add(object_size)
                                .context("verification size overflow")?;
                            progress.after = Some(path.clone());
                            progress.through_unix_ms = now;
                            historical_observed = true;
                        }
                        let projection = progress
                            .projections
                            .as_mut()
                            .context("projection progress missing")?;
                        if projection.pending.is_some() {
                            if let Some(required) = audit_pending_projection(
                                projection,
                                native
                                    .as_ref()
                                    .context("native certification reader missing")?,
                                &read,
                                &selected.store,
                                &mut used,
                                max_bytes,
                                &mut leaf_pages,
                                now,
                                &mut check,
                            )? {
                                blocked_large = true;
                                required_bytes = Some(required);
                                record_projection_oversized(
                                    projection, path, required, max_bytes, now,
                                );
                                blocked_input = Some(read.file_audit_witness(
                                    path,
                                    declared_audit_policy(),
                                    max_bytes,
                                )?);
                                break;
                            }
                            if projection.pending.is_some() {
                                break;
                            }
                        }
                    } else {
                        selected.store.verify_retained_source_deep(&artifact)?;
                        selected.store.load_retained_bounded(&artifact, max_bytes)?;
                        used += object_size;
                        progress.checked_files = progress
                            .checked_files
                            .checked_add(1)
                            .context("verification count overflow")?;
                        progress.checked_bytes = progress
                            .checked_bytes
                            .checked_add(object_size)
                            .context("verification size overflow")?;
                        progress.after = Some(path.clone());
                        progress.through_unix_ms = now;
                        historical_observed = true;
                    }
                    scanned += 1;
                    check()?;
                    if leaf_pages == 128 {
                        break;
                    }
                }
                if historical_observed {
                    progress.through_unix_ms = now;
                }
                if scanned == page.paths.len() && page.next_after.is_none() {
                    let last = page.paths.last();
                    if !progress.objects_complete
                        && last.is_none_or(|path| progress.after.as_ref() == Some(path))
                    {
                        progress.objects_complete = true;
                        if page.paths.is_empty() && progress.checked_files == 0 {
                            progress.through_unix_ms = now;
                        }
                    }
                    let projection = progress
                        .projections
                        .as_mut()
                        .context("projection progress missing")?;
                    if projection.pending.is_none()
                        && last.is_none_or(|path| projection.after.as_ref() == Some(path))
                    {
                        projection.exhausted = true;
                    }
                }
                Ok(())
            })()?;
            if progress.objects_complete
                && let Some(state) = read.collection_state()?
            {
                ensure!(
                    progress.checked_files == state.counts.files,
                    "baseline verification file coverage incomplete"
                );
                if audit_projections
                    && let Some(projection) = &progress.projections
                    && projection.exhausted
                {
                    ensure!(
                        projection.visited_files == state.counts.files,
                        "projection verification file coverage incomplete"
                    );
                }
            }
            if audit_projections
                && progress.objects_complete
                && progress
                    .projections
                    .as_ref()
                    .is_some_and(|projection| projection.exhausted)
                && !blocked_large
            {
                let projection = progress
                    .projections
                    .as_mut()
                    .context("projection progress missing")?;
                if let Some(required) = audit_imported_projection(
                    projection,
                    &read,
                    &selected.store,
                    &mut used,
                    max_bytes,
                    &mut leaf_pages,
                    now,
                    &mut check,
                )? {
                    blocked_large = true;
                    required_bytes = Some(required);
                    record_projection_oversized(
                        projection,
                        "<imported-analysis>",
                        required,
                        max_bytes,
                        now,
                    );
                    blocked_input =
                        Some(read.import_audit_witness(declared_audit_policy(), max_bytes)?);
                }
            }
            if audit_projections && !blocked_large {
                let projection = progress
                    .projections
                    .as_mut()
                    .context("projection progress missing")?;
                if projection.exhausted
                    && projection.imported.evaluated
                    && projection.recent_observed_unix_ms.is_none()
                {
                    let required = read.recent_audit_bytes()?;
                    if required > max_bytes {
                        blocked_large = true;
                        required_bytes = Some(required);
                        blocked_input =
                            Some(read.recent_audit_witness(declared_audit_policy(), max_bytes)?);
                        record_projection_oversized(
                            projection,
                            "<recent-changes>",
                            required,
                            max_bytes,
                            now,
                        );
                    } else if used.saturating_add(required) <= max_bytes {
                        check()?;
                        // Recent values are one bounded map-metadata component:
                        // <=16 MiB plus a <=64 KiB header and <=191 data chunks. The 128-page
                        // cursor above counts projection resource leaves, not
                        // authenticated map values or lexical metadata streams.
                        read.audit_recent_changes()?;
                        used = used
                            .checked_add(required)
                            .context("recent audit byte overflow")?;
                        ensure!(
                            used <= max_bytes,
                            "recent audit exceeded shared byte admission"
                        );
                        projection.recent_observed_unix_ms = Some(now);
                        projection.recent_checked_bytes = required;
                    }
                }
            }
            Ok(())
        })();
        // Partial successes precede the failing object; retain them, but never
        // turn an uncertain/failed turn into a complete clean audit.
        let work = match work {
            Err(primary) if !interrupted => {
                self.bound.revoke_native_integrity();
                progress.unresolved = true;
                if primary.is::<crate::current::PrivateIntegrityFailure>() {
                    // Only typed structural evidence is known damage. Busy,
                    // permission and other uncertain I/O retain unresolved
                    // continuation without manufacturing a corruption claim.
                    // The actual publication digest is the existing bounded
                    // component identity; both durable stores are attempted by
                    // record_damage before ordinary progress reloads sticky state.
                    match self.record_damage(&read, head.token()?.digest, now) {
                        Ok(_) => Err(primary),
                        Err(damage) => Err(primary.context(format!(
                            "Verify corruption persistence also failed: {damage:#}"
                        ))),
                    }
                } else {
                    Err(primary)
                }
            }
            other => other,
        };
        drop(read);
        let persistence = (|| -> Result<()> {
            let _lock = manifest_lock(&self.collection.directory)?;
            ensure!(
                self.check()? == fence && writer.census_mutation_epoch() == generation,
                "verification owner changed"
            );
            ensure!(
                self.registry.integrity_audit_veto(self.lease)? == Some(job),
                "verification audit admission changed"
            );
            let manifest = self.collection.manifest()?;
            let current = FileStore::new(self.collection.slot(manifest.active));
            let check = current.current_read(&scope)?;
            ensure!(
                check.token()? == head.token()?,
                "verification publication changed"
            );
            drop(check);
            // Reload to preserve concurrent sticky damage; never overwrite an
            // old saved observation with the earlier in-memory copy.
            let mut state = self
                .load(fence.0)?
                .context("anchor required before Verify")?;
            let recorded = self.registry.recorded_integrity_damage(self.lease)?;
            let known = state.known_damage.len() >= MAX_DAMAGE
                || state
                    .known_damage
                    .iter()
                    .any(|damage| damage.incarnation == head.scope.incarnation)
                || recorded.len() >= MAX_DAMAGE
                || recorded
                    .iter()
                    .any(|damage| damage.incarnation == head.scope.incarnation);
            if known {
                progress.unresolved = true;
            } else if work.is_ok()
                && progress.objects_complete
                && progress.lexical_observed_unix_ms.is_some()
                && progress
                    .projections
                    .as_ref()
                    .is_some_and(ProjectionAuditProgress::declared_complete)
            {
                // A partial new component cannot heal an earlier unresolved audit.
                progress.unresolved = false;
            }
            if let Some(epoch) = recovery {
                self.registry.require_recovery_epoch(self.lease, epoch)?;
                let receipt = RecoveryReceipt {
                    version: 1,
                    owner: self.collection.binding,
                    namespace: fence.0,
                    epoch: epoch.clone(),
                    progress: progress.clone(),
                };
                receipt.validate(self.collection.binding)?;
                write(self.collection, RECOVERY_RECEIPT, &receipt)?;
                if audit_projections {
                    let receipt = ProjectionReceipt {
                        version: 4,
                        owner: self.collection.binding,
                        namespace: fence.0,
                        job: progress_key,
                        incarnation: head.scope.incarnation,
                        progress: progress
                            .projections
                            .clone()
                            .context("recovery projection progress missing")?,
                    };
                    receipt.validate(self.collection.binding)?;
                    write(self.collection, PROJECTION_RECEIPT, &receipt)?;
                }
            } else {
                state.verify = Some(progress.clone());
                state.head = head.clone();
                state.worker_epoch = fence.1;
                write(self.collection, RECEIPT, &state)?;
                let receipt = ProjectionReceipt {
                    version: 4,
                    owner: self.collection.binding,
                    namespace: fence.0,
                    job: progress_key,
                    incarnation: head.scope.incarnation,
                    progress: progress
                        .projections
                        .clone()
                        .context("projection progress missing")?,
                };
                receipt.validate(self.collection.binding)?;
                write(self.collection, PROJECTION_RECEIPT, &receipt)?;
            }
            Ok(())
        })();
        if persistence.is_err() {
            self.bound.revoke_native_integrity();
        }
        match (work, persistence) {
            (Err(primary), Err(persist)) => {
                return Err(primary.context(format!(
                    "Verify progress persistence also failed: {persist:#}"
                )));
            }
            (Err(primary), Ok(())) if interrupted => {
                return Err(
                    match crate::service::registry::AuditDrainProof::new(
                        self.registry,
                        self.lease,
                        self.bound,
                        job,
                    ) {
                        Ok(proof) => primary.context(proof),
                        Err(_) => primary,
                    },
                );
            }
            (Err(primary), Ok(())) => return Err(primary),
            (Ok(()), Err(persist)) => return Err(persist),
            (Ok(()), Ok(())) => {}
        }
        let drained = crate::service::registry::AuditDrainProof::new(
            self.registry,
            self.lease,
            self.bound,
            job,
        )
        .ok();
        let complete_projection = audit_projections
            && progress
                .projections
                .as_ref()
                .is_some_and(ProjectionAuditProgress::complete_coverage);
        let completed = if progress.complete() && complete_projection && !blocked_large {
            Some(AuditCompletionProof {
                drained: crate::service::registry::AuditDrainProof::new(
                    self.registry,
                    self.lease,
                    self.bound,
                    job,
                )?,
                head: state_head_for_completion.clone(),
            })
        } else {
            None
        };
        let retained_completed = if audit_projections
            && progress.complete()
            && !blocked_large
            && let Some(projection) = progress.projections.as_ref()
            && !projection.coverage_skipped
            && projection.schema == 4
            && projection.exhausted
            && projection.pending.is_none()
            && projection.source_pending.is_none()
            && projection.imported.evaluated
            && projection.recent_observed_unix_ms.is_some()
        {
            projection.validate()?;
            Some(RetainedAuditCompletionProof {
                drained: crate::service::registry::AuditDrainProof::new(
                    self.registry,
                    self.lease,
                    self.bound,
                    job,
                )?,
                observation: RetainedAuditObservation {
                    version: 1,
                    policy: declared_audit_policy(),
                    head: state_head_for_completion.token()?,
                    recent_unix_ms: projection.recent_observed_unix_ms,
                    lexical_unix_ms: progress
                        .lexical_observed_unix_ms
                        .context("retained audit lexical observation missing")?,
                    source_first_unix_ms: progress.started_unix_ms,
                    source_through_unix_ms: progress.through_unix_ms,
                    checked_files: progress.checked_files,
                    checked_bytes: progress.checked_bytes,
                    scalars: projection.scalars.clone(),
                    native_facts: projection.native_facts.clone(),
                    native_edges: projection.native_edges.clone(),
                    read_ranges: projection.read_ranges.clone(),
                    imported: projection.imported.coverage.clone(),
                    authoritative_analysis: projection.imported.authoritative_digest,
                    analysis_first_unix_ms: projection.imported.authoritative_first_unix_ms,
                    analysis_through_unix_ms: projection.imported.authoritative_through_unix_ms,
                },
            })
        } else {
            None
        };
        let query_completed = retained_completed
            .as_ref()
            .filter(|_| {
                progress
                    .projections
                    .as_ref()
                    .is_some_and(ProjectionAuditProgress::declared_complete)
            })
            .map(|retained| -> Result<QueryComponentAuditProof> {
                Ok(QueryComponentAuditProof {
                    drained: crate::service::registry::AuditDrainProof::new(
                        self.registry,
                        self.lease,
                        self.bound,
                        job,
                    )?,
                    observation: retained.observation.clone(),
                })
            })
            .transpose()?;
        let recovery_completed = if progress.complete()
            && !blocked_large
            && (!audit_projections
                || progress
                    .projections
                    .as_ref()
                    .is_some_and(ProjectionAuditProgress::declared_complete))
        {
            if let Some(epoch) = recovery {
                self.registry.require_recovery_epoch(self.lease, epoch)?;
                Some(EnabledComponentAuditProof {
                    drained: crate::service::registry::AuditDrainProof::new(
                        self.registry,
                        self.lease,
                        self.bound,
                        job,
                    )?,
                    epoch: epoch.clone(),
                    head: state_head_for_completion,
                })
            } else {
                None
            }
        } else {
            None
        };
        Ok(VerifyTurn {
            verified_files: progress.checked_files.saturating_sub(files_before),
            completed,
            progress,
            blocked_large,
            blocked_input,
            required_bytes,
            admitted_bytes: max_bytes,
            charged_bytes: used,
            drained,
            recovery_completed,
            retained_completed,
            query_completed,
        })
    }
}

#[cfg(test)]
pub(crate) fn test_projection_receipt_value(
    collection: &Collection,
    head: &CurrentHead,
    namespace: Incarnation,
    job: Sha256Digest,
) -> Result<serde_json::Value> {
    serde_json::to_value(ProjectionReceipt {
        version: 4,
        owner: collection.binding,
        namespace,
        job,
        incarnation: head.scope.incarnation,
        progress: ProjectionAuditProgress::new(head.artifact_root, 1),
    })
    .map_err(Into::into)
}
#[cfg(test)]
pub(crate) fn test_write_projection_receipt(
    collection: &Collection,
    value: serde_json::Value,
) -> Result<()> {
    let receipt: ProjectionReceipt = serde_json::from_value(value)?;
    receipt.validate(collection.binding)?;
    write(collection, PROJECTION_RECEIPT, &receipt)
}

#[cfg(test)]
pub(crate) fn test_projection_progress(collection: &Collection) -> Result<ProjectionAuditProgress> {
    let receipt: ProjectionReceipt = json(&collection.directory.join(PROJECTION_RECEIPT))?;
    receipt.validate(collection.binding)?;
    Ok(receipt.progress)
}

#[cfg(test)]
pub(crate) fn test_write_recovery_receipt(
    collection: &Collection,
    value: serde_json::Value,
) -> Result<()> {
    let receipt: RecoveryReceipt = serde_json::from_value(value)?;
    receipt.validate(collection.binding)?;
    write(collection, RECOVERY_RECEIPT, &receipt)
}

#[cfg(test)]
pub(crate) fn test_recovery_receipt_bytes(value: serde_json::Value) -> Result<Vec<u8>> {
    let receipt: RecoveryReceipt = serde_json::from_value(value)?;
    Ok(serde_json::to_vec(&receipt)?)
}
/// Inert valid-format metadata for ownership tests, never a physical proof.
#[cfg(test)]
pub(crate) fn test_seed_recovery_receipt(
    collection: &Collection,
    head: &CurrentHead,
    namespace: Incarnation,
    project: airk_core::service::ProjectBinding,
) -> Result<Vec<u8>> {
    let job = Sha256Digest::from_bytes([0; 32]);
    let epoch = RecoveryContext {
        namespace,
        project,
        worker: Incarnation::from_bytes([0; 16]),
        worker_epoch: 1,
        job,
        inherited: job,
        head: head.token()?,
        root: job,
        selected: job,
        profile: NATIVE_RECOVERY_PROFILE,
    };
    let receipt = RecoveryReceipt {
        version: 1,
        owner: collection.binding,
        namespace,
        epoch,
        progress: VerifyProgress::reconcile(None, job, head, 0),
    };
    receipt.validate(collection.binding)?;
    write(collection, RECOVERY_RECEIPT, &receipt)?;
    Ok(serde_json::to_vec(&receipt)?)
}
