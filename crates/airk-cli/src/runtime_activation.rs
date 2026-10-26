// SPDX-License-Identifier: Apache-2.0
//! Durable installer-owned activation; public selection follows exact candidate health.
use super::*;

const INTENT: &str = "activation.json";
const MAX_INTENT: u64 = 16384;

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Selected,
    AbortedOrFailed,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    schema: u32,
    install: PathBuf,
    candidate: String,
    preceding: Option<String>,
    outcome: Outcome,
    observed_unix_ms: u64,
}

pub(super) fn result(root: &Path) -> Result<Option<Receipt>> {
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
pub(super) struct Intent {
    schema: u32,
    install: PathBuf,
    control: PathBuf,
    pub candidate: String,
    prior: Option<Selection>,
    manager: bool,
    phase: Phase,
}

fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
fn selection(root: &Path) -> Result<Option<Selection>> {
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
            previous: self.prior.as_ref().map(|prior| prior.active.clone()),
        }
    }
    fn save(&self, root: &Path) -> Result<()> {
        ensure!(
            serde_json::to_vec(self)?.len() as u64 <= MAX_INTENT,
            "activation intent exceeds bound"
        );
        replace_json(root, INTENT, self)
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
        let actual = selection(root)?;
        ensure!(
            actual == self.prior || (self.phase == Phase::Probe && actual == Some(self.selected())),
            "public runtime selection changed during activation; preserving intent"
        );
        Ok(())
    }
}

pub(super) fn pending(root: &Path) -> Result<Option<Intent>> {
    if !exists(&root.join(INTENT))? {
        return Ok(None);
    }
    let intent: Intent = serde_json::from_slice(&read_bounded(&root.join(INTENT), MAX_INTENT)?)?;
    intent.validate(root)?;
    Ok(Some(intent))
}
pub(super) fn require_settled(root: &Path) -> Result<()> {
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
    let manager = crate::service_manager::ServiceManager::for_runtime(root)?;
    let managed = manager.owns_restart()?;
    ensure!(
        expected_manager.is_none_or(|expected| expected == managed),
        "startup ownership changed during activation"
    );
    if !managed {
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

fn manager(root: &Path, intent: &Intent) -> Result<crate::service_manager::ServiceManager> {
    let manager = crate::service_manager::ServiceManager::for_runtime(root)?;
    ensure!(
        manager.owns_restart()? == intent.manager,
        "startup ownership changed during activation"
    );
    Ok(manager)
}

async fn probe(root: &Path, approval: &Approval, main: &Path, manager: bool) -> Result<()> {
    start_selected(root, approval, main, Some(manager)).await?;
    ensure!(
        matches!(
            service_runtime::request_for_version(
                &layout::control(root)?,
                ControlMethod::RequestEpoch,
                &approval.version
            )
            .await?,
            ControlResponse::RequestEpoch { .. }
        ),
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
        let _quiescence = quiescent(root)?;
        // Abort only unfinished owned copying; finalize an already completed
        // rename before restoring the preceding verified launcher.
        launcher_repair::settle_for_uninstall(root)?;
        if let Some(prior) = &intent.prior {
            launcher_repair::repair(root, &prior.active)?;
        }
    }
    if let Some(prior) = &intent.prior {
        let (main, approval) = verify_runtime(root, &prior.active)?;
        start_selected(root, &approval, &main, Some(intent.manager)).await?;
    }
    record(root, intent, Outcome::AbortedOrFailed)?;
    finish(root)
}

/// Caller holds the installation lock. Every durable phase can be repeated after
/// process death; a committed public selection is never silently undone.
pub(super) async fn resume(root: &Path) -> Result<()> {
    let mut intent = pending(root)?.context("no pending activation")?;
    let _launcher_guard = if matches!(intent.phase, Phase::Drain | Phase::Launcher) {
        let (main, _) = verify_runtime(root, &intent.candidate)?;
        launcher_repair::replacement_guard(root, &main)?
    } else {
        None
    };
    result(root)?;
    ensure!(
        layout::control(root)? == intent.control,
        "activation state location changed"
    );
    managed::require_unmanaged(root)?;
    if intent.phase == Phase::Rollback {
        return rollback(root, &intent).await;
    }
    let owner = manager(root, &intent)?;
    if intent.phase == Phase::Drain {
        if let Some(prior) = &intent.prior {
            let (main, approval) = verify_runtime(root, &prior.active)?;
            drain(root, &main, &approval).await?;
        }
        if intent.manager {
            owner.stop()?;
        }
        owner.require_stopped_for_maintenance()?;
        drop(quiescent(root)?);
        intent.phase(root, Phase::Launcher)?;
    }
    if intent.phase == Phase::Launcher {
        owner.require_stopped_for_maintenance()?;
        let _quiescence = quiescent(root)?;
        launcher_repair::repair(root, &intent.candidate)?;
        intent.phase(root, Phase::Probe)?;
    }
    let (main, approval) = verify_runtime(root, &intent.candidate)?;
    let committed = selection(root)? == Some(intent.selected());
    match probe(root, &approval, &main, intent.manager).await {
        Ok(_) => {
            if !committed {
                replace_json(root, "selected.json", &intent.selected())?;
            }
            record(root, &intent, Outcome::Selected)?;
            finish(root)
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

pub(super) async fn begin(root: &Path, candidate: &str) -> Result<Selection> {
    managed::require_unmanaged(root)?;
    let (candidate_main, _) = verify_runtime(root, candidate)?;
    let _launcher_guard = launcher_repair::replacement_guard(root, &candidate_main)?;
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
        let prior = selection(root)?;
        if prior
            .as_ref()
            .is_some_and(|prior| prior.active == candidate)
        {
            start_selected(root, &approval, &main, None).await?;
            return prior.context("selection disappeared");
        }
        if let Some(prior) = &prior {
            verify_runtime(root, &prior.active)?;
        }
        let manager = crate::service_manager::ServiceManager::for_runtime(root)?.owns_restart()?;
        Intent {
            schema: 1,
            install: root.canonicalize()?,
            control,
            candidate: candidate.into(),
            prior,
            manager,
            phase: Phase::Drain,
        }
        .save(root)?;
    }
    resume(root).await?;
    let selected = selection_record(root)?;
    ensure!(
        selected.active == candidate,
        "pending activation was aborted; preceding selection remains active"
    );
    Ok(selected)
}

pub(super) async fn abort(root: &Path) -> Result<()> {
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

#[cfg(test)]
#[path = "runtime_activation_tests.rs"]
mod tests;
