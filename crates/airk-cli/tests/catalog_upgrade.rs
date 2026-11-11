// SPDX-License-Identifier: Apache-2.0
//! Process-level compatibility checks in disposable control state only.
#[path = "common/read_retry.rs"]
mod read_retry;
use airk_core::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    service::{ControlMethod, ControlResponse, ProjectRegistration},
};
use airk_store::service::registry::{
    ServiceRegistry,
    catalog::upgrade::{
        self, CatalogUpgrade, CatalogUpgradeObservation, ReadinessEvidence, RuntimeRefusal,
    },
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, params};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

fn fixture() -> Result<tempfile::TempDir> {
    let root = private_fixture()?;
    seed_registration(
        root.path(),
        "/fictional/offline-source".into(),
        "/fictional/offline-index".into(),
    )?;
    fs::write(
        root.path().join("source-and-collection-sentinel"),
        b"untouched",
    )?;
    Ok(root)
}
fn private_fixture() -> Result<tempfile::TempDir> {
    Ok(match std::env::var_os("AIRK_PRIVATE_CENSUS_BASE") {
        Some(base) => tempfile::Builder::new()
            .prefix("airk-catalog-upgrade-")
            .tempdir_in(base)?,
        None => tempfile::tempdir()?,
    })
}
fn seed_registration(root: &Path, source: String, data: String) -> Result<()> {
    drop(ServiceRegistry::open(
        root,
        Incarnation::from_bytes([1; 16]),
    )?);
    let registration = ProjectRegistration::new(source, data, "fixture".into())?;
    let connection = Connection::open(root.join("service-1.4.sqlite3"))?;
    connection.execute(
        "INSERT INTO projects(binding,payload,epoch,paused,registered) VALUES(?1,?2,7,1,1)",
        params![
            registration.binding.to_string(),
            serde_json::to_vec(&registration)?
        ],
    )?;
    Ok(())
}

struct FixtureService {
    #[cfg(unix)]
    child: airk_process::GroupChild,
    #[cfg(not(unix))]
    child: std::process::Child,
    #[cfg(not(unix))]
    job: airk_process::ChildJob,
}
impl FixtureService {
    fn start(mut command: Command) -> Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        #[cfg(unix)]
        {
            Ok(Self {
                child: airk_process::GroupChild::spawn(&mut command)?,
            })
        }
        #[cfg(not(unix))]
        {
            let mut child = command.spawn()?;
            let job = match airk_process::ChildJob::attach(&mut child) {
                Ok(job) => job,
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
            };
            Ok(Self { child, job })
        }
    }
}
impl Drop for FixtureService {
    fn drop(&mut self) {
        #[cfg(unix)]
        self.child.drain();
        #[cfg(not(unix))]
        {
            self.job.drain();
            let _ = self.child.wait();
        }
    }
}

