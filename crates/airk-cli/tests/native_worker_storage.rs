// SPDX-License-Identifier: Apache-2.0
//! Production project-worker enrollment and accepted-work drain on untouched storage.
//! An unqualified native boundary is strict fallback, not a positive owner result.
use airk_cli::project_protocol::{self as protocol, Command as Control, Reply};
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{JobKind, JobState, ProjectRegistration, WorkerIdentity},
    service_resources::{WaitReason, WorkAllowance},
};
use airk_store::service::{ProjectLease, data_scope, registry::ServiceRegistry};
use anyhow::{Context, Result, ensure};
use rusqlite::OptionalExtension;
use serde_json::Value;
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const ORIGINAL: &str = "native worker retained copper lighthouse\nsecond original line\n";

struct Worker {
    child: Child,
    input: Option<ChildStdin>,
    replies: mpsc::Receiver<Reply>,
    reader: Option<thread::JoinHandle<()>>,
    stderr: PathBuf,
    identity: WorkerIdentity,
}

impl Worker {
    fn start(root: &Path, project: &ProjectRegistration, service: Incarnation) -> Result<Self> {
        let stderr = root.join(format!(
            "native-worker-{}.stderr",
            airk_kernel::service_broker::new_instance_id()?
        ));
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
            .stderr(fs::File::create(&stderr)?)
            .spawn()?;
        let input = child.stdin.take();
        let mut output = child.stdout.take().context("worker stdout")?;
        let (send, replies) = mpsc::sync_channel(8);
        let reader = thread::spawn(move || {
            while let Ok(Some(reply)) = airk_cli::service_ipc::read_frame::<_, Reply>(&mut output) {
                if send.try_send(reply).is_err() {
                    break;
                }
            }
        });
        let mut worker = Self {
            child,
            input,
            replies,
            reader: Some(reader),
            stderr,
            identity: WorkerIdentity {
                service,
                worker: airk_kernel::service_broker::new_instance_id()?,
                project: project.binding,
                fencing_generation: 0,
            },
        };
        worker.send(&protocol::Init {
            engine: Sha256Digest::from_bytes([0; 32]),
            runtime: root.join("control"),
            project: project.binding,
            service,
            worker: worker.identity.worker,
        })?;
        let Reply::Ready { identity, .. } = worker.reply()? else {
            anyhow::bail!("production project worker did not become ready");
        };
        worker.identity = identity;
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
            "activation refused"
        );
        Ok(worker)
    }

    fn send(&mut self, message: &impl serde::Serialize) -> Result<()> {
        airk_cli::service_ipc::write_frame(
            self.input.as_mut().context("worker input closed")?,
            message,
        )?;
        Ok(())
    }

    fn reply(&self) -> Result<Reply> {
        self.replies
            .recv_timeout(Duration::from_secs(20))
            .with_context(|| format!("worker reply deadline; stderr_tail={}", self.stderr_tail()))
    }

    fn stderr_tail(&self) -> String {
        (|| -> std::io::Result<String> {
            let mut file = fs::File::open(&self.stderr)?;
            file.seek(SeekFrom::Start(file.metadata()?.len().saturating_sub(4096)))?;
            let mut bytes = Vec::new();
            file.take(4096).read_to_end(&mut bytes)?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        })()
        .unwrap_or_else(|_| "unavailable".into())
    }

    fn query_original(&mut self, root: &Path) -> Result<Value> {
        self.send(&Control::Grant {
            caller: airk_cli::local_ipc::caller_identity()?,
        })?;
        let Reply::Grant { grant } = self.reply()? else {
            anyhow::bail!("production worker refused query grant");
        };
        ensure!(
            grant.identity == self.identity,
            "query grant changed worker"
        );
        let identity = self.identity;
        tokio::runtime::Runtime::new()?.block_on(async {
            let endpoint = airk_cli::local_ipc::endpoint(&root.join("control"), &protocol::role(identity.worker))?;
            let mut stream = tokio::time::timeout(Duration::from_secs(5), airk_cli::local_ipc::connect(&endpoint)).await??;
            airk_cli::local_ipc::write_message(&mut stream, &grant, Duration::from_secs(5)).await?;
            let accepted: protocol::Accepted = airk_cli::local_ipc::read_message(&mut stream, Duration::from_secs(5)).await?.context("query acceptance EOF")?;
            ensure!(accepted.identity == identity, "accepted query changed worker");
            // native read: exactly one first evidence request proves activation did not require a hidden warm-up or replay.
            airk_cli::local_ipc::write_message(&mut stream, &protocol::Query {
                frontend: None,
                tool: "airk_read_file".into(),
                arguments: serde_json::json!({"path":"original.html","start_line":1,"end_line":1,"max_bytes":4096,"response_mode":"full"}).as_object().context("fixture arguments")?.clone(),
                native: None,
                status_receipt: Default::default(),
            }, Duration::from_secs(5)).await?;
            let reply: protocol::QueryResult = airk_cli::local_ipc::read_message(&mut stream, Duration::from_secs(20)).await?.context("evidence EOF")?;
            Ok::<_, anyhow::Error>(reply.result)
        })
    }

    fn read_original(&mut self, root: &Path) -> Result<()> {
        let reply = self.query_original(root)?;
        ensure!(
            reply.get("isError") != Some(&Value::Bool(true)),
            "first evidence failed: {reply}"
        );
        let evidence = reply.to_string();
        ensure!(
            evidence.contains("copper lighthouse") && evidence.contains("original.html"),
            "first evidence lost text/citation: {reply}"
        );
        Ok(())
    }

    fn assert_owns_project(&mut self, root: &Path) -> Result<()> {
        ensure!(
            self.child.try_wait()?.is_none(),
            "worker exited during accepted work"
        );
        let refusal = ProjectLease::acquire(&root.join("control"), self.identity.project)
            .err()
            .context("accepted worker released its project lease")?;
        ensure!(
            matches!(
                refusal.downcast_ref::<std::fs::TryLockError>(),
                Some(std::fs::TryLockError::WouldBlock)
            ),
            "unexpected project lease refusal: {refusal:#}"
        );
        Ok(())
    }

    fn finish_status(&mut self) -> Result<std::process::ExitStatus> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = self.child.try_wait()? {
                return Ok(status);
            }
            ensure!(
                Instant::now() < deadline,
                "production worker drain deadline; stderr_tail={}",
                self.stderr_tail()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn finish(&mut self) -> Result<()> {
        let status = self.finish_status()?;
        ensure!(
            status.success(),
            "production worker drain failed: {status}; stderr_tail={}",
            self.stderr_tail()
        );
        Ok(())
    }

    fn kill_and_reap(&mut self) -> Result<()> {
        self.child.kill()?;
        let status = self.child.wait()?;
        ensure!(
            !status.success(),
            "killed production worker unexpectedly completed a clean drain"
        );
        self.input.take();
        if let Some(reader) = self.reader.take() {
            reader
                .join()
                .map_err(|_| anyhow::anyhow!("killed worker reply reader panicked"))?;
        }
        Ok(())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn owner_state(root: &Path, project: &ProjectRegistration) -> Result<Option<Value>> {
    let connection = rusqlite::Connection::open_with_flags(
        root.join("control/service-1.4.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let encoded: Option<String> = connection
        .query_row(
            "SELECT substr(engine,1,4097) FROM service_recovery_budget WHERE scope=?1",
            [format!("owner_session:{}", project.binding)],
            |row| row.get(0),
        )
        .optional()?;
    encoded
        .map(|bytes| {
            ensure!(bytes.len() <= 4096, "owner state exceeds bound");
            Ok(serde_json::from_str(&bytes)?)
        })
        .transpose()
}

fn fixture(positive: bool) -> Result<tempfile::TempDir> {
    let owner = if positive {
        tempfile::Builder::new().prefix("airk-production-owner-").tempdir_in(
            std::env::var_os("AIRK_PRIVATE_CENSUS_BASE").context("positive qualification requires AIRK_PRIVATE_CENSUS_BASE on an already private native namespace")?
        )?
    } else {
        tempfile::tempdir()?
    };
    // No chmod, ACL rewrite or handcrafted owner capability: production alone decides eligibility.
    fs::create_dir(owner.path().join("source"))?;
    fs::create_dir(owner.path().join("data"))?;
    fs::write(owner.path().join("source/original.html"), ORIGINAL)?;
    airk_cli::local_ipc::prepare_root(&owner.path().join("control"))?;
    Ok(owner)
}

fn worker_drain_and_reopen(positive: bool) -> Result<()> {
    for closed_input in [false, true] {
        let owned = fixture(positive)?;
        let root = owned.path().canonicalize()?;
        let service = airk_kernel::service_broker::new_instance_id()?;
        let project = airk_kernel::service_projects::registration(
            &root.join("source"),
            &root.join("data"),
            "native-worker-storage",
        )?;
        let mut registry = ServiceRegistry::open(&root.join("control"), service)?;
        registry.register(&project)?;
        data_scope::bind(&root.join("data"), &root.join("control"), service)?;
        {
            let kernel = airk_kernel::Kernel::open_service_collection(
                root.join("source"),
                Some(root.join("data")),
                &project.collection,
            )?;
            kernel.configure_vectors(false)?;
            kernel.index()?;
        }
        ensure!(
            owner_state(&root, &project)?.is_none(),
            "fixture pre-created owner authority"
        );
        let mut worker = Worker::start(&root, &project, service)?;
        worker.read_original(&root)?;
        let active = owner_state(&root, &project)?;
        if let Some(active) = &active {
            ensure!(
                active["state"] == "active"
                    && active["worker"] == serde_json::to_value(worker.identity.worker)?,
                "production owner not Active for actual worker: {active}"
            );
        } else {
            ensure!(
                !positive,
                "prequalified production worker did not establish positive eligibility"
            );
            eprintln!(
                "untouched production storage: strict fallback; positive native eligibility not qualified"
            );
        }
        let audit = registry.enqueue_internal(
            project.binding,
            service,
            JobKind::Verify,
            airk_kernel::service_broker::new_instance_id()?,
        )?;
        let ledger = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join("data/.quota-1.4/ledger.lock"))?;
        ledger.try_lock()?;
        let grant = WorkAllowance {
            id: airk_kernel::service_broker::new_instance_id()?,
            identity: worker.identity,
            job: audit.id,
            extraction_threads: 1,
            inference_threads: 1,
            working_memory_bytes: 256 << 20,
            background_io_bytes_per_second: 8 << 20,
            ttl_ms: 120000,
        };
        let grant_id = grant.id;
        worker.send(&Control::RunJob {
            identity: worker.identity,
            allowance: grant,
        })?;
        ensure!(
            matches!(worker.reply()?, Reply::WorkAccepted),
            "Verify not accepted"
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            worker.send(&Control::WorkStatus {
                identity: worker.identity,
                control: None,
                keep_for_maintenance: false,
                freshness: None,
            })?;
            let Reply::WorkStatus { observation } = worker.reply()? else {
                anyhow::bail!("work status missing");
            };
            ensure!(
                observation.finished != Some(grant_id),
                "held Verify completed before physical quota admission"
            );
            if registry.job(audit.id)?.state == JobState::Running
                && observation.waiting_for == Some(WaitReason::NativeLock)
            {
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "Verify did not reach real ledger admission"
            );
            thread::sleep(Duration::from_millis(10));
        }
        worker.assert_owns_project(&root)?;
        if !closed_input {
            worker.send(&Control::Stop)?;
        }
        worker.input.take();
        worker.assert_owns_project(&root)?;
        ensure!(
            owner_state(&root, &project)? == active,
            "Stop/EOF settled owner while accepted Verify was blocked"
        );
        ledger.unlock()?;
        drop(ledger);
        worker.finish()?;
        drop(worker);
        ensure!(
            registry.job(audit.id)?.state == JobState::Succeeded,
            "accepted Verify did not finish before owner drain: {:?}",
            registry.job(audit.id)?
        );
        let clean = owner_state(&root, &project)?;
        match (&active, &clean) {
            (Some(active), Some(clean)) => ensure!(
                clean["state"] == "clean" && clean["nonce"] == active["nonce"],
                "owner did not settle exact admitted session: {clean}"
            ),
            (None, None) => (),
            _ => anyhow::bail!("strict fallback changed owner authority during drain"),
        }
        drop(ProjectLease::acquire(
            &root.join("control"),
            project.binding,
        )?);
        let recovery =
            root.join("data/collections-1.4/native-worker-storage/integrity-recovery.json");
        let before_recovery = fs::read(&recovery).ok();
        let mut reopened = Worker::start(&root, &project, service)?;
        reopened.read_original(&root)?;
        if let Some(previous) = &clean {
            let current = owner_state(&root, &project)?.context("clean reopen lost owner")?;
            ensure!(
                current["state"] == "active"
                    && current["nonce"] != previous["nonce"]
                    && current["epoch"].as_u64() > previous["epoch"].as_u64(),
                "clean reopen reused stale worker authority: {current}"
            );
        } else {
            ensure!(
                owner_state(&root, &project)?.is_none(),
                "strict reopen invented native authority"
            );
        }
        ensure!(
            fs::read(&recovery).ok() == before_recovery,
            "healthy clean reopen consumed exceptional recovery"
        );
        reopened.send(&Control::Stop)?;
        reopened.input.take();
        reopened.finish()?;
        ensure!(
            fs::read_to_string(root.join("source/original.html"))? == ORIGINAL,
            "production owner changed source bytes"
        );
    }
    Ok(())
}

#[test]
fn native_worker_untouched_storage_drains_accepted_verify_and_reopens() -> Result<()> {
    worker_drain_and_reopen(false)
}

#[test]
#[ignore = "requires AIRK_PRIVATE_CENSUS_BASE on already private supported native storage; no ACL changes"]
fn native_worker_positive_owner_qualifies_actual_stop_eof_and_clean_reopen() -> Result<()> {
    worker_drain_and_reopen(true)
}

fn optional_bounded_receipt(path: &Path) -> Result<Option<Vec<u8>>> {
    if !path.try_exists()? {
        return Ok(None);
    }
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 16384,
        "fixture receipt is not a bounded regular file"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(16385).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 16384, "fixture receipt grew beyond bound");
    Ok(Some(bytes))
}

fn durable_damage_row(root: &Path, project: &ProjectRegistration) -> Result<Option<String>> {
    let connection = rusqlite::Connection::open_with_flags(
        root.join("control/service-1.4.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let encoded: Option<String> = connection
        .query_row(
            "SELECT substr(engine,1,16385) FROM service_recovery_budget WHERE scope=?1",
            [format!("integrity_damage:{}", project.binding)],
            |row| row.get(0),
        )
        .optional()?;
    ensure!(
        encoded.as_ref().is_none_or(|bytes| bytes.len() <= 16384),
        "fixture durable damage exceeds bound"
    );
    Ok(encoded)
}

fn run_accepted_verify(
    worker: &mut Worker,
    registry: &mut ServiceRegistry,
    project: &ProjectRegistration,
    service: Incarnation,
) -> Result<airk_core::service::JobRecord> {
    let audit = registry.enqueue_internal(
        project.binding,
        service,
        JobKind::Verify,
        airk_kernel::service_broker::new_instance_id()?,
    )?;
    let grant = WorkAllowance {
        id: airk_kernel::service_broker::new_instance_id()?,
        identity: worker.identity,
        job: audit.id,
        extraction_threads: 1,
        inference_threads: 1,
        working_memory_bytes: 768 << 20,
        background_io_bytes_per_second: 8 << 20,
        ttl_ms: 120000,
    };
    let grant_id = grant.id;
    worker.send(&Control::RunJob {
        identity: worker.identity,
        allowance: grant,
    })?;
    ensure!(
        matches!(worker.reply()?, Reply::WorkAccepted),
        "production worker refused the accepted Verify"
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        worker.send(&Control::WorkStatus {
            identity: worker.identity,
            control: None,
            keep_for_maintenance: false,
            freshness: None,
        })?;
        let Reply::WorkStatus { observation } = worker.reply()? else {
            anyhow::bail!("accepted Verify work observation missing");
        };
        let wire = serde_json::to_value(&observation)?;
        ensure!(
            wire.get("failure").is_none(),
            "production Verify reported failure: {wire}"
        );
        if observation.finished == Some(grant_id) {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "production Verify did not complete within its fixture budget"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let completed = registry.job(audit.id)?;
    ensure!(
        completed.state == JobState::Succeeded,
        "accepted Verify did not succeed: {completed:?}"
    );
    Ok(completed)
}

fn receipt_difference(before: &[u8], after: Option<&[u8]>) -> Result<Value> {
    let before: Value = serde_json::from_slice(before)?;
    let after = after.map(serde_json::from_slice::<Value>).transpose()?;
    let keys = before
        .as_object()
        .context("ordinary receipt object")?
        .keys()
        .chain(
            after
                .as_ref()
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|object| object.keys()),
        )
        .collect::<std::collections::BTreeSet<_>>();
    let changed = keys
        .into_iter()
        .filter(|key| before.get(*key) != after.as_ref().and_then(|after| after.get(*key)))
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "after_present":after.is_some(),
        "changed_top_level_fields":changed,
        "before_worker_epoch":before["worker_epoch"],
        "after_worker_epoch":after.as_ref().and_then(|value| value.get("worker_epoch")),
        "history_equal":after.as_ref().is_some_and(|value|value.get("verify")==before.get("verify")),
        "local_damage_equal":after.as_ref().is_some_and(|value|value.get("known_damage")==before.get("known_damage")),
    }))
}

#[test]
#[ignore = "requires AIRK_PRIVATE_CENSUS_BASE on already private supported native storage; no ACL changes"]
fn native_worker_positive_owner_recovers_inherited_active_after_actual_process_kill() -> Result<()>
{
    let owned = fixture(true)?;
    let root = owned.path().canonicalize()?;
    let service = airk_kernel::service_broker::new_instance_id()?;
    let project = airk_kernel::service_projects::registration(
        &root.join("source"),
        &root.join("data"),
        "native-worker-storage",
    )?;
    let mut registry = ServiceRegistry::open(&root.join("control"), service)?;
    registry.register(&project)?;
    data_scope::bind(&root.join("data"), &root.join("control"), service)?;
    {
        let kernel = airk_kernel::Kernel::open_service_collection(
            root.join("source"),
            Some(root.join("data")),
            &project.collection,
        )?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
    }
    let receipts = root.join("data/collections-1.4/native-worker-storage");
    let ordinary = receipts.join("integrity-observation.json");
    let exceptional = receipts.join("integrity-recovery.json");
    let mut first = Worker::start(&root, &project, service)?;
    first.read_original(&root)?;
    // Establish actual ordinary physical history through the production role,
    // rather than treating creation of the first metadata anchor as a reset.
    let historical_audit = run_accepted_verify(&mut first, &mut registry, &project, service)?;
    let inherited = owner_state(&root, &project)?.context("positive worker did not arm")?;
    ensure!(
        inherited["state"] == "active"
            && inherited["worker"] == serde_json::to_value(first.identity.worker)?,
        "positive production worker lacked actual Active authority: {inherited}"
    );
    let ordinary_before = optional_bounded_receipt(&ordinary)?
        .context("first accepted Verify did not retain ordinary history")?;
    let historical: Value = serde_json::from_slice(&ordinary_before)?;
    ensure!(
        historical["verify"]["job"] == serde_json::to_value(historical_audit.id)?
            && historical["verify"]["checked_files"] == 1
            && historical["verify"]["lexical_observed_unix_ms"].is_u64(),
        "first accepted Verify did not establish meaningful dated physical history: {historical}"
    );
    let damage_before = durable_damage_row(&root, &project)?;
    ensure!(
        optional_bounded_receipt(&exceptional)?.is_none(),
        "fresh positive worker unexpectedly consumed exceptional recovery"
    );
    first.kill_and_reap()?;
    drop(first);
    ensure!(
        owner_state(&root, &project)? == Some(inherited.clone()),
        "process death acquired Clean authority or changed inherited Active"
    );
    drop(ProjectLease::acquire(
        &root.join("control"),
        project.binding,
    )?);

    let mut recovered = Worker::start(&root, &project, service)?;
    ensure!(
        serde_json::to_value(recovered.identity.worker)? != inherited["worker"]
            && recovered.identity.fencing_generation
                > inherited["epoch"]
                    .as_u64()
                    .context("inherited worker epoch")?,
        "replacement worker did not establish a fresh actual process epoch"
    );
    recovered.send(&Control::WorkStatus {
        identity: recovered.identity,
        control: None,
        keep_for_maintenance: false,
        freshness: None,
    })?;
    let Reply::WorkStatus { observation } = recovered.reply()? else {
        anyhow::bail!("replacement worker integrity observation missing");
    };
    let observation = serde_json::to_value(observation)?;
    ensure!(
        observation["integrity_observation_failure"] == "native_owner_recovery_required",
        "inherited Active did not report required physical recovery: {observation}"
    );
    recovered.read_original(&root)?;
    ensure!(
        owner_state(&root, &project)? == Some(inherited.clone())
            && optional_bounded_receipt(&exceptional)?.is_none(),
        "replacement startup or strict evidence silently acquired fresh authority"
    );
    let audit = run_accepted_verify(&mut recovered, &mut registry, &project, service)?;
    let active = owner_state(&root, &project)?.context("recovery did not rearm native owner")?;
    ensure!(
        active["state"] == "active"
            && active["worker"] == serde_json::to_value(recovered.identity.worker)?
            && active["nonce"] != inherited["nonce"]
            && active["epoch"] == serde_json::to_value(recovered.identity.fencing_generation)?
            && active["basis"] == "physical_verify"
            && active["head"] == inherited["head"],
        "accepted physical recovery did not rearm the exact new owner/head: {active}"
    );
    let exceptional = optional_bounded_receipt(&exceptional)?
        .context("actual production recovery did not retain its bounded receipt")?;
    let exceptional: Value = serde_json::from_slice(&exceptional)?;
    ensure!(
        exceptional["epoch"]["profile"] == "projected_v1"
            && exceptional["epoch"]["job"] == serde_json::to_value(audit.id)?
            && exceptional["epoch"]["worker"] == serde_json::to_value(recovered.identity.worker)?
            && exceptional["epoch"]["worker_epoch"]
                == serde_json::to_value(recovered.identity.fencing_generation)?,
        "production recovery used the wrong fixed profile/job/epoch: {exceptional}"
    );
    let ordinary_after = optional_bounded_receipt(&ordinary)?;
    let damage_after = durable_damage_row(&root, &project)?;
    ensure!(
        ordinary_after.as_deref() == Some(ordinary_before.as_slice())
            && damage_after == damage_before,
        "exceptional recovery rewrote ordinary history or durable damage: ordinary_difference={}, durable_damage_equal={}",
        receipt_difference(&ordinary_before, ordinary_after.as_deref())?,
        damage_after == damage_before,
    );
    recovered.read_original(&root)?;
    recovered.send(&Control::Stop)?;
    recovered.input.take();
    recovered.finish()?;
    let clean = owner_state(&root, &project)?.context("recovered worker drain lost owner")?;
    ensure!(
        clean["state"] == "clean"
            && clean["nonce"] == active["nonce"]
            && clean["head"] == inherited["head"],
        "recovered production worker failed exact clean drain: {clean}"
    );
    ensure!(
        fs::read_to_string(root.join("source/original.html"))? == ORIGINAL,
        "actual killed-owner recovery changed original source bytes"
    );
    Ok(())
}

#[test]
fn native_worker_damaged_selected_source_refuses_evidence() -> Result<()> {
    let owned = fixture(false)?;
    let root = owned.path().canonicalize()?;
    let service = airk_kernel::service_broker::new_instance_id()?;
    let project = airk_kernel::service_projects::registration(
        &root.join("source"),
        &root.join("data"),
        "native-worker-storage",
    )?;
    let mut registry = ServiceRegistry::open(&root.join("control"), service)?;
    registry.register(&project)?;
    data_scope::bind(&root.join("data"), &root.join("control"), service)?;
    {
        let kernel = airk_kernel::Kernel::open_service_collection(
            root.join("source"),
            Some(root.join("data")),
            &project.collection,
        )?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
    }
    let mut worker = Worker::start(&root, &project, service)?;
    worker.read_original(&root)?;
    let active = owner_state(&root, &project)?;
    let object = {
        let collection = airk_store::service::collection::Collection::existing(
            &root.join("data"),
            &project.root,
            &project.collection,
        )?;
        let store = collection.selected_store()?;
        let scope = store
            .current_scope(&project.root, &project.collection)?
            .context("indexed scope")?;
        let read = store.current_read(&scope)?;
        ensure!(
            read.native_line_projection(&store, "original.html")?
                .is_some(),
            "fixture did not produce a native selected-source index"
        );
        let artifact = read
            .artifact("original.html")?
            .context("indexed artifact")?;
        let retained = artifact.retained.as_ref().context("retained artifact")?;
        let digest = retained
            .source_encoding
            .as_ref()
            .map_or(artifact.content_hash.as_str(), |encoding| {
                encoding.stored_digest.as_str()
            });
        store.directory().join("objects/sources").join(digest)
    };
    let mut damaged = fs::read(&object)?;
    ensure!(!damaged.is_empty(), "empty retained source fixture");
    damaged[0] ^= 0x80;
    fs::write(object, damaged)?;
    // native read: one deliberate corruption request must refuse; retrying would conceal whether this worker observed the damage.
    let reply = worker.query_original(&root)?;
    ensure!(
        reply.get("isError") == Some(&Value::Bool(true)),
        "damaged retained source returned successful evidence: {reply}"
    );
    worker.send(&Control::Stop)?;
    worker.input.take();
    // A damaged private owner may exit unsuccessfully when Clean settlement is
    // refused; the safety contract is its retained Active record, not an exit code.
    let _ = worker.finish_status()?;
    if let Some(active) = active {
        let retained = owner_state(&root, &project)?.context("damage erased native owner")?;
        ensure!(
            retained["state"] == "active" && retained["nonce"] == active["nonce"],
            "known damage acquired Clean authority: {retained}"
        );
    } else {
        ensure!(
            owner_state(&root, &project)?.is_none(),
            "strict damage refusal invented owner authority"
        );
        eprintln!(
            "selected corruption refused on strict fallback; positive native damage settlement not qualified"
        );
    }
    ensure!(
        fs::read_to_string(root.join("source/original.html"))? == ORIGINAL,
        "corruption fixture changed original source"
    );
    Ok(())
}
