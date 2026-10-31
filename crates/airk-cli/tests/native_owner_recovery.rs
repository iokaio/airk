// SPDX-License-Identifier: Apache-2.0
//! Actual manual Store-owner processes; NOT production worker lifecycle qualification.
#![cfg(windows)]
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{JobKind, JobOutcome, JobState, ProjectRegistration, WorkerIdentity},
};
use airk_store::{
    FileStore,
    service::{
        ProjectLease,
        collection::Collection,
        data_scope,
        registry::{AuditEvidence, ServiceRegistry},
    },
};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};
const MARKER: &[u8] = b"airk-owned-f10b-recovery-process-v1\n";
fn namespace() -> Incarnation {
    Incarnation::from_bytes([131; 16])
}
fn registration(root: &Path) -> Result<ProjectRegistration> {
    ProjectRegistration::new(
        root.join("source").to_str().context("source utf8")?.into(),
        root.join("data").to_str().context("data utf8")?.into(),
        "f10b-process".into(),
    )
}
fn receipt(root: &Path, name: &str) -> PathBuf {
    root.join("data/collections-1.4/f10b-process").join(name)
}
fn bounded(path: &Path) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= 16384,
        "fixture receipt shape"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(16385).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 16384, "fixture receipt bound");
    Ok(bytes)
}
fn emit(stage: &str, job: Option<Sha256Digest>, data: serde_json::Value) -> Result<()> {
    let bytes = serde_json::to_vec(&serde_json::json!({"stage":stage,"job":job,"data":data}))?;
    ensure!(bytes.len() <= 4096, "fixture event bound");
    let mut out = std::io::stdout().lock();
    out.write_all(&bytes)?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}