#[test]
fn activated_format_serves_actual_control_and_post_start_writes_refuse_restore() -> Result<()> {
    let owner = private_fixture()?;
    let control = owner.path().join("control");
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    for path in [&control, &source, &data] {
        fs::create_dir(path)?;
    }
    let source = source.canonicalize()?;
    let data = data.canonicalize()?;
    fs::write(source.join("sentinel.txt"), b"source remains unchanged")?;
    seed_registration(
        &control,
        source.to_str().unwrap().into(),
        data.to_str().unwrap().into(),
    )?;
    airk_store::service::data_scope::bind(&data, &control, Incarnation::from_bytes([1; 16]))?;
    let upgrade = CatalogUpgrade::acquire(&control)?;
    airk_kernel::service_catalog_upgrade::prepare(
        &upgrade,
        airk_cli::runtime_pair::engine_identity_at(&candidate())?,
    )?;
    upgrade.migrate()?;
    assert!(
        upgrade.settle().is_err(),
        "production activation remains disabled"
    );
    drop(upgrade);
    // Format/transport fixture only: Store's actual settlement is exercised by
    // its boundary tests. This manual seed does not qualify managed activation
    // or bypass the production gate in the distributed command path.
    let connection = Connection::open(control.join("service-1.4.sqlite3"))?;
    connection.execute("UPDATE index_catalog_meta SET activated=1", [])?;
    drop(connection);
    let record = control.join("service-1.4.owner.json");
    let mut metadata: serde_json::Value = serde_json::from_slice(&fs::read(&record)?)?;
    metadata.as_object_mut().unwrap().remove("catalog_upgrade");
    fs::write(record, serde_json::to_vec(&metadata)?)?;
    let log = owner.path().join("service.log");
    let mut command = Command::new(candidate());
    command
        .args(["serve", "--runtime-dir"])
        .arg(&control)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(fs::File::create(&log)?));
    let mut service = FixtureService::start(command)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let reply = runtime.block_on(airk_cli::service_runtime::request(
            &control,
            ControlMethod::RequestEpoch,
        ));
        if matches!(reply, Ok(ControlResponse::RequestEpoch { .. })) {
            break;
        }
        ensure!(
            service.child.try_wait()?.is_none(),
            "candidate service exited before registry readiness: {}",
            fs::read_to_string(&log)?
        );
        ensure!(
            Instant::now() < deadline,
            "registry readiness failed: {}; {}",
            match &reply {
                Ok(response) => serde_json::to_string(response)?,
                Err(error) => format!("{error:#}"),
            },
            fs::read_to_string(&log)?
        );
        // No job/configuration key exists: reacquiring only the metadata epoch
        // cannot replay user work. This is the canonical setup-epoch boundary.
        std::thread::sleep(Duration::from_millis(25));
    }
    exercise_native_catalog_transports(&control, &runtime)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let response = runtime.block_on(airk_cli::service_runtime::request(
        &control,
        ControlMethod::Shutdown,
    ))?;
    ensure!(
        !matches!(response, ControlResponse::Failure { .. }),
        "shutdown refused: {}",
        serde_json::to_string(&response)?
    );
    while service.child.try_wait()?.is_none() {
        ensure!(
            Instant::now() < deadline,
            "candidate did not drain; {}",
            fs::read_to_string(&log)?
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    drop(service);
    let registry = ServiceRegistry::open_saved(&control)?;
    assert!(registry.catalog_state()?.unwrap().accepted_new_writes);
    assert!(
        registry
            .get(airk_kernel::service_projects::registration(&source, &data, "fixture")?.binding)?
            .paused
    );
    drop(registry);
    let before = fs::read(control.join("service-1.4.sqlite3"))?;
    let upgrade = CatalogUpgrade::acquire(&control)?;
    ensure!(
        upgrade
            .restore()
            .unwrap_err()
            .to_string()
            .contains("forward repair"),
        "post-start native writes must fence downgrade"
    );
    assert_eq!(fs::read(control.join("service-1.4.sqlite3"))?, before);
    assert_eq!(
        fs::read(source.join("sentinel.txt"))?,
        b"source remains unchanged"
    );
    assert!(
        !data.join("collections-1.4/fixture").exists(),
        "metadata activation must not rebuild the paused index"
    );
    Ok(())
}
fn candidate() -> PathBuf {
    std::env::var_os("AIRK_CATALOG_CANDIDATE_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_airk").into())
}

