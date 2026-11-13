// SPDX-License-Identifier: Apache-2.0
//! Installer coordinator for catalog Genesis. SQL stays in Store. A raw
//! registry is never upgraded in place: the clean break creates fresh state.
use super::*;
use airk_core::identity::{Sha256Digest, framed_digest};
use std::io::{Seek, SeekFrom};

mod authority;

pub(super) fn activate_rebuild_genesis(root: &Path, intent: &Intent) -> Result<()> {
    authority::activate_rebuild_genesis(root, intent)
}

const MAX_ERROR: u64 = 65536;

pub(super) fn activate_managed_genesis(
    installer: &InstallerAuthority<'_>,
    policy: &managed::Managed,
    origin: &airk_store::service::registry::catalog::genesis::ManagedGenesisOrigin,
    owner: &mut airk_store::service::registry::catalog::genesis::CatalogGenesis,
) -> Result<()> {
    authority::activate_managed(installer, policy, origin, owner)
}

/// Fresh activation settles Genesis before its first probe. An ordinary
/// update drains the service and requires the existing catalog; a registry
/// without one is refused instead of being upgraded in place.
pub(super) async fn activate(root: &Path, intent: &Intent) -> Result<()> {
    let (main, approval) = verify_runtime(root, &intent.candidate)?;
    let process_owner = manager(root, intent)?;
    if intent.manager {
        process_owner.stop()?;
    }
    if intent.prior.is_none() {
        // No ordinary service has been admitted by this fresh route. Genesis
        // acquires the actual service lease before initialization; a live owner
        // refuses rather than being inferred absent or adopted as fresh state.
        process_owner.require_stopped_for_maintenance()?;
        return authority::activate_fresh(root, intent);
    }
    drain(root, &main, &approval).await?;
    process_owner.require_stopped_for_maintenance()?;
    intent.validate(root)?;
    ensure!(
        intent.phase == Phase::Probe && selection(root)? == Some(intent.selected()),
        "catalog admission requires the committed exact candidate selection"
    );
    ensure!(
        layout::control(root)? == intent.control,
        "catalog control binding changed"
    );
    // An ordinary update still proves the exact predecessor payload it replaces.
    let prior = intent
        .prior
        .as_ref()
        .context("catalog admission requires a verified predecessor")?;
    verify_runtime(root, &prior.active)?;
    let admission = quiescent(root)?;
    admission.require_catalog_compatible(&main)?;
    // The caller probes the drained candidate again; there is no backfill.
    ensure!(admission.catalog.is_some(), CATALOG_UPGRADE_UNSUPPORTED);
    Ok(())
}

/// The payload's version is deliberately independent from this executable's
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
    let mut child = airk_host::native_process::ContainedChild::spawn(&mut command)?;
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
        for prior in [
            None,
            Some(Selection {
                schema: 1,
                active: "b".repeat(64),
                previous: None,
            }),
        ] {
            let intent = Intent {
                schema: 1,
                install: missing.clone(),
                control: missing.join("control"),
                candidate: "a".repeat(64),
                prior,
                manager: false,
                phase: Phase::Probe,
                checkpoint: None,
            };
            let error = activate(&missing, &intent).await.unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("runtime installation is missing"),
                "{error:#}"
            );
            assert!(!super::resume_catalog_before_lifecycle(&missing, None).await?);
            assert!(!missing.exists());
        }
        Ok(())
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
                airk_host::process_roles::install(&main)?;
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
                airk_host::install_layout::record_control(&service)?;
                let operation = airk_kernel::service_broker::new_instance_id()?;
                let namespace = if prefix >= 1 {
                    let saved = ServiceRegistry::open(&control, operation)?;
                    Some(saved.incarnation()?)
                } else {
                    None
                };
                if prefix >= 2 {
                    airk_host::install_layout::record_registry(&service, namespace.unwrap())?;
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
            let _ = control_client::request_for_version(
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
