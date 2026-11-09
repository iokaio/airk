// SPDX-License-Identifier: Apache-2.0
//! Same-selected native maintenance for explicitly reviewed saved-index adoption.
//!
//! This module creates neither an activation nor an issuer-rotation transaction.
//! The returned capability keeps the installer lock and every native owner lease
//! until the caller finishes its native catalog/Current transaction and restores
//! the service's original running policy. Dropping it leaves its exact durable
//! maintenance fence for explicit recovery; Drop never starts a service.
use super::*;
use airk_core::{
    catalog_legacy_adoption::LegacyAdoptionPlan,
    identity::{Incarnation, Sha256Digest, framed_digest},
    service_checkpoint::{CheckpointState, UpgradeCheckpointObservation},
};
use airk_store::service::registry::{ServiceRegistry, checkpoint as store};

const JOURNAL: &str = "legacy-maintenance.json";
const MAX_JOURNAL: u64 = 512 * 1024;
const MAX_ADOPTION_PLANS: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Admitted,
    Checkpointed,
    Quiescent,
    Restoring,
    RestoreFailed,
    Restored,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Boundary {
    GracefulNativeCheckpoint,
    CommittedStoresUnderNativeExclusion,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    install: PathBuf,
    control: PathBuf,
    operation: Incarnation,
    selected: Selection,
    execution_sha256: Sha256Digest,
    original_running: bool,
    original_explicit_user_stop: bool,
    original_recorded_instance: Option<Incarnation>,
    original_health: Option<ServiceHealth>,
    cycle: u64,
    cycle_health: Option<ServiceHealth>,
    cycle_recorded_instance: Option<Incarnation>,
    checkpoint_operation: Incarnation,
    cycle_state_sha256: Option<Sha256Digest>,
    manager_owned: bool,
    manager_definition_sha256: Sha256Digest,
    phase: Phase,
    checkpoint: Option<UpgradeCheckpointObservation>,
    boundary: Option<Boundary>,
    quiescent_state_sha256: Option<Sha256Digest>,
    #[serde(default)]
    adoption_plans: Vec<LegacyAdoptionPlan>,
    // Earlier unpublished single-group journals retain their exact plan on read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    adoption_plan: Option<LegacyAdoptionPlan>,
    restoration_state_sha256: Option<Sha256Digest>,
    restoration: Option<RestoreObservation>,
    restore_failure: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RestoreObservation {
    pub operation: Incarnation,
    pub originally_running: bool,
    pub originally_explicitly_stopped: bool,
    pub restarted: bool,
    pub service_instance: Option<Incarnation>,
    pub execution_sha256: Sha256Digest,
    pub observed_unix_ms: u64,
}

#[derive(Debug, Serialize)]
struct RestoreNeedsNativeRecovery {
    code: &'static str,
    operation: Incarnation,
    checkpoint_owner: Option<Incarnation>,
    recorded_successor: Option<Incarnation>,
    guidance: &'static str,
}
impl std::fmt::Display for RestoreNeedsNativeRecovery {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.guidance)
    }
}
impl std::error::Error for RestoreNeedsNativeRecovery {}

fn restart_allowed(previously_running: bool, explicit_user_stop: bool) -> bool {
    previously_running && !explicit_user_stop
}
fn restoration_needs_reconcile(
    phase: Phase,
    previously_running: bool,
    explicit_user_stop: bool,
) -> bool {
    matches!(phase, Phase::Restoring | Phase::RestoreFailed)
        && restart_allowed(previously_running, explicit_user_stop)
}
fn unchanged_user_stop(original: bool, current: bool) -> Result<()> {
    ensure!(
        original == current,
        "saved-index maintenance explicit User Stop changed; preserve its current policy"
    );
    Ok(())
}
fn require_pending_operation(
    phase: Phase,
    saved: Incarnation,
    requested: Incarnation,
) -> Result<()> {
    ensure!(
        phase == Phase::Restored || saved == requested,
        "another native maintenance operation is pending"
    );
    Ok(())
}
fn restored_owner_is_fresh(
    prior: Option<Incarnation>,
    observed: Incarnation,
    recorded: Option<Incarnation>,
    preservation_verified: bool,
    admission_closed: bool,
) -> Result<bool> {
    if Some(observed) == prior {
        return Ok(false);
    }
    ensure!(
        preservation_verified && !admission_closed && recorded == Some(observed),
        "live maintenance successor lacks settled-checkpoint, preserved-state and exact native owner proof"
    );
    Ok(true)
}

/// The only mutation authority this helper gives its caller is the held native
/// exclusion. It does not grant source access, invent an upgrade or bypass the
/// catalog/import operation's own exact receipt and publication checks.
#[must_use = "restore the maintenance capability on both success and failure"]
pub(crate) struct Maintenance {
    journal: Journal,
    main: PathBuf,
    approval: Approval,
    exclusion: Option<Quiescence>,
    _installer: File,
}

fn manager_digest(manager: &crate::service_manager::ServiceManager) -> Result<Sha256Digest> {
    Ok(Sha256Digest::from_bytes(
        Sha256::digest(serde_json::to_vec(&manager.plan()?)?).into(),
    ))
}

fn registry(control: &Path) -> Result<ServiceRegistry> {
    ensure!(
        control.join("service-1.4.sqlite3").try_exists()?,
        "saved-index maintenance requires an existing native service registry"
    );
    layout::validate_control(control)?;
    ServiceRegistry::open_saved(control)
}

