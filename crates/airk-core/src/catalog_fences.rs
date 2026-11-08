// SPDX-License-Identifier: Apache-2.0
//! Pure complete catalog fence wire contracts and bounded identity validation.
//! These checks do not authenticate native worker acknowledgements, process
//! reaping, or an installed-authority census. Native adapters supply that proof.
use crate::{
    catalog_management::{
        CatalogOperation, CatalogReceipt, CatalogRequest, MAX_RECEIPT_BYTES, catalog_operation_id,
    },
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{IndexId, encoded_size},
    service::{ProjectBinding, ScopedReadRef, WorkerIdentity},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

const MAX_TARGETS: usize = 128;
pub const MAX_NATIVE_RETIREMENT_EVIDENCE_BYTES: usize = 2 * MAX_RECEIPT_BYTES;

/// Readonly native recovery observation for explicit LOCAL journal settlement.
/// This is neither a new native-work proof nor permission to delete storage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeIndexRetirementEvidence {
    pub catalog_clock: crate::service::CatalogClock,
    pub index: crate::index_catalog::IndexDefinition,
    pub retirement: FencedReceipt,
}
impl NativeIndexRetirementEvidence {
    pub fn validate(&self) -> Result<()> {
        self.index.validate()?;
        self.retirement.validate()?;
        let CatalogOperation::RetireIndex {
            id,
            expected_definition_revision,
            expected_source_revision,
            ..
        } = &self.retirement.request.operation
        else {
            anyhow::bail!("native retirement evidence requires a canonical Retire receipt");
        };
        let obligation = self
            .retirement
            .proof
            .worker_obligation
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("native retirement census is missing"))?;
        ensure!(
            self.index.tombstone
                && self.index.id == *id
                && self.index.definition_revision == expected_definition_revision.next()?
                && self.index.source_revision == expected_source_revision.next()?
                && obligation.index_id == *id
                && obligation.backing == self.index.backing.binding
                && self.catalog_clock.namespace == self.retirement.request.key.namespace
                && self.catalog_clock.revision >= self.retirement.metadata.catalog_revision,
            "native retirement evidence lineage changed"
        );
        // Historical inactive retirement need not claim that missing source
        // import metadata became complete. Preserve the full native index.
        encoded_size(self, MAX_NATIVE_RETIREMENT_EVIDENCE_BYTES)?;
        Ok(())
    }
}

