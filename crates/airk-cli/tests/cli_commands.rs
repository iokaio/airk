// SPDX-License-Identifier: Apache-2.0
//! Functional tests of the command workflows (through `airkctl`) and of the
//! pre-split operations kept behind `airk __airk_role setup-interim` (U93).
//! Parser coverage moved with the user CLI to `airkctl`'s help-catalog and
//! CLI-reference drift tests (split Phase 4).
use anyhow::{Context, Result, ensure};
mod common;
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

fn airk() -> Command {
    // Only Windows adjusts the process (no console window).
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(env!("CARGO_BIN_EXE_airk"));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

/// An in-process `airkctl` run (split Phase 4) for a command `airkctl` serves.
fn ctl_run(args: &[&str]) -> Result<Output> {
    Ok(common::ctl::Ctl::new().args(args).output()?)
}

fn ctl_json_ok(args: &[&str]) -> Result<Value> {
    let output = ctl_run(args)?;
    ensure!(
        output.status.success(),
        "airkctl {} failed: {}{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("airkctl {} printed no JSON", args.join(" ")))
}

fn replacement_json(owner: &Path, args: &[&str]) -> Result<Value> {
    let output = common::fixture_command(owner, args)?;
    ensure!(
        output.status.success(),
        "replacement fixture command failed: {}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout)
        .context("replacement fixture did not return complete native JSON")
}

#[test]
fn replacement_installation_flags_are_enforced_and_fresh_admission_replays_without_state()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let install = owner.path().join("install");
    let install = install.to_str().context("UTF8 install path")?;
    let candidate = "a".repeat(64);
    let operation = "b".repeat(64);
    let base = [
        "runtime",
        "replace-installation",
        "--install-dir",
        install,
        "--candidate-sha256",
        &candidate,
    ];
    for flags in [
        vec!["--reinstall"],
        vec!["--operation-id", &operation],
        vec!["--generation-id", &operation],
    ] {
        let args = base.iter().copied().chain(flags).collect::<Vec<_>>();
        let output = common::fixture_command(owner.path(), &args)?;
        ensure!(
            !output.status.success(),
            "incomplete installer authorization accepted: {args:?}"
        );
        ensure!(
            String::from_utf8_lossy(&output.stderr).contains("required"),
            "unexpected parser failure: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let mut args = base.to_vec();
    args.extend([
        "--reinstall",
        "--operation-id",
        &operation,
        "--generation-id",
        &operation,
        "--apply",
    ]);
    let admitted = replacement_json(owner.path(), &args)?;
    ensure!(
        admitted["replaced"] == false && admitted["plan"]["reason"] == "no_installation",
        "{admitted}"
    );
    ensure!(
        !Path::new(install).exists(),
        "admission must not create a service namespace or an index"
    );
    fs::create_dir(install)?;
    fs::write(
        Path::new(install).join("successor.txt"),
        b"do not reset this successor",
    )?;
    let replay = replacement_json(owner.path(), &args)?;
    ensure!(
        replay["replaced"] == false && replay["plan"]["reason"] == "installation_already_replaced",
        "{replay}"
    );
    ensure!(fs::read(Path::new(install).join("successor.txt"))? == b"do not reset this successor");
    Ok(())
}

#[test]
fn same_build_cli_replacement_removes_saved_indexes_and_reactivation_starts_empty() -> Result<()> {
    use airk_store::service::registry::ServiceRegistry;
    let mut owner = tempfile::tempdir()?;
    let mut source_owner = tempfile::tempdir()?;
    owner.disable_cleanup(true);
    source_owner.disable_cleanup(true);
    let owner_path = owner.path().canonicalize()?;
    let source = source_owner.path().canonicalize()?;
    struct StopOnDrop<'a> {
        install: std::path::PathBuf,
        owner: &'a mut tempfile::TempDir,
        source: &'a mut tempfile::TempDir,
        predecessor: Option<common::FixtureForeground>,
        passed: bool,
    }
    impl Drop for StopOnDrop<'_> {
        fn drop(&mut self) {
            let stopped = common::shutdown_installed_fixture(&self.install, self.owner.path());
            let child = self
                .predecessor
                .as_mut()
                .map(|child| child.require_exited().or_else(|_| child.shutdown_raw()))
                .transpose();
            let complete = match (stopped, child) {
                (Ok(()), Ok(_)) => Ok(()),
                (Err(stop), Err(child)) => Err(anyhow::anyhow!(
                    "canonical Stop: {stop:#}; owned predecessor: {child:#}"
                )),
                (Err(error), _) | (_, Err(error)) => Err(error),
            };
            match complete {
                Ok(()) if self.passed => {
                    self.owner.disable_cleanup(false);
                    self.source.disable_cleanup(false);
                }
                Ok(()) => eprintln!(
                    "failed replacement fixture retained after verified Stop: owner={}, source={}",
                    self.owner.path().display(),
                    self.source.path().display()
                ),
                Err(error) => {
                    let receipt = json!({"cleanup_complete":false,"install":self.install,"owner":self.owner.path(),"source":self.source.path(),"reason":format!("{error:#}")});
                    let recorded = serde_json::to_vec(&receipt)
                        .map_err(std::io::Error::other)
                        .and_then(|bytes| {
                            fs::write(self.owner.path().join("cleanup-incomplete.json"), bytes)
                        });
                    if let Err(record_error) = recorded {
                        eprintln!("could not retain fixture cleanup receipt: {record_error}");
                    }
                    eprintln!(
                        "replacement fixture cleanup incomplete; owner={}, source={} retained: {error:#}",
                        self.owner.path().display(),
                        self.source.path().display()
                    );
                }
            }
        }
    }
    let mut cleanup = StopOnDrop {
        install: owner_path.join("install"),
        owner: &mut owner,
        source: &mut source_owner,
        predecessor: None,
        passed: false,
    };
    let (install, control, _) = common::installed_runtime_fixture(&owner_path)?;
    let old_namespace = ServiceRegistry::open_saved(&control)?.incarnation()?;
    let selection: Value = serde_json::from_slice(&fs::read(install.join("selected.json"))?)?;
    let digest = selection["active"].as_str().context("selected digest")?;
    let data = owner_path.join("custom-indexes");
    fs::write(source.join("original.txt"), b"keep original source")?;
    let install_path = install.to_str().context("UTF8 install")?;
    cleanup.predecessor = Some(common::FixtureForeground::spawn(&install, &owner_path)?);
    cleanup.predecessor.as_mut().unwrap().wait_ready()?;
    let (cold_namespace, cold_resources) =
        common::observe_empty_installed_fixture(&install, &owner_path, "cold-before-setup")?;
    ensure!(
        cold_namespace == old_namespace,
        "cold baseline crossed predecessor namespace"
    );
    common::setup_fixture_project(&control, &source, &data, "before-reinstall")?;
    cleanup.predecessor.as_mut().unwrap().shutdown_raw()?;
    ensure!(
        !ServiceRegistry::open_saved(&control)?
            .bindings()?
            .is_empty()
    );
    let previous_selection = fs::read(install.join("selected.json"))?;
    let previous_registry = fs::read(control.join("service-1.4.sqlite3"))?;
    let different = if digest == "0".repeat(64) {
        "1".repeat(64)
    } else {
        "0".repeat(64)
    };
    let unidentified = common::fixture_command(
        &owner_path,
        &[
            "runtime",
            "replace-installation",
            "--install-dir",
            install_path,
            "--candidate-sha256",
            &different,
            "--apply",
        ],
    )?;
    ensure!(
        !unidentified.status.success()
            && String::from_utf8_lossy(&unidentified.stderr)
                .contains("replacement_operation_required"),
        "unidentified replacement was not refused: {}{}",
        String::from_utf8_lossy(&unidentified.stdout),
        String::from_utf8_lossy(&unidentified.stderr)
    );
    ensure!(
        fs::read(install.join("selected.json"))? == previous_selection
            && fs::read(control.join("service-1.4.sqlite3"))? == previous_registry
            && data.exists(),
        "refused replacement changed owned state"
    );
    cleanup.predecessor = Some(common::FixtureForeground::spawn(&install, &owner_path)?);
    cleanup.predecessor.as_mut().unwrap().wait_ready()?;
    let token = "c".repeat(64);
    let receipt = replacement_json(
        &owner_path,
        &[
            "runtime",
            "replace-installation",
            "--install-dir",
            install_path,
            "--candidate-sha256",
            digest,
            "--reinstall",
            "--operation-id",
            &token,
            "--generation-id",
            &token,
            "--apply",
        ],
    )?;
    ensure!(receipt["replaced"] == true, "{receipt}");
    cleanup.predecessor.as_mut().unwrap().require_exited()?;
    ensure!(
        !install.exists() && !data.exists(),
        "the old installation and external registered indexes survived"
    );
    let payload = owner_path.join("payload");
    let state = owner_path.join("state");
    replacement_json(
        &owner_path,
        &[
            "runtime",
            "stage",
            "--source",
            payload.to_str().unwrap(),
            "--install-dir",
            install_path,
            "--state-dir",
            state.to_str().unwrap(),
            "--approved-sha256",
            digest,
            "--verification-basis",
            "organization-digest",
        ],
    )?;
    replacement_json(
        &owner_path,
        &[
            "runtime",
            "activate",
            "--install-dir",
            install_path,
            "--runtime-sha256",
            digest,
        ],
    )?;
    let (new_namespace, fresh_resources) =
        common::observe_empty_installed_fixture(&install, &owner_path, "fresh-after-reinstall")?;
    let mut cold = serde_json::to_value(cold_resources)?;
    let mut fresh = serde_json::to_value(fresh_resources)?;
    // Preserve complete samples above. These host telemetry fields may change
    // with time; all configuration, policy headroom and work charges must match
    // the independently observed empty predecessor exactly.
    for field in [
        "observed_rss_bytes",
        "available_memory_bytes",
        "telemetry_complete",
        "pressure",
    ] {
        cold.as_object_mut()
            .context("cold resources object")?
            .remove(field)
            .context("cold resource telemetry field missing")?;
        fresh
            .as_object_mut()
            .context("fresh resources object")?
            .remove(field)
            .context("fresh resource telemetry field missing")?;
    }
    ensure!(
        cold == fresh,
        "fresh resource configuration/reservations differ from independently captured cold baseline: before={cold}, after={fresh}"
    );
    ensure!(
        new_namespace != old_namespace,
        "live reinstallation reused old namespace"
    );
    common::shutdown_installed_fixture(&install, &owner_path)?;
    common::observe_empty_stopped_fixture(&state.join("control"), &owner_path, new_namespace)?;
    let registry = ServiceRegistry::open_saved(&state.join("control"))?;
    ensure!(
        registry.incarnation()? != old_namespace,
        "reinstall reused the old namespace"
    );
    ensure!(
        registry.bindings()?.is_empty() && registry.pending_projects(32)?.is_empty(),
        "reinstallation created projects or indexing jobs"
    );
    ensure!(fs::read(source.join("original.txt"))? == b"keep original source");
    cleanup.passed = true;
    Ok(())
}

#[test]
fn standalone_uninstall_without_confirmation_preserves_the_explicit_installation() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let install = owner.path().join("install");
    fs::create_dir(&install)?;
    let original = b"Unreviewed installation bytes must survive a declined uninstall.";
    fs::write(install.join("unknown.bin"), original)?;
    let output = airk()
        .args(["__airk_role", "setup-interim"])
        .arg("--service-install-dir")
        .arg(&install)
        .arg("uninstall")
        .stdin(Stdio::null())
        .output()?;
    ensure!(
        !output.status.success(),
        "noninteractive uninstall ran without confirmation"
    );
    ensure!(
        String::from_utf8_lossy(&output.stderr).contains("interactive setup needs a terminal"),
        "uninstall failed for an unrelated reason: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(fs::read(install.join("unknown.bin"))? == original);
    ensure!(
        fs::read_dir(&install)?.count() == 1,
        "unconfirmed uninstall changed installation metadata"
    );
    Ok(())
}

/// Direct reads can return a typed busy/retry refusal while workers start.
fn headless_read(root: &Path, data: &Path, args: &[&str]) -> Result<Value> {
    let mut command = common::evidence_command(root, data, "demo");
    command.args(args).env_remove("AIRK_USAGE_MODE");
    let output = common::read_retry::output(&mut command)?;
    ensure!(
        output.status.success(),
        "airk {} failed: {}{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

/// `init` acknowledges compatibility registration; it cannot create a catalog
/// index or turn an unreviewed root into an authorized mapping.
#[test]
fn installed_init_acknowledges_only_existing_reviewed_mapping() -> Result<()> {
    use airk_core::service::{ControlMethod, ControlResponse};
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let original = b"# Existing reviewed init source\n";
    fs::write(source.path().join("original.md"), original)?;
    let service = common::Service::reviewed(source.path(), data.path(), "init-existing");
    // `init` is an airkctl command after the split (Phase 4); its library
    // entry runs against the fixture's control root.
    let airkctl::registration::Init::Registered(project) = airkctl::registration::init(
        &service.control,
        env!("CARGO_PKG_VERSION"),
        source.path(),
        data.path(),
        "init-existing",
        None,
    )
    .map_err(|line| anyhow::anyhow!("existing init failed: {}", line.message))?
    else {
        anyhow::bail!("existing init was not acknowledged");
    };
    let acknowledged = serde_json::to_value(&project)?;
    assert_eq!(
        acknowledged["registration"],
        serde_json::to_value(&service.project)?
    );
    assert!(Path::new(&service.project.data_root).is_dir());
    assert_eq!(fs::read(source.path().join("original.md"))?, original);

    let unreviewed = tempfile::tempdir()?;
    let absent = tempfile::tempdir()?;
    let absent_data = absent.path().join("not-created/nested");
    fs::write(
        unreviewed.path().join("original.md"),
        b"# Unreviewed source\n",
    )?;
    let unmapped =
        airk_cli::service_registration::prepare(unreviewed.path(), &absent_data, "init-unmapped")?;
    let ControlResponse::Health {
        observation: before,
    } = service.request(ControlMethod::Health)
    else {
        anyhow::bail!("missing service health before refused init");
    };
    let rejected = airkctl::registration::init(
        &service.control,
        env!("CARGO_PKG_VERSION"),
        unreviewed.path(),
        &absent_data,
        "init-unmapped",
        None,
    );
    ensure!(
        !matches!(rejected, Ok(airkctl::registration::Init::Registered(_))),
        "fresh unmapped root was registered by init"
    );
    assert!(!absent.path().join("not-created").exists());
    assert_eq!(
        fs::read(unreviewed.path().join("original.md"))?,
        b"# Unreviewed source\n"
    );
    let ControlResponse::Health { observation: after } = service.request(ControlMethod::Health)
    else {
        anyhow::bail!("missing service health after refused init");
    };
    assert_eq!(after.instance, before.instance);
    assert_eq!(after.catalog_clock, before.catalog_clock);
    let ControlResponse::Projects { page } = service.request(ControlMethod::Projects {
        after: None,
        revision: None,
        limit: 32,
    }) else {
        anyhow::bail!("missing native project roster after refused init");
    };
    assert!(page.after.is_none(), "fixture project roster was truncated");
    assert!(
        page.projects
            .iter()
            .any(|entry| entry.registration == service.project)
    );
    assert!(
        page.projects
            .iter()
            .all(|entry| entry.registration.binding != unmapped.binding)
    );
    service.shutdown_reviewed()?;
    Ok(())
}

#[test]
fn installed_query_scope_calls_and_shell_retain_canonical_authority() -> Result<()> {
    let diagnostic = |stage: &str, alias: &str, output: &Output| {
        format!(
            "{stage} {alias}: exit={:?}, stdout={:?}, stderr={:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout[..output.stdout.len().min(16384)]),
            String::from_utf8_lossy(&output.stderr[..output.stderr.len().min(16384)])
        )
    };
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(source.path().join("original.md"), b"# Reviewed scope\n")?;
    let service = common::Service::reviewed(source.path(), data.path(), "scope-cli");
    let mut expected = None;
    for alias in ["query_scope", "airk_query_scope"] {
        let output = common::scoped_command(source.path(), data.path(), "scope-cli")
            .args(["tools", "call", alias, "--args", r#"{"view":"projects"}"#])
            .output()?;
        ensure!(
            output.status.success(),
            "scope call failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let envelope: Value = serde_json::from_slice(&output.stdout)
            .with_context(|| diagnostic("successful call envelope", alias, &output))?;
        assert_ne!(envelope["isError"], true);
        let body: Value = serde_json::from_str(
            envelope["content"][0]["text"]
                .as_str()
                .context("scope text")?,
        )
        .with_context(|| diagnostic("successful call body", alias, &output))?;
        assert_eq!(body["mode"], "frozen_single_index");
        let observation = serde_json::from_value(body["scope_observation"].clone())
            .context("canonical scope observation")?;
        let args = serde_json::from_value(json!({"view":"projects"}))?;
        assert_eq!(
            body,
            airk_core::combined_reads::query_scope::installed_body(&args, &observation)?
        );
        let raw = common::ctl::Ctl::new()
            .arg("--service-runtime-dir")
            .arg(&service.control)
            .arg("--service-project")
            .arg(service.project.binding.to_string())
            .arg("--root")
            .arg(&service.project.root)
            .arg("--data-dir")
            .arg(&service.project.data_root)
            .args([
                "--collection",
                &service.project.collection,
                "tools",
                "call",
                alias,
                "--args",
                r#"{"view":"projects"}"#,
            ])
            .output()?;
        // A fresh reviewed grant cannot be inferred from registration alone.
        // airkctl has no private-installation override: the installed route
        // is the folder-resolved project (M1) on the installation's control root.
        let installed = common::ctl::Ctl::new()
            .arg("--service-runtime-dir")
            .arg(&service.control)
            .arg("--root")
            .arg(source.path())
            .arg("--data-dir")
            .arg(data.path())
            .args(["--collection", "scope-cli"])
            .args(["tools", "call", alias, "--args", r#"{"view":"projects"}"#])
            .output()?;
        for (route, denied) in [("raw denial", raw), ("installed denial", installed)] {
            assert!(!denied.status.success());
            let envelope: Value = serde_json::from_slice(&denied.stdout)
                .with_context(|| diagnostic(route, alias, &denied))?;
            assert_eq!(envelope["isError"], true);
            let body: Value = serde_json::from_str(
                envelope["content"][0]["text"]
                    .as_str()
                    .context("refusal text")?,
            )
            .with_context(|| diagnostic(route, alias, &denied))?;
            assert_eq!(body["error"]["code"], "scoped_read_unavailable");
            assert_eq!(body["error"]["retryable"], false);
        }
        if let Some(expected) = &expected {
            assert_eq!(&body, expected);
        }
        expected = Some(body.clone());
        let shell = common::scoped_command(source.path(), data.path(), "scope-cli")
            .args(["tools", "shell"])
            .input(format!("call {alias} {{\"view\":\"projects\"}}\nexit\n"))
            .output()?;
        ensure!(
            shell.status.success() && shell.stderr.is_empty(),
            "scope shell failed: {}",
            String::from_utf8_lossy(&shell.stderr)
        );
        let text = std::str::from_utf8(&shell.stdout)?;
        let text = text.split_once("airk> ").context("shell prompt")?.1;
        let text = text
            .strip_prefix("Result:")
            .context("shell success label")?
            .trim_start();
        let rendered = serde_json::Deserializer::from_str(text)
            .into_iter::<Value>()
            .next()
            .context("shell body missing")?
            .with_context(|| diagnostic("shell body", alias, &shell))?;
        assert_eq!(rendered, body);
        let invalid = common::scoped_command(source.path(), data.path(), "scope-cli")
            .args([
                "tools",
                "call",
                alias,
                "--args",
                r#"{"view":"projects","response_mode":"full"}"#,
            ])
            .output()?;
        assert!(!invalid.status.success());
        let refusal: Value = serde_json::from_slice(&invalid.stdout)
            .with_context(|| diagnostic("invalid argument envelope", alias, &invalid))?;
        assert_eq!(refusal["isError"], true);
        assert!(String::from_utf8_lossy(&invalid.stdout).contains("INVALID_SCHEMA"));
    }
    service.shutdown_reviewed()?;
    Ok(())
}

/// Current CLI enrollment never infers a grant from an old-looking registration.
/// The positive read connection and its revocation use an actual native owner.
#[test]
fn native_http_commands_preserve_private_selection_and_revocation() -> Result<()> {
    use airk_core::identity::Sha256Digest;
    use sha2::{Digest, Sha256};
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        source.path().join("original.md"),
        b"# Native HTTP command fixture\n",
    )?;
    let service = common::Service::reviewed(source.path(), data.path(), "http-commands");
    let inputs = tempfile::tempdir()?;
    let private = inputs.path().join("private");
    airk_cli::local_ipc::prepare_root(&private)?;
    let write_input = |name: &str, value: &Value| -> Result<std::path::PathBuf> {
        let path = private.join(name);
        fs::write(&path, serde_json::to_vec(value)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(path)
    };
    let runtime = |role: &str| {
        let mut command = service.installed_command(source.path(), data.path(), "http-commands");
        command
            .args(["runtime", role, "--install-dir"])
            .arg(service.install_dir());
        command
    };
    let value = |command: &mut Command| -> Result<Value> {
        let output = command.output()?;
        ensure!(
            output.status.success(),
            "native HTTP command failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    let endpoint = value(&mut runtime("http-endpoint"))?;
    assert_eq!(endpoint["state"], "ready");
    let id = airk_kernel::service_broker::new_instance_id()?.to_string();
    let selection = write_input(
        "selection.json",
        &serde_json::to_value(service.http_selection())?,
    )?;
    let enroll = || -> Result<Value> {
        value(
            runtime("http-connection")
                .arg("--selection-file")
                .arg(&selection)
                .args(["--connection-id", &id, "--client", "codex"]),
        )
    };
    let enrolled = enroll()?;
    assert_eq!(enrolled["connection_id"], id);
    assert_eq!(enrolled["endpoint"], endpoint);
    assert_eq!(
        enrolled["selection"],
        serde_json::to_value(service.http_selection())?
    );
    assert!(enrolled.get("token").is_none());
    assert!(enrolled["credential_file"].is_string() && enrolled["ca_certificate_file"].is_string());
    assert_eq!(
        enroll()?,
        enrolled,
        "same-ID retry changed authority or credential references"
    );
    let existing = value(runtime("http-connection-info").args(["--connection-id", &id]))?;
    assert_eq!(existing, enrolled);

    let airk_core::service::ControlResponse::Health {
        observation: health,
    } = service.request(airk_core::service::ControlMethod::Health)
    else {
        anyhow::bail!("missing owner identity before read-only connection lookup");
    };
    let active = health
        .runtime_sha256
        .as_deref()
        .context("approved runtime missing")?;
    let lookup = |identifier: &str| {
        value(runtime("lookup-http-connection").args([
            "--expected-active",
            active,
            "--connection-id",
            identifier,
        ]))
    };
    let credential = Path::new(
        enrolled["credential_file"]
            .as_str()
            .context("credential path missing")?,
    );
    let credential_bytes = fs::read(credential)?;
    // The header helper for agents started outside VS Code prints exactly the
    // owner-private bearer of the existing connection; it enrolls nothing.
    let header = || {
        let mut command = runtime("http-credential");
        command.args(["--connection-id", &id, "--format", "header-json"]);
        command
    };
    assert_eq!(
        value(&mut header())?,
        json!({"Authorization": format!("Bearer {}", std::str::from_utf8(&credential_bytes)?)})
    );
    assert!(
        fs::read(credential)? == credential_bytes,
        "the header helper must not rotate credentials"
    );
    let owned = lookup(&id)?;
    assert_eq!(owned["status"], "owned");
    assert_eq!(owned["connection"], enrolled);
    assert_eq!(owned["active"], active);
    assert_eq!(owned["connection_id"], id);
    let unknown_id = airk_kernel::service_broker::new_instance_id()?.to_string();
    let unknown = lookup(&unknown_id)?;
    assert_eq!(unknown["status"], "not_owned");
    assert_eq!(unknown["connection_id"], unknown_id);
    assert!(unknown.get("connection").is_none());
    assert!(
        !runtime("http-connection-info")
            .args(["--connection-id", &unknown_id])
            .output()?
            .status
            .success()
    );
    assert!(
        fs::read(credential)? == credential_bytes,
        "lookup must not rotate credentials"
    );
    assert_eq!(
        value(runtime("http-connection-info").args(["--connection-id", &id]))?,
        enrolled
    );

    // Both operations inspect existing metadata only: no trust install/remove
    // and no rotation prepare/commit occurs in this ordinary CLI regression.
    let trust = value(runtime("http-trust").args(["--action", "inspect"]))?;
    assert_eq!(trust["schema"], 1);
    assert_eq!(trust["ca_sha256"], endpoint["ca_sha256"]);
    let rotation = value(runtime("http-issuer-rotation").args(["--action", "status"]))?;
    assert_eq!(rotation["schema"], 1);
    assert_eq!(rotation["current_endpoint"], endpoint);
    assert!(rotation["rotation"].is_null());

    // These are explicitly current, non-imported declarations. Hash the actual
    // owned declaration bytes; native migration must request review, never mint
    // predecessor authority or silently reuse the current read grant.
    let declaration = serde_json::to_vec(
        &json!({"schema":1,"kind":"current_fixture","registration":service.project}),
    )?;
    let receipt = Sha256Digest::from_bytes(Sha256::digest(&declaration).into()).to_string();
    fs::write(private.join("current-declaration.json"), &declaration)?;
    let registration = write_input(
        "registration.json",
        &serde_json::to_value(&service.project)?,
    )?;
    let legacy_id = airk_kernel::service_broker::new_instance_id()?.to_string();
    let legacy = value(
        runtime("http-legacy-connection")
            .arg("--registration-file")
            .arg(registration)
            .args([
                "--configuration-receipt",
                &receipt,
                "--connection-id",
                &legacy_id,
                "--client",
                "codex",
            ]),
    )?;
    assert_eq!(legacy["status"], "review_required");
    assert_eq!(legacy["code"], "legacy_selection_review_required");
    assert_eq!(
        legacy["registration"],
        serde_json::to_value(&service.project)?
    );
    assert!(
        !runtime("http-connection-info")
            .args(["--connection-id", &legacy_id])
            .output()?
            .status
            .success()
    );
    let second_source = tempfile::tempdir()?;
    let second_data = tempfile::tempdir()?;
    fs::write(
        second_source.path().join("original.md"),
        b"# Second current declaration\n",
    )?;
    let second =
        service.additional_project(second_source.path(), second_data.path(), "http-second")?;
    let second_declaration =
        serde_json::to_vec(&json!({"schema":1,"kind":"current_fixture","registration":second}))?;
    let second_receipt =
        Sha256Digest::from_bytes(Sha256::digest(&second_declaration).into()).to_string();
    fs::write(
        private.join("second-current-declaration.json"),
        second_declaration,
    )?;
    let union = write_input(
        "registrations.json",
        &json!({"schema":1,"no_default_project":true,"registrations":[
            {"registration":service.project,"configuration_receipt":receipt},
            {"registration":second,"configuration_receipt":second_receipt}
        ]}),
    )?;
    let union_id = airk_kernel::service_broker::new_instance_id()?.to_string();
    let refused = value(
        runtime("http-legacy-union-connection")
            .arg("--registrations-file")
            .arg(union)
            .args(["--connection-id", &union_id, "--client", "codex"]),
    )?;
    assert_eq!(refused["status"], "review_required");
    assert_eq!(refused["code"], "legacy_union_review_required");
    assert_eq!(refused["no_default_project"], true);
    assert_eq!(
        refused["resolutions"]
            .as_array()
            .context("missing complete union resolutions")?
            .len(),
        2
    );
    assert!(
        !runtime("http-connection-info")
            .args(["--connection-id", &union_id])
            .output()?
            .status
            .success()
    );
    let revoked = value(runtime("http-revoke").args(["--connection-id", &id]))?;
    assert_eq!(revoked["connection_id"], id);
    let credential_snapshot = || -> Result<Option<Vec<u8>>> {
        match fs::read(credential) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    };
    let credential_after_revoke = credential_snapshot()?;
    let refused_header = header().output()?;
    assert!(!refused_header.status.success() && refused_header.stdout.is_empty());
    assert!(
        !String::from_utf8_lossy(&refused_header.stderr)
            .contains(std::str::from_utf8(&credential_bytes)?),
        "a refused header helper never prints the revoked token"
    );
    let unavailable = lookup(&id)?;
    assert_eq!(unavailable["status"], "unavailable");
    assert_eq!(unavailable["code"], "owned_connection_revoked");
    assert!(unavailable.get("connection").is_none());
    assert!(
        credential_snapshot()? == credential_after_revoke,
        "lookup must not recreate a revoked credential"
    );
    let airk_core::service::ControlResponse::Health { observation: after } =
        service.request(airk_core::service::ControlMethod::Health)
    else {
        anyhow::bail!("missing owner identity after read-only connection lookup");
    };
    assert_eq!(after.instance, health.instance);
    assert!(
        !runtime("http-connection-info")
            .args(["--connection-id", &id])
            .output()?
            .status
            .success()
    );
    assert_eq!(
        fs::read(source.path().join("original.md"))?,
        b"# Native HTTP command fixture\n"
    );
    service.shutdown_reviewed()?;
    Ok(())
}

/// Public migration commands must not bootstrap an installation from untrusted
/// paths. Standalone adoption is refused (standalone_adoption_unsupported); the
/// remaining legacy cache maintenance is covered by the runtime_install tests.
#[test]
fn legacy_runtime_commands_require_an_existing_owned_installation() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    let install = root.join("not-installed");
    let global = root.join("editor-global-storage");
    let private = root.join("private-inputs");
    airk_cli::local_ipc::prepare_root(&private)?;
    let config = root.join("config.toml");
    let original = b"[mcp_servers.original]\ncommand = 'retained-original'\n";
    fs::write(&config, original)?;
    let locator = json!({"path":config,"format":"codex_toml","server":"original"});
    let single = private.join("locator.json");
    let plural = private.join("locators.json");
    let single_bytes = serde_json::to_vec(&locator)?;
    let plural_bytes = serde_json::to_vec(&vec![locator])?;
    for (path, bytes) in [(&single, &single_bytes), (&plural, &plural_bytes)] {
        fs::write(path, bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
    }
    let expected = "a".repeat(64);
    let operation = "b".repeat(32);
    for role in [
        "inspect-legacy-launchers",
        "retire-legacy-launchers",
        "inspect-legacy-caches",
        "prepare-legacy-caches",
        "reprepare-legacy-caches",
        "retire-legacy-caches",
        "adopt-legacy-index",
        "adopt-legacy-indexes",
    ] {
        let mut command = airk();
        command
            .args([
                "__airk_role",
                "setup-interim",
                "runtime",
                role,
                "--install-dir",
            ])
            .arg(&install)
            .args(["--expected-active", &expected]);
        if matches!(
            role,
            "inspect-legacy-caches" | "prepare-legacy-caches" | "reprepare-legacy-caches"
        ) {
            command
                .arg("--global-storage")
                .arg(&global)
                .arg("--origin-locators-file")
                .arg(&plural);
        }
        if matches!(role, "reprepare-legacy-caches" | "retire-legacy-caches") {
            command.args(["--expected-inspection-sha256", &expected]);
        }
        if matches!(role, "adopt-legacy-index" | "adopt-legacy-indexes") {
            command.args([
                "--operation-id",
                &operation,
                "--expected-cache-inspection-sha256",
                &expected,
            ]);
            if role == "adopt-legacy-index" {
                command.arg("--locator-file").arg(&single);
            } else {
                command.arg("--locators-file").arg(&plural);
            }
        }
        let output = command.stdin(Stdio::null()).output()?;
        ensure!(
            output.status.code() == Some(1),
            "{role} must reach native ownership refusal, not argument parsing: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        ensure!(
            !String::from_utf8_lossy(&output.stderr).contains("Usage:"),
            "{role} rejected the test arguments instead of native ownership"
        );
        assert!(!install.exists(), "{role} created a missing installation");
        assert!(!global.exists(), "{role} created an unowned editor cache");
        assert_eq!(fs::read(&config)?, original);
        assert_eq!(fs::read(&single)?, single_bytes);
        assert_eq!(fs::read(&plural)?, plural_bytes);
    }
    Ok(())
}

/// Functional coverage of the headless workflow the CLI guide documents.
#[test]
fn headless_workflow_exercises_query_and_management_commands() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    fs::create_dir_all(&source)?;
    fs::create_dir_all(&data)?;
    fs::write(
        source.join("notes.md"),
        "# Copper harbor\n\nThe copper harbor ledger records tides.\n\n## Requirements\n\nThe ledger must be retained.\n",
    )?;
    fs::write(
        source.join("lib.rs"),
        "use std::fmt;\n/// Copper ledger.\npub struct Ledger;\npub fn record_tide() -> usize { 1 }\nfn caller() { record_tide(); }\n",
    )?;
    let original_sources = ["notes.md", "lib.rs"]
        .into_iter()
        .map(|name| Ok((name, fs::read(source.join(name))?)))
        .collect::<Result<Vec<_>>>()?;
    let service = common::Service::reviewed(&source, &data, "demo");
    let control_text = service.control.to_string_lossy().into_owned();
    let binding = service.project.binding.to_string();
    let prepared = common::call(&source, &data, "demo", &["service", "prepare-registration"])?;
    assert_eq!(prepared["binding"], binding);
    // Compatibility registration acknowledges the existing exact mapping; the
    // fixture already created its Index through the actual reviewed setup path.
    common::call(
        &source,
        &data,
        "demo",
        &["service", "register", "--runtime-dir", &control_text],
    )?;
    let projects = ctl_json_ok(&["projects", "--runtime-dir", &control_text])?;
    ensure!(
        projects["projects"][0]["project"] == binding
            && projects["projects"][0]["collection"] == "demo",
        "projects listed {projects}"
    );
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    // Submit one real CLI index request and observe its exact accepted job;
    // a read failure never authorizes replaying the mutation.
    let accepted = common::call(&source, &data, "demo", &["index"])?;
    let job = service.wait(serde_json::from_value(accepted["job"]["id"].clone())?);
    assert_eq!(job.state, airk_core::service::JobState::Succeeded);
    let search = headless_read(
        &source,
        &data,
        &["search", "copper harbor", "--mode", "lexical"],
    )?;
    ensure!(
        search.to_string().contains("notes.md"),
        "indexed citation missing: {search}"
    );
    for args in [
        &["status"][..],
        &["files"],
        &["read", "notes.md", "--start-line", "1", "--end-line", "2"],
        &["info", "notes.md"],
        &["outline", "lib.rs"],
        &["symbol", "Ledger"],
        &["imports"],
        &["facts"],
        &["refs", "record_tide"],
        &["context", "notes.md", "3"],
        &["changes", "--path", "notes.md"],
        &["code-facts"],
        &["relationships", "record_tide"],
        &["settings", "scan_limits"],
        &["parse", "find copper harbor"],
        &["tools", "list", "--json"],
        &["tools", "call", "airk_status", "--args", "{}"],
    ] {
        headless_read(&source, &data, args).with_context(|| format!("airk {}", args.join(" ")))?;
    }
    for args in [
        &["usage", "policy"][..],
        &["usage", "report"],
        &["inspect"],
        &["pause"],
        &["resume"],
        &["watch", "on"],
        &["watch", "off"],
    ] {
        common::call(&source, &data, "demo", args)
            .with_context(|| format!("owner command airk {}", args.join(" ")))?;
        if args == ["watch", "on"] {
            // Observe the enabled watch's reconciliation before disabling it.
            // A disable that overtakes it leaves requested > completed (ledger
            // follow-up, Phase 4); the pre-split CLI's start-up time hid that.
            let until = Instant::now() + Duration::from_secs(30);
            loop {
                let airk_core::service::ControlResponse::Watch { observation, .. } = service
                    .metadata(
                        airk_core::service::ControlMethod::Watch {
                            project: service.project.binding,
                        },
                        until,
                    )
                else {
                    anyhow::bail!("watch observation unavailable");
                };
                if observation.completed_revision == observation.requested_revision {
                    break;
                }
                ensure!(
                    Instant::now() + Duration::from_millis(50) < until,
                    "enabled watch did not reconcile: {observation:?}"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    ctl_json_ok(&[
        "service",
        "logs",
        "--runtime-dir",
        &control_text,
        "--limit",
        "4",
    ])?;
    ctl_json_ok(&["--service-runtime-dir", &control_text, "model-catalog"])?;
    // Usage feedback needs a recorded request ID from a real response.
    common::call(
        &source,
        &data,
        "demo",
        &["usage", "configure", "--mode", "diagnostic_events"],
    )?;
    let deadline = Instant::now() + Duration::from_secs(60);
    let request_id = loop {
        let found = headless_read(
            &source,
            &data,
            &["search", "copper harbor", "--mode", "lexical"],
        )?;
        // The tool envelope carries the logical result as JSON text.
        let payload: Value = match found["content"][0]["text"].as_str() {
            Some(text) => serde_json::from_str(text)?,
            None => Value::Null,
        };
        if let Some(id) = payload["request_id"].as_str() {
            break id.to_owned();
        }
        ensure!(
            Instant::now() < deadline,
            "no request_id while usage is recorded: {found}"
        );
        std::thread::sleep(Duration::from_millis(250));
    };
    common::call(
        &source,
        &data,
        "demo",
        &["usage", "feedback", &request_id, "--usefulness", "useful"],
    )?;
    common::call(&source, &data, "demo", &["usage", "clear"])?;
    // Durable client requests: prepare saves without submitting; list, forget and retry act on it.
    let requests = ["service", "requests", "--runtime-dir", &control_text];
    let request = |args: &[&str]| ctl_json_ok(&[&requests[..], args].concat());
    let saved = request(&["prepare", "--project", &binding, "--kind", "inspect"])?;
    let nonce = saved["prepared"]
        .as_str()
        .context("prepared nonce")?
        .to_owned();
    ensure!(saved["submitted"] == false, "{saved}");
    ensure!(request(&["list"])?.to_string().contains(&nonce));
    request(&["forget", &nonce])?;
    ensure!(!request(&["list"])?.to_string().contains(&nonce));
    let again = request(&["prepare", "--project", &binding, "--kind", "inspect"])?;
    request(&["retry", again["prepared"].as_str().context("nonce")?])?;
    service.settle_reviewed_index(&source, &data, Instant::now() + Duration::from_secs(90))?;
    // A legacy unregister request cannot bypass the current catalog's source
    // review/fence. Submit it once and retain its saved CLI key on refusal.
    let refused = common::command(&source, &data, "demo")
        .args(["configure", "--stdin"])
        .input(br#"{"operation":"unregister"}"#.to_vec())
        .output()?;
    ensure!(
        !refused.status.success()
            && String::from_utf8_lossy(&refused.stderr).contains("source_change_review_required"),
        "legacy unregister did not preserve catalog review: {}{}",
        String::from_utf8_lossy(&refused.stdout),
        String::from_utf8_lossy(&refused.stderr)
    );
    let retained = headless_read(
        &source,
        &data,
        &["search", "copper harbor", "--mode", "lexical"],
    )?;
    ensure!(
        retained.to_string().contains("notes.md"),
        "refused unregister lost publication: {retained}"
    );
    // Native IPC uses the exact SourceMutationDraft and validated fenced
    // receipt, with one CatalogApply. CLI stream qualification is separate.
    service
        .retire_index()
        .context("reviewed native Index retirement")?;
    let retired = common::evidence_command(&source, &data, "demo")
        .arg("status")
        .output()?;
    ensure!(
        !retired.status.success(),
        "retired Index still admitted its old scoped reader"
    );
    // The CLI resolves catalog metadata before constructing the query handler.
    // A completed retirement can therefore refuse before any MCP envelope exists.
    if retired.stdout.is_empty() {
        assert_eq!(
            String::from_utf8_lossy(&retired.stderr).trim(),
            "airkctl: selected catalog index metadata is not ready"
        );
    } else {
        let retired_reply: Value = serde_json::from_slice(&retired.stdout)?;
        ensure!(
            retired_reply["isError"] == true
                && retired_reply["structuredContent"]["error"]["code"]
                    == "scoped_connection_unavailable"
                && retired_reply["structuredContent"]["error"]["query_submitted"] == false,
            "retirement must refuse the old authority before a query: {retired_reply}"
        );
    }
    for (name, bytes) in original_sources {
        assert_eq!(
            fs::read(source.join(name))?,
            bytes,
            "retirement changed source {name}"
        );
    }
    service.shutdown_reviewed()?;
    Ok(())
}

#[test]
fn explorer_commands_write_results_only_to_new_output_files() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let out = |name: &str| directory.path().join(name).to_string_lossy().into_owned();
    // airkctl runs the Explorer operations in the service (M11); a bare
    // service without projects stands in for the pre-split offline run.
    let service = common::BareService::start();
    let control = service.control.to_string_lossy().into_owned();
    let run =
        |args: &[&str]| ctl_run(&[&["--service-runtime-dir", control.as_str()][..], args].concat());
    let json_ok = |args: &[&str]| {
        ctl_json_ok(&[&["--service-runtime-dir", control.as_str()][..], args].concat())
    };
    // --output is mandatory for every Explorer command.
    ensure!(!run(&["explorer", "list-templates"])?.status.success());
    let listed = json_ok(&[
        "explorer",
        "list-templates",
        "--output-file",
        &out("templates.json"),
    ])?;
    ensure!(listed["ok"] == true);
    let templates: Value = serde_json::from_slice(&fs::read(out("templates.json"))?)?;
    let id = templates["templates"][0]["template_id"]
        .as_str()
        .context("template id")?;
    json_ok(&[
        "explorer",
        "get-template",
        "--args",
        &json!({"template_id":id}).to_string(),
        "--output-file",
        &out("template.json"),
    ])?;
    // An existing output file is never overwritten.
    ensure!(
        !run(&[
            "explorer",
            "list-templates",
            "--output-file",
            &out("templates.json")
        ])?
        .status
        .success()
    );
    let draft = json!({"schema_version":"1.0","idempotency_key":"cli-1","title":"Find copper","draft":{"query":"copper"}});
    json_ok(&[
        "explorer",
        "draft-search",
        "--args",
        &draft.to_string(),
        "--output-file",
        &out("search.json"),
    ])?;
    let written: Value = serde_json::from_slice(&fs::read(out("search.json"))?)?;
    ensure!(
        written["valid"] == true && written["admitted"] == false && written["executed"] == false,
        "{written}"
    );
    let status = json_ok(&[
        "explorer",
        "proposal-status",
        "--draft",
        &out("search.json"),
        "--output-file",
        &out("status.json"),
    ])?;
    ensure!(status["ok"] == true);
    // An invalid draft is still written for review, and the command fails.
    ensure!(
        !run(&[
            "explorer",
            "draft-plan",
            "--args",
            "{\"unexpected\":1}",
            "--output-file",
            &out("plan.json")
        ])?
        .status
        .success()
    );
    ensure!(Path::new(&out("plan.json")).is_file());
    ensure!(
        !run(&[
            "explorer",
            "draft-strategy",
            "--args",
            "{}",
            "--output-file",
            &out("strategy.json")
        ])?
        .status
        .success()
    );
    ensure!(
        !run(&[
            "explorer",
            "suggest-view",
            "--args",
            "{\"kind\":\"search\",\"draft\":{}}",
            "--output-file",
            &out("view.json")
        ])?
        .status
        .success()
    );
    json_ok(&[
        "explorer",
        "validate-draft",
        "--args",
        &json!({"kind":"search","draft":draft}).to_string(),
        "--output-file",
        &out("valid.json"),
    ])?;
    // A handoff needs a running editor; one no window published fails before creating output.
    ensure!(
        !run(&[
            "explorer",
            "get-handoff",
            "--handoff-id",
            &"a".repeat(32),
            "--output-file",
            &out("handoff.json")
        ])?
        .status
        .success()
    );
    ensure!(!Path::new(&out("handoff.json")).exists());
    Ok(())
}