#[test]
fn native_migration_requires_exact_admission_and_replays_without_rebuilding() -> Result<()> {
    let binary = candidate();
    let root = fixture()?;
    let migrate = |id: Incarnation| -> Result<Output> {
        let mut command = Command::new(&binary);
        command
            .args(["runtime", "catalog-upgrade-migrate", "--control-root"])
            .arg(root.path())
            .arg("--upgrade-id")
            .arg(id.to_string());
        run(&command)
    };
    let wrong_id = Incarnation::from_bytes([201; 16]);
    let before = files(root.path())?;
    let output = migrate(wrong_id)?;
    ensure!(
        !output.status.success() && String::from_utf8_lossy(&output.stderr).contains("not fenced"),
        "unadmitted migration: {output:?}"
    );
    assert!(files(root.path())? == before);
    let owner = CatalogUpgrade::acquire(root.path())?;
    let receipt = airk_kernel::service_catalog_upgrade::prepare(
        &owner,
        airk_cli::runtime_pair::engine_identity_at(&binary)?,
    )?;
    drop(owner);
    let before = files(root.path())?;
    let output = migrate(wrong_id)?;
    ensure!(
        !output.status.success()
            && String::from_utf8_lossy(&output.stderr)
                .contains("differs from the admitted upgrade"),
        "crossed migration: {output:?}"
    );
    assert!(files(root.path())? == before);
    let output = migrate(receipt.plan.id)?;
    ensure!(
        output.status.success(),
        "native migration failed: {output:?}"
    );
    let actual: upgrade::UpgradeReceipt = serde_json::from_slice(&output.stdout)?;
    assert_eq!(actual.plan, receipt.plan);
    assert_eq!(actual.stage, upgrade::UpgradeStage::Imports);
    assert_eq!(
        serde_json::to_value(upgrade::pending(root.path())?)?,
        serde_json::to_value(Some(&actual))?
    );
    let before = files(root.path())?;
    let replay = migrate(receipt.plan.id)?;
    ensure!(
        replay.status.success(),
        "migration replay failed: {replay:?}"
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&replay.stdout)?,
        serde_json::to_value(&actual)?
    );
    assert!(files(root.path())? == before);
    let ordinary = run(&requests(&binary, root.path(), false))?;
    ensure!(
        !ordinary.status.success()
            && String::from_utf8_lossy(&ordinary.stderr).contains("catalog upgrade is pending"),
        "migration admitted an ordinary reader: {ordinary:?}"
    );
    CatalogUpgrade::resume_for_installer(root.path(), actual.plan.installer_identity)?.restore()?;
    assert!(
        run(&requests(&binary, root.path(), false))?
            .status
            .success()
    );
    Ok(())
}

#[test]
fn native_migration_refuses_another_installer_before_database_access() -> Result<()> {
    let binary = candidate();
    let root = fixture()?;
    let owner = CatalogUpgrade::acquire(root.path())?;
    let receipt =
        airk_kernel::service_catalog_upgrade::prepare(&owner, Sha256Digest::from_bytes([202; 32]))?;
    drop(owner);
    let before = files(root.path())?;
    let mut command = Command::new(&binary);
    command
        .args(["runtime", "catalog-upgrade-migrate", "--control-root"])
        .arg(root.path())
        .arg("--upgrade-id")
        .arg(receipt.plan.id.to_string());
    let output = run(&command)?;
    ensure!(
        !output.status.success()
            && String::from_utf8_lossy(&output.stderr)
                .contains("another verified installer payload"),
        "crossed installer entered migration: {output:?}"
    );
    assert!(files(root.path())? == before);
    assert_eq!(
        upgrade::pending(root.path())?.unwrap().stage,
        upgrade::UpgradeStage::Admission
    );
    Ok(())
}

fn run(command: &Command) -> Result<Output> {
    // native read: installer probes are single-shot, under held ownership leases;
    // negative reads and writer refusals must never be retried into success.
    Ok(read_retry::once_until(
        command,
        Instant::now() + Duration::from_secs(30),
    )?)
}
fn requests(binary: &Path, root: &Path, write: bool) -> Command {
    let mut command = Command::new(binary);
    command
        .args(["service", "requests", "--runtime-dir"])
        .arg(root);
    if write {
        command.args(["forget", "11111111111111111111111111111111"]);
    } else {
        command.arg("list");
    }
    command
}
fn probe(binary: &Path, root: &Path, id: Incarnation, nonce: Incarnation) -> Result<Output> {
    let mut command = Command::new(binary);
    command
        .args(["runtime", "catalog-upgrade-probe", "--control-root"])
        .arg(root)
        .arg("--upgrade-id")
        .arg(id.to_string())
        .arg("--nonce")
        .arg(nonce.to_string());
    run(&command)
}
fn files(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    fn collect(root: &Path, path: &Path, entries: &mut BTreeMap<PathBuf, Vec<u8>>) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                collect(root, &entry.path(), entries)?;
            } else {
                // Ownership locks are fixed empty files. Windows locks a byte
                // beyond EOF; do not issue a read against the held lock range.
                let bytes = if entry.metadata()?.len() == 0 {
                    Vec::new()
                } else {
                    fs::read(entry.path())?
                };
                entries.insert(entry.path().strip_prefix(root)?.into(), bytes);
            }
        }
        Ok(())
    }
    let mut entries = BTreeMap::new();
    collect(root, root, &mut entries)?;
    Ok(entries)
}

