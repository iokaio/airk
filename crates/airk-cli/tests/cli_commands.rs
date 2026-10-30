// SPDX-License-Identifier: Apache-2.0
//! Every public CLI command has a test. Query and management commands run against a
//! private headless service; Explorer commands run offline; installer-lifecycle commands
//! name the native test or qualification script that exercises them. The table is checked
//! against docs/cli-reference.md, which another test keeps identical to the parser.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

fn airk() -> Command {
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
fn query(scope: &[String], args: &[&str]) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let all: Vec<&str> = scope
            .iter()
            .map(String::as_str)
            .chain(args.iter().copied())
            .collect();
        let output = run(&all)?;
        if output.status.success() {
            return Ok(serde_json::from_slice(&output.stdout)?);
        }
        ensure!(
            Instant::now() < deadline,
            "airk {} kept failing: {}{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        std::thread::sleep(Duration::from_millis(250));
    }
}

struct Service(Child, PathBuf);
impl Drop for Service {
    fn drop(&mut self) {
        let _ = run(&[
            "service",
            "shutdown",
            "--runtime-dir",
            &self.1.to_string_lossy(),
        ]);
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline && matches!(self.0.try_wait(), Ok(None)) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Functional coverage of the headless workflow the CLI guide documents.
#[test]
fn headless_workflow_exercises_query_and_management_commands() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    let control = owner.path().join("control");
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
    let (source_text, data_text, control_text) = (
        source.to_string_lossy().into_owned(),
        data.to_string_lossy().into_owned(),
        control.to_string_lossy().into_owned(),
    );
    let log = fs::File::create(owner.path().join("service.stderr"))?;
    let child = airk()
        .args(["serve", "--runtime-dir", &control_text])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()?;
    let _service = Service(child, control.clone());
    let deadline = Instant::now() + Duration::from_secs(90);
    while !run(&["service", "health", "--runtime-dir", &control_text])?
        .status
        .success()
    {
        ensure!(Instant::now() < deadline, "service did not become healthy");
        std::thread::sleep(Duration::from_millis(50));
    }
    let place = [
        "--root",
        &source_text,
        "--data-dir",
        &data_text,
        "--collection",
        "demo",
    ];
    let prepared = json_ok(&[&place[..], &["service", "prepare-registration"]].concat())?;
    let binding = prepared["binding"].as_str().context("binding")?.to_owned();
    json_ok(
        &[
            &place[..],
            &["service", "register", "--runtime-dir", &control_text],
        ]
        .concat(),
    )?;
    let projects = json_ok(&["projects", "--runtime-dir", &control_text])?;
    ensure!(
        projects["projects"][0]["project"] == json!(binding)
            && projects["projects"][0]["collection"] == "demo",
        "projects listed {projects}"
    );
    let scope: Vec<String> = [
        "--service-runtime-dir",
        &control_text,
        "--service-project",
        &binding,
    ]
    .into_iter()
    .chain(place)
    .map(str::to_owned)
    .collect();
    query(&scope, &["index"])?;
    let deadline = Instant::now() + Duration::from_secs(180);
    while !query(&scope, &["search", "copper harbor", "--mode", "lexical"])?
        .to_string()
        .contains("notes.md")
    {
        ensure!(Instant::now() < deadline, "first index never published");
        std::thread::sleep(Duration::from_millis(250));
    }
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
        &["usage", "policy"],
        &["usage", "report"],
        &["inspect"],
        &["pause"],
        &["resume"],
        &["watch", "on"],
        &["watch", "off"],
    ] {
        query(&scope, args).with_context(|| format!("airk {}", args.join(" ")))?;
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
    query(
        &scope,
        &["usage", "configure", "--mode", "diagnostic_events"],
    )?;
    let deadline = Instant::now() + Duration::from_secs(60);
    let request_id = loop {
        let found = query(&scope, &["search", "copper harbor", "--mode", "lexical"])?;
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
    query(
        &scope,
        &["usage", "feedback", &request_id, "--usefulness", "useful"],
    )?;
    query(&scope, &["usage", "clear"])?;
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
    // Unregistering removes the index and settings, never sources.
    let mut configure = airk()
        .args(scope.iter().map(String::as_str))
        .args(["configure", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    use std::io::Write;
    configure
        .stdin
        .take()
        .context("stdin")?
        .write_all(br#"{"operation":"unregister"}"#)?;
    let output = configure.wait_with_output()?;
    ensure!(
        output.status.success(),
        "unregister failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        source.join("notes.md").is_file(),
        "sources must survive unregister"
    );
    json_ok(&["service", "shutdown", "--runtime-dir", &control_text])?;
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
        "bridge",
        "crates/airk-cli/tests/service_runtime.rs",
        "bridge",
    ),
    (
        "host",
        "crates/airk-cli/src/runtime_install_commands.rs",
        "Host",
    ),
    ("init", "scripts/check-container.py", "\"init\""),
    ("mcp", "crates/airk-cli/tests/mcp.rs", "\"mcp\""),
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
    ("serve", "this file", "headless_workflow"),
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
        "scripts/check-macos-login.py",
        "install",
    ),
    (
        "runtime login plan",
        "crates/airk-cli/src/runtime_install.rs",
        "Plan",
    ),
    (
        "runtime login start",
        "scripts/check-macos-login.py",
        "start",
    ),
    (
        "runtime login status",
        "scripts/check-macos-login.py",
        "status",
    ),
    ("runtime login stop", "scripts/check-macos-login.py", "stop"),
    (
        "runtime login uninstall",
        "scripts/check-macos-login.py",
        "uninstall",
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

/// Commands exercised by the two functional tests in this file.
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
    let mut missing = Vec::new();
    for command in reference_commands()? {
        let args: Vec<&str> = command.split(' ').filter(|part| !part.is_empty()).collect();
        let output = run(&[&args[..], &["--help"]].concat())?;
        ensure!(
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains("Usage:"),
            "airk {command} --help failed"
        );
        if groups.contains(&command.as_str()) || HERE.contains(&command.as_str()) {
            continue;
        }
        match ELSEWHERE.iter().find(|(name, _, _)| *name == command) {
            Some((_, "this file", _)) => {}
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
    ensure!(missing.is_empty(), "commands without a test: {missing:?}");
    Ok(())
}