fn wait_for_kill() -> Result<()> {
    let mut byte = [0u8; 1];
    std::io::stdin().lock().read_exact(&mut byte)?;
    anyhow::bail!("fixture requires parent kill/reap, not graceful return")
}
fn grant(
    identity: WorkerIdentity,
    job: Sha256Digest,
) -> airk_core::service_resources::WorkAllowance {
    airk_core::service_resources::WorkAllowance {
        id: Incarnation::from_bytes([132; 16]),
        identity,
        job,
        extraction_threads: 1,
        inference_threads: 1,
        working_memory_bytes: 256 << 20,
        background_io_bytes_per_second: 8 << 20,
        ttl_ms: 120000,
    }
}
fn source_object(store: &FileStore, project: &ProjectRegistration, path: &str) -> Result<PathBuf> {
    let scope = store
        .current_scope(&project.root, &project.collection)?
        .context("scope")?;
    let read = store.current_read(&scope)?;
    let artifact = read.artifact(path)?.context("artifact")?;
    let retained = artifact.retained.as_ref().context("retained")?;
    let digest = retained
        .source_encoding
        .as_ref()
        .map_or(artifact.content_hash.as_str(), |encoding| {
            encoding.stored_digest.as_str()
        });
    Ok(store.directory().join("objects/sources").join(digest))
}
fn spoil(path: &Path) -> Result<Vec<u8>> {
    let original = fs::read(path)?;
    let mut changed = original.clone();
    ensure!(!changed.is_empty(), "empty fixture object");
    changed[0] ^= 0x80;
    fs::write(path, changed)?;
    Ok(original)
}
#[test]
#[ignore = "owned subprocess only; parent tests invoke exact helper"]
fn native_owner_recovery_process() -> Result<()> {
    let root =
        PathBuf::from(std::env::var_os("AIRK_F10B_ROOT").context("owned root")?).canonicalize()?;
    ensure!(
        fs::read(root.join("fixture-owned"))? == MARKER,
        "foreign fixture"
    );
    let mode = std::env::var("AIRK_F10B_MODE")?;
    let setup = matches!(mode.as_str(), "damage" | "first-page" | "clean");
    let project = registration(&root)?;
    let control = root.join("control");
    let mut registry = ServiceRegistry::open(&control, namespace())?;
    if setup {
        registry.register(&project)?;
    }
    let bound = data_scope::bind(Path::new(&project.data_root), &control, namespace())?;
    let mut lease = ProjectLease::acquire(&control, project.binding)?;
    registry.admit_worker(&mut lease)?;
    let lease = Arc::new(lease);
    let identity = WorkerIdentity {
        service: namespace(),
        worker: airk_kernel::service_broker::new_instance_id()?,
        project: project.binding,
        fencing_generation: lease.identity().1.context("epoch")?,
    };
    airk_kernel::service_execution::install(Path::new(&project.data_root), identity)?;
    let kernel = airk_kernel::Kernel::open_service_collection_for_diagnostics(
        Path::new(&project.root),
        Some(project.data_root.clone().into()),
        &project.collection,
    )?;
    if setup {
        kernel.configure_vectors(false)?;
        let indexed = registry.enqueue_internal(
            project.binding,
            namespace(),
            JobKind::Index,
            airk_kernel::service_broker::new_instance_id()?,
        )?;
        registry
            .claim_job_id(&lease, indexed.id)?
            .context("index claim")?;
        let allowance = grant(identity, indexed.id);
        airk_kernel::service_execution::activate(&allowance)?;
        let report =
            airk_kernel::service_execution::run(allowance, || kernel.index_service_sources())?;
        ensure!(report.indexed == 2, "actual two-source index");
        registry.complete_job(&lease, indexed.id, JobOutcome::Succeeded { revision: None })?;
    }
    let collection = Collection::existing(
        Path::new(&project.data_root),
        &project.root,
        &project.collection,
    )?;
    let store = collection.selected_store()?;
    let scope = store
        .current_scope(&project.root, &project.collection)?
        .context("scope")?;
    if mode == "clean" {
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        let read = store.current_read(&scope)?;
        owner.anchor(&read)?;
        drop(read);
        collection
            .arm_native_owner(&mut registry, lease, &bound, identity)?
            .settle_after_owned_drain(&mut registry)?;
        emit(
            "clean",
            None,
            serde_json::json!({"epoch":identity.fencing_generation}),
        )?;
        return Ok(());
    }
    if mode == "healthy" {
        let ordinary = bounded(&receipt(&root, "integrity-observation.json"))?;
        let exceptional = bounded(&receipt(&root, "integrity-recovery.json"))?;
        ensure!(
            exceptional == b"{",
            "missing malformed no-recovery-read witness"
        );
        let db = rusqlite::Connection::open_with_flags(
            control.join("service-1.4.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let before: i64 = db.query_row(
            "SELECT count(*) FROM service_jobs WHERE kind='verify'",
            [],
            |row| row.get(0),
        )?;
        collection
            .arm_native_owner(&mut registry, lease, &bound, identity)?
            .settle_after_owned_drain(&mut registry)?;
        let after: i64 = db.query_row(
            "SELECT count(*) FROM service_jobs WHERE kind='verify'",
            [],
            |row| row.get(0),
        )?;
        ensure!(before == 0 && after == 0, "clean reopen admitted Verify");
        ensure!(
            bounded(&receipt(&root, "integrity-observation.json"))? == ordinary
                && bounded(&receipt(&root, "integrity-recovery.json"))? == exceptional,
            "clean reopen changed audit receipts"
        );
        emit(
            "healthy",
            None,
            serde_json::json!({"verify_jobs":0,"exceptional_receipt_untouched":true,"ordinary_unchanged":true}),
        )?;
        return Ok(());
    }
    // Setup arms an actual positive owner and deliberately never settles it.
    let session = if setup {
        Some(collection.arm_native_owner(&mut registry, lease.clone(), &bound, identity)?)
    } else {
        None
    };
    if !setup {
        registry.recover_jobs(&lease)?;
    }
    let job = if setup {
        registry.enqueue_internal(
            project.binding,
            namespace(),
            JobKind::Verify,
            airk_kernel::service_broker::new_instance_id()?,
        )?
    } else {
        registry
            .next_pending_job(project.binding)?
            .context("same pending Verify")?
    };
    ensure!(job.kind == JobKind::Verify, "unexpected job");
    registry
        .claim_job_id(&lease, job.id)?
        .context("Verify claim")?;
    if setup {
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        let read = store.current_read(&scope)?;
        owner.anchor(&read)?;
        drop(read);
        let mut writer = store.lock()?;
        let first = owner.verify_turn(&mut writer, job.id, 1, 1 << 20, 100, || Ok(()))?;
        ensure!(
            first.verified_files == 1 && !first.progress.complete(),
            "ordinary first physical page"
        );
        drop(writer);
    }
    if mode == "damage" {
        let ordinary_path = receipt(&root, "integrity-observation.json");
        let saved = bounded(&ordinary_path)?;
        let a = source_object(&store, &project, "a.txt")?;
        let b = source_object(&store, &project, "b.txt")?;
        ensure!(a != b, "fault sources deduplicated");
        spoil(&a)?;
        let restore_b = spoil(&b)?;
        // Persistent trigger is confined to this owned fixture database, not a
        // product fault flag or a public registry SQL capability.
        let db = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
        db.execute_batch("CREATE TRIGGER refuse_owned_damage BEFORE INSERT ON service_recovery_budget WHEN NEW.scope LIKE 'integrity_damage:%' BEGIN SELECT RAISE(ABORT,'owned damage failure'); END")?;
        let fault_armed = std::cell::Cell::new(false);
        let checkpoints = std::cell::Cell::new(0usize);
        let detection = {
            let owner = collection.integrity_owner(&registry, &lease, &bound)?;
            let mut writer = store.lock()?;
            owner
                .verify_turn(&mut writer, job.id, 1, 1 << 20, 101, || {
                    let next = checkpoints.get() + 1;
                    checkpoints.set(next);
                    if next == 2 {
                        fs::remove_file(&ordinary_path)?;
                        fs::create_dir(&ordinary_path)?;
                        fault_armed.set(true);
                    }
                    Ok(())
                })
                .err()
                .context("second real Verify accepted corrupt b")?
        };
        ensure!(
            fault_armed.get()
                && format!("{detection:#}").contains("b.txt")
                && format!("{detection:#}").contains("persistence also failed"),
            "real b detection/persistence failure boundary was not exercised"
        );
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        let read = store.current_read(&scope)?;
        let component = read.token()?.digest;
        let error = owner
            .record_damage(&read, component, 102)
            .err()
            .context("actual damage stores unexpectedly succeeded")?;
        ensure!(
            format!("{error:#}").contains("neither damage store confirmed persistence"),
            "both actual record_damage stores did not fail"
        );
        drop(read);
        ensure!(
            registry.recorded_integrity_damage(&lease)?.is_empty(),
            "damage control falsely persisted"
        );
        db.execute_batch("DROP TRIGGER refuse_owned_damage")?;
        drop(db);
        fs::remove_dir(&ordinary_path)?;
        fs::write(&ordinary_path, &saved)?;
        fs::write(&b, restore_b)?;
        ensure!(
            bounded(&ordinary_path)? == saved,
            "original physical-page history changed"
        );
        emit(
            "dual_failed",
            Some(job.id),
            serde_json::json!({"ordinary_checked":1,"real_second_page_failed":true,"damage_stores_failed":2}),
        )?;
        let _retained = session;
        return wait_for_kill();
    }
    let ordinary = bounded(&receipt(&root, "integrity-observation.json"))?;
    let purpose = collection.begin_native_recovery(&registry, &lease, &bound, identity, job.id)?;
    let now = match mode.as_str() {
        "first-page" => 200,
        "second-page" => 300,
        "finish" | "reject" => 400,
        _ => anyhow::bail!("unknown mode"),
    };
    let owner = collection.integrity_owner(&registry, &lease, &bound)?;
    let mut writer = store.lock()?;
    if mode == "reject" {
        ensure!(
            owner
                .verify_recovery_turn(&mut writer, &purpose, 1, 1 << 20, now, || Ok(()))
                .is_err(),
            "fresh process skipped damaged a behind ordinary cursor"
        );
        drop(writer);
        let state: serde_json::Value =
            serde_json::from_slice(&bounded(&receipt(&root, "integrity-recovery.json"))?)?;
        ensure!(
            state["progress"]["checked_files"] == 0 && state["progress"]["unresolved"] == true,
            "fresh rejection manufactured page coverage"
        );
        ensure!(
            bounded(&receipt(&root, "integrity-observation.json"))? == ordinary,
            "recovery rewrote ordinary history"
        );
        ensure!(
            collection
                .arm_native_owner(&mut registry, lease, &bound, identity)
                .is_err(),
            "failed fresh recovery became Clean"
        );
        emit(
            "rejected",
            Some(job.id),
            serde_json::json!({"fresh_checked":0,"ordinary_unchanged":true}),
        )?;
        return Ok(());
    }
    let first = owner.verify_recovery_turn(&mut writer, &purpose, 1, 1 << 20, now, || Ok(()))?;
    let actual = serde_json::to_value(&first.progress)?;
    ensure!(
        first.verified_files == 1
            && !first.progress.complete()
            && actual["checked_files"] == 1
            && actual["started_unix_ms"] == now
            && actual["lexical_observed_unix_ms"] == now,
        "new actual epoch reused a prior recovery page"
    );
    ensure!(
        bounded(&receipt(&root, "integrity-observation.json"))? == ordinary,
        "fresh turn rewrote ordinary history"
    );
    if mode != "finish" {
        drop(writer);
        emit(
            "recovery_page",
            Some(job.id),
            serde_json::json!({"checked":1,"started":now,"epoch":identity.fencing_generation}),
        )?;
        let _retained = session;
        return wait_for_kill();
    }
    let done = owner.verify_recovery_turn(&mut writer, &purpose, 1, 1 << 20, now + 1, || Ok(()))?;
    ensure!(
        done.verified_files == 1 && done.progress.complete(),
        "fresh required EOF did not finish two physical inputs"
    );
    ensure!(
        done.completed.is_none(),
        "baseline recovery fabricated global projection coverage"
    );
    let drained = done.drained.context("actual baseline drain")?;
    let required = done
        .recovery_completed
        .context("actual fresh required proof")?;
    drop(writer);
    collection.recover_native_owner(&mut registry, &lease, &bound, identity, job.id, &required)?;
    registry.complete_job_with_maintenance_and_audit_at(
        &lease,
        job.id,
        JobOutcome::Failed {
            code: "integrity_coverage_incomplete".into(),
        },
        now + 2,
        None,
        Some(AuditEvidence::Drained(&drained)),
    )?;
    ensure!(
        registry.job(job.id)?.state == JobState::Failed
            && registry.integrity_audit_veto(&lease)?.is_none(),
        "fresh recovery failed actual settlement"
    );
    ensure!(
        bounded(&receipt(&root, "integrity-observation.json"))? == ordinary,
        "completion rewrote ordinary history"
    );
    collection
        .arm_native_owner(&mut registry, lease, &bound, identity)?
        .settle_after_owned_drain(&mut registry)?;
    emit(
        "completed",
        Some(job.id),
        serde_json::json!({"first_fresh_checked":1,"complete_checked":2,"epoch":identity.fencing_generation,"ordinary_unchanged":true}),
    )?;
    Ok(())
}
struct Process {
    child: Child,
    input: Option<ChildStdin>,
    events: mpsc::Receiver<serde_json::Value>,
    reader: Option<thread::JoinHandle<()>>,
    stderr: PathBuf,
}
impl Process {
    fn start(root: &Path, mode: &str) -> Result<Self> {
        let stderr = root.join(format!("{mode}.stderr"));
        let file = fs::File::create(&stderr)?;
        let mut child = Command::new(std::env::current_exe()?)
            .args([
                "--ignored",
                "--exact",
                "native_owner_recovery_process",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("AIRK_F10B_ROOT", root)
            .env("AIRK_F10B_MODE", mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(file))
            .spawn()?;
        let input = child.stdin.take();
        let output = child.stdout.take().context("child stdout")?;
        let (send, events) = mpsc::sync_channel(8);
        let reader = thread::spawn(move || {
            let mut output = BufReader::new(output);
            loop {
                let mut line = Vec::new();
                let count = output.by_ref().take(4097).read_until(b'\n', &mut line);
                if !matches!(count,Ok(n) if n>0&&line.len()<=4096) {
                    break;
                }
                if let Some(start) = line.iter().position(|byte| *byte == b'{') {
                    let Ok(event) = serde_json::from_slice(&line[start..]) else {
                        break;
                    };
                    if send.try_send(event).is_err() {
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            input,
            events,
            reader: Some(reader),
            stderr,
        })
    }
    fn event(&self, stage: &str) -> Result<serde_json::Value> {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .context("event deadline")?;
            let event = self
                .events
                .recv_timeout(remaining)
                .with_context(|| format!("owned event {stage} failed; {}", tail(&self.stderr)))?;
            if event["stage"] == stage {
                return Ok(event);
            }
        }
    }
    fn kill_reap(&mut self) -> Result<()> {
        self.child.kill()?;
        self.child.wait()?;
        self.input.take();
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = self.child.try_wait()? {
                ensure!(
                    status.success(),
                    "owned helper failed: {}",
                    tail(&self.stderr)
                );
                return Ok(());
            }
            ensure!(Instant::now() < deadline, "owned helper drain deadline");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
fn tail(path: &Path) -> String {
    use std::io::{Seek, SeekFrom};
    let result = (|| -> std::io::Result<String> {
        let mut file = fs::File::open(path)?;
        let len = file.metadata()?.len();
        file.seek(SeekFrom::Start(len.saturating_sub(4096)))?;
        let mut bytes = Vec::new();
        file.take(4096).read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    })();
    result.unwrap_or_else(|_| "stderr unavailable".into())
}
fn fixture() -> Result<tempfile::TempDir> {
    let owned = match std::env::var_os("AIRK_PRIVATE_CENSUS_BASE") {
        Some(base) => tempfile::Builder::new()
            .prefix("airk-native-owner-")
            .tempdir_in(base)?,
        None => tempfile::tempdir()?,
    };
    let root = owned.path().canonicalize()?;
    fs::write(root.join("fixture-owned"), MARKER)?;
    fs::create_dir(root.join("source"))?;
    airk_cli::local_ipc::prepare_root(&root.join("control"))?;
    fs::write(root.join("source/a.txt"), "first physical retained input\n")?;
    fs::write(
        root.join("source/b.txt"),
        "second independent physical input\n",
    )?;
    Ok(owned)
}
fn unchanged(root: &Path) -> Result<()> {
    ensure!(
        fs::read(root.join("source/a.txt"))? == b"first physical retained input\n"
            && fs::read(root.join("source/b.txt"))? == b"second independent physical input\n",
        "native owner changed original sources"
    );
    Ok(())
}
#[test]
fn native_owner_fresh_process_rechecks_damage_behind_cursor_after_dual_write_failure() -> Result<()>
{
    let owned = fixture()?;
    let root = owned.path().canonicalize()?;
    let mut first = Process::start(&root, "damage")?;
    let before = first.event("dual_failed")?;
    first.kill_reap()?;
    drop(first);
    let mut fresh = Process::start(&root, "reject")?;
    let rejected = fresh.event("rejected")?;
    fresh.finish()?;
    drop(fresh);
    ensure!(
        before["job"] == rejected["job"] && rejected["data"]["fresh_checked"] == 0,
        "fresh actual process skipped original damage"
    );
    unchanged(&root)
}
#[test]
fn native_owner_second_process_death_rechecks_required_first_page_in_third_epoch() -> Result<()> {
    let owned = fixture()?;
    let root = owned.path().canonicalize()?;
    let mut first = Process::start(&root, "first-page")?;
    let a = first.event("recovery_page")?;
    first.kill_reap()?;
    drop(first);
    let ordinary = bounded(&receipt(&root, "integrity-observation.json"))?;
    let mut second = Process::start(&root, "second-page")?;
    let b = second.event("recovery_page")?;
    second.kill_reap()?;
    drop(second);
    ensure!(
        a["job"] == b["job"]
            && a["data"]["epoch"] != b["data"]["epoch"]
            && a["data"]["started"] != b["data"]["started"],
        "second process inherited stale recovery epoch"
    );
    let mut third = Process::start(&root, "finish")?;
    let c = third.event("completed")?;
    third.finish()?;
    drop(third);
    ensure!(
        c["job"] == a["job"]
            && c["data"]["first_fresh_checked"] == 1
            && c["data"]["complete_checked"] == 2
            && c["data"]["epoch"] != b["data"]["epoch"],
        "third epoch did not produce fresh complete coverage"
    );
    ensure!(
        bounded(&receipt(&root, "integrity-observation.json"))? == ordinary,
        "crash recovery relabeled historical progress"
    );
    unchanged(&root)
}
#[test]
fn native_owner_healthy_clean_process_reopen_does_not_consume_exceptional_audit() -> Result<()> {
    let owned = fixture()?;
    let root = owned.path().canonicalize()?;
    let mut first = Process::start(&root, "clean")?;
    first.event("clean")?;
    first.finish()?;
    drop(first);
    fs::write(receipt(&root, "integrity-recovery.json"), b"{")?;
    let mut reopened = Process::start(&root, "healthy")?;
    let event = reopened.event("healthy")?;
    reopened.finish()?;
    drop(reopened);
    ensure!(
        event["data"]["verify_jobs"] == 0
            && event["data"]["ordinary_unchanged"] == true
            && event["data"]["exceptional_receipt_untouched"] == true,
        "healthy reopen performed exceptional audit"
    );
    unchanged(&root)
}
