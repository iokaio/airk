// SPDX-License-Identifier: Apache-2.0
//! Durable installer-owned activation; public selection follows exact candidate health.
use super::*;

mod catalog_upgrade;
mod checkpoint;
pub(super) use checkpoint::prepare_native_checkpoint;
pub mod rebuild;
pub(super) use rebuild::SuccessorPermit;

const INTENT: &str = "activation.json";
const MAX_INTENT: u64 = 16384;

pub(super) fn activate_managed_genesis(
    authority: &InstallerAuthority<'_>,
    policy: &managed::Managed,
    origin: &airk_store::service::registry::catalog::genesis::ManagedGenesisOrigin,
    owner: &mut airk_store::service::registry::catalog::genesis::CatalogGenesis,
) -> Result<()> {
    catalog_upgrade::activate_managed_genesis(authority, policy, origin, owner)
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Selected,
    AbortedOrFailed,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    schema: u32,
    install: PathBuf,
    candidate: String,
    preceding: Option<String>,
    outcome: Outcome,
    observed_unix_ms: u64,
}

pub fn result(root: &Path) -> Result<Option<Receipt>> {
    let path = root.join("activation-result.json");
    if !exists(&path)? {
        return Ok(None);
    }
    let receipt: Receipt = serde_json::from_slice(&read_bounded(&path, MAX_INTENT)?)?;
    ensure!(
        receipt.schema == 1 && receipt.install == root.canonicalize()?,
        "activation result ownership mismatch"
    );
    digest(&receipt.candidate)?;
    if let Some(prior) = &receipt.preceding {
        digest(prior)?;
    }
    Ok(Some(receipt))
}
pub fn checkpoint_status(root: &Path) -> Result<Option<serde_json::Value>> {
    checkpoint::status(root)
}
pub(super) fn checkpoint_retiring(root: &Path, candidate: &str) -> Result<()> {
    checkpoint::retiring(root, candidate)
}
pub fn verify_admission_release(
    root: &Path,
    control: &Path,
    operation: airk_core::identity::Incarnation,
    selected_inventory: &str,
) -> Result<()> {
    checkpoint::verify_release(root, control, operation, selected_inventory)
}

fn record(root: &Path, intent: &Intent, outcome: Outcome) -> Result<()> {
    result(root)?; // Never overwrite a foreign or malformed historical receipt.
    let receipt = Receipt {
        schema: 1,
        install: intent.install.clone(),
        candidate: intent.candidate.clone(),
        preceding: intent.prior.as_ref().map(|prior| prior.active.clone()),
        outcome,
        observed_unix_ms: unix_ms()?,
    };
    ensure!(
        serde_json::to_vec(&receipt)?.len() as u64 <= MAX_INTENT,
        "activation receipt exceeds bound"
    );
    replace_json(root, "activation-result.json", &receipt)
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Drain,
    Launcher,
    Probe,
    Rollback,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    schema: u32,
    install: PathBuf,
    control: PathBuf,
    pub candidate: String,
    prior: Option<Selection>,
    manager: bool,
    phase: Phase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checkpoint: Option<String>,
}

fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
pub(super) fn admitted_service_candidate(root: &Path, control: &Path) -> Result<Option<String>> {
    Ok(pending(root)?
        .filter(|intent| intent.control == control && intent.phase == Phase::Probe)
        .map(|intent| intent.candidate))
}

