// SPDX-License-Identifier: Apache-2.0
use super::*;

/// Run a command line in process; returns (exit, stdout, stderr).
fn run_line(arguments: &[&str]) -> (u8, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut io = Io {
        stdin: &mut std::io::empty(),
        stdout: &mut stdout,
        stderr: &mut stderr,
        full_responses: false,
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
            "admin-launch/1"
        ])
    );
}

#[test]
fn unserved_commands_and_argument_errors_keep_their_classes() {
    for unserved in [
        &["explorer", "templates"][..],
        &["service", "login", "status"],
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
    let (code, _, stderr) = run_line(&["status"]);
    assert_eq!(code, 3);
    assert!(stderr.contains("--service-runtime-dir"), "{stderr}");
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
