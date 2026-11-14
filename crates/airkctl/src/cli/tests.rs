// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn startup_failure_uses_i2_without_constructing_the_parser() {
    for output in ["json", "text"] {
        let args = ["airkctl", "--output", output, "status"].map(OsString::from);
        let mut stderr = Vec::new();
        assert_eq!(startup_failure(&args, &mut stderr, "thread unavailable"), 8);
        if output == "json" {
            let line: Value = serde_json::from_slice(&stderr).unwrap();
            assert_eq!(line["class"], "internal");
            assert_eq!(line["code"], "internal");
            assert_eq!(line["retryable"], false);
            assert_eq!(line["message"], "thread unavailable");
        } else {
            assert_eq!(stderr, b"airkctl: thread unavailable\n");
        }
    }
}

/// Run a command line in process; returns (exit, stdout, stderr).
fn run_line(arguments: &[&str]) -> (u8, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut io = Io {
        read_attempt: None,
        stdin: &mut std::io::empty(),
        stdout: &mut stdout,
        stderr: &mut stderr,
        full_responses: false,
        window: None,
    };
    let mut line = vec![OsString::from("airkctl")];
    line.extend(arguments.iter().map(OsString::from));
    let code = run(line, &mut io);
    (
        code,
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

#[test]
fn version_answers_locally_in_both_formats() {
    let (code, stdout, stderr) = run_line(&["--version"]);
    assert_eq!(
        (code, stdout.as_str(), stderr.as_str()),
        (0, &*format!("airkctl {VERSION}\n"), "")
    );
    let (code, stdout, _) = run_line(&["version"]);
    assert_eq!(code, 0);
    assert!(stdout.starts_with(&format!("airkctl {VERSION} (AIRK Service)\n")));
    let (code, stdout, _) = run_line(&["--output", "json", "version"]);
    assert_eq!(code, 0);
    let document: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        document["capabilities"],
        serde_json::json!([
            "session/2",
            "events/1",
            "journal/1",
            "tools/1",
            "explorer/1",
            "agents/1",
            "agents-launch/1",
            "agents-verify/1",
            "agents-entry-remove/1",
            "admin-launch/1",
            "window-scope/1"
        ])
    );
}

#[test]
fn exact_entry_raw_argument_bound_and_conflicts_are_checked_before_discovery() {
    let selector = r#"{"agent":"claude","scope":"project","file":"/fixture/.mcp.json","containerPath":["mcpServers"],"serverKey":"airk","transport":"stdio","status":"stdio_retired"}"#;
    let at_limit = format!(
        "{selector}{}",
        " ".repeat(crate::contract::MAX_INVOKE_ARG_BYTES - selector.len())
    );
    assert!(parse_agent_entry(&at_limit).is_ok());
    assert!(parse_agent_entry(&(at_limit + " ")).is_err());
    for args in [
        vec!["agents", "remove", "--entry", "{}"],
        vec!["agents", "remove", "--entry"],
        vec![
            "agents",
            "remove",
            "--entry",
            selector,
            "--connection-id",
            "11111111111111111111111111111111",
        ],
    ] {
        let (code, stdout, _) = run_line(&args);
        assert_eq!(code, 2, "{args:?}");
        assert!(stdout.is_empty());
    }
}

#[test]
fn unserved_commands_and_argument_errors_keep_their_classes() {
    for unserved in [
        &["explorer", "templates"][..],
        // `service login` is served since U101; another unknown subcommand.
        &["library", "export"],
    ] {
        let (code, _, stderr) = run_line(unserved);
        assert_eq!(code, 5, "{unserved:?}");
        assert!(stderr.contains("not served"), "{unserved:?}: {stderr}");
    }
    for invalid in [
        &[][..],
        &["search"],
        &["tools", "list", "--bogus"],
        &["--output", "yaml", "version"],
    ] {
        assert_eq!(run_line(invalid).0, 2, "{invalid:?}");
    }
    let (code, _, stderr) = run_line(&["--output", "json", "search"]);
    assert_eq!(code, 2);
    let line: Value = serde_json::from_str(stderr.trim()).unwrap();
    assert_eq!(
        (line["class"].as_str(), line["code"].as_str()),
        (Some("usage"), Some("usage"))
    );
    assert_eq!(
        run_line(&[
            "tools",
            "call",
            "x",
            "--args",
            "[1]",
            "--service-runtime-dir",
            "."
        ])
        .2,
        "airkctl: MCP arguments must be a JSON object\n"
    );
}

#[test]
fn a_missing_installation_or_service_exits_three() {
    // Discovery reads the fixed per-user pointer (no override, by design), so
    // the missing-installation branch is observable only on a host without an
    // installation (CI); the missing-service branch below runs everywhere.
    if matches!(
        crate::install::discover(),
        crate::install::Discovery::NotFound
    ) {
        let (code, _, stderr) = run_line(&["status"]);
        assert_eq!(code, 3);
        assert!(stderr.contains("--service-runtime-dir"), "{stderr}");
    }
    let missing = std::env::temp_dir().join("airkctl-no-such-control-root");
    let (code, _, stderr) = run_line(&[
        "--output",
        "json",
        "tools",
        "list",
        "--service-runtime-dir",
        missing.to_str().unwrap(),
    ]);
    assert_eq!(code, 3);
    let line: Value = serde_json::from_str(stderr.trim()).unwrap();
    assert_eq!(line["code"], "service_unavailable");
    assert_eq!(line["retryable"], true);
}

#[test]
fn parse_project_conflicts_are_refused_before_the_service() {
    let first = "ab".repeat(32);
    let second = "cd".repeat(32);
    let (code, _, stderr) = run_line(&[
        "--service-project",
        &first,
        "parse",
        "q",
        "--project",
        &second,
    ]);
    assert_eq!(
        (code, stderr.as_str()),
        (2, "airkctl: conflicting project selections\n")
    );
}

#[test]
fn plain_drops_only_the_verbatim_drive_prefix() {
    assert_eq!(
        plain(PathBuf::from(r"\\?\D:\tools\airkctl.exe")),
        PathBuf::from(r"D:\tools\airkctl.exe")
    );
    assert_eq!(
        plain(PathBuf::from(r"\\?\UNC\server\share\airkctl.exe")),
        PathBuf::from(r"\\?\UNC\server\share\airkctl.exe")
    );
    assert_eq!(
        plain(PathBuf::from("/usr/local/bin/airkctl")),
        PathBuf::from("/usr/local/bin/airkctl")
    );
}

/// C11a: the embedded `airk-mcp` help catalog is exactly this parser's help.
#[test]
fn help_catalog_matches_the_embedded_airk_mcp_catalog() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../airk-mcp/src/help/cli_commands.json");
    // Clap renders the whole tree; give it a thread with room to do so.
    let catalog = std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(help_catalog)
        .unwrap()
        .join()
        .unwrap();
    assert!(catalog.contains_key("agents connect"));
    assert!(!catalog.keys().any(|key| key.starts_with("runtime")));
    let text = serde_json::to_string_pretty(&catalog).unwrap() + "\n";
    if std::env::var_os("AIRK_WRITE_CLI_HELP").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let saved = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        saved == text,
        "regenerate with AIRK_WRITE_CLI_HELP=1 cargo test -p airkctl --lib help_catalog"
    );
}