pub(super) fn selection(root: &Path) -> Result<Option<Selection>> {
    if exists(&root.join("selected.json"))? {
        Ok(Some(selection_record(root)?))
    } else {
        Ok(None)
    }
}
impl Intent {
    fn selected(&self) -> Selection {
        Selection {
            schema: 1,
            active: self.candidate.clone(),
            previous: self
                .prior
                .as_ref()
                .map(|prior| prior.active.clone())
                .filter(|prior| prior != &self.candidate),
        }
    }
    fn save(&self, root: &Path) -> Result<()> {
        ensure!(
            serde_json::to_vec(self)?.len() as u64 <= MAX_INTENT,
            "activation intent exceeds bound"
        );
        let mut saved = self.clone();
        if self.phase == Phase::Rollback
            && self.checkpoint.is_some()
            && !checkpoint::predecessor_checkpoint_capable(root, self)?
        {
            // Predecessors reject unknown schema-1 fields. The exact current
            // journal restores this reference for the new installer on resume.
            saved.checkpoint = None;
        }
        replace_json(root, INTENT, &saved)
    }
    fn phase(&mut self, root: &Path, phase: Phase) -> Result<()> {
        self.phase = phase;
        self.save(root)
    }
    fn validate(&self, root: &Path) -> Result<()> {
        ensure!(
            self.schema == 1 && self.install == root.canonicalize()?,
            "activation ownership mismatch"
        );
        digest(&self.candidate)?;
        ensure!(
            self.control.is_absolute()
                && self.control.to_str().is_some_and(|path| path.len() <= 4096),
            "invalid activation control binding"
        );
        if let Some(prior) = &self.prior {
            ensure!(
                prior.schema == 1 && prior.active != self.candidate,
                "invalid activation predecessor"
            );
            digest(&prior.active)?;
            if let Some(previous) = &prior.previous {
                digest(previous)?;
            }
        }
        if let Some(operation) = &self.checkpoint {
            ensure!(
                operation.len() == 32
                    && operation
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                "invalid activation checkpoint operation"
            );
        }
        let actual = selection(root)?;
        ensure!(
            actual == self.prior || (self.phase == Phase::Probe && actual == Some(self.selected())),
            "public runtime selection changed during activation; preserving intent"
        );
        Ok(())
    }
}

pub fn pending(root: &Path) -> Result<Option<Intent>> {
    if !exists(&root.join(INTENT))? {
        return Ok(None);
    }
    let mut intent: Intent =
        serde_json::from_slice(&read_bounded(&root.join(INTENT), MAX_INTENT)?)?;
    intent.validate(root)?;
    checkpoint::reattach_legacy_rollback(root, &mut intent)?;
    Ok(Some(intent))
}
pub fn require_settled(root: &Path) -> Result<()> {
    ensure!(
        pending(root)?.is_none(),
        "runtime activation is pending; resume ensure or explicitly abort-activation first"
    );
    Ok(())
}
fn finish(root: &Path) -> Result<()> {
    fs::remove_file(root.join(INTENT))?;
    sync_directory(root)
}

/// Only foreground OS startup follows this private intent. Clients still see the
/// preceding public selection until exact native candidate health has succeeded.
pub(super) fn startup(root: &Path) -> Result<Option<(PathBuf, Approval)>> {
    let Some(intent) = pending(root)? else {
        return Ok(None);
    };
    ensure!(
        layout::control(root)? == intent.control,
        "activation state location changed"
    );
    let identity = match intent.phase {
        Phase::Probe => &intent.candidate,
        Phase::Rollback => {
            &intent
                .prior
                .as_ref()
                .context("initial activation is being aborted")?
                .active
        }
        Phase::Drain | Phase::Launcher => {
            bail!("runtime activation is draining or repairing its launcher")
        }
    };
    verify_runtime(root, identity).map(Some)
}

/// None discovers current ownership; a durable activation supplies its saved
/// policy so losing/changing an OS owner cannot silently fall back to on-demand.
pub(super) async fn start_selected(
    root: &Path,
    approval: &Approval,
    main: &Path,
    expected_manager: Option<bool>,
) -> Result<ServiceHealth> {
    start_selected_with_binding(root, approval, main, expected_manager, None).await
}

