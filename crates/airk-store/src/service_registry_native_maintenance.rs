// SPDX-License-Identifier: Apache-2.0
//! Bounded typed maintenance plans and exact owned job associations.
use super::{
    NativeJob, NativeServiceRegistry, State, enqueue_record, observations::worker_epoch,
    request_identity,
};
use crate::service::ProjectLease;
use airk_core::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    service::{JobError, JobKind, JobOutcome, JobState, ProjectBinding, RequestKey},
    service_maintenance::{
        CompactionReceipt, ItemPhase, MaintenancePlan, PlanCounts, PlanError, PlanItem,
        PlanOutcome, PlanPage, PlanPhase, RepairOperation, RepairRequest, RepairSelection,
        RepairWork,
    },
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg(test)]
#[path = "service_registry_native_maintenance_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Step {
    Inspect,
    Action,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Link {
    pub id: Sha256Digest,
    pub project: ProjectBinding,
    pub step: Step,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Member {
    item: PlanItem,
    attempts: u8,
    next_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    sequence: u64,
    request: RepairRequest,
    engine: Sha256Digest,
    phase: PlanPhase,
    paused: bool,
    paused_at: u64,
    canceled: bool,
    preview: Option<Sha256Digest>,
    fallback_approved: bool,
    outcome: Option<PlanOutcome>,
    inspection_until: u64,
    created_ms: u64,
    updated_ms: u64,
    members: BTreeMap<String, Member>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MaintenanceState {
    sequence: u64,
    plans: BTreeMap<String, Plan>,
    requests: BTreeMap<String, String>,
    automatic: BTreeMap<String, Automatic>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Automatic {
    observation: airk_core::service::MaintenanceObservation,
    retries: u8,
    next_ms: u64,
}
pub(super) fn block_automatic(state: &mut State, project: ProjectBinding) {
    if let Some(automatic) = state.maintenance.automatic.get_mut(&project.to_string()) {
        automatic.retries = 7;
    }
}
pub(super) fn reset_automatic(state: &mut State, project: ProjectBinding) {
    if let Some(automatic) = state.maintenance.automatic.get_mut(&project.to_string()) {
        automatic.retries = 0;
        automatic.next_ms = 0;
    }
}
fn automatic_due(state: &State, project: ProjectBinding, automatic: &Automatic) -> bool {
    state
        .projects
        .get(&project.to_string())
        .is_some_and(|entry| entry.registered && !entry.paused)
        && (automatic
            .observation
            .pending_incarnations
            .is_some_and(|pending| pending > 0)
            || automatic.observation.failed)
        && automatic.retries < if automatic.observation.failed { 3 } else { 7 }
}
pub(super) fn clock_ms() -> Result<u64> {
    Ok(u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?)
}
fn validate_item(item: &PlanItem) -> Result<()> {
    ensure!(
        serde_json::to_vec(item)?.len() <= 16384
            && item.availability == item.action.and_then(|action| action.availability())
            && item.action != Some(RepairOperation::Auto),
        "invalid maintenance item or availability"
    );
    if let Some(observation) = &item.inspection {
        observation.snapshot.validate()?;
        ensure!(
            observation.snapshot.project == item.project
                && observation.worker.project == item.project
                && observation.worker.fencing_generation > 0
                && observation.worker.fencing_generation <= observation.current_fencing_generation,
            "foreign maintenance inspection"
        );
    }
    if let Some(observation) = &item.progress {
        observation.progress.validate()?;
        ensure!(
            observation.worker.project == item.project
                && observation.worker.fencing_generation > 0
                && observation.worker.fencing_generation <= observation.current_fencing_generation
                && observation.job.project == item.project
                && observation.progress.job == observation.job.id
                && Some(observation.job.id) == item.action_job.or(item.inspection_job)
                && serde_json::to_vec(observation)?.len() <= 4096,
            "foreign or oversized maintenance progress"
        );
    }
    for code in item
        .code
        .iter()
        .chain(item.outcome.iter().filter_map(|outcome| match outcome {
            JobOutcome::Failed { code } => Some(code),
            _ => None,
        }))
    {
        ensure!(
            !code.is_empty()
                && code.len() <= 128
                && code
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
            "invalid maintenance failure code"
        );
    }
    if let Some(receipt) = &item.compaction {
        ensure!(
            receipt.bytes_reclaimed == receipt.bytes_before.saturating_sub(receipt.bytes_after),
            "invalid compaction size receipt"
        );
    }
    Ok(())
}
impl MaintenanceState {
    pub(super) fn validate(&self, state: &State) -> Result<()> {
        ensure!(
            self.automatic.len() <= super::MAX_PROJECTS,
            "automatic maintenance count exceeded"
        );
        for (id, automatic) in &self.automatic {
            automatic.observation.validate()?;
            ensure!(
                state.projects.contains_key(id)
                    && automatic.retries <= 7
                    && serde_json::to_vec(&automatic.observation)?.len() <= 4096,
                "invalid maintenance observation"
            );
        }
        ensure!(
            self.plans.len() <= 4
                && self
                    .plans
                    .values()
                    .map(|plan| plan.members.len())
                    .sum::<usize>()
                    <= 1024
                && self.requests.len() <= 4,
            "maintenance plan capacity exceeded"
        );
        for (id, plan) in &self.plans {
            let _: Sha256Digest = id.parse()?;
            ensure!(
                plan.request == plan.request.canonical()?
                    && !plan.members.is_empty()
                    && plan.sequence <= self.sequence,
                "invalid maintenance plan"
            );
            for (project, member) in &plan.members {
                ensure!(
                    project == &member.item.project.to_string()
                        && state.projects.contains_key(project)
                        && member.attempts <= 3,
                    "invalid maintenance member"
                );
                validate_item(&member.item)?;
            }
        }
        for id in self.requests.values() {
            ensure!(
                self.plans.contains_key(id),
                "maintenance receipt lost its plan"
            );
        }
        for job in state.jobs.values() {
            if let Some(link) = job.plan {
                let plan = self
                    .plans
                    .get(&link.id.to_string())
                    .context("maintenance job lost its plan")?;
                let member = plan
                    .members
                    .get(&link.project.to_string())
                    .context("maintenance job lost its member")?;
                ensure!(
                    link.project == job.record.project
                        && match link.step {
                            Step::Inspect => member.item.inspection_job == Some(job.record.id),
                            Step::Action => member.item.action_job == Some(job.record.id),
                        },
                    "maintenance job association changed"
                );
            }
        }
        Ok(())
    }
}
fn counts(plan: &Plan) -> Result<PlanCounts> {
    let mut counts = PlanCounts::default();
    for member in plan.members.values() {
        counts.selected = counts
            .selected
            .checked_add(1)
            .context("maintenance count overflow")?;
        let field = match member.item.phase {
            ItemPhase::Unobserved | ItemPhase::InspectQueued | ItemPhase::Inspecting => {
                &mut counts.awaiting_inspection
            }
            ItemPhase::Ready => &mut counts.ready,
            ItemPhase::ActionQueued => &mut counts.queued,
            ItemPhase::Running => &mut counts.running,
            ItemPhase::Succeeded => &mut counts.succeeded,
            ItemPhase::Failed => &mut counts.failed,
            ItemPhase::Stale => &mut counts.stale,
            ItemPhase::Skipped => &mut counts.skipped,
            ItemPhase::Canceled => &mut counts.canceled,
        };
        *field += 1;
        if member.item.cleanup_pending {
            counts.cleanup_pending += 1;
        }
    }
    Ok(counts)
}
fn public(plan: &Plan, id: Sha256Digest) -> Result<MaintenancePlan> {
    Ok(MaintenancePlan {
        engine: plan.engine,
        id,
        operation: plan.request.operation,
        all_registered: matches!(plan.request.selection, RepairSelection::All),
        phase: plan.phase,
        paused: plan.paused,
        counts: counts(plan)?,
        preview_digest: plan.preview,
        outcome: plan.outcome,
        created_unix_ms: plan.created_ms,
        updated_unix_ms: plan.updated_ms,
    })
}
fn get(state: &State, id: Sha256Digest) -> Result<&Plan> {
    state
        .maintenance
        .plans
        .get(&id.to_string())
        .context(PlanError::Unknown)
}
pub(super) fn eligible(state: &State, job: &NativeJob) -> bool {
    if !matches!(job.record.kind, JobKind::PlanInspect | JobKind::Repair) {
        return true;
    }
    job.plan
        .and_then(|link| state.maintenance.plans.get(&link.id.to_string()))
        .is_some_and(|plan| {
            !plan.paused
                && !plan.canceled
                && match job.record.kind {
                    JobKind::PlanInspect => plan.phase == PlanPhase::Planning,
                    _ => plan.phase == PlanPhase::Running,
                }
        })
}
pub(super) fn control(state: &State, id: Sha256Digest) -> Result<(bool, bool)> {
    match state.jobs.get(&id.to_string()).and_then(|job| job.plan) {
        Some(link) => {
            let plan = get(state, link.id)?;
            Ok((plan.paused, plan.phase == PlanPhase::Canceling))
        }
        None => Ok((false, false)),
    }
}
fn adjusted(state: &State, item: &PlanItem) -> Result<PlanItem> {
    let mut item = item.clone();
    let epoch = state
        .projects
        .get(&item.project.to_string())
        .context("maintenance project missing")?
        .epoch;
    if let Some(observation) = &mut item.inspection {
        observation.current_fencing_generation = epoch;
    }
    if let Some(observation) = &mut item.progress {
        observation.current_fencing_generation = epoch;
    }
    validate_item(&item)?;
    Ok(item)
}
impl NativeServiceRegistry {
    pub fn record_maintenance(
        &mut self,
        lease: &ProjectLease,
        observation: &airk_core::service::MaintenanceObservation,
    ) -> Result<()> {
        observation.validate()?;
        self.write(|state| {
            worker_epoch(state, &self.root, lease)?;
            let automatic = state
                .maintenance
                .automatic
                .entry(lease.binding.to_string())
                .or_insert(Automatic {
                    observation: observation.clone(),
                    retries: 0,
                    next_ms: 0,
                });
            automatic.observation = observation.clone();
            if observation.pending_incarnations == Some(0) && !observation.failed {
                automatic.retries = 0;
                automatic.next_ms = 0;
            }
            Ok(())
        })
    }
    pub fn maintenance_observation(
        &self,
        project: ProjectBinding,
    ) -> Result<Option<airk_core::service::MaintenanceObservation>> {
        self.read(|state| {
            Ok(state
                .maintenance
                .automatic
                .get(&project.to_string())
                .map(|automatic| automatic.observation.clone()))
        })
    }
    pub fn maintenance_retrying(&self, project: ProjectBinding) -> Result<bool> {
        self.read(|state| {
            Ok(state
                .maintenance
                .automatic
                .get(&project.to_string())
                .is_some_and(|automatic| automatic_due(state, project, automatic)))
        })
    }
    pub fn due_maintenance(&self, now_ms: u64) -> Result<Vec<ProjectBinding>> {
        self.read(|state| {
            let mut due = Vec::new();
            for (id, automatic) in &state.maintenance.automatic {
                let project = id.parse()?;
                if automatic_due(state, project, automatic)
                    && automatic.next_ms <= now_ms
                    && !state.jobs.values().any(|job| {
                        job.record.project == project
                            && matches!(job.record.state, JobState::Pending | JobState::Running)
                    })
                {
                    due.push((automatic.next_ms, project));
                }
            }
            due.sort_unstable();
            Ok(due
                .into_iter()
                .take(32)
                .map(|(_, project)| project)
                .collect())
        })
    }
    pub fn enqueue_due_maintenance(
        &mut self,
        project: ProjectBinding,
        caller: Incarnation,
        nonce: Incarnation,
        now_ms: u64,
    ) -> Result<airk_core::service::JobRecord> {
        let (namespace, epoch) = self.request_epoch()?;
        let key = RequestKey {
            namespace,
            epoch,
            nonce,
        };
        self.prepare_job_request(caller, &key, None)?;
        self.write(|state| {
            if state
                .accepted_requests
                .contains_key(&request_identity(caller, &key))
            {
                return enqueue_record(state, project, caller, &key, JobKind::Maintain);
            }
            let automatic = state
                .maintenance
                .automatic
                .get(&project.to_string())
                .context("maintenance is no longer eligible")?;
            ensure!(
                automatic_due(state, project, automatic)
                    && automatic.next_ms <= now_ms
                    && !state.jobs.values().any(|job| job.record.project == project
                        && matches!(job.record.state, JobState::Pending | JobState::Running)),
                "maintenance is no longer eligible"
            );
            let next = now_ms
                .checked_add((1u64 << (automatic.retries + 1)) * 1000)
                .context("maintenance deadline overflow")?;
            let record = enqueue_record(state, project, caller, &key, JobKind::Maintain)?;
            let automatic = state
                .maintenance
                .automatic
                .get_mut(&project.to_string())
                .unwrap();
            automatic.retries += 1;
            automatic.next_ms = next;
            Ok(record)
        })
    }
    pub fn create_maintenance_plan(
        &mut self,
        caller: Incarnation,
        key: &RequestKey,
        request: &RepairRequest,
        now_ms: u64,
        engine: Sha256Digest,
    ) -> Result<MaintenancePlan> {
        let request = request.canonical()?;
        self.write(|state| {
            let identity = request_identity(caller, key);
            if let Some(id) = state.maintenance.requests.get(&identity) {
                let plan = get(state, id.parse()?)?;
                ensure!(plan.request == request, PlanError::KeyConflict);
                return Ok(Ok(public(plan, id.parse()?)?));
            }
            ensure!(
                key.namespace == state.request_namespace && key.epoch == state.request_epoch,
                PlanError::KeyExpired
            );
            let members = match &request.selection {
                RepairSelection::Selected(projects) => {
                    for project in projects {
                        ensure!(
                            state
                                .projects
                                .get(&project.to_string())
                                .is_some_and(|project| project.registered),
                            "maintenance project is not registered"
                        );
                    }
                    projects.clone()
                }
                RepairSelection::All => state
                    .projects
                    .values()
                    .filter(|project| project.registered)
                    .map(|project| project.registration.binding)
                    .collect(),
            };
            ensure!(
                !members.is_empty() && members.len() <= 1024,
                PlanError::Capacity
            );
            let mut rotated = false;
            while state.maintenance.plans.len() >= 4
                || state
                    .maintenance
                    .plans
                    .values()
                    .map(|plan| plan.members.len())
                    .sum::<usize>()
                    + members.len()
                    > 1024
            {
                let id = state
                    .maintenance
                    .plans
                    .iter()
                    .filter(|(_, plan)| plan.phase == PlanPhase::Finished)
                    .min_by_key(|(_, plan)| plan.sequence)
                    .map(|(id, _)| id.clone())
                    .context(PlanError::Capacity)?;
                state.maintenance.plans.remove(&id);
                state
                    .maintenance
                    .requests
                    .retain(|_, accepted| accepted != &id);
                for job in state.jobs.values_mut() {
                    if job.plan.is_some_and(|link| link.id.to_string() == id) {
                        job.plan = None;
                    }
                }
                rotated = true;
            }
            if rotated {
                state.request_epoch = state
                    .request_epoch
                    .checked_add(1)
                    .context("maintenance request epoch exhausted")?;
                state.bump()?;
                return Ok(Err(PlanError::KeyExpired.into()));
            }
            let id = framed_digest(
                b"airk-maintenance-plan/1.4",
                &[
                    caller.as_bytes(),
                    key.namespace.as_bytes(),
                    &key.epoch.to_le_bytes(),
                    key.nonce.as_bytes(),
                    &serde_json::to_vec(&request)?,
                ],
            );
            state.maintenance.sequence = state
                .maintenance
                .sequence
                .checked_add(1)
                .context("maintenance plan sequence exhausted")?;
            let members = members
                .into_iter()
                .map(|project| {
                    (
                        project.to_string(),
                        Member {
                            item: PlanItem {
                                project,
                                phase: ItemPhase::Unobserved,
                                inspection_job: None,
                                inspection: None,
                                action: None,
                                availability: None,
                                explicit_fallback_required: false,
                                action_job: None,
                                outcome: None,
                                code: None,
                                cleanup_pending: false,
                                result_revision: None,
                                progress: None,
                                compaction: None,
                            },
                            attempts: 0,
                            next_ms: 0,
                        },
                    )
                })
                .collect();
            let plan = Plan {
                sequence: state.maintenance.sequence,
                request,
                engine,
                phase: PlanPhase::Planning,
                paused: false,
                paused_at: 0,
                canceled: false,
                preview: None,
                fallback_approved: false,
                outcome: None,
                inspection_until: now_ms
                    .checked_add(600_000)
                    .context("inspection bound overflow")?,
                created_ms: now_ms,
                updated_ms: now_ms,
                members,
            };
            let record = public(&plan, id)?;
            state.maintenance.plans.insert(id.to_string(), plan);
            state.maintenance.requests.insert(identity, id.to_string());
            state.bump()?;
            Ok(Ok(record))
        })?
    }
    pub fn maintenance_plan(&self, id: Sha256Digest) -> Result<MaintenancePlan> {
        self.read(|state| public(get(state, id)?, id))
    }
    pub fn maintenance_plan_page(
        &self,
        id: Sha256Digest,
        after: Option<ProjectBinding>,
        limit: usize,
    ) -> Result<PlanPage> {
        ensure!((1..=32).contains(&limit), "invalid maintenance page bound");
        self.read(|state| {
            let stored = get(state, id)?;
            let plan = public(stored, id)?;
            let mut used = serde_json::to_vec(&plan)?.len() + 128;
            let mut items = Vec::new();
            let mut more = false;
            for member in stored
                .members
                .values()
                .filter(|member| after.is_none_or(|after| member.item.project > after))
            {
                let item = adjusted(state, &member.item)?;
                let bytes = serde_json::to_vec(&item)?.len();
                if items.len() == limit || used + bytes > 48 * 1024 {
                    more = true;
                    break;
                }
                used += bytes;
                items.push(item);
            }
            let after = if more {
                Some(
                    items
                        .last()
                        .context("maintenance item cannot fit page")?
                        .project,
                )
            } else {
                None
            };
            let page = PlanPage { plan, items, after };
            ensure!(
                serde_json::to_vec(&page)?.len() <= 48 * 1024,
                "maintenance page exceeds byte bound"
            );
            Ok(page)
        })
    }
    pub fn approve_maintenance_plan(
        &mut self,
        id: Sha256Digest,
        preview: Sha256Digest,
        allow_fallback: bool,
        now_ms: u64,
    ) -> Result<MaintenancePlan> {
        self.write(|state| {
            let plan = state
                .maintenance
                .plans
                .get_mut(&id.to_string())
                .context(PlanError::Unknown)?;
            ensure!(plan.preview == Some(preview), PlanError::Stale);
            if matches!(plan.phase, PlanPhase::Running | PlanPhase::Finished) {
                return public(plan, id);
            }
            ensure!(
                plan.phase == PlanPhase::Ready && !plan.paused,
                PlanError::NotReady
            );
            ensure!(
                allow_fallback
                    || !plan
                        .members
                        .values()
                        .any(|member| member.item.phase == ItemPhase::Ready
                            && member.item.explicit_fallback_required),
                PlanError::ConfirmationRequired
            );
            plan.phase = PlanPhase::Running;
            plan.fallback_approved = allow_fallback;
            plan.updated_ms = now_ms;
            public(plan, id)
        })
    }
    pub fn pause_maintenance_plan(
        &mut self,
        id: Sha256Digest,
        paused: bool,
        now_ms: u64,
    ) -> Result<MaintenancePlan> {
        self.write(|state| {
            let plan = state
                .maintenance
                .plans
                .get_mut(&id.to_string())
                .context(PlanError::Unknown)?;
            ensure!(
                !matches!(plan.phase, PlanPhase::Canceling | PlanPhase::Finished),
                PlanError::InvalidTransition
            );
            if plan.paused == paused {
                return public(plan, id);
            }
            if !paused {
                let elapsed = now_ms.saturating_sub(plan.paused_at);
                plan.inspection_until = plan
                    .inspection_until
                    .checked_add(elapsed)
                    .context("paused inspection bound overflow")?;
                for member in plan.members.values_mut().filter(|member| {
                    member.next_ms > 0
                        && matches!(
                            member.item.phase,
                            ItemPhase::Unobserved
                                | ItemPhase::Ready
                                | ItemPhase::InspectQueued
                                | ItemPhase::Inspecting
                        )
                }) {
                    member.next_ms = member
                        .next_ms
                        .checked_add(elapsed)
                        .context("paused deadline overflow")?;
                }
            }
            plan.paused = paused;
            plan.paused_at = if paused { now_ms } else { 0 };
            plan.updated_ms = now_ms;
            public(plan, id)
        })
    }
    pub fn cancel_maintenance_plan(
        &mut self,
        id: Sha256Digest,
        now_ms: u64,
    ) -> Result<MaintenancePlan> {
        self.write(|state| {
            let plan = state
                .maintenance
                .plans
                .get_mut(&id.to_string())
                .context(PlanError::Unknown)?;
            if plan.phase == PlanPhase::Finished {
                return public(plan, id);
            }
            plan.phase = PlanPhase::Canceling;
            plan.canceled = true;
            plan.paused = false;
            plan.updated_ms = now_ms;
            for member in plan.members.values_mut().filter(|member| {
                matches!(member.item.phase, ItemPhase::Unobserved | ItemPhase::Ready)
            }) {
                member.item.phase = ItemPhase::Canceled;
                member.item.code = Some("plan_canceled".into());
            }
            finalize(plan, now_ms)?;
            public(plan, id)
        })
    }
    pub fn maintenance_job_control(&self, id: Sha256Digest) -> Result<(bool, bool)> {
        self.read(|state| control(state, id))
    }
    pub fn repair_work(
        &self,
        lease: &ProjectLease,
        id: Sha256Digest,
        engine: Sha256Digest,
    ) -> Result<RepairWork> {
        self.read(|state| {
            let epoch = worker_epoch(state, &self.root, lease)?;
            let job = state
                .jobs
                .get(&id.to_string())
                .context(JobError::UnknownJob)?;
            ensure!(
                job.record.project == lease.binding
                    && job.record.kind == JobKind::Repair
                    && job.record.state == JobState::Running
                    && job.record.fencing_generation == epoch,
                JobError::StaleWorker
            );
            let link = job.plan.context(PlanError::InvalidTransition)?;
            let plan = get(state, link.id)?;
            ensure!(plan.phase != PlanPhase::Canceling, PlanError::Canceled);
            ensure!(plan.engine == engine, PlanError::Stale);
            ensure!(
                link.step == Step::Action && plan.phase == PlanPhase::Running,
                PlanError::InvalidTransition
            );
            let member = plan
                .members
                .get(&link.project.to_string())
                .context(PlanError::Unknown)?;
            ensure!(
                member.item.phase == ItemPhase::Running && member.item.action_job == Some(id),
                PlanError::InvalidTransition
            );
            ensure!(
                !member.item.explicit_fallback_required || plan.fallback_approved,
                PlanError::ConfirmationRequired
            );
            Ok(RepairWork {
                operation: member.item.action.context("approved action missing")?,
                expected: member
                    .item
                    .inspection
                    .as_ref()
                    .context("approved inspection missing")?
                    .snapshot
                    .clone(),
            })
        })
    }
    pub fn record_compaction_receipt(
        &self,
        lease: &ProjectLease,
        id: Sha256Digest,
        receipt: CompactionReceipt,
    ) -> Result<()> {
        ensure!(
            receipt.bytes_reclaimed == receipt.bytes_before.saturating_sub(receipt.bytes_after),
            "invalid compaction size receipt"
        );
        self.write(|state| {
            let epoch = worker_epoch(state, &self.root, lease)?;
            let job = state
                .jobs
                .get(&id.to_string())
                .context(JobError::UnknownJob)?;
            ensure!(
                job.record.project == lease.binding
                    && job.record.kind == JobKind::Repair
                    && job.record.state == JobState::Running
                    && job.record.fencing_generation == epoch,
                PlanError::Stale
            );
            let link = job.plan.context(PlanError::InvalidTransition)?;
            let plan = state
                .maintenance
                .plans
                .get_mut(&link.id.to_string())
                .context(PlanError::Unknown)?;
            ensure!(
                plan.phase == PlanPhase::Running
                    && plan.request.operation == RepairOperation::Compact
                    && link.step == Step::Action,
                PlanError::InvalidTransition
            );
            let member = plan
                .members
                .get_mut(&link.project.to_string())
                .context(PlanError::Unknown)?;
            ensure!(
                member.item.phase == ItemPhase::Running
                    && member.item.action == Some(RepairOperation::Compact)
                    && member.item.action_job == Some(id),
                PlanError::InvalidTransition
            );
            ensure!(
                member
                    .item
                    .inspection
                    .as_ref()
                    .and_then(|value| value.snapshot.publication)
                    == Some(receipt.token),
                PlanError::Stale
            );
            member.item.compaction = Some(receipt);
            Ok(())
        })
    }
}

fn finalize(plan: &mut Plan, now_ms: u64) -> Result<()> {
    let counts = counts(plan)?;
    if plan.phase == PlanPhase::Planning && counts.awaiting_inspection == 0 {
        let mut parts = vec![serde_json::to_vec(&plan.engine)?];
        for member in plan.members.values() {
            let item = &member.item;
            let guard = item
                .inspection
                .as_ref()
                .map(|observation| observation.snapshot.guard_digest())
                .transpose()?;
            parts.push(serde_json::to_vec(&(
                item.project,
                item.phase,
                item.action,
                item.explicit_fallback_required,
                guard,
                item.inspection
                    .as_ref()
                    .and_then(|observation| observation.snapshot.storage.as_ref()),
                &item.code,
            ))?);
        }
        let references: Vec<_> = parts.iter().map(Vec::as_slice).collect();
        plan.preview = Some(framed_digest(b"airk-maintenance-preview/1.4", &references));
        plan.phase = PlanPhase::Ready;
        plan.updated_ms = now_ms;
    }
    if matches!(plan.phase, PlanPhase::Running | PlanPhase::Canceling)
        && counts.succeeded + counts.failed + counts.stale + counts.skipped + counts.canceled
            == counts.selected
    {
        plan.outcome = Some(if plan.phase == PlanPhase::Canceling {
            PlanOutcome::Canceled
        } else if counts.succeeded == counts.selected && counts.cleanup_pending == 0 {
            PlanOutcome::Succeeded
        } else {
            PlanOutcome::PartialFailure
        });
        plan.phase = PlanPhase::Finished;
        plan.updated_ms = now_ms;
    }
    Ok(())
}

fn propose(
    operation: RepairOperation,
    snapshot: &airk_core::service_inspection::InspectionSnapshot,
) -> (Option<RepairOperation>, bool, Option<&'static str>) {
    use airk_core::service_inspection::{IndexState, SourceState};
    let action = if operation != RepairOperation::Auto {
        operation
    } else if snapshot.recovery_required == Some(true) {
        RepairOperation::Recover
    } else if snapshot.index_state == IndexState::Unavailable {
        RepairOperation::Rebuild
    } else {
        RepairOperation::Reconcile
    };
    if snapshot.settings_digest.is_none()
        && !matches!(action, RepairOperation::Recover | RepairOperation::Compact)
    {
        return (None, false, Some("settings_unavailable"));
    }
    if matches!(
        action,
        RepairOperation::Reconcile | RepairOperation::Rebuild
    ) && snapshot.source_state != SourceState::Available
    {
        return (None, false, Some("source_unavailable"));
    }
    let fallback = action == RepairOperation::Rebuild
        && matches!(
            snapshot.index_state,
            IndexState::Unavailable | IndexState::RecoveryRequired
        );
    (Some(action), fallback, None)
}
pub(super) fn inspection(
    state: &mut State,
    observation: &airk_core::service_inspection::InspectionObservation,
) -> Result<()> {
    let Some(link) = state
        .jobs
        .get(&observation.job.to_string())
        .and_then(|job| job.plan)
    else {
        return Ok(());
    };
    ensure!(
        link.step == Step::Inspect && link.project == observation.snapshot.project,
        "foreign planned inspection"
    );
    let member = state
        .maintenance
        .plans
        .get_mut(&link.id.to_string())
        .context(PlanError::Unknown)?
        .members
        .get_mut(&link.project.to_string())
        .context(PlanError::Unknown)?;
    if matches!(
        member.item.phase,
        ItemPhase::InspectQueued | ItemPhase::Inspecting
    ) {
        member.item.inspection = Some(observation.clone());
    }
    Ok(())
}
pub(super) fn progress(
    state: &mut State,
    observation: &airk_core::service_progress::ProgressObservation,
) -> Result<()> {
    let Some(link) = state
        .jobs
        .get(&observation.job.id.to_string())
        .and_then(|job| job.plan)
    else {
        return Ok(());
    };
    state
        .maintenance
        .plans
        .get_mut(&link.id.to_string())
        .context(PlanError::Unknown)?
        .members
        .get_mut(&link.project.to_string())
        .context(PlanError::Unknown)?
        .item
        .progress = Some(observation.clone());
    Ok(())
}
pub(super) fn claimed(state: &mut State, id: Sha256Digest) -> Result<()> {
    let Some(link) = state.jobs.get(&id.to_string()).and_then(|job| job.plan) else {
        return Ok(());
    };
    state
        .maintenance
        .plans
        .get_mut(&link.id.to_string())
        .context(PlanError::Unknown)?
        .members
        .get_mut(&link.project.to_string())
        .context(PlanError::Unknown)?
        .item
        .phase = match link.step {
        Step::Inspect => ItemPhase::Inspecting,
        Step::Action => ItemPhase::Running,
    };
    Ok(())
}
pub(super) fn completed(
    state: &mut State,
    id: Sha256Digest,
    now_ms: u64,
    maintenance: Option<&airk_core::service::MaintenanceObservation>,
) -> Result<()> {
    let job = state
        .jobs
        .get(&id.to_string())
        .context(JobError::UnknownJob)?;
    let Some(link) = job.plan else {
        return Ok(());
    };
    if matches!(job.record.state, JobState::Pending | JobState::Running) {
        return Ok(());
    }
    let outcome = job
        .record
        .outcome
        .clone()
        .context("terminal maintenance job has no receipt")?;
    let plan = state
        .maintenance
        .plans
        .get_mut(&link.id.to_string())
        .context(PlanError::Unknown)?;
    let member = plan
        .members
        .get_mut(&link.project.to_string())
        .context(PlanError::Unknown)?;
    if member.item.phase.terminal() {
        return Ok(());
    }
    match link.step {
        Step::Inspect => match &outcome {
            JobOutcome::Succeeded { .. } if plan.phase != PlanPhase::Canceling => {
                let observation = member
                    .item
                    .inspection
                    .as_ref()
                    .context("inspection completed without a captured observation")?;
                ensure!(observation.job == id, "planned inspection receipt mismatch");
                let (action, fallback, code) =
                    propose(plan.request.operation, &observation.snapshot);
                member.item.action = action;
                member.item.availability = action.and_then(|action| action.availability());
                member.item.outcome = None;
                member.item.explicit_fallback_required = fallback;
                member.item.code = code.map(str::to_owned);
                member.item.phase = if action.is_some() {
                    ItemPhase::Ready
                } else {
                    ItemPhase::Skipped
                };
                member.attempts = 0;
                member.next_ms = 0;
            }
            JobOutcome::Failed { code } => {
                if member.attempts < 3 && plan.phase != PlanPhase::Canceling {
                    member.item.phase = ItemPhase::Unobserved;
                    member.item.inspection = None;
                    member.next_ms = now_ms
                        .checked_add(1000u64 << member.attempts)
                        .context("inspection retry overflow")?;
                } else {
                    member.item.phase = ItemPhase::Skipped;
                }
                member.item.code = Some(code.clone());
                member.item.outcome = Some(outcome.clone());
            }
            _ => {
                member.item.phase = ItemPhase::Canceled;
                member.item.code = Some("inspection_canceled".into());
            }
        },
        Step::Action => {
            member.item.phase = match &outcome {
                JobOutcome::Succeeded { revision } => {
                    member.item.result_revision = *revision;
                    ItemPhase::Succeeded
                }
                JobOutcome::Failed { code } if code == "maintenance_plan_stale" => ItemPhase::Stale,
                JobOutcome::Failed { code } if code == "plan_source_unavailable" => {
                    ItemPhase::Skipped
                }
                JobOutcome::Failed { code }
                    if matches!(
                        code.as_str(),
                        "storage_pressure"
                            | "storage_quota_root"
                            | "storage_quota_collection"
                            | "storage_quota_reservation"
                            | "physical_storage_headroom"
                            | "filesystem_storage_full"
                            | "redb_storage_full"
                    ) && plan.phase != PlanPhase::Canceling =>
                {
                    if member.attempts < 3 {
                        member.next_ms = now_ms
                            .checked_add(10_000u64 << member.attempts.saturating_sub(1))
                            .context("maintenance pressure backoff overflow")?;
                        ItemPhase::Ready
                    } else {
                        ItemPhase::Failed
                    }
                }
                JobOutcome::Failed { .. } => ItemPhase::Failed,
                JobOutcome::Canceled => ItemPhase::Canceled,
            };
            member.item.code = match &outcome {
                JobOutcome::Failed { code } => Some(code.clone()),
                _ => None,
            };
            member.item.outcome = Some(outcome);
            member.item.cleanup_pending = maintenance
                .map(|sample| sample.failed || sample.pending_incarnations != Some(0))
                .unwrap_or(member.item.action == Some(RepairOperation::Rebuild));
        }
    }
    validate_item(&member.item)?;
    finalize(plan, now_ms)
}
fn enqueue_planned(state: &mut State, link: Link, now_ms: u64) -> Result<()> {
    let plan = get(state, link.id)?;
    let member = plan
        .members
        .get(&link.project.to_string())
        .context(PlanError::Unknown)?;
    ensure!(!plan.paused && !plan.canceled, PlanError::InvalidTransition);
    let kind = match link.step {
        Step::Inspect => {
            ensure!(
                plan.phase == PlanPhase::Planning && member.item.phase == ItemPhase::Unobserved,
                PlanError::InvalidTransition
            );
            JobKind::PlanInspect
        }
        Step::Action => {
            ensure!(
                plan.phase == PlanPhase::Running && member.item.phase == ItemPhase::Ready,
                PlanError::InvalidTransition
            );
            ensure!(
                !member.item.explicit_fallback_required || plan.fallback_approved,
                PlanError::ConfirmationRequired
            );
            JobKind::Repair
        }
    };
    let source = link.step == Step::Action
        && matches!(
            member.item.action,
            Some(RepairOperation::Reconcile | RepairOperation::Rebuild)
        );
    let step = match link.step {
        Step::Inspect => b"inspect".as_slice(),
        Step::Action => b"action".as_slice(),
    };
    let caller = framed_digest(b"maintenance-job-caller/1.4", &[link.id.as_bytes()]);
    let nonce = framed_digest(
        b"maintenance-job-request/1.4",
        &[
            link.id.as_bytes(),
            link.project.as_bytes(),
            step,
            &now_ms.to_le_bytes(),
        ],
    );
    let key = RequestKey {
        namespace: state.request_namespace,
        epoch: state.request_epoch,
        nonce: Incarnation::from_bytes(nonce.as_bytes()[..16].try_into()?),
    };
    let record = enqueue_record(
        state,
        link.project,
        Incarnation::from_bytes(caller.as_bytes()[..16].try_into()?),
        &key,
        kind,
    )?;
    // Retrying one member replaces only that member's exact association. Prior
    // terminal receipts stay readable but no longer control or update the plan.
    let old = state
        .maintenance
        .plans
        .get(&link.id.to_string())
        .unwrap()
        .members
        .get(&link.project.to_string())
        .unwrap();
    let previous = match link.step {
        Step::Inspect => old.item.inspection_job,
        Step::Action => old.item.action_job,
    };
    if let Some(previous) = previous
        && let Some(job) = state.jobs.get_mut(&previous.to_string())
    {
        job.plan = None;
    }
    let job = state.jobs.get_mut(&record.id.to_string()).unwrap();
    job.plan = Some(link);
    if source {
        job.source_revision = Some(
            state
                .watches
                .entry(link.project.to_string())
                .or_default()
                .requested,
        );
    }
    let member = state
        .maintenance
        .plans
        .get_mut(&link.id.to_string())
        .unwrap()
        .members
        .get_mut(&link.project.to_string())
        .unwrap();
    match link.step {
        Step::Inspect => {
            member.item.phase = ItemPhase::InspectQueued;
            member.item.inspection_job = Some(record.id);
            member.item.inspection = None;
        }
        Step::Action => {
            member.item.phase = ItemPhase::ActionQueued;
            member.item.action_job = Some(record.id);
            member.item.outcome = None;
            member.item.compaction = None;
            member.item.code = None;
        }
    }
    member.item.progress = None;
    member.attempts = member
        .attempts
        .checked_add(1)
        .context("maintenance attempts overflow")?;
    member.next_ms = if link.step == Step::Inspect {
        now_ms
            .checked_add(600_000)
            .context("maintenance deadline overflow")?
    } else {
        0
    };
    Ok(())
}
fn advancing(plan: &Plan) -> bool {
    !plan.paused
        && matches!(
            plan.phase,
            PlanPhase::Planning | PlanPhase::Running | PlanPhase::Canceling
        )
}
impl NativeServiceRegistry {
    pub fn advance_maintenance_plans(&mut self, now_ms: u64) -> Result<bool> {
        self.prune_job_receipts(None)?;
        // Observing no active plan has no effects. Plans arriving afterward are
        // handled by the next turn; the write path selects again under its lock.
        if !self.read(|state| Ok(state.maintenance.plans.values().any(advancing)))? {
            return Ok(false);
        }
        self.write(|state| {
            let mut plans: Vec<_> = state
                .maintenance
                .plans
                .iter()
                .filter(|(_, plan)| advancing(plan))
                .map(|(id, plan)| (plan.sequence, id.clone()))
                .collect();
            plans.sort_unstable();
            let runnable = !plans.is_empty();
            let mut budget = 16;
            for (_, id) in plans {
                let id: Sha256Digest = id.parse()?;
                let plan = get(state, id)?;
                let phase = plan.phase;
                let expired = phase == PlanPhase::Planning && now_ms >= plan.inspection_until;
                let members: Vec<_> = plan
                    .members
                    .values()
                    .filter(|member| match (phase, member.item.phase) {
                        (PlanPhase::Planning, ItemPhase::Unobserved) => {
                            expired || member.next_ms <= now_ms
                        }
                        (PlanPhase::Running, ItemPhase::Ready) => member.next_ms <= now_ms,
                        (
                            PlanPhase::Canceling,
                            ItemPhase::InspectQueued
                            | ItemPhase::Inspecting
                            | ItemPhase::ActionQueued
                            | ItemPhase::Running,
                        ) => true,
                        (PlanPhase::Planning, ItemPhase::InspectQueued | ItemPhase::Inspecting) => {
                            expired || member.next_ms <= now_ms
                        }
                        _ => false,
                    })
                    .take(budget.min(4))
                    .map(|member| member.item.clone())
                    .collect();
                for item in members {
                    if budget == 0 {
                        break;
                    }
                    budget -= 1;
                    if expired && item.phase == ItemPhase::Unobserved {
                        let member = state
                            .maintenance
                            .plans
                            .get_mut(&id.to_string())
                            .unwrap()
                            .members
                            .get_mut(&item.project.to_string())
                            .unwrap();
                        member.item.phase = ItemPhase::Skipped;
                        member.item.code = Some("inspection_admission_timeout".into());
                        continue;
                    }
                    let step = match (phase, item.phase) {
                        (PlanPhase::Planning, ItemPhase::Unobserved) => Some(Step::Inspect),
                        (PlanPhase::Running, ItemPhase::Ready) => Some(Step::Action),
                        _ => None,
                    };
                    if let Some(step) = step {
                        if let Err(error) = enqueue_planned(
                            state,
                            Link {
                                id,
                                project: item.project,
                                step,
                            },
                            now_ms,
                        ) && !matches!(
                            error.downcast_ref::<JobError>(),
                            Some(JobError::Capacity | JobError::KeyExpired)
                        ) {
                            return Err(error);
                        }
                    } else if let Some(job_id) = item.action_job.or(item.inspection_job) {
                        let job = state
                            .jobs
                            .get_mut(&job_id.to_string())
                            .context(JobError::UnknownJob)?;
                        if job.record.state == JobState::Pending {
                            job.record.state = JobState::Canceled;
                            job.record.outcome = Some(JobOutcome::Canceled);
                        } else if job.record.state == JobState::Running {
                            job.cancellation_requested = true;
                        }
                        super::native_background::completed(state, job_id, Some(now_ms))?;
                        completed(state, job_id, now_ms, None)?;
                    }
                }
                finalize(
                    state.maintenance.plans.get_mut(&id.to_string()).unwrap(),
                    now_ms,
                )?;
            }
            Ok(runnable)
        })
    }
}
