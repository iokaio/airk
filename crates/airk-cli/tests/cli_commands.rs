// SPDX-License-Identifier: Apache-2.0
//! Every public CLI command has a test. Query and management commands run against a
//! private headless service; Explorer commands run offline; installer-lifecycle commands
//! name the native test or qualification script that exercises them. The table is checked
//! against commands discovered from the actual native parser help, independently
//! of generated docs/cli-reference.md. Another test keeps full documentation exact.
use anyhow::{Context, Result, ensure};
mod common;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
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

fn run(args: &[&str]) -> Result<Output> {
    Ok(airk().args(args).stdin(Stdio::null()).output()?)
}

fn json_ok(args: &[&str]) -> Result<Value> {
    let output = run(args)?;
    ensure!(
        output.status.success(),
        "airk {} failed: {}{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("airk {} printed no JSON", args.join(" ")))
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
    let initialized = service
        .installed_command(source.path(), data.path(), "init-existing")
        .arg("init")
        .output()?;
    ensure!(
        initialized.status.success(),
        "existing init failed: {}{}",
        String::from_utf8_lossy(&initialized.stdout),
        String::from_utf8_lossy(&initialized.stderr)
    );
    let acknowledged: Value = serde_json::from_slice(&initialized.stdout)?;
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
    let rejected = service
        .installed_command(unreviewed.path(), &absent_data, "init-unmapped")
        .arg("init")
        .output()?;
    ensure!(
        !rejected.status.success(),
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
    use std::io::Write;
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
        let raw = airk()
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
        // Imported legacy grants use this route in check-standalone-adoption.py.
        let installed = service
            .installed_command(source.path(), data.path(), "scope-cli")
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
        let mut child = common::scoped_command(source.path(), data.path(), "scope-cli")
            .args(["tools", "shell"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        writeln!(
            child.stdin.take().context("shell stdin")?,
            "call {alias} {{\"view\":\"projects\"}}\nexit"
        )?;
        let shell = child.wait_with_output()?;
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
/// paths. Positive conversion/retirement is separately exercised by the native
/// legacy tests and check-standalone-adoption.py against authentic predecessors.
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
            .args(["runtime", role, "--install-dir"])
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
    let projects = json_ok(&["projects", "--runtime-dir", &control_text])?;
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
    }
    json_ok(&[
        "service",
        "logs",
        "--runtime-dir",
        &control_text,
        "--limit",
        "4",
    ])?;
    json_ok(&["model-catalog"])?;
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
    let request = |args: &[&str]| json_ok(&[&requests[..], args].concat());
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
    let mut unregister = common::command(&source, &data, "demo")
        .args(["configure", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    use std::io::Write;
    unregister
        .stdin
        .take()
        .context("unregister stdin missing")?
        .write_all(br#"{"operation":"unregister"}"#)?;
    let refused = unregister.wait_with_output()?;
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
            "Error: selected catalog index metadata is not ready"
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
    // --output is mandatory for every Explorer command.
    ensure!(!run(&["explorer", "list-templates"])?.status.success());
    let listed = json_ok(&[
        "explorer",
        "list-templates",
        "--output",
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
        "--output",
        &out("template.json"),
    ])?;
    // An existing output file is never overwritten.
    ensure!(
        !run(&[
            "explorer",
            "list-templates",
            "--output",
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
        "--output",
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
        "--output",
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
            "--output",
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
            "--output",
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
            "--output",
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
        "--output",
        &out("valid.json"),
    ])?;
    // A handoff needs a running editor; a missing descriptor fails before creating output.
    ensure!(
        !run(&[
            "explorer",
            "get-handoff",
            "--token",
            &format!("ex-share-{}", "a".repeat(64)),
            "--explorer-descriptor",
            &out("missing/launch-1.json"),
            "--output",
            &out("handoff.json")
        ])?
        .status
        .success()
    );
    ensure!(!Path::new(&out("handoff.json")).exists());
    Ok(())
}

/// Commands whose effects need an installed runtime, login manager, legacy state or an
/// editor are exercised elsewhere; each entry names that test so it cannot silently rot.
const ELSEWHERE: &[(&str, &str, &str)] = &[
    (
        "runtime lookup-http-connection",
        "this file",
        "native_http_commands_preserve_private_selection_and_revocation",
    ),
    (
        "runtime inspect-legacy-launchers",
        "this file",
        "legacy_runtime_commands_require_an_existing_owned_installation",
    ),
    (
        "runtime retire-legacy-launchers",
        "this file",
        "legacy_runtime_commands_require_an_existing_owned_installation",
    ),
    (
        "runtime inspect-legacy-caches",
        "this file",
        "legacy_runtime_commands_require_an_existing_owned_installation",
    ),
    (
        "runtime prepare-legacy-caches",
        "this file",
        "legacy_runtime_commands_require_an_existing_owned_installation",
    ),
    (
        "runtime reprepare-legacy-caches",
        "this file",
        "legacy_runtime_commands_require_an_existing_owned_installation",
    ),
    (
        "runtime retire-legacy-caches",
        "this file",
        "legacy_runtime_commands_require_an_existing_owned_installation",
    ),
    (
        "runtime adopt-legacy-index",
        "this file",
        "legacy_runtime_commands_require_an_existing_owned_installation",
    ),
    (
        "runtime adopt-legacy-indexes",
        "this file",
        "legacy_runtime_commands_require_an_existing_owned_installation",
    ),
    (
        "runtime source-scope",
        "crates/airk-cli/src/runtime_install_commands.rs",
        "source_scope_authenticates_digest_control_and_missing_state_without_creation",
    ),
    (
        "runtime http-endpoint",
        "this file",
        "native_http_commands_preserve_private_selection_and_revocation",
    ),
    (
        "runtime http-connection",
        "this file",
        "native_http_commands_preserve_private_selection_and_revocation",
    ),
    (
        "runtime http-connection-info",
        "this file",
        "native_http_commands_preserve_private_selection_and_revocation",
    ),
    (
        "runtime http-revoke",
        "this file",
        "native_http_commands_preserve_private_selection_and_revocation",
    ),
    (
        "runtime http-trust",
        "this file",
        "native_http_commands_preserve_private_selection_and_revocation",
    ),
    (
        "runtime http-issuer-rotation",
        "this file",
        "native_http_commands_preserve_private_selection_and_revocation",
    ),
    (
        "runtime http-legacy-connection",
        "this file",
        "native_http_commands_preserve_private_selection_and_revocation",
    ),
    (
        "runtime http-legacy-union-connection",
        "this file",
        "native_http_commands_preserve_private_selection_and_revocation",
    ),
    (
        "bridge",
        "crates/airk-cli/tests/service_runtime.rs",
        "bridge",
    ),
    (
        "host",
        "crates/airk-cli/src/runtime_install_commands.rs",
        "Host",
    ),
    (
        "init",
        "this file",
        "installed_init_acknowledges_only_existing_reviewed_mapping",
    ),
    (
        "mcp",
        "crates/airk-cli/tests/service_runtime.rs",
        "assert_retired_stdio_roles",
    ),
    (
        "setup",
        "crates/airk-cli/tests/configuration_native.rs",
        "\"setup\"",
    ),
    (
        "setup-model",
        "scripts/check-service-clients.py",
        "setup-model",
    ),
    ("search", "this file", "headless_workflow"),
    (
        "serve",
        "crates/airk-cli/tests/service_runtime.rs",
        "supervised_jobs_and_direct_queries_preserve_grants_and_stop_on_service_loss",
    ),
    ("tools shell", "crates/airk-cli/tests/tools.rs", "shell"),
    (
        "usage calibrate",
        "scripts/check-tokenizer-calibration.py",
        "calibrate",
    ),
    ("usage import", "scripts/check-usage-inputs.py", "import"),
    (
        "service console",
        "crates/airk-cli/tests/service_runtime.rs",
        "console",
    ),
    (
        "service record-error",
        "packages/vscode/src/test/diagnosticLogs.test.ts",
        "record-error",
    ),
    ("service rpc", "scripts/check-service-clients.py", "\"rpc\""),
    (
        "service prepare-storage",
        "crates/airk-cli/tests/configuration_native.rs",
        "prepare-storage",
    ),
    (
        "service prepare-configuration",
        "crates/airk-cli/tests/configuration_native.rs",
        "prepare-configuration",
    ),
    (
        "service requests prepare-configuration",
        "crates/airk-cli/tests/configuration_native.rs",
        "prepare-configuration",
    ),
    (
        "service legacy derived-receipts",
        "crates/airk-cli/tests/legacy_native.rs",
        "derived-receipts",
    ),
    (
        "service legacy forget",
        "crates/airk-cli/src/legacy_startup_tests.rs",
        ".forget(approval)",
    ),
    (
        "service legacy forget-derived",
        "crates/airk-cli/src/legacy_startup.rs",
        "ForgetDerived",
    ),
    (
        "service legacy list",
        "crates/airk-cli/tests/legacy_native.rs",
        "list",
    ),
    (
        "service legacy preview-derived",
        "crates/airk-cli/tests/legacy_native.rs",
        "preview-derived",
    ),
    (
        "service legacy remove-derived",
        "crates/airk-cli/tests/legacy_native.rs",
        "remove-derived",
    ),
    (
        "service legacy remove-startup",
        "packages/vscode/src/test/serviceBootstrap.test.ts",
        "remove-startup",
    ),
    (
        "service legacy review",
        "crates/airk-cli/tests/legacy_native.rs",
        "review",
    ),
    (
        "service legacy show",
        "crates/airk-cli/src/legacy_startup.rs",
        "Show",
    ),
    (
        "runtime abandon-lifecycle",
        "crates/airk-cli/src/runtime_lifecycle.rs",
        "abandon",
    ),
    (
        "runtime abort-activation",
        "scripts/check-runtime-activation.py",
        "abort-activation",
    ),
    (
        "runtime abort-launcher-repair",
        "crates/airk-cli/src/runtime_launcher_repair.rs",
        "abort",
    ),
    (
        "runtime abort-layout",
        "crates/airk-cli/src/runtime_layout.rs",
        "abort",
    ),
    (
        "runtime abort-stage",
        "crates/airk-cli/src/runtime_staging.rs",
        "abort",
    ),
    (
        "runtime activate",
        "scripts/check-runtime-activation.py",
        "activate",
    ),
    (
        "runtime container-host",
        "scripts/check-container.py",
        "container",
    ),
    (
        "runtime diagnose",
        "scripts/check-service-install.py",
        "diagnose",
    ),
    (
        "runtime ensure",
        "scripts/check-managed-runtime.py",
        "ensure",
    ),
    (
        "runtime foreground",
        "crates/airk-cli/src/runtime_install_commands.rs",
        "Foreground",
    ),
    (
        "runtime inspect",
        "scripts/check-service-install.py",
        "inspect",
    ),
    (
        "runtime inspect-candidate",
        "packages/vscode/src/test/serviceBootstrap.test.ts",
        "inspect-candidate",
    ),
    (
        "runtime inventory",
        "crates/airk-cli/src/runtime_inventory.rs",
        "fn measurement_is_bounded",
    ),
    (
        "runtime launch",
        "crates/airk-cli/tests/service_runtime.rs",
        "launch",
    ),
    (
        "runtime legacy-retirement-candidates",
        "scripts/check-runtime-legacy-retirement.py",
        "f.admin(\"candidates\")",
    ),
    (
        "runtime legacy-retirement-review",
        "scripts/check-runtime-legacy-retirement.py",
        "f.admin(\"review\",",
    ),
    (
        "runtime legacy-retirement-authorize",
        "scripts/check-runtime-legacy-retirement.py",
        "f.admin(\"authorize\",",
    ),
    (
        "runtime legacy-retirement-receipt",
        "scripts/check-runtime-legacy-retirement.py",
        "f.admin(\"receipt\",",
    ),
    (
        "runtime legacy-retirement-resume",
        "scripts/check-runtime-legacy-retirement.py",
        "f.admin(\"resume\",",
    ),
    (
        "runtime lifecycle",
        "crates/airk-cli/src/runtime_managed_tests.rs",
        "lifecycle",
    ),
    (
        "runtime location",
        "packages/vscode/src/test/extensionUninstall.test.ts",
        "location",
    ),
    (
        "runtime login install",
        "scripts/check-macos-direct-runtime.py",
        "runtime(\"login\", \"--install-dir\", install, \"install\")",
    ),
    (
        "runtime login plan",
        "crates/airk-cli/src/runtime_install.rs",
        "Plan",
    ),
    (
        "runtime login start",
        "scripts/check-macos-direct-runtime.py",
        "runtime(\"login\", \"--install-dir\", install, \"start\")",
    ),
    (
        "runtime login status",
        "scripts/check-macos-direct-runtime.py",
        "runtime(\"login\", \"--install-dir\", install, \"status\")",
    ),
    (
        "runtime login stop",
        "scripts/check-macos-direct-runtime.py",
        "runtime(\"login\", \"--install-dir\", install, \"stop\")",
    ),
    (
        "runtime login uninstall",
        "scripts/check-macos-direct-runtime.py",
        "runtime(\"login\", \"--install-dir\", install, \"uninstall\")",
    ),
    (
        "runtime managed",
        "crates/airk-cli/src/runtime_managed_tests.rs",
        "managed",
    ),
    (
        "runtime prepare-client",
        "scripts/check-container.py",
        "prepare-client",
    ),
    (
        "runtime purge",
        "crates/airk-cli/src/runtime_inventory.rs",
        "fn purge_contents_keeps",
    ),
    (
        "runtime remove",
        "scripts/check-runtime-activation.py",
        "remove",
    ),
    (
        "runtime repair-launcher",
        "scripts/check-service-install.py",
        "repair-launcher",
    ),
    (
        "runtime repair-payload",
        "crates/airk-cli/src/runtime_payload_repair.rs",
        "fn ",
    ),
    (
        "runtime reset-startup",
        "scripts/check-service-install.py",
        "reset-startup",
    ),
    (
        "runtime retire-previous",
        "scripts/check-service-install.py",
        "retire-previous",
    ),
    (
        "runtime rollback",
        "crates/airk-cli/src/runtime_activation.rs",
        "rollback",
    ),
    (
        "runtime self-test",
        "crates/airk-cli/tests/bundled_model.rs",
        "self-test",
    ),
    (
        "runtime snapshot",
        "crates/airk-cli/tests/service_runtime.rs",
        "snapshot",
    ),
    (
        "runtime stage",
        "crates/airk-cli/src/runtime_staging_tests.rs",
        "stage",
    ),
    ("runtime start", "scripts/check-service-install.py", "start"),
    (
        "runtime uninstall",
        "packages/vscode/src/test/extensionUninstall.test.ts",
        "uninstall",
    ),
];

/// Commands exercised by the functional tests in this file.
const HERE: &[&str] = &[
    "changes",
    "code-facts",
    "configure",
    "context",
    "explorer draft-plan",
    "explorer draft-search",
    "explorer draft-strategy",
    "explorer get-handoff",
    "explorer get-template",
    "explorer list-templates",
    "explorer proposal-status",
    "explorer suggest-view",
    "explorer validate-draft",
    "facts",
    "files",
    "imports",
    "index",
    "info",
    "inspect",
    "model-catalog",
    "outline",
    "parse",
    "pause",
    "projects",
    "read",
    "refs",
    "relationships",
    "resume",
    "service health",
    "service logs",
    "service prepare-registration",
    "service register",
    "service requests forget",
    "service requests list",
    "service requests prepare",
    "service requests retry",
    "service shutdown",
    "settings",
    "status",
    "symbol",
    "tools call",
    "tools list",
    "usage clear",
    "usage configure",
    "usage feedback",
    "usage policy",
    "usage report",
    "watch",
];

fn reference_commands() -> Result<Vec<String>> {
    let reference = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli-reference.md"),
    )?;
    Ok(reference
        .lines()
        .filter_map(|line| line.strip_prefix("## airk"))
        .map(|rest| rest.trim().to_owned())
        .collect())
}

/// Read only Clap's visible Commands table, ignoring wrapped descriptions and
/// the generated help alias. Never use generated Markdown as command discovery.
fn help_children(help: &str) -> Result<Vec<String>> {
    let mut children = Vec::new();
    let mut table = false;
    for line in help.lines() {
        if line == "Commands:" {
            table = true;
            continue;
        }
        if !table || line.trim().is_empty() {
            continue;
        }
        if !line.starts_with("  ") {
            break;
        }
        let Some(entry) = line
            .strip_prefix("  ")
            .filter(|rest| !rest.starts_with(' '))
        else {
            continue; // Wrapped description; it is not a command row.
        };
        let name = entry
            .split_whitespace()
            .next()
            .context("empty command row")?;
        ensure!(
            name.bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
            "Unexpected native help command row: {line}"
        );
        if name != "help" {
            children.push(name.to_owned());
        }
    }
    Ok(children)
}

fn parser_commands() -> Result<BTreeSet<String>> {
    let mut pending = vec![String::new()];
    let mut commands = BTreeSet::new();
    while let Some(command) = pending.pop() {
        ensure!(
            commands.insert(command.clone()),
            "Duplicate native command path: {command}"
        );
        let args: Vec<&str> = command.split_whitespace().chain(["--help"]).collect();
        let output = run(&args)?;
        ensure!(output.status.success(), "airk {command} --help failed");
        let help = std::str::from_utf8(&output.stdout)?;
        ensure!(help.contains("Usage:"), "airk {command} --help lacks usage");
        for child in help_children(help)? {
            pending.push(if command.is_empty() {
                child
            } else {
                format!("{command} {child}")
            });
        }
    }
    Ok(commands)
}

#[test]
fn every_public_command_has_a_test_and_prints_help() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let groups = [
        "",
        "explorer",
        "runtime",
        "runtime login",
        "service",
        "service legacy",
        "service requests",
        "tools",
        "usage",
    ];
    // Keep parser discovery independent of stale generated docs. Exercise the
    // table reader's boundaries without starting any command beyond --help.
    ensure!(
        help_children(
            "Usage: airk [COMMAND]\n\nCommands:\n  runtime  Runtime commands\n           wrapped description\n  help     Generated alias\n\nOptions:\n  --help  Help\n"
        )? == vec!["runtime".to_owned()]
    );
    ensure!(help_children("Usage: airk read [OPTIONS]\n\nOptions:\n  --help  Help\n")?.is_empty());
    let discovered = parser_commands()?;
    let documented: BTreeSet<_> = reference_commands()?.into_iter().collect();
    let mut missing = Vec::new();
    for command in &discovered {
        if groups.contains(&command.as_str()) || HERE.contains(&command.as_str()) {
            continue;
        }
        match ELSEWHERE
            .iter()
            .find(|(name, _, _)| *name == command.as_str())
        {
            Some((_, "this file", needle)) => {
                ensure!(
                    include_str!("cli_commands.rs")
                        .lines()
                        .any(|line| line.trim_start().starts_with(&format!("fn {needle}"))),
                    "{command}: local coverage function {needle} no longer exists"
                );
            }
            Some((_, file, needle)) => {
                let text = fs::read_to_string(root.join(file))
                    .with_context(|| format!("{command}: {file}"))?;
                ensure!(
                    text.contains(needle),
                    "{command}: {file} no longer contains {needle}"
                );
            }
            None => missing.push(command),
        }
    }
    ensure!(
        missing.is_empty() && discovered == documented,
        "commands without a test: {missing:?}; parser commands missing from generated docs: {:?}; stale documented commands: {:?}",
        discovered.difference(&documented).collect::<Vec<_>>(),
        documented.difference(&discovered).collect::<Vec<_>>()
    );
    Ok(())
}
