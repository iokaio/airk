// SPDX-License-Identifier: Apache-2.0
//! Private ownership fence spanning prepare, wrapper placement and completion.
//! Pure claims/transition checks only: adapters must prove the live facts.
use super::super::{
    IndexDeletion, Locations, OperationKind, Refusal, SetupOperation, SetupPhase, SetupReceipt,
    Signing, canonical, decode,
};
use super::{
    InstallationImage, MAX_RECOVERY_BYTES, PublicationReceipt, RetainedRoots, RootIdentity,
    RootObservation, sha256,
};
use crate::identity::{Incarnation, Sha256Digest};
use serde::{Deserialize, Serialize};

pub const ACTIVE_SCHEMA: &str = "airk-active-setup/1";
/// Existing canonical I4 bytes carried by a private removal admission caller.
pub const EXPECTED_POINTER_FILE: &str = "expected-current.json";
pub const EXPECTED_RECORD_FILE: &str = "expected-install-record.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Admitted,
    Prepared,
    PairPublished,
    RemovalApplied,
    Settled,
}

/// A native-created object first bound in private staging before publication
/// at an initially absent root. The role and nonce determine its staging name;
/// no receipt-supplied arbitrary artifact path is accepted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatedRoot {
    pub identity: RootIdentity,
    pub parent: RootIdentity,
    pub nonce: Incarnation,
}

fn same_volume(a: &RootIdentity, b: &RootIdentity) -> bool {
    match (a, b) {
        (
            RootIdentity::Windows {
                volume_serial: a, ..
            },
            RootIdentity::Windows {
                volume_serial: b, ..
            },
        ) => a == b,
        (RootIdentity::Unix { device: a, .. }, RootIdentity::Unix { device: b, .. }) => a == b,
        _ => false,
    }
}

fn check_identity(identity: &RootIdentity, windows: bool) -> Result<(), Refusal> {
    identity.validate()?;
    if matches!(identity, RootIdentity::Windows { .. }) != windows {
        return Err(Refusal::Malformed);
    }
    Ok(())
}

