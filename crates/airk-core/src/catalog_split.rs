// SPDX-License-Identifier: Apache-2.0
//! Proposed P10 contracts only. Native owners authenticate, stage, reserve and execute.
use crate::{
    catalog_management::{MAX_PAGE_BYTES, MAX_RECEIPT_BYTES, catalog_operation_id},
    catalog_source_change::{SourceConfigurationSnapshot, source_job_id},
    identity::{Incarnation, RevisionToken, Sha256Digest, framed_digest},
    index_catalog::{
        CatalogRevision, IndexId, PolicyReference, QueryProjectId, ScopeAtom, atoms, encoded_size,
        label,
    },
    service_records::{ProjectRegistration, RequestKey},
    source_policy::{SourcePolicy, SplitDestination, SplitPolicyPlan},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const SPLIT_CONTRACT: u32 = 2;
pub const MAX_SPLIT_JOBS: usize = 16;
pub const MAX_SPLIT_PROJECTS: usize = 32;
pub const MAX_SPLIT_SNAPSHOT_BYTES: usize = MAX_PAGE_BYTES - 2048;
pub const MAX_SPLIT_FINALIZATION_BYTES: usize = 1024;
pub const MAX_SPLIT_RECEIPT_BYTES: usize = MAX_RECEIPT_BYTES - 2048;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "baseline_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SplitBaseline {
    Persisted {
        reference: PolicyReference,
    },
    /// Reviewed virtual all-root policy at INITIAL. It was never persisted.
    /// Source/registration/catalog revisions below bind its native derivation.
    VirtualUnrestricted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitProjectEffect {
    pub project_id: QueryProjectId,
    pub expected_scope_revision: CatalogRevision,
    pub expected_original_membership: Option<CatalogRevision>,
    pub retain_original: bool,
    /// None declines destination membership; Some is a fully reviewed scope.
    /// Membership never implicitly creates an evidence grant or changes defaults.
    pub destination_scope: Option<Vec<ScopeAtom>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitCapacityReview {
    pub destination_growth: u64,
    pub destination_temporary: u64,
    pub original_rewrite: u64,
    pub cleanup_allowance: u64,
    pub original_quota_digest: Sha256Digest,
    pub destination_quota_digest: Sha256Digest,
}
impl SplitCapacityReview {
    pub fn total(&self) -> Result<u64> {
        let total = self
            .destination_growth
            .checked_add(self.destination_temporary)
            .and_then(|value| value.checked_add(self.original_rewrite))
            .and_then(|value| value.checked_add(self.cleanup_allowance));
        let total = total
            .filter(|value| *value <= i64::MAX as u64)
            .ok_or_else(|| anyhow::anyhow!("split peak capacity overflow"))?;
        ensure!(
            self.destination_growth > 0 && self.cleanup_allowance > 0,
            "split requires destination and cleanup capacity"
        );
        Ok(total)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_policy: Option<crate::retention::SplitExtractionPolicyReference>,
    pub schema: u32,
    pub required_policy_contract: u32,
    pub caller: Incarnation,
    pub key: RequestKey,
    pub expected_catalog_revision: CatalogRevision,
    pub original_index: IndexId,
    pub original_registration: ProjectRegistration,
    pub expected_definition_revision: CatalogRevision,
    pub expected_source_revision: CatalogRevision,
    pub original_incarnation: Incarnation,
    pub original_settings_digest: Sha256Digest,
    pub source_configuration: SourceConfigurationSnapshot,
    pub baseline: SplitBaseline,
    pub before_policy_digest: Sha256Digest,
    pub selectors: Vec<ScopeAtom>,
    pub destination_name: String,
    pub destination_registration: ProjectRegistration,
    pub destination: SplitDestination,
    pub compiled_plan_digest: Sha256Digest,
    /// Native metadata requires all affected Projects plus explicitly chosen additions.
    pub projects: Vec<SplitProjectEffect>,
    pub capacity: SplitCapacityReview,
    pub require_semantic_ready: bool,
    /// Dated/native settings only; cutover may not overwrite later user changes.
    pub original_watch_digest: Sha256Digest,
    pub destination_watch_enabled: bool,
}
impl SplitRequest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.required_policy_contract
                    == if self.extraction_policy.is_some() {
                        3
                    } else {
                        SPLIT_CONTRACT
                    },
            "unsupported split contract"
        );
        self.original_registration.validate()?;
        self.destination_registration.validate()?;
        self.source_configuration.validate()?;
        if let Some(reference) = &self.extraction_policy {
            reference.validate()?;
        }
        label(&self.destination_name)?;
        atoms(&self.selectors)?;
        ensure!(
            !self.selectors.is_empty() && self.selectors.windows(2).all(|pair| pair[0] < pair[1]),
            "split selectors must be nonempty and canonically ordered"
        );
        ensure!(
            self.original_registration.binding != self.destination_registration.binding
                && self.original_registration.root == self.destination_registration.root
                && self.original_registration.root == self.source_configuration.primary_root
                && self.destination.owner_binding == self.destination_registration.binding
                && self.destination.collection == self.destination_registration.collection
                && self.original_index != self.destination.index_id,
            "split cannot change primary source or share backing identity"
        );
        self.capacity.total()?;
        ensure!(
            self.projects.len() <= MAX_SPLIT_PROJECTS,
            "split Project review exceeds capacity"
        );
        let destination_sources = self
            .destination
            .source_ids
            .values()
            .copied()
            .collect::<BTreeSet<_>>();
        let mut projects = BTreeSet::new();
        for effect in &self.projects {
            ensure!(
                projects.insert(effect.project_id),
                "duplicate split Project effect"
            );
            ensure!(
                effect.retain_original || effect.destination_scope.is_some(),
                "split Project removal needs explicit destination scope"
            );
            if let Some(scope) = &effect.destination_scope {
                atoms(scope)?;
                ensure!(
                    !scope.is_empty()
                        && scope
                            .iter()
                            .all(|atom| destination_sources.contains(&atom.source_root_id)),
                    "split destination membership scope invalid"
                );
            }
        }
        encoded_size(self, MAX_PAGE_BYTES)?;
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/catalog-split-request/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
    pub fn operation_id(&self) -> Result<Incarnation> {
        catalog_operation_id(self.caller, &self.key)
    }
    pub fn validate_plan(&self, before: &SourcePolicy, plan: &SplitPolicyPlan) -> Result<()> {
        self.validate()?;
        before.validate()?;
        plan.validate_against(before)?;
        ensure!(
            before.index_id == self.original_index
                && before.owner_binding == self.original_registration.binding
                && before.collection == self.original_registration.collection
                && before.identity()? == self.before_policy_digest,
            "split baseline binding differs"
        );
        match &self.baseline {
            SplitBaseline::Persisted { reference } => ensure!(
                reference.epoch == before.epoch && reference.digest == before.identity()?,
                "split persisted policy review stale"
            ),
            SplitBaseline::VirtualUnrestricted => ensure!(
                before.epoch == CatalogRevision::INITIAL
                    && before.positive_includes.is_none()
                    && before.denies.is_empty(),
                "virtual baseline must be explicitly unrestricted at INITIAL"
            ),
        }
        let mut aliases = std::collections::BTreeMap::new();
        for root in &before.roots {
            if root.owner {
                ensure!(
                    root.canonical_path == self.original_registration.root
                        && root.aliases.is_empty(),
                    "split primary descriptor differs"
                );
            } else {
                ensure!(
                    root.aliases.len() == 1
                        && aliases
                            .insert(root.aliases[0].clone(), root.canonical_path.clone())
                            .is_none(),
                    "split alias descriptor differs"
                );
            }
        }
        ensure!(
            aliases == self.source_configuration.aliases,
            "split source configuration differs"
        );
        ensure!(
            plan.operation_id == self.operation_id()?
                && plan.selectors == self.selectors
                && plan.destination_binding == self.destination
                && framed_digest(b"airk/catalog-split-plan/1", &[&serde_json::to_vec(plan)?])
                    == self.compiled_plan_digest,
            "split compiled plan differs from review"
        );
        // Existing/virtual before roots must be reconstructed from exact native
        // source rows at expected_source_revision; a frontend digest is no proof.
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitStage {
    Accepted,
    DestinationPrepared,
    Building,
    DestinationValidated,
    OriginalFenced,
    RemainderPublished,
    MembershipApplied,
    CleanupPending,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitJobPurpose {
    DestinationBuild,
    OriginalWitness,
    OriginalRemoval,
    DestinationCleanup,
    OriginalCleanup,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitJobRef {
    pub purpose: SplitJobPurpose,
    pub caller: Incarnation,
    pub key: RequestKey,
    pub backing: Sha256Digest,
    pub job: Sha256Digest,
    pub engine_identity: Sha256Digest,
    /// Native claim must compare exact saved ConfigurationWork/input lineage.
    pub configuration_digest: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum SplitRunState {
    Pending,
    Running,
    NeedsAttention { code: String },
    CanceledBeforeFence { cleanup_pending: bool },
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitRemainderWitness {
    #[serde(default)]
    pub job: Option<Sha256Digest>,
    pub index_id: IndexId,
    pub before_policy_digest: Sha256Digest,
    pub publication: RevisionToken,
    pub source_root_id: crate::index_catalog::SourceRootId,
    pub logical_path: String,
    pub source_relative_path: String,
    pub expected_hash: Sha256Digest,
    pub observed_from_unix_ms: u64,
    pub observed_to_unix_ms: u64,
    pub bytes_read: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitReceipt {
    pub schema: u32,
    pub operation_id: Incarnation,
    pub request_digest: Sha256Digest,
    pub revision: CatalogRevision,
    /// Exact predecessor for the native compare-and-swap transition.
    pub previous_receipt_digest: Option<Sha256Digest>,
    pub catalog_revision: CatalogRevision,
    pub stage: SplitStage,
    pub state: SplitRunState,
    #[serde(default)]
    pub cleanup_attention: Option<String>,
    #[serde(default)]
    pub terminal_publication_slots: std::collections::BTreeMap<IndexId, Sha256Digest>,
    /// All component owner allocations must already be held before build starts.
    pub capacity_receipt: Option<Sha256Digest>,
    /// Actual immutable root-allocation identities. Partial funding remains
    /// owned by Accepted intent and never permits document capture.
    pub funding_receipts: std::collections::BTreeMap<String, Sha256Digest>,
    pub destination_storage: Option<Sha256Digest>,
    pub destination_publication: Option<RevisionToken>,
    pub destination_validation: Option<Sha256Digest>,
    pub remainder_witness: Option<SplitRemainderWitness>,
    pub original_fence: Option<Sha256Digest>,
    pub remainder_publication: Option<RevisionToken>,
    pub membership_receipt: Option<Sha256Digest>,
    /// Native proof that owned physical work/journals are reconciled. This
    /// authorizes releasing unused funding, but is not operation completion.
    pub physical_cleanup_authorization: Option<Sha256Digest>,
    pub funding_release_receipts: std::collections::BTreeMap<String, Sha256Digest>,
    pub cleanup_receipt: Option<Sha256Digest>,
    pub cancellation_cleanup_receipt: Option<Sha256Digest>,
    pub jobs: Vec<SplitJobRef>,
}
impl SplitReceipt {
    pub fn supports_metadata_continuation(&self) -> bool {
        self.stage != SplitStage::Complete
            && !matches!(
                self.state,
                SplitRunState::Complete
                    | SplitRunState::CanceledBeforeFence {
                        cleanup_pending: false
                    }
            )
    }
    pub fn validate_for(&self, request: &SplitRequest) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.operation_id == request.operation_id()?
                && self.request_digest == request.identity()?
                && self.catalog_revision >= request.expected_catalog_revision,
            "split receipt identity conflict"
        );
        ensure!(
            self.previous_receipt_digest.is_none() == (self.revision == CatalogRevision::INITIAL),
            "split receipt predecessor differs"
        );
        if self.revision == CatalogRevision::INITIAL {
            ensure!(
                self.stage == SplitStage::Accepted,
                "initial split receipt must be accepted"
            );
        }
        if let Some(code) = &self.cleanup_attention {
            ensure!(
                matches!(
                    self.state,
                    SplitRunState::CanceledBeforeFence {
                        cleanup_pending: true
                    }
                ) && !code.is_empty()
                    && code.len() <= 128
                    && code.bytes().all(|byte| byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'_'),
                "invalid split cleanup attention latch"
            );
        }
        ensure!(
            self.terminal_publication_slots.len() <= 2
                && self
                    .terminal_publication_slots
                    .keys()
                    .all(|index| *index == request.original_index
                        || *index == request.destination.index_id),
            "split terminal publication slot owner differs"
        );
        let required = |stage, present| -> Result<()> {
            ensure!(
                present == (self.stage >= stage),
                "split stage proof mismatch"
            );
            Ok(())
        };
        ensure!(
            self.stage < SplitStage::DestinationPrepared || self.destination_storage.is_some(),
            "split destination metadata proof absent"
        );
        let roots = [
            &request.original_registration.data_root,
            &request.destination_registration.data_root,
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        ensure!(
            self.funding_receipts.len() <= roots.len()
                && self
                    .funding_receipts
                    .keys()
                    .all(|root| roots.contains(root)),
            "split funding owner differs"
        );
        ensure!(
            self.capacity_receipt.is_none() || self.funding_receipts.len() == roots.len(),
            "split complete capacity lacks root receipts"
        );
        ensure!(
            self.stage < SplitStage::DestinationPrepared || self.capacity_receipt.is_some(),
            "split cannot build before complete funding"
        );
        required(
            SplitStage::DestinationValidated,
            self.destination_publication.is_some() && self.destination_validation.is_some(),
        )?;
        ensure!(
            self.destination_publication.is_some() == self.destination_validation.is_some(),
            "partial destination proof"
        );
        if let Some(witness) = &self.remainder_witness {
            crate::index_catalog::relative_path(&witness.source_relative_path, false)?;
            ensure!(
                witness.index_id == request.original_index
                    && witness.before_policy_digest == request.before_policy_digest
                    && witness.logical_path.len() <= 4096
                    && !witness.logical_path.is_empty()
                    && witness.observed_from_unix_ms > 0
                    && witness.observed_to_unix_ms >= witness.observed_from_unix_ms,
                "split remainder witness identity differs"
            );
        }
        ensure!(
            self.stage < SplitStage::OriginalFenced || self.remainder_witness.is_some(),
            "split cutover requires nonempty live remainder witness"
        );
        required(SplitStage::OriginalFenced, self.original_fence.is_some())?;
        required(
            SplitStage::RemainderPublished,
            self.remainder_publication.is_some(),
        )?;
        required(
            SplitStage::MembershipApplied,
            self.membership_receipt.is_some(),
        )?;
        required(SplitStage::Complete, self.cleanup_receipt.is_some())?;
        ensure!(
            self.funding_release_receipts
                .keys()
                .all(|root| self.funding_receipts.contains_key(root)),
            "split release names unfunded owner"
        );
        ensure!(
            self.funding_release_receipts.is_empty()
                || self.physical_cleanup_authorization.is_some(),
            "split funding released before physical cleanup authority"
        );
        if self.stage == SplitStage::Complete
            || matches!(
                self.state,
                SplitRunState::CanceledBeforeFence {
                    cleanup_pending: false
                }
            )
        {
            ensure!(
                self.physical_cleanup_authorization.is_some()
                    && self.funding_release_receipts.len() == self.funding_receipts.len(),
                "split terminal state has unreleased funding"
            );
        }
        ensure!(
            (self.stage == SplitStage::Complete) == matches!(self.state, SplitRunState::Complete),
            "split terminal state differs from stage"
        );
        ensure!(
            self.cancellation_cleanup_receipt.is_some()
                == matches!(
                    self.state,
                    SplitRunState::CanceledBeforeFence {
                        cleanup_pending: false
                    }
                ),
            "split cancellation cleanup proof differs"
        );
        match &self.state {
            SplitRunState::CanceledBeforeFence { .. } => ensure!(
                self.stage < SplitStage::OriginalFenced,
                "post-fence cancellation cannot undo transfer"
            ),
            SplitRunState::NeedsAttention { code } => ensure!(
                !code.is_empty()
                    && code.len() <= 128
                    && code.bytes().all(|byte| byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'_'),
                "invalid split failure code"
            ),
            SplitRunState::Complete => ensure!(
                self.stage == SplitStage::Complete,
                "split completion requires settled cleanup"
            ),
            _ => ensure!(
                self.stage != SplitStage::Complete,
                "terminal split stage requires complete state"
            ),
        }
        ensure!(
            self.jobs.len() <= MAX_SPLIT_JOBS,
            "split job receipt exceeds capacity"
        );
        let mut jobs = BTreeSet::new();
        for work in &self.jobs {
            ensure!(
                work.key.namespace == request.key.namespace
                    && work.job == source_job_id(work.backing, work.caller, &work.key)?
                    && jobs.insert(work.job)
                    && work.backing
                        == match work.purpose {
                            SplitJobPurpose::DestinationBuild
                            | SplitJobPurpose::DestinationCleanup =>
                                request.destination_registration.binding,
                            _ => request.original_registration.binding,
                        },
                "split job identity conflict"
            );
        }
        encoded_size(self, MAX_SPLIT_RECEIPT_BYTES)?;
        Ok(())
    }
    pub fn identity(&self, request: &SplitRequest) -> Result<Sha256Digest> {
        self.validate_for(request)?;
        Ok(framed_digest(
            b"airk/catalog-split-receipt/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitRecoveryAction {
    Resume,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitRecoveryRequest {
    pub caller: Incarnation,
    pub key: RequestKey,
    pub operation_id: Incarnation,
    pub request_digest: Sha256Digest,
    pub expected_receipt_digest: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_finalization_revision: Option<u64>,
    pub action: SplitRecoveryAction,
}
impl SplitRecoveryRequest {
    pub fn validate_for(&self, request: &SplitRequest, receipt: &SplitReceipt) -> Result<()> {
        ensure!(
            self.expected_finalization_revision
                .is_none_or(|revision| revision > 0),
            "invalid split finalization revision"
        );
        ensure!(
            self.key.namespace == request.key.namespace
                && self.operation_id == receipt.operation_id
                && self.request_digest == request.identity()?
                && self.expected_receipt_digest == receipt.identity(request)?,
            "split recovery review changed"
        );
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum SplitRecoveryOutcome {
    ResumeAccepted {
        accepted_job: Sha256Digest,
        receipt_digest: Sha256Digest,
        resulting_receipt: Box<SplitReceipt>,
    },
    /// Exact metadata continuation when no runnable worker belongs to this
    /// phase. Status reads never manufacture this accepted action.
    ContinuationAccepted {
        receipt_digest: Sha256Digest,
        resulting_receipt: Box<SplitReceipt>,
    },
    /// Clears only the exact frozen-plan attention latch; the immutable terminal
    /// bytes and current receipt remain unchanged through partial publication.
    FinalizationAccepted {
        receipt_digest: Sha256Digest,
        terminal_receipt_digest: Sha256Digest,
        terminal_receipt: Box<SplitReceipt>,
        finalization_revision: u64,
    },
    CancelBeforeFenceAccepted {
        receipt_digest: Sha256Digest,
        cleanup_pending: bool,
        resulting_receipt: Box<SplitReceipt>,
    },
    /// This is a refusal to undo the fence, not successful cancellation.
    RestrictiveCompletionRequired {
        receipt_digest: Sha256Digest,
    },
    AlreadyComplete {
        receipt_digest: Sha256Digest,
    },
    Unknown {
        caller: Incarnation,
        key: RequestKey,
    },
}

/// Exact recovery acknowledgment. Retain it independently of a later operation
/// observation; read-only receipt lookup never resubmits the request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitRecoveryReceipt {
    pub request: SplitRecoveryRequest,
    pub request_digest: Sha256Digest,
    pub outcome: SplitRecoveryOutcome,
}
impl SplitRecoveryReceipt {
    pub fn validate_for(&self, request: &SplitRequest, reviewed: &SplitReceipt) -> Result<()> {
        self.request.validate_for(request, reviewed)?;
        ensure!(
            self.request_digest
                == framed_digest(
                    b"airk/catalog-split-recovery/1",
                    &[&serde_json::to_vec(&self.request)?]
                ),
            "split recovery receipt identity differs"
        );
        let reviewed_digest = reviewed.identity(request)?;
        let successor = |next: &SplitReceipt, digest: Sha256Digest| -> Result<()> {
            next.validate_for(request)?;
            ensure!(
                digest == next.identity(request)?
                    && next.previous_receipt_digest == Some(reviewed_digest)
                    && next.revision == reviewed.revision.next()?
                    && next.stage >= reviewed.stage
                    && next.catalog_revision >= reviewed.catalog_revision
                    && (reviewed.capacity_receipt.is_none()
                        || next.capacity_receipt == reviewed.capacity_receipt)
                    && reviewed
                        .funding_receipts
                        .iter()
                        .all(|(root, digest)| next.funding_receipts.get(root) == Some(digest))
                    && reviewed
                        .funding_release_receipts
                        .iter()
                        .all(|(root, digest)| next.funding_release_receipts.get(root)
                            == Some(digest))
                    && reviewed
                        .terminal_publication_slots
                        .iter()
                        .all(|(index, digest)| next.terminal_publication_slots.get(index)
                            == Some(digest))
                    && next.jobs.starts_with(&reviewed.jobs),
                "split recovery successor lineage differs"
            );
            for (old, new) in [
                (reviewed.destination_storage, next.destination_storage),
                (reviewed.destination_validation, next.destination_validation),
                (reviewed.original_fence, next.original_fence),
                (reviewed.membership_receipt, next.membership_receipt),
                (reviewed.cleanup_receipt, next.cleanup_receipt),
                (
                    reviewed.physical_cleanup_authorization,
                    next.physical_cleanup_authorization,
                ),
                (
                    reviewed.cancellation_cleanup_receipt,
                    next.cancellation_cleanup_receipt,
                ),
            ] {
                ensure!(
                    old.is_none() || old == new,
                    "split recovery changed an existing proof"
                );
            }
            ensure!(
                reviewed.remainder_witness.is_none()
                    || reviewed.remainder_witness == next.remainder_witness,
                "split recovery changed remainder witness"
            );
            ensure!(
                reviewed.destination_publication.is_none()
                    || reviewed.destination_publication == next.destination_publication,
                "split recovery changed destination publication"
            );
            ensure!(
                reviewed.remainder_publication.is_none()
                    || reviewed.remainder_publication == next.remainder_publication,
                "split recovery changed remainder publication"
            );
            Ok(())
        };
        match &self.outcome {
            SplitRecoveryOutcome::ResumeAccepted {
                accepted_job,
                receipt_digest,
                resulting_receipt,
            } => {
                ensure!(
                    self.request.action == SplitRecoveryAction::Resume
                        && reviewed.stage != SplitStage::Complete
                        && !matches!(
                            reviewed.state,
                            SplitRunState::CanceledBeforeFence {
                                cleanup_pending: false
                            }
                        ),
                    "unexpected split resume acknowledgment"
                );
                successor(resulting_receipt, *receipt_digest)?;
                let work = resulting_receipt
                    .jobs
                    .last()
                    .ok_or_else(|| anyhow::anyhow!("split resume lacks accepted job"))?;
                ensure!(
                    resulting_receipt.jobs.len() == reviewed.jobs.len() + 1
                        && work.job == *accepted_job
                        && work.caller == self.request.caller
                        && work.key == self.request.key
                        && !reviewed.jobs.iter().any(|job| job.job == *accepted_job),
                    "split resume accepted job differs"
                );
                if matches!(
                    reviewed.state,
                    SplitRunState::CanceledBeforeFence {
                        cleanup_pending: true
                    }
                ) {
                    ensure!(
                        work.purpose == SplitJobPurpose::DestinationCleanup
                            && resulting_receipt.stage == reviewed.stage
                            && resulting_receipt.state
                                == (SplitRunState::CanceledBeforeFence {
                                    cleanup_pending: true
                                }),
                        "canceled split may resume destination cleanup only"
                    );
                } else {
                    ensure!(
                        matches!(
                            resulting_receipt.state,
                            SplitRunState::Pending | SplitRunState::Running
                        ) && match work.purpose {
                            SplitJobPurpose::DestinationBuild =>
                                resulting_receipt.stage >= SplitStage::DestinationPrepared
                                    && resulting_receipt.stage < SplitStage::DestinationValidated,
                            SplitJobPurpose::OriginalWitness =>
                                resulting_receipt.stage == SplitStage::DestinationValidated,
                            SplitJobPurpose::OriginalRemoval =>
                                resulting_receipt.stage >= SplitStage::OriginalFenced
                                    && resulting_receipt.stage < SplitStage::RemainderPublished,
                            SplitJobPurpose::OriginalCleanup =>
                                resulting_receipt.stage >= SplitStage::MembershipApplied
                                    && resulting_receipt.stage < SplitStage::Complete,
                            SplitJobPurpose::DestinationCleanup => false,
                        },
                        "split resume purpose differs from phase"
                    );
                }
            }
            SplitRecoveryOutcome::FinalizationAccepted {
                receipt_digest,
                terminal_receipt_digest,
                terminal_receipt,
                finalization_revision,
            } => {
                ensure!(
                    self.request.action == SplitRecoveryAction::Resume
                        && *receipt_digest == reviewed_digest
                        && self
                            .request
                            .expected_finalization_revision
                            .is_some_and(
                                |revision| revision.checked_add(1) == Some(*finalization_revision)
                            ),
                    "split frozen finalization acknowledgment differs"
                );
                if reviewed.stage == SplitStage::Complete
                    || matches!(
                        reviewed.state,
                        SplitRunState::CanceledBeforeFence {
                            cleanup_pending: false
                        }
                    )
                {
                    ensure!(
                        terminal_receipt.as_ref() == reviewed
                            && *terminal_receipt_digest == reviewed_digest,
                        "split acknowledgment target changed completed receipt"
                    );
                } else {
                    successor(terminal_receipt, *terminal_receipt_digest)?;
                }
                ensure!(
                    terminal_receipt.jobs == reviewed.jobs
                        && (terminal_receipt.stage == SplitStage::Complete
                            || matches!(
                                terminal_receipt.state,
                                SplitRunState::CanceledBeforeFence {
                                    cleanup_pending: false
                                }
                            )),
                    "split frozen finalization target is not terminal"
                );
            }
            SplitRecoveryOutcome::ContinuationAccepted {
                receipt_digest,
                resulting_receipt,
            } => {
                ensure!(
                    self.request.action == SplitRecoveryAction::Resume
                        && reviewed.supports_metadata_continuation(),
                    "unexpected split metadata continuation acknowledgment"
                );
                successor(resulting_receipt, *receipt_digest)?;
                let mut expected = reviewed.clone();
                expected.revision = reviewed.revision.next()?;
                expected.previous_receipt_digest = Some(reviewed_digest);
                expected.cleanup_attention = None;
                expected.state = if matches!(
                    reviewed.state,
                    SplitRunState::CanceledBeforeFence {
                        cleanup_pending: true
                    }
                ) {
                    reviewed.state.clone()
                } else {
                    SplitRunState::Pending
                };
                expected.catalog_revision = resulting_receipt.catalog_revision;
                ensure!(
                    resulting_receipt.as_ref() == &expected,
                    "split metadata continuation changed its phase, proofs or jobs"
                );
            }
            SplitRecoveryOutcome::CancelBeforeFenceAccepted {
                receipt_digest,
                cleanup_pending,
                resulting_receipt,
            } => {
                ensure!(
                    self.request.action == SplitRecoveryAction::Cancel
                        && reviewed.stage < SplitStage::OriginalFenced,
                    "unexpected split cancellation acknowledgment"
                );
                successor(resulting_receipt, *receipt_digest)?;
                ensure!(
                    resulting_receipt.stage == reviewed.stage
                        && resulting_receipt.state
                            == (SplitRunState::CanceledBeforeFence {
                                cleanup_pending: *cleanup_pending
                            }),
                    "split cancellation outcome differs from successor"
                );
                let added = &resulting_receipt.jobs[reviewed.jobs.len()..];
                ensure!(
                    added.len() <= 1
                        && added
                            .iter()
                            .all(|work| work.purpose == SplitJobPurpose::DestinationCleanup
                                && work.caller == self.request.caller
                                && work.key == self.request.key),
                    "split cancellation appended unrelated work"
                );
            }
            SplitRecoveryOutcome::RestrictiveCompletionRequired { receipt_digest } => ensure!(
                self.request.action == SplitRecoveryAction::Cancel
                    && reviewed.stage >= SplitStage::OriginalFenced
                    && reviewed.stage < SplitStage::Complete
                    && *receipt_digest == reviewed_digest,
                "unexpected split restrictive completion acknowledgment"
            ),
            SplitRecoveryOutcome::AlreadyComplete { receipt_digest } => ensure!(
                (reviewed.stage == SplitStage::Complete
                    || matches!(
                        reviewed.state,
                        SplitRunState::CanceledBeforeFence {
                            cleanup_pending: false
                        }
                    ))
                    && *receipt_digest == reviewed_digest,
                "split completion acknowledgment differs"
            ),
            SplitRecoveryOutcome::Unknown { caller, key } => ensure!(
                *caller == self.request.caller && *key == self.request.key,
                "split recovery uncertainty crossed request"
            ),
        }
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
}

/// The destination is sealed while building. The original reference is only a
/// required-capability/active-operation marker until the exact stage fence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitOwnerRole {
    OriginalRemainder,
    Destination,
}

/// These are additional split constraints, never alternatives to ordinary scope,
/// publication, source-policy, grant and delivery checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitEvidenceConstraint {
    PreserveOriginalAdmission,
    SealedDestination,
    SealedCutover,
    ReviewedActive,
}

/// Persist destination requirement before staged bytes exist. An original marker
/// may be installed early without denying its existing reads. Older runtimes
/// still refuse unsupported required capabilities. Keep capability after cutover.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitOwnerReference {
    pub required_contract: u32,
    pub operation_id: Incarnation,
    pub request_digest: Sha256Digest,
    pub index_id: IndexId,
    pub role: SplitOwnerRole,
    /// Cutover activation proof; None alone does NOT seal an original index.
    /// Internal builders require native accepted-operation authority, never a
    /// public bypass. Missing current native lifecycle proof fails closed.
    pub activation_receipt: Option<Sha256Digest>,
}
impl SplitOwnerReference {
    pub fn validate_for(&self, request: &SplitRequest, receipt: &SplitReceipt) -> Result<()> {
        receipt.validate_for(request)?;
        let expected_index = match self.role {
            SplitOwnerRole::OriginalRemainder => request.original_index,
            SplitOwnerRole::Destination => request.destination.index_id,
        };
        ensure!(
            self.required_contract == SPLIT_CONTRACT
                && self.operation_id == receipt.operation_id
                && self.request_digest == receipt.request_digest
                && self.index_id == expected_index,
            "split owner requirement differs"
        );
        if self.activation_receipt.is_some() {
            ensure!(
                receipt.stage >= SplitStage::MembershipApplied
                    && self.activation_receipt == receipt.membership_receipt,
                "split owner activated before reviewed cutover"
            );
        }
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
    pub fn evidence_constraint(
        &self,
        request: &SplitRequest,
        receipt: &SplitReceipt,
    ) -> Result<SplitEvidenceConstraint> {
        self.validate_for(request, receipt)?;
        if self.activation_receipt.is_some() {
            return Ok(SplitEvidenceConstraint::ReviewedActive);
        }
        Ok(match self.role {
            SplitOwnerRole::Destination => SplitEvidenceConstraint::SealedDestination,
            SplitOwnerRole::OriginalRemainder if receipt.stage < SplitStage::OriginalFenced => {
                SplitEvidenceConstraint::PreserveOriginalAdmission
            }
            SplitOwnerRole::OriginalRemainder => SplitEvidenceConstraint::SealedCutover,
        })
    }
}

/// Complete bounded metadata used by trusted draft preparation. Filesystem,
/// ownership and current settings qualification remain native work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitSourceState {
    pub catalog_revision: CatalogRevision,
    pub index: crate::index_catalog::IndexDefinition,
    pub policy: SourcePolicy,
    pub baseline: SplitBaseline,
    pub source_configuration: SourceConfigurationSnapshot,
    pub watch_digest: Sha256Digest,
    pub affected_projects: Vec<SplitProjectEffect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitFinalizationProgress {
    Prepared,
    Publishing,
    Acknowledging,
    Complete,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitFinalizationStatus {
    pub schema: u32,
    pub revision: u64,
    pub terminal_receipt_digest: Sha256Digest,
    pub terminal_receipt_revision: CatalogRevision,
    pub slots: std::collections::BTreeMap<IndexId, Sha256Digest>,
    pub progress: SplitFinalizationProgress,
    pub attention: Option<String>,
    #[serde(default)]
    pub funding_acknowledged: std::collections::BTreeSet<Sha256Digest>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitSnapshot {
    pub request: SplitRequest,
    pub baseline: SourcePolicy,
    pub plan: SplitPolicyPlan,
    pub receipt: SplitReceipt,
    pub receipt_digest: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finalization: Option<SplitFinalizationStatus>,
}
/// Admission reserves response space for the largest terminal receipt before
/// accepting immutable request/policy/plan data. Recovery adds at most 16 KiB.
pub fn validate_snapshot_admission(
    request: &SplitRequest,
    baseline: &SourcePolicy,
    plan: &SplitPolicyPlan,
) -> Result<()> {
    let envelope = serde_json::json!({"request":request,"baseline":baseline,"plan":plan,"receipt":null,"receipt_digest":"0".repeat(64)});
    let maximum = serde_json::to_vec(&envelope)?
        .len()
        .checked_sub(4)
        .and_then(|bytes| bytes.checked_add(MAX_RECEIPT_BYTES))
        .and_then(|bytes| bytes.checked_add(MAX_SPLIT_FINALIZATION_BYTES))
        .ok_or_else(|| anyhow::anyhow!("split terminal frame preflight overflow"))?;
    ensure!(
        maximum <= MAX_SPLIT_SNAPSHOT_BYTES,
        "split immutable snapshot leaves insufficient terminal receipt/frame capacity"
    );
    Ok(())
}
impl SplitSnapshot {
    pub fn validate(&self) -> Result<()> {
        self.request.validate_plan(&self.baseline, &self.plan)?;
        self.receipt.validate_for(&self.request)?;
        ensure!(
            self.receipt_digest == self.receipt.identity(&self.request)?,
            "split native receipt digest differs"
        );
        if let Some(finalization) = &self.finalization {
            encoded_size(finalization, MAX_SPLIT_FINALIZATION_BYTES)?;
            ensure!(
                finalization.schema == 1
                    && finalization.revision > 0
                    && finalization.slots == self.receipt.terminal_publication_slots,
                "split finalization envelope identity differs"
            );
            ensure!(
                finalization
                    .attention
                    .as_ref()
                    .is_none_or(|code| !code.is_empty()
                        && code.len() <= 128
                        && code.bytes().all(|byte| byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || byte == b'_')),
                "invalid split finalization attention"
            );
            let terminal = self.receipt.stage == SplitStage::Complete
                || matches!(
                    self.receipt.state,
                    SplitRunState::CanceledBeforeFence {
                        cleanup_pending: false
                    }
                );
            ensure!(
                finalization.funding_acknowledged.iter().all(|digest| self
                    .receipt
                    .funding_release_receipts
                    .values()
                    .any(|release| release == digest)),
                "finalization acknowledged unknown funding release"
            );
            if terminal {
                ensure!(
                    matches!(
                        finalization.progress,
                        SplitFinalizationProgress::Acknowledging
                            | SplitFinalizationProgress::Complete
                    ) && finalization.terminal_receipt_digest == self.receipt_digest
                        && finalization.terminal_receipt_revision == self.receipt.revision,
                    "split completed publication differs"
                );
                if finalization.progress == SplitFinalizationProgress::Complete {
                    ensure!(
                        finalization.funding_acknowledged.len()
                            == self.receipt.funding_release_receipts.len(),
                        "split terminal acknowledgment remains pending"
                    );
                }
            } else {
                ensure!(
                    matches!(
                        finalization.progress,
                        SplitFinalizationProgress::Prepared | SplitFinalizationProgress::Publishing
                    ) && finalization.terminal_receipt_revision == self.receipt.revision.next()?,
                    "split frozen finalization revision differs"
                );
            }
        }
        validate_snapshot_admission(&self.request, &self.baseline, &self.plan)?;
        // Recovery ack <=16 KiB plus this snapshot leaves >=2 KiB envelope room.
        encoded_size(self, MAX_SPLIT_SNAPSHOT_BYTES)?;
        Ok(())
    }
}

/// Typed native job result. Removal uses the existing P9 exact policy report;
/// a destination proof names its actual independent current publication.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitJobResult {
    pub schema: u32,
    pub operation_id: Incarnation,
    pub request_digest: Sha256Digest,
    pub purpose: SplitJobPurpose,
    pub publication: Option<RevisionToken>,
    pub policy_digest: Sha256Digest,
    pub selected_sources: u64,
    pub actual_bytes: Option<u64>,
    pub semantic_ready: bool,
    pub cleanup_complete: bool,
    /// Qualified from the actual retained capsule by the native Store adapter.
    pub policy_application: Option<crate::catalog_policy::SourcePolicyApplyReport>,
    #[serde(default)]
    pub remainder_witness: Option<SplitRemainderWitness>,
}
impl SplitJobResult {
    pub fn validate_for(&self, request: &SplitRequest, plan: &SplitPolicyPlan) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.operation_id == request.operation_id()?
                && self.request_digest == request.identity()?,
            "split job result identity differs"
        );
        if self.purpose == SplitJobPurpose::OriginalWitness {
            let witness = self
                .remainder_witness
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("split original witness result missing"))?;
            ensure!(
                self.policy_digest == request.before_policy_digest
                    && self.publication == Some(witness.publication)
                    && witness.index_id == request.original_index
                    && witness.before_policy_digest == request.before_policy_digest
                    && self.selected_sources == 1
                    && self.actual_bytes == Some(witness.bytes_read)
                    && self.policy_application.is_none()
                    && !self.cleanup_complete,
                "split original witness result differs"
            );
            crate::index_catalog::relative_path(&witness.source_relative_path, false)?;
            ensure!(
                plan.remainder
                    .allows_canonical(witness.source_root_id, &witness.source_relative_path),
                "split witness result outside remainder"
            );
        } else if self.purpose == SplitJobPurpose::DestinationBuild {
            ensure!(
                self.publication.is_some()
                    && self.policy_digest == plan.destination.identity()?
                    && self.selected_sources > 0
                    && (!request.require_semantic_ready || self.semantic_ready),
                "split destination validation incomplete"
            );
        } else {
            let expected = if self.purpose == SplitJobPurpose::DestinationCleanup {
                plan.destination.identity()?
            } else {
                plan.remainder.identity()?
            };
            ensure!(
                self.policy_digest == expected,
                "split job result policy differs"
            );
        }
        ensure!(
            self.purpose == SplitJobPurpose::OriginalWitness || self.remainder_witness.is_none(),
            "unrelated split job contains witness result"
        );
        if let Some(report) = &self.policy_application {
            report.validate()?;
            ensure!(
                report.policy_digest == self.policy_digest,
                "split policy application digest differs"
            );
        }
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
}