#[test]
fn actual_candidate_probe_preserves_the_complete_observation_and_refuses_a_crossed_payload()
-> Result<()> {
    let binary = candidate();
    for crossed in [false, true] {
        let root = fixture()?;
        let owner = CatalogUpgrade::acquire(root.path())?;
        let identity = if crossed {
            Sha256Digest::from_bytes([3; 32])
        } else {
            airk_cli::runtime_pair::engine_identity_at(&binary)?
        };
        let receipt = airk_kernel::service_catalog_upgrade::prepare(&owner, identity)?;
        owner.migrate()?;
        let before = files(root.path())?;
        let nonce = Incarnation::from_bytes([4; 16]);
        let output = probe(&binary, root.path(), receipt.plan.id, nonce)?;
        if crossed {
            ensure!(
                !output.status.success(),
                "crossed candidate was accepted: {output:?}"
            );
            ensure!(
                String::from_utf8_lossy(&output.stderr).contains("payload identity differs"),
                "wrong refusal: {output:?}"
            );
        } else {
            ensure!(
                output.status.success(),
                "candidate probe failed: {output:?}"
            );
            let actual: CatalogUpgradeObservation = serde_json::from_slice(&output.stdout)?;
            assert_eq!(
                actual,
                upgrade::observe(root.path(), receipt.plan.id, nonce)?
            );
            assert!(!actual.production_activation_enabled);
        }
        let ordinary = run(&requests(&binary, root.path(), false))?;
        ensure!(
            !ordinary.status.success()
                && String::from_utf8_lossy(&ordinary.stderr).contains("catalog upgrade is pending"),
            "ordinary reader crossed admission: {ordinary:?}"
        );
        assert_eq!(
            files(root.path())?,
            before,
            "probes cannot mutate control, collection or source state"
        );
        drop(owner);
        let mut service = Command::new(&binary);
        service.args(["serve", "--runtime-dir"]).arg(root.path());
        let output = run(&service)?;
        ensure!(
            !output.status.success()
                && String::from_utf8_lossy(&output.stderr).contains("catalog upgrade is pending"),
            "ordinary service started before catalog settlement: {output:?}"
        );
        assert_eq!(
            files(root.path())?,
            before,
            "pending startup must not create endpoints or start source/helper supervisors"
        );
    }
    Ok(())
}

fn predecessor_identity(binary: &Path) -> Result<Sha256Digest> {
    let directory = binary
        .parent()
        .context("predecessor payload directory missing")?;
    let pair: airk_cli::runtime_pair::RuntimePair =
        serde_json::from_slice(&fs::read(directory.join("airk-runtime.json"))?)?;
    ensure!(
        pair.airk_sha256 == airk_cli::runtime_pair::digest_file(binary)?,
        "predecessor main identity changed"
    );
    let helper = directory.join(if cfg!(windows) {
        "airk-embed.exe"
    } else {
        "airk-embed"
    });
    ensure!(
        pair.embed_sha256 == airk_cli::runtime_pair::digest_file(&helper)?,
        "predecessor helper identity changed"
    );
    Ok(framed_digest(
        b"airk-maintenance-engine/1.4",
        &[&serde_json::to_vec(&pair)?],
    ))
}

