// SPDX-License-Identifier: Apache-2.0
//! Actual kernel-owner restart and production project-worker RunJob gates.
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{JobKind, JobOutcome, JobState, ProjectRegistration, WorkerIdentity},
    service_resources::WorkAllowance,
};
use airk_store::service::{
    ProjectLease, data_scope,
    registry::{AuditEvidence, IntegrityAuditState, ServiceRegistry},
};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
/// Reads at most4096input bytes into fixed scratch; UTF8 display may expand.
fn stderr_tail(path: &Path) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let result = (|| -> std::io::Result<String> {
        let mut file = fs::File::open(path)?;
        let length = file.metadata()?.len();
        file.seek(SeekFrom::Start(length.saturating_sub(4096)))?;
        let mut bytes = [0u8; 4096];
        let mut used = 0;
        while used < bytes.len() {
            let count = file.read(&mut bytes[used..])?;
            if count == 0 {
                break;
            }
            used += count;
        }
        Ok(String::from_utf8_lossy(&bytes[..used]).into_owned())
    })();
    result.unwrap_or_else(|_| "stderr unavailable".into())
}
const MARKER: &[u8] = b"airk-owned-f11-process-fixture-v1\n";
fn namespace() -> Incarnation {
    Incarnation::from_bytes([121; 16])
}
fn registration(root: &Path) -> Result<ProjectRegistration> {
    ProjectRegistration::new(
        root.join("source").to_str().context("source utf8")?.into(),
        root.join("data").to_str().context("data utf8")?.into(),
        "f11-native-restart".into(),
    )
}
fn allowance(identity: WorkerIdentity, job: Sha256Digest) -> Result<WorkAllowance> {
    Ok(WorkAllowance {
        id: airk_kernel::service_broker::new_instance_id()?,
        identity,
        job,
        extraction_threads: 1,
        inference_threads: 1,
        working_memory_bytes: 256 << 20,
        background_io_bytes_per_second: 8 << 20,
        ttl_ms: 120000,
    })
}
fn event(
    stage: &str,
    job: Sha256Digest,
    progress: Option<&airk_store::service::collection::integrity::verify::VerifyProgress>,
) -> Result<()> {
    let value = progress.map(serde_json::to_value).transpose()?;
    let mut stdout = std::io::stdout().lock();
    writeln!(
        stdout,
        "{}",
        serde_json::json!({"stage":stage,"job":job,"checked":value.as_ref().and_then(|v|v["checked_files"].as_u64()),"started":value.as_ref().and_then(|v|v["started_unix_ms"].as_u64()),"lexical":value.as_ref().and_then(|v|v["lexical_observed_unix_ms"].as_u64())})
    )?;
    stdout.flush()?;
    Ok(())
}
#[test]
#[ignore = "owned subprocess helper; invoked only by durable_verify_actual_process_restart"]
fn integrity_owner_process() -> Result<()> {
    let root =
        PathBuf::from(std::env::var_os("AIRK_F11_OWNED_ROOT").context("owned root required")?)
            .canonicalize()?;
    ensure!(
        fs::read(root.join("fixture-owned"))? == MARKER,
        "foreign fixture root"
    );
    let mode = std::env::var("AIRK_F11_MODE")?;
    let project = registration(&root)?;
    let control = root.join("control");
    let mut registry = ServiceRegistry::open(&control, namespace())?;
    if mode == "setup" {
        registry.register(&project)?;
    }
    let bound = data_scope::bind(Path::new(&project.data_root), &control, namespace())?;
    let mut lease = ProjectLease::acquire(&control, project.binding)?;
    registry.admit_worker(&mut lease)?;
    let identity = WorkerIdentity {
        service: namespace(),
        worker: airk_kernel::service_broker::new_instance_id()?,
        project: project.binding,
        fencing_generation: lease.identity().1.context("admitted epoch")?,
    };
    airk_kernel::service_execution::install(Path::new(&project.data_root), identity)?;
    let kernel = airk_kernel::Kernel::open_service_collection_for_diagnostics(
        Path::new(&project.root),
        Some(project.data_root.clone().into()),
        &project.collection,
    )?;
    if mode == "setup" {
        let indexed = registry.enqueue_internal(
            project.binding,
            namespace(),
            JobKind::Index,
            airk_kernel::service_broker::new_instance_id()?,
        )?;
        registry
            .claim_job_id(&lease, indexed.id)?
            .context("source claim")?;
        let grant = allowance(identity, indexed.id)?;
        airk_kernel::service_execution::activate(&grant)?;
        let report = airk_kernel::service_execution::run(grant, || kernel.index_service_sources())?;
        ensure!(
            report.indexed == 65,
            "actual source adapter did not index65files"
        );
        registry.complete_job(&lease, indexed.id, JobOutcome::Succeeded { revision: None })?;
        let audit = registry.enqueue_internal(
            project.binding,
            namespace(),
            JobKind::Verify,
            airk_kernel::service_broker::new_instance_id()?,
        )?;
        event("setup", audit.id, None)?;
        return Ok(());
    }
    if mode == "resume" {
        registry.recover_jobs(&lease)?;
    }
    let job = registry
        .next_pending_job(project.binding)?
        .context("same durable Verify pending")?;
    ensure!(job.kind == JobKind::Verify, "unexpected durable job");
    let job = registry
        .claim_job_id(&lease, job.id)?
        .context("actual Verify claim")?;
    let grant = allowance(identity, job.id)?;
    airk_kernel::service_execution::activate(&grant)?;
    if mode == "first" {
        let turn = airk_kernel::service_execution::run(grant, || {
            kernel.verify_service_turn(&registry, &lease, &bound, job.id)
        })?;
        ensure!(
            turn.verified_files == 32 && !turn.progress.complete() && turn.completed.is_none(),
            "first actual bounded turn differs"
        );
        ensure!(
            registry.integrity_audit_state(&lease)?
                == IntegrityAuditState::LiveAudit { job: job.id },
            "fresh admitted audit not live"
        );
        event("first_persisted", job.id, Some(&turn.progress))?;
        // Real process retains actual lease; parent kills only after durable turn.
        let mut byte = [0u8; 1];
        std::io::Read::read_exact(&mut std::io::stdin().lock(), &mut byte)?;
        anyhow::bail!("fixture expected forced process death, not graceful completion");
    }
    ensure!(mode == "resume", "unknown owned mode");
    ensure!(
        registry.integrity_audit_state(&lease)? == IntegrityAuditState::Unresolved { job: job.id },
        "unclean inherited guard must be unresolved"
    );
    ensure!(
        registry
            .complete_job(&lease, job.id, JobOutcome::Succeeded { revision: None })
            .is_err(),
        "generic success cleared physical guard"
    );
    let mut final_proof = None;
    airk_kernel::service_execution::run(grant, || -> Result<()> {
        for turn_index in 0..3 {
            let turn = kernel.verify_service_turn(&registry, &lease, &bound, job.id)?;
            if turn_index == 0 {
                ensure!(
                    turn.verified_files == 32,
                    "resume reset/repeated the persisted page"
                );
                event("resumed", job.id, Some(&turn.progress))?;
            }
            if turn.progress.complete() {
                event("complete", job.id, Some(&turn.progress))?;
                final_proof = turn.completed;
                break;
            }
        }
        Ok(())
    })?;
    let proof = final_proof.context("real resumed audit lacks sealed completion")?;
    registry.complete_job_with_maintenance_and_audit_at(
        &lease,
        job.id,
        JobOutcome::Succeeded {
            revision: Some(proof.revision()?),
        },
        1000,
        None,
        Some(AuditEvidence::Complete(&proof)),
    )?;
    ensure!(
        registry.job(job.id)?.state == JobState::Succeeded
            && registry.integrity_audit_state(&lease)? == IntegrityAuditState::Clear,
        "actual completion did not settle guard"
    );
    event("settled", job.id, None)?;
    Ok(())
}
struct OwnedProcess {
    child: Child,
    input: Option<ChildStdin>,
    events: mpsc::Receiver<String>,
    stderr: PathBuf,
    reader: Option<thread::JoinHandle<()>>,
}
impl OwnedProcess {
    fn start(root: &Path, mode: &str) -> Result<Self> {
        let stderr_path = root.join(format!("{mode}.stderr"));
        let stderr = fs::File::create(&stderr_path)?;
        let mut child = Command::new(std::env::current_exe()?)
            .args([
                "--ignored",
                "--exact",
                "integrity_owner_process",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("AIRK_F11_OWNED_ROOT", root)
            .env("AIRK_F11_MODE", mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr))
            .spawn()?;
        let input = child.stdin.take();
        let output = child.stdout.take().context("child stdout")?;
        let (send, events) = mpsc::sync_channel(16);
        let reader = thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if let Some(start) = line.find('{')
                    && send.try_send(line[start..].to_owned()).is_err()
                {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            events,
            stderr: stderr_path,
            reader: Some(reader),
        })
    }
    fn event(&self, stage: &str) -> Result<serde_json::Value> {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .context("owned event deadline")?;
            let value: serde_json::Value =
                serde_json::from_str(&self.events.recv_timeout(remaining).with_context(|| {
                    format!(
                        "owned helper event {stage} failed; stderr_tail={}",
                        stderr_tail(&self.stderr)
                    )
                })?)?;
            if value["stage"] == stage {
                return Ok(value);
            }
        }
    }
    fn kill_and_reap(&mut self) -> Result<()> {
        self.child.kill()?;
        self.child.wait()?;
        self.input.take();
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self.child.try_wait()? {
                ensure!(
                    status.success(),
                    "owned helper failed; stderr_tail={}",
                    stderr_tail(&self.stderr)
                );
                return Ok(());
            }
            ensure!(Instant::now() < deadline, "owned helper did not drain");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
#[test]
fn durable_verify_actual_process_restart() -> Result<()> {
    let owned = tempfile::tempdir()?;
    let root = owned.path().canonicalize()?;
    fs::write(root.join("fixture-owned"), MARKER)?;
    fs::create_dir(root.join("source"))?;
    fs::create_dir(root.join("control"))?;
    for ordinal in 0..65 {
        fs::write(
            root.join("source")
                .join(format!("fixture-{ordinal:03}.txt")),
            format!("inert retained fixture {ordinal}\n"),
        )?;
    }
    let mut setup = OwnedProcess::start(&root, "setup")?;
    let initial = setup.event("setup")?;
    setup.finish()?;
    drop(setup);
    let mut first = OwnedProcess::start(&root, "first")?;
    let before = first.event("first_persisted")?;
    assert_eq!(before["job"], initial["job"]);
    assert_eq!(before["checked"], 32);
    first.kill_and_reap()?;
    drop(first);
    let mut resume = OwnedProcess::start(&root, "resume")?;
    let after = resume.event("resumed")?;
    assert_eq!(after["job"], before["job"]);
    assert_eq!(after["checked"], 64);
    assert_eq!(after["started"], before["started"]);
    assert_eq!(after["lexical"], before["lexical"]);
    let complete = resume.event("complete")?;
    assert_eq!(complete["checked"], 65);
    assert_eq!(complete["started"], before["started"]);
    assert_eq!(complete["lexical"], before["lexical"]);
    resume.event("settled")?;
    resume.finish()?;
    for ordinal in 0..65 {
        assert_eq!(
            fs::read_to_string(
                root.join("source")
                    .join(format!("fixture-{ordinal:03}.txt"))
            )?,
            format!("inert retained fixture {ordinal}\n")
        );
    }
    Ok(())
}

struct NativeProtocolWorker {
    child: Child,
    input: Option<ChildStdin>,
    replies: mpsc::Receiver<airk_cli::project_protocol::Reply>,
    reader: Option<thread::JoinHandle<()>>,
    stderr: PathBuf,
}
impl Drop for NativeProtocolWorker {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
impl NativeProtocolWorker {
    fn start(root: &Path) -> Result<Self> {
        let stderr = root.join("project-worker.stderr");
        let log = fs::File::create(&stderr)?;
        let mut command = Command::new(env!("CARGO_BIN_EXE_airk"));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .arg("project-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log))
            .spawn()?;
        let input = child.stdin.take();
        let mut output = child.stdout.take().context("native worker stdout")?;
        let (send, replies) = mpsc::sync_channel(8);
        let reader = thread::spawn(move || {
            while let Ok(Some(reply)) = airk_cli::service_ipc::read_frame::<
                _,
                airk_cli::project_protocol::Reply,
            >(&mut output)
            {
                if send.try_send(reply).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            replies,
            reader: Some(reader),
            stderr,
        })
    }
    fn send(&mut self, value: &impl serde::Serialize) -> Result<()> {
        airk_cli::service_ipc::write_frame(
            self.input.as_mut().context("native worker input closed")?,
            value,
        )?;
        Ok(())
    }
    fn reply(&self) -> Result<airk_cli::project_protocol::Reply> {
        self.replies
            .recv_timeout(Duration::from_secs(15))
            .with_context(|| {
                format!(
                    "native worker reply failed; stderr_tail={}",
                    stderr_tail(&self.stderr)
                )
            })
    }
    fn stop(&mut self) -> Result<()> {
        self.send(&airk_cli::project_protocol::Command::Stop)?;
        self.input.take();
        self.finish()
    }
    fn assert_project_owned(&mut self, root: &Path, project: Sha256Digest) -> Result<()> {
        ensure!(
            self.child.try_wait()?.is_none(),
            "accepted work owner exited before ledger release"
        );
        let held = ProjectLease::acquire(&root.join("control"), project)
            .err()
            .context("worker released its accepted project lease")?;
        ensure!(
            matches!(
                held.downcast_ref::<std::fs::TryLockError>(),
                Some(std::fs::TryLockError::WouldBlock)
            ),
            "unexpected lease refusal: {held:#}"
        );
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self.child.try_wait()? {
                ensure!(status.success(), "native worker failed clean drain");
                return Ok(());
            }
            ensure!(Instant::now() < deadline, "native worker drain timed out");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
#[test]
fn native_project_worker_runjob_consumes_real_audit_completion() -> Result<()> {
    use airk_cli::project_protocol::{self, Command as Control, Reply};
    let owned = tempfile::tempdir()?;
    let root = owned.path().canonicalize()?;
    fs::write(root.join("fixture-owned"), MARKER)?;
    fs::create_dir(root.join("source"))?;
    airk_cli::local_ipc::prepare_root(&root.join("control"))?;
    for ordinal in 0..65 {
        fs::write(
            root.join("source")
                .join(format!("fixture-{ordinal:03}.txt")),
            format!("native dispatched retained input {ordinal}\n"),
        )?;
    }
    let mut setup = OwnedProcess::start(&root, "setup")?;
    let ready = setup.event("setup")?;
    setup.finish()?;
    drop(setup);
    let job: Sha256Digest = serde_json::from_value(ready["job"].clone())?;
    let project = registration(&root)?;
    let mut worker = NativeProtocolWorker::start(&root)?;
    worker.send(&project_protocol::Init {
        engine: Sha256Digest::from_bytes([0; 32]),
        runtime: root.join("control"),
        project: project.binding,
        service: namespace(),
        worker: airk_kernel::service_broker::new_instance_id()?,
    })?;
    let Reply::Ready { identity, .. } = worker.reply()? else {
        anyhow::bail!("native worker Ready missing")
    };
    worker.send(&Control::Activate {
        identity,
        allowance: airk_core::service::QueryAllowance {
            concurrent_queries: 1,
            response_bytes: airk_core::service::MAX_FRAME_BYTES,
        },
        pool: None,
        pool_pending: false,
    })?;
    ensure!(
        matches!(worker.reply()?, Reply::Activated),
        "native activation refused"
    );
    let grant = allowance(identity, job)?;
    let grant_id = grant.id;
    worker.send(&Control::RunJob {
        identity,
        allowance: grant,
    })?;
    ensure!(
        matches!(worker.reply()?, Reply::WorkAccepted),
        "native RunJob refused actual audit"
    );
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        ensure!(
            Instant::now() < deadline,
            "native Verify completion deadline"
        );
        worker.send(&Control::WorkStatus {
            identity,
            control: None,
            keep_for_maintenance: false,
            freshness: None,
        })?;
        let Reply::WorkStatus { observation } = worker.reply()? else {
            anyhow::bail!("native work observation missing")
        };
        let wire = serde_json::to_value(&observation)?;
        ensure!(
            wire.get("failure").is_none(),
            "native Verify reported a work failure: {}",
            wire["failure"]
        );
        if observation.finished == Some(grant_id) {
            let progress = observation
                .progress
                .context("actual verified delta missing")?;
            ensure!(
                // The existing post-success settings inspection contributes
                // one verification event in addition to the65physical files.
                progress.verified_objects == 66,
                "actual worker aggregate must include65files plus1inspection: got {}; progress={progress:?}",
                progress.verified_objects
            );
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    let registry = ServiceRegistry::open_saved(&root.join("control"))?;
    let completed = registry.job(job)?;
    let durable = durable_verify_state(&root, &project)?;
    ensure!(
        durable["verify"]["job"] == serde_json::to_value(job)?,
        "durable Verify job changed"
    );
    ensure!(
        durable["verify"]["checked_files"] == 65,
        "durable physical file count differs"
    );
    let physical: airk_store::service::collection::integrity::verify::VerifyProgress =
        serde_json::from_value(durable["verify"].clone())?;
    ensure!(
        physical.complete(),
        "durable Verify is not physically complete"
    );
    let head: airk_store::current::CurrentHead = serde_json::from_value(durable["head"].clone())?;
    let token = head.token()?;
    let expected = airk_core::identity::RevisionToken {
        incarnation: token.incarnation,
        generation: token.sequence,
        revision: token.digest,
    };
    let collection = airk_store::service::collection::Collection::existing(
        Path::new(&project.data_root),
        &project.root,
        &project.collection,
    )?;
    let selected = collection.selected_store()?;
    let scope = selected
        .current_scope(&project.root, &project.collection)?
        .context("current scope missing")?;
    ensure!(
        selected.current_read(&scope)?.token()? == token,
        "durable Verify head is not current"
    );
    ensure!(
        matches!(&completed.outcome, Some(JobOutcome::Succeeded { revision: Some(revision) }) if *revision == expected),
        "actual worker did not consume exact physical completion proof"
    );
    drop(registry);
    worker.stop()?;
    drop(worker);
    let mut registry = ServiceRegistry::open(&root.join("control"), namespace())?;
    let mut lease = ProjectLease::acquire(&root.join("control"), project.binding)?;
    registry.admit_worker(&mut lease)?;
    ensure!(
        registry.integrity_audit_state(&lease)? == IntegrityAuditState::Clear,
        "actual worker completion left stale physical guard"
    );
    for ordinal in 0..65 {
        assert_eq!(
            fs::read_to_string(
                root.join("source")
                    .join(format!("fixture-{ordinal:03}.txt"))
            )?,
            format!("native dispatched retained input {ordinal}\n")
        );
    }
    Ok(())
}

fn durable_verify_state(root: &Path, project: &ProjectRegistration) -> Result<serde_json::Value> {
    let data = Path::new(&project.data_root).canonicalize()?;
    ensure!(
        data.parent() == Some(root),
        "durable fixture data escaped owned root"
    );
    let path = data
        .join("collections-1.4")
        .join(&project.collection)
        .join("integrity-observation.json");
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(16385).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 16384,
        "durable Verify receipt exceeds metadata budget"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

#[test]
fn native_project_worker_accepted_verify_cancel_and_closed_input_retain_owned_drain() -> Result<()>
{
    use airk_cli::project_protocol::{self, Command as Control, Reply, WorkControl};
    for action in ["cancel", "eof", "stop"] {
        let owned = tempfile::tempdir()?;
        let root = owned.path().canonicalize()?;
        fs::write(root.join("fixture-owned"), MARKER)?;
        fs::create_dir(root.join("source"))?;
        airk_cli::local_ipc::prepare_root(&root.join("control"))?;
        let original = "accepted Verify retained input\n";
        for ordinal in 0..65 {
            fs::write(
                root.join("source")
                    .join(format!("fixture-{ordinal:03}.txt")),
                original,
            )?;
        }
        let mut setup = OwnedProcess::start(&root, "setup")?;
        let ready = setup.event("setup")?;
        setup.finish()?;
        drop(setup);
        let job: Sha256Digest = serde_json::from_value(ready["job"].clone())?;
        let project = registration(&root)?;
        let mut worker = NativeProtocolWorker::start(&root)?;
        worker.send(&project_protocol::Init {
            engine: Sha256Digest::from_bytes([0; 32]),
            runtime: root.join("control"),
            project: project.binding,
            service: namespace(),
            worker: airk_kernel::service_broker::new_instance_id()?,
        })?;
        let Reply::Ready { identity, .. } = worker.reply()? else {
            anyhow::bail!("accepted-drain worker Ready missing")
        };
        worker.send(&Control::Activate {
            identity,
            allowance: airk_core::service::QueryAllowance {
                concurrent_queries: 1,
                response_bytes: airk_core::service::MAX_FRAME_BYTES,
            },
            pool: None,
            pool_pending: false,
        })?;
        ensure!(
            matches!(worker.reply()?, Reply::Activated),
            "accepted-drain activation refused"
        );
        // Use the existing service quota lock only after all activation/setup.
        // Its real job admission reports NativeLock and checks WorkControl.
        let ledger = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join("data/.quota-1.4/ledger.lock"))?;
        ledger.try_lock()?;
        let grant = allowance(identity, job)?;
        let grant_id = grant.id;
        worker.send(&Control::RunJob {
            identity,
            allowance: grant,
        })?;
        ensure!(
            matches!(worker.reply()?, Reply::WorkAccepted),
            "accepted-drain RunJob refused"
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            ensure!(
                Instant::now() < deadline,
                "accepted Verify did not enter actual quota admission"
            );
            worker.send(&Control::WorkStatus {
                identity,
                control: None,
                keep_for_maintenance: false,
                freshness: None,
            })?;
            let Reply::WorkStatus { observation } = worker.reply()? else {
                anyhow::bail!("accepted-drain WorkStatus missing")
            };
            ensure!(
                observation.finished != Some(grant_id),
                "Verify finished while its actual ledger was held"
            );
            let registry = ServiceRegistry::open_saved(&root.join("control"))?;
            let record = registry.job(job)?;
            if record.state == JobState::Running
                && observation.waiting_for
                    == Some(airk_core::service_resources::WaitReason::NativeLock)
            {
                ensure!(
                    observation.active == Some(grant_id),
                    "waiting grant changed"
                );
                ensure!(
                    observation
                        .progress
                        .as_ref()
                        .is_some_and(|p| p.verified_objects == 0),
                    "blocked Verify invented physical work"
                );
                // Read the actual persisted veto through an independent read-only
                // handle; no production connection or mutation hook is exposed.
                let connection = rusqlite::Connection::open_with_flags(
                    root.join("control/service-1.4.sqlite3"),
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )?;
                let saved: String = connection.query_row(
                    "SELECT engine FROM service_recovery_budget WHERE scope=?1 AND failures=0 AND suppressed=1",
                    [format!("integrity:{}",project.binding)], |row| row.get(0))?;
                ensure!(
                    saved == job.to_string(),
                    "accepted Verify guard was not durably admitted"
                );
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        worker.assert_project_owned(&root, project.binding)?;
        if action == "cancel" {
            worker.send(&Control::WorkStatus {
                identity,
                control: Some(WorkControl {
                    grant: grant_id,
                    paused: false,
                    canceled: true,
                    yield_to_source: false,
                }),
                keep_for_maintenance: false,
                freshness: None,
            })?;
            ensure!(
                matches!(worker.reply()?, Reply::WorkStatus { .. }),
                "explicit cancellation was not observed"
            );
        }
        if action != "eof" {
            worker.send(&Control::Stop)?;
        }
        worker.input.take();
        // Explicit cancellation may already have safely completed. Only
        // uncanceled accepted EOF/Stop work must retain ownership until release.
        if action != "cancel" {
            worker.assert_project_owned(&root, project.binding)?;
        }
        ledger.unlock()?;
        drop(ledger);
        worker.finish()?;
        drop(worker);
        let registry = ServiceRegistry::open_saved(&root.join("control"))?;
        let completed = registry.job(job)?;
        let progress = registry
            .progress_observation(project.binding)?
            .context("drained progress missing")?
            .progress;
        if action == "cancel" {
            ensure!(
                matches!(completed.outcome, Some(JobOutcome::Canceled)),
                "explicitly canceled Verify did not remain Canceled"
            );
            ensure!(
                progress.verified_objects == 0,
                "canceled-before-physical Verify reported physical work"
            );
        } else {
            let durable = durable_verify_state(&root, &project)?;
            ensure!(
                durable["verify"]["checked_files"] == 65
                    && durable["verify"]["job"] == serde_json::to_value(job)?,
                "closed-input owner did not finish its exact65-file job"
            );
            let physical: airk_store::service::collection::integrity::verify::VerifyProgress =
                serde_json::from_value(durable["verify"].clone())?;
            ensure!(
                physical.complete(),
                "closed-input owner did not finish physical proof"
            );
            let head: airk_store::current::CurrentHead =
                serde_json::from_value(durable["head"].clone())?;
            let token = head.token()?;
            let expected = airk_core::identity::RevisionToken {
                incarnation: token.incarnation,
                generation: token.sequence,
                revision: token.digest,
            };
            ensure!(
                matches!(&completed.outcome, Some(JobOutcome::Succeeded { revision: Some(revision) }) if *revision==expected),
                "closed-input owner did not consume exact completion proof"
            );
            ensure!(
                progress.verified_objects == 66,
                "closed-input aggregate did not retain65files plus1inspection"
            );
        }
        let connection = rusqlite::Connection::open_with_flags(
            root.join("control/service-1.4.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let guarded: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_recovery_budget WHERE scope=?1)",
            [format!("integrity:{}", project.binding)],
            |row| row.get(0),
        )?;
        ensure!(!guarded, "clean fresh Verify drain retained an audit veto");
        drop(connection);
        drop(registry);
        drop(ProjectLease::acquire(
            &root.join("control"),
            project.binding,
        )?);
        for ordinal in 0..65 {
            ensure!(
                fs::read_to_string(
                    root.join("source")
                        .join(format!("fixture-{ordinal:03}.txt"))
                )? == original,
                "accepted Verify drain changed source bytes"
            );
        }
    }
    Ok(())
}
