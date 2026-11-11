// SPDX-License-Identifier: Apache-2.0
//! Pure complete catalog fence wire contracts and bounded identity validation.
//! These checks do not authenticate native worker acknowledgements, process
//! reaping, or an installed-authority census. Native adapters supply that proof.
use crate::{
    catalog_management::{CatalogReceipt, CatalogRequest, MAX_RECEIPT_BYTES, catalog_operation_id},
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::encoded_size,
    service::{ScopedReadRef, WorkerIdentity},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

const MAX_TARGETS: usize = 128;

/// A known rejection before native preparation reserves a durable receipt. This
/// marker alone never authorizes reopening: the native coordinator must also
/// prove exact receipt absence, unchanged namespace/revision and zero effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FencePrepareRejected {
    Capacity,
    StaleReview,
}
impl std::fmt::Display for FencePrepareRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Capacity => "catalog receipt capacity exceeded",
            Self::StaleReview => "catalog review stale",
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
fn target_key(target: &FenceTarget) -> Result<String> {
    Ok(serde_json::to_string(target)?)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FencePlan {
    pub service: Incarnation,
    pub gate_generation: u64,
    pub targets: Vec<FenceTarget>,
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
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
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
        self.plan.validate()?;
        ensure!(
            self.schema == 1
                && self.request.operation.requires_authority_fence()
                && self.operation_id
                    == catalog_operation_id(self.request.caller, &self.request.key)?
                && self.prepared_digest == self.identity()?,
            "catalog fence intent identity conflict"
        );
        validate_confirmations(&self.plan, &self.confirmations)?;
        ensure!(
            match self.stage {
                FenceStage::Prepared => self.confirmations.is_empty(),
                FenceStage::FenceUnknown => self.confirmations.len() != self.plan.targets.len(),
                FenceStage::Fenced => self.confirmations.len() == self.plan.targets.len(),
            },
            "catalog fence stage does not match confirmations"
        );
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
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
        };
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
            },
            stage: FenceStage::Fenced,
            confirmations: self.proof.confirmations.clone(),
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
