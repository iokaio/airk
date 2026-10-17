// SPDX-License-Identifier: Apache-2.0
//! Opt-in historical selection and bounded operator mutations, separate from publication identity.
use crate::retention::MaintenanceBudget;
use airk_core::identity::{Incarnation, RevisionToken, Sha256Digest, framed_digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const HISTORY_POLICY_VERSION: u32 = 1;
pub const HISTORY_ADMIN_VERSION: u32 = 1;
pub const MAX_HISTORY_ADMIN_BYTES: usize = 65536;
pub const MAX_OPERATOR_RECEIPTS: usize = 32;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryWorkBudget {
    #[serde(flatten)]
    pub maintenance: MaintenanceBudget,
    pub verification_bytes: u64,
}
impl Default for HistoryWorkBudget {
    fn default() -> Self {
        Self {
            maintenance: MaintenanceBudget::default(),
            // Authentication streams whole surviving catalogs, not just their
            // inventories. Keep this I/O allowance separate from the metadata
            // reservation so a multi-GiB catalog can finish object GC.
            verification_bytes: 8 * 1024 * 1024 * 1024,
        }
    }
}
impl HistoryWorkBudget {
    pub(crate) fn validate(self) -> Result<()> {
        self.maintenance.validate()?;
        ensure!(
            (16384..=8 * 1024 * 1024 * 1024).contains(&self.verification_bytes),
            "history verification budget outside supported bounds"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryPolicyV1 {
    pub version: u32,
    pub enabled: bool,
    pub recent: u8,
    pub hourly_hours: u8,
    pub max_packages: u16,
    pub max_bytes: u64,
    pub quota_basis_points: u16,
}
impl Default for HistoryPolicyV1 {
    fn default() -> Self {
        Self {
            version: 1,
            enabled: false,
            recent: 3,
            hourly_hours: 24,
            max_packages: 12,
            max_bytes: 256 * 1024 * 1024,
            quota_basis_points: 1000,
        }
    }
}
impl HistoryPolicyV1 {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == HISTORY_POLICY_VERSION
                && self.recent <= 3
                && self.hourly_hours <= 24
                && (1..=12).contains(&self.max_packages)
                && (1..=256 * 1024 * 1024).contains(&self.max_bytes)
                && (1..=1000).contains(&self.quota_basis_points),
            "invalid history policy bounds"
        );
        Ok(())
    }
    pub fn byte_limit(&self, collection_limit: u64) -> u64 {
        self.max_bytes.min(
            (u128::from(collection_limit) * u128::from(self.quota_basis_points) / 10000) as u64,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryPolicyReceipt {
    pub policy: HistoryPolicyV1,
    pub sequence: u64,
    pub digest: Sha256Digest,
    pub explicit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAction {
    ConfigureHistory,
    CreateCheckpoint,
    RemoveCheckpoint,
    Purge,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorReceipt {
    pub version: u32,
    pub sequence: u64,
    pub head: RevisionToken,
    pub action: OperatorAction,
    pub operation_digest: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdminState {
    pub version: u32,
    pub incarnation: Incarnation,
    pub sequence: u64,
    pub policy: HistoryPolicyReceipt,
    pub checkpoints: BTreeMap<String, crate::checkpoints::Checkpoint>,
    pub receipts: Vec<OperatorReceipt>,
}

pub(crate) fn digest(value: &impl Serialize) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"history-operator/1",
        &[&serde_json::to_vec(value)?],
    ))
}
impl AdminState {
    pub(crate) fn empty(incarnation: Incarnation) -> Result<Self> {
        let policy = HistoryPolicyV1::default();
        Ok(Self {
            version: HISTORY_ADMIN_VERSION,
            incarnation,
            sequence: 0,
            policy: HistoryPolicyReceipt {
                digest: digest(&policy)?,
                policy,
                sequence: 0,
                explicit: false,
            },
            checkpoints: BTreeMap::new(),
            receipts: vec![],
        })
    }
    pub(crate) fn validate(&self) -> Result<()> {
        self.policy.policy.validate()?;
        ensure!(
            self.version == HISTORY_ADMIN_VERSION
                && self.policy.digest == digest(&self.policy.policy)?
                && self.policy.sequence <= self.sequence
                && self.checkpoints.len() <= crate::checkpoints::MAX_CHECKPOINTS
                && self.receipts.len() <= MAX_OPERATOR_RECEIPTS,
            "invalid history administrative state"
        );
        for (label, checkpoint) in &self.checkpoints {
            checkpoint.validate()?;
            ensure!(
                label == &checkpoint.label
                    && checkpoint.publication.token.incarnation == self.incarnation
                    && checkpoint.sequence <= self.sequence,
                "foreign checkpoint administrative binding"
            );
        }
        let mut previous = 0;
        for receipt in &self.receipts {
            ensure!(
                receipt.version == 1
                    && receipt.sequence > previous
                    && receipt.sequence <= self.sequence
                    && receipt.head.incarnation == self.incarnation,
                "invalid operator receipt ordering or scope"
            );
            previous = receipt.sequence;
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_HISTORY_ADMIN_BYTES,
            "history administrative state exceeds bound"
        );
        Ok(())
    }
    pub(crate) fn record(
        &mut self,
        head: RevisionToken,
        action: OperatorAction,
        operation: &impl Serialize,
    ) -> Result<OperatorReceipt> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("operator sequence exhausted"))?;
        let receipt = OperatorReceipt {
            version: 1,
            sequence: self.sequence,
            head,
            action,
            operation_digest: digest(operation)?,
        };
        self.receipts.push(receipt.clone());
        if self.receipts.len() > MAX_OPERATOR_RECEIPTS {
            self.receipts.remove(0);
        }
        Ok(receipt)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryRepresentation {
    Full,
    Historical,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryProtection {
    Active,
    Predecessor,
    Reader,
    Checkpoint,
    Conversion,
    ClockUncertain,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryEntry {
    pub revision: RevisionToken,
    pub representation: HistoryRepresentation,
    pub observed_unix_seconds: u64,
    pub metadata_bytes: u64,
    pub protected: Vec<HistoryProtection>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HistorySelectionRequest {
    pub as_of_unix_seconds: Option<u64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct HistoryPlan {
    pub version: u32,
    pub head: RevisionToken,
    pub operator_sequence: u64,
    pub policy_digest: Sha256Digest,
    pub as_of_unix_seconds: u64,
    pub retained: Vec<HistoryEntry>,
    pub convert: Vec<RevisionToken>,
    pub remove: Vec<RevisionToken>,
    pub projected_history_bytes: Option<u64>,
    pub byte_limit: u64,
    pub over_target_protected: bool,
    pub complete: bool,
    pub digest: Sha256Digest,
}

pub(crate) struct Candidate {
    pub descriptor: crate::history::HistoryDescriptor,
    pub entry: HistoryEntry,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn metadata_bytes(descriptor: &crate::history::HistoryDescriptor) -> Result<u64> {
    descriptor.metadata.values().try_fold(
        descriptor.canonical_bytes()?.len() as u64,
        |sum, item| {
            sum.checked_add(item.bytes)
                .ok_or_else(|| anyhow::anyhow!("history metadata accounting overflow"))
        },
    )
}

fn projected_bytes(
    candidates: &[Candidate],
    selected: &std::collections::BTreeSet<Sha256Digest>,
) -> Result<u64> {
    use crate::history::DependencyKind;
    let mut searchable = std::collections::BTreeSet::new();
    for candidate in candidates {
        if candidate.entry.representation == HistoryRepresentation::Full
            && candidate.entry.protected.iter().any(|reason| {
                matches!(
                    reason,
                    HistoryProtection::Active
                        | HistoryProtection::Predecessor
                        | HistoryProtection::Reader
                )
            })
        {
            for item in &candidate.descriptor.dependencies {
                searchable.insert((item.kind, item.digest));
            }
        }
    }
    let mut dependencies = BTreeMap::<(DependencyKind, Sha256Digest), u64>::new();
    let mut bytes = 0u64;
    for candidate in candidates {
        if !selected.contains(&candidate.entry.revision.revision) {
            continue;
        }
        bytes = bytes
            .checked_add(candidate.entry.metadata_bytes)
            .ok_or_else(|| anyhow::anyhow!("history projection overflow"))?;
        for item in &candidate.descriptor.dependencies {
            let key = (item.kind, item.digest);
            if !searchable.contains(&key)
                && let Some(previous) = dependencies.insert(key, item.bytes)
            {
                ensure!(
                    previous == item.bytes,
                    "inconsistent projected history dependency"
                );
            }
        }
    }
    for size in dependencies.values() {
        bytes = bytes
            .checked_add(*size)
            .ok_or_else(|| anyhow::anyhow!("history projection overflow"))?;
    }
    Ok(bytes)
}

fn select(
    state: &AdminState,
    head: RevisionToken,
    candidates: &[Candidate],
    byte_limit: u64,
    as_of: u64,
    complete: bool,
) -> Result<HistoryPlan> {
    use std::collections::BTreeSet;
    let eligible = candidates
        .iter()
        .filter(|candidate| {
            !candidate.entry.protected.iter().any(|reason| {
                matches!(
                    reason,
                    HistoryProtection::Active | HistoryProtection::Predecessor
                )
            })
        })
        .collect::<Vec<_>>();
    let mut selected = BTreeSet::new();
    let mut mandatory = BTreeSet::new();
    for candidate in &eligible {
        if !candidate.entry.protected.is_empty() {
            mandatory.insert(candidate.entry.revision.revision);
            selected.insert(candidate.entry.revision.revision);
        }
    }
    if state.policy.policy.enabled {
        for candidate in eligible
            .iter()
            .take(usize::from(state.policy.policy.recent))
        {
            selected.insert(candidate.entry.revision.revision);
        }
        let first = as_of.saturating_sub(u64::from(state.policy.policy.hourly_hours) * 3600);
        let mut hours = BTreeSet::new();
        for candidate in &eligible {
            let observed = candidate.entry.observed_unix_seconds;
            if observed >= first && observed <= as_of && hours.insert(observed / 3600) {
                selected.insert(candidate.entry.revision.revision);
            }
        }
    } else {
        // Disabling selection never implicitly purges already retained history.
        for candidate in &eligible {
            if candidate.entry.representation == HistoryRepresentation::Historical {
                selected.insert(candidate.entry.revision.revision);
            }
        }
    }
    if complete && state.policy.policy.enabled {
        for candidate in eligible.iter().rev() {
            if selected.len() <= usize::from(state.policy.policy.max_packages)
                && projected_bytes(candidates, &selected)? <= byte_limit
            {
                break;
            }
            if !mandatory.contains(&candidate.entry.revision.revision) {
                selected.remove(&candidate.entry.revision.revision);
            }
        }
    }
    let projected = if complete {
        Some(projected_bytes(candidates, &selected)?)
    } else {
        None
    };
    let pinned = candidates
        .iter()
        .filter(|candidate| {
            candidate
                .entry
                .protected
                .contains(&HistoryProtection::Checkpoint)
        })
        .map(|candidate| candidate.descriptor.clone())
        .collect::<Vec<_>>();
    let pinned_over_limit = crate::checkpoints::footprint(&pinned)? > byte_limit
        || pinned.len() > usize::from(state.policy.policy.max_packages);
    let mut plan = HistoryPlan {
        version: 1,
        head,
        operator_sequence: state.sequence,
        policy_digest: state.policy.digest,
        as_of_unix_seconds: as_of,
        retained: vec![],
        convert: vec![],
        remove: vec![],
        projected_history_bytes: projected,
        byte_limit,
        over_target_protected: projected.is_some_and(|bytes| bytes > byte_limit)
            || mandatory.len() > usize::from(state.policy.policy.max_packages)
            || pinned_over_limit,
        complete,
        digest: Sha256Digest::from_bytes([0; 32]),
    };
    for candidate in candidates {
        let searchable = candidate.entry.protected.iter().any(|reason| {
            matches!(
                reason,
                HistoryProtection::Active | HistoryProtection::Predecessor
            )
        });
        if searchable || selected.contains(&candidate.entry.revision.revision) || !complete {
            plan.retained.push(candidate.entry.clone());
            if complete
                && !searchable
                && selected.contains(&candidate.entry.revision.revision)
                && candidate.entry.representation == HistoryRepresentation::Full
                && !candidate
                    .entry
                    .protected
                    .contains(&HistoryProtection::Reader)
            {
                plan.convert.push(candidate.entry.revision);
            }
        } else if complete
            && state.policy.policy.enabled
            && candidate.entry.representation == HistoryRepresentation::Historical
        {
            plan.remove.push(candidate.entry.revision);
        }
    }
    plan.digest = digest(&(
        &plan.head,
        plan.operator_sequence,
        plan.policy_digest,
        plan.as_of_unix_seconds,
        &plan.retained,
        &plan.convert,
        &plan.remove,
        plan.projected_history_bytes,
        plan.byte_limit,
        plan.over_target_protected,
        plan.complete,
    ))?;
    ensure!(
        serde_json::to_vec(&plan)?.len() <= 131072,
        "history preview exceeds response budget"
    );
    Ok(plan)
}

pub(crate) fn preview_work(
    store: &crate::FileStore,
    collection_limit: u64,
    request: &HistorySelectionRequest,
    work: &mut crate::maintenance::WorkBudget,
) -> Result<HistoryPlan> {
    use anyhow::Context;
    let state = crate::sqlite::history_admin(store.directory())?;
    let current = store
        .catalog()?
        .context("history preview requires indexed collection")?;
    let head = current
        .receipt
        .as_ref()
        .context("history preview requires v2 migration")?
        .token;
    let parent = current
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.observation.parent);
    let limit = state.policy.policy.byte_limit(collection_limit);
    let as_of = request.as_of_unix_seconds.unwrap_or_else(now);
    if crate::history::pending_count(store)? > 0 || crate::maintenance::retirement_count(store)? > 0
    {
        return select(&state, head, &[], limit, as_of, false);
    }
    let count = crate::sqlite::history_receipt_count(store.directory())?;
    let mut scan = crate::sqlite::history_selection(store.directory())?
        .filter(|scan| scan.matches(head, &state, count) && as_of >= scan.as_of_unix_seconds)
        .unwrap_or_else(|| initial_scan(head, &state, count, as_of));
    if !scan.complete {
        advance_scan(store, &state, parent, &mut scan, work, 256)?;
    }
    let mut complete = scan.complete;
    let mut observations = scan.candidates.into_values().collect::<Vec<_>>();
    observations
        .sort_by_key(|observation| std::cmp::Reverse(observation.publication.token.generation));
    let mut candidates = vec![];
    for observation in observations {
        if work.expired() {
            complete = false;
            break;
        }
        let receipt = observation.publication;
        let historical =
            crate::sqlite::history_binding(store.directory(), &receipt.token)?.is_some();
        let mut protected = vec![];
        if observation.observed_unix_seconds > as_of {
            protected.push(HistoryProtection::ClockUncertain);
        }
        if receipt.token == head {
            protected.push(HistoryProtection::Active);
        }
        if Some(receipt.token) == parent {
            protected.push(HistoryProtection::Predecessor);
        }
        if state
            .checkpoints
            .values()
            .any(|pin| pin.publication.token == receipt.token)
        {
            protected.push(HistoryProtection::Checkpoint);
        }
        let lease = if protected.iter().any(|reason| {
            matches!(
                reason,
                HistoryProtection::Active | HistoryProtection::Predecessor
            )
        }) {
            None
        } else {
            crate::retention::exclusive(
                &store
                    .directory()
                    .join("generations")
                    .join(receipt.token.revision.to_string()),
            )?
        };
        if lease.is_none()
            && protected.iter().all(|reason| {
                !matches!(
                    reason,
                    HistoryProtection::Active | HistoryProtection::Predecessor
                )
            })
        {
            protected.push(HistoryProtection::Reader);
        }
        let descriptor = match crate::history::describe_revision(store, &receipt.token, work) {
            Ok(value) => value,
            Err(error) if error.is::<crate::maintenance::WorkLimit>() => {
                complete = false;
                break;
            }
            Err(error) => return Err(error),
        };
        let entry = HistoryEntry {
            revision: receipt.token,
            representation: if historical {
                HistoryRepresentation::Historical
            } else {
                HistoryRepresentation::Full
            },
            observed_unix_seconds: descriptor.observed_unix_seconds,
            metadata_bytes: metadata_bytes(&descriptor)?,
            protected,
        };
        candidates.push(Candidate { descriptor, entry });
        drop(lease);
    }
    select(&state, head, &candidates, limit, as_of, complete)
}

impl crate::FileStore {
    pub fn history_policy(&self) -> Result<HistoryPolicyReceipt> {
        let _lifecycle = self.lifecycle(false)?;
        Ok(crate::sqlite::history_admin(self.directory())?.policy)
    }
    pub fn configure_history(
        &self,
        writer: &crate::WriterLock,
        quota: &crate::quota::Quota,
        policy: HistoryPolicyV1,
    ) -> Result<HistoryPolicyReceipt> {
        use anyhow::Context;
        crate::maintenance::check_writer(self, writer, quota)?;
        policy.validate()?;
        ensure!(
            !policy.enabled || crate::maintenance::retirement_count(self)? == 0,
            "finish ordinary retention maintenance before enabling historical selection"
        );
        let current = self
            .catalog()?
            .context("history policy requires indexed collection")?;
        let head = current
            .receipt
            .as_ref()
            .context("history policy requires v2 migration")?
            .token;
        let expected = crate::sqlite::history_admin(self.directory())?;
        let mut next = expected.clone();
        let mutation = next.record(head, OperatorAction::ConfigureHistory, &policy)?;
        next.policy = HistoryPolicyReceipt {
            digest: digest(&policy)?,
            policy,
            sequence: mutation.sequence,
            explicit: true,
        };
        let reservation = quota.reserve_available(
            &current.snapshot.collection,
            MaintenanceBudget::default().max_metadata_bytes,
        )?;
        crate::sqlite::commit_history_admin(
            self.directory(),
            &expected,
            &next,
            &head,
            &reservation,
        )?;
        Ok(next.policy)
    }
    pub fn history_preview(
        &self,
        request: &HistorySelectionRequest,
        budget: HistoryWorkBudget,
    ) -> Result<HistoryPlan> {
        let _lifecycle = self.lifecycle(false)?;
        let mut work = crate::maintenance::WorkBudget::history(budget)?;
        preview_work(self, effective_collection_limit(self)?, request, &mut work)
    }
}

pub(crate) fn effective_collection_limit(store: &crate::FileStore) -> Result<u64> {
    use anyhow::Context;
    use std::io::Read;
    let root = store
        .directory()
        .parent()
        .and_then(std::path::Path::parent)
        .context("collection data root missing")?;
    let collection = store
        .directory()
        .file_name()
        .and_then(|name| name.to_str())
        .context("invalid collection name")?;
    let path = root.join(".quota/policy.json");
    let policy = match path.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            crate::quota::QuotaPolicy::default()
        }
        Err(error) => return Err(error.into()),
        Ok(metadata) => {
            let parent = path.parent().unwrap().symlink_metadata()?;
            ensure!(
                parent.is_dir()
                    && !parent.file_type().is_symlink()
                    && metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() <= 65536,
                "invalid quota policy for historical accounting"
            );
            let mut bytes = vec![];
            std::fs::File::open(&path)?
                .take(65537)
                .read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() <= 65536,
                "quota policy exceeds historical accounting bound"
            );
            serde_json::from_slice::<crate::quota::QuotaPolicy>(&bytes)?
        }
    };
    Ok(policy
        .collections
        .get(collection)
        .copied()
        .unwrap_or(policy.default_collection_bytes))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Observation {
    pub publication: airk_core::identity::PublicationReceipt,
    pub observed_unix_seconds: u64,
    pub historical: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectionScan {
    pub version: u32,
    pub head: RevisionToken,
    pub operator_sequence: u64,
    pub policy_digest: Sha256Digest,
    pub receipt_count: u64,
    pub as_of_unix_seconds: u64,
    pub before_generation: Option<u64>,
    pub complete: bool,
    pub recent: Vec<Sha256Digest>,
    pub hourly: BTreeMap<u64, Sha256Digest>,
    pub candidates: BTreeMap<Sha256Digest, Observation>,
}
impl SelectionScan {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1
                && self.recent.len() <= 3
                && self.hourly.len() <= 25
                && self.candidates.len() <= 512
                && self
                    .before_generation
                    .is_none_or(|generation| generation <= self.head.generation)
                && serde_json::to_vec(self)?.len() <= 262144,
            "invalid history selection scan bounds"
        );
        for (key, observation) in &self.candidates {
            observation.publication.token.validate_against(&self.head)?;
            ensure!(
                *key == observation.publication.token.revision,
                "selection observation identity mismatch"
            );
        }
        for key in self.recent.iter().chain(self.hourly.values()) {
            ensure!(
                self.candidates.contains_key(key),
                "selection winner missing"
            );
        }
        Ok(())
    }
    fn matches(&self, head: RevisionToken, state: &AdminState, count: u64) -> bool {
        self.head == head
            && self.operator_sequence == state.sequence
            && self.policy_digest == state.policy.digest
            && self.receipt_count == count
    }
}

fn scan_add(
    scan: &mut SelectionScan,
    observation: Observation,
    state: &AdminState,
    parent: Option<RevisionToken>,
) -> Result<()> {
    let token = observation.publication.token;
    let searchable = token == scan.head || Some(token) == parent;
    let pin = state
        .checkpoints
        .values()
        .any(|checkpoint| checkpoint.publication.token == token);
    let mut keep = searchable || pin || observation.historical;
    if !searchable && state.policy.policy.enabled {
        if scan.recent.len() < usize::from(state.policy.policy.recent) {
            scan.recent.push(token.revision);
            keep = true;
        }
        let first = scan
            .as_of_unix_seconds
            .saturating_sub(u64::from(state.policy.policy.hourly_hours) * 3600);
        let observed = observation.observed_unix_seconds;
        if observed >= first
            && observed <= scan.as_of_unix_seconds
            && !scan.hourly.contains_key(&(observed / 3600))
        {
            scan.hourly.insert(observed / 3600, token.revision);
            keep = true;
        }
        // Future timestamps cannot establish that content is old enough to expire.
        if observed > scan.as_of_unix_seconds {
            keep = true;
        }
    }
    if keep {
        scan.candidates.insert(token.revision, observation);
    }
    ensure!(
        scan.candidates.len() <= 512,
        "protected historical observation count exceeds bound"
    );
    scan.before_generation = Some(token.generation);
    Ok(())
}

fn initial_scan(head: RevisionToken, state: &AdminState, count: u64, as_of: u64) -> SelectionScan {
    SelectionScan {
        version: 1,
        head,
        operator_sequence: state.sequence,
        policy_digest: state.policy.digest,
        receipt_count: count,
        as_of_unix_seconds: as_of,
        before_generation: None,
        complete: false,
        recent: vec![],
        hourly: BTreeMap::new(),
        candidates: BTreeMap::new(),
    }
}

fn advance_scan(
    store: &crate::FileStore,
    state: &AdminState,
    parent: Option<RevisionToken>,
    scan: &mut SelectionScan,
    work: &mut crate::maintenance::WorkBudget,
    maximum: usize,
) -> Result<()> {
    let page =
        crate::sqlite::history_scan_receipts(store.directory(), scan.before_generation, maximum)?;
    let exhausted = page.len() < maximum;
    for receipt in page {
        if work.expired() {
            return Ok(());
        }
        let observation = match crate::history::observation(store, &receipt, work) {
            Ok(value) => value,
            Err(error) if error.is::<crate::maintenance::WorkLimit>() => return Ok(()),
            Err(error) => return Err(error),
        };
        scan_add(scan, observation, state, parent)?;
    }
    scan.complete = exhausted || scan.before_generation == Some(0);
    scan.validate()?;
    Ok(())
}

pub(crate) fn advance_selection(
    store: &crate::FileStore,
    reservation: &crate::quota::Reservation,
    work: &mut crate::maintenance::WorkBudget,
    maximum: usize,
) -> Result<SelectionScan> {
    use anyhow::Context;
    let state = crate::sqlite::history_admin(store.directory())?;
    let current = store
        .catalog()?
        .context("historical selection requires indexed collection")?;
    let head = current
        .receipt
        .as_ref()
        .context("historical selection requires v2 publication")?
        .token;
    let parent = current
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.observation.parent);
    let count = crate::sqlite::history_receipt_count(store.directory())?;
    let mut scan = crate::sqlite::history_selection(store.directory())?
        .filter(|scan| scan.matches(head, &state, count))
        .unwrap_or_else(|| initial_scan(head, &state, count, now()));
    if !scan.complete {
        advance_scan(store, &state, parent, &mut scan, work, maximum)?;
        crate::sqlite::commit_history_selection(store.directory(), &scan, reservation)?;
    }
    Ok(scan)
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoryMaintenanceRun {
    pub head: RevisionToken,
    pub conversions: Vec<crate::history::HistoryConversion>,
    pub removals: Vec<crate::history::HistoryRemoval>,
    pub pending: bool,
    pub error: Option<String>,
}

pub(crate) fn maintain_work(
    store: &crate::FileStore,
    collection_limit: u64,
    reservation: &crate::quota::Reservation,
    work: &mut crate::maintenance::WorkBudget,
    maximum: usize,
) -> Result<HistoryMaintenanceRun> {
    use anyhow::Context;
    if !crate::sqlite::history_admin(store.directory())?
        .policy
        .policy
        .enabled
        && crate::history::pending_count(store)? == 0
    {
        // No conversion or recovery work needs an inventory. Loading the full
        // catalog here used most of a cleanup pass merely to discover that
        // history is disabled. Object GC still authenticates every live root.
        let head = crate::sqlite::persistent_revision(store.directory())?
            .and_then(|header| header.token)
            .context("history requires v2 publication")?;
        crate::sqlite::receipt(store.directory(), &head)?
            .context("history publication receipt missing")?;
        return Ok(HistoryMaintenanceRun {
            head,
            conversions: vec![],
            removals: vec![],
            pending: false,
            error: None,
        });
    }
    let current = store
        .catalog()?
        .context("history requires indexed collection")?;
    let head = current
        .receipt
        .as_ref()
        .context("history requires v2 publication")?
        .token;
    let parent = current
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.observation.parent);
    let mut result = HistoryMaintenanceRun {
        head,
        conversions: vec![],
        removals: vec![],
        pending: false,
        error: None,
    };
    let attempt = (|| -> Result<()> {
        let recovered =
            crate::history::recover_conversions(store, head, parent, reservation, work, maximum)?;
        result.conversions = recovered.conversions;
        result.removals = recovered.removals;
        result.pending = recovered.pending;
        if result.pending || work.expired() {
            return Ok(());
        }
        let state = crate::sqlite::history_admin(store.directory())?;
        if !state.policy.policy.enabled {
            return Ok(());
        }
        if crate::maintenance::retirement_count(store)? > 0 {
            result.pending = true;
            return Ok(());
        }
        let scan = advance_selection(store, reservation, work, maximum.min(256))?;
        if !scan.complete {
            result.pending = true;
            return Ok(());
        }
        let plan = preview_work(
            store,
            collection_limit,
            &HistorySelectionRequest::default(),
            work,
        )?;
        if !plan.complete {
            result.pending = true;
            return Ok(());
        }
        result.pending |= plan.retained.iter().any(|entry| {
            entry.representation == HistoryRepresentation::Full
                && entry.protected.contains(&HistoryProtection::Reader)
                && !entry.protected.contains(&HistoryProtection::Active)
                && !entry.protected.contains(&HistoryProtection::Predecessor)
        });
        for token in plan.convert {
            if work.expired() || result.conversions.len() + result.removals.len() >= maximum {
                result.pending = true;
                break;
            }
            let outcome =
                crate::history::convert_selected(store, token, head, parent, reservation, work)?;
            result.pending |= outcome.status != crate::history::ConversionStatus::Complete;
            result.conversions.push(outcome);
        }
        for token in plan.remove {
            if work.expired() || result.conversions.len() + result.removals.len() >= maximum {
                result.pending = true;
                break;
            }
            let outcome =
                crate::history::remove_selected(store, token, head, parent, reservation, work)?;
            result.pending |= outcome.status != crate::history::ConversionStatus::Complete;
            result.removals.push(outcome);
        }
        Ok(())
    })();
    if let Err(error) = attempt {
        result.pending = true;
        result.error = Some(format!("{error:#}").chars().take(2048).collect());
    }
    Ok(result)
}

impl crate::FileStore {
    pub fn maintain_history(
        &self,
        writer: &crate::WriterLock,
        quota: &crate::quota::Quota,
        budget: HistoryWorkBudget,
    ) -> Result<HistoryMaintenanceRun> {
        use anyhow::Context;
        crate::maintenance::check_writer(self, writer, quota)?;
        let mut work = crate::maintenance::WorkBudget::history(budget)?;
        let current = self
            .catalog()?
            .context("history requires indexed collection")?;
        let reservation = quota.reserve_available(
            &current.snapshot.collection,
            budget.maintenance.max_metadata_bytes,
        )?;
        maintain_work(
            self,
            effective_collection_limit(self)?,
            &reservation,
            &mut work,
            budget.maintenance.max_candidates,
        )
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct RetainedReceiptPage {
    pub head: RevisionToken,
    pub receipts: Vec<airk_core::identity::PublicationReceipt>,
    pub next_before_generation: Option<u64>,
    pub complete: bool,
}
impl crate::FileStore {
    pub fn retained_receipts(
        &self,
        before_generation: Option<u64>,
        limit: usize,
    ) -> Result<RetainedReceiptPage> {
        let _lifecycle = self.lifecycle(false)?;
        crate::sqlite::retained_receipts(self.directory(), before_generation, limit)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeRequest {
    pub revisions: Vec<RevisionToken>,
    #[serde(default)]
    pub checkpoints: Vec<crate::checkpoints::RemoveCheckpoint>,
    #[serde(default)]
    pub clear_lineage: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeApplyRequest {
    pub request: PurgeRequest,
    pub expected_plan_digest: Sha256Digest,
}
#[derive(Clone, Debug, Serialize)]
pub struct PurgeTarget {
    pub revision: RevisionToken,
    pub descriptor_digest: Sha256Digest,
    pub protected: Vec<HistoryProtection>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PurgePlan {
    pub version: u32,
    pub head: RevisionToken,
    pub operator_sequence: u64,
    pub policy_digest: Sha256Digest,
    pub request_digest: Sha256Digest,
    pub targets: Vec<PurgeTarget>,
    pub clear_lineage: bool,
    pub digest: Sha256Digest,
}
#[derive(Clone, Debug, Serialize)]
pub struct PurgeReceipt {
    pub mutation: OperatorReceipt,
    pub removals: Vec<crate::history::HistoryRemoval>,
    pub protected: Vec<RevisionToken>,
    pub pending: bool,
    pub error: Option<String>,
    /// The kernel must publish an explicit lineage-clearing revision separately.
    pub lineage_publication_required: bool,
}

fn purge_plan(
    store: &crate::FileStore,
    request: &PurgeRequest,
    work: &mut crate::maintenance::WorkBudget,
) -> Result<PurgePlan> {
    use anyhow::Context;
    ensure!(
        request.revisions.len() <= 128 && request.checkpoints.len() <= 4,
        "purge request exceeds bounds"
    );
    ensure!(
        request
            .revisions
            .iter()
            .map(ToString::to_string)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == request.revisions.len(),
        "duplicate purge revision"
    );
    let state = crate::sqlite::history_admin(store.directory())?;
    let current = store
        .catalog()?
        .context("purge requires indexed collection")?;
    let head = current
        .receipt
        .as_ref()
        .context("purge requires v2 publication")?
        .token;
    let parent = current
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.observation.parent);
    let mut labels = std::collections::BTreeSet::new();
    for checkpoint in &request.checkpoints {
        ensure!(
            labels.insert(checkpoint.label.clone()),
            "duplicate checkpoint removal"
        );
        let found = state
            .checkpoints
            .get(&checkpoint.label)
            .context("checkpoint label unavailable")?;
        ensure!(
            found.publication.token == checkpoint.expected_revision,
            "checkpoint removal revision mismatch"
        );
    }
    let mut plan = PurgePlan {
        version: 1,
        head,
        operator_sequence: state.sequence,
        policy_digest: state.policy.digest,
        request_digest: digest(request)?,
        targets: vec![],
        clear_lineage: request.clear_lineage,
        digest: Sha256Digest::from_bytes([0; 32]),
    };
    for token in &request.revisions {
        token.validate_against(&head)?;
        let descriptor = crate::history::describe_revision(store, token, work)?;
        let mut protected = vec![];
        if *token == head {
            protected.push(HistoryProtection::Active);
        }
        if Some(*token) == parent {
            protected.push(HistoryProtection::Predecessor);
        }
        if state
            .checkpoints
            .values()
            .any(|pin| pin.publication.token == *token && !labels.contains(&pin.label))
        {
            protected.push(HistoryProtection::Checkpoint);
        }
        let source = store
            .directory()
            .join("generations")
            .join(token.revision.to_string());
        if crate::sqlite::history_binding(store.directory(), token)?.is_none()
            || crate::history::conversion_intent(store, token)?.is_some()
            || source.exists()
        {
            // A bound package is readable before conversion finishes retiring
            // the full representation. Purge must preserve that recovery work.
            // Ordinary full generations remain under searchable retention.
            protected.push(HistoryProtection::Conversion);
        }
        let lease = crate::retention::exclusive(&source)?;
        if lease.is_none() {
            protected.push(HistoryProtection::Reader);
        }
        plan.targets.push(PurgeTarget {
            revision: *token,
            descriptor_digest: descriptor.binding_digest()?.sha256,
            protected,
        });
    }
    plan.digest = digest(&(
        plan.version,
        plan.head,
        plan.operator_sequence,
        plan.policy_digest,
        plan.request_digest,
        &plan.targets,
        plan.clear_lineage,
    ))?;
    Ok(plan)
}

impl crate::FileStore {
    pub fn purge_preview(
        &self,
        request: &PurgeRequest,
        budget: HistoryWorkBudget,
    ) -> Result<PurgePlan> {
        let _lifecycle = self.lifecycle(false)?;
        let mut work = crate::maintenance::WorkBudget::history(budget)?;
        purge_plan(self, request, &mut work)
    }
    pub fn purge_apply(
        &self,
        writer: &crate::WriterLock,
        quota: &crate::quota::Quota,
        request: &PurgeApplyRequest,
        budget: HistoryWorkBudget,
    ) -> Result<PurgeReceipt> {
        use anyhow::Context;
        crate::maintenance::check_writer(self, writer, quota)?;
        let mut work = crate::maintenance::WorkBudget::history(budget)?;
        let plan = purge_plan(self, &request.request, &mut work)?;
        ensure!(
            plan.digest == request.expected_plan_digest,
            "purge plan changed; preview again"
        );
        let expected = crate::sqlite::history_admin(self.directory())?;
        ensure!(
            expected.sequence == plan.operator_sequence,
            "purge operator state changed"
        );
        let current = self
            .catalog()?
            .context("purge requires indexed collection")?;
        let parent = current
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.observation.parent);
        let reservation = quota.reserve_available(
            &current.snapshot.collection,
            budget.maintenance.max_metadata_bytes,
        )?;
        let mut next = expected.clone();
        for checkpoint in &request.request.checkpoints {
            next.checkpoints.remove(&checkpoint.label);
        }
        let mutation = next.record(
            plan.head,
            OperatorAction::Purge,
            &(request.expected_plan_digest, &request.request),
        )?;
        crate::sqlite::commit_history_admin(
            self.directory(),
            &expected,
            &next,
            &plan.head,
            &reservation,
        )?;
        let mut result = PurgeReceipt {
            mutation,
            removals: vec![],
            protected: vec![],
            pending: false,
            error: None,
            lineage_publication_required: request.request.clear_lineage,
        };
        for target in plan.targets {
            if !target.protected.is_empty() {
                result.protected.push(target.revision);
                result.pending = true;
                continue;
            }
            if work.expired() || result.removals.len() >= budget.maintenance.max_candidates {
                result.pending = true;
                break;
            }
            match crate::history::remove_selected(
                self,
                target.revision,
                plan.head,
                parent,
                &reservation,
                &mut work,
            ) {
                Ok(outcome) => {
                    result.pending |= outcome.status != crate::history::ConversionStatus::Complete;
                    result.removals.push(outcome);
                }
                Err(error) => {
                    result.pending = true;
                    result.error = Some(format!("{error:#}").chars().take(2048).collect());
                    break;
                }
            }
        }
        Ok(result)
    }
}