/// `docs/cli-reference.md` is generated from this parser (split Phase 4; it
/// documented `airk` before the user CLI moved here).
fn cli_reference() -> String {
    let catalog = std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(help_catalog)
        .unwrap()
        .join()
        .unwrap();
    let title = |name: &str| {
        if name.is_empty() {
            "airkctl".to_owned()
        } else {
            format!("airkctl {name}")
        }
    };
    let mut text = String::from(
        "# AIRK CLI reference\n\n<!-- Generated from airkctl's parser by `cargo test -p airkctl --lib cli_reference`; do not edit. Set AIRK_WRITE_CLI_REFERENCE=1 to regenerate. -->\n\nEvery public `airkctl` command and subcommand, exactly as `airkctl <command> --help` prints it. `airk` itself is the service and takes no commands. For concepts and workflows, see [Running AIRK headless](headless-cli.md) and [the CLI guide](cli.md).\n\n## Contents\n\n",
    );
    for name in catalog.keys() {
        let title = title(name);
        text.push_str(&format!("- [`{title}`](#{})\n", title.replace(' ', "-")));
    }
    for (name, help) in &catalog {
        text.push_str(&format!(
            "\n## {}\n\n```text\n{}\n```\n",
            title(name),
            help.trim_end()
        ));
    }
    text.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn cli_reference_documents_every_public_command() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli-reference.md");
    let expected = cli_reference();
    if std::env::var_os("AIRK_WRITE_CLI_REFERENCE").is_some() {
        std::fs::write(&path, &expected).unwrap();
    }
    let saved = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        saved == expected,
        "regenerate with AIRK_WRITE_CLI_REFERENCE=1 cargo test -p airkctl --lib cli_reference"
    );
}

