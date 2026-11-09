// SPDX-License-Identifier: Apache-2.0
//! Installer coordinator for the gated catalog cutover. SQL stays in Store;
//! the exact candidate process performs migration before ordinary service start.
use super::*;
use airk_core::identity::{Sha256Digest, framed_digest};
use airk_store::service::registry::catalog::upgrade::{
    self, CatalogUpgrade, CatalogUpgradeObservation, ReadinessEvidence, RuntimeRefusal,
    UpgradeReceipt, UpgradeStage,
};
use std::io::{Seek, SeekFrom};

mod authority;

pub(super) fn activate_rebuild_genesis(root: &Path, intent: &Intent) -> Result<()> {
    authority::activate_rebuild_genesis(root, intent)
}

const MAX_RECEIPT: u64 = 32 * 1024 * 1024;
const MAX_ERROR: u64 = 65536;

pub(super) fn activate_managed_genesis(
    installer: &InstallerAuthority<'_>,
    policy: &managed::Managed,
    origin: &airk_store::service::registry::catalog::genesis::ManagedGenesisOrigin,
    owner: &mut airk_store::service::registry::catalog::genesis::CatalogGenesis,
) -> Result<()> {
    authority::activate_managed(installer, policy, origin, owner)
}

fn resolved_control(root: &Path, installer: Sha256Digest) -> Result<PathBuf> {
    match layout::control(root) {
        Ok(control) => Ok(control),
        Err(error) if error.is::<airk_store::service::registry::catalog::AdmissionPending>() => {
            layout::catalog_control(root, installer)
        }
        Err(error) => Err(error),
    }
}

pub(super) fn control_for_resume(root: &Path, intent: &Intent) -> Result<PathBuf> {
    intent.validate(root)?;
    ensure!(
        intent.phase == Phase::Probe
            && (selection(root)? == Some(intent.selected())
                || (intent.prior.is_none() && selection(root)?.is_none())),
        "pending catalog requires its exact installer activation"
    );
    let (main, approval) = verify_runtime(root, &intent.candidate)?;
    layout::catalog_control(
        root,
        runtime_pair::engine_identity_at_version(&main, &approval.version)?,
    )
}

/// The production switch admits only the owned installer transition. Normal
/// settled-state reads add no metadata scan or executable qualification probe.
pub(super) async fn begin_if_enabled(root: &Path, intent: &Intent) -> Result<bool> {
    if !upgrade::PRODUCTION_ACTIVATION_ENABLED {
        return Ok(false);
    }
    activate(root, intent).await
}

pub(super) async fn resume_if_enabled(root: &Path, intent: &Intent) -> Result<bool> {
    if !upgrade::PRODUCTION_ACTIVATION_ENABLED {
        return Ok(false);
    }
    if let Some(receipt) =
        airk_store::service::registry::catalog::genesis::pending(&intent.control)?
        && receipt.stage != airk_store::service::registry::catalog::genesis::GenesisStage::Settled
    {
        return activate(root, intent).await;
    }
    let Some(receipt) = upgrade::pending(&intent.control)? else {
        return Ok(false);
    };
    if matches!(
        receipt.stage,
        UpgradeStage::Settled | UpgradeStage::Restored
    ) {
        return Ok(false);
    }
    if receipt.stage == UpgradeStage::Restoring {
        return resume_restore(root, intent).await;
    }
    activate(root, intent).await
}

async fn resume_restore(root: &Path, intent: &Intent) -> Result<bool> {
    intent.validate(root)?;
    let (main, approval) = verify_runtime(root, &intent.candidate)?;
    let installer = runtime_pair::engine_identity_at_version(&main, &approval.version)?;
    ensure!(
        intent.phase == Phase::Probe
            && selection(root)? == Some(intent.selected())
            && resolved_control(root, installer)? == intent.control,
        "catalog restore activation binding changed"
    );
    CatalogUpgrade::check_installer_resume(&intent.control, installer)?;
    let owner = manager(root, intent)?;
    if intent.manager {
        owner.stop()?;
    }
    if let Err(error) = drain(root, &main, &approval).await
        && !error.is::<airk_store::service::registry::catalog::AdmissionPending>()
    {
        return Err(error);
    }
    owner.require_stopped_for_maintenance()?;
    let upgrade = CatalogUpgrade::resume_for_installer(&intent.control, installer)?;
    ensure!(
        upgrade::pending(&intent.control)?
            .is_some_and(|receipt| receipt.stage == UpgradeStage::Restoring),
        "accepted catalog restore was superseded"
    );
    upgrade.restore()?;
    Ok(true) // Probe the compatible candidate; do not readmit migration.
}