#[test]
#[ignore = "requires explicitly selected retained predecessor runtimes; no downloads or installed state"]
fn actual_predecessors_refuse_reads_and_writes_at_admission_and_the_schema_floor() -> Result<()> {
    let selected = std::env::var_os("AIRK_CATALOG_PREDECESSOR_BINARIES")
        .context("select exact retained predecessor binaries")?;
    let predecessors: Vec<_> = std::env::split_paths(&selected).collect();
    ensure!(
        !predecessors.is_empty() && predecessors.len() <= 8,
        "bounded predecessor selection required"
    );
    let root = fixture()?;
    for binary in &predecessors {
        for write in [false, true] {
            let output = run(&requests(binary, root.path(), write))?;
            ensure!(
                output.status.success(),
                "predecessor cannot execute the original schema-11 command: {output:?}"
            );
        }
    }
    let owner = CatalogUpgrade::acquire(root.path())?;
    let receipt = airk_kernel::service_catalog_upgrade::prepare(
        &owner,
        airk_cli::runtime_pair::engine_identity_at(&candidate())?,
    )?;
    let mut evidence = Vec::new();
    for migrated in [false, true] {
        if migrated {
            owner.migrate()?;
        }
        let before = files(root.path())?;
        for binary in &predecessors {
            let mut refusals = Vec::new();
            for write in [false, true] {
                let output = run(&requests(binary, root.path(), write))?;
                ensure!(
                    !output.status.success()
                        && String::from_utf8_lossy(&output.stderr).contains("catalog_upgrade"),
                    "expected actual owner-format refusal, not a parser/connection error: {output:?}"
                );
                refusals.push(framed_digest(
                    b"airk/catalog-predecessor-refusal/1",
                    &[&output.stderr],
                ));
            }
            if migrated {
                evidence.push(RuntimeRefusal {
                    payload_identity: predecessor_identity(binary)?,
                    read_refusal_digest: refusals[0],
                    write_refusal_digest: refusals[1],
                });
            }
        }
        assert_eq!(
            files(root.path())?,
            before,
            "predecessor refusal changed state"
        );
    }
    // Isolated format-floor fixture: remove only the admission field so the
    // retained reader must inspect schema 12. This never enables ordinary access.
    let owner_path = root.path().join("service-1.4.owner.json");
    let fenced = fs::read(&owner_path)?;
    let mut without_fence: serde_json::Value = serde_json::from_slice(&fenced)?;
    without_fence
        .as_object_mut()
        .unwrap()
        .remove("catalog_upgrade");
    fs::write(&owner_path, serde_json::to_vec(&without_fence)?)?;
    let before = files(root.path())?;
    for binary in &predecessors {
        for write in [false, true] {
            let output = run(&requests(binary, root.path(), write))?;
            ensure!(
                !output.status.success()
                    && String::from_utf8_lossy(&output.stderr)
                        .contains("unsupported saved service registry format"),
                "expected schema-floor refusal: {output:?}"
            );
        }
    }
    assert_eq!(files(root.path())?, before);
    fs::write(&owner_path, fenced)?;
    let output = probe(
        &candidate(),
        root.path(),
        receipt.plan.id,
        Incarnation::from_bytes([7; 16]),
    )?;
    ensure!(
        output.status.success(),
        "actual candidate probe failed: {output:?}"
    );
    let observation = serde_json::from_slice(&output.stdout)?;
    let ready = owner.record_readiness(ReadinessEvidence {
        schema: 1,
        candidate: observation,
        predecessor_refusals: evidence,
    })?;
    assert_eq!(ready.stage, upgrade::UpgradeStage::Readiness);
    assert!(
        owner.settle().is_err(),
        "production activation stays disabled at P5"
    );
    owner.restore()?;
    for binary in predecessors {
        let output = run(&requests(&binary, root.path(), false))?;
        ensure!(
            output.status.success(),
            "restored predecessor read failed: {output:?}"
        );
    }
    Ok(())
}