/// A known rejection before native preparation reserves a durable receipt. This
/// marker alone never authorizes reopening: the native coordinator must also
/// prove exact receipt absence, unchanged namespace/revision and zero effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FencePrepareRejected {
    Capacity,
    StaleReview,
    SourceBusy,
}
impl std::fmt::Display for FencePrepareRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Capacity => "catalog receipt capacity exceeded",
            Self::StaleReview => "catalog review stale",
            Self::SourceBusy => "source change requires accepted jobs to settle",
        })
    }
}
impl std::error::Error for FencePrepareRejected {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FenceTarget {
    pub worker: WorkerIdentity,
    pub scope: ScopedReadRef,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FenceConfirmationMethod {
    NativeScopeAck,
    NativeWorkerReap,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FenceConfirmation {
    pub target: FenceTarget,
    pub method: FenceConfirmationMethod,
}
/// Exact mutating-worker census captured under closed native start admission.
/// An empty census still binds retirement to its native index and backing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerDrainObligation {
    pub index_id: IndexId,
    pub backing: ProjectBinding,
    pub workers: Vec<WorkerIdentity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<WorkerProviderObligation>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerProviderObligation {
    pub worker: WorkerIdentity,
    pub history: WorkerProviderHistory,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerProviderHistory {
    NativeNeverAuthorized,
    Helpers {
        generations: Vec<Incarnation>,
    },
    InProcess {
        generations: Vec<Incarnation>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        completed_authorizations: Option<crate::inference_provider::CompletionLedgerObservation>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerFenceConfirmationMethod {
    NativeWorkerReap,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerFenceConfirmation {
    pub worker: WorkerIdentity,
    pub method: WorkerFenceConfirmationMethod,
}
fn worker_key(worker: &WorkerIdentity) -> String {
    serde_json::to_string(worker).expect("fixed worker identity serialization")
}
fn target_key(target: &FenceTarget) -> Result<String> {
    Ok(serde_json::to_string(target)?)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FencePlan {
    pub service: Incarnation,
    pub gate_generation: u64,
    pub targets: Vec<FenceTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_obligation: Option<WorkerDrainObligation>,
}
impl FencePlan {
    pub fn new(
        service: Incarnation,
        gate_generation: u64,
        mut targets: Vec<FenceTarget>,
    ) -> Result<Self> {
        ensure!(
            targets.len() <= MAX_TARGETS,
            "catalog fence target capacity exceeded"
        );
        targets.sort_by_cached_key(|target| {
            serde_json::to_string(target).expect("fixed fence target serialization")
        });
        let plan = Self {
            service,
            gate_generation,
            targets,
            worker_obligation: None,
        };
        plan.validate()?;
        Ok(plan)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.gate_generation > 0 && self.targets.len() <= MAX_TARGETS,
            "invalid catalog fence plan bound"
        );
        let mut keys = Vec::new();
        for target in &self.targets {
            target.scope.validate()?;
            ensure!(
                target.worker.service == self.service && target.worker.fencing_generation > 0,
                "catalog fence worker identity mismatch"
            );
            // The complete ref includes the selected-subset fingerprint AND its
            // native lease generation. Distinct subsets/leases for the same
            // grant/index must each remain captured and acknowledged.
            keys.push(target_key(target)?);
        }
        ensure!(
            keys.windows(2).all(|pair| pair[0] < pair[1]),
            "catalog fence targets must be unique and canonical"
        );
        if let Some(obligation) = &self.worker_obligation {
            ensure!(
                obligation.workers.len() <= MAX_TARGETS,
                "catalog worker census capacity exceeded"
            );
            let mut previous = None;
            for worker in &obligation.workers {
                let key = worker_key(worker);
                ensure!(
                    worker.service == self.service
                        && worker.project == obligation.backing
                        && worker.fencing_generation > 0
                        && previous.as_ref().is_none_or(|old: &String| old < &key),
                    "catalog worker census identity mismatch"
                );
                previous = Some(key);
            }
            ensure!(
                obligation.providers.len() == obligation.workers.len(),
                "catalog provider census coverage incomplete"
            );
            for (worker, provider) in obligation.workers.iter().zip(&obligation.providers) {
                ensure!(
                    *worker == provider.worker,
                    "catalog provider census identity mismatch"
                );
                let generations = match &provider.history {
                    WorkerProviderHistory::NativeNeverAuthorized => continue,
                    WorkerProviderHistory::Helpers { generations } => {
                        ensure!(!generations.is_empty(), "empty helper provider history");
                        generations
                    }
                    WorkerProviderHistory::InProcess {
                        generations,
                        completed_authorizations,
                    } => {
                        if let Some(completed) = completed_authorizations {
                            completed.validate()?;
                            ensure!(
                                completed.service_instance == self.service,
                                "catalog completion ledger service mismatch"
                            );
                        }
                        ensure!(
                            !generations.is_empty() || completed_authorizations.is_some(),
                            "empty native provider history lacks completion ledger"
                        );
                        generations
                    }
                };
                {
                    ensure!(
                        generations.len() <= 32
                            && generations.windows(2).all(|pair| pair[0] < pair[1]),
                        "catalog provider generations must be bounded unique and canonical"
                    );
                }
            }
        }
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
    pub fn validate_for(&self, request: &CatalogRequest) -> Result<()> {
        self.validate()?;
        if let Some(obligation) = &self.worker_obligation {
            for provider in &obligation.providers {
                if let WorkerProviderHistory::InProcess {
                    completed_authorizations: Some(anchor),
                    ..
                } = &provider.history
                {
                    ensure!(
                        anchor.namespace == request.key.namespace,
                        "catalog completion ledger namespace mismatch"
                    );
                }
            }
        }
        match (&request.operation, &self.worker_obligation) {
            (
                CatalogOperation::RetireIndex { id, .. }
                | CatalogOperation::ChangeIndexPolicy { id, .. },
                Some(obligation),
            ) => ensure!(
                *id == obligation.index_id,
                "catalog retirement worker obligation mismatch"
            ),
            (
                CatalogOperation::RetireIndex { .. } | CatalogOperation::ChangeIndexPolicy { .. },
                None,
            ) => {
                anyhow::bail!("catalog retirement worker census missing")
            }
            (_, None) => {}
            (_, Some(_)) => anyhow::bail!("unexpected catalog worker drain obligation"),
        }
        Ok(())
    }
    pub fn with_worker_obligation(mut self, mut obligation: WorkerDrainObligation) -> Result<Self> {
        obligation.workers.sort_by_cached_key(worker_key);
        obligation
            .providers
            .sort_by_cached_key(|provider| worker_key(&provider.worker));
        for provider in &mut obligation.providers {
            if let WorkerProviderHistory::Helpers { generations }
            | WorkerProviderHistory::InProcess { generations, .. } = &mut provider.history
            {
                generations.sort();
            }
        }
        self.worker_obligation = Some(obligation);
        self.validate()?;
        Ok(self)
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/catalog-fence-plan/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FenceStage {
    Prepared,
    FenceUnknown,
    Fenced,
}
impl FenceStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::FenceUnknown => "fence_unknown",
            Self::Fenced => "fenced",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FenceIntent {
    pub schema: u32,
    pub operation_id: Incarnation,
    pub request: CatalogRequest,
    pub preview_digest: Sha256Digest,
    pub prepared_digest: Sha256Digest,
    pub plan: FencePlan,
    pub stage: FenceStage,
    pub confirmations: Vec<FenceConfirmation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub worker_confirmations: Vec<WorkerFenceConfirmation>,
}
impl FenceIntent {
    pub fn identity(&self) -> Result<Sha256Digest> {
        Ok(framed_digest(
            b"airk/catalog-fence-prepare/1",
            &[
                &serde_json::to_vec(&self.request)?,
                &serde_json::to_vec(&self.preview_digest)?,
                &serde_json::to_vec(&self.plan)?,
            ],
        ))
    }
    pub fn validate(&self) -> Result<()> {
        self.request.validate()?;
        self.plan.validate_for(&self.request)?;
        ensure!(
            self.schema == 1
                && self.request.operation.requires_authority_fence()
                && self.operation_id
                    == catalog_operation_id(self.request.caller, &self.request.key)?
                && self.prepared_digest == self.identity()?,
            "catalog fence intent identity conflict"
        );
        validate_confirmations(&self.plan, &self.confirmations)?;
        validate_worker_confirmations(&self.plan, &self.worker_confirmations)?;
        let complete = self.confirmations.len() == self.plan.targets.len()
            && self.worker_confirmations.len()
                == self
                    .plan
                    .worker_obligation
                    .as_ref()
                    .map_or(0, |o| o.workers.len());
        ensure!(
            match self.stage {
                FenceStage::Prepared =>
                    self.confirmations.is_empty() && self.worker_confirmations.is_empty(),
                FenceStage::FenceUnknown => !complete,
                FenceStage::Fenced => complete,
            },
            "catalog fence stage does not match confirmations"
        );
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
}
pub fn validate_worker_confirmations(
    plan: &FencePlan,
    confirmations: &[WorkerFenceConfirmation],
) -> Result<()> {
    plan.validate()?;
    let expected = plan
        .worker_obligation
        .as_ref()
        .map(|o| o.workers.as_slice())
        .unwrap_or(&[]);
    ensure!(
        confirmations.len() <= expected.len(),
        "catalog worker confirmation capacity exceeded"
    );
    let mut previous = None;
    for confirmation in confirmations {
        let key = worker_key(&confirmation.worker);
        ensure!(
            previous.as_ref().is_none_or(|old: &String| old < &key)
                && expected.contains(&confirmation.worker),
            "catalog worker confirmation mismatch"
        );
        previous = Some(key);
    }
    Ok(())
}
/// Validate a bounded canonical subset, without authenticating its native method.
pub fn validate_confirmations(plan: &FencePlan, confirmations: &[FenceConfirmation]) -> Result<()> {
    plan.validate()?;
    ensure!(
        confirmations.len() <= plan.targets.len(),
        "catalog fence confirmation capacity exceeded"
    );
    let mut previous = None;
    for confirmation in confirmations {
        let key = target_key(&confirmation.target)?;
        ensure!(
            previous.as_ref().is_none_or(|old: &String| old < &key)
                && plan.targets.contains(&confirmation.target),
            "catalog fence confirmation mismatch"
        );
        previous = Some(key);
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogFenceProof {
    pub plan_digest: Sha256Digest,
    pub service: Incarnation,
    pub gate_generation: u64,
    pub confirmations: Vec<FenceConfirmation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_obligation: Option<WorkerDrainObligation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub worker_confirmations: Vec<WorkerFenceConfirmation>,
}
impl CatalogFenceProof {
    pub fn from_intent(intent: &FenceIntent) -> Result<Self> {
        intent.validate()?;
        ensure!(
            intent.stage == FenceStage::Fenced,
            "catalog fence confirmations incomplete"
        );
        Ok(Self {
            plan_digest: intent.plan.identity()?,
            service: intent.plan.service,
            gate_generation: intent.plan.gate_generation,
            confirmations: intent.confirmations.clone(),
            worker_obligation: intent.plan.worker_obligation.clone(),
            worker_confirmations: intent.worker_confirmations.clone(),
        })
    }
    pub fn validate(&self) -> Result<()> {
        let plan = FencePlan {
            service: self.service,
            gate_generation: self.gate_generation,
            targets: self
                .confirmations
                .iter()
                .map(|c| c.target.clone())
                .collect(),
            worker_obligation: self.worker_obligation.clone(),
        };
        validate_worker_confirmations(&plan, &self.worker_confirmations)?;
        ensure!(
            self.worker_confirmations.len()
                == plan
                    .worker_obligation
                    .as_ref()
                    .map_or(0, |o| o.workers.len()),
            "catalog worker reap proof incomplete"
        );
        ensure!(
            plan.identity()? == self.plan_digest,
            "catalog fence proof mismatch"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FencedReceipt {
    pub request: CatalogRequest,
    pub metadata: CatalogReceipt,
    pub preview_digest: Sha256Digest,
    pub prepared_digest: Sha256Digest,
    pub proof: CatalogFenceProof,
}
impl FencedReceipt {
    pub fn validate(&self) -> Result<()> {
        self.proof.validate()?;
        let intent = FenceIntent {
            schema: 1,
            operation_id: self.metadata.operation_id,
            request: self.request.clone(),
            preview_digest: self.preview_digest,
            prepared_digest: self.prepared_digest,
            plan: FencePlan {
                service: self.proof.service,
                gate_generation: self.proof.gate_generation,
                targets: self
                    .proof
                    .confirmations
                    .iter()
                    .map(|c| c.target.clone())
                    .collect(),
                worker_obligation: self.proof.worker_obligation.clone(),
            },
            stage: FenceStage::Fenced,
            confirmations: self.proof.confirmations.clone(),
            worker_confirmations: self.proof.worker_confirmations.clone(),
        };
        intent.validate()?;
        ensure!(
            self.metadata.schema == 1
                && self.metadata.authority_fence_required
                && self.metadata.caller == self.request.caller
                && self.metadata.key == self.request.key
                && self.metadata.request_digest == self.request.identity()?
                && self.metadata.catalog_revision
                    == self.request.expected_catalog_revision.next()?
                && self.metadata.operation_id
                    == catalog_operation_id(self.metadata.caller, &self.metadata.key)?,
            "catalog fenced receipt identity conflict"
        );
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
    pub fn validate_for(
        &self,
        request: &CatalogRequest,
        preview_digest: Sha256Digest,
        prepared_digest: Sha256Digest,
        plan: &FencePlan,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            self.request == *request
                && self.metadata.caller == request.caller
                && self.metadata.key == request.key
                && self.metadata.request_digest == request.identity()?
                && self.metadata.catalog_revision == request.expected_catalog_revision.next()?
                && self.preview_digest == preview_digest
                && self.prepared_digest == prepared_digest
                && self.proof.plan_digest == plan.identity()?
                && self.proof.service == plan.service
                && self.proof.gate_generation == plan.gate_generation
                && self.proof.worker_obligation == plan.worker_obligation
                && self
                    .proof
                    .confirmations
                    .iter()
                    .map(|c| &c.target)
                    .eq(plan.targets.iter()),
            "catalog fenced receipt does not match native admission"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum FenceObservation {
    Pending(FenceIntent),
    Applied(FencedReceipt),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FencePreparation {
    Prepared(FenceIntent),
    Replay(FencedReceipt),
}

#[cfg(test)]
#[path = "catalog_fences_provider_tests.rs"]
mod provider_tests;
