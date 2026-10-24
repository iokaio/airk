// SPDX-License-Identifier: Apache-2.0
//! Bounded durable repair membership and previews; this module never opens indexes.
use super::ServiceRegistry;
use airk_core::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    service::{ProjectBinding, RequestKey},
    service_maintenance::{
        ItemPhase, MaintenancePlan, PlanCounts, PlanError, PlanItem, PlanOutcome, PlanPage,
        PlanPhase, RepairRequest, RepairSelection,
    },
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

const MAX_PLANS: i64 = 4;
const MAX_MEMBERS: i64 = 1024;
const MAX_ITEM: usize = 16_384;
pub(super) const ELIGIBLE:&str="(j.kind NOT IN('plan_inspect','repair') OR EXISTS(
    SELECT 1 FROM service_plan_jobs l JOIN service_plans p2 ON p2.id=l.plan
    WHERE l.job=j.id AND p2.paused=0 AND p2.canceled=0
    AND ((j.kind='plan_inspect' AND p2.phase='planning') OR (j.kind='repair' AND p2.phase='running'))))";
pub(super) fn clock_ms() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}
pub(super) fn job_control(connection: &Connection, job: Sha256Digest) -> Result<(bool, bool)> {
    let owners = linked_items(connection, job)?;
    ensure!(owners.len() <= 1, "maintenance job has multiple owners");
    match owners.first() {
        Some((id, _, _)) => {
            let current = plan(connection, *id)?;
            Ok((current.paused, current.phase == PlanPhase::Canceling))
        }
        None => Ok((false, false)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlanStep {
    Inspect,
    Action,
}
impl PlanStep {
    fn name(self) -> &'static str {
        match self {
            Self::Inspect => "inspect",
            Self::Action => "action",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct PlannedJob {
    pub plan: Sha256Digest,
    pub project: ProjectBinding,
    pub step: PlanStep,
    pub now_ms: u64,
}

pub(super) fn prepare_job(
    connection: &Connection,
    link: PlannedJob,
    project: ProjectBinding,
    kind: airk_core::service::JobKind,
) -> Result<bool> {
    use airk_core::service::JobKind;
    let header = plan(connection, link.plan)?;
    let member = item(connection, link.plan, project)?;
    ensure!(
        link.project == project && !header.paused,
        PlanError::InvalidTransition
    );
    match link.step {
        PlanStep::Inspect => ensure!(
            header.phase == PlanPhase::Planning
                && member.phase == ItemPhase::Unobserved
                && kind == JobKind::PlanInspect,
            PlanError::InvalidTransition
        ),
        PlanStep::Action => {
            ensure!(
                header.phase == PlanPhase::Running
                    && member.phase == ItemPhase::Ready
                    && kind == JobKind::Repair,
                PlanError::InvalidTransition
            );
            let approved: bool = connection.query_row(
                "SELECT fallback_approved FROM service_plans WHERE id=?1",
                [link.plan.to_string()],
                |row| row.get(0),
            )?;
            ensure!(
                !member.explicit_fallback_required || approved,
                PlanError::ConfirmationRequired
            );
        }
    }
    Ok(link.step == PlanStep::Action
        && matches!(
            member.action,
            Some(
                airk_core::service_maintenance::RepairOperation::Reconcile
                    | airk_core::service_maintenance::RepairOperation::Rebuild
            )
        ))
}
pub(super) fn associate_job(
    connection: &Connection,
    link: PlannedJob,
    job: Sha256Digest,
) -> Result<()> {
    let mut member = item(connection, link.plan, link.project)?;
    match link.step {
        PlanStep::Inspect => {
            member.phase = ItemPhase::InspectQueued;
            member.inspection_job = Some(job);
            member.inspection = None;
        }
        PlanStep::Action => {
            member.phase = ItemPhase::ActionQueued;
            member.action_job = Some(job);
            member.outcome = None;
            member.code = None;
        }
    }
    member.progress = None;
    save_item(connection, link.plan, &member)?;
    connection.execute(
        "INSERT INTO service_plan_jobs(plan,project,step,job) VALUES(?1,?2,?3,?4)
        ON CONFLICT(plan,project,step) DO UPDATE SET job=excluded.job",
        params![
            link.plan.to_string(),
            link.project.to_string(),
            link.step.name(),
            job.to_string()
        ],
    )?;
    let deadline = if link.step == PlanStep::Inspect {
        link.now_ms
            .checked_add(600_000)
            .context("maintenance deadline overflow")?
    } else {
        0
    };
    connection.execute(
        "UPDATE service_plan_items SET attempts=attempts+1,next_ms=?1 WHERE plan=?2 AND project=?3",
        params![
            i64::try_from(deadline)?,
            link.plan.to_string(),
            link.project.to_string()
        ],
    )?;
    Ok(())
}
pub(super) const SCHEMA:&str="
CREATE TABLE IF NOT EXISTS service_plans(
    seq INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT UNIQUE NOT NULL CHECK(length(id)=64),
    request BLOB NOT NULL CHECK(length(request)<=20000),engine TEXT NOT NULL CHECK(length(engine)=64),phase TEXT NOT NULL,
    paused INTEGER NOT NULL CHECK(paused IN(0,1)),paused_at INTEGER NOT NULL CHECK(paused_at>=0),canceled INTEGER NOT NULL CHECK(canceled IN(0,1)),
    preview TEXT,fallback_approved INTEGER NOT NULL CHECK(fallback_approved IN(0,1)),outcome TEXT,
    inspection_until INTEGER NOT NULL CHECK(inspection_until>=0),created_ms INTEGER NOT NULL CHECK(created_ms>=0),updated_ms INTEGER NOT NULL CHECK(updated_ms>=0));
CREATE TABLE IF NOT EXISTS service_plan_requests(
    namespace TEXT NOT NULL,epoch INTEGER NOT NULL,nonce TEXT NOT NULL,caller TEXT NOT NULL,
    plan TEXT NOT NULL REFERENCES service_plans(id) ON DELETE CASCADE,
    PRIMARY KEY(namespace,epoch,nonce,caller)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS service_plan_items(
    plan TEXT NOT NULL REFERENCES service_plans(id) ON DELETE CASCADE,
    project TEXT NOT NULL REFERENCES projects(binding),phase TEXT NOT NULL,payload BLOB NOT NULL CHECK(length(payload)<=16384),
    attempts INTEGER NOT NULL CHECK(attempts BETWEEN 0 AND 3),next_ms INTEGER NOT NULL CHECK(next_ms>=0),
    PRIMARY KEY(plan,project)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS service_plan_items_stage ON service_plan_items(plan,phase,project);
CREATE TABLE IF NOT EXISTS service_plan_jobs(
    plan TEXT NOT NULL,project TEXT NOT NULL,step TEXT NOT NULL CHECK(step IN('inspect','action')),
    job TEXT NOT NULL REFERENCES service_jobs(id) ON DELETE CASCADE,
    FOREIGN KEY(plan,project) REFERENCES service_plan_items(plan,project) ON DELETE CASCADE,
    PRIMARY KEY(plan,project,step)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS service_plan_jobs_job ON service_plan_jobs(job);";

fn linked_items(
    connection: &Connection,
    job: Sha256Digest,
) -> Result<Vec<(Sha256Digest, ProjectBinding, PlanStep)>> {
    let mut statement = connection
        .prepare("SELECT plan,project,step FROM service_plan_jobs WHERE job=?1 LIMIT 5")?;
    let raw = statement
        .query_map([job.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        raw.len() <= MAX_PLANS as usize,
        "too many maintenance job owners"
    );
    raw.into_iter()
        .map(|(plan, project, step)| {
            Ok((
                plan.parse()?,
                project.parse()?,
                match step.as_str() {
                    "inspect" => PlanStep::Inspect,
                    "action" => PlanStep::Action,
                    _ => anyhow::bail!("invalid maintenance job step"),
                },
            ))
        })
        .collect()
}
pub(super) fn record_inspection(
    connection: &Connection,
    observation: &airk_core::service_inspection::InspectionObservation,
) -> Result<()> {
    for (id, project, step) in linked_items(connection, observation.job)? {
        ensure!(
            step == PlanStep::Inspect && project == observation.snapshot.project,
            "foreign planned inspection"
        );
        let mut member = item(connection, id, project)?;
        if matches!(
            member.phase,
            ItemPhase::InspectQueued | ItemPhase::Inspecting
        ) {
            member.inspection = Some(observation.clone());
            save_item(connection, id, &member)?;
        }
    }
    Ok(())
}
pub(super) fn job_claimed(connection: &Connection, job: Sha256Digest) -> Result<()> {
    for (id, project, step) in linked_items(connection, job)? {
        let mut member = item(connection, id, project)?;
        member.phase = match step {
            PlanStep::Inspect => ItemPhase::Inspecting,
            PlanStep::Action => ItemPhase::Running,
        };
        save_item(connection, id, &member)?;
    }
    Ok(())
}
fn propose(
    operation: airk_core::service_maintenance::RepairOperation,
    snapshot: &airk_core::service_inspection::InspectionSnapshot,
) -> (
    Option<airk_core::service_maintenance::RepairOperation>,
    bool,
    Option<&'static str>,
) {
    use airk_core::{
        service_inspection::{IndexState, SourceState},
        service_maintenance::RepairOperation as Action,
    };
    let action = if operation != Action::Auto {
        operation
    } else if snapshot.recovery_required == Some(true) {
        Action::Recover
    } else if snapshot.index_state == IndexState::Unavailable {
        Action::Rebuild
    } else {
        Action::Reconcile
    };
    if snapshot.settings_digest.is_none() && action != Action::Recover {
        return (None, false, Some("settings_unavailable"));
    }
    if matches!(action, Action::Reconcile | Action::Rebuild)
        && snapshot.source_state != SourceState::Available
    {
        return (None, false, Some("source_unavailable"));
    }
    let fallback = action == Action::Rebuild
        && matches!(
            snapshot.index_state,
            IndexState::Unavailable | IndexState::RecoveryRequired
        );
    (Some(action), fallback, None)
}
pub(super) fn job_completed(
    connection: &Connection,
    job: Sha256Digest,
    now_ms: u64,
    maintenance: Option<&airk_core::service::MaintenanceObservation>,
) -> Result<()> {
    use airk_core::service::{JobOutcome, JobState};
    let record = super::jobs::record(connection, job)?;
    if matches!(record.state, JobState::Pending | JobState::Running) {
        return Ok(());
    }
    for (id, project, step) in linked_items(connection, job)? {
        let mut member = item(connection, id, project)?;
        if member.phase.terminal() {
            continue;
        }
        let header = plan(connection, id)?;
        let outcome = record
            .outcome
            .clone()
            .context("terminal maintenance job has no receipt")?;
        match step {
            PlanStep::Inspect => match &outcome {
                JobOutcome::Succeeded { .. } if header.phase != PlanPhase::Canceling => {
                    let observation = member
                        .inspection
                        .as_ref()
                        .context("inspection completed without a captured observation")?;
                    ensure!(
                        observation.job == job,
                        "planned inspection receipt mismatch"
                    );
                    let (action, fallback, code) = propose(header.operation, &observation.snapshot);
                    member.action = action;
                    member.availability = action.and_then(|action| action.availability());
                    member.outcome = None;
                    member.explicit_fallback_required = fallback;
                    member.code = code.map(str::to_owned);
                    member.phase = if action.is_some() {
                        ItemPhase::Ready
                    } else {
                        ItemPhase::Skipped
                    };
                    connection.execute("UPDATE service_plan_items SET attempts=0,next_ms=0 WHERE plan=?1 AND project=?2",
                        params![id.to_string(),project.to_string()])?;
                }
                JobOutcome::Failed { code } => {
                    let attempts: i64 = connection.query_row(
                        "SELECT attempts FROM service_plan_items WHERE plan=?1 AND project=?2",
                        params![id.to_string(), project.to_string()],
                        |row| row.get(0),
                    )?;
                    if attempts < 3 && header.phase != PlanPhase::Canceling {
                        member.phase = ItemPhase::Unobserved;
                        member.inspection = None;
                        let next = now_ms
                            .checked_add(1000_u64 << u32::try_from(attempts)?)
                            .context("inspection retry overflow")?;
                        connection.execute(
                            "UPDATE service_plan_items SET next_ms=?1 WHERE plan=?2 AND project=?3",
                            params![i64::try_from(next)?, id.to_string(), project.to_string()],
                        )?;
                    } else {
                        member.phase = ItemPhase::Skipped;
                    }
                    member.code = Some(code.clone());
                    member.outcome = Some(outcome.clone());
                }
                _ => {
                    member.phase = ItemPhase::Canceled;
                    member.code = Some("inspection_canceled".into());
                }
            },
            PlanStep::Action => {
                member.phase = match &outcome {
                    JobOutcome::Succeeded { revision } => {
                        member.result_revision = *revision;
                        ItemPhase::Succeeded
                    }
                    JobOutcome::Failed { code } if code == "maintenance_plan_stale" => {
                        ItemPhase::Stale
                    }
                    JobOutcome::Failed { code } if code == "plan_source_unavailable" => {
                        ItemPhase::Skipped
                    }
                    JobOutcome::Failed { code }
                        if code == "storage_pressure" && header.phase != PlanPhase::Canceling =>
                    {
                        let attempts: i64 = connection.query_row(
                            "SELECT attempts FROM service_plan_items WHERE plan=?1 AND project=?2",
                            params![id.to_string(), project.to_string()],
                            |row| row.get(0),
                        )?;
                        if attempts < 3 {
                            let next = now_ms
                                .checked_add(10_000_u64 << u32::try_from(attempts - 1)?)
                                .context("maintenance pressure backoff overflow")?;
                            connection.execute("UPDATE service_plan_items SET next_ms=?1 WHERE plan=?2 AND project=?3",
                                params![i64::try_from(next)?,id.to_string(),project.to_string()])?;
                            ItemPhase::Ready
                        } else {
                            ItemPhase::Failed
                        }
                    }
                    JobOutcome::Failed { .. } => ItemPhase::Failed,
                    JobOutcome::Canceled => ItemPhase::Canceled,
                };
                member.code = match &outcome {
                    JobOutcome::Failed { code } => Some(code.clone()),
                    _ => None,
                };
                member.outcome = Some(outcome);
                // A cached earlier sample cannot prove cleanup for this action.
                // An interrupted rebuild receipt proves publication, not retirement.
                member.cleanup_pending = maintenance
                    .map(|sample| sample.failed || sample.pending_incarnations != Some(0))
                    .unwrap_or(
                        member.action
                            == Some(airk_core::service_maintenance::RepairOperation::Rebuild),
                    );
            }
        }
        save_item(connection, id, &member)?;
        finalize_preview(connection, id, now_ms)?;
        finalize_execution(connection, id, now_ms)?;
    }
    Ok(())
}
pub(super) fn record_progress(
    connection: &Connection,
    observation: &airk_core::service_progress::ProgressObservation,
) -> Result<()> {
    for (id, project, _) in linked_items(connection, observation.job.id)? {
        let mut member = item(connection, id, project)?;
        member.progress = Some(observation.clone());
        save_item(connection, id, &member)?;
    }
    Ok(())
}
pub(super) fn settle_project_jobs(
    connection: &Connection,
    project: ProjectBinding,
    now_ms: u64,
) -> Result<()> {
    let mut statement = connection.prepare(
        "SELECT DISTINCT l.job FROM service_plan_jobs l JOIN service_jobs j ON j.id=l.job
        WHERE l.project=?1 AND j.state IN('succeeded','failed','canceled') LIMIT 9",
    )?;
    let jobs = statement
        .query_map([project.to_string()], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(jobs.len() <= 8, "too many project maintenance jobs");
    for job in jobs {
        job_completed(connection, job.parse()?, now_ms, None)?;
    }
    Ok(())
}

fn text<T: serde::Serialize>(value: T) -> Result<String> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .context("maintenance enum must be a string")
}
fn decode<T: serde::de::DeserializeOwned>(value: String) -> Result<T> {
    Ok(serde_json::from_value(value.into())?)
}
pub(super) fn item(
    connection: &Connection,
    plan: Sha256Digest,
    project: ProjectBinding,
) -> Result<PlanItem> {
    let (phase, bytes): (String, Vec<u8>) = connection.query_row(
        "SELECT phase,substr(payload,1,16385) FROM service_plan_items WHERE plan=?1 AND project=?2",
        params![plan.to_string(), project.to_string()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    ensure!(bytes.len() <= MAX_ITEM, "oversized maintenance item");
    let mut item: PlanItem = serde_json::from_slice(&bytes)?;
    ensure!(
        item.project == project && item.phase == decode::<ItemPhase>(phase)?,
        "foreign maintenance item"
    );
    let epoch: i64 = connection.query_row(
        "SELECT epoch FROM projects WHERE binding=?1",
        [project.to_string()],
        |row| row.get(0),
    )?;
    if let Some(observation) = &mut item.inspection {
        observation.current_fencing_generation = u64::try_from(epoch)?;
    }
    if let Some(observation) = &mut item.progress {
        observation.current_fencing_generation = u64::try_from(epoch)?;
    }
    validate_item(&item)?;
    Ok(item)
}
fn validate_item(item: &PlanItem) -> Result<()> {
    ensure!(
        item.availability == item.action.and_then(|action| action.availability()),
        "maintenance action availability mismatch"
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
    ensure!(
        item.code.as_ref().is_none_or(|code| !code.is_empty()
            && code.len() <= 128
            && code
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')),
        "invalid maintenance outcome code"
    );
    ensure!(
        item.action
            .is_none_or(|action| action != airk_core::service_maintenance::RepairOperation::Auto),
        "unresolved maintenance action"
    );
    if let Some(progress) = &item.progress {
        progress.progress.validate()?;
        ensure!(
            progress.worker.project == item.project
                && progress.worker.fencing_generation > 0
                && progress.worker.fencing_generation <= progress.current_fencing_generation
                && progress.job.project == item.project
                && progress.progress.job == progress.job.id
                && Some(progress.job.id) == item.action_job.or(item.inspection_job)
                && serde_json::to_vec(progress)?.len() <= 4096,
            "foreign or oversized maintenance progress"
        );
    }
    if let Some(airk_core::service::JobOutcome::Failed { code }) = &item.outcome {
        ensure!(
            !code.is_empty()
                && code.len() <= 128
                && code
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "invalid maintenance failure"
        );
    }
    Ok(())
}
pub(super) fn save_item(
    connection: &Connection,
    plan: Sha256Digest,
    item: &PlanItem,
) -> Result<()> {
    validate_item(item)?;
    let bytes = serde_json::to_vec(item)?;
    ensure!(
        bytes.len() <= MAX_ITEM,
        "maintenance item exceeds byte bound"
    );
    ensure!(
        connection.execute(
            "UPDATE service_plan_items SET phase=?1,payload=?2 WHERE plan=?3 AND project=?4",
            params![
                text(item.phase)?,
                bytes,
                plan.to_string(),
                item.project.to_string()
            ]
        )? == 1,
        PlanError::Unknown
    );
    Ok(())
}
pub(super) fn plan(connection: &Connection, id: Sha256Digest) -> Result<MaintenancePlan> {
    let raw = connection
        .query_row(
            "SELECT substr(request,1,20001),phase,paused,preview,outcome,created_ms,updated_ms,engine
        FROM service_plans WHERE id=?1",
            [id.to_string()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_,String>(7)?,
                ))
            },
        )
        .optional()?
        .ok_or(PlanError::Unknown)?;
    ensure!(raw.0.len() <= 20_000, "oversized maintenance request");
    let request: RepairRequest = serde_json::from_slice(&raw.0)?;
    request.canonical()?;
    let mut counts = PlanCounts::default();
    let mut statement = connection
        .prepare("SELECT phase,count(*) FROM service_plan_items WHERE plan=?1 GROUP BY phase")?;
    let mut rows = statement.query([id.to_string()])?;
    while let Some(row) = rows.next()? {
        let phase = decode::<ItemPhase>(row.get(0)?)?;
        let count = u16::try_from(row.get::<_, i64>(1)?)?;
        counts.selected = counts
            .selected
            .checked_add(count)
            .context("maintenance count overflow")?;
        let field = match phase {
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
        *field = field
            .checked_add(count)
            .context("maintenance count overflow")?;
    }
    ensure!(
        counts.selected > 0 && counts.selected <= MAX_MEMBERS as u16,
        "invalid maintenance membership"
    );
    let pending:i64=connection.query_row("SELECT count(*) FROM service_plan_items WHERE plan=?1 AND json_extract(payload,'$.cleanup_pending')=1",
        [id.to_string()],|row|row.get(0))?;
    counts.cleanup_pending = pending.try_into()?;
    Ok(MaintenancePlan {
        engine: raw.7.parse()?,
        id,
        operation: request.operation,
        all_registered: matches!(request.selection, RepairSelection::All),
        phase: decode(raw.1)?,
        paused: raw.2,
        counts,
        preview_digest: raw.3.map(|digest| digest.parse()).transpose()?,
        outcome: raw.4.map(decode).transpose()?,
        created_unix_ms: raw.5.try_into()?,
        updated_unix_ms: raw.6.try_into()?,
    })
}

impl ServiceRegistry {
    pub fn repair_work(
        &self,
        lease: &crate::service::ProjectLease,
        job: Sha256Digest,
        engine: Sha256Digest,
    ) -> Result<airk_core::service_maintenance::RepairWork> {
        use airk_core::service::{JobError, JobKind, JobState};
        let tx = self.connection.unchecked_transaction()?;
        let epoch = super::jobs::worker_epoch(&tx, &self.root, lease)?;
        let running = super::jobs::record(&tx, job)?;
        ensure!(
            running.project == lease.identity().0
                && running.kind == JobKind::Repair
                && running.state == JobState::Running
                && running.fencing_generation == epoch,
            JobError::StaleWorker
        );
        let owners = linked_items(&tx, job)?;
        ensure!(owners.len() == 1, PlanError::InvalidTransition);
        let (id, project, step) = owners[0];
        let current = plan(&tx, id)?;
        ensure!(current.phase != PlanPhase::Canceling, PlanError::Canceled);
        ensure!(current.engine == engine, PlanError::Stale);
        ensure!(
            step == PlanStep::Action && current.phase == PlanPhase::Running,
            PlanError::InvalidTransition
        );
        let member = item(&tx, id, project)?;
        ensure!(
            member.phase == ItemPhase::Running && member.action_job == Some(job),
            PlanError::InvalidTransition
        );
        let approved: bool = tx.query_row(
            "SELECT fallback_approved FROM service_plans WHERE id=?1",
            [id.to_string()],
            |row| row.get(0),
        )?;
        ensure!(
            !member.explicit_fallback_required || approved,
            PlanError::ConfirmationRequired
        );
        Ok(airk_core::service_maintenance::RepairWork {
            operation: member.action.context("approved action missing")?,
            expected: member
                .inspection
                .context("approved inspection missing")?
                .snapshot,
        })
    }

    /// Only privately owned plan jobs inherit these controls. Normal requests and
    /// other plans are never canceled or paused through semantic job joining.
    pub fn maintenance_job_control(&self, job: Sha256Digest) -> Result<(bool, bool)> {
        let tx = self.connection.unchecked_transaction()?;
        job_control(&tx, job)
    }

    /// One bounded metadata turn. Index opening and work remain worker-owned.
    /// Every enqueue and its plan association commit in the same transaction.
    pub fn advance_maintenance_plans(&mut self, now_ms: u64) -> Result<bool> {
        let plans = {
            let mut statement = self.connection.prepare(
                "SELECT id FROM service_plans WHERE phase IN('planning','running','canceling')
                 AND paused=0 ORDER BY seq LIMIT 5",
            )?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        ensure!(plans.len() <= MAX_PLANS as usize, PlanError::Capacity);
        let runnable = !plans.is_empty();
        let mut budget = 16;
        for raw in plans {
            let id = raw.parse()?;
            let current = self.maintenance_plan(id)?;
            let inspection_until: i64 = self.connection.query_row(
                "SELECT inspection_until FROM service_plans WHERE id=?1",
                [id.to_string()],
                |row| row.get(0),
            )?;
            let inspection_expired =
                current.phase == PlanPhase::Planning && now_ms >= u64::try_from(inspection_until)?;
            let members = {
                let mut statement = self.connection.prepare(
                    "SELECT project FROM service_plan_items WHERE plan=?1 AND (
                     (?4='planning' AND phase='unobserved' AND (next_ms<=?2 OR ?3)) OR
                     (?4='running' AND phase='ready' AND next_ms<=?2) OR
                     (?4='canceling' AND phase IN('inspect_queued','inspecting','action_queued','running')) OR
                     (?4='planning' AND phase IN('inspect_queued','inspecting') AND (?3 OR next_ms<=?2)))
                     ORDER BY project LIMIT ?5",
                )?;
                statement
                    .query_map(
                        params![
                            raw,
                            i64::try_from(now_ms)?,
                            inspection_expired,
                            text(current.phase)?,
                            budget.min(4)
                        ],
                        |row| row.get::<_, String>(0),
                    )?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            };
            for project in members {
                if budget == 0 {
                    break;
                }
                let project = project.parse()?;
                let member = item(&self.connection, id, project)?;
                if inspection_expired && member.phase == ItemPhase::Unobserved {
                    budget -= 1;
                    let tx = self
                        .connection
                        .transaction_with_behavior(TransactionBehavior::Immediate)?;
                    let mut member = item(&tx, id, project)?;
                    member.phase = ItemPhase::Skipped;
                    member.code = Some("inspection_admission_timeout".into());
                    save_item(&tx, id, &member)?;
                    tx.commit()?;
                    continue;
                }
                let step = match (current.phase, member.phase) {
                    (PlanPhase::Planning, ItemPhase::Unobserved) => Some(PlanStep::Inspect),
                    (PlanPhase::Running, ItemPhase::Ready) => Some(PlanStep::Action),
                    _ => None,
                };
                if let Some(step) = step {
                    budget -= 1;
                    match self.enqueue_planned(PlannedJob {
                        plan: id,
                        project,
                        step,
                        now_ms,
                    }) {
                        Ok(_) => {}
                        Err(error) if error.is::<airk_core::service::JobError>() => {
                            // A full ledger waits for finite receipt rotation; it
                            // does not create another request or lose membership.
                            if !matches!(
                                error.downcast_ref::<airk_core::service::JobError>(),
                                Some(
                                    airk_core::service::JobError::Capacity
                                        | airk_core::service::JobError::KeyExpired
                                )
                            ) {
                                return Err(error);
                            }
                        }
                        Err(error) => return Err(error),
                    }
                } else if current.phase == PlanPhase::Canceling {
                    if let Some(job) = member.action_job.or(member.inspection_job) {
                        budget -= 1;
                        self.request_cancellation(job)?;
                    }
                } else if matches!(
                    member.phase,
                    ItemPhase::InspectQueued | ItemPhase::Inspecting
                ) {
                    let deadline: i64 = self.connection.query_row(
                        "SELECT next_ms FROM service_plan_items WHERE plan=?1 AND project=?2",
                        params![id.to_string(), project.to_string()],
                        |row| row.get(0),
                    )?;
                    if inspection_expired || (deadline > 0 && u64::try_from(deadline)? <= now_ms) {
                        budget -= 1;
                        self.request_cancellation(
                            member.inspection_job.context("inspection job missing")?,
                        )?;
                    }
                }
            }
            let tx = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            finalize_preview(&tx, id, now_ms)?;
            finalize_execution(&tx, id, now_ms)?;
            tx.commit()?;
        }
        Ok(runnable)
    }

    pub fn approve_maintenance_plan(
        &mut self,
        id: Sha256Digest,
        preview: Sha256Digest,
        allow_fallback: bool,
        now_ms: u64,
    ) -> Result<MaintenancePlan> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = plan(&tx, id)?;
        ensure!(current.preview_digest == Some(preview), PlanError::Stale);
        if matches!(current.phase, PlanPhase::Running | PlanPhase::Finished) {
            return Ok(current);
        }
        ensure!(
            current.phase == PlanPhase::Ready && !current.paused,
            PlanError::NotReady
        );
        let fallback: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_plan_items WHERE plan=?1 AND phase='ready'
            AND json_extract(payload,'$.explicit_fallback_required')=1)",
            [id.to_string()],
            |row| row.get(0),
        )?;
        ensure!(!fallback || allow_fallback, PlanError::ConfirmationRequired);
        tx.execute("UPDATE service_plans SET phase='running',fallback_approved=?1,updated_ms=?2 WHERE id=?3",
            params![allow_fallback,i64::try_from(now_ms)?,id.to_string()])?;
        let approved = plan(&tx, id)?;
        tx.commit()?;
        Ok(approved)
    }
    pub fn pause_maintenance_plan(
        &mut self,
        id: Sha256Digest,
        paused: bool,
        now_ms: u64,
    ) -> Result<MaintenancePlan> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = plan(&tx, id)?;
        ensure!(
            !matches!(current.phase, PlanPhase::Canceling | PlanPhase::Finished),
            PlanError::InvalidTransition
        );
        if current.paused == paused {
            return Ok(current);
        }
        if !paused {
            let since: i64 = tx.query_row(
                "SELECT paused_at FROM service_plans WHERE id=?1",
                [id.to_string()],
                |row| row.get(0),
            )?;
            let elapsed = now_ms.saturating_sub(u64::try_from(since)?);
            // Shift finite probe/pressure waits, never already settled members.
            // Paused time does not exhaust admission or retry backoff.
            let deadlines = {
                let mut statement=tx.prepare("SELECT project,next_ms FROM service_plan_items WHERE plan=?1 AND next_ms>0 AND phase IN('unobserved','ready','inspect_queued','inspecting') LIMIT 1024")?;
                statement
                    .query_map([id.to_string()], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            };
            let until: i64 = tx.query_row(
                "SELECT inspection_until FROM service_plans WHERE id=?1",
                [id.to_string()],
                |row| row.get(0),
            )?;
            let shifted = u64::try_from(until)?
                .checked_add(elapsed)
                .context("paused inspection bound overflow")?;
            tx.execute(
                "UPDATE service_plans SET inspection_until=?1 WHERE id=?2",
                params![i64::try_from(shifted)?, id.to_string()],
            )?;
            for (project, deadline) in deadlines {
                let shifted = u64::try_from(deadline)?
                    .checked_add(elapsed)
                    .context("paused deadline overflow")?;
                tx.execute(
                    "UPDATE service_plan_items SET next_ms=?1 WHERE plan=?2 AND project=?3",
                    params![i64::try_from(shifted)?, id.to_string(), project],
                )?;
            }
        }
        tx.execute(
            "UPDATE service_plans SET paused=?1,paused_at=?2,updated_ms=?3 WHERE id=?4",
            params![
                paused,
                if paused { i64::try_from(now_ms)? } else { 0 },
                i64::try_from(now_ms)?,
                id.to_string()
            ],
        )?;
        let result = plan(&tx, id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn cancel_maintenance_plan(
        &mut self,
        id: Sha256Digest,
        now_ms: u64,
    ) -> Result<MaintenancePlan> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = plan(&tx, id)?;
        if current.phase == PlanPhase::Finished {
            return Ok(current);
        }
        tx.execute("UPDATE service_plans SET phase='canceling',canceled=1,paused=0,updated_ms=?1 WHERE id=?2",
            params![i64::try_from(now_ms)?,id.to_string()])?;
        let projects = {
            let mut statement=tx.prepare("SELECT project FROM service_plan_items WHERE plan=?1 AND phase IN('unobserved','ready') LIMIT 1024")?;
            statement
                .query_map([id.to_string()], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for project in projects {
            let mut member = item(&tx, id, project.parse()?)?;
            member.phase = ItemPhase::Canceled;
            member.code = Some("plan_canceled".into());
            save_item(&tx, id, &member)?;
        }
        let result = plan(&tx, id)?;
        tx.commit()?;
        Ok(result)
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
        let bytes = serde_json::to_vec(&request)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let accepted:Option<String>=tx.query_row("SELECT plan FROM service_plan_requests WHERE namespace=?1 AND epoch=?2 AND nonce=?3 AND caller=?4",
            params![key.namespace.to_string(),i64::try_from(key.epoch).map_err(|_|PlanError::KeyExpired)?,key.nonce.to_string(),caller.to_string()],|row|row.get(0)).optional()?;
        if let Some(id) = accepted {
            let original: Vec<u8> = tx.query_row(
                "SELECT substr(request,1,20001) FROM service_plans WHERE id=?1",
                [&id],
                |row| row.get(0),
            )?;
            ensure!(original == bytes, PlanError::KeyConflict);
            return plan(&tx, id.parse()?);
        }
        let (namespace, epoch): (String, i64) = tx.query_row(
            "SELECT incarnation,request_epoch FROM service_meta WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        ensure!(
            namespace == key.namespace.to_string() && u64::try_from(epoch)? == key.epoch,
            PlanError::KeyExpired
        );
        let members = match &request.selection {
            RepairSelection::Selected(projects) => {
                for project in projects {
                    ensure!(
                        tx.query_row(
                            "SELECT EXISTS(SELECT 1 FROM projects WHERE binding=?1 AND registered=1)",
                            [project.to_string()],
                            |row| row.get::<_, bool>(0)
                        )?,
                        "maintenance project is not registered"
                    );
                }
                projects.clone()
            }
            RepairSelection::All => {
                let mut statement = tx.prepare(
                    "SELECT binding FROM projects WHERE registered=1 ORDER BY binding LIMIT 1025",
                )?;
                let values = statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .map(|row| row?.parse())
                    .collect::<Result<Vec<ProjectBinding>>>()?;
                ensure!(
                    !values.is_empty() && values.len() <= MAX_MEMBERS as usize,
                    PlanError::Capacity
                );
                values
            }
        };
        let mut rotated = false;
        loop {
            let count: i64 =
                tx.query_row("SELECT count(*) FROM service_plans", [], |row| row.get(0))?;
            let total: i64 =
                tx.query_row("SELECT count(*) FROM service_plan_items", [], |row| {
                    row.get(0)
                })?;
            if count < MAX_PLANS && total + members.len() as i64 <= MAX_MEMBERS {
                break;
            }
            ensure!(tx.execute("DELETE FROM service_plans WHERE seq IN(SELECT seq FROM service_plans WHERE phase='finished' ORDER BY seq LIMIT 1)",[])?==1,PlanError::Capacity);
            rotated = true;
        }
        if rotated {
            // A deleted receipt must never make its old key new again. Commit
            // bounded retention and expire unknown old keys before admission.
            let next = epoch
                .checked_add(1)
                .context("maintenance request epoch exhausted")?;
            tx.execute(
                "UPDATE service_meta SET request_epoch=?1 WHERE id=1",
                [next],
            )?;
            tx.commit()?;
            return Err(PlanError::KeyExpired.into());
        }
        let id = framed_digest(
            b"airk-maintenance-plan/1.4",
            &[
                caller.as_bytes(),
                key.namespace.as_bytes(),
                &key.epoch.to_le_bytes(),
                key.nonce.as_bytes(),
                &bytes,
            ],
        );
        tx.execute("INSERT INTO service_plans(id,request,engine,phase,paused,paused_at,canceled,preview,fallback_approved,outcome,inspection_until,created_ms,updated_ms)
            VALUES(?1,?2,?4,'planning',0,0,0,NULL,0,NULL,?5,?3,?3)",params![id.to_string(),bytes,i64::try_from(now_ms)?,engine.to_string(),i64::try_from(now_ms.checked_add(600_000).context("inspection bound overflow")?)?])?;
        for project in members {
            let member = PlanItem {
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
            };
            let payload = serde_json::to_vec(&member)?;
            tx.execute(
                "INSERT INTO service_plan_items VALUES(?1,?2,'unobserved',?3,0,0)",
                params![id.to_string(), project.to_string(), payload],
            )?;
        }
        tx.execute(
            "INSERT INTO service_plan_requests VALUES(?1,?2,?3,?4,?5)",
            params![
                key.namespace.to_string(),
                epoch,
                key.nonce.to_string(),
                caller.to_string(),
                id.to_string()
            ],
        )?;
        let record = plan(&tx, id)?;
        tx.commit()?;
        Ok(record)
    }
    pub fn maintenance_plan(&self, id: Sha256Digest) -> Result<MaintenancePlan> {
        let tx = self.connection.unchecked_transaction()?;
        plan(&tx, id)
    }
    pub fn maintenance_plan_page(
        &self,
        id: Sha256Digest,
        after: Option<ProjectBinding>,
        limit: usize,
    ) -> Result<PlanPage> {
        ensure!((1..=32).contains(&limit), "invalid maintenance page bound");
        let tx = self.connection.unchecked_transaction()?;
        let record = plan(&tx, id)?;
        let mut statement=tx.prepare("SELECT project FROM service_plan_items WHERE plan=?1 AND (?2 IS NULL OR project>?2) ORDER BY project LIMIT ?3")?;
        let mut rows = statement.query(params![
            id.to_string(),
            after.map(|id| id.to_string()),
            (limit + 1) as i64
        ])?;
        let mut items = Vec::new();
        let mut more = false;
        let mut used = serde_json::to_vec(&record)?.len() + 128;
        while let Some(row) = rows.next()? {
            let project: ProjectBinding = row.get::<_, String>(0)?.parse()?;
            let member = item(&tx, id, project)?;
            let bytes = serde_json::to_vec(&member)?.len();
            if items.len() == limit || used + bytes > 48 * 1024 {
                more = true;
                break;
            }
            used += bytes;
            items.push(member);
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
        let page = PlanPage {
            plan: record,
            items,
            after,
        };
        ensure!(
            serde_json::to_vec(&page)?.len() <= 48 * 1024,
            "maintenance page exceeds byte bound"
        );
        Ok(page)
    }
}

/// Complete preview generation only after every requested inspection settled.
/// The digest binds each proposed action and its captured index/settings guard.
pub(super) fn finalize_preview(
    connection: &Connection,
    id: Sha256Digest,
    now_ms: u64,
) -> Result<()> {
    let current = plan(connection, id)?;
    if current.phase != PlanPhase::Planning || current.counts.awaiting_inspection > 0 {
        return Ok(());
    }
    let mut statement = connection.prepare(
        "SELECT project FROM service_plan_items WHERE plan=?1 ORDER BY project LIMIT 1025",
    )?;
    let projects = statement
        .query_map([id.to_string()], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(projects.len() <= MAX_MEMBERS as usize, PlanError::Capacity);
    let mut parts = vec![serde_json::to_vec(&current.engine)?];
    for project in projects {
        let member = item(connection, id, project.parse()?)?;
        let guard = member
            .inspection
            .as_ref()
            .map(|observation| observation.snapshot.guard_digest())
            .transpose()?;
        parts.push(serde_json::to_vec(&(
            member.project,
            member.phase,
            member.action,
            member.explicit_fallback_required,
            guard,
            member
                .inspection
                .as_ref()
                .and_then(|observation| observation.snapshot.storage.as_ref()),
            member.code,
        ))?);
    }
    let references: Vec<_> = parts.iter().map(Vec::as_slice).collect();
    let digest = framed_digest(b"airk-maintenance-preview/1.4", &references);
    connection.execute("UPDATE service_plans SET phase='ready',preview=?1,updated_ms=?2 WHERE id=?3 AND phase='planning'",
        params![digest.to_string(),i64::try_from(now_ms)?,id.to_string()])?;
    Ok(())
}

pub(super) fn finalize_execution(
    connection: &Connection,
    id: Sha256Digest,
    now_ms: u64,
) -> Result<()> {
    let current = plan(connection, id)?;
    if !matches!(current.phase, PlanPhase::Running | PlanPhase::Canceling) {
        return Ok(());
    }
    let done = current.counts.succeeded
        + current.counts.failed
        + current.counts.stale
        + current.counts.skipped
        + current.counts.canceled;
    if done != current.counts.selected {
        return Ok(());
    }
    let outcome = if current.phase == PlanPhase::Canceling {
        PlanOutcome::Canceled
    } else if current.counts.succeeded == current.counts.selected
        && current.counts.cleanup_pending == 0
    {
        PlanOutcome::Succeeded
    } else {
        PlanOutcome::PartialFailure
    };
    connection.execute(
        "UPDATE service_plans SET phase='finished',outcome=?1,updated_ms=?2 WHERE id=?3",
        params![text(outcome)?, i64::try_from(now_ms)?, id.to_string()],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "service_maintenance_tests.rs"]
mod tests;