// Actual owned service, stdin CLI and trusted console all retain full canonical
// administrative results. This fixture does not qualify production activation.
fn exercise_native_catalog_transports(
    control: &Path,
    runtime: &tokio::runtime::Runtime,
) -> Result<()> {
    use airk_core::{catalog_management::*, index_catalog::*, service::RequestKey};
    use std::io::Write;
    let direct = |method| runtime.block_on(airk_cli::service_runtime::request(control, method));
    let cli = |method: &ControlMethod| -> Result<ControlResponse> {
        let mut child = Command::new(candidate())
            .args(["service", "rpc", "--runtime-dir"])
            .arg(control)
            .arg("--stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .context("missing RPC stdin")?
            .write_all(&serde_json::to_vec(method)?)?;
        let output = child.wait_with_output()?;
        ensure!(
            output.status.success(),
            "catalog RPC failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    let console = |method: ControlMethod| -> Result<ControlResponse> {
        let mut child = Command::new(candidate())
            .args(["service", "console", "--runtime-dir"])
            .arg(control)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let id = Incarnation::from_bytes([90; 16]);
        let mut input = child.stdin.take().context("missing console stdin")?;
        airk_cli::service_ipc::write_frame(
            &mut input,
            &airk_cli::console_client::Request { id, method },
        )?;
        drop(input);
        let output = child.wait_with_output()?;
        ensure!(
            output.status.success(),
            "catalog console failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let response: airk_cli::console_client::Response =
            airk_cli::service_ipc::read_frame(&mut output.stdout.as_slice())?
                .context("missing console response")?;
        ensure!(response.id == id, "console identity changed");
        Ok(response.response)
    };
    let equal = |a: ControlResponse, b: ControlResponse| -> Result<()> {
        ensure!(
            serde_json::to_value(a)? == serde_json::to_value(b)?,
            "canonical catalog results differ"
        );
        Ok(())
    };
    let page = ControlMethod::CatalogPage {
        request: CatalogPageRequest {
            kind: CatalogKind::Projects,
            index_id: None,
            project_id: None,
            after: None,
            expected_catalog_revision: None,
            limit: 1,
            max_bytes: MAX_PAGE_BYTES,
        },
    };
    equal(direct(page.clone())?, cli(&page)?)?;
    equal(direct(page.clone())?, console(page.clone())?)?;
    let ControlResponse::CatalogPage { page } = direct(page)? else {
        anyhow::bail!("catalog page unavailable")
    };
    let ControlResponse::RequestEpoch { namespace, epoch } = direct(ControlMethod::RequestEpoch)?
    else {
        anyhow::bail!("catalog epoch unavailable")
    };
    let id = QueryProjectId::from_identity(Incarnation::from_bytes([91; 16]));
    let request = CatalogRequest {
        caller: Incarnation::from_bytes([92; 16]),
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([93; 16]),
        },
        expected_catalog_revision: page.revision,
        operation: CatalogOperation::CreateProject {
            id,
            name: "Native catalog fixture".into(),
        },
    };
    let preview_method = ControlMethod::CatalogPreview {
        request: request.clone(),
    };
    let preview = cli(&preview_method)?;
    equal(direct(preview_method.clone())?, console(preview_method)?)?;
    let ControlResponse::CatalogPreview {
        preview,
        preview_digest,
    } = preview
    else {
        anyhow::bail!("catalog preview unavailable")
    };
    ensure!(
        !preview.authority_fence_required,
        "create unexpectedly requires authority"
    );
    ensure!(
        preview_digest == preview.identity()?,
        "native preview digest differs"
    );
    let apply = ControlMethod::CatalogApply {
        request: request.clone(),
        preview_digest,
    };
    let invalid = cli(&ControlMethod::CatalogApply {
        request: request.clone(),
        preview_digest: Sha256Digest::from_bytes([0; 32]),
    })?;
    ensure!(
        matches!(invalid, ControlResponse::Failure { .. }),
        "changed preview was accepted"
    );
    ensure!(
        matches!(
            direct(ControlMethod::CatalogReceipt {
                caller: request.caller,
                key: request.key.clone()
            })?,
            ControlResponse::CatalogReceipt { receipt: None }
        ),
        "failed review committed a receipt"
    );
    let applied = console(apply.clone())?;
    let ControlResponse::CatalogApplied { ref receipt } = applied else {
        anyhow::bail!("catalog apply unavailable")
    };
    ensure!(
        receipt.request_digest == request.identity()?,
        "receipt lost request identity"
    );
    let revision = receipt.catalog_revision;
    // Reconciliation and explicit keyed replay preserve the original receipt.
    equal(applied, cli(&apply)?)?;
    let receipt = ControlMethod::CatalogReceipt {
        caller: request.caller,
        key: request.key.clone(),
    };
    equal(direct(receipt.clone())?, cli(&receipt)?)?;
    equal(direct(receipt.clone())?, console(receipt)?)?;
    let detail = ControlMethod::CatalogDetail {
        entity: CatalogEntity::Project { id },
        expected_catalog_revision: Some(revision),
    };
    equal(direct(detail.clone())?, cli(&detail)?)?;
    equal(direct(detail.clone())?, console(detail)?)?;
    let host_context = ControlMethod::CatalogHostContext {
        key: Sha256Digest::from_bytes([95; 32]),
        expected_catalog_revision: Some(revision),
    };
    equal(direct(host_context.clone())?, cli(&host_context)?)?;
    equal(
        direct(host_context.clone())?,
        console(host_context.clone())?,
    )?;
    let ControlResponse::CatalogHostContext { descriptor } = direct(host_context.clone())? else {
        anyhow::bail!("native host context unavailable")
    };
    descriptor.validate()?;
    ensure!(
        descriptor.namespace == namespace
            && descriptor.catalog_revision == revision
            && descriptor.context.is_none(),
        "host context inferred an association or changed its snapshot"
    );
    let mut claimed_host = serde_json::to_value(&host_context)?;
    claimed_host["catalog_host_context"]["host"] = serde_json::to_value(descriptor.host)?;
    ensure!(
        serde_json::from_value::<ControlMethod>(claimed_host).is_err(),
        "native host context accepted a claimed wire host"
    );
    ensure!(
        matches!(
            direct(ControlMethod::CatalogHostContext {
                key: descriptor.key,
                expected_catalog_revision: Some(page.revision),
            })?,
            ControlResponse::Failure { .. }
        ),
        "host context ignored its expected catalog revision"
    );
    equal(direct(host_context.clone())?, console(host_context)?)?;
    let context = ControlMethod::CatalogContext {
        host: Sha256Digest::from_bytes([94; 32]),
        key: Sha256Digest::from_bytes([95; 32]),
        expected_catalog_revision: Some(revision),
    };
    equal(direct(context.clone())?, cli(&context)?)?;
    equal(direct(context.clone())?, console(context)?)?;
    let mut revoke = request.clone();
    revoke.key.nonce = Incarnation::from_bytes([96; 16]);
    revoke.expected_catalog_revision = revision;
    revoke.operation = CatalogOperation::DeleteProject {
        id,
        expected_definition_revision: CatalogRevision::INITIAL,
        expected_scope_revision: CatalogRevision::INITIAL,
        expected_defaults_revision: CatalogRevision::INITIAL,
    };
    let ControlResponse::CatalogPreview {
        preview,
        preview_digest,
    } = direct(ControlMethod::CatalogPreview {
        request: revoke.clone(),
    })?
    else {
        anyhow::bail!("delete preview unavailable")
    };
    ensure!(
        preview.authority_fence_required,
        "delete omitted authority fence"
    );
    let refusal = cli(&ControlMethod::CatalogApply {
        request: revoke.clone(),
        preview_digest,
    })?;
    ensure!(
        matches!(refusal, ControlResponse::Failure { code } if code == "catalog_authority_fence_unavailable"),
        "unfenced deletion was accepted"
    );
    ensure!(
        matches!(
            direct(ControlMethod::CatalogReceipt {
                caller: revoke.caller,
                key: revoke.key
            })?,
            ControlResponse::CatalogReceipt { receipt: None }
        ),
        "refused deletion committed a receipt"
    );
    ensure!(
        matches!(
            direct(ControlMethod::CatalogDetail {
                entity: CatalogEntity::Project { id },
                expected_catalog_revision: Some(revision)
            })?,
            ControlResponse::CatalogDetail {
                detail: CatalogDetail { item: Some(_), .. }
            }
        ),
        "refused deletion changed catalog"
    );
    Ok(())
}