/// U104: `library set-create --index` checks its indexes before any request.
#[test]
fn set_create_indexes_are_checked_before_the_service() {
    let missing = std::env::temp_dir().join("airkctl-no-such-control-root");
    let missing = missing.to_str().unwrap();
    let (code, _, stderr) = run_line(&[
        "library",
        "set-create",
        "--name",
        "Docs",
        "--index",
        "not-an-index",
        "--runtime-dir",
        missing,
    ]);
    assert_eq!(
        (code, stderr.as_str()),
        (2, "airkctl: invalid --index: not-an-index\n")
    );
    let index = "idx_01010101010101010101010101010101";
    let (code, _, stderr) = run_line(&[
        "library",
        "set-create",
        "--name",
        "Docs",
        "--index",
        index,
        "--index",
        index,
        "--runtime-dir",
        missing,
    ]);
    assert_eq!(
        (code, stderr.as_str()),
        (
            2,
            "airkctl: --index idx_01010101010101010101010101010101 is named twice\n"
        )
    );
}

/// U105: open read, connection and description arguments are checked before
/// the service.
#[test]
fn open_agent_arguments_are_checked_before_the_service() {
    let missing = std::env::temp_dir().join("airkctl-no-such-control-root");
    let missing = missing.to_str().unwrap();
    let project = "ab".repeat(32);
    let (code, _, stderr) = run_line(&[
        "--service-project",
        &project,
        "--service-runtime-dir",
        missing,
        "tools",
        "call",
        "search",
        "--index",
        "docs",
    ]);
    assert_eq!(
        (code, stderr.as_str()),
        (
            2,
            "airkctl: --index, --set and --connection cannot be combined with a project or session selection\n"
        )
    );
    let mut too_many = vec!["--service-runtime-dir", missing, "tools", "call", "search"];
    let names: Vec<String> = (0..33).map(|n| format!("index{n}")).collect();
    for name in &names {
        too_many.extend(["--index", name.as_str()]);
    }
    assert_eq!(run_line(&too_many).0, 2);
    let connection = "1c".repeat(16);
    for invalid in [
        // `connect` needs --connection-id or --open, never both.
        &["agents", "connect", "--client", "codex"][..],
        &[
            "agents",
            "connect",
            "--client",
            "codex",
            "--open",
            "--connection-id",
            &connection,
        ],
        &[
            "agents",
            "default",
            "--connection-id",
            &connection,
            "--clear",
            "--index",
            "docs",
        ],
        &[
            "agents",
            "default",
            "--connection-id",
            &connection,
            "--agent-choice",
            "maybe",
        ],
        &["library", "describe", "--text", "x"],
        &[
            "library",
            "describe",
            "--index",
            "idx_01010101010101010101010101010101",
            "--text",
            "x",
            "--clear",
        ],
    ] {
        assert_eq!(run_line(invalid).0, 2, "{invalid:?}");
    }
}

#[test]
fn portable_recovery_parser_accepts_only_original_request_not_pid_or_location_overrides() {
    assert!(
        Cli::try_parse_from([
            "airkctl",
            "service",
            "setup",
            "--resume",
            "/private/original.json"
        ])
        .is_ok()
    );
    for args in [
        vec![
            "airkctl",
            "service",
            "setup",
            "--resume",
            "/private/original.json",
            "--pid",
            "1234",
        ],
        vec![
            "airkctl",
            "service",
            "setup",
            "--resume",
            "/private/original.json",
            "--program",
            "/other",
        ],
        vec![
            "airkctl",
            "service",
            "setup",
            "--resume",
            "/private/original.json",
            "--data",
            "/other",
        ],
    ] {
        assert!(Cli::try_parse_from(args).is_err());
    }
}