fn read(root: &Path) -> Result<Option<Journal>> {
    if !root.join(JOURNAL).try_exists()? {
        return Ok(None);
    }
    let mut journal: Journal =
        serde_json::from_slice(&read_bounded(&root.join(JOURNAL), MAX_JOURNAL)?)?;
    ensure!(
        journal.schema == 1
            && journal.install == root.canonicalize()?
            && journal.control == layout::control(root)?
            && journal.original_running == journal.original_health.is_some(),
        "saved-index maintenance journal ownership changed"
    );
    digest(&journal.selected.active)?;
    if let Some(observed) = &journal.original_health {
        ensure!(
            Some(observed.instance) == journal.original_recorded_instance
                && observed.executable_sha256 == journal.execution_sha256.to_string()
                && observed.runtime_sha256.as_deref() == Some(journal.selected.active.as_str()),
            "saved-index maintenance original native owner changed"
        );
    }
    ensure!(
        journal.checkpoint_operation
            == checkpoint_operation(
                journal.operation,
                journal.cycle,
                journal.cycle_recorded_instance
            ),
        "saved-index maintenance native checkpoint cycle identity changed"
    );
    if let Some(observed) = &journal.cycle_health {
        ensure!(
            Some(observed.instance) == journal.cycle_recorded_instance
                && observed.executable_sha256 == journal.execution_sha256.to_string()
                && observed.runtime_sha256.as_deref() == Some(journal.selected.active.as_str()),
            "saved-index maintenance cycle native owner changed"
        );
    }
    if let Some(checkpoint) = &journal.checkpoint {
        checkpoint.validate()?;
        ensure!(
            checkpoint.operation_id == journal.checkpoint_operation
                && Some(checkpoint.service_instance) == journal.cycle_recorded_instance
                && checkpoint.execution_sha256 == journal.execution_sha256
                && checkpoint.candidate_execution_sha256 == journal.execution_sha256
                && checkpoint.state == CheckpointState::Ready,
            "saved-index maintenance checkpoint crossed its original selected owner"
        );
    }
    if let Some(plan) = journal.adoption_plan.take() {
        ensure!(
            journal.adoption_plans.is_empty(),
            "maintenance journal crossed single and batched plan identities"
        );
        journal.adoption_plans.push(plan);
    }
    ensure!(
        journal.adoption_plans.len() <= MAX_ADOPTION_PLANS,
        "maintenance adoption plan count exceeds bound"
    );
    let mut checked = Vec::new();
    for plan in &journal.adoption_plans {
        ensure!(
            Some(plan.quiescence_sha256) == journal.quiescent_state_sha256,
            "maintenance bound plan changed its original native quiescence proof"
        );
        ensure!(
            !plan_already_bound(&checked, plan)?,
            "maintenance journal repeated an adoption group"
        );
        checked.push(plan.clone());
    }
    if let Some(observed) = &journal.restoration {
        ensure!(
            observed.operation == journal.operation
                && observed.execution_sha256 == journal.execution_sha256
                && observed.originally_running == journal.cycle_health.is_some()
                && observed.originally_explicitly_stopped == journal.original_explicit_user_stop
                && observed.restarted == observed.service_instance.is_some()
                && observed.restarted
                    == (observed.originally_running && !observed.originally_explicitly_stopped),
            "saved-index maintenance restoration receipt changed"
        );
    }
    Ok(Some(journal))
}

/// True when an interrupted maintenance cycle for `operation` is pending.
pub(super) fn pending_operation(root: &Path, operation: Incarnation) -> Result<bool> {
    Ok(read(root)?.is_some_and(|journal| journal.operation == operation))
}

pub(super) fn require_manager_change_settled(root: &Path) -> Result<()> {
    let path = root.join(JOURNAL);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
        Ok(_) => safe_file(&path)?,
    }
    {
        let root = root.canonicalize()?;
        let journal = read(&root)?.context("native maintenance journal disappeared")?;
        ensure!(
            journal.phase == Phase::Restored,
            "legacy_maintenance_pending_before_manager_change: resume the exact maintenance operation first"
        );
    }
    Ok(())
}

fn save(journal: &Journal) -> Result<()> {
    ensure!(
        serde_json::to_vec(journal)?.len() as u64 <= MAX_JOURNAL,
        "saved-index maintenance journal exceeds its complete-metadata bound"
    );
    replace_json(&journal.install, JOURNAL, journal)
}

fn baseline_name(operation: Incarnation) -> String {
    format!("legacy-maintenance-state-{operation}.json")
}

fn cycle_baseline_name(journal: &Journal) -> String {
    format!(
        "legacy-maintenance-cycle-{}-{}.json",
        journal.operation, journal.cycle
    )
}

fn restoration_snapshot_name(operation: Incarnation, cycle: u64) -> String {
    format!("legacy-maintenance-restoration-{operation}-{cycle}.json")
}

fn cycle_baseline(journal: &Journal) -> Result<Option<store::CheckpointSnapshot>> {
    let Some(expected) = journal.cycle_state_sha256 else {
        return Ok(None);
    };
    let saved: store::CheckpointSnapshot = serde_json::from_slice(&read_bounded(
        &journal.install.join(cycle_baseline_name(journal)),
        MAX_JOURNAL,
    )?)?;
    ensure!(
        saved.control == journal.control && saved.digest()? == expected,
        "saved-index maintenance cycle baseline changed"
    );
    Ok(Some(saved))
}