async fn activate(root: &Path, intent: &Intent) -> Result<bool> {
    let (main, approval) = verify_runtime(root, &intent.candidate)?;
    let process_owner = manager(root, intent)?;
    if intent.prior.is_none() {
        if intent.manager {
            process_owner.stop()?;
        }
        // No ordinary service has been admitted by this fresh route. Genesis
        // acquires the actual service lease before initialization; a live owner
        // refuses rather than being inferred absent or adopted as fresh state.
        process_owner.require_stopped_for_maintenance()?;
        authority::activate_fresh(root, intent)?;
        return Ok(true);
    }
    let pending = upgrade::pending(&intent.control)?;
    if pending.as_ref().is_some_and(|receipt| {
        !matches!(
            receipt.stage,
            UpgradeStage::Settled | UpgradeStage::Restored
        )
    }) {
        ensure!(
            resolved_control(
                root,
                runtime_pair::engine_identity_at_version(&main, &approval.version)?
            )? == intent.control,
            "catalog activation state binding changed"
        );
    }
    if intent.manager {
        process_owner.stop()?;
    }
    if pending.as_ref().is_none_or(|receipt| {
        matches!(
            receipt.stage,
            UpgradeStage::Settled | UpgradeStage::Restored
        )
    }) {
        drain(root, &main, &approval).await?;
    } else if pending
        .as_ref()
        .is_some_and(|receipt| receipt.stage == UpgradeStage::Admission)
    {
        // The intent can survive a crash just before arming its owner fence.
        // Drain a still-compatible service; an already armed fence is expected.
        if let Err(error) = drain(root, &main, &approval).await
            && !error.is::<airk_store::service::registry::catalog::AdmissionPending>()
        {
            return Err(error);
        }
    }
    process_owner.require_stopped_for_maintenance()?;
    // Match existing installer maintenance: keep this accepted operation and
    // installation lock together, with no detached migration task that could
    // outlive cancellation of its enclosing installer future. The service is
    // stopped and normal admission stays fenced throughout the synchronous work.
    let Some((owner, receipt)) = prepare_readiness(root, intent)? else {
        return Ok(true); // We drained the service; the caller must probe/restart it.
    };
    authority::settle_upgrade(root, intent, owner, receipt)?;
    Ok(true)
}

