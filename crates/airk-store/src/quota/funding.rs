// SPDX-License-Identifier: Apache-2.0
//! Durable operation funding. One bounded record owns parent and child credit.
//! Registry/job authority is checked by the native caller before binding a use.
//! No collection recovery or source I/O runs while the quota ledger is held.
use super::*;
use airk_core::{
    catalog_management::catalog_operation_id,
    catalog_split::{SplitJobPurpose, SplitRequest},
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::IndexId,
    service::ProjectRegistration,
};

const DIRECTORY: &str = "split-allocations";
pub(super) const MAX_RECORD_BYTES: u64 = 65_536;
const MAX_OPERATIONS: usize = 64;
const MAX_CHILDREN: usize = 64;

#[cfg(test)]
#[path = "funding_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundingBudget {
    pub index_id: IndexId,
    pub bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundingPolicyReview {
    pub data_root: PathBuf,
    pub policy: QuotaPolicy,
    pub digest: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundingRequest {
    pub operation_id: Incarnation,
    pub request_digest: Sha256Digest,
    pub namespace: Incarnation,
    pub original_index: IndexId,
    pub original: ProjectRegistration,
    pub destination_index: IndexId,
    pub destination: ProjectRegistration,
    pub budgets: Vec<FundingBudget>,
    pub quota_digests: BTreeMap<String, Sha256Digest>,
}
impl FundingRequest {
    pub fn for_split(request: &SplitRequest, mut budgets: Vec<FundingBudget>) -> Result<Self> {
        request.validate()?;
        budgets.sort_by_key(|budget| budget.index_id);
        let mut quota_digests = BTreeMap::new();
        quota_digests.insert(
            request.original_registration.data_root.clone(),
            request.capacity.original_quota_digest,
        );
        if let Some(prior) = quota_digests.insert(
            request.destination_registration.data_root.clone(),
            request.capacity.destination_quota_digest,
        ) {
            ensure!(
                prior == request.capacity.destination_quota_digest,
                "same-root quota review digests differ"
            );
        }
        let value = Self {
            operation_id: catalog_operation_id(request.caller, &request.key)?,
            request_digest: request.identity()?,
            namespace: request.key.namespace,
            original_index: request.original_index,
            original: request.original_registration.clone(),
            destination_index: request.destination.index_id,
            destination: request.destination_registration.clone(),
            budgets,
            quota_digests,
        };
        value.validate()?;
        ensure!(
            value
                .budgets
                .iter()
                .try_fold(0u64, |total, budget| checked_add(total, budget.bytes))?
                == request.capacity.total()?,
            "funding differs from reviewed total capacity"
        );
        let original = value
            .budgets
            .iter()
            .find(|budget| budget.index_id == value.original_index)
            .unwrap()
            .bytes;
        let destination = value
            .budgets
            .iter()
            .find(|budget| budget.index_id == value.destination_index)
            .unwrap()
            .bytes;
        let original_metadata = request.capacity.cleanup_allowance / 2;
        let destination_metadata = request.capacity.cleanup_allowance - original_metadata;
        ensure!(
            original >= checked_add(request.capacity.original_rewrite, original_metadata)?
                && destination
                    >= checked_add(
                        checked_add(
                            request.capacity.destination_growth,
                            request.capacity.destination_temporary
                        )?,
                        destination_metadata
                    )?,
            "participant funding cannot move reviewed build/rewrite credit across roots"
        );
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        self.original.validate()?;
        self.destination.validate()?;
        ensure!(
            self.original_index != self.destination_index
                && self.budgets.len() == 2
                && self.budgets[0].index_id < self.budgets[1].index_id,
            "funding requires two distinct canonical participants"
        );
        for budget in &self.budgets {
            ensure!(
                budget.bytes > 0
                    && budget.bytes <= i64::MAX as u64
                    && self.registration(budget.index_id).is_some(),
                "invalid participant funding"
            );
        }
        if self.original.data_root == self.destination.data_root {
            ensure!(
                self.original.collection != self.destination.collection,
                "funding participants cannot share a collection backing"
            );
        }
        let roots = [&self.original.data_root, &self.destination.data_root]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        ensure!(
            self.quota_digests.len() == roots.len()
                && self.quota_digests.keys().all(|root| roots.contains(root)),
            "funding quota review owner differs"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= 16_384,
            "funding request exceeds bound"
        );
        Ok(())
    }
    fn registration(&self, id: IndexId) -> Option<&ProjectRegistration> {
        if id == self.original_index {
            Some(&self.original)
        } else if id == self.destination_index {
            Some(&self.destination)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundingPhase {
    pub revision: u64,
    pub purpose: SplitJobPurpose,
    pub job: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantFunding {
    pub index_id: IndexId,
    pub collection: String,
    pub initial_bytes: u64,
    pub baseline_used_bytes: u64,
    pub remaining_growth: u64,
    pub consumed_growth: u64,
    pub phase: Option<FundingPhase>,
    pub settled_revision: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FundingChildKind {
    Current {
        scope: CurrentGrantScope,
        allocation: CurrentGrantScope,
    },
    Reservation {
        staging_name: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundingChild {
    pub id: Sha256Digest,
    pub index_id: IndexId,
    pub phase: FundingPhase,
    pub kind: FundingChildKind,
    pub capacity: u64,
    /// Credit held until the native phase reconciles actual managed growth.
    pub held_bytes: u64,
    pub charged: u64,
    pub closed: bool,
    /// Completed operation's current journal retains this exact allocation.
    pub current_owned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundingReceipt {
    pub version: u32,
    pub request: FundingRequest,
    pub data_root: PathBuf,
    pub root_identity: Sha256Digest,
    pub revision: u64,
    pub participants: Vec<ParticipantFunding>,
    pub children: Vec<FundingChild>,
    pub takeovers: Vec<FundingTakeover>,
    pub complete: bool,
    pub release: Option<FundingRelease>,
    pub release_acknowledged: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundingTakeover {
    pub kind: FundingTakeoverKind,
    pub index_id: IndexId,
    pub previous_phase: Option<FundingPhase>,
    pub next_phase: FundingPhase,
    pub previous_revision: u64,
    pub previous_observation: Sha256Digest,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FundingTakeoverKind {
    CanceledDestinationCleanup,
    FailedWorkRetry,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundingRelease {
    pub revision: u64,
    pub unused_growth_released: BTreeMap<IndexId, u64>,
    pub current_allocations_retained: Vec<RetainedCurrentFunding>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedCurrentFunding {
    pub id: Sha256Digest,
    pub index_id: IndexId,
    pub scope: CurrentGrantScope,
    pub allocation: CurrentGrantScope,
    pub capacity: u64,
    pub held_bytes: u64,
}
impl FundingReceipt {
    /// Immutable allocation identity; mutable consumption is separately observed.
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/split-quota-funding/1",
            &[
                &serde_json::to_vec(&self.request)?,
                self.data_root
                    .to_str()
                    .context("funding root UTF-8")?
                    .as_bytes(),
                self.root_identity.as_bytes(),
            ],
        ))
    }
    pub fn validate_for(&self, request: &SplitRequest) -> Result<()> {
        self.validate()?;
        ensure!(
            self.request == FundingRequest::for_split(request, self.request.budgets.clone())?,
            "funding belongs to a different split request"
        );
        Ok(())
    }
    pub fn release_identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        ensure!(
            self.complete,
            "funding has not released unused operation credit"
        );
        Ok(framed_digest(
            b"airk/split-quota-release/1",
            &[
                self.identity()?.as_bytes(),
                &serde_json::to_vec(self.release.as_ref().context("funding release absent")?)?,
            ],
        ))
    }
    fn filename(&self) -> String {
        format!("f-{}.json", self.request.operation_id)
    }
    fn validate(&self) -> Result<()> {
        self.request.validate()?;
        ensure!(
            self.version == 1
                && self.revision > 0
                && self.data_root.is_absolute()
                && !self
                    .data_root
                    .components()
                    .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
                && (1..=2).contains(&self.participants.len())
                && self.children.len() <= MAX_CHILDREN,
            "invalid bounded funding record"
        );
        ensure!(
            self.takeovers.len() <= airk_core::catalog_split::MAX_SPLIT_JOBS,
            "funding cleanup takeover history exceeds bound"
        );
        let mut takeover_revisions = BTreeMap::new();
        for takeover in &self.takeovers {
            let participant_valid = match takeover.kind {
                FundingTakeoverKind::CanceledDestinationCleanup => {
                    takeover.index_id == self.request.destination_index
                        && takeover.next_phase.purpose == SplitJobPurpose::DestinationCleanup
                }
                FundingTakeoverKind::FailedWorkRetry => {
                    takeover.previous_phase.as_ref().is_some_and(|previous| {
                        previous.purpose == takeover.next_phase.purpose
                            && previous.job != takeover.next_phase.job
                    }) && match takeover.next_phase.purpose {
                        SplitJobPurpose::DestinationBuild => {
                            takeover.index_id == self.request.destination_index
                        }
                        SplitJobPurpose::OriginalRemoval | SplitJobPurpose::OriginalCleanup => {
                            takeover.index_id == self.request.original_index
                        }
                        SplitJobPurpose::DestinationCleanup => false,
                        SplitJobPurpose::OriginalWitness => false,
                    }
                }
            };
            ensure!(
                participant_valid
                    && takeover.next_phase.revision > 0
                    && takeover.previous_revision > 0
                    && takeover.previous_revision < self.revision
                    && takeover
                        .previous_phase
                        .as_ref()
                        .is_none_or(|phase| phase.revision < takeover.next_phase.revision)
                    && takeover_revisions
                        .insert(takeover.index_id, takeover.next_phase.revision)
                        .is_none_or(|previous| previous < takeover.next_phase.revision),
                "invalid exact cleanup takeover lineage"
            );
        }
        ensure!(
            self.complete == self.release.is_some()
                && (!self.release_acknowledged || self.complete),
            "funding release state differs from its native receipt"
        );
        if let Some(release) = &self.release {
            ensure!(
                release.revision > 0
                    && release.revision <= self.revision
                    && release
                        .unused_growth_released
                        .keys()
                        .copied()
                        .eq(self.participants.iter().map(|part| part.index_id))
                    && release.current_allocations_retained.len() <= MAX_CHILDREN,
                "invalid funding release proof"
            );
            let mut retained_ids = std::collections::BTreeSet::new();
            for retained in &release.current_allocations_retained {
                let participant = self
                    .participants
                    .iter()
                    .find(|part| part.index_id == retained.index_id)
                    .context("released current allocation crosses participant")?;
                ensure!(
                    retained_ids.insert(retained.id)
                        && retained.capacity > 0
                        && retained.held_bytes >= retained.capacity
                        && retained.scope.collection == participant.collection
                        && retained.allocation.collection == participant.collection
                        && retained.scope.incarnation == retained.allocation.incarnation,
                    "invalid retained current funding proof"
                );
            }
            for participant in &self.participants {
                let retained = release
                    .current_allocations_retained
                    .iter()
                    .filter(|item| item.index_id == participant.index_id)
                    .try_fold(0u64, |total, item| checked_add(total, item.held_bytes))?;
                ensure!(
                    checked_add(
                        checked_add(
                            participant.consumed_growth,
                            release.unused_growth_released[&participant.index_id]
                        )?,
                        retained
                    )? == participant.initial_bytes,
                    "released operation funding is not conserved"
                );
            }
        }
        let mut expected = Vec::new();
        for budget in &self.request.budgets {
            let registration = self
                .request
                .registration(budget.index_id)
                .context("funding participant missing")?;
            if Path::new(&registration.data_root) == self.data_root {
                expected.push(budget.index_id);
            }
        }
        ensure!(
            self.participants
                .iter()
                .map(|part| part.index_id)
                .eq(expected),
            "funding root participant roster differs"
        );
        let mut ids = std::collections::BTreeSet::new();
        for child in &self.children {
            ensure!(
                ids.insert(child.id)
                    && child.capacity > 0
                    && child.held_bytes >= child.capacity
                    && child.charged <= child.capacity,
                "invalid funded child allowance"
            );
            let participant = self
                .participants
                .iter()
                .find(|part| part.index_id == child.index_id)
                .context("funded child crosses root or participant")?;
            ensure!(
                child.phase.revision > 0
                    && (self.complete || participant.phase.as_ref() == Some(&child.phase)),
                "funded child phase is not current"
            );
            ensure!(
                !child.current_owned || self.complete,
                "live split child cannot have independent current ownership"
            );
            ensure!(
                !self.complete
                    || (child.current_owned
                        && !child.closed
                        && matches!(child.kind, FundingChildKind::Current { .. })),
                "completed operation retains an unsettled child"
            );
            match &child.kind {
                FundingChildKind::Current { scope, allocation } => ensure!(
                    scope.collection == participant.collection
                        && allocation.collection == scope.collection
                        && allocation.incarnation == scope.incarnation,
                    "funded current scope changed"
                ),
                FundingChildKind::Reservation { staging_name } => ensure!(
                    staging_name == &format!("r-funded-{}", child.id) && !child.current_owned,
                    "funded staging identity changed"
                ),
            }
        }
        for part in &self.participants {
            crate::validate_collection(&part.collection)?;
            let budget = self
                .request
                .budgets
                .iter()
                .find(|budget| budget.index_id == part.index_id)
                .context("participant budget missing")?;
            ensure!(
                budget.bytes == part.initial_bytes
                    && self
                        .request
                        .registration(part.index_id)
                        .is_some_and(|registration| registration.collection == part.collection),
                "participant funding binding changed"
            );
            ensure!(
                part.settled_revision.is_none_or(|revision| part
                    .phase
                    .as_ref()
                    .is_some_and(|phase| phase.revision == revision)),
                "funding settlement belongs to another phase"
            );
            if !self.complete {
                let held = self
                    .children
                    .iter()
                    .filter(|child| child.index_id == part.index_id)
                    .try_fold(0u64, |total, child| checked_add(total, child.held_bytes))?;
                ensure!(
                    checked_add(
                        checked_add(part.remaining_growth, part.consumed_growth)?,
                        held
                    )? == part.initial_bytes,
                    "operation funding accounting is not conserved"
                );
            } else {
                ensure!(
                    part.remaining_growth == 0,
                    "completed funding retains parent credit"
                );
            }
        }
        ensure!(
            serde_json::to_vec(self)?.len() as u64 <= MAX_RECORD_BYTES,
            "funding record exceeds bound"
        );
        Ok(())
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct FundingUse {
    pub operation_id: Incarnation,
    pub request_digest: Sha256Digest,
    pub data_root: PathBuf,
    pub root_identity: Sha256Digest,
    pub index_id: IndexId,
    pub phase: FundingPhase,
}
#[derive(Clone, Debug)]
pub(super) struct FundedChildRef {
    pub owner: FundingUse,
    pub child: Sha256Digest,
}

fn root_identity(quota: &Quota, namespace: Incarnation) -> Result<Sha256Digest> {
    use crate::service::registry::catalog::setup::storage::SetupDirectory;
    ensure!(
        quota.namespace == Namespace::Service,
        "split funding requires service quota"
    );
    let root = SetupDirectory::open_canonical(&quota.root)?;
    pinned_identity(&root, &quota.root, namespace)
}
fn pinned_identity(
    root: &crate::service::registry::catalog::setup::storage::SetupDirectory,
    logical_root: &Path,
    namespace: Incarnation,
) -> Result<Sha256Digest> {
    let owner = root.read_file(
        std::ffi::OsStr::new(crate::service::data_scope::OWNER),
        16_384,
    )?;
    let value: serde_json::Value = serde_json::from_slice(&owner)?;
    ensure!(
        value["version"] == 1
            && value["namespace"] == namespace.to_string()
            && value["data"] == logical_root.to_str().context("funding root UTF-8")?,
        "funding root ownership differs"
    );
    let identity = root.identity()?;
    Ok(framed_digest(
        b"airk/split-quota-root/1",
        &[&identity.0.to_be_bytes(), &identity.1.to_be_bytes(), &owner],
    ))
}

fn read_record(quota: &Quota, operation: Incarnation) -> Result<FundingReceipt> {
    let directory = quota.control_path().join(DIRECTORY);
    unlinked(&directory)?;
    let record: FundingReceipt = json_read(
        &directory.join(format!("f-{operation}.json")),
        MAX_RECORD_BYTES,
    )?;
    record.validate()?;
    ensure!(
        record.data_root == quota.root
            && record.request.operation_id == operation
            && record.root_identity == root_identity(quota, record.request.namespace)?,
        "funding root or operation changed"
    );
    Ok(record)
}
fn persist(quota: &Quota, record: &FundingReceipt) -> Result<()> {
    persist_observed(quota, record, |_| Ok(()))
}
#[derive(Clone, Copy)]
enum PersistPoint {
    BeforeReplace,
    AfterReplace,
}
fn persist_observed(
    quota: &Quota,
    record: &FundingReceipt,
    mut observer: impl FnMut(PersistPoint) -> Result<()>,
) -> Result<()> {
    record.validate()?;
    let directory = quota.control_path().join(DIRECTORY);
    fs::create_dir_all(&directory)?;
    unlinked(&directory)?;
    crate::capacity::require_growth(&quota.root, MAX_RECORD_BYTES)?;
    let mut pending = tempfile::Builder::new()
        .prefix("funding-")
        .tempfile_in(&directory)?;
    pending.write_all(&serde_json::to_vec(record)?)?;
    observer(PersistPoint::BeforeReplace)?;
    crate::durability::persist_synced(pending, &directory.join(record.filename()))?;
    crate::maintenance::sync_parent(&directory)?;
    observer(PersistPoint::AfterReplace)?;
    Ok(())
}

fn inventory(quota: &Quota, deadline: Option<Instant>) -> Result<Vec<FundingReceipt>> {
    let directory = quota.control_path().join(DIRECTORY);
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    unlinked(&directory)?;
    let mut result = Vec::new();
    for (count, entry) in entries.enumerate() {
        within_deadline(deadline)?;
        ensure!(
            count < MAX_OPERATIONS * 2 + 1,
            "funding operation inventory exceeds bound"
        );
        let entry = entry?;
        let name = entry.file_name();
        let text = name.to_str().context("funding name UTF-8")?;
        if text == "leases" {
            unlinked(&entry.path())?;
            ensure!(entry.file_type()?.is_dir(), "funding leases not directory");
            for (visited, lease) in fs::read_dir(entry.path())?.enumerate() {
                within_deadline(deadline)?;
                ensure!(
                    visited < MAX_RESERVATIONS * 2,
                    "funding lease inventory exceeds bound"
                );
                let lease = lease?;
                validate_lease_name(&lease.file_name())?;
                unlinked(&lease.path())?;
                ensure!(
                    lease.file_type()?.is_file() && lease.metadata()?.len() == 0,
                    "funded lease has unexpected contents"
                );
            }
            continue;
        }
        unlinked(&entry.path())?;
        ensure!(
            entry.file_type()?.is_file() && entry.metadata()?.len() <= MAX_RECORD_BYTES,
            "invalid funding entry"
        );
        if text.starts_with("funding-") {
            continue;
        }
        let operation = text
            .strip_prefix("f-")
            .and_then(|name| name.strip_suffix(".json"))
            .context("unexpected funding entry")?
            .parse()?;
        result.push(read_record(quota, operation)?);
        ensure!(
            result.len() <= MAX_OPERATIONS,
            "funding operation count exhausted"
        );
    }
    Ok(result)
}

/// The setup owner uses its already captured native directory handles, including
/// Windows rename/identity protections. No path-based fallback is allowed here.
pub(super) fn pinned_records(
    control: &crate::service::registry::catalog::setup::storage::SetupDirectory,
    root: &crate::service::registry::catalog::setup::storage::SetupDirectory,
    logical_root: &Path,
) -> Result<Vec<LiveRecord>> {
    pinned_records_checked(control, root, logical_root, None)
}
fn pinned_records_checked(
    control: &crate::service::registry::catalog::setup::storage::SetupDirectory,
    root: &crate::service::registry::catalog::setup::storage::SetupDirectory,
    logical_root: &Path,
    retired_registration: Option<&ProjectRegistration>,
) -> Result<Vec<LiveRecord>> {
    use crate::service::registry::catalog::setup::storage::SetupFileMode;
    use std::ffi::OsStr;
    let directory = match control.open_dir(OsStr::new(DIRECTORY)) {
        Ok(directory) => directory,
        Err(error)
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|error| error.kind() == io::ErrorKind::NotFound) =>
        {
            return Ok(Vec::new());
        }
        Err(error) => return Err(error),
    };
    ensure!(
        directory.identity()?.0 == root.identity()?.0,
        "funding inventory crosses filesystem"
    );
    let mut result = Vec::new();
    let mut operations = 0;
    for (visited, entry) in directory.entries()?.enumerate() {
        ensure!(
            visited < MAX_OPERATIONS * 2 + 1,
            "funding inventory exceeds bound"
        );
        let name = entry?;
        let text = name.to_str().context("funding name UTF-8")?;
        if text == "leases" {
            let leases = directory.open_dir(&name)?;
            for (visited, entry) in leases.entries()?.enumerate() {
                ensure!(
                    visited < MAX_RESERVATIONS * 2,
                    "funding lease inventory exceeds bound"
                );
                let name = entry?;
                validate_lease_name(&name)?;
                ensure!(
                    leases
                        .open_file(&name, SetupFileMode::Read)?
                        .metadata()?
                        .len()
                        == 0,
                    "funded lease has unexpected contents"
                );
            }
            continue;
        }
        let bytes = directory.read_file(&name, MAX_RECORD_BYTES)?;
        if text.starts_with("funding-") {
            continue;
        }
        let record: FundingReceipt = serde_json::from_slice(&bytes)?;
        record.validate()?;
        ensure!(
            text == record.filename()
                && record.data_root == logical_root
                && record.root_identity
                    == pinned_identity(root, logical_root, record.request.namespace)?,
            "pinned funding root binding changed"
        );
        if let Some(registration) = retired_registration {
            retired::participant(&record, registration)?;
        }
        result.extend(accounting(&record)?);
        operations += 1;
        ensure!(
            operations <= MAX_OPERATIONS && result.len() <= MAX_RESERVATIONS,
            "funding inventory accounting bound exhausted"
        );
    }
    Ok(result)
}

fn validate_lease_name(name: &std::ffi::OsStr) -> Result<()> {
    let text = name.to_str().context("funded lease name UTF-8")?;
    let id: Sha256Digest = text
        .strip_prefix("l-")
        .and_then(|text| text.strip_suffix(".lease"))
        .context("unexpected funded lease name")?
        .parse()?;
    ensure!(
        text == format!("l-{id}.lease"),
        "noncanonical funded lease name"
    );
    Ok(())
}
fn accounting(record: &FundingReceipt) -> Result<Vec<LiveRecord>> {
    let mut result = Vec::new();
    for part in &record.participants {
        // Closed child credit remains held until native journal/staging recovery.
        if part.remaining_growth > 0 {
            result.push(LiveRecord {
                record: Record {
                    collection: part.collection.clone(),
                    bytes: part.remaining_growth,
                },
            });
        }
    }
    for child in &record.children {
        let part = record
            .participants
            .iter()
            .find(|part| part.index_id == child.index_id)
            .context("funding participant missing")?;
        result.push(LiveRecord {
            record: Record {
                collection: part.collection.clone(),
                bytes: child.held_bytes,
            },
        });
    }
    Ok(result)
}
pub(super) fn records(quota: &Quota, deadline: Option<Instant>) -> Result<Vec<LiveRecord>> {
    let mut result = Vec::new();
    for record in inventory(quota, deadline)? {
        result.extend(accounting(&record)?);
    }
    ensure!(
        result.len() <= MAX_RESERVATIONS,
        "funding accounting row bound exhausted"
    );
    Ok(result)
}
pub(super) fn has_allocations(quota: &Quota) -> Result<bool> {
    Ok(!inventory(quota, None)?.is_empty())
}

fn check_use(record: &FundingReceipt, owner: &FundingUse) -> Result<usize> {
    ensure!(
        !record.complete
            && record.request.operation_id == owner.operation_id
            && record.request.request_digest == owner.request_digest
            && record.data_root == owner.data_root
            && record.root_identity == owner.root_identity,
        "funding use belongs to another accepted operation"
    );
    let offset = record
        .participants
        .iter()
        .position(|part| part.index_id == owner.index_id)
        .context("funding use participant missing")?;
    ensure!(
        record.participants[offset].phase.as_ref() == Some(&owner.phase),
        "funding phase changed"
    );
    Ok(offset)
}
fn advance(record: &mut FundingReceipt) -> Result<()> {
    record.revision = record
        .revision
        .checked_add(1)
        .filter(|value| *value <= i64::MAX as u64)
        .context("funding revision exhausted")?;
    Ok(())
}

impl Quota {
    /// Idempotent post-terminal acknowledgment using existing native objects
    /// only. A removed allocation is accepted solely under its actual saved
    /// terminal root-release authority; missing roots/ledgers are not repaired.
    pub fn acknowledge_service_funding(
        root: &Path,
        operation: Incarnation,
        permit: &crate::service::registry::catalog::split::FundingTerminalPermit,
    ) -> Result<bool> {
        use crate::service::registry::catalog::setup::storage::{SetupDirectory, SetupFileMode};
        use std::ffi::OsStr;
        let namespace = permit.validate_acknowledgment_root(root, operation)?;
        let root_pin = SetupDirectory::open_canonical(root)?;
        let expected_root = pinned_identity(&root_pin, root, namespace)?;
        let _lifecycle = super::existing_service_lifecycle_lease(root)?;
        let control = root_pin.open_dir(OsStr::new(Namespace::Service.control()))?;
        let ledger = control.open_file(OsStr::new("ledger.lock"), SetupFileMode::ReadWrite)?;
        ledger
            .try_lock()
            .context("funding acknowledgment ledger is busy")?;
        let directory = match control.open_dir(OsStr::new(DIRECTORY)) {
            Ok(directory) => directory,
            Err(error)
                if error
                    .downcast_ref::<io::Error>()
                    .is_some_and(|error| error.kind() == io::ErrorKind::NotFound) =>
            {
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        let name = format!("f-{operation}.json");
        let owned = match directory.open_file(OsStr::new(&name), SetupFileMode::Read) {
            Ok(owned) => owned,
            Err(error)
                if error
                    .downcast_ref::<io::Error>()
                    .is_some_and(|error| error.kind() == io::ErrorKind::NotFound) =>
            {
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        let mut record: FundingReceipt =
            serde_json::from_slice(&directory.read_file(OsStr::new(&name), MAX_RECORD_BYTES)?)?;
        record.validate()?;
        ensure!(
            record.data_root == root
                && record.root_identity == expected_root
                && record.request.operation_id == operation
                && record.request.namespace == namespace,
            "funding acknowledgment root identity changed"
        );
        permit.validate_acknowledged(&record)?;
        if record.release_acknowledged {
            return Ok(false);
        }
        if record.children.is_empty() {
            directory.unlink_file_if_identity(OsStr::new(&name), &owned)?;
            return Ok(true);
        }
        let scratch = format!("funding-ack-{operation}-{}", record.revision);
        record.release_acknowledged = true;
        advance(&mut record)?;
        record.validate()?;
        let bytes = serde_json::to_vec(&record)?;
        let mut pending = match directory.open_file(OsStr::new(&scratch), SetupFileMode::ReadWrite)
        {
            Ok(file) => {
                let prior = directory.read_file(OsStr::new(&scratch), MAX_RECORD_BYTES)?;
                ensure!(
                    bytes.starts_with(&prior),
                    "funding acknowledgment scratch belongs to another write"
                );
                file
            }
            Err(error)
                if error
                    .downcast_ref::<io::Error>()
                    .is_some_and(|error| error.kind() == io::ErrorKind::NotFound) =>
            {
                directory.open_file(OsStr::new(&scratch), SetupFileMode::CreateNew)?
            }
            Err(error) => return Err(error),
        };
        pending.set_len(0)?;
        pending.write_all(&bytes)?;
        pending.sync_all()?;
        directory.replace_file(OsStr::new(&scratch), &pending, OsStr::new(&name), &owned)?;
        Ok(true)
    }

    /// Optimistic preview of the selected spelling. No root, control directory,
    /// policy, lease, or ledger is created; admission rechecks the digest later.
    pub fn review_service_funding_policy(root: &Path) -> Result<FundingPolicyReview> {
        use crate::service::registry::catalog::setup::storage::SetupDirectory;
        use std::ffi::OsStr;
        ensure!(
            root.is_absolute()
                && !root
                    .components()
                    .any(|part| matches!(part, Component::CurDir | Component::ParentDir)),
            "funding preview requires selected canonical root spelling"
        );
        let missing = |error: &anyhow::Error| {
            error
                .downcast_ref::<io::Error>()
                .is_some_and(|error| error.kind() == io::ErrorKind::NotFound)
        };
        let policy = match SetupDirectory::open_canonical(root) {
            Ok(root) => match root.open_dir(OsStr::new(Namespace::Service.control())) {
                Ok(control) => match control.read_file(OsStr::new("policy.json"), POLICY_MAX) {
                    Ok(bytes) => serde_json::from_slice(&bytes)?,
                    Err(error) if missing(&error) => QuotaPolicy::default(),
                    Err(error) => return Err(error),
                },
                Err(error) if missing(&error) => QuotaPolicy::default(),
                Err(error) => return Err(error),
            },
            Err(error) if missing(&error) => QuotaPolicy::default(),
            Err(error) => return Err(error),
        };
        policy.validate()?;
        Ok(FundingPolicyReview {
            data_root: root.to_path_buf(),
            digest: policy_digest(&policy)?,
            policy,
        })
    }

    pub(crate) fn takeover_cleanup_funding(
        &self,
        store: &crate::FileStore,
        writer: &crate::WriterLock,
        expected: &FundingReceipt,
        phase: FundingPhase,
    ) -> Result<(FundingUse, FundingReceipt)> {
        let permit = store
            .split_access
            .as_ref()
            .context("cleanup funding takeover requires its native worker")?;
        let owner = FundingUse {
            operation_id: expected.request.operation_id,
            request_digest: expected.request.request_digest,
            data_root: self.root.clone(),
            root_identity: expected.root_identity,
            index_id: expected.request.destination_index,
            phase: phase.clone(),
        };
        permit.validate_funding_use(&owner)?;
        ensure!(
            phase.purpose == SplitJobPurpose::DestinationCleanup,
            "only exact destination cleanup can take over an unsettled build"
        );
        let before = self.observe_funding(owner.operation_id)?;
        ensure!(
            before == *expected && !before.complete,
            "cleanup funding takeover observation changed"
        );
        let part = before
            .participants
            .iter()
            .position(|part| part.index_id == owner.index_id)
            .context("cleanup funding participant belongs to another root")?;
        super::current::validate_writer(self, writer, &before.participants[part].collection)?;
        let record = self.transfer_cleanup_children(&before, &owner, |_| Ok(()))?;
        Ok((owner, record))
    }
    pub(crate) fn takeover_retry_funding(
        &self,
        store: &crate::FileStore,
        writer: &crate::WriterLock,
        expected: &FundingReceipt,
        phase: FundingPhase,
    ) -> Result<(FundingUse, FundingReceipt)> {
        let permit = store
            .split_access
            .as_ref()
            .context("retry funding takeover requires exact claimed work")?;
        let index_id = match phase.purpose {
            SplitJobPurpose::DestinationBuild => expected.request.destination_index,
            SplitJobPurpose::OriginalRemoval | SplitJobPurpose::OriginalCleanup => {
                expected.request.original_index
            }
            SplitJobPurpose::DestinationCleanup => {
                anyhow::bail!("canceled destination uses its cleanup takeover")
            }
            SplitJobPurpose::OriginalWitness => {
                anyhow::bail!("read-only split witness has no funded mutation phase")
            }
        };
        let owner = FundingUse {
            operation_id: expected.request.operation_id,
            request_digest: expected.request.request_digest,
            data_root: self.root.clone(),
            root_identity: expected.root_identity,
            index_id,
            phase,
        };
        permit.validate_funding_use(&owner)?;
        let before = self.observe_funding(owner.operation_id)?;
        ensure!(
            &before == expected && !before.complete,
            "retry funding observation changed"
        );
        let participant = before
            .participants
            .iter()
            .find(|part| part.index_id == index_id)
            .context("retry participant belongs to another root")?;
        super::current::validate_writer(self, writer, &participant.collection)?;
        let previous = participant
            .phase
            .as_ref()
            .context("retry has no admitted prior funding phase")?;
        if previous == &owner.phase {
            check_use(&before, &owner)?;
            return Ok((owner, before));
        }
        permit.validate_funding_retry_takeover(previous, &owner)?;
        let record = self.transfer_funded_children(
            &before,
            &owner,
            FundingTakeoverKind::FailedWorkRetry,
            |_| Ok(()),
        )?;
        Ok((owner, record))
    }

    fn transfer_cleanup_children(
        &self,
        before: &FundingReceipt,
        owner: &FundingUse,
        observer: impl FnMut(PersistPoint) -> Result<()>,
    ) -> Result<FundingReceipt> {
        self.transfer_funded_children(
            before,
            owner,
            FundingTakeoverKind::CanceledDestinationCleanup,
            observer,
        )
    }
    fn transfer_funded_children(
        &self,
        before: &FundingReceipt,
        owner: &FundingUse,
        kind: FundingTakeoverKind,
        observer: impl FnMut(PersistPoint) -> Result<()>,
    ) -> Result<FundingReceipt> {
        ensure!(
            !before.complete
                && owner.operation_id == before.request.operation_id
                && owner.request_digest == before.request.request_digest
                && owner.data_root == self.root
                && owner.root_identity == before.root_identity,
            "cleanup takeover crosses operation, root, or participant"
        );
        let phase = &owner.phase;
        let part = before
            .participants
            .iter()
            .position(|part| part.index_id == owner.index_id)
            .context("cleanup funding participant belongs to another root")?;
        if before.participants[part].phase.as_ref() == Some(phase) {
            check_use(before, owner)?;
            self.validate_funding(before)?;
            return Ok(before.clone());
        }
        match kind {
            FundingTakeoverKind::CanceledDestinationCleanup => ensure!(
                owner.index_id == before.request.destination_index
                    && phase.purpose == SplitJobPurpose::DestinationCleanup,
                "cleanup takeover crosses participant"
            ),
            FundingTakeoverKind::FailedWorkRetry => {
                let previous = before.participants[part]
                    .phase
                    .as_ref()
                    .context("retry funding has no predecessor")?;
                ensure!(
                    previous.purpose == phase.purpose
                        && previous.job != phase.job
                        && match phase.purpose {
                            SplitJobPurpose::DestinationBuild =>
                                owner.index_id == before.request.destination_index,
                            SplitJobPurpose::OriginalRemoval | SplitJobPurpose::OriginalCleanup =>
                                owner.index_id == before.request.original_index,
                            SplitJobPurpose::DestinationCleanup => false,
                            SplitJobPurpose::OriginalWitness => false,
                        },
                    "retry takeover changes participant or work purpose"
                );
            }
        }
        ensure!(
            before.participants[part]
                .phase
                .as_ref()
                .is_none_or(|prior| prior.revision < phase.revision)
                && before.takeovers.len() < airk_core::catalog_split::MAX_SPLIT_JOBS,
            "cleanup funding takeover must advance within bounded lineage"
        );
        // The actual claimed cleanup worker and owning writer exclude the old
        // builder. Every retained ordinary capture additionally needs a free OS
        // lease; missing lease metadata is not permission to delete its stage.
        let mut leases = Vec::new();
        for child in before
            .children
            .iter()
            .filter(|child| child.index_id == owner.index_id)
        {
            if matches!(child.kind, FundingChildKind::Reservation { .. }) {
                let path = self
                    .control_path()
                    .join(DIRECTORY)
                    .join("leases")
                    .join(format!("l-{}.lease", child.id));
                match fs::symlink_metadata(&path) {
                    Ok(_) => {
                        unlinked(&path)?;
                        let lease = OpenOptions::new().read(true).write(true).open(path)?;
                        ensure!(
                            lease.metadata()?.len() == 0,
                            "funded lease has unexpected contents"
                        );
                        lease
                            .try_lock()
                            .context("prior split build still owns a capture lease")?;
                        leases.push(lease);
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        let _ledger = self.lock()?;
        let mut record = read_record(self, owner.operation_id)?;
        ensure!(
            &record == before,
            "cleanup funding changed while validating child leases"
        );
        let previous_observation = framed_digest(
            b"airk/split-funding-takeover/1",
            &[&serde_json::to_vec(before)?],
        );
        record.takeovers.push(FundingTakeover {
            kind,
            index_id: owner.index_id,
            previous_phase: record.participants[part].phase.clone(),
            next_phase: phase.clone(),
            previous_revision: before.revision,
            previous_observation,
        });
        record.participants[part].phase = Some(phase.clone());
        record.participants[part].settled_revision = None;
        for child in record
            .children
            .iter_mut()
            .filter(|child| child.index_id == owner.index_id)
        {
            child.phase = phase.clone();
        }
        advance(&mut record)?;
        persist_observed(self, &record, observer)?;
        drop(leases);
        Ok(record)
    }

    pub fn reserve_funding(&self, request: &FundingRequest) -> Result<FundingReceipt> {
        request.validate()?;
        let _ledger = self.lock()?;
        let identity = root_identity(self, request.namespace)?;
        let path = self
            .control_path()
            .join(DIRECTORY)
            .join(format!("f-{}.json", request.operation_id));
        if path.try_exists()? {
            let prior = read_record(self, request.operation_id)?;
            ensure!(
                &prior.request == request,
                "funding operation request changed"
            );
            return Ok(prior);
        }
        ensure!(
            inventory(self, None)?.len() < MAX_OPERATIONS,
            "funding operation count exhausted"
        );
        let records = self.live_records()?;
        let policy = self.read_policy()?;
        ensure!(
            request
                .quota_digests
                .get(self.root.to_str().context("funding root UTF-8")?)
                == Some(&policy_digest(&policy)?),
            "funding quota policy review changed"
        );
        let status = self.measure(&policy, &records, None)?;
        let mut participants = Vec::new();
        let mut additional = 0u64;
        for budget in &request.budgets {
            let registration = request
                .registration(budget.index_id)
                .context("funding participant missing")?;
            if Path::new(&registration.data_root) != self.root {
                continue;
            }
            let present = status.collections.get(&registration.collection);
            let used = present.map_or(0, |item| item.used_bytes);
            let occupied = present.map_or(Ok(0), |item| {
                checked_add(item.used_bytes, item.reserved_bytes)
            })?;
            let available = policy
                .limit(&registration.collection)
                .saturating_sub(occupied);
            ensure!(
                budget.bytes <= available,
                QuotaPressure {
                    scope: PressureScope::Collection,
                    required_bytes: budget.bytes,
                    available_bytes: available,
                    protected_bytes: None
                }
            );
            additional = checked_add(additional, budget.bytes)?;
            participants.push(ParticipantFunding {
                index_id: budget.index_id,
                collection: registration.collection.clone(),
                initial_bytes: budget.bytes,
                baseline_used_bytes: used,
                remaining_growth: budget.bytes,
                consumed_growth: 0,
                phase: None,
                settled_revision: None,
            });
        }
        ensure!(
            !participants.is_empty() && records.len() + participants.len() <= MAX_RESERVATIONS,
            "funding root has no participant or no accounting rows"
        );
        let available = policy
            .root_bytes
            .saturating_sub(checked_add(status.used_bytes, status.reserved_bytes)?);
        ensure!(
            additional <= available,
            QuotaPressure {
                scope: PressureScope::Root,
                required_bytes: additional,
                available_bytes: available,
                protected_bytes: None
            }
        );
        crate::capacity::require_growth(&self.root, additional)?;
        let record = FundingReceipt {
            version: 1,
            request: request.clone(),
            data_root: self.root.clone(),
            root_identity: identity,
            revision: 1,
            participants,
            children: Vec::new(),
            takeovers: Vec::new(),
            complete: false,
            release: None,
            release_acknowledged: false,
        };
        persist(self, &record)?;
        Ok(record)
    }
    pub fn observe_funding(&self, operation: Incarnation) -> Result<FundingReceipt> {
        let _ledger = self.lock()?;
        read_record(self, operation)
    }
    pub fn validate_funding(&self, receipt: &FundingReceipt) -> Result<()> {
        let _ledger = self.lock()?;
        ensure!(
            read_record(self, receipt.request.operation_id)? == *receipt,
            "funding observation changed"
        );
        Ok(())
    }
    pub fn begin_funding_phase(
        &self,
        receipt: &FundingReceipt,
        index_id: IndexId,
        phase: FundingPhase,
    ) -> Result<FundingUse> {
        let _ledger = self.lock()?;
        let mut record = read_record(self, receipt.request.operation_id)?;
        ensure!(
            record == *receipt && !record.complete && phase.revision > 0,
            "funding phase review changed"
        );
        let previous = record
            .participants
            .iter()
            .find(|part| part.index_id == index_id)
            .context("funding participant missing")?;
        ensure!(
            !record
                .children
                .iter()
                .any(|child| child.index_id == index_id)
                || previous.settled_revision == previous.phase.as_ref().map(|phase| phase.revision),
            "previous funded phase requires native settlement"
        );
        let participant = record
            .participants
            .iter_mut()
            .find(|part| part.index_id == index_id)
            .context("funding participant missing")?;
        ensure!(
            participant
                .phase
                .as_ref()
                .is_none_or(|prior| phase.revision > prior.revision),
            "funding phase must advance"
        );
        ensure!(
            matches!(
                (&phase.purpose, index_id == record.request.destination_index),
                (
                    SplitJobPurpose::DestinationBuild | SplitJobPurpose::DestinationCleanup,
                    true
                ) | (
                    SplitJobPurpose::OriginalRemoval | SplitJobPurpose::OriginalCleanup,
                    false
                )
            ),
            "funding purpose crosses participant"
        );
        participant.phase = Some(phase.clone());
        participant.settled_revision = None;
        for child in record
            .children
            .iter_mut()
            .filter(|child| child.index_id == index_id)
        {
            ensure!(
                !child.closed && matches!(child.kind, FundingChildKind::Current { .. }),
                "only a settled current recovery allocation can cross a phase"
            );
            child.phase = phase.clone();
        }
        advance(&mut record)?;
        persist(self, &record)?;
        Ok(FundingUse {
            operation_id: record.request.operation_id,
            request_digest: record.request.request_digest,
            data_root: self.root.clone(),
            root_identity: record.root_identity,
            index_id,
            phase,
        })
    }
    pub fn with_funding(mut self, owner: FundingUse) -> Result<Self> {
        let _ledger = self.lock()?;
        check_use(&read_record(&self, owner.operation_id)?, &owner)?;
        self.funding = Some(owner);
        Ok(self)
    }
    pub fn funding_use(&self) -> Option<&FundingUse> {
        self.funding.as_ref()
    }
    /// Native writer planning only. Public quota status still reports all held
    /// reservations; prepaid growth must not be mistaken for global free space.
    pub fn operation_growth_headroom(&self, collection: &str) -> Result<Option<u64>> {
        let Some(owner) = &self.funding else {
            return Ok(None);
        };
        let _ledger = self.lock()?;
        let record = read_record(self, owner.operation_id)?;
        let participant = check_use(&record, owner)?;
        ensure!(
            record.participants[participant].collection == collection,
            "funded growth planning crosses participant"
        );
        Ok(Some(record.participants[participant].remaining_growth))
    }
    pub fn funding_policy_digest(&self) -> Result<Sha256Digest> {
        policy_digest(&self.policy()?)
    }
    pub fn adopt_funding_phase(
        &self,
        receipt: &FundingReceipt,
        index_id: IndexId,
        phase: &FundingPhase,
    ) -> Result<FundingUse> {
        let _ledger = self.lock()?;
        let record = read_record(self, receipt.request.operation_id)?;
        ensure!(
            record == *receipt && !record.complete,
            "funding recovery observation changed"
        );
        let owner = FundingUse {
            operation_id: record.request.operation_id,
            request_digest: record.request.request_digest,
            data_root: self.root.clone(),
            root_identity: record.root_identity,
            index_id,
            phase: phase.clone(),
        };
        check_use(&record, &owner)?;
        Ok(owner)
    }
    pub(crate) fn validate_funding_store(
        &self,
        owner: &FundingUse,
        store: &crate::FileStore,
    ) -> Result<()> {
        let _ledger = self.lock()?;
        let record = read_record(self, owner.operation_id)?;
        let offset = check_use(&record, owner)?;
        ensure!(
            store.managed_path()?.root == self.root
                && store.collection_id()? == record.participants[offset].collection,
            "funding runtime capability crosses collection"
        );
        Ok(())
    }
    pub(crate) fn require_store_funding(&self, store: &crate::FileStore) -> Result<()> {
        match (&self.funding, &store.funding) {
            (None, None) => Ok(()),
            (Some(left), Some(right))
                if left.operation_id == right.operation_id
                    && left.request_digest == right.request_digest
                    && left.index_id == right.index_id
                    && left.phase == right.phase
                    && left.root_identity == right.root_identity =>
            {
                self.validate_funding_store(left, store)
            }
            _ => anyhow::bail!("writer lost its exact operation funding capability"),
        }
    }
}

fn policy_digest(policy: &QuotaPolicy) -> Result<Sha256Digest> {
    use sha2::{Digest, Sha256};
    Ok(Sha256Digest::from_bytes(
        Sha256::digest(serde_json::to_vec(policy)?).into(),
    ))
}

fn allocate(
    quota: &Quota,
    owner: &FundingUse,
    kind: FundingChildKind,
    bytes: u64,
    available: bool,
    deadline: Option<Instant>,
) -> Result<FundedChildRef> {
    ensure!(
        matches!(kind, FundingChildKind::Current { .. }),
        "reservation allocation requires its prepared lease handoff"
    );
    Ok(allocate_prepared(quota, owner, kind, bytes, available, deadline, |_| Ok(()))?.0)
}

fn allocate_prepared(
    quota: &Quota,
    owner: &FundingUse,
    kind: FundingChildKind,
    bytes: u64,
    available: bool,
    deadline: Option<Instant>,
    mut observer: impl FnMut(PersistPoint) -> Result<()>,
) -> Result<(FundedChildRef, Option<PreparedReservationLease>)> {
    within_deadline(deadline)?;
    let mut record = read_record(quota, owner.operation_id)?;
    let participant = check_use(&record, owner)?;
    ensure!(
        record.children.len() < MAX_CHILDREN
            && quota.records_without_recovery(deadline)?.len() < MAX_RESERVATIONS,
        "funded child accounting capacity exhausted"
    );
    let bytes = if available {
        bytes.min(record.participants[participant].remaining_growth)
    } else {
        bytes
    };
    ensure!(
        bytes > 0 && bytes <= record.participants[participant].remaining_growth,
        QuotaPressure {
            scope: PressureScope::Reservation,
            required_bytes: bytes,
            available_bytes: record.participants[participant].remaining_growth,
            protected_bytes: None
        }
    );
    // The persisted revision cannot restart when a worker adopts its phase.
    let ordinal = record.revision;
    let id = match &kind {
        FundingChildKind::Current { allocation, .. } => framed_digest(
            b"airk/funded-current/1",
            &[
                owner.operation_id.as_bytes(),
                owner.request_digest.as_bytes(),
                &serde_json::to_vec(allocation)?,
            ],
        ),
        FundingChildKind::Reservation { .. } => reservation_identity(owner, ordinal)?,
    };
    ensure!(
        !record.children.iter().any(|child| child.id == id),
        "funded child exists; exact native recovery adoption required"
    );
    let reservation = matches!(&kind, FundingChildKind::Reservation { .. });
    let kind = match kind {
        FundingChildKind::Reservation { .. } => FundingChildKind::Reservation {
            staging_name: format!("r-funded-{id}"),
        },
        value => value,
    };
    record.participants[participant].remaining_growth -= bytes;
    record.participants[participant].settled_revision = None;
    record.children.push(FundingChild {
        id,
        index_id: owner.index_id,
        phase: owner.phase.clone(),
        kind,
        capacity: bytes,
        held_bytes: bytes,
        charged: 0,
        closed: reservation,
        current_owned: false,
    });
    advance(&mut record)?;
    let reference = FundedChildRef {
        owner: owner.clone(),
        child: id,
    };
    // A prepared closed child owns its credit and prospective staging name but
    // has never exposed a live capture. Existing native settlement can recover
    // this state even if no lease could be created, including after a takeover.
    within_deadline(deadline)?;
    persist_observed(quota, &record, &mut observer)?;
    if !reservation {
        return Ok((reference, None));
    }
    let mut lease = PreparedReservationLease::open(quota, id, true)?;
    record
        .children
        .last_mut()
        .context("allocated child missing")?
        .closed = false;
    advance(&mut record)?;
    within_deadline(deadline)?;
    // Opening can commit and still report a synchronization failure. Keep the
    // lease on disk once this boundary is attempted; a File is not a live
    // Reservation whose destructor could recursively acquire this ledger.
    lease.created = false;
    persist_observed(quota, &record, observer)?;
    Ok((reference, Some(lease)))
}

fn reservation_identity(owner: &FundingUse, ordinal: u64) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"airk/funded-reservation/1",
        &[
            owner.operation_id.as_bytes(),
            owner.request_digest.as_bytes(),
            owner.index_id.identity().as_bytes(),
            &serde_json::to_vec(&owner.phase)?,
            &ordinal.to_be_bytes(),
        ],
    ))
}

struct PreparedReservationLease {
    directory: crate::service::registry::catalog::setup::storage::SetupDirectory,
    name: std::ffi::OsString,
    file: Option<File>,
    created: bool,
}
impl PreparedReservationLease {
    fn open(quota: &Quota, child: Sha256Digest, allow_create: bool) -> Result<Self> {
        use crate::service::registry::catalog::setup::storage::{SetupDirectory, SetupFileMode};
        let parent = SetupDirectory::open_canonical(&quota.control_path().join(DIRECTORY))?;
        let directory = if allow_create {
            parent.create_dir(std::ffi::OsStr::new("leases"))?
        } else {
            parent.open_dir(std::ffi::OsStr::new("leases"))?
        };
        let name: std::ffi::OsString = format!("l-{child}.lease").into();
        let (file, created) = if !allow_create {
            (directory.open_file(&name, SetupFileMode::ReadWrite)?, false)
        } else {
            match directory.open_file(&name, SetupFileMode::CreateNew) {
                Ok(file) => (file, true),
                Err(error)
                    if error
                        .downcast_ref::<io::Error>()
                        .is_some_and(|error| error.kind() == io::ErrorKind::AlreadyExists) =>
                {
                    // The durable closed child already owns this exact name.
                    // Never sweep unrelated well-formed orphan names.
                    (directory.open_file(&name, SetupFileMode::ReadWrite)?, false)
                }
                Err(error) => return Err(error),
            }
        };
        let pending = Self {
            directory,
            name,
            file: Some(file),
            created,
        };
        let file = pending
            .file
            .as_ref()
            .context("funded lease handle missing")?;
        file.try_lock()
            .context("funded reservation has a live owner")?;
        ensure!(
            file.metadata()?.len() == 0,
            "funded lease has unexpected contents"
        );
        file.sync_all()?;
        pending.directory.sync()?;
        parent.sync()?;
        Ok(pending)
    }
    fn published(mut self) -> Result<File> {
        self.created = false;
        self.file
            .take()
            .context("prepared funded lease handle missing")
    }
}
impl Drop for PreparedReservationLease {
    fn drop(&mut self) {
        if self.created
            && let Some(file) = &self.file
        {
            // Roll back only the exact newly created inode, before publication.
            let _ = self.directory.unlink_file_if_identity(&self.name, file);
        }
    }
}

pub(super) fn reserve(
    quota: &Quota,
    owner: &FundingUse,
    collection: &str,
    bytes: u64,
    available: bool,
    deadline: Option<Instant>,
) -> Result<Reservation> {
    let ledger = quota.lock_before(deadline)?;
    let record = read_record(quota, owner.operation_id)?;
    let offset = check_use(&record, owner)?;
    ensure!(
        record.participants[offset].collection == collection,
        "funded reservation crosses participant"
    );
    let (child, lease) = allocate_prepared(
        quota,
        owner,
        FundingChildKind::Reservation {
            staging_name: String::new(),
        },
        bytes,
        available,
        deadline,
        |_| Ok(()),
    )?;
    reservation_handle_with_lease(
        quota,
        child,
        ledger.lifecycle,
        lease
            .context("prepared funded lease missing")?
            .published()?,
    )
}
fn reservation_handle_with_lease(
    quota: &Quota,
    child: FundedChildRef,
    lifecycle: Arc<File>,
    lease: File,
) -> Result<Reservation> {
    let record = read_record(quota, child.owner.operation_id)?;
    let saved = record
        .children
        .iter()
        .find(|entry| entry.id == child.child)
        .context("funded reservation missing")?;
    let FundingChildKind::Reservation { staging_name } = &saved.kind else {
        anyhow::bail!("funded child is not a reservation");
    };
    let directory = quota.control_path().join(DIRECTORY).join("leases");
    let lease_path = directory.join(format!("l-{}.lease", child.child));
    let collection = record
        .request
        .registration(saved.index_id)
        .context("funded collection missing")?
        .collection
        .clone();
    Ok(Reservation(Arc::new(ReservationInner {
        quota: quota.clone(),
        collection,
        record_path: quota.control_path().join("reservations").join(staging_name),
        lease_path,
        lease: Some(lease),
        limit: saved.capacity,
        charged: AtomicU64::new(saved.charged),
        best_effort: false,
        funded: Some(child),
        _lifecycle: lifecycle,
    })))
}
pub(super) fn charge(quota: &Quota, reference: &FundedChildRef, bytes: u64) -> Result<u64> {
    let _ledger = quota.lock()?;
    let mut record = read_record(quota, reference.owner.operation_id)?;
    check_use(&record, &reference.owner)?;
    let child = record
        .children
        .iter_mut()
        .find(|child| child.id == reference.child)
        .context("funded reservation missing")?;
    ensure!(
        !child.closed && child.phase == reference.owner.phase,
        "funded reservation phase changed"
    );
    child.charged = checked_add(child.charged, bytes)?;
    ensure!(
        child.charged <= child.capacity,
        "funded reservation exhausted"
    );
    let charged = child.charged;
    let part = check_use(&record, &reference.owner)?;
    record.participants[part].settled_revision = None;
    advance(&mut record)?;
    persist(quota, &record)?;
    Ok(charged)
}
pub(super) fn close_reservation(quota: &Quota, reference: &FundedChildRef) -> Result<()> {
    let _ledger = quota.lock()?;
    let mut record = read_record(quota, reference.owner.operation_id)?;
    check_use(&record, &reference.owner)?;
    let child = record
        .children
        .iter_mut()
        .find(|child| child.id == reference.child)
        .context("funded reservation missing")?;
    ensure!(
        matches!(child.kind, FundingChildKind::Reservation { .. }),
        "funded close kind changed"
    );
    child.closed = true;
    let part = check_use(&record, &reference.owner)?;
    record.participants[part].settled_revision = None;
    advance(&mut record)?;
    persist(quota, &record)
}

fn reference(record: &FundingReceipt, child: &FundingChild) -> FundedChildRef {
    FundedChildRef {
        owner: FundingUse {
            operation_id: record.request.operation_id,
            request_digest: record.request.request_digest,
            data_root: record.data_root.clone(),
            root_identity: record.root_identity,
            index_id: child.index_id,
            phase: child.phase.clone(),
        },
        child: child.id,
    }
}

pub(super) fn current_references(quota: &Quota) -> Result<Vec<FundedChildRef>> {
    let mut result = Vec::new();
    for record in inventory(quota, None)? {
        for child in &record.children {
            if !child.closed && matches!(child.kind, FundingChildKind::Current { .. }) {
                result.push(reference(&record, child));
            }
        }
    }
    Ok(result)
}
pub(super) fn find_current(
    quota: &Quota,
    scope: &CurrentGrantScope,
) -> Result<Option<FundedChildRef>> {
    let mut result = None;
    for reference in current_references(quota)? {
        if current_state(quota, &reference)?.0 == *scope {
            ensure!(result.is_none(), "duplicate funded current scope");
            result = Some(reference);
        }
    }
    Ok(result)
}
pub(super) fn current_state(
    quota: &Quota,
    reference: &FundedChildRef,
) -> Result<(CurrentGrantScope, CurrentGrantScope, u64)> {
    let record = read_record(quota, reference.owner.operation_id)?;
    let child = record
        .children
        .iter()
        .find(|child| child.id == reference.child)
        .context("funded current child missing")?;
    ensure!(
        !child.closed
            && child.index_id == reference.owner.index_id
            && child.phase == reference.owner.phase,
        "funded current child binding changed"
    );
    let FundingChildKind::Current { scope, allocation } = &child.kind else {
        anyhow::bail!("funded current kind changed");
    };
    Ok((scope.clone(), allocation.clone(), child.capacity))
}
pub(super) fn require_current_authority(quota: &Quota, reference: &FundedChildRef) -> Result<()> {
    let record = read_record(quota, reference.owner.operation_id)?;
    let child = record
        .children
        .iter()
        .find(|child| child.id == reference.child)
        .context("funded current child missing")?;
    if child.current_owned && record.complete {
        return Ok(());
    }
    let owner = quota.funding.as_ref().context(
        "funded current journal requires exact operation recovery before orphan release",
    )?;
    ensure!(
        owner.operation_id == reference.owner.operation_id
            && owner.request_digest == reference.owner.request_digest
            && owner.index_id == reference.owner.index_id
            && owner.phase == reference.owner.phase,
        "funded current recovery belongs to another operation phase"
    );
    check_use(&record, owner)?;
    Ok(())
}
pub(super) fn reserve_current(
    quota: &Quota,
    owner: &FundingUse,
    scope: CurrentGrantScope,
    bytes: u64,
) -> Result<FundedChildRef> {
    let record = read_record(quota, owner.operation_id)?;
    let offset = check_use(&record, owner)?;
    ensure!(
        record.participants[offset].collection == scope.collection
            && find_current(quota, &scope)?.is_none(),
        "funded current scope already exists or crosses participant"
    );
    allocate(
        quota,
        owner,
        FundingChildKind::Current {
            allocation: scope.clone(),
            scope,
        },
        bytes,
        false,
        None,
    )
}
pub(super) fn resize_current(quota: &Quota, reference: &FundedChildRef, bytes: u64) -> Result<()> {
    require_current_authority(quota, reference)?;
    let mut record = read_record(quota, reference.owner.operation_id)?;
    let offset = record
        .children
        .iter()
        .position(|child| child.id == reference.child)
        .context("funded current missing")?;
    let extra = bytes.saturating_sub(record.children[offset].held_bytes);
    if record.complete {
        if extra > 0 {
            super::current::admit(
                quota,
                &current_state(quota, reference)?.0.collection,
                extra,
                false,
            )?;
        }
    } else {
        let participant = check_use(&record, &reference.owner)?;
        ensure!(
            extra <= record.participants[participant].remaining_growth,
            "funded current resize exceeds admitted capacity"
        );
        record.participants[participant].remaining_growth -= extra;
        record.participants[participant].settled_revision = None;
    }
    record.children[offset].held_bytes = checked_add(record.children[offset].held_bytes, extra)?;
    record.children[offset].capacity = bytes;
    if record.complete {
        record.children[offset].held_bytes = bytes;
    }
    advance(&mut record)?;
    persist(quota, &record)
}
pub(super) fn release_current(quota: &Quota, reference: &FundedChildRef) -> Result<()> {
    require_current_authority(quota, reference)?;
    let mut record = read_record(quota, reference.owner.operation_id)?;
    let offset = record
        .children
        .iter()
        .position(|child| child.id == reference.child)
        .context("funded current missing")?;
    if record.complete {
        record.children.remove(offset);
    } else {
        record.children[offset].closed = true;
    }
    if !record.complete {
        let part = check_use(&record, &reference.owner)?;
        record.participants[part].settled_revision = None;
    }
    advance(&mut record)?;
    // Complete operation evidence remains bounded until its final current child
    // retires. The native catalog retains historical admission/completion proof.
    if record.complete && record.release_acknowledged && record.children.is_empty() {
        fs::remove_file(quota.control_path().join(DIRECTORY).join(record.filename()))?;
        crate::maintenance::sync_parent(&quota.control_path().join(DIRECTORY))
    } else {
        persist(quota, &record)
    }
}
pub(super) fn rebind_current(
    quota: &Quota,
    reference: &FundedChildRef,
    scope: &CurrentGrantScope,
) -> Result<()> {
    require_current_authority(quota, reference)?;
    let (old, _, _) = current_state(quota, reference)?;
    ensure!(
        scope.collection == old.collection
            && scope.incarnation == old.incarnation
            && find_current(quota, scope)?.is_none(),
        "funded current rebind crosses scope or duplicates grant"
    );
    let mut record = read_record(quota, reference.owner.operation_id)?;
    let child = record
        .children
        .iter_mut()
        .find(|child| child.id == reference.child)
        .context("funded current missing")?;
    let FundingChildKind::Current { scope: current, .. } = &mut child.kind else {
        anyhow::bail!("funded current kind changed");
    };
    *current = scope.clone();
    advance(&mut record)?;
    persist(quota, &record)
}

impl Quota {
    /// The permit is produced from persisted native physical-cleanup
    /// authorization. Pure frontend receipt hashes cannot release root funding.
    pub fn finish_funding(
        &self,
        expected: &FundingReceipt,
        permit: &crate::service::registry::catalog::split::FundingTerminalPermit,
    ) -> Result<FundingReceipt> {
        let _ledger = self.lock()?;
        let mut record = read_record(self, expected.request.operation_id)?;
        ensure!(record == *expected, "funding release observation changed");
        permit.validate(&record)?;
        if record.complete {
            return Ok(record);
        }
        for participant in &record.participants {
            ensure!(
                participant.phase.is_none()
                    || participant.settled_revision
                        == participant.phase.as_ref().map(|phase| phase.revision),
                "funding phase journal/staging has not settled"
            );
        }
        ensure!(
            record.children.iter().all(
                |child| !child.closed && matches!(child.kind, FundingChildKind::Current { .. })
            ),
            "funding retains unsettled capture or journal children"
        );
        let unused_growth_released = record
            .participants
            .iter()
            .map(|part| (part.index_id, part.remaining_growth))
            .collect();
        let current_allocations_retained = record
            .children
            .iter()
            .map(|child| match &child.kind {
                FundingChildKind::Current { scope, allocation } => RetainedCurrentFunding {
                    id: child.id,
                    index_id: child.index_id,
                    scope: scope.clone(),
                    allocation: allocation.clone(),
                    capacity: child.capacity,
                    held_bytes: child.held_bytes,
                },
                _ => unreachable!(),
            })
            .collect();
        for participant in &mut record.participants {
            participant.remaining_growth = 0;
        }
        for child in &mut record.children {
            child.current_owned = true;
        }
        record.complete = true;
        advance(&mut record)?;
        record.release = Some(FundingRelease {
            revision: record.revision,
            unused_growth_released,
            current_allocations_retained,
        });
        persist(self, &record)?;
        Ok(record)
    }

    /// Terminal native acknowledgment precedes forgetting an empty allocation.
    /// Actual current children stay discoverable under their normal writer.
    pub fn forget_released_funding(
        &self,
        expected: &FundingReceipt,
        permit: &crate::service::registry::catalog::split::FundingTerminalPermit,
    ) -> Result<bool> {
        let _ledger = self.lock()?;
        let mut record = read_record(self, expected.request.operation_id)?;
        ensure!(
            record == *expected && record.complete,
            "funding acknowledgment observation changed"
        );
        permit.validate_acknowledged(&record)?;
        if record.children.is_empty() {
            fs::remove_file(self.control_path().join(DIRECTORY).join(record.filename()))?;
            crate::maintenance::sync_parent(&self.control_path().join(DIRECTORY))?;
            return Ok(true);
        }
        record.release_acknowledged = true;
        advance(&mut record)?;
        persist(self, &record)?;
        Ok(false)
    }

    /// Enumerate exact durable child identities before operation recovery. This
    /// does not infer orphanhood from lease absence or delete staging.
    pub fn funded_children(&self, owner: &FundingUse) -> Result<Vec<FundingChild>> {
        let _ledger = self.lock()?;
        let record = read_record(self, owner.operation_id)?;
        check_use(&record, owner)?;
        Ok(record
            .children
            .into_iter()
            .filter(|child| child.index_id == owner.index_id)
            .collect())
    }
    /// Exact deferred staging inventory for a canceled participant. Recognition
    /// is not deletion authority: only native disposed-current reconciliation
    /// may remove these bytes or release their still-held capacity.
    pub(crate) fn operation_staging_for_disposal(
        &self,
        owner: &FundingUse,
    ) -> Result<std::collections::BTreeSet<PathBuf>> {
        let record = self.observe_funding(owner.operation_id)?;
        check_use(&record, owner)?;
        ensure!(
            owner.phase.purpose == SplitJobPurpose::DestinationCleanup
                && owner.index_id == record.request.destination_index,
            "staging disposal requires exact destination cleanup"
        );
        let registration = record
            .request
            .registration(owner.index_id)
            .context("staging participant absent")?;
        let mut paths = std::collections::BTreeSet::new();
        for child in record
            .children
            .iter()
            .filter(|child| child.index_id == owner.index_id)
        {
            if let FundingChildKind::Reservation { staging_name } = &child.kind {
                ensure!(
                    child.closed && child.phase == owner.phase,
                    "funded staging still has an open or foreign capture"
                );
                paths.insert(staging_path(
                    self,
                    &registration.collection,
                    &self.control_path().join("reservations").join(staging_name),
                )?);
            }
        }
        ensure!(
            paths.len() <= MAX_CHILDREN,
            "funded staging inventory exceeds bound"
        );
        Ok(paths)
    }

    /// Exact native claim/writer custody may close dead capture handles before
    /// journal recovery. This NEVER removes staging, releases credit, changes
    /// Current grants or declares Ready. Physical settlement still requires the
    /// existing actual Ready/no-current/disposed owner proof.
    pub(crate) fn close_abandoned_funded_reservations(&self, owner: &FundingUse) -> Result<usize> {
        let before = self.observe_funding(owner.operation_id)?;
        check_use(&before, owner)?;
        let open = before
            .children
            .iter()
            .filter(|child| {
                child.index_id == owner.index_id
                    && !child.closed
                    && matches!(child.kind, FundingChildKind::Reservation { .. })
            })
            .collect::<Vec<_>>();
        ensure!(
            open.len() <= MAX_CHILDREN,
            "abandoned capture inventory exceeds bound"
        );
        if open.is_empty() {
            return Ok(0);
        }
        let mut leases = Vec::with_capacity(open.len());
        for child in &open {
            ensure!(
                child.phase == owner.phase,
                "abandoned capture phase differs"
            );
            let path = self
                .control_path()
                .join(DIRECTORY)
                .join("leases")
                .join(format!("l-{}.lease", child.id));
            // Missing metadata on an open durable child is uncertainty, never
            // positive proof that its owner stopped or that its staging is safe.
            unlinked(&path)?;
            let lease = OpenOptions::new().read(true).write(true).open(path)?;
            ensure!(
                lease.metadata()?.len() == 0,
                "funded lease has unexpected contents"
            );
            lease
                .try_lock()
                .context("claimed recovery capture remains live")?;
            leases.push(lease);
        }
        let _ledger = self.lock()?;
        let mut record = read_record(self, owner.operation_id)?;
        ensure!(
            record == before,
            "funded capture observation changed during native custody transfer"
        );
        let ids = open
            .iter()
            .map(|child| child.id)
            .collect::<std::collections::BTreeSet<_>>();
        for child in &mut record.children {
            if ids.contains(&child.id) {
                child.closed = true;
            }
        }
        let participant = check_use(&record, owner)?;
        record.participants[participant].settled_revision = None;
        advance(&mut record)?;
        persist(self, &record)?;
        drop(leases);
        Ok(ids.len())
    }

    /// Native recovery first validates its accepted job/stage and journal outside
    /// the ledger. The exact persisted staging/charge identity is then adopted.
    pub fn adopt_funded_reservation(
        &self,
        owner: &FundingUse,
        id: Sha256Digest,
    ) -> Result<Reservation> {
        self.adopt_funded_reservation_observed(owner, id, |_| Ok(()))
    }
    fn adopt_funded_reservation_observed(
        &self,
        owner: &FundingUse,
        id: Sha256Digest,
        observer: impl FnMut(PersistPoint) -> Result<()>,
    ) -> Result<Reservation> {
        let ledger = self.lock()?;
        let mut record = read_record(self, owner.operation_id)?;
        check_use(&record, owner)?;
        let offset = record
            .children
            .iter()
            .position(|child| child.id == id)
            .context("funded reservation missing")?;
        ensure!(
            record.children[offset].index_id == owner.index_id
                && record.children[offset].phase == owner.phase
                && matches!(
                    record.children[offset].kind,
                    FundingChildKind::Reservation { .. }
                ),
            "funded staging recovery identity changed"
        );
        let mut lease = PreparedReservationLease::open(self, id, record.children[offset].closed)?;
        record.children[offset].closed = false;
        let part = check_use(&record, owner)?;
        record.participants[part].settled_revision = None;
        advance(&mut record)?;
        lease.created = false;
        persist_observed(self, &record, observer)?;
        reservation_handle_with_lease(
            self,
            FundedChildRef {
                owner: owner.clone(),
                child: id,
            },
            ledger.lifecycle.clone(),
            lease.published()?,
        )
    }
    /// Existing native writer/journal owner invokes this only after Ready/no
    /// pending, after closing captures. Cleanup runs outside ledger.lock; missing
    /// or malformed stage/journal proof preserves every funded allowance.
    pub(crate) fn reconcile_funding_phase(
        &self,
        writer: &crate::WriterLock,
        owner: &FundingUse,
        preserved_current: Option<&CurrentGrantScope>,
    ) -> Result<FundingReceipt> {
        self.reconcile_funding_phase_preserving(writer, owner, preserved_current, &[])
    }
    pub(crate) fn reconcile_funding_phase_preserving(
        &self,
        writer: &crate::WriterLock,
        owner: &FundingUse,
        preserved_current: Option<&CurrentGrantScope>,
        preserved: &[&Reservation],
    ) -> Result<FundingReceipt> {
        self.reconcile_funding_phase_preserving_observed(
            writer,
            owner,
            preserved_current,
            preserved,
            || {},
        )
    }
    fn reconcile_funding_phase_preserving_observed(
        &self,
        writer: &crate::WriterLock,
        owner: &FundingUse,
        preserved_current: Option<&CurrentGrantScope>,
        preserved: &[&Reservation],
        after_cleanup: impl FnOnce(),
    ) -> Result<FundingReceipt> {
        super::current::validate_writer(
            self,
            writer,
            &self
                .observe_funding(owner.operation_id)?
                .request
                .registration(owner.index_id)
                .context("funded participant missing")?
                .collection,
        )?;
        let before = self.observe_funding(owner.operation_id)?;
        check_use(&before, owner)?;
        let mut preserved_ids = std::collections::BTreeSet::new();
        for reservation in preserved {
            let reference = reservation
                .0
                .funded
                .as_ref()
                .context("live checkpoint reservation is not operation funded")?;
            check_use(&before, &reference.owner)?;
            ensure!(
                reference.owner.index_id == owner.index_id
                    && reference.owner.phase == owner.phase
                    && reservation.0.lease.is_some()
                    && preserved_ids.insert(reference.child),
                "live checkpoint reservation crosses participant/phase or repeats a handle"
            );
            ensure!(
                before
                    .children
                    .iter()
                    .any(|child| child.id == reference.child
                        && !child.closed
                        && matches!(child.kind, FundingChildKind::Reservation { .. })),
                "live checkpoint handle has no open funded child"
            );
        }
        let mut leases = Vec::new();
        for child in before
            .children
            .iter()
            .filter(|child| child.index_id == owner.index_id)
        {
            if let FundingChildKind::Current { scope, .. } = &child.kind {
                ensure!(
                    child.closed || preserved_current == Some(scope),
                    "funded current journal still owns capacity"
                );
                continue;
            }
            if !child.closed {
                ensure!(
                    preserved_ids.contains(&child.id),
                    "funded capture has not reached its native settlement boundary"
                );
                continue;
            }
            let path = self
                .control_path()
                .join(DIRECTORY)
                .join("leases")
                .join(format!("l-{}.lease", child.id));
            // A prior exact settlement may have removed this lease immediately
            // before its atomic record replacement. The closed durable child
            // still holds all credit and is safe to reconcile again.
            match fs::symlink_metadata(&path) {
                Ok(_) => unlinked(&path)?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            }
            let lease = OpenOptions::new().read(true).write(true).open(&path)?;
            lease.try_lock().context("funded capture remains live")?;
            leases.push((path, lease));
        }
        // Validate every child/lease before deleting any staging. A later live
        // owner cannot turn an earlier partial sweep into an apparent settlement.
        for child in before
            .children
            .iter()
            .filter(|child| child.index_id == owner.index_id)
        {
            if child.closed
                && let FundingChildKind::Reservation { staging_name } = &child.kind
            {
                cleanup_staging(
                    self,
                    &before
                        .request
                        .registration(owner.index_id)
                        .unwrap()
                        .collection,
                    &self.control_path().join("reservations").join(staging_name),
                )?;
            }
        }
        // A measured consumption checkpoint is not release-then-reserve. Parent,
        // remaining children and consumed growth change in ONE synced record.
        after_cleanup();
        let _ledger = self.lock()?;
        let mut record = read_record(self, owner.operation_id)?;
        let mut comparable = record.clone();
        if !preserved_ids.is_empty() {
            // Capture providers may finish already-authorized writes while
            // exact closed staging is cleaned outside ledger. Only monotonic
            // charge changes on the actual borrowed live children are allowed.
            for child in comparable
                .children
                .iter_mut()
                .filter(|child| preserved_ids.contains(&child.id))
            {
                let prior = before
                    .children
                    .iter()
                    .find(|prior| prior.id == child.id)
                    .context("preserved child disappeared")?;
                ensure!(
                    child.charged >= prior.charged && !child.closed,
                    "live checkpoint child charge regressed or closed"
                );
                child.charged = prior.charged;
            }
            comparable.revision = before.revision;
        }
        ensure!(
            comparable == before,
            "funding changed during native settlement"
        );
        let policy = self.read_policy()?;
        let status = self.measure(&policy, &self.records_without_recovery(None)?, None)?;
        let part = check_use(&record, owner)?;
        let collection = record.participants[part].collection.clone();
        let used = status
            .collections
            .get(&collection)
            .map_or(0, |item| item.used_bytes);
        record
            .children
            .retain(|child| child.index_id != owner.index_id || !child.closed);
        let held = record
            .children
            .iter()
            .filter(|child| child.index_id == owner.index_id)
            .try_fold(0u64, |total, child| checked_add(total, child.held_bytes))?;
        let consumed = used.saturating_sub(record.participants[part].baseline_used_bytes);
        let committed = checked_add(held, consumed)?;
        ensure!(
            committed <= record.participants[part].initial_bytes,
            "native phase growth exceeds its accepted funded peak"
        );
        record.participants[part].consumed_growth = consumed;
        record.participants[part].remaining_growth =
            record.participants[part].initial_bytes - committed;
        record.participants[part].settled_revision = if record.children.iter().any(|child| {
            child.index_id == owner.index_id
                && !child.closed
                && matches!(child.kind, FundingChildKind::Reservation { .. })
        }) {
            None
        } else {
            Some(owner.phase.revision)
        };
        advance(&mut record)?;
        // Remove only the exact closed leases while the ledger excludes an
        // adopter. If replacement fails, old closed children remain fully
        // funded and the next native settlement tolerates their missing leases.
        for (path, lease) in leases {
            drop(lease);
            fs::remove_file(path)?;
        }
        persist(self, &record)?;
        Ok(record)
    }
}

#[path = "funding_retired.rs"]
mod retired;
pub(super) use retired::RetiredCurrentInventory;
pub(super) use retired::clean_break_inventory;
