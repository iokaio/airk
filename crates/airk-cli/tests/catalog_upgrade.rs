// SPDX-License-Identifier: Apache-2.0
//! Process-level compatibility checks in disposable control state only. The
//! in-place catalog upgrade commands remain parseable but are refused.
mod common;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::IndexId,
    service::{ControlMethod, ControlResponse, ProjectRegistration},
};
use airk_store::service::registry::ServiceRegistry;
use anyhow::{Context, Result, ensure};
use common::{ctl::Ctl, read_retry};
use rusqlite::{Connection, params};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

// TempDir owns cleanup; only a newly created child is a native control root.
// Do not repair permissions on existing caller/shared/user directories.
struct PrivateFixture {
    _owner: tempfile::TempDir,
    control: PathBuf,
}
impl PrivateFixture {
    fn path(&self) -> &Path {
        &self.control
    }
}

fn fixture() -> Result<PrivateFixture> {
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
fn private_fixture() -> Result<PrivateFixture> {
    let owner = match std::env::var_os("AIRK_PRIVATE_CENSUS_BASE") {
        Some(base) => tempfile::Builder::new()
            .prefix("airk-catalog-upgrade-")
            .tempdir_in(base)?,
        None => tempfile::tempdir()?,
    };
    let control = owner.path().join("native-control");
    ensure!(
        !control.exists(),
        "native fixture control child already exists"
    );
    airk_cli::local_ipc::prepare_root(&control)?;
    Ok(PrivateFixture {
        _owner: owner,
        control,
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
    airk_cli::local_ipc::prepare_root(&control)?;
    for path in [&source, &data] {
        fs::create_dir(path)?;
    }
    let source = source.canonicalize()?;
    let data = data.canonicalize()?;
    fs::write(source.join("sentinel.txt"), b"source remains unchanged")?;
    drop(ServiceRegistry::open(
        &control,
        Incarnation::from_bytes([1; 16]),
    )?);
    activated_authority_zero(
        &control,
        airk_cli::runtime_pair::engine_identity_at(&candidate())?,
    )?;
    let log = owner.path().join("service.log");
    let mut command = Command::new(candidate());
    // The private foreground service role (split Phase 4; was `airk serve`).
    command
        .args(["__airk_role", "service-foreground", "--runtime-dir"])
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
    drop(registry);
    assert_eq!(
        fs::read(source.join("sentinel.txt"))?,
        b"source remains unchanged"
    );
    assert!(
        !data.join("collections-1.4/fixture").exists(),
        "catalog activation must not create index storage"
    );
    Ok(())
}
/// Format/transport fixture only: the real owned Genesis migration, then a
/// manual authority-0 activation with its admission fence cleared, exactly as
/// the former migration fixture did. This does not qualify managed activation
/// or bypass the production gate in the distributed command path.
fn activated_authority_zero(control: &Path, installer: Sha256Digest) -> Result<()> {
    use airk_store::service::{ServiceLease, registry::catalog::genesis::CatalogGenesis};
    let mut genesis = CatalogGenesis::begin(
        ServiceLease::acquire(control)?,
        installer,
        Incarnation::from_bytes([2; 16]),
    )?;
    genesis.backup()?;
    genesis.migrate()?;
    drop(genesis);
    let connection = Connection::open(control.join("service-1.4.sqlite3"))?;
    connection.execute("UPDATE index_catalog_meta SET activated=1", [])?;
    drop(connection);
    let record = control.join("service-1.4.owner.json");
    let mut metadata: serde_json::Value = serde_json::from_slice(&fs::read(&record)?)?;
    metadata
        .as_object_mut()
        .context("fixture owner record")?
        .remove("catalog_upgrade");
    fs::write(record, serde_json::to_vec(&metadata)?)?;
    Ok(())
}

fn candidate() -> PathBuf {
    std::env::var_os("AIRK_CATALOG_CANDIDATE_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_airk").into())
}

fn refused(output: &Output, command: &str) -> Result<()> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    ensure!(
        output.status.code() == Some(1)
            && stderr.contains("catalog_upgrade_unsupported")
            && !stderr.contains("Usage:"),
        "{command} was not refused as an unsupported in-place upgrade: {output:?}"
    );
    Ok(())
}

/// The hidden installer-only P5 commands and the explicit legacy-retirement
/// commands keep parsing but refuse before touching stdin, the installation or
/// any control state, whether or not a saved upgrade admission exists.
#[test]
fn in_place_upgrade_commands_are_refused_before_any_state_access() -> Result<()> {
    let binary = candidate();
    let nonce = Incarnation::from_bytes([4; 16]);
    let host = Sha256Digest::from_bytes([5; 32]);
    let control_commands = |control: &Path, id: Incarnation| {
        let mut migrate = Command::new(&binary);
        migrate
            .args([
                "__airk_role",
                "setup-interim",
                "runtime",
                "catalog-upgrade-migrate",
                "--control-root",
            ])
            .arg(control)
            .arg("--upgrade-id")
            .arg(id.to_string());
        let mut probe = Command::new(&binary);
        probe
            .args([
                "__airk_role",
                "setup-interim",
                "runtime",
                "catalog-upgrade-probe",
                "--control-root",
            ])
            .arg(control)
            .arg("--upgrade-id")
            .arg(id.to_string())
            .arg("--nonce")
            .arg(nonce.to_string());
        let mut authority = Command::new(&binary);
        authority
            .args([
                "__airk_role",
                "setup-interim",
                "runtime",
                "catalog-authority-probe",
                "--control-root",
            ])
            .arg(control)
            .arg("--operation")
            .arg(id.to_string())
            .arg("--native-host")
            .arg(host.to_string())
            .arg("--nonce")
            .arg(nonce.to_string());
        [
            ("catalog-upgrade-migrate", migrate),
            ("catalog-upgrade-probe", probe),
            ("catalog-authority-probe --native-host", authority),
        ]
    };
    // false: no admission. true: an in-place upgrade journal and its admission
    // fence retained from a predecessor; no current writer can produce them.
    for admitted in [false, true] {
        let root = fixture()?;
        let id = Incarnation::from_bytes([201; 16]);
        if admitted {
            // The predecessor's upgrade owner held the service lease.
            drop(airk_store::service::ServiceLease::acquire(root.path())?);
            fs::write(
                root.path().join("index-catalog-upgrade.json"),
                b"retained predecessor upgrade journal",
            )?;
            let record = root.path().join("service-1.4.owner.json");
            let mut metadata: serde_json::Value = serde_json::from_slice(&fs::read(&record)?)?;
            metadata["catalog_upgrade"] = serde_json::to_value(id)?;
            fs::write(record, serde_json::to_vec(&metadata)?)?;
        }
        let before = files(root.path())?;
        for (name, command) in control_commands(root.path(), id) {
            refused(&run(&command)?, name)?;
            assert!(files(root.path())? == before, "{name} changed state");
        }
        if admitted {
            // No client-side reader opens the store after the split (airkctl
            // reaches only the service), so only service startup is checked.
            let mut service = Command::new(&binary);
            service
                .args(["__airk_role", "service-foreground", "--runtime-dir"])
                .arg(root.path());
            let output = run(&service)?;
            ensure!(
                !output.status.success()
                    && String::from_utf8_lossy(&output.stderr)
                        .contains("catalog upgrade is pending"),
                "ordinary service started before catalog settlement: {output:?}"
            );
            assert_eq!(
                files(root.path())?,
                before,
                "pending startup must not create endpoints or start source/helper supervisors"
            );
        }
    }
    let owner = tempfile::tempdir()?;
    let missing = owner.path().join("not-installed");
    let index = IndexId::from_identity(Incarnation::from_bytes([6; 16])).to_string();
    for (action, extra) in [
        ("legacy-retirement-candidates", vec![]),
        (
            "legacy-retirement-review",
            vec!["--index-id", index.as_str()],
        ),
        ("legacy-retirement-authorize", vec![]),
        ("legacy-retirement-receipt", vec![]),
        ("legacy-retirement-resume", vec![]),
    ] {
        let mut command = Command::new(&binary);
        command
            .args([
                "__airk_role",
                "setup-interim",
                "runtime",
                action,
                "--install-dir",
            ])
            .arg(&missing)
            .args(extra);
        refused(&run(&command)?, action)?;
        assert!(!missing.exists(), "{action} created a missing installation");
    }
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

// Actual owned service and the airkctl metadata relay both retain full canonical
// administrative results. This fixture does not qualify production activation.
fn exercise_native_catalog_transports(
    control: &Path,
    runtime: &tokio::runtime::Runtime,
) -> Result<()> {
    use airk_core::{catalog_management::*, index_catalog::*, service::RequestKey};
    let direct = |method| runtime.block_on(airk_cli::service_runtime::request(control, method));
    let cli = |method: &ControlMethod| -> Result<ControlResponse> {
        let output = Ctl::new()
            .args(["service", "rpc", "--runtime-dir"])
            .arg(control)
            .arg("--stdin")
            .input(serde_json::to_vec(method)?)
            .output()?;
        ensure!(
            output.status.success(),
            "catalog RPC failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    // The trusted console is retired (split Phase 4); its relay filter is the
    // one `airkctl service rpc` applies, so the relay comparison runs there.
    let console = |method: ControlMethod| cli(&method);
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
    let ControlResponse::CatalogPage { page, capabilities } = direct(page)? else {
        anyhow::bail!("catalog page unavailable")
    };
    ensure!(
        capabilities == airk_store::service::registry::catalog::capabilities(),
        "catalog transport lost native capabilities"
    );
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
    let ControlResponse::CatalogOutcomeUnknown {
        code,
        caller,
        key,
        request_digest,
        observation,
    } = invalid
    else {
        anyhow::bail!("changed preview did not retain its canonical uncertain outcome")
    };
    ensure!(
        code == "catalog_apply_outcome_unknown"
            && caller == request.caller
            && key == request.key
            && request_digest == request.identity()?
            && observation.is_none(),
        "changed preview lost original native request identity"
    );
    let ControlResponse::CatalogDetail {
        detail,
        capabilities,
    } = direct(ControlMethod::CatalogDetail {
        entity: CatalogEntity::Project { id },
        expected_catalog_revision: Some(request.expected_catalog_revision),
    })?
    else {
        anyhow::bail!("failed preview changed the native catalog clock")
    };
    ensure!(
        capabilities == airk_store::service::registry::catalog::capabilities(),
        "catalog detail lost native capabilities"
    );
    ensure!(
        detail.namespace == namespace
            && detail.revision == request.expected_catalog_revision
            && detail.item.is_none(),
        "failed preview changed catalog project metadata"
    );
    ensure!(
        matches!(
            direct(ControlMethod::CatalogReceipt {
                caller: request.caller,
                key: request.key.clone()
            })?,
            ControlResponse::CatalogReceipt {
                receipt: None,
                fence: None
            }
        ),
        "failed review committed a receipt"
    );
    let applied = console(apply.clone())?;
    let ControlResponse::CatalogApplied {
        ref receipt,
        fenced_receipt: None,
    } = applied
    else {
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
    let ControlResponse::CatalogHostContext {
        descriptor,
        capabilities,
    } = direct(host_context.clone())?
    else {
        anyhow::bail!("native host context unavailable")
    };
    descriptor.validate()?;
    ensure!(
        capabilities == airk_store::service::registry::catalog::capabilities()
            && capabilities.policy_contract >= 2,
        "native host context lost the connected runtime's split capability"
    );
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
            ControlResponse::CatalogReceipt {
                receipt: None,
                fence: None
            }
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
                detail: CatalogDetail { item: Some(_), .. },
                ..
            }
        ),
        "refused deletion changed catalog"
    );
    Ok(())
}
