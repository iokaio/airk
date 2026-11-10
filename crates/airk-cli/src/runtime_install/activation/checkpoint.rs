// SPDX-License-Identifier: Apache-2.0
//! Installer checkpoint references. Canonical stores remain the state authority.
use super::*;
use airk_store::service::registry::checkpoint::{self as store, CheckpointSnapshot};

const JOURNAL: &str = "upgrade-journal.json";
const MAX_CHECKPOINT: u64 = 512 * 1024;
const MAX_MANIFEST_PACKET: u64 = MAX_CHECKPOINT - MAX_INTENT - 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Stage {
    StagedVerified,
    Quiescing,
    CheckpointReady,
    OldStopped,
    ActivationPrepared,
    CandidateStarted,
    Committed,
    Retiring,
    Aborted,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    operation: String,
    install: PathBuf,
    control: PathBuf,
    candidate: String,
    predecessor: Option<String>,
    manager_owned: bool,
    manager_definition_sha256: String,
    stage: Stage,
    revision: u64,
    previous_digest: Option<String>,
    checkpoint_sha256: Option<String>,
    observed_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    predecessor_shutdown_failure: Option<ShutdownDiagnostic>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ShutdownDiagnostic {
    observed_unix_ms: u64,
    bytes: u64,
    sha256: String,
    message: String,
    truncated: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    journal: Journal,
    sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    operation: String,
    install: PathBuf,
    control: PathBuf,
    candidate: String,
    predecessor: Option<String>,
    manager_definition_sha256: String,
    captured_unix_ms: u64,
    boundary: Boundary,
    native: Option<airk_core::service_checkpoint::UpgradeCheckpointObservation>,
    state: CheckpointSnapshot,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Boundary {
    GracefulNativeCommit,
    NoRunningServiceCommittedStores,
    CommittedStoresAfterVerifiedOwnerExit,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn name(operation: &str) -> Result<String> {
    ensure!(
        operation.len() == 32
            && operation
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "invalid checkpoint operation"
    );
    Ok(format!("checkpoint-{operation}.json"))
}

fn binding(journal: &Journal, root: &Path, intent: &Intent) -> Result<()> {
    name(&journal.operation)?;
    digest(&journal.candidate)?;
    digest(&journal.manager_definition_sha256)?;
    if let Some(prior) = &journal.predecessor {
        digest(prior)?;
    }
    if let Some(previous) = &journal.previous_digest {
        digest(previous)?;
    }
    if let Some(checkpoint) = &journal.checkpoint_sha256 {
        digest(checkpoint)?;
    }
    ensure!(
        journal.schema == 1
            && journal.install == root.canonicalize()?
            && journal.control == intent.control
            && journal.candidate == intent.candidate
            && journal.predecessor == intent.prior.as_ref().map(|prior| prior.active.clone())
            && journal.manager_owned == intent.manager
            && intent.checkpoint.as_deref() == Some(&journal.operation),
        "upgrade journal crossed its activation binding"
    );
    Ok(())
}

fn read(root: &Path) -> Result<Option<Envelope>> {
    if !exists(&root.join(JOURNAL))? {
        return Ok(None);
    }
    let bytes = read_bounded(&root.join(JOURNAL), MAX_INTENT)?;
    let envelope: Envelope = serde_json::from_slice(&bytes)?;
    ensure!(
        hash(&serde_json::to_vec(&envelope.journal)?) == envelope.sha256
            && serde_json::to_vec(&envelope)? == bytes,
        "upgrade journal digest or encoding changed"
    );
    let journal = &envelope.journal;
    if let Some(diagnostic) = &journal.predecessor_shutdown_failure {
        digest(&diagnostic.sha256)?;
        ensure!(
            !diagnostic.message.is_empty()
                && diagnostic.message.len() <= 1024
                && diagnostic.bytes >= diagnostic.message.len() as u64
                && diagnostic.truncated == (diagnostic.bytes > diagnostic.message.len() as u64)
                && (diagnostic.truncated
                    || hash(diagnostic.message.as_bytes()) == diagnostic.sha256),
            "upgrade predecessor shutdown diagnostic changed"
        );
    }
    name(&journal.operation)?;
    digest(&journal.candidate)?;
    digest(&journal.manager_definition_sha256)?;
    if let Some(prior) = &journal.predecessor {
        digest(prior)?;
        ensure!(
            prior != &journal.candidate,
            "upgrade predecessor equals candidate"
        );
    }
    if let Some(previous) = &journal.previous_digest {
        digest(previous)?;
    }
    if let Some(checkpoint) = &journal.checkpoint_sha256 {
        digest(checkpoint)?;
    }
    ensure!(
        journal.schema == 1
            && journal.install == root.canonicalize()?
            && journal.control.is_absolute()
            && journal.revision <= 32
            && (journal.revision == 0 || journal.previous_digest.is_some()),
        "upgrade journal root, schema or revision changed"
    );
    Ok(Some(envelope))
}

fn save(root: &Path, journal: Journal, expected: Option<&str>) -> Result<()> {
    ensure!(
        read(root)?.as_ref().map(|value| value.sha256.as_str()) == expected,
        "upgrade_journal_conflict: expected preceding digest changed"
    );
    let envelope = Envelope {
        sha256: hash(&serde_json::to_vec(&journal)?),
        journal,
    };
    ensure!(
        serde_json::to_vec(&envelope)?.len() as u64 <= MAX_INTENT,
        "upgrade journal capacity exceeded"
    );
    replace_json(root, JOURNAL, &envelope)
}

fn manager_binding(root: &Path, intent: &Intent) -> Result<String> {
    let manager = crate::service_manager::ServiceManager::for_runtime(root)?;
    ensure!(
        manager.owns_restart()? == intent.manager,
        "startup ownership changed before checkpoint binding"
    );
    if intent.manager {
        return Ok(hash(&serde_json::to_vec(&manager.plan()?)?));
    }
    // No OS definition exists to capture for user-on-demand ownership. In
    // particular, a safely aborted first activation can retain empty control
    // leases without any selected image from which to generate such a plan.
    // Root/control/candidate bindings remain in the journal, and actual restart
    // ownership is rechecked here and by the ordinary activation coordinator.
    Ok(hash(b"airk/checkpoint-manager/no-owned-restart/1"))
}

/// Called under the installer lock before publishing the new activation intent.
pub(super) fn initialize(root: &Path, intent: &mut Intent) -> Result<()> {
    let prior = read(root)?;
    if intent.prior.is_none() && prior.is_none() {
        // A genuine first activation has no predecessor state to checkpoint.
        // Genesis retains sole ownership of creating its namespace. Validate
        // the retained layout before considering absence: a lost initialized
        // control directory must never be reclassified as a fresh install.
        crate::install_layout::validate_control(&intent.control)?;
        match fs::symlink_metadata(&intent.control) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ensure!(
                    intent.checkpoint.is_none(),
                    "fresh activation has an orphan checkpoint reference"
                );
                return Ok(());
            }
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
    }
    if let Some(envelope) = &prior
        && !matches!(
            envelope.journal.stage,
            Stage::Committed | Stage::Retiring | Stage::Aborted
        )
    {
        intent.checkpoint = Some(envelope.journal.operation.clone());
        binding(&envelope.journal, root, intent)?;
        ensure!(
            envelope.journal.stage == Stage::StagedVerified,
            "upgrade recovery requires its original activation intent"
        );
        return Ok(());
    }
    let operation = airk_kernel::service_broker::new_instance_id()?.to_string();
    let manager_definition_sha256 = manager_binding(root, intent)?;
    let journal = Journal {
        schema: 1,
        operation: operation.clone(),
        install: root.canonicalize()?,
        control: intent.control.clone(),
        candidate: intent.candidate.clone(),
        predecessor: intent.prior.as_ref().map(|prior| prior.active.clone()),
        manager_owned: intent.manager,
        manager_definition_sha256,
        stage: Stage::StagedVerified,
        revision: 0,
        previous_digest: prior.as_ref().map(|value| value.sha256.clone()),
        checkpoint_sha256: None,
        observed_unix_ms: unix_ms()?,
        predecessor_shutdown_failure: None,
    };
    intent.checkpoint = Some(operation);
    binding(&journal, root, intent)?;
    save(
        root,
        journal,
        prior.as_ref().map(|value| value.sha256.as_str()),
    )
}

/// A failed graceful attempt does not authorize a kill or prove an exit.
/// Preserve its first bounded diagnostic through subsequent hard-stop recovery.
#[cfg(any(target_os = "macos", test))]
pub(super) fn predecessor_shutdown_failed(
    root: &Path,
    intent: &Intent,
    error: &anyhow::Error,
) -> Result<()> {
    let mut envelope =
        read(root)?.context("upgrade journal missing for graceful shutdown diagnostic")?;
    binding(&envelope.journal, root, intent)?;
    ensure!(
        intent.prior.is_some(),
        "no predecessor for shutdown diagnostic"
    );
    if envelope.journal.predecessor_shutdown_failure.is_some() {
        return Ok(());
    }
    let message = format!("Graceful predecessor shutdown failed: {error:#}");
    let mut end = message.len().min(1024);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    envelope.journal.predecessor_shutdown_failure = Some(ShutdownDiagnostic {
        observed_unix_ms: unix_ms()?,
        bytes: message.len() as u64,
        sha256: hash(message.as_bytes()),
        message: message[..end].to_owned(),
        truncated: end != message.len(),
    });
    envelope.journal.previous_digest = Some(envelope.sha256.clone());
    envelope.journal.revision = envelope
        .journal
        .revision
        .checked_add(1)
        .context("upgrade journal revision exhausted")?;
    envelope.journal.observed_unix_ms = unix_ms()?;
    save(root, envelope.journal, Some(&envelope.sha256))
}

pub(super) fn stage(root: &Path, intent: &Intent, next: Stage) -> Result<()> {
    if intent.checkpoint.is_none() {
        return Ok(());
    } // Strict historical schema-1 decoder.
    let envelope = read(root)?.context("upgrade journal missing")?;
    binding(&envelope.journal, root, intent)?;
    if envelope.journal.stage == next {
        return Ok(());
    }
    let previous = envelope.journal.stage;
    // A crash can leave the journal one durable effect ahead of activation.json.
    // Reconcile the same operation without repeating or moving that proof back.
    fn rank(stage: Stage) -> usize {
        match stage {
            Stage::StagedVerified => 0,
            Stage::Quiescing => 1,
            Stage::CheckpointReady => 2,
            Stage::OldStopped => 3,
            Stage::ActivationPrepared => 4,
            Stage::CandidateStarted => 5,
            Stage::Committed => 6,
            Stage::Retiring => 7,
            Stage::Aborted => 8,
        }
    }
    if previous != Stage::Aborted && next != Stage::Aborted && rank(previous) > rank(next) {
        return Ok(());
    }
    if next == Stage::CheckpointReady {
        let bytes = read_bounded(
            &root.join(name(&envelope.journal.operation)?),
            MAX_CHECKPOINT,
        )?;
        ensure!(
            envelope.journal.checkpoint_sha256.as_ref() == Some(&hash(&bytes)),
            "checkpoint_ready requires its immutable complete manifest"
        );
    }
    ensure!(
        matches!(
            (previous, next),
            (Stage::StagedVerified, Stage::Quiescing)
                | (Stage::Quiescing, Stage::CheckpointReady)
                | (Stage::CheckpointReady, Stage::OldStopped)
                | (Stage::OldStopped, Stage::ActivationPrepared)
                | (Stage::ActivationPrepared, Stage::CandidateStarted)
                | (Stage::CandidateStarted, Stage::Committed)
                | (Stage::Committed, Stage::Retiring)
        ) || next == Stage::Aborted && !matches!(previous, Stage::Committed | Stage::Retiring),
        "invalid upgrade journal transition"
    );
    let mut journal = envelope.journal;
    journal.stage = next;
    journal.revision = journal
        .revision
        .checked_add(1)
        .context("upgrade revision overflow")?;
    journal.previous_digest = Some(envelope.sha256.clone());
    journal.observed_unix_ms = unix_ms()?;
    save(root, journal, Some(&envelope.sha256))
}

/// Full roster and engine compatibility refusal happens while the predecessor
/// can still serve. Capacity errors never intentionally stop it first.
pub(super) fn preflight(root: &Path, intent: &Intent) -> Result<()> {
    if intent.checkpoint.is_none() {
        return Ok(());
    }
    let envelope = read(root)?.context("upgrade journal missing")?;
    binding(&envelope.journal, root, intent)?;
    let (target, _) = verify_runtime(root, &intent.candidate)?;
    require_target(&intent.control, &target)?;
    if envelope.journal.stage == Stage::StagedVerified {
        ensure!(
            manager_binding(root, intent)? == envelope.journal.manager_definition_sha256,
            "captured manager definition changed before quiescence"
        );
    }
    preflight_live_state(root, intent)
}

fn preflight_live_state(root: &Path, intent: &Intent) -> Result<()> {
    let state = store::preflight_snapshot(&intent.control)?;
    if !checkpoint_capable(root, intent)? {
        // A legacy owner is still serving here. Observe accepted work without
        // acquiring its collection publication locks before verified shutdown.
        require_preflight_compatible(root, intent, &state)?;
    }
    Ok(())
}

fn require_compatible(root: &Path, intent: &Intent, state: &CheckpointSnapshot) -> Result<()> {
    let (predecessor, candidate) = compatibility_engines(root, intent)?;
    state.require_compatible(predecessor, candidate)
}

fn require_preflight_compatible(
    root: &Path,
    intent: &Intent,
    state: &store::PreflightSnapshot,
) -> Result<()> {
    let (predecessor, candidate) = compatibility_engines(root, intent)?;
    state.require_compatible(predecessor, candidate)
}

fn compatibility_engines(
    root: &Path,
    intent: &Intent,
) -> Result<(
    Option<airk_core::identity::Sha256Digest>,
    airk_core::identity::Sha256Digest,
)> {
    let (main, approval) = verify_runtime(root, &intent.candidate)?;
    let candidate = runtime_pair::engine_identity_at_version(&main, &approval.version)?;
    let predecessor = intent
        .prior
        .as_ref()
        .map(|prior| {
            let (main, approval) = verify_runtime(root, &prior.active)?;
            runtime_pair::engine_identity_at_version(&main, &approval.version)
        })
        .transpose()?;
    Ok((predecessor, candidate))
}

fn manifest(
    root: &Path,
    intent: &Intent,
    journal: &Journal,
    state: CheckpointSnapshot,
) -> Result<Manifest> {
    Ok(Manifest {
        schema: 1,
        operation: journal.operation.clone(),
        install: root.canonicalize()?,
        control: intent.control.clone(),
        candidate: intent.candidate.clone(),
        predecessor: journal.predecessor.clone(),
        manager_definition_sha256: journal.manager_definition_sha256.clone(),
        captured_unix_ms: unix_ms()?,
        boundary: Boundary::CommittedStoresAfterVerifiedOwnerExit,
        native: None,
        state,
    })
}

/// The caller holds every canonical Quiescence lease and has stopped the saved
/// manager. This is a legacy committed-store adapter, not a live graceful drain.
pub(super) fn capture(root: &Path, intent: &Intent, _guard: &Quiescence) -> Result<()> {
    let Some(operation) = &intent.checkpoint else {
        return Ok(());
    };
    let mut envelope = read(root)?.context("upgrade journal missing")?;
    binding(&envelope.journal, root, intent)?;
    let path = root.join(name(operation)?);
    if exists(&path)? {
        let bytes = read_bounded(&path, MAX_CHECKPOINT)?;
        let saved: Manifest = serde_json::from_slice(&bytes)?;
        let state = store::snapshot_for_schema(&intent.control, saved.state.schema)?;
        ensure!(
            saved.operation == *operation
                && saved.install == root.canonicalize()?
                && saved.control == intent.control
                && saved.candidate == intent.candidate
                && saved.predecessor == envelope.journal.predecessor
                && saved.manager_definition_sha256 == envelope.journal.manager_definition_sha256
                && serde_json::to_vec(&saved.state)? == serde_json::to_vec(&state)?,
            "checkpoint state changed after immutable capture"
        );
        let sha256 = hash(&bytes);
        ensure!(
            envelope
                .journal
                .checkpoint_sha256
                .as_ref()
                .is_none_or(|saved| saved == &sha256),
            "checkpoint digest changed"
        );
        envelope.journal.checkpoint_sha256 = Some(sha256);
    } else {
        let state = store::snapshot(&intent.control)?;
        require_compatible(root, intent, &state)?;
        ensure!(
            envelope.journal.stage == Stage::Quiescing
                && envelope.journal.checkpoint_sha256.is_none(),
            "checkpoint manifest disappeared"
        );
        let mut manifest = manifest(root, intent, &envelope.journal, state)?;
        if checkpoint_capable(root, intent)? || intent.prior.is_none() {
            manifest.boundary = Boundary::NoRunningServiceCommittedStores;
        }
        let bytes = serde_json::to_vec(&manifest)?;
        ensure!(
            bytes.len() as u64 <= MAX_MANIFEST_PACKET,
            "checkpoint_limit: complete roster exceeds manifest capacity"
        );
        let mut temporary = tempfile::NamedTempFile::new_in(root)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(&path)?;
        sync_directory(root)?;
        envelope.journal.checkpoint_sha256 = Some(hash(&bytes));
    }
    if envelope.journal.stage == Stage::Quiescing {
        envelope.journal.revision = envelope
            .journal
            .revision
            .checked_add(1)
            .context("upgrade revision overflow")?;
        envelope.journal.previous_digest = Some(envelope.sha256.clone());
        save(root, envelope.journal, Some(&envelope.sha256))?;
        stage(root, intent, Stage::CheckpointReady)?;
    }
    Ok(())
}

pub(super) fn verify(root: &Path, intent: &Intent) -> Result<()> {
    if intent.checkpoint.is_none() {
        return Ok(());
    }
    let envelope = read(root)?.context("upgrade journal missing")?;
    binding(&envelope.journal, root, intent)?;
    let bytes = read_bounded(
        &root.join(name(&envelope.journal.operation)?),
        MAX_CHECKPOINT,
    )?;
    ensure!(
        Some(hash(&bytes)).as_ref() == envelope.journal.checkpoint_sha256.as_ref(),
        "immutable checkpoint digest changed"
    );
    let saved: Manifest = serde_json::from_slice(&bytes)?;
    ensure!(
        saved.schema == 1
            && saved.operation == envelope.journal.operation
            && saved.install == root.canonicalize()?
            && saved.control == intent.control
            && saved.candidate == intent.candidate
            && saved.predecessor == envelope.journal.predecessor,
        "checkpoint activation binding changed"
    );
    if let Some(native) = &saved.native {
        native.validate()?;
        ensure!(
            saved.boundary == Boundary::GracefulNativeCommit
                && native.operation_id.to_string() == saved.operation,
            "checkpoint native boundary crossed"
        );
        let packet = service_runtime::checkpoint::read_packet(
            &intent.control,
            native.operation_id,
            native
                .native_packet_sha256
                .context("native checkpoint packet digest absent")?,
        )?;
        ensure!(
            packet.state.digest()? == saved.state.digest()?
                && native.state_sha256 == Some(saved.state.digest()?),
            "checkpoint native state reference changed"
        );
    } else {
        ensure!(
            saved.boundary != Boundary::GracefulNativeCommit,
            "graceful checkpoint lacks native proof"
        );
    }
    let state = store::snapshot_for_schema(&intent.control, saved.state.schema)?;
    require_compatible(root, intent, &state)?;
    ensure!(
        serde_json::to_vec(&saved.state)? == serde_json::to_vec(&state)?,
        "checkpoint canonical state changed before candidate recovery"
    );
    Ok(())
}

fn checkpoint_capable(root: &Path, intent: &Intent) -> Result<bool> {
    let Some(prior) = &intent.prior else {
        return Ok(false);
    };
    let (main, approval) = verify_runtime(root, &prior.active)?;
    let version = approval
        .version
        .split('.')
        .map(str::parse::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(version.len() == 3, "invalid predecessor version");
    Ok(version.as_slice() >= [1, 8, 14].as_slice() && runtime_pair::single_executable(&main)?)
}

pub(super) fn predecessor_checkpoint_capable(root: &Path, intent: &Intent) -> Result<bool> {
    checkpoint_capable(root, intent)
}

pub(super) fn reattach_legacy_rollback(root: &Path, intent: &mut Intent) -> Result<()> {
    if intent.checkpoint.is_some() || intent.phase != Phase::Rollback {
        return Ok(());
    }
    let Some(envelope) = read(root)? else {
        return Ok(());
    };
    if matches!(envelope.journal.stage, Stage::Committed | Stage::Retiring) {
        return Ok(());
    }
    ensure!(
        envelope.journal.install == root.canonicalize()?
            && envelope.journal.control == intent.control
            && envelope.journal.candidate == intent.candidate
            && envelope.journal.predecessor
                == intent.prior.as_ref().map(|prior| prior.active.clone())
            && envelope.journal.manager_owned == intent.manager,
        "legacy rollback crossed active checkpoint ownership"
    );
    intent.checkpoint = Some(envelope.journal.operation.clone());
    binding(&envelope.journal, root, intent)
}

/// New predecessors acknowledge actual durable work and native model completion
/// while still alive. Old versions use only the explicitly named exit adapter.
pub(super) async fn prepare_graceful(root: &Path, intent: &Intent) -> Result<()> {
    let Some(operation) = &intent.checkpoint else {
        return Ok(());
    };
    if !checkpoint_capable(root, intent)? {
        return Ok(());
    }
    let saved = read(root)?.context("upgrade journal missing")?;
    binding(&saved.journal, root, intent)?;
    if saved.journal.checkpoint_sha256.is_some() {
        return verify(root, intent);
    }
    // Native Ready may have committed before the temporary installer exited.
    // Recover its original owner/timestamp even if that daemon has since exited.
    let operation_id = operation.parse()?;
    let packet_path = intent
        .control
        .join(service_runtime::checkpoint::packet_name(operation_id));
    if exists(&packet_path)? {
        let digest = service_runtime::checkpoint::packet_digest(&intent.control, operation_id)?;
        let packet =
            service_runtime::checkpoint::read_packet(&intent.control, operation_id, digest)?;
        let prior = intent
            .prior
            .as_ref()
            .context("checkpoint predecessor missing")?;
        let (preceding, _) = verify_runtime(root, &prior.active)?;
        let (candidate, _) = verify_runtime(root, &intent.candidate)?;
        ensure!(
            packet.execution_sha256.to_string() == runtime_pair::digest_file(&preceding)?
                && packet.candidate_execution_sha256.to_string()
                    == runtime_pair::digest_file(&candidate)?,
            "saved native checkpoint changed approved image bindings"
        );
        let observation = airk_core::service_checkpoint::UpgradeCheckpointObservation {
            schema: 1,
            operation_id,
            service_instance: packet.service_instance,
            execution_sha256: packet.execution_sha256,
            candidate_execution_sha256: packet.candidate_execution_sha256,
            state: airk_core::service_checkpoint::CheckpointState::Ready,
            pending_operations: Some(0),
            sealed_workers: packet.workers.len(),
            state_sha256: Some(packet.state.digest()?),
            native_packet_sha256: Some(digest),
            provider: Some(packet.provider),
            blocked_code: None,
            blocked_details: None,
        };
        observation.validate()?;
        if observation.blocked_code.as_deref() == Some("native_source_checkpoint_barrier_failed") {
            bail!(
                "native_source_checkpoint_barrier_failed: {}; predecessor remains fenced; resume or abort the same activation",
                observation
                    .blocked_details
                    .as_deref()
                    .unwrap_or("native source barrier did not complete")
            );
        }
        return capture_native(root, intent, observation);
    }
    if let Ok(lease) = airk_store::service::ServiceLease::acquire(&intent.control) {
        drop(lease);
        // No running owner can acknowledge accepted work. Final capture will
        // refuse incompatible unfinished work rather than manufacture an ACK.
        let state = store::snapshot(&intent.control)?;
        return require_compatible(root, intent, &state);
    }
    let prior = intent
        .prior
        .as_ref()
        .context("checkpoint predecessor missing")?;
    let (main, approval) = verify_runtime(root, &prior.active)?;
    let observed = health_identity(root, &approval, &runtime_pair::digest_file(&main)?).await?;
    let (candidate, _) = verify_runtime(root, &intent.candidate)?;
    let candidate_sha = runtime_pair::digest_file(&candidate)?.parse()?;
    let operation = operation.parse()?;
    let observation = prepare_native_checkpoint(
        &intent.control,
        operation,
        &approval.version,
        &observed,
        candidate_sha,
    )
    .await?;
    capture_native(root, intent, observation)
}

/// Authenticated native checkpoint exchange shared by installer maintenance.
/// The caller owns the accepted journal and verifies the approved executable;
/// this helper never invents an activation or changes its operation identity.
pub(crate) async fn prepare_native_checkpoint(
    control: &Path,
    operation: airk_core::identity::Incarnation,
    version: &str,
    observed: &ServiceHealth,
    candidate_sha: airk_core::identity::Sha256Digest,
) -> Result<airk_core::service_checkpoint::UpgradeCheckpointObservation> {
    let until = tokio::time::Instant::now() + Duration::from_secs(180);
    let mut method = ControlMethod::PrepareUpgradeCheckpoint {
        operation_id: operation,
        candidate_execution_sha256: candidate_sha,
    };
    let mut latest = None;
    let mut last_stage = None;
    let mut diagnostic_sequence = 0usize;
    loop {
        let response = tokio::time::timeout_at(
            until,
            service_runtime::request_for_version_and_instance(
                control,
                method,
                version,
                observed.instance,
            ),
        )
        .await
        .map_err(|_| checkpoint_wait_failure("native_checkpoint_pending", latest.clone()))?
        .with_context(|| {
            checkpoint_wait_failure("native_checkpoint_observation_failed", latest.clone())
        })?;
        let ControlResponse::UpgradeCheckpoint { observation } = response else {
            return Err(checkpoint_wait_failure(
                "native_checkpoint_response_unavailable",
                latest,
            ));
        };
        observation.validate().with_context(|| {
            checkpoint_wait_failure("native_checkpoint_response_invalid", latest.clone())
        })?;
        if !(observation.operation_id == operation
            && observation.service_instance == observed.instance
            && observation.execution_sha256.to_string() == observed.executable_sha256
            && observation.candidate_execution_sha256 == candidate_sha)
        {
            return Err(checkpoint_wait_failure(
                "native_checkpoint_response_crossed",
                latest,
            ));
        }
        let stage = (
            observation.state,
            observation.blocked_code.clone(),
            observation.blocked_details.clone(),
        );
        if last_stage.as_ref() != Some(&stage) {
            if let Some(directory) = crate::process_diagnostics::directory() {
                let _ = checkpoint_diagnostic(&directory, diagnostic_sequence, &observation);
            }
            diagnostic_sequence = diagnostic_sequence.saturating_add(1);
            last_stage = Some(stage);
        }
        latest = Some(observation.clone());
        if observation.blocked_code.as_deref() == Some("native_source_checkpoint_barrier_failed") {
            return Err(checkpoint_wait_failure(
                "native_source_checkpoint_barrier_failed",
                latest,
            ));
        }
        if observation.state == airk_core::service_checkpoint::CheckpointState::Ready {
            return Ok(*observation);
        }
        if tokio::time::Instant::now() >= until {
            return Err(checkpoint_wait_failure("native_checkpoint_pending", latest));
        }
        tokio::time::sleep_until(
            until.min(tokio::time::Instant::now() + Duration::from_millis(250)),
        )
        .await;
        method = ControlMethod::ObserveUpgradeCheckpoint {
            operation_id: operation,
        };
    }
}

fn checkpoint_wait_failure(
    code: &'static str,
    observation: Option<Box<airk_core::service_checkpoint::UpgradeCheckpointObservation>>,
) -> anyhow::Error {
    anyhow::anyhow!(
        "{}",
        serde_json::json!({
            "code":code,"observation":observation,
            "guidance":"Inspect and resume or abort the same accepted operation; no checkpoint success, release of admission, or native exit is inferred."
        })
    )
}

fn checkpoint_diagnostic(
    directory: &Path,
    sequence: usize,
    observation: &airk_core::service_checkpoint::UpgradeCheckpointObservation,
) -> Result<()> {
    // Opt-in diagnostics only. Never alter operation admission or discard its
    // complete final refusal if this bounded auxiliary sink cannot be written.
    ensure!(sequence < 32, "checkpoint diagnostic stage bound reached");
    observation.validate()?;
    let bytes = serde_json::to_vec(observation)?;
    ensure!(
        bytes.len() as u64 <= MAX_CHECKPOINT,
        "checkpoint diagnostic exceeds bound"
    );
    fs::create_dir_all(directory)?;
    let path = directory.join(format!(
        "checkpoint-{}-{}-{sequence}.json",
        observation.service_instance, observation.operation_id
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    options.open(path)?.write_all(&bytes)?;
    Ok(())
}

fn capture_native(
    root: &Path,
    intent: &Intent,
    observation: airk_core::service_checkpoint::UpgradeCheckpointObservation,
) -> Result<()> {
    let mut envelope = read(root)?.context("upgrade journal missing")?;
    binding(&envelope.journal, root, intent)?;
    let (target, _) = verify_runtime(root, &intent.candidate)?;
    require_target(&intent.control, &target)?;
    ensure!(
        envelope.journal.stage == Stage::Quiescing,
        "native checkpoint journal phase changed"
    );
    let packet = service_runtime::checkpoint::read_packet(
        &intent.control,
        observation.operation_id,
        observation
            .native_packet_sha256
            .context("native checkpoint packet digest absent")?,
    )?;
    ensure!(
        observation.state_sha256 == Some(packet.state.digest()?)
            && packet.service_instance == observation.service_instance
            && packet.candidate_execution_sha256 == observation.candidate_execution_sha256,
        "native checkpoint packet crossed response"
    );
    require_compatible(root, intent, &packet.state)?;
    let mut manifest = manifest(root, intent, &envelope.journal, packet.state)?;
    manifest.captured_unix_ms = packet.captured_unix_ms;
    manifest.boundary = Boundary::GracefulNativeCommit;
    manifest.native = Some(observation);
    let path = root.join(name(&envelope.journal.operation)?);
    if exists(&path)? {
        let saved: Manifest = serde_json::from_slice(&read_bounded(&path, MAX_CHECKPOINT)?)?;
        manifest.captured_unix_ms = saved.captured_unix_ms;
        ensure!(
            serde_json::to_vec(&saved)? == serde_json::to_vec(&manifest)?,
            "immutable native checkpoint changed"
        );
    } else {
        let data = serde_json::to_vec(&manifest)?;
        ensure!(
            data.len() as u64 <= MAX_MANIFEST_PACKET,
            "checkpoint_limit: complete native checkpoint exceeds manifest capacity; service remains alive and fenced"
        );
        let mut temporary = tempfile::NamedTempFile::new_in(root)?;
        temporary.write_all(&data)?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(&path)?;
        sync_directory(root)?;
    }
    envelope.journal.checkpoint_sha256 = Some(runtime_pair::digest_file(&path)?);
    envelope.journal.revision = envelope
        .journal
        .revision
        .checked_add(1)
        .context("upgrade revision overflow")?;
    envelope.journal.previous_digest = Some(envelope.sha256.clone());
    save(root, envelope.journal, Some(&envelope.sha256))?;
    stage(root, intent, Stage::CheckpointReady)
}

pub(super) async fn settle_admission(root: &Path, intent: &Intent) -> Result<()> {
    if let Some(operation) = &intent.checkpoint {
        let operation = operation.parse()?;
        if let Some(selected) = selection(root)? {
            let (main, approval) = verify_runtime(root, &selected.active)?;
            let version = approval
                .version
                .split('.')
                .map(str::parse::<u32>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if version.len() == 3 && version.as_slice() >= [1, 8, 14].as_slice() {
                match health_identity(root, &approval, &runtime_pair::digest_file(&main)?).await {
                    Ok(owner) => {
                        let response = service_runtime::request_for_version_and_instance(
                            &intent.control,
                            ControlMethod::ReleaseUpgradeAdmission {
                                operation_id: operation,
                            },
                            &approval.version,
                            owner.instance,
                        )
                        .await?;
                        ensure!(
                            matches!(response,ControlResponse::UpgradeAdmissionReleased {operation_id} if operation_id==operation),
                            "native admission release remains pending"
                        );
                        return Ok(());
                    }
                    Err(error) => {
                        let exclusion = airk_store::service::ServiceLease::acquire(&intent.control)
                            .context(error)?;
                        service_runtime::checkpoint::settle(&intent.control, operation)?;
                        drop(exclusion);
                        return Ok(());
                    }
                }
            }
        }
        service_runtime::checkpoint::settle(&intent.control, operation)?;
    }
    Ok(())
}

pub(super) fn verify_release(
    root: &Path,
    control: &Path,
    operation: airk_core::identity::Incarnation,
    selected_inventory: &str,
) -> Result<()> {
    let envelope = read(root)?.context("admission release journal missing")?;
    let journal = &envelope.journal;
    ensure!(
        journal.operation == operation.to_string()
            && journal.control == control
            && selection_record(root)?.active == selected_inventory,
        "admission release selection or operation changed"
    );
    let expected = match journal.stage {
        Stage::Committed | Stage::Retiring => Some(journal.candidate.as_str()),
        Stage::Aborted => journal.predecessor.as_deref(),
        _ => bail!("admission release journal is not terminal"),
    };
    ensure!(
        expected == Some(selected_inventory),
        "admission release target differs from committed outcome"
    );
    if let Some(expected) = &journal.checkpoint_sha256 {
        let data = read_bounded(&root.join(name(&journal.operation)?), MAX_CHECKPOINT)?;
        ensure!(
            hash(&data) == *expected,
            "admission release checkpoint digest changed"
        );
        let _saved: Manifest = serde_json::from_slice(&data)?;
    }
    Ok(())
}

pub(super) fn require_target(control: &Path, main: &Path) -> Result<()> {
    // A claimed fresh installation has no control directory yet. Validate its
    // retained layout first: a lost initialized directory must never become a
    // fresh-install exemption from the target format check.
    crate::install_layout::validate_control(control)?;
    match fs::symlink_metadata(control) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let required = airk_store::service::registry::runtime_contract::required_capabilities(control)?;
    if required.minimum_registry_schema == 0 {
        return Ok(());
    }
    let observation = self_test_observation(main)?;
    let capabilities:airk_core::index_catalog::CatalogCapabilities=serde_json::from_value(observation.get("index_catalog").cloned().context("target runtime does not declare native registry capabilities; retain the current selection and use a qualified intermediate version")?)?;
    required.require_supported(&capabilities)
}

pub(super) fn arm_recovery(root: &Path, intent: &Intent, _guard: &Quiescence) -> Result<()> {
    if let Some(operation) = &intent.checkpoint {
        let (main, _) = verify_runtime(root, &intent.candidate)?;
        service_runtime::checkpoint::arm_recovery(
            &intent.control,
            operation.parse()?,
            runtime_pair::digest_file(&main)?.parse()?,
        )?;
    }
    Ok(())
}

pub(super) fn expected_namespace(
    root: &Path,
    intent: &Intent,
) -> Result<Option<airk_core::identity::Incarnation>> {
    let Some(operation) = &intent.checkpoint else {
        return Ok(None);
    };
    let envelope = read(root)?.context("upgrade journal missing")?;
    binding(&envelope.journal, root, intent)?;
    let bytes = read_bounded(&root.join(name(operation)?), MAX_CHECKPOINT)?;
    ensure!(
        envelope.journal.checkpoint_sha256.as_ref() == Some(&hash(&bytes)),
        "checkpoint digest changed before recovery readiness"
    );
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    Ok(manifest.state.namespace)
}

/// Complete saved metadata only. Status does not open indexes, start workers,
/// or reinterpret a historical completion observation as a new native witness.
pub(super) fn status(root: &Path) -> Result<Option<serde_json::Value>> {
    let Some(envelope) = read(root)? else {
        return Ok(None);
    };
    let path = root.join(name(&envelope.journal.operation)?);
    let manifest = if exists(&path)? {
        let data = read_bounded(&path, MAX_CHECKPOINT)?;
        if let Some(expected) = &envelope.journal.checkpoint_sha256 {
            ensure!(
                hash(&data) == *expected,
                "checkpoint status immutable digest changed"
            );
        }
        let manifest: Manifest = serde_json::from_slice(&data)?;
        ensure!(
            manifest.schema == 1
                && manifest.operation == envelope.journal.operation
                && manifest.install == root.canonicalize()?
                && manifest.control == envelope.journal.control
                && manifest.candidate == envelope.journal.candidate
                && manifest.predecessor == envelope.journal.predecessor
                && manifest.manager_definition_sha256 == envelope.journal.manager_definition_sha256,
            "checkpoint status ownership changed"
        );
        manifest.state.digest()?;
        if let Some(native) = &manifest.native {
            native.validate()?;
        }
        Some(manifest)
    } else {
        ensure!(
            envelope.journal.checkpoint_sha256.is_none(),
            "checkpoint status manifest missing"
        );
        None
    };
    let value = serde_json::json!({"schema":1,"saved_only":true,"live_state_verified":false,"native_packet_live_verified":false,"complete":true,"journal":envelope,"manifest":manifest});
    ensure!(
        serde_json::to_vec(&value)?.len() as u64 <= MAX_CHECKPOINT,
        "checkpoint_status_complete_metadata_limit"
    );
    Ok(Some(value))
}

pub(super) fn retiring(root: &Path, candidate: &str) -> Result<()> {
    let Some(envelope) = read(root)? else {
        return Ok(());
    };
    if envelope.journal.candidate != candidate {
        return Ok(());
    }
    ensure!(
        matches!(envelope.journal.stage, Stage::Committed | Stage::Retiring),
        "checkpoint journal is not committed for retirement"
    );
    ensure!(
        selection_record(root)?.active == candidate,
        "retirement candidate is no longer selected"
    );
    if envelope.journal.stage == Stage::Retiring {
        return Ok(());
    }
    let mut journal = envelope.journal;
    journal.stage = Stage::Retiring;
    journal.revision = journal
        .revision
        .checked_add(1)
        .context("upgrade revision overflow")?;
    journal.previous_digest = Some(envelope.sha256.clone());
    journal.observed_unix_ms = unix_ms()?;
    save(root, journal, Some(&envelope.sha256))
}

#[cfg(test)]
mod tests;