/// Caller holds installer ownership and has stopped its saved OS manager.
/// Returning the upgrade owner retains all leases through the settlement choice.
/// P5 fixtures exercise this exact implementation without enabling production.
fn prepare_readiness(
    root: &Path,
    intent: &Intent,
) -> Result<Option<(CatalogUpgrade, UpgradeReceipt)>> {
    intent.validate(root)?;
    ensure!(
        intent.phase == Phase::Probe && selection(root)? == Some(intent.selected()),
        "catalog admission requires the committed exact candidate selection"
    );
    manager(root, intent)?.require_stopped_for_maintenance()?;
    let (main, approval) = verify_runtime(root, &intent.candidate)?;
    let installer = runtime_pair::engine_identity_at_version(&main, &approval.version)?;
    ensure!(
        resolved_control(root, installer)? == intent.control,
        "catalog control binding changed"
    );
    let prior = intent
        .prior
        .as_ref()
        .context("catalog upgrade requires a verified predecessor")?;
    let (predecessor, _) = verify_runtime(root, &prior.active)?;
    let predecessor_identity = engine_identity(&predecessor)?;
    let saved = upgrade::pending(&intent.control)?;
    if let Some(receipt) = &saved
        && receipt.authority_v1_host.is_some()
        && receipt.plan.installer_identity == installer
        && matches!(
            receipt.stage,
            UpgradeStage::Readiness | UpgradeStage::Settled
        )
    {
        // Post-seal recovery must consume its authority-1 evidence. Re-running
        // the P5 zero-grant import lane would reject or overwrite that seal.
        let owner = CatalogUpgrade::resume_for_installer(&intent.control, installer)?;
        return Ok(Some((owner, receipt.clone())));
    }
    let owner = if saved.as_ref().is_some_and(|receipt| {
        !matches!(
            receipt.stage,
            UpgradeStage::Settled | UpgradeStage::Restored
        )
    }) {
        ensure!(
            installer != predecessor_identity,
            "catalog predecessor equals candidate"
        );
        CatalogUpgrade::resume_for_installer(&intent.control, installer)?
    } else {
        let admission = quiescent(root)?;
        admission.require_catalog_compatible(&main)?;
        if admission.catalog.is_some() {
            return Ok(None); // A subsequent supported runtime update needs no backfill.
        }
        ensure!(
            installer != predecessor_identity,
            "catalog predecessor equals candidate"
        );
        ensure!(
            saved.is_none(),
            "catalog snapshot was restored; explicit reviewed retry is required"
        );
        ensure!(
            !data_migration::needed(root)?,
            "finish legacy directory relocation before catalog admission"
        );
        let Quiescence {
            _service: service,
            _projects: projects,
            _embedding: embedding,
            _source_watch: watch,
            ..
        } = admission;
        drop(projects);
        drop(embedding);
        drop(watch);
        CatalogUpgrade::acquire_with_service_for_installer(service, installer)?
    };
    if let Some(receipt) = &saved
        && receipt.stage == UpgradeStage::Readiness
        && owner.catalog_state()?.is_some_and(|state| state.activated)
    {
        // U6 can commit activation or clear admission before its receipt.
        // Resume settlement without rearming admission or rerunning U3.
        return Ok(Some((owner, receipt.clone())));
    }
    let admitted = airk_kernel::service_catalog_upgrade::prepare(&owner, installer)?;
    ensure!(
        !matches!(
            admitted.stage,
            UpgradeStage::Restoring | UpgradeStage::Restored | UpgradeStage::Settled
        ),
        "catalog recovery must be settled explicitly before migration"
    );
    // The durable owner fence protects the handoff to the native candidate. An
    // unexpected owner blocks its acquisition rather than permitting SQL work.
    drop(owner);
    let mut migration = Command::new(&main);
    migration
        .args(["runtime", "catalog-upgrade-migrate", "--control-root"])
        .arg(&intent.control)
        .arg("--upgrade-id")
        .arg(admitted.plan.id.to_string());
    let output = capture(
        &intent.control,
        migration,
        MAX_RECEIPT + 1,
        Duration::from_secs(300),
    )?;
    require_success(
        &output,
        "candidate catalog migration failed; resume the recorded upgrade",
    )?;
    let migrated: UpgradeReceipt = serde_json::from_slice(&output.stdout)?;
    ensure!(
        migrated.plan == admitted.plan
            && matches!(
                migrated.stage,
                UpgradeStage::Imports | UpgradeStage::Readiness
            ),
        "candidate returned another migration receipt"
    );
    let owner = CatalogUpgrade::resume_for_installer(&intent.control, installer)?;
    let current =
        upgrade::pending(&intent.control)?.context("catalog migration receipt missing")?;
    ensure!(
        serde_json::to_value(&current)? == serde_json::to_value(&migrated)?,
        "candidate migration receipt is stale or incomplete"
    );
    let native_host = crate::catalog_host_context::current_native_host(current.plan.namespace)?;
    let current = owner.bind_authority_v1_host(native_host)?;
    let nonce = current
        .readiness
        .as_ref()
        .map(|evidence| evidence.candidate.nonce)
        .map_or_else(airk_kernel::service_broker::new_instance_id, Ok)?;
    let mut probe = Command::new(&main);
    probe
        .args(["runtime", "catalog-upgrade-probe", "--control-root"])
        .arg(&intent.control)
        .arg("--upgrade-id")
        .arg(current.plan.id.to_string())
        .arg("--nonce")
        .arg(nonce.to_string());
    let observed = capture(&intent.control, probe, 16384, Duration::from_secs(30))?;
    require_success(&observed, "candidate catalog readiness failed")?;
    let candidate: CatalogUpgradeObservation = serde_json::from_slice(&observed.stdout)?;
    ensure!(
        candidate.installer_identity == installer && candidate.nonce == nonce,
        "candidate readiness identity changed"
    );
    if let Some(evidence) = &current.readiness {
        // Keep the original refusal observations on an exact replay. Rechecking
        // the candidate snapshot and immutable predecessor identity is enough;
        // rerunning a failed mutator could produce different diagnostic bytes.
        ensure!(
            candidate == evidence.candidate
                && evidence.predecessor_refusals.len() == 1
                && evidence.predecessor_refusals[0].payload_identity == predecessor_identity,
            "saved catalog readiness was superseded; review the changed state"
        );
        return Ok(Some((owner, current)));
    }

    let predecessor_refusals = authority::preseal_refusals(root, intent, current.plan.id, nonce)?;
    let ready = owner.record_readiness(ReadinessEvidence {
        schema: 1,
        candidate,
        predecessor_refusals,
    })?;
    Ok(Some((owner, ready)))
}

