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
    assert_eq!(document["capabilities"], serde_json::json!(["tools/1"]));
}

#[test]
fn unserved_commands_and_argument_errors_keep_their_classes() {
    for unserved in [&["index"][..], &["agents", "connect"], &["tools", "shell"]] {
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