fn checkpoint_operation(
    operation: Incarnation,
    cycle: u64,
    instance: Option<Incarnation>,
) -> Incarnation {
    let digest = framed_digest(
        b"airk/saved-index-maintenance-native-checkpoint/1",
        &[
            operation.as_bytes(),
            &cycle.to_le_bytes(),
            instance
                .unwrap_or(Incarnation::from_bytes([0; 16]))
                .as_bytes(),
        ],
    );
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    Incarnation::from_bytes(bytes)
}

fn baseline(journal: &Journal) -> Result<Option<store::CheckpointSnapshot>> {
    let Some(expected) = journal.quiescent_state_sha256 else {
        return Ok(None);
    };
    let saved: store::CheckpointSnapshot = serde_json::from_slice(&read_bounded(
        &journal.install.join(baseline_name(journal.operation)),
        MAX_JOURNAL,
    )?)?;
    ensure!(
        saved.control == journal.control && saved.digest()? == expected,
        "saved-index maintenance immutable baseline changed"
    );
    Ok(Some(saved))
}

fn save_baseline(journal: &Journal, snapshot: &store::CheckpointSnapshot) -> Result<()> {
    save_snapshot(journal, &baseline_name(journal.operation), snapshot)
}

fn save_snapshot(
    journal: &Journal,
    name: &str,
    snapshot: &store::CheckpointSnapshot,
) -> Result<()> {
    let bytes = serde_json::to_vec(snapshot)?;
    ensure!(
        bytes.len() as u64 <= MAX_JOURNAL,
        "saved-index maintenance baseline exceeds bound"
    );
    if journal.install.join(name).try_exists()? {
        ensure!(
            read_bounded(&journal.install.join(name), MAX_JOURNAL)? == bytes,
            "saved-index maintenance immutable baseline already differs"
        );
        return Ok(());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(&journal.install)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(journal.install.join(name))?;
    #[cfg(unix)]
    File::open(&journal.install)?.sync_all()?;
    Ok(())
}

fn exact<T: Serialize>(left: &T, right: &T) -> Result<bool> {
    Ok(serde_json::to_vec(left)? == serde_json::to_vec(right)?)
}

/// Reobserve a retained checkpoint using its original representation. Native
/// predecessors and interrupted journals may still carry schema-one history.
fn current_snapshot(journal: &Journal) -> Result<store::CheckpointSnapshot> {
    let schema = if let Some(saved) = cycle_baseline(journal)? {
        Some(saved.schema)
    } else if let Some(observed) = &journal.checkpoint {
        Some(
            service_runtime::checkpoint::read_packet(
                &journal.control,
                journal.checkpoint_operation,
                observed
                    .native_packet_sha256
                    .context("maintenance checkpoint packet digest missing")?,
            )?
            .state
            .schema,
        )
    } else {
        None
    };
    match schema {
        Some(schema) => store::snapshot_for_schema(&journal.control, schema),
        None => store::snapshot(&journal.control),
    }
}

fn require_job_history_preserved(
    before: &store::CheckpointSnapshot,
    after: &store::CheckpointSnapshot,
) -> Result<()> {
    // Compact snapshots expand only unfinished jobs and their requests. These
    // complete table commitments preserve terminal history and configuration.
    for name in ["service_jobs", "service_requests", "service_configuration"] {
        ensure!(
            before.tables.iter().find(|table| table.name == name)
                == after.tables.iter().find(|table| table.name == name),
            "saved-index maintenance changed native job history: {name}"
        );
    }
    Ok(())
}

fn plan_already_bound(
    plans: &[LegacyAdoptionPlan],
    requested: &LegacyAdoptionPlan,
) -> Result<bool> {
    requested.validate()?;
    let index = airk_core::catalog_setup::setup_index_id(&requested.setup)?;
    for saved in plans {
        saved.validate()?;
        if saved == requested {
            return Ok(true);
        }
        ensure!(
            airk_core::catalog_setup::setup_index_id(&saved.setup)? != index
                && saved.setup.registration.binding != requested.setup.registration.binding
                && saved.original_registration.binding != requested.original_registration.binding,
            "maintenance adoption group changed its native request, original backing or introduced index"
        );
    }
    ensure!(
        plans.len() < MAX_ADOPTION_PLANS,
        "maintenance adoption plan count exceeds bound"
    );
    Ok(false)
}

fn require_plan_context(
    plan: &LegacyAdoptionPlan,
    namespace: Option<Incarnation>,
    revision: airk_core::index_catalog::CatalogRevision,
    quiescence: Sha256Digest,
) -> Result<()> {
    plan.validate()?;
    ensure!(
        Some(plan.setup.key.namespace) == namespace
            && plan.setup.expected_catalog_revision == revision
            && plan.quiescence_sha256 == quiescence,
        "maintenance adoption plan does not match its original native baseline and current held catalog revision"
    );
    Ok(())
}

fn checked_native_receipt(
    plan: &LegacyAdoptionPlan,
    receipt: Option<airk_core::catalog_setup::IndexSetupReceipt>,
) -> Result<Option<airk_core::catalog_setup::IndexSetupReceipt>> {
    plan.validate()?;
    if let Some(receipt) = &receipt {
        receipt.validate()?;
        ensure!(
            receipt.request == plan.setup
                && receipt.legacy_adoption == Some(plan.identity()?)
                && receipt.index_id == airk_core::catalog_setup::setup_index_id(&plan.setup)?,
            "saved-index maintenance adopted rows lack their exact bound native plan receipt"
        );
    }
    Ok(receipt)
}

fn setup_checkpoint_work(
    receipt: &airk_core::catalog_setup::IndexSetupReceipt,
) -> Result<Option<store::UnfinishedWork>> {
    receipt.validate()?;
    if receipt.stage == airk_core::catalog_setup::SetupStage::Ready {
        return Ok(None);
    }
    Ok(Some(store::UnfinishedWork {
        owner: "index_operation_receipts".into(),
        id: receipt.operation_id.to_string(),
        kind: "catalog_index_setup".into(),
        state: receipt.stage.as_str().into(),
        original_engine: None,
        payload_digest: Some(framed_digest(
            b"airk/checkpoint-operation/1",
            &[&serde_json::to_vec(receipt)?],
        )),
    }))
}

fn require_setup_continuations(
    before: &[store::UnfinishedWork],
    after: &[store::UnfinishedWork],
    receipts: &[&airk_core::catalog_setup::IndexSetupReceipt],
) -> Result<()> {
    let mut before = before.to_vec();
    let mut after = after.to_vec();
    for receipt in receipts {
        let history =
            airk_store::service::registry::catalog::setup::legacy_adoption_checkpoint_history(
                receipt,
            )?;
        let history = history
            .iter()
            .map(setup_checkpoint_work)
            .collect::<Result<Vec<_>>>()?;
        let owns = |work: &store::UnfinishedWork| {
            work.owner == "index_operation_receipts" && work.id == receipt.operation_id.to_string()
        };
        let prior = before.iter().filter(|work| owns(work)).collect::<Vec<_>>();
        ensure!(
            prior.len() <= 1
                && prior
                    .iter()
                    .all(|work| history.iter().any(|saved| saved.as_ref() == Some(*work))),
            "saved-index maintenance changed its prior native adoption continuation"
        );
        let current = after.iter().filter(|work| owns(work)).collect::<Vec<_>>();
        let expected = setup_checkpoint_work(receipt)?;
        ensure!(
            match expected.as_ref() {
                Some(expected) => current.len() == 1 && current[0] == expected,
                None => current.is_empty(),
            },
            "saved-index maintenance lost or crossed its native adoption continuation"
        );
        before.retain(|work| !owns(work));
        after.retain(|work| !owns(work));
    }
    ensure!(
        before == after,
        "saved-index maintenance changed unrelated accepted continuations"
    );
    Ok(())
}

/// No caller-supplied table names or ignored fields authorize preservation.
/// Only the exact native journal plan and its actual Store setup receipt admit
/// the new binding/index; every prior publication, job and continuation remains.
fn require_preserved(
    journal: &Journal,
    before: &store::CheckpointSnapshot,
    after: &store::CheckpointSnapshot,
) -> Result<()> {
    ensure!(
        before.complete
            && after.complete
            && before.schema == after.schema
            && before.control == after.control
            && before.namespace == after.namespace,
        "saved-index maintenance checkpoint scope changed"
    );
    require_job_history_preserved(before, after)?;
    ensure!(
        exact(&before.jobs, &after.jobs)?
            && before.requests == after.requests
            && before.retained_intents == after.retained_intents
            && before.retained_intent_contract == after.retained_intent_contract
            && exact(
                &before.retained_intent_disposition,
                &after.retained_intent_disposition
            )?,
        "saved-index maintenance changed accepted jobs, requests or continuations"
    );
    let ledger = registry(&journal.control)?;
    let mut native = Vec::new();
    for plan in &journal.adoption_plans {
        plan.validate()?;
        if let Some(receipt) = checked_native_receipt(
            plan,
            ledger.index_setup_receipt(plan.setup.caller, &plan.setup.key)?,
        )? {
            native.push((plan, receipt));
        }
    }
    require_setup_continuations(
        &before.unfinished,
        &after.unfinished,
        &native
            .iter()
            .map(|(_, receipt)| receipt)
            .collect::<Vec<_>>(),
    )?;
    for binding in &before.project_bindings {
        ensure!(
            after.project_bindings.contains(binding),
            "saved-index maintenance removed an existing native binding"
        );
    }
    for binding in &after.project_bindings {
        ensure!(
            before.project_bindings.contains(binding)
                || native
                    .iter()
                    .any(|(plan, _)| plan.setup.registration.binding == *binding),
            "saved-index maintenance introduced an unplanned native binding"
        );
    }
    for publication in &before.publications {
        let current = after
            .publications
            .iter()
            .find(|item| item.binding == publication.binding)
            .context("saved-index maintenance removed an existing publication")?;
        ensure!(
            exact(publication, current)?
                || native
                    .iter()
                    .any(|(plan, _)| plan.setup.registration.binding == publication.binding),
            "saved-index maintenance changed another index's publication"
        );
    }
    for publication in &after.publications {
        ensure!(
            before
                .publications
                .iter()
                .any(|item| item.binding == publication.binding)
                || native
                    .iter()
                    .any(|(plan, _)| plan.setup.registration.binding == publication.binding),
            "saved-index maintenance introduced an unplanned publication"
        );
    }
    for index in &before.indexes {
        let current = after
            .indexes
            .iter()
            .find(|item| item.id == index.id)
            .context("saved-index maintenance removed an existing index")?;
        ensure!(
            exact(index, current)?
                || native
                    .iter()
                    .any(|(plan, receipt)| receipt.index_id == index.id
                        && current.binding == plan.setup.registration.binding
                        && !current.retired),
            "saved-index maintenance changed an existing index identity or policy"
        );
    }
    for index in &after.indexes {
        ensure!(
            before.indexes.iter().any(|item| item.id == index.id)
                || native
                    .iter()
                    .any(|(plan, receipt)| receipt.index_id == index.id
                        && plan.setup.registration.binding == index.binding),
            "saved-index maintenance introduced an unplanned index"
        );
    }
    if let Some((_, receipt)) = native
        .iter()
        .max_by_key(|(_, receipt)| receipt.catalog_revision.get())
    {
        // Schema 13 is the native adoption barrier, not an invented engine
        // migration. The Store receipt above proves this one planned advance.
        ensure!(
            after.registry_schema == Some(airk_core::index_catalog::REGISTRY_SCHEMA)
                && before
                    .registry_schema
                    .is_some_and(|schema| schema <= airk_core::index_catalog::REGISTRY_SCHEMA),
            "saved-index maintenance registry schema crossed native adoption support"
        );
        let mut expected = before
            .catalog_state
            .clone()
            .context("maintenance baseline catalog missing")?;
        let current = after
            .catalog_state
            .as_ref()
            .context("maintenance resulting catalog missing")?;
        ensure!(
            current.revision == receipt.catalog_revision
                || current.revision == receipt.catalog_revision.next()?
                || current.revision == expected.revision
                || current.revision == expected.revision.next()?,
            "saved-index maintenance catalog revision exceeds its adoption and imported-authority operations"
        );
        expected.schema = current.schema;
        expected.revision = current.revision;
        expected.accepted_new_writes = true;
        ensure!(
            expected == *current,
            "saved-index maintenance changed catalog authority, namespace or policy"
        );
    } else {
        ensure!(
            before.digest()? == after.digest()?,
            "native state changed before a planned adoption receipt existed"
        );
    }
    Ok(())
}

fn require_selection(journal: &Journal, main: &Path) -> Result<()> {
    ensure!(
        selection_record(&journal.install)? == journal.selected
            && runtime_pair::digest_file(main)? == journal.execution_sha256.to_string()
            && runtime_pair::current_executable_sha256()? == journal.execution_sha256.to_string(),
        "saved-index maintenance requires the actual same selected native execution"
    );
    let manager = crate::service_manager::ServiceManager::for_runtime(&journal.install)?;
    ensure!(
        manager.owns_restart()? == journal.manager_owned
            && manager_digest(&manager)? == journal.manager_definition_sha256,
        "saved-index maintenance manager ownership or definition changed"
    );
    unchanged_user_stop(
        journal.original_explicit_user_stop,
        registry(&journal.control)?.service_explicitly_stopped()?,
    )?;
    lifecycle::require_settled(&journal.install)
}

fn require_original_recorded_owner(journal: &Journal) -> Result<()> {
    let current = registry(&journal.control)?.recorded_service_instance()?;
    if current != journal.cycle_recorded_instance
        && matches!(journal.phase, Phase::Restoring | Phase::RestoreFailed)
    {
        return Err(RestoreNeedsNativeRecovery {
            code: "maintenance_restored_successor_health_unavailable", operation: journal.operation,
            checkpoint_owner: journal.cycle_recorded_instance, recorded_successor: current,
            guidance: "The recorded replacement has no authenticated healthy native owner. Retain this exact maintenance journal and selected runtime, review native service diagnostics and explicit same-selected recovery, then retry the same adoption operation. An absent endpoint does not authorize substituting the original checkpoint owner.",
        }.into());
    }
    ensure!(
        current == journal.cycle_recorded_instance,
        "saved-index maintenance recorded native owner changed"
    );
    Ok(())
}

async fn observe_running(
    root: &Path,
    approval: &Approval,
    execution: Sha256Digest,
) -> Result<Option<ServiceHealth>> {
    match health(root, approval, &execution.to_string()).await {
        Ok(observed) => Ok(Some(observed)),
        Err(error) => {
            let proof = quiescent(root).context(error)?;
            drop(proof);
            Ok(None)
        }
    }
}

fn copy_health(observed: &Option<ServiceHealth>) -> Result<Option<ServiceHealth>> {
    observed
        .as_ref()
        .map(|value| -> Result<ServiceHealth> {
            Ok(serde_json::from_slice(&serde_json::to_vec(value)?)?)
        })
        .transpose()
}

impl Maintenance {
    pub(crate) fn root(&self) -> &Path {
        &self.journal.install
    }
    pub(crate) fn control(&self) -> &Path {
        &self.journal.control
    }
    pub(crate) fn quiescence_sha256(&self) -> Result<Sha256Digest> {
        self.journal
            .quiescent_state_sha256
            .context("maintenance original quiescence baseline missing")
    }
    /// Bind the exact reviewed native plan before its first reservation. Syntax
    /// alone grants nothing; restoration also requires its actual Store receipt.
    pub(crate) fn bind_adoption_plan(&mut self, plan: &LegacyAdoptionPlan) -> Result<()> {
        plan.validate()?;
        ensure!(
            self.exclusion.is_some(),
            "native adoption plan requires held maintenance exclusion"
        );
        if plan_already_bound(&self.journal.adoption_plans, plan)? {
            return Ok(());
        }
        let before = baseline(&self.journal)?.context("maintenance baseline is missing")?;
        let index_id = airk_core::catalog_setup::setup_index_id(&plan.setup)?;
        let current = current_snapshot(&self.journal)?;
        let cycle =
            cycle_baseline(&self.journal)?.context("maintenance native cycle baseline missing")?;
        require_preserved(&self.journal, &cycle, &current)?;
        let catalog = current
            .catalog_state
            .as_ref()
            .context("maintenance held native catalog missing")?;
        require_plan_context(
            plan,
            before.namespace,
            catalog.revision,
            self.quiescence_sha256()?,
        )?;
        ensure!(
            !before
                .project_bindings
                .contains(&plan.setup.registration.binding)
                && !before.indexes.iter().any(|index| index.id == index_id),
            "saved-index maintenance adoption must introduce its own new backing"
        );
        self.journal.adoption_plans.push(plan.clone());
        if let Err(error) = save(&self.journal) {
            self.journal.adoption_plans.pop();
            return Err(error);
        }
        Ok(())
    }

    /// All adoption mutations borrow the actual retained owner exclusion. The
    /// writable connection cannot escape this closure or survive restoration.
    pub(crate) fn with_adoption_registry<T>(
        &self,
        plan: &LegacyAdoptionPlan,
        action: impl FnOnce(&mut ServiceRegistry) -> Result<T>,
    ) -> Result<T> {
        require_selection(&self.journal, &self.main)?;
        require_original_recorded_owner(&self.journal)?;
        ensure!(
            self.journal.phase == Phase::Quiescent,
            "adoption maintenance is not quiescent"
        );
        ensure!(
            plan_already_bound(&self.journal.adoption_plans, plan)?,
            "adoption plan is not durably bound"
        );
        let exclusion = self
            .exclusion
            .as_ref()
            .context("adoption owner exclusion missing")?;
        ensure!(
            exclusion._service.root() == self.journal.control,
            "adoption control owner changed"
        );
        let cycle =
            cycle_baseline(&self.journal)?.context("maintenance native cycle baseline missing")?;
        let current = current_snapshot(&self.journal)?;
        require_preserved(&self.journal, &cycle, &current)?;
        let namespace = current
            .namespace
            .context("adoption native namespace missing")?;
        ensure!(
            plan.setup.key.namespace == namespace,
            "adoption namespace changed"
        );
        ServiceRegistry::with_existing_maintenance(&exclusion._service, namespace, action)
    }

    /// The installer lock and all exclusion leases remain owned until this
    /// method settles the exact checkpoint, and remain installer-fenced while
    /// it restores only the original non-UserStopped running service.
    pub(crate) async fn restore(mut self) -> Result<RestoreObservation> {
        match self.restore_inner().await {
            Ok(observed) => Ok(observed),
            Err(error) => {
                self.journal.phase = Phase::RestoreFailed;
                self.journal.restore_failure =
                    Some(format!("{error:#}").chars().take(4096).collect());
                if let Err(saved) = save(&self.journal) {
                    return Err(error.context(format!(
                        "maintenance restoration failed and its failure receipt could not be saved: {saved:#}"
                    )));
                }
                Err(error.context("saved-index maintenance restoration remains pending"))
            }
        }
    }

    /// Preserve the caller's primary failure while retaining any separate
    /// restoration failure in its durable journal and returned error chain.
    pub(crate) async fn finish<T>(self, primary: Result<T>) -> Result<T> {
        let restored = self.restore().await;
        match (primary, restored) {
            (Ok(value), Ok(_)) => Ok(value),
            (Err(error), Ok(_)) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Err(error), Err(restore)) => Err(error.context(format!(
                "the original saved-index operation failed; service restoration also failed: {restore:#}"
            ))),
        }
    }

    async fn restore_inner(&mut self) -> Result<RestoreObservation> {
        require_selection(&self.journal, &self.main)?;
        require_original_recorded_owner(&self.journal)?;
        if self.exclusion.is_none() {
            self.exclusion = Some(quiescent(&self.journal.install)?);
        }
        let manager = crate::service_manager::ServiceManager::for_runtime(&self.journal.install)?;
        manager.require_stopped_for_maintenance()?;
        if let Some(before) = cycle_baseline(&self.journal)? {
            let after = current_snapshot(&self.journal)?;
            require_preserved(&self.journal, &before, &after)?;
            save_snapshot(
                &self.journal,
                &restoration_snapshot_name(self.journal.operation, self.journal.cycle),
                &after,
            )?;
            self.journal.restoration_state_sha256 = Some(after.digest()?);
        }
        self.journal.phase = Phase::Restoring;
        self.journal.restore_failure = None;
        save(&self.journal)?;
        // Settling cached live admission is forbidden. This happens only after
        // all actual service/project/model/source owners have been excluded.
        service_runtime::checkpoint::settle(
            &self.journal.control,
            self.journal.checkpoint_operation,
        )?;
        drop(self.exclusion.take());
        let restart = restart_allowed(
            self.journal.cycle_health.is_some(),
            self.journal.original_explicit_user_stop,
        );
        let service_instance = if restart {
            lifecycle::require_start_allowed(&self.journal.install)?;
            let observed = activation::start_selected(
                &self.journal.install,
                &self.approval,
                &self.main,
                Some(self.journal.manager_owned),
            )
            .await?;
            ensure!(
                Some(observed.instance) != self.journal.cycle_recorded_instance
                    && observed.executable_sha256 == self.journal.execution_sha256.to_string(),
                "saved-index maintenance did not restore a fresh exact selected native owner"
            );
            Some(observed.instance)
        } else {
            None
        };
        let observed = RestoreObservation {
            operation: self.journal.operation,
            originally_running: self.journal.cycle_health.is_some(),
            originally_explicitly_stopped: self.journal.original_explicit_user_stop,
            restarted: restart,
            service_instance,
            execution_sha256: self.journal.execution_sha256,
            observed_unix_ms: unix_ms()?,
        };
        self.journal.phase = Phase::Restored;
        self.journal.restoration = Some(observed.clone());
        save(&self.journal)?;
        Ok(observed)
    }

    fn capture_exclusion(&mut self, exclusion: Quiescence) -> Result<()> {
        require_selection(&self.journal, &self.main)?;
        require_original_recorded_owner(&self.journal)?;
        let manager = crate::service_manager::ServiceManager::for_runtime(&self.journal.install)?;
        manager.require_stopped_for_maintenance()?;
        let snapshot = current_snapshot(&self.journal)?;
        let engine = runtime_pair::engine_identity_at_version(&self.main, &self.approval.version)?;
        snapshot.require_compatible(Some(engine), engine)?;
        let state = snapshot.digest()?;
        let existing = cycle_baseline(&self.journal)?;
        if let Some(before) = existing.as_ref() {
            require_preserved(&self.journal, before, &snapshot)?;
        } else if let Some(observed) = &self.journal.checkpoint {
            let packet = service_runtime::checkpoint::read_packet(
                &self.journal.control,
                self.journal.checkpoint_operation,
                observed
                    .native_packet_sha256
                    .context("maintenance checkpoint packet digest missing")?,
            )?;
            ensure!(
                packet.service_instance == observed.service_instance
                    && packet.execution_sha256 == self.journal.execution_sha256
                    && packet.candidate_execution_sha256 == self.journal.execution_sha256
                    && packet.state.digest()? == state
                    && observed.state_sha256 == Some(state),
                "saved-index maintenance changed accepted jobs, continuations or canonical state before native exclusion"
            );
            self.journal.boundary = Some(Boundary::GracefulNativeCheckpoint);
        } else {
            self.journal.boundary = Some(Boundary::CommittedStoresUnderNativeExclusion);
        }
        service_runtime::checkpoint::arm_recovery(
            &self.journal.control,
            self.journal.checkpoint_operation,
            self.journal.execution_sha256,
        )?;
        if self.journal.quiescent_state_sha256.is_none() {
            save_baseline(&self.journal, &snapshot)?;
            self.journal.quiescent_state_sha256 = Some(state);
        }
        if existing.is_none() {
            save_snapshot(
                &self.journal,
                &cycle_baseline_name(&self.journal),
                &snapshot,
            )?;
            self.journal.cycle_state_sha256 = Some(state);
        }
        self.journal.phase = Phase::Quiescent;
        save(&self.journal)?;
        self.exclusion = Some(exclusion);
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        require_selection(&self.journal, &self.main)?;
        require_original_recorded_owner(&self.journal)?;
        if self.journal.checkpoint.is_none()
            && let Some(original) = &self.journal.cycle_health
        {
            match activation::prepare_native_checkpoint(
                &self.journal.control,
                self.journal.checkpoint_operation,
                &self.approval.version,
                original,
                self.journal.execution_sha256,
            )
            .await
            {
                Ok(observed) => {
                    self.journal.checkpoint = Some(observed);
                    self.journal.phase = Phase::Checkpointed;
                    save(&self.journal)?;
                }
                Err(error) => {
                    // Endpoint failure is not exit proof. Only every actual
                    // native owner lease can qualify this interrupted boundary.
                    let exclusion = quiescent(&self.journal.install).context(error)?;
                    let manager =
                        crate::service_manager::ServiceManager::for_runtime(&self.journal.install)?;
                    if self.journal.manager_owned {
                        manager.stop()?;
                    }
                    return self.capture_exclusion(exclusion);
                }
            }
        }
        let manager = crate::service_manager::ServiceManager::for_runtime(&self.journal.install)?;
        if self.journal.manager_owned {
            manager.stop()?;
        }
        if self.journal.cycle_health.is_some() {
            // Uses the existing exact-image native drain only after its modern
            // checkpoint; no bare Shutdown is used as transaction safety proof.
            drain(&self.journal.install, &self.main, &self.approval).await?;
        }
        self.capture_exclusion(quiescent(&self.journal.install)?)
    }
}