fn present(observed: &RootObservation) -> Option<&RootIdentity> {
    match observed {
        RootObservation::Absent => None,
        RootObservation::Present { identity } => Some(identity),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootBinding {
    pub initial: RootObservation,
    pub created: Option<CreatedRoot>,
}

impl RootBinding {
    fn validate(&self, windows: bool) -> Result<(), Refusal> {
        if let Some(identity) = present(&self.initial) {
            check_identity(identity, windows)?;
        }
        if let Some(created) = &self.created {
            check_identity(&created.identity, windows)?;
            check_identity(&created.parent, windows)?;
            if present(&self.initial).is_some()
                || created.identity == created.parent
                || !same_volume(&created.identity, &created.parent)
                || created.nonce.as_bytes().iter().all(|byte| *byte == 0)
            {
                return Err(Refusal::Malformed);
            }
        }
        Ok(())
    }
    fn expected(&self) -> Option<&RootIdentity> {
        present(&self.initial).or_else(|| self.created.as_ref().map(|created| &created.identity))
    }
    fn follows(&self, prior: &Self) -> bool {
        self.initial == prior.initial && (self.created == prior.created || prior.created.is_none())
    }
}

/// A directory-root replacement is distinct from changing files inside the
/// same program directory. Both the original and native-owned staged root are
/// bound before wrapper placement, and the original must remain preserved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramReplacement {
    pub original: RootIdentity,
    pub staged: CreatedRoot,
    pub payload_inventory_sha256: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootBindings {
    pub program: RootBinding,
    pub data: RootBinding,
    pub state: RootBinding,
    pub cache: RootBinding,
    pub logs: RootBinding,
    pub program_replacement: Option<ProgramReplacement>,
}

impl RootBindings {
    fn rows(&self) -> [&RootBinding; 5] {
        [
            &self.program,
            &self.data,
            &self.state,
            &self.cache,
            &self.logs,
        ]
    }
    fn validate(&self, windows: bool, inventory: Sha256Digest) -> Result<(), Refusal> {
        let mut identities = Vec::new();
        for binding in self.rows() {
            binding.validate(windows)?;
            if let Some(identity) = binding.expected() {
                if identities.contains(&identity) {
                    return Err(Refusal::Malformed);
                }
                identities.push(identity);
            }
        }
        if let Some(replacement) = &self.program_replacement {
            let staged = RootBinding {
                initial: RootObservation::Absent,
                created: Some(replacement.staged.clone()),
            };
            staged.validate(windows)?;
            if Some(&replacement.original) != self.program.expected()
                || identities.contains(&&replacement.staged.identity)
                || !same_volume(&replacement.original, &replacement.staged.identity)
                || replacement.payload_inventory_sha256 != inventory
            {
                return Err(Refusal::Malformed);
            }
        }
        Ok(())
    }
    fn follows(&self, prior: &Self) -> bool {
        self.rows()
            .iter()
            .zip(prior.rows())
            .all(|(next, old)| next.follows(old))
            && (self.program_replacement == prior.program_replacement
                || prior.program_replacement.is_none())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootSnapshot {
    pub program: RootObservation,
    pub retained: RetainedRoots,
}

impl RootSnapshot {
    fn rows(&self) -> [&RootObservation; 5] {
        [
            &self.program,
            &self.retained.data,
            &self.retained.state,
            &self.retained.cache,
            &self.retained.logs,
        ]
    }
    fn validate(&self, windows: bool) -> Result<(), Refusal> {
        self.retained.validate(windows)?;
        if let Some(identity) = present(&self.program) {
            check_identity(identity, windows)?;
            if self.rows()[1..]
                .iter()
                .any(|root| present(root) == Some(identity))
            {
                return Err(Refusal::Malformed);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Target {
    Place {
        publication: Box<PublicationReceipt>,
    },
    Remove {
        installation: Box<InstallationImage>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveSetup {
    pub schema: String,
    pub sequence: u64,
    pub operation_sha256: Sha256Digest,
    pub operation: SetupOperation,
    pub admitted_unix_ms: u64,
    pub authority_root: RootIdentity,
    pub target: Target,
    pub selected_roots: RootBindings,
    pub prior_roots: Option<RootSnapshot>,
    pub stage: Stage,
}

fn locations(locations: &Locations) -> [&str; 5] {
    [
        &locations.program,
        &locations.data,
        &locations.state,
        &locations.cache,
        &locations.logs,
    ]
}

impl ActiveSetup {
    pub fn decode(bytes: &[u8], operation: &SetupOperation) -> Result<Self, Refusal> {
        let active: Self = decode(bytes, MAX_RECOVERY_BYTES, ACTIVE_SCHEMA, |r: &Self| {
            &r.schema
        })?;
        active.validate()?;
        active.admit_same_operation(operation)?;
        Ok(active)
    }
    pub fn admit_same_operation(&self, operation: &SetupOperation) -> Result<(), Refusal> {
        if &self.operation != operation || self.operation_sha256 != sha256(&canonical(operation)) {
            return Err(Refusal::Malformed);
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), Refusal> {
        if self.schema != ACTIVE_SCHEMA {
            return Err(Refusal::UnknownSchema);
        }
        SetupOperation::decode(&canonical(&self.operation), self.admitted_unix_ms)?;
        self.admit_same_operation(&self.operation)?;
        if self.sequence == 0 {
            return Err(Refusal::Malformed);
        }
        let (image, prior) = match &self.target {
            Target::Place { publication } => {
                publication.validate_for(&self.operation)?;
                if self.stage == Stage::RemovalApplied {
                    return Err(Refusal::Malformed);
                }
                (&publication.candidate, publication.prior.as_ref())
            }
            Target::Remove { installation } => {
                installation.validate()?;
                if !matches!(
                    self.operation.kind,
                    OperationKind::Uninstall | OperationKind::Purge
                ) || installation.record.locations != self.operation.locations
                    || self.stage == Stage::PairPublished
                {
                    return Err(Refusal::Malformed);
                }
                (installation.as_ref(), None)
            }
        };
        let windows = matches!(image.record.signing, Signing::Windows { .. });
        check_identity(&self.authority_root, windows)?;
        self.selected_roots
            .validate(windows, self.operation.payload_inventory_sha256)?;
        match (prior, &self.prior_roots) {
            (Some(prior), Some(roots)) => {
                roots.validate(windows)?;
                if roots
                    .rows()
                    .iter()
                    .any(|root| present(root) == Some(&self.authority_root))
                {
                    return Err(Refusal::Malformed);
                }
                for ((old_path, new_path), (old_root, selected)) in
                    locations(&prior.record.locations)
                        .iter()
                        .zip(locations(&self.operation.locations))
                        .zip(roots.rows().iter().zip(self.selected_roots.rows()))
                {
                    if *old_path == new_path && *old_root != &selected.initial {
                        return Err(Refusal::Malformed);
                    }
                }
            }
            (None, None) => {}
            _ => return Err(Refusal::Malformed),
        }
        if self
            .selected_roots
            .rows()
            .iter()
            .any(|root| root.expected() == Some(&self.authority_root))
        {
            return Err(Refusal::Malformed);
        }
        if self.stage != Stage::Admitted
            && (self.selected_roots.program.expected().is_none()
                || self.selected_roots.state.expected().is_none()
                || matches!(self.target, Target::Place { .. })
                    && self
                        .selected_roots
                        .rows()
                        .iter()
                        .any(|root| root.expected().is_none()))
        {
            return Err(Refusal::Malformed);
        }
        if (self.operation.kind == OperationKind::Purge
            || self.operation.options.delete_indexes == IndexDeletion::Purge)
            && !self.operation.acknowledgements.index_reset
        {
            return Err(Refusal::Malformed);
        }
        if matches!(self.target, Target::Remove { .. })
            && self.selected_roots.program_replacement.is_some()
        {
            return Err(Refusal::Malformed);
        }
        Ok(())
    }
    /// A transition cannot change owner, prior snapshots, admitted operation or
    /// an already-bound root. New creation/staging identities bind only before
    /// Prepare releases the native lock to the wrapper.
    pub fn validate_successor(&self, previous: &Self) -> Result<(), Refusal> {
        self.validate()?;
        previous.validate()?;
        let stage_ok = self.stage == previous.stage && self.stage != Stage::Settled
            || matches!(
                (previous.stage, self.stage),
                (Stage::Admitted, Stage::Prepared)
                    | (Stage::Prepared, Stage::PairPublished)
                    | (Stage::Prepared, Stage::RemovalApplied)
                    | (Stage::PairPublished, Stage::Settled)
                    | (Stage::RemovalApplied, Stage::Settled)
            );
        if previous.sequence.checked_add(1) != Some(self.sequence)
            || !stage_ok
            || self.operation != previous.operation
            || self.operation_sha256 != previous.operation_sha256
            || self.admitted_unix_ms != previous.admitted_unix_ms
            || self.authority_root != previous.authority_root
            || self.target != previous.target
            || self.prior_roots != previous.prior_roots
            || !self.selected_roots.follows(&previous.selected_roots)
            || previous.stage != Stage::Admitted && self.selected_roots != previous.selected_roots
        {
            return Err(Refusal::Malformed);
        }
        Ok(())
    }
    /// Expiry never releases the logical placement fence. Only the bound
    /// candidate may be considered for startup, after full pair publication;
    /// adapters must still prove actual images, root continuity and admission.
    pub fn candidate_start_allowed(
        &self,
        install_id: Incarnation,
        main_sha256: Sha256Digest,
    ) -> bool {
        self.validate().is_ok()
            && self.stage == Stage::PairPublished
            && self.operation.options.start
            && matches!(&self.target, Target::Place { publication } if publication.candidate.record.install_id == install_id && publication.candidate.record.executables.airk.sha256 == main_sha256)
    }
    pub fn validate_live_roots(
        &self,
        authority: &RootIdentity,
        selected: &RootSnapshot,
        prior: Option<&RootSnapshot>,
        displaced_program: Option<&RootObservation>,
    ) -> Result<(), Refusal> {
        self.validate()?;
        if authority != &self.authority_root {
            return Err(Refusal::Malformed);
        }
        for (index, (binding, observed)) in self
            .selected_roots
            .rows()
            .iter()
            .zip(selected.rows())
            .enumerate()
        {
            let expected =
                if index == 0 && matches!(self.stage, Stage::PairPublished | Stage::Settled) {
                    self.selected_roots
                        .program_replacement
                        .as_ref()
                        .map(|r| &r.staged.identity)
                        .or_else(|| binding.expected())
                } else {
                    binding.expected()
                };
            let original = present(&binding.initial);
            let allowed_prepared_program = index == 0
                && self.stage == Stage::Prepared
                && self
                    .selected_roots
                    .program_replacement
                    .as_ref()
                    .is_some_and(|r| present(observed) == Some(&r.staged.identity));
            let allowed_initial_absence =
                self.stage == Stage::Admitted && original.is_none() && present(observed).is_none();
            let removed_program = index == 0
                && matches!(self.stage, Stage::RemovalApplied | Stage::Settled)
                && matches!(self.target, Target::Remove { .. })
                && present(observed).is_none();
            if present(observed) != expected
                && !allowed_prepared_program
                && !allowed_initial_absence
                && !removed_program
            {
                return Err(Refusal::Malformed);
            }
        }
        match &self.selected_roots.program_replacement {
            Some(replacement)
                if present(&selected.program) == Some(&replacement.staged.identity) =>
            {
                if displaced_program.and_then(present) != Some(&replacement.original) {
                    return Err(Refusal::Malformed);
                }
            }
            _ if displaced_program.is_some() => return Err(Refusal::Malformed),
            _ => {}
        }
        match (&self.target, &self.prior_roots, prior) {
            (Target::Place { publication }, Some(expected), Some(observed)) => {
                let image = publication.prior.as_ref().ok_or(Refusal::Malformed)?;
                for (((old_path, new_path), old), (live, selected)) in
                    locations(&image.record.locations)
                        .iter()
                        .zip(locations(&self.operation.locations))
                        .zip(expected.rows())
                        .zip(observed.rows().iter().zip(selected.rows()))
                {
                    if (*old_path == new_path && *live != selected)
                        || (*old_path != new_path && *live != old)
                    {
                        return Err(Refusal::Malformed);
                    }
                }
            }
            (_, None, None) => {}
            _ => return Err(Refusal::Malformed),
        }
        Ok(())
    }
    /// A terminal tag alone never frees the fence. Native consumers also prove
    /// the full current pair/removal and rooted filesystem state before clearing.
    pub fn validate_settlement(&self, receipt: &SetupReceipt) -> Result<(), Refusal> {
        self.validate()?;
        SetupReceipt::decode(&canonical(receipt))?;
        if self.stage != Stage::Settled
            || receipt.operation_id != self.operation.operation_id
            || receipt.operation_sha256 != self.operation_sha256
            || receipt.kind != self.operation.kind
            || receipt.expires_unix_ms != self.operation.expires_unix_ms
            || !receipt.closed
            || receipt.failure.is_some()
        {
            return Err(Refusal::Malformed);
        }
        let complete = receipt
            .steps
            .iter()
            .position(|step| step.phase == SetupPhase::Complete && step.outcome == "closed")
            .ok_or(Refusal::Malformed)?;
        let phases = match self.target {
            Target::Place { .. } => (
                SetupPhase::RecordWritten,
                "written",
                SetupPhase::PointerWritten,
                "written",
            ),
            Target::Remove { .. } => (
                SetupPhase::PointerRemoved,
                "removed",
                SetupPhase::RecordRemoved,
                "removed",
            ),
        };
        let first = receipt
            .steps
            .iter()
            .position(|step| {
                step.phase == phases.0
                    && (step.outcome == phases.1
                        || phases.1 == "removed" && step.outcome == "absent")
            })
            .ok_or(Refusal::Malformed)?;
        let second = receipt
            .steps
            .iter()
            .position(|step| {
                step.phase == phases.2
                    && (step.outcome == phases.3
                        || phases.3 == "removed" && step.outcome == "absent")
            })
            .ok_or(Refusal::Malformed)?;
        if first >= second || second >= complete || complete + 1 != receipt.steps.len() {
            return Err(Refusal::Malformed);
        }
        let purged = receipt
            .steps
            .iter()
            .position(|step| step.phase == SetupPhase::DataPurged && step.outcome == "purged");
        let expects_purge = self.operation.kind == OperationKind::Purge
            || self.operation.options.delete_indexes == IndexDeletion::Purge;
        let purge_order_wrong = purged.is_some_and(|position| {
            position >= complete
                || match self.target {
                    Target::Place { .. } => position >= first,
                    Target::Remove { .. } => position <= second,
                }
        });
        if expects_purge != purged.is_some() || purge_order_wrong {
            return Err(Refusal::Malformed);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