/// Borrowed, non-Clone, non-serialized ownership of one saved activation's
/// startup inputs. It cannot authorize another operation or survive its caller.
pub(super) struct StartupBinding<'a> {
    root: &'a Path,
    intent: &'a Intent,
    runtime: String,
    main: PathBuf,
}
impl<'a> StartupBinding<'a> {
    pub(super) fn new(root: &'a Path, intent: &'a Intent, runtime: &str, main: &Path) -> Self {
        Self {
            root,
            intent,
            runtime: runtime.into(),
            main: main.into(),
        }
    }
    pub(super) fn validate(&self, root: &Path, runtime: &str) -> Result<()> {
        ensure!(
            self.root == root
                && self.runtime == runtime
                && self.intent.install == root.canonicalize()?,
            "activation startup binding changed"
        );
        let saved = pending(root)?.context("activation startup operation disappeared")?;
        ensure!(
            serde_json::to_value(&saved)? == serde_json::to_value(self.intent)?,
            "activation startup operation changed"
        );
        ensure!(
            layout::control(root)? == self.intent.control,
            "activation startup control changed"
        );
        let expected = match self.intent.phase {
            Phase::Probe => self.intent.candidate.as_str(),
            Phase::Rollback => self
                .intent
                .prior
                .as_ref()
                .context("rollback startup predecessor missing")?
                .active
                .as_str(),
            _ => bail!("activation startup phase does not admit a process"),
        };
        ensure!(expected == runtime, "activation startup runtime changed");
        managed::require_unmanaged(root)?;
        let (main, approval) = verify_runtime(root, runtime)?;
        ensure!(
            main == self.main && approval.manifest_sha256 == runtime,
            "activation startup payload changed"
        );
        ensure!(
            crate::service_manager::ServiceManager::for_runtime(root)?.owns_restart()?
                == self.intent.manager,
            "startup ownership changed during activation"
        );
        Ok(())
    }
}

async fn start_for_intent(
    root: &Path,
    intent: &Intent,
    approval: &Approval,
    main: &Path,
) -> Result<ServiceHealth> {
    let binding = StartupBinding::new(root, intent, &approval.manifest_sha256, main);
    start_selected_with_binding(root, approval, main, Some(intent.manager), Some(binding)).await
}

/// Who starts a stopped service. A registered login/startup entry means the OS
/// manager owns the one service process and is asked to start it; only an
/// installation without one (never registered, removed, or refused) starts the
/// detached on-demand `serve` child. A CLI command never becomes the service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StartRoute {
    OsManager,
    OnDemandChild,
}

pub(super) fn start_route(manager_owns_restart: bool) -> StartRoute {
    if manager_owns_restart {
        StartRoute::OsManager
    } else {
        StartRoute::OnDemandChild
    }
}