fn registry_refusal(error: &str) -> bool {
    !error.contains("Usage:")
        && !error.contains("unexpected argument")
        && !error.contains("unrecognized subcommand")
        && (error.contains("unknown field `catalog_upgrade`")
            || error.contains("unsupported saved service registry format")
            || error.contains("unsupported service registry format"))
}

/// Predecessor version is deliberately independent from this executable's
/// version. The installer has already admitted its exact inventory and helper.
fn engine_identity(main: &Path) -> Result<Sha256Digest> {
    let directory = runtime_pair::payload_root(main)?;
    let pair: runtime_pair::RuntimePair =
        serde_json::from_slice(&read_bounded(&directory.join("airk-runtime.json"), 4096)?)?;
    ensure!(
        matches!(pair.schema, 1 | 2)
            && pair.airk_sha256 == runtime_pair::digest_file(main)?
            && (pair.is_single_executable()
                || pair.embed_sha256 == runtime_pair::digest_file(&directory.join(helper_name()))?),
        "predecessor pair identity changed"
    );
    Ok(framed_digest(
        b"airk-maintenance-engine/1.4",
        &[&serde_json::to_vec(&pair)?],
    ))
}

struct Captured {
    success: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}
fn require_success(output: &Captured, context: &str) -> Result<()> {
    ensure!(
        output.success,
        "{context}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// Private temporary output files avoid pipe/descendant deadlocks and reader
/// threads. Monitor their sizes while waiting, drain owned descendants before
/// reading, and never silently truncate a receipt or error.
fn capture(
    control: &Path,
    mut command: Command,
    maximum: u64,
    timeout: Duration,
) -> Result<Captured> {
    let mut stdout = tempfile::tempfile_in(control)?;
    let mut stderr = tempfile::tempfile_in(control)?;
    command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?);
    hidden(&mut command);
    let mut child = crate::native_process::ContainedChild::spawn(&mut command)?;
    let until = std::time::Instant::now() + timeout;
    let success = loop {
        ensure!(
            stdout.metadata()?.len() <= maximum && stderr.metadata()?.len() <= MAX_ERROR,
            "catalog native command output exceeded its byte allowance"
        );
        if let Some(status) = child.try_wait()? {
            break status.success();
        }
        ensure!(
            std::time::Instant::now() < until,
            "catalog native command timed out; inspect the saved upgrade before resuming"
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    drop(child);
    let read = |file: &mut File, maximum| -> Result<Vec<u8>> {
        ensure!(
            file.metadata()?.len() <= maximum,
            "catalog native command output grew beyond its byte allowance"
        );
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        file.take(maximum + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= maximum,
            "catalog native command output exceeded its byte allowance"
        );
        Ok(bytes)
    };
    Ok(Captured {
        success,
        stdout: read(&mut stdout, maximum)?,
        stderr: read(&mut stderr, MAX_ERROR)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predecessor_parser_and_connection_errors_are_not_registry_refusal_evidence() {
        for error in [
            "unknown field `catalog_upgrade`, expected `root`",
            "unsupported saved service registry format",
            "unsupported service registry format",
        ] {
            assert!(registry_refusal(error), "{error}");
        }
        for error in [
            "cannot connect to catalog_upgrade",
            "unexpected argument: catalog_upgrade\nUsage: airk service",
            "unrecognized subcommand 'unsupported service registry format'",
            "Usage: unknown field `catalog_upgrade`",
        ] {
            assert!(!registry_refusal(error), "{error}");
        }
    }

    fn fixture_command(mode: &str) -> Result<Command> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "runtime_install::activation::catalog_upgrade::tests::native_output_fixture",
                "--nocapture",
            ])
            .env("AIRK_CATALOG_OUTPUT_FIXTURE", mode);
        Ok(command)
    }

    #[test]
    fn native_output_fixture() {
        match std::env::var("AIRK_CATALOG_OUTPUT_FIXTURE").as_deref() {
            Ok("complete") => {
                print!("complete native receipt");
                eprint!("complete native diagnostic");
            }
            Ok("oversize") => {
                std::io::stdout().write_all(&vec![b'x'; 8192]).unwrap();
            }
            Ok("wait") => std::thread::sleep(Duration::from_secs(60)),
            _ => {}
        }
    }

    #[test]
    fn native_output_is_complete_bounded_and_deadline_drained() -> Result<()> {
        let root = tempfile::tempdir()?;
        let output = capture(
            root.path(),
            fixture_command("complete")?,
            4096,
            Duration::from_secs(10),
        )?;
        assert!(output.success);
        assert!(String::from_utf8_lossy(&output.stdout).contains("complete native receipt"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("complete native diagnostic"));
        let overflow = capture(
            root.path(),
            fixture_command("oversize")?,
            512,
            Duration::from_secs(10),
        )
        .err()
        .context("oversized receipt accepted")?;
        assert!(
            overflow.to_string().contains("byte allowance"),
            "{overflow:#}"
        );
        let timeout = capture(
            root.path(),
            fixture_command("wait")?,
            4096,
            Duration::from_millis(200),
        )
        .err()
        .context("blocked command accepted")?;
        assert!(timeout.to_string().contains("timed out"), "{timeout:#}");
        assert_eq!(
            fs::read_dir(root.path())?.count(),
            0,
            "private output files must be removed after success/refusal/timeout"
        );
        Ok(())
    }

    #[tokio::test]
    async fn missing_installation_cannot_admit_catalog_work_or_create_state() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let missing = temporary.path().join("must-not-be-created");
        let intent = Intent {
            schema: 1,
            install: missing.clone(),
            control: missing.join("control"),
            candidate: "a".repeat(64),
            prior: Some(Selection {
                schema: 1,
                active: "b".repeat(64),
                previous: None,
            }),
            manager: false,
            phase: Phase::Probe,
            checkpoint: None,
        };
        let begin = begin_if_enabled(&missing, &intent).await;
        if upgrade::PRODUCTION_ACTIVATION_ENABLED {
            assert!(
                begin
                    .unwrap_err()
                    .to_string()
                    .contains("runtime installation is missing")
            );
        } else {
            assert!(!begin?);
        }
        let resume = resume_if_enabled(&missing, &intent).await;
        if upgrade::PRODUCTION_ACTIVATION_ENABLED {
            assert!(
                resume.is_err(),
                "missing native control cannot authorize recovery"
            );
        } else {
            assert!(!resume?);
        }
        assert!(!super::resume_catalog_before_lifecycle(&missing, None).await?);
        assert!(!missing.exists());
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires an explicitly approved fresh pair and retained predecessor; isolated installation only"]
    async fn native_installer_catalog_handoff_replays_readiness_and_restores_without_rebuild()
    -> Result<()> {
        use crate::runtime_install::native_connection_tests::NativeFixture;
        use airk_core::service::ProjectRegistration;
        use airk_store::service::registry::ServiceRegistry;
        let fixture = NativeFixture::new().await?;
        let result = async {
            let prior_source = PathBuf::from(
                std::env::var_os("AIRK_CATALOG_PREDECESSOR_RUNTIME")
                    .context("select retained predecessor runtime")?,
            );
            let prior_approved = std::env::var("AIRK_CATALOG_PREDECESSOR_APPROVED_SHA256")
                .context("select independently recorded predecessor approval")?;
            let prior = stage(
                &prior_source,
                &fixture.root,
                &prior_approved,
                VerificationBasis::OrganizationDigest,
            )?;
            let _installer = install_lock(&fixture.root)?;
            let (main, approval) = verify_runtime(&fixture.root, &fixture.identity)?;
            drain(&fixture.root, &main, &approval).await?;
            let base = fixture.root.parent().context("fixture parent missing")?;
            let source = base.join("catalog-source");
            let data = base.join("catalog-data");
            fs::create_dir(&source)?;
            fs::create_dir(&data)?;
            let source = source.canonicalize()?;
            let data = data.canonicalize()?;
            fs::write(source.join("sentinel"), b"unchanged source bytes")?;
            fs::write(
                data.join("opaque-retained-sentinel"),
                b"unchanged retained bytes",
            )?;
            let mut registry = ServiceRegistry::open(
                &fixture.control,
                airk_kernel::service_broker::new_instance_id()?,
            )?;
            let namespace = registry.saved_request_epoch()?.0;
            airk_store::service::data_scope::bind(&data, &fixture.control, namespace)?;
            let registration = ProjectRegistration::new(
                source.to_str().unwrap().into(),
                data.to_str().unwrap().into(),
                "catalog-fixture".into(),
            )?;
            registry.register(&registration)?;
            registry.pause(registration.binding, true)?;
            assert!(registry.get(registration.binding)?.paused);
            drop(registry);
            let intent = Intent {
                schema: 1,
                install: fixture.root.canonicalize()?,
                control: fixture.control.clone(),
                candidate: fixture.identity.clone(),
                prior: Some(Selection {
                    schema: 1,
                    active: prior.manifest_sha256,
                    previous: None,
                }),
                manager: false,
                phase: Phase::Probe,
                checkpoint: None,
            };
            replace_json(&fixture.root, "selected.json", &intent.selected())?;
            intent.save(&fixture.root)?;
            let (owner, ready) = prepare_readiness(&fixture.root, &intent)?
                .context("catalog upgrade did not prepare")?;
            assert_eq!(ready.stage, UpgradeStage::Readiness);
            assert_eq!(ready.plan.indexes.len(), 1);
            assert_eq!(
                ready.readiness.as_ref().unwrap().predecessor_refusals.len(),
                1
            );
            assert!(owner.settle().is_err(), "P5 cannot activate normal state");
            drop(owner);
            let (owner, replay) =
                prepare_readiness(&fixture.root, &intent)?.context("catalog resume missing")?;
            assert_eq!(
                serde_json::to_value(&ready)?,
                serde_json::to_value(&replay)?
            );
            // Boundary-format fixtures only. Actual SQL cutover interruption
            // uses Store's settlement tests; P6 must qualify the enabled consumer
            // path. Here prove installer dispatch never reruns U3 after U6.
            let database = fixture.control.join("service-1.4.sqlite3");
            rusqlite::Connection::open(&database)?
                .execute("UPDATE index_catalog_meta SET activated=1", [])?;
            drop(owner);
            let before = fs::read(&database)?;
            let (owner, resumed) = prepare_readiness(&fixture.root, &intent)?
                .context("activation-boundary resume missing")?;
            assert_eq!(
                serde_json::to_value(&ready)?,
                serde_json::to_value(&resumed)?
            );
            assert!(fs::read(&database)? == before);
            assert!(owner.catalog_state()?.unwrap().activated);
            drop(owner);
            let owner_path = fixture.control.join("service-1.4.owner.json");
            let mut owner_record: serde_json::Value =
                serde_json::from_slice(&fs::read(&owner_path)?)?;
            owner_record
                .as_object_mut()
                .unwrap()
                .remove("catalog_upgrade");
            replace_json(&fixture.control, "service-1.4.owner.json", &owner_record)?;
            let (owner, resumed) = prepare_readiness(&fixture.root, &intent)?
                .context("cleared-admission resume missing")?;
            assert_eq!(
                serde_json::to_value(&ready)?,
                serde_json::to_value(&resumed)?
            );
            assert!(fs::read(&database)? == before);
            owner.restore()?;
            drop(owner);
            // Simulate a crash after the successful restore's admission clear,
            // before its final external receipt, then use the real coordinator.
            let mut restoring =
                upgrade::pending(&fixture.control)?.context("restore receipt missing")?;
            assert_eq!(restoring.stage, UpgradeStage::Restored);
            restoring.stage = UpgradeStage::Restoring;
            replace_json(&fixture.control, "index-catalog-upgrade.json", &restoring)?;
            assert!(resume_restore(&fixture.root, &intent).await?);
            assert_eq!(
                upgrade::pending(&fixture.control)?.unwrap().stage,
                UpgradeStage::Restored
            );
            probe(&fixture.root, &approval, &main, false).await?;
            assert_eq!(
                fs::read(source.join("sentinel"))?,
                b"unchanged source bytes"
            );
            assert_eq!(
                fs::read(data.join("opaque-retained-sentinel"))?,
                b"unchanged retained bytes"
            );
            assert!(
                !data.join("collections-1.4").exists(),
                "metadata handoff cannot rebuild the paused index"
            );
            let registry = ServiceRegistry::open_saved(&fixture.control)?;
            assert!(registry.get(registration.binding)?.paused);
            assert!(registry.catalog_state()?.is_none());
            drop(registry);
            complete_selected(&fixture.root, &intent).await?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        let cleanup = fixture.cleanup().await;
        result.and(cleanup)
    }
}

#[cfg(test)]
mod copied_genesis_recovery_tests {
    use super::*;
    use airk_store::service::{
        ServiceLease,
        registry::{
            ServiceRegistry,
            catalog::genesis::{self, CatalogGenesis},
        },
    };

    #[tokio::test]
    #[ignore = "requires fresh sealed AIRK_F03_RUNTIME and explicit approval; run serially"]
    async fn actual_fresh_installer_resumes_native_namespace_and_genesis_prefixes() -> Result<()> {
        let source = PathBuf::from(
            std::env::var_os("AIRK_F03_RUNTIME").context("fresh sealed runtime required")?,
        )
        .canonicalize()?;
        let approved =
            std::env::var("AIRK_F03_APPROVED_SHA256").context("explicit approval required")?;
        ensure!(
            runtime_pair::digest_file(&source.join("airk-install.json"))? == approved,
            "approved runtime differs"
        );
        for prefix in 0..4 {
            let directory = tempfile::tempdir()?;
            let base = directory.path().canonicalize()?;
            let root = base.join("install");
            staging::stage_with_layout(
                &source,
                &root,
                &approved,
                VerificationBasis::OrganizationDigest,
                None,
                RuntimeChannel::Stable,
            )?;
            let control = layout::control(&root)?;
            let result = async {
                let lock = install_lock(&root)?;
                drop(quiescent(&root)?);
                let (main, approval) = verify_runtime(&root, &approved)?;
                let installer = runtime_pair::engine_identity_at_version(&main, &approval.version)?;
                launcher_repair::repair(&root, &approved)?;
                crate::process_roles::install(&main)?;
                let intent = Intent {
                    schema: 1,
                    install: root.canonicalize()?,
                    control: control.clone(),
                    candidate: approved.clone(),
                    prior: None,
                    manager: false,
                    phase: Phase::Probe,
                    checkpoint: None,
                };
                intent.save(&root)?;
                let service = ServiceLease::acquire(&control)?;
                layout::record_control(&service)?;
                let operation = airk_kernel::service_broker::new_instance_id()?;
                let namespace = if prefix >= 1 {
                    let saved = ServiceRegistry::open(&control, operation)?;
                    Some(saved.incarnation()?)
                } else {
                    None
                };
                if prefix >= 2 {
                    layout::record_registry(&service, namespace.unwrap())?;
                }
                if prefix == 3 {
                    let mut owner = CatalogGenesis::begin(service, installer, operation)?;
                    owner.backup()?;
                    owner.migrate()?;
                    drop(owner);
                } else {
                    drop(service);
                }
                assert!(!root.join("selected.json").exists());
                drop(lock);
                crate::runtime_install::activate(&root, &approved).await?;
                let receipt = genesis::pending(&control)?.context("resumed Genesis missing")?;
                assert_eq!(receipt.stage, genesis::GenesisStage::Settled);
                if let Some(namespace) = namespace {
                    assert_eq!(
                        receipt.namespace, namespace,
                        "resume replaced the original namespace"
                    );
                }
                assert_eq!(selection_record(&root)?.active, approved);
                let health = ensure_running(&root).await?;
                assert_eq!(
                    ServiceRegistry::open_saved(&control)?.recorded_service_instance()?,
                    Some(health.instance)
                );
                Ok::<_, anyhow::Error>(())
            }
            .await;
            let _ = service_runtime::request_for_version(
                &control,
                ControlMethod::Shutdown,
                env!("CARGO_PKG_VERSION"),
            )
            .await;
            let cleanup = wait_quiescent(&root).await;
            result
                .with_context(|| format!("copied native initialization prefix {prefix}"))
                .and(cleanup)?;
        }
        Ok(())
    }
}