/// Enter before catalog reservation; callers must not hold an installer lock.
/// The 32-hex operation is the caller's durable adoption operation identity.
async fn reconcile_live_restoration(
    journal: &mut Journal,
    approval: &Approval,
) -> Result<Option<RestoreObservation>> {
    if !restoration_needs_reconcile(
        journal.phase,
        journal.cycle_health.is_some(),
        journal.original_explicit_user_stop,
    ) {
        return Ok(None);
    }
    let Ok(observed) = health(
        &journal.install,
        approval,
        &journal.execution_sha256.to_string(),
    )
    .await
    else {
        return Ok(None);
    };
    // A still-running old checkpoint owner must resume its own native drain;
    // it is not a restored replacement and no fence is cleared around it.
    if !restored_owner_is_fresh(
        journal.cycle_recorded_instance,
        observed.instance,
        registry(&journal.control)?.recorded_service_instance()?,
        journal.restoration_state_sha256.is_some()
            && baseline(journal)?.is_some()
            && cycle_baseline(journal)?.is_some(),
        service_runtime::checkpoint::admission_closed(&journal.control)?,
    )? {
        return Ok(None);
    }
    let restored = RestoreObservation {
        operation: journal.operation,
        originally_running: true,
        originally_explicitly_stopped: false,
        restarted: true,
        service_instance: Some(observed.instance),
        execution_sha256: journal.execution_sha256,
        observed_unix_ms: unix_ms()?,
    };
    journal.phase = Phase::Restored;
    journal.restoration = Some(restored.clone());
    journal.restore_failure = None;
    save(journal)?;
    Ok(Some(restored))
}