async fn start_selected_with_binding(
    root: &Path,
    approval: &Approval,
    main: &Path,
    expected_manager: Option<bool>,
    binding: Option<StartupBinding<'_>>,
) -> Result<ServiceHealth> {
    if selection(root)?.is_some_and(|selected| selected.active == approval.manifest_sha256) {
        ensure!(
            !state_contract::required(root)?,
            "clean_break_required: start cannot reopen predecessor state"
        );
    }
    let manager = crate::service_manager::ServiceManager::for_runtime(root)?;
    let managed = manager.owns_restart()?;
    ensure!(
        expected_manager.is_none_or(|expected| expected == managed),
        "startup ownership changed during activation"
    );
    if start_route(managed) == StartRoute::OnDemandChild {
        if let Some(binding) = binding {
            return start_for_operation(root, approval, main, binding).await;
        }
        return start(root, approval, main, true).await;
    }
    let expected = runtime_pair::digest_file(main)?;
    if let Ok(observation) = health(root, approval, &expected).await {
        return Ok(observation);
    }
    // OS policy owns its process. Never start a competing detached child.
    manager.start()?;
    let until = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if let Ok(observation) = health(root, approval, &expected).await {
            return Ok(observation);
        }
        ensure!(
            tokio::time::Instant::now() < until,
            "OS-managed runtime did not report exact healthy payload within 60 seconds"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Ordinary settled election only. The legacy activation/probe/rollback/startup
/// routes retain their existing independent verification and saved identities.
pub(super) async fn start_verified(verified: &VerifiedSelection<'_>) -> Result<ServiceHealth> {
    start_verified_checked(verified, None).await
}

pub(super) async fn start_verified_checked(
    verified: &VerifiedSelection<'_>,
    cancel: Option<&setup::SetupCancellation>,
) -> Result<ServiceHealth> {
    if let Some(cancel) = cancel {
        cancel.checkpoint()?;
    }
    let root = verified.authority.root;
    ensure!(
        !state_contract::required(root)?,
        "clean_break_required: installed state has not crossed the 1.8.16 baseline"
    );
    ensure!(
        verified.selection.active == verified.approval.manifest_sha256,
        "runtime selection identity mismatch"
    );
    let manager = crate::service_manager::ServiceManager::for_runtime(root)?;
    if start_route(manager.owns_restart()?) == StartRoute::OnDemandChild {
        return start_with_digest_checked(
            root,
            &verified.approval,
            &verified.main,
            true,
            Some(&verified.main_sha256),
            cancel,
        )
        .await;
    }
    // OS ownership is still authoritative; never start a competing detached child.
    let (observation, _attempt) = setup::observe_or_effect(
        health(root, &verified.approval, &verified.main_sha256),
        cancel,
        || manager.start().map(|_| ()),
    )
    .await?;
    if let Some(observation) = observation {
        return Ok(observation);
    }
    let until = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if let Ok(observation) = health(root, &verified.approval, &verified.main_sha256).await {
            return Ok(observation);
        }
        ensure!(
            tokio::time::Instant::now() < until,
            "OS-managed runtime did not report exact healthy payload within 60 seconds"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn manager(root: &Path, intent: &Intent) -> Result<crate::service_manager::ServiceManager> {
    let manager = crate::service_manager::ServiceManager::for_runtime(root)?;
    ensure!(
        manager.owns_restart()? == intent.manager,
        "startup ownership changed during activation"
    );
    Ok(manager)
}

async fn probe(root: &Path, intent: &Intent, approval: &Approval, main: &Path) -> Result<()> {
    let observed = start_for_intent(root, intent, approval, main).await?;
    probe_observed(root, approval, observed).await
}

/// A rebuild starts its candidate once under its own journal, which may already
/// have settled the activation intent. It keeps ordinary immediate admission.
async fn probe_unscheduled(
    root: &Path,
    approval: &Approval,
    main: &Path,
    manager: bool,
) -> Result<()> {
    let observed = start_selected(root, approval, main, Some(manager)).await?;
    probe_observed(root, approval, observed).await
}

async fn probe_observed(root: &Path, approval: &Approval, observed: ServiceHealth) -> Result<()> {
    if let Some(intent) = pending(root)?
        && let Some(operation) = &intent.checkpoint
        && airk_host::checkpoint_fence::admission_closed(&intent.control)?
    {
        let operation = operation.parse()?;
        let response = control_client::request_for_version_and_instance(
            &intent.control,
            ControlMethod::UpgradeRecoveryProbe {
                operation_id: operation,
            },
            &approval.version,
            observed.instance,
        )
        .await?;
        let expected_namespace = checkpoint::expected_namespace(root, &intent)?;
        ensure!(
            matches!(response,ControlResponse::UpgradeRecoveryReady {operation_id,service_instance,execution_sha256,namespace} if operation_id==operation && service_instance==observed.instance && execution_sha256.to_string()==observed.executable_sha256 && expected_namespace.is_none_or(|expected|namespace==Some(expected))),
            "candidate recovery registry is not ready"
        );
        return Ok(());
    }
    let response = if let Some(permit) = state_contract::initialized_probe(
        root,
        &approval.manifest_sha256,
        &approval.version,
        observed.instance,
        &observed.executable_sha256,
    )? {
        control_client::request_initialized_probe(&permit).await?
    } else {
        control_client::request_for_version(
            &layout::control(root)?,
            ControlMethod::RequestEpoch,
            &approval.version,
        )
        .await?
    };
    ensure!(
        matches!(response, ControlResponse::RequestEpoch { .. }),
        "candidate control health is available but its registry is not ready"
    );
    Ok(())
}

async fn rollback(root: &Path, intent: &Intent) -> Result<()> {
    result(root)?;
    ensure!(
        selection(root)? == intent.prior,
        "committed activation requires an explicit verified rollback"
    );
    if let Some(prior) = &intent.prior {
        let (main, _) = verify_runtime(root, &prior.active)?;
        checkpoint::require_target(&intent.control, &main)?;
    }
    let manager = manager(root, intent)?;
    if intent.manager {
        manager.stop()?;
    }
    let (candidate, approval) = verify_runtime(root, &intent.candidate)?;
    if let Err(candidate_error) = drain(root, &candidate, &approval).await {
        let prior = intent.prior.as_ref().context(candidate_error)?;
        let (main, approval) = verify_runtime(root, &prior.active)?;
        drain(root, &main, &approval).await?;
    }
    manager.require_stopped_for_maintenance()?;
    {
        let quiescence = quiescent(root)?;
        if let Some(prior) = &intent.prior {
            let (main, _) = verify_runtime(root, &prior.active)?;
            quiescence.require_catalog_compatible(&main)?;
        }
        // Abort only unfinished owned copying; finalize an already completed
        // rename before restoring the preceding verified launcher.
        launcher_repair::settle_for_uninstall(root)?;
        if let Some(prior) = &intent.prior {
            launcher_repair::repair(root, &prior.active)?;
        }
    }
    if let Some(prior) = &intent.prior {
        let (main, approval) = verify_runtime(root, &prior.active)?;
        start_for_intent(root, intent, &approval, &main).await?;
    }
    record(root, intent, Outcome::AbortedOrFailed)?;
    checkpoint::stage(root, intent, checkpoint::Stage::Aborted)?;
    checkpoint::settle_admission(root, intent).await?;
    finish(root)
}

/// Caller holds the installation lock. Only an accepted fresh Genesis activation
/// resumes before lifecycle settlement; no in-place catalog upgrade can resume.
pub(super) async fn resume_catalog_before_lifecycle(
    root: &Path,
    expected: Option<&str>,
) -> Result<bool> {
    let Some(intent) = pending(root)? else {
        return Ok(false);
    };
    if intent.phase != Phase::Probe || intent.prior.is_some() {
        return Ok(false);
    }
    intent.validate(root)?;
    ensure!(
        expected.is_none_or(|candidate| candidate == intent.candidate),
        "another candidate owns the pending fresh activation"
    );
    resume(root).await?;
    Ok(true)
}

/// Resume under the same installer ownership, preserving the public selection.
pub(super) async fn resume(root: &Path) -> Result<()> {
    if !rebuild::pending(root)? && state_contract::required(root)? {
        bail!(
            "clean_break_required: activate a supported 1.8.16-or-later distribution to replace the old state"
        );
    }
    if rebuild::pending(root)? {
        rebuild::resume(root).await?;
        return Ok(());
    }
    require_issuer_rotation_settled(root)?;
    let mut intent = pending(root)?.context("no pending activation")?;
    result(root)?;
    let fresh = intent.phase == Phase::Probe && intent.prior.is_none();
    let control = if fresh {
        // Only this accepted fresh installer operation may resume its native
        // initialization before the ordinary registry binding is ready. The
        // Genesis owner rechecks the exact layout/namespace under exclusion.
        airk_host::install_layout::initialization_control(root)?
    } else {
        layout::control(root)?
    };
    ensure!(
        control == intent.control,
        "activation state location changed"
    );
    managed::require_unmanaged(root)?;
    if intent.phase == Phase::Rollback {
        return rollback(root, &intent).await;
    }
    let owner = manager(root, &intent)?;
    if intent.phase == Phase::Drain {
        checkpoint::preflight(root, &intent)?;
        checkpoint::stage(root, &intent, checkpoint::Stage::Quiescing)?;
        checkpoint::prepare_graceful(root, &intent).await?;
        if intent.manager {
            owner.stop()?;
        }
        if let Some(prior) = &intent.prior {
            #[cfg(target_os = "macos")]
            {
                let (main, approval) = verify_runtime(root, &prior.active)?;
                // Healthy predecessors still upgrade on supported old systems
                // without the newer atomic hard-stop API. If graceful stop
                // fails, retain its first diagnostic before exact-token stop.
                if let Err(error) = drain(root, &main, &approval).await {
                    checkpoint::predecessor_shutdown_failed(root, &intent, &error)?;
                }
            }
            upgrade_stop::stop(root, &prior.active)?;
        }
    }
    let _launcher_guard = if matches!(intent.phase, Phase::Drain | Phase::Launcher) {
        let (main, _) = verify_runtime(root, &intent.candidate)?;
        launcher_repair::replacement_guard(root, &main)?
    } else {
        None
    };
    if intent.phase == Phase::Drain {
        if let Some(prior) = &intent.prior {
            let (main, approval) = verify_runtime(root, &prior.active)?;
            drain(root, &main, &approval).await?;
        }
        owner.require_stopped_for_maintenance()?;
        let quiescence = quiescent(root)?;
        checkpoint::capture(root, &intent, &quiescence)?;
        checkpoint::arm_recovery(root, &intent, &quiescence)?;
        checkpoint::stage(root, &intent, checkpoint::Stage::OldStopped)?;
        drop(quiescence);
        intent.phase(root, Phase::Launcher)?;
    }
    if intent.phase == Phase::Launcher {
        owner.require_stopped_for_maintenance()?;
        let quiescence = quiescent(root)?;
        let (main, approval) = verify_runtime(root, &intent.candidate)?;
        quiescence.require_catalog_compatible(&main)?;
        checkpoint::verify(root, &intent)?;
        launcher_repair::repair(root, &intent.candidate)?;
        if approval.version == env!("CARGO_PKG_VERSION") {
            airk_host::process_roles::install(&main)?;
        }
        intent.phase(root, Phase::Probe)?;
    }
    checkpoint::stage(root, &intent, checkpoint::Stage::ActivationPrepared)?;
    let (main, approval) = verify_runtime(root, &intent.candidate)?;
    let committed = selection(root)? == Some(intent.selected());
    if intent.prior.is_none() {
        // Fresh Genesis must precede the first ordinary service probe: even a
        // health-only legacy startup records service history and cannot later
        // be adopted as a fresh namespace. Keep the real absent selection until
        // qualified metadata and actual candidate health have both succeeded.
        catalog_upgrade::activate(root, &intent).await?;
        probe(root, &intent, &approval, &main).await?;
        if !committed {
            replace_json(root, "selected.json", &intent.selected())?;
        }
        return complete_selected(root, &intent).await;
    }
    if committed && data_migration::needed(root)? {
        migrate_selected(root, &intent, &main, &approval).await?;
    }
    match probe(root, &intent, &approval, &main).await {
        Ok(_) => {
            if !committed {
                replace_json(root, "selected.json", &intent.selected())?;
            }
            // Directory migration follows verified candidate health and durable
            // selection. A partial layout must resume with the matching runtime,
            // rather than automatically rolling back to an older metadata reader.
            if data_migration::needed(root)? {
                migrate_selected(root, &intent, &main, &approval).await?;
                probe(root, &intent, &approval, &main).await?;
            }
            // Drain and require the existing catalog; an in-place upgrade of
            // a registry without one is refused (catalog_upgrade_unsupported).
            catalog_upgrade::activate(root, &intent).await?;
            probe(root, &intent, &approval, &main).await?;
            complete_selected(root, &intent).await
        }
        Err(error) if committed => {
            Err(error
                .context("selected candidate is unavailable; public selection remains committed"))
        }
        Err(error) => {
            intent.phase(root, Phase::Rollback)?;
            rollback(root, &intent)
                .await
                .context("candidate failed and predecessor restoration is still pending")?;
            Err(error.context("candidate failed; preceding public selection was preserved"))
        }
    }
}

async fn complete_selected(root: &Path, intent: &Intent) -> Result<()> {
    checkpoint::stage(root, intent, checkpoint::Stage::CandidateStarted)?;
    record(root, intent, Outcome::Selected)?;
    checkpoint::stage(root, intent, checkpoint::Stage::Committed)?;
    checkpoint::settle_admission(root, intent).await?;
    finish(root)
}

#[cfg(test)]
mod setup_tests {
    use super::*;

    #[tokio::test]
    async fn accepted_activation_completion_after_close_does_not_authorize_final_start()
    -> Result<()> {
        let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
        let prior = "c".repeat(64);
        replace_json(
            &root,
            "selected.json",
            &Selection {
                schema: 1,
                active: identity.clone(),
                previous: Some(prior.clone()),
            },
        )?;
        replace_json(
            &root,
            INTENT,
            &serde_json::json!({
                "schema":1,"install":root,"control":layout::control(&root)?,"candidate":identity,
                "prior":{"schema":1,"active":prior,"previous":null},"manager":false,"phase":"probe"
            }),
        )?;
        let lock = install_lock(&root)?;
        let _authority = InstallerAuthority {
            root: &root,
            _lock: &lock,
        };
        let intent = pending(&root)?.context("saved exact intent missing")?;
        let cancel = setup::SetupCancellation::default();
        let (release, released) = tokio::sync::oneshot::channel::<()>();
        // Inject only the completed exact probe result. Use the real production
        // receipt/finish operation, then the actual post-resume admission fence.
        let mut completion = Box::pin(setup::settle_then_checkpoint(
            async {
                released.await?;
                complete_selected(&root, &intent).await
            },
            Some(&cancel),
        ));
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(completion.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        cancel.close();
        release
            .send(())
            .expect("accepted intent completion retained");
        assert!(completion.await.is_err());
        assert!(pending(&root)?.is_none());
        let receipt = result(&root)?.context("accepted intent lost completion receipt")?;
        assert_eq!(receipt.candidate, identity);
        assert!(matches!(receipt.outcome, Outcome::Selected));
        assert_eq!(selection_record(&root)?.active, identity);
        assert!(!root.join(format!("startup-{identity}.json")).exists());
        assert!(!root.join("bootstrap.json").exists());
        Ok(())
    }
}

async fn migrate_selected(
    root: &Path,
    intent: &Intent,
    main: &Path,
    approval: &Approval,
) -> Result<()> {
    let owner = manager(root, intent)?;
    if intent.manager {
        owner.stop()?;
    }
    // The matching runtime may itself be resuming this installation. Its
    // authenticated shutdown drains owned workers without terminating the caller.
    drain(root, main, approval).await?;
    owner.require_stopped_for_maintenance()?;
    data_migration::run(root)
}

/// Called under the installer lock, before any drain, launcher repair or start.
/// A completed exact replay only acknowledges its receipt, including after Stop.
pub(super) fn reviewed_request(root: &Path, candidate: &str, expected: &str) -> Result<bool> {
    digest(candidate)?;
    digest(expected)?;
    managed::require_unmanaged(root)?;
    if let Some(intent) = pending(root)? {
        ensure!(
            intent.candidate == candidate
                && intent
                    .prior
                    .as_ref()
                    .is_some_and(|prior| prior.active == expected),
            "pending activation differs from the reviewed request"
        );
        intent.validate(root)?;
        return Ok(false);
    }
    let active = selection(root)?.context("reviewed activation requires an existing selection")?;
    if active.active == expected {
        ensure!(
            candidate != expected,
            "reviewed runtime is already selected"
        );
        return Ok(false);
    }
    if active.active == candidate
        && let Some(receipt) = result(root)?
        && receipt.candidate == candidate
        && receipt.preceding.as_deref() == Some(expected)
        && matches!(receipt.outcome, Outcome::Selected)
    {
        verify_runtime(root, candidate)?;
        return Ok(true);
    }
    bail!("runtime selection changed since review")
}

/// The caller holds the installer lock.
pub(super) async fn begin(root: &Path, candidate: &str) -> Result<Selection> {
    // An interrupted standalone adoption may still hold its own native
    // admission fence; settle it rather than letting it block this activation.
    legacy_maintenance::settle_locked(root).await?;
    legacy_maintenance::require_settled(root)?;
    if state_contract::required(root)? {
        let selected = selection(root)?.context("clean-break predecessor missing")?;
        return rebuild::activate(root, candidate, &selected.active).await;
    }
    if rebuild::pending(root)? {
        return rebuild::resume(root).await;
    }
    require_issuer_rotation_settled(root)?;
    managed::require_unmanaged(root)?;
    verify_runtime(root, candidate)?;
    ensure!(
        !exists(&root.join("uninstall.json"))?,
        "resume pending uninstall before activation"
    );
    if let Some(intent) = pending(root)? {
        ensure!(
            intent.candidate == candidate,
            "another activation is pending; resume or abort it first"
        );
    } else {
        result(root)?;
        let control = layout::control(root)?;
        launcher_repair::require_settled(root)?;
        let (main, approval) = verify_runtime(root, candidate)?;
        checkpoint::require_target(&control, &main)?;
        let prior = selection(root)?;
        if prior
            .as_ref()
            .is_some_and(|prior| prior.active == candidate)
        {
            start_selected(root, &approval, &main, None).await?;
            return prior.context("selection disappeared");
        }
        legacy_caches::require_settled(root)?;
        if let Some(prior) = &prior {
            verify_runtime(root, &prior.active)?;
        }
        let manager = crate::service_manager::ServiceManager::for_runtime(root)?.owns_restart()?;
        let mut intent = Intent {
            schema: 1,
            install: root.canonicalize()?,
            control,
            candidate: candidate.into(),
            prior,
            manager,
            phase: Phase::Drain,
            checkpoint: None,
        };
        checkpoint::initialize(root, &mut intent)?;
        intent.save(root)?;
    }
    resume(root).await?;
    let selected = selection_record(root)?;
    ensure!(
        selected.active == candidate,
        "pending activation was aborted; preceding selection remains active"
    );
    Ok(selected)
}

pub async fn abort(root: &Path) -> Result<()> {
    require_issuer_rotation_settled(root)?;
    let Some(mut intent) = pending(root)? else {
        return Ok(());
    };
    ensure!(
        layout::control(root)? == intent.control,
        "activation state location changed"
    );
    ensure!(
        selection(root)? == intent.prior,
        "activation already committed; use explicit runtime rollback"
    );
    intent.phase(root, Phase::Rollback)?;
    rollback(root, &intent).await
}

/// A newer installer may cancel an exact older intent only before candidate probe.
/// Keep the guard under the installer lock, including across competing windows.
pub fn require_superseded(root: &Path, candidate: &str, active: &str) -> Result<()> {
    let intent = pending(root)?.context("saved activation disappeared")?;
    ensure!(
        layout::control(root)? == intent.control,
        "activation state location changed"
    );
    manager(root, &intent)?;
    ensure!(
        intent.candidate == candidate
            && intent
                .prior
                .as_ref()
                .is_some_and(|prior| prior.active == active)
            && matches!(intent.phase, Phase::Drain | Phase::Launcher),
        "saved activation changed or candidate probe already began; inspect before superseding"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