pub(crate) async fn enter(root: &Path, operation: &str) -> Result<Maintenance> {
    let operation: Incarnation = operation.parse()?;
    let root = root.canonicalize()?;
    let installer = install_lock(&root)?;
    managed::require_unmanaged(&root)?;
    ensure!(
        activation::pending(&root)?.is_none(),
        "pending activation owns native maintenance"
    );
    lifecycle::require_settled(&root)?;
    launcher_repair::require_settled(&root)?;
    payload_repair::require_settled(&root)?;
    require_issuer_rotation_settled(&root)?;
    ensure!(
        !root.join("uninstall.json").try_exists()?,
        "runtime uninstall owns native maintenance"
    );
    let control = layout::control(&root)?;
    store::preflight_capacity(&control)?;
    let selected = selection_record(&root)?;
    let (main, approval) = verify_runtime(&root, &selected.active)?;
    let execution_sha256: Sha256Digest = runtime_pair::digest_file(&main)?.parse()?;
    ensure!(
        runtime_pair::current_executable_sha256()? == execution_sha256.to_string(),
        "saved-index maintenance CLI is not the actual selected native image"
    );
    let mut prior = read(&root)?;
    if let Some(saved) = prior.as_mut() {
        require_pending_operation(saved.phase, saved.operation, operation)?;
        if saved.operation == operation {
            require_selection(saved, &main)?;
            reconcile_live_restoration(saved, &approval).await?;
        }
    }
    let journal = match prior {
        Some(saved) if saved.phase != Phase::Restored => {
            ensure!(
                saved.operation == operation,
                "another saved-index maintenance operation is pending"
            );
            ensure!(
                saved.selected == selected && saved.execution_sha256 == execution_sha256,
                "pending saved-index maintenance selected execution changed"
            );
            saved
        }
        Some(mut saved) if saved.operation == operation => {
            require_selection(&saved, &main)?;
            baseline(&saved)?.context("restored maintenance original baseline missing")?;
            let cycle_health = observe_running(&root, &approval, execution_sha256).await?;
            let cycle_recorded_instance = registry(&control)?.recorded_service_instance()?;
            if let Some(observed) = &cycle_health {
                ensure!(
                    Some(observed.instance) == cycle_recorded_instance,
                    "maintenance reentry native owner changed"
                );
            }
            saved.cycle = saved
                .cycle
                .checked_add(1)
                .context("maintenance cycle overflow")?;
            saved.cycle_health = cycle_health;
            saved.cycle_recorded_instance = cycle_recorded_instance;
            saved.checkpoint_operation =
                checkpoint_operation(operation, saved.cycle, cycle_recorded_instance);
            saved.cycle_state_sha256 = None;
            saved.checkpoint = None;
            saved.boundary = None;
            saved.restoration = None;
            saved.restoration_state_sha256 = None;
            saved.restore_failure = None;
            saved.phase = Phase::Admitted;
            save(&saved)?;
            saved
        }
        _ => {
            let manager = crate::service_manager::ServiceManager::for_runtime(&root)?;
            let original_health = observe_running(&root, &approval, execution_sha256).await?;
            let saved = registry(&control)?;
            let cycle_recorded_instance = saved.recorded_service_instance()?;
            let cycle_health = copy_health(&original_health)?;
            let journal = Journal {
                schema: 1,
                install: root.clone(),
                control,
                operation,
                selected,
                execution_sha256,
                original_running: original_health.is_some(),
                original_explicit_user_stop: saved.service_explicitly_stopped()?,
                original_recorded_instance: saved.recorded_service_instance()?,
                original_health,
                cycle: 0,
                cycle_health,
                cycle_recorded_instance,
                checkpoint_operation: checkpoint_operation(operation, 0, cycle_recorded_instance),
                cycle_state_sha256: None,
                manager_owned: manager.owns_restart()?,
                manager_definition_sha256: manager_digest(&manager)?,
                phase: Phase::Admitted,
                checkpoint: None,
                boundary: None,
                quiescent_state_sha256: None,
                adoption_plans: Vec::new(),
                adoption_plan: None,
                restoration_state_sha256: None,
                restoration: None,
                restore_failure: None,
            };
            if let Some(observed) = &journal.original_health {
                ensure!(
                    Some(observed.instance) == journal.original_recorded_instance,
                    "maintenance live native owner differs from its recorded service instance"
                );
            }
            save(&journal)?;
            journal
        }
    };
    let mut capability = Maintenance {
        journal,
        main,
        approval,
        exclusion: None,
        _installer: installer,
    };
    match capability.stop().await {
        Ok(()) => Ok(capability),
        Err(primary) => match capability.restore().await {
            Ok(_) => Err(primary),
            Err(restore) => Err(primary.context(format!(
                "saved-index maintenance admission failed and restoration remains pending: {restore:#}"
            ))),
        },
    }
}

/// Dated saved native restoration evidence only. This does not make a live
/// health assertion, start an owner, reopen an index or settle a checkpoint.
pub(crate) fn result(root: &Path, operation: &str) -> Result<Option<RestoreObservation>> {
    let operation: Incarnation = operation.parse()?;
    let root = root.canonicalize()?;
    let _installer = existing_install_lock(&root)?;
    let Some(journal) = read(&root)? else {
        return Ok(None);
    };
    if journal.operation != operation || journal.phase != Phase::Restored {
        return Ok(None);
    }
    journal
        .restoration
        .context("restored native maintenance result missing")
        .map(Some)
}

#[cfg(test)]
mod tests;
