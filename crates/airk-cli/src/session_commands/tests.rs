// SPDX-License-Identifier: Apache-2.0
//! The in-session allowlist against the exact argv shapes the VS Code extension
//! runs today (serviceBootstrap.ts, serviceConsole.ts, serviceExtension.ts,
//! usagePanel.ts with model.ts `launchArgs` in its direct form), and the
//! long-lived, interactive or credential-bearing commands it must refuse.
use super::*;

const INSTALL: &str = "C:\\fixture\\AIRK\\runtime";
const CONTROL: &str = "C:\\fixture\\AIRK\\state\\service-v1\\control";
const SHA: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const ID: &str = "03030303030303030303030303030303";

fn admitted(argv: &[&str]) -> Option<Admission> {
    let argv: Vec<String> = std::iter::once("airk")
        .chain(argv.iter().copied())
        .map(str::to_owned)
        .collect();
    on_cli_stack(move || {
        let cli = Cli::try_parse_from(&argv)
            .unwrap_or_else(|error| panic!("{argv:?} must parse: {error}"));
        admission(&cli)
    })
}

/// model.ts `launchArgs(spec, command)` for a direct service binding.
fn project(command: &[&'static str]) -> Vec<&'static str> {
    let mut argv = vec![
        "--service-install-dir",
        INSTALL,
        "--root",
        "C:\\fixture\\source",
        "--data-dir",
        "C:\\fixture\\data",
        "--collection",
        "default",
        "--service-project",
        SHA,
    ];
    argv.extend_from_slice(command);
    argv
}

#[test]
fn extension_read_only_commands_run_concurrently() {
    let mut concurrent: Vec<Vec<&str>> = [
        &["runtime", "location", "--channel", "stable"][..],
        &["runtime", "self-test"],
        &["runtime", "inspect", "--install-dir", INSTALL],
        &["runtime", "snapshot", "--install-dir", INSTALL],
        &["runtime", "diagnose", "--install-dir", INSTALL],
        &["runtime", "inventory", "--install-dir", INSTALL],
        &[
            "runtime",
            "inspect-candidate",
            "--install-dir",
            INSTALL,
            "--runtime-sha256",
            SHA,
        ],
        &["runtime", "http-endpoint", "--install-dir", INSTALL],
        &[
            "runtime",
            "http-connection-info",
            "--install-dir",
            INSTALL,
            "--connection-id",
            ID,
        ],
        &[
            "runtime",
            "lookup-http-connection",
            "--install-dir",
            INSTALL,
            "--expected-active",
            SHA,
            "--connection-id",
            ID,
        ],
        &[
            "runtime",
            "inspect-legacy-launchers",
            "--install-dir",
            INSTALL,
            "--expected-active",
            SHA,
        ],
        &[
            "runtime",
            "inspect-legacy-caches",
            "--install-dir",
            INSTALL,
            "--expected-active",
            SHA,
            "--global-storage",
            "C:\\fixture\\storage",
        ],
        &[
            "runtime",
            "source-scope",
            "--install-dir",
            INSTALL,
            "--expected-control",
            CONTROL,
            "--expected-runtime",
            SHA,
            "--source-root",
            "C:\\fixture\\source",
        ],
        &[
            "--root",
            "C:\\fixture\\source",
            "--data-dir",
            "C:\\fixture\\data",
            "--collection",
            "default",
            "service",
            "prepare-registration",
        ],
        &[
            "service",
            "logs",
            "--runtime-dir",
            CONTROL,
            "--limit",
            "32",
            "--format",
            "json",
        ],
        &[
            "service",
            "logs",
            "--runtime-dir",
            CONTROL,
            "--all",
            "--format",
            "json",
            "--output",
            "C:\\fixture\\capture.json",
        ],
        &[
            "service",
            "logs",
            "--runtime-dir",
            CONTROL,
            "--raw",
            "--output",
            "C:\\fixture\\raw.jsonl",
        ],
        &["service", "rpc", "--runtime-dir", CONTROL, "--stdin"],
        &[
            "service",
            "legacy",
            "review",
            "--runtime-dir",
            CONTROL,
            "--root",
            "C:/fixture/old",
            "--data-dir",
            "C:/fixture/old-data",
            "--collection",
            "default",
            "--executable",
            "C:/fixture/old/airk.exe",
        ],
        &["service", "legacy", "list", "--runtime-dir", CONTROL],
        &[
            "service",
            "legacy",
            "show",
            "--runtime-dir",
            CONTROL,
            "--approval",
            SHA,
        ],
    ]
    .iter()
    .map(|argv| argv.to_vec())
    .collect();
    concurrent.push(project(&["tools", "call", "airk_status", "--args", "{}"]));
    concurrent.push(project(&[
        "usage", "report", "--days", "7", "--format", "json",
    ]));
    concurrent.push(project(&[
        "usage",
        "import",
        "C:\\fixture\\m.json",
        "--preview",
    ]));
    for argv in &concurrent {
        assert_eq!(admitted(argv), Some(Admission::Concurrent), "{argv:?}");
    }
}

#[test]
fn extension_installer_and_mutating_commands_run_exclusively() {
    let exclusive: &[&[&str]] = &[
        &[
            "runtime",
            "stage",
            "--source",
            "C:\\fixture\\bundle",
            "--install-dir",
            INSTALL,
            "--approved-sha256",
            SHA,
            "--verification-basis",
            "trusted-distribution",
        ],
        &[
            "runtime",
            "managed",
            "--source",
            "C:\\fixture\\bundle",
            "--install-dir",
            INSTALL,
            "--approved-sha256",
            SHA,
            "--verification-basis",
            "trusted-distribution",
            "--lifecycle",
            "external",
        ],
        &[
            "runtime",
            "activate",
            "--install-dir",
            INSTALL,
            "--runtime-sha256",
            SHA,
        ],
        &[
            "runtime",
            "activate",
            "--install-dir",
            INSTALL,
            "--runtime-sha256",
            SHA,
            "--expected-active",
            SHA,
        ],
        &["runtime", "ensure", "--install-dir", INSTALL],
        &["runtime", "start", "--install-dir", INSTALL],
        &["runtime", "prepare-client", "--install-dir", INSTALL],
        &["runtime", "login", "--install-dir", INSTALL, "install"],
        &[
            "runtime",
            "replace-installation",
            "--install-dir",
            INSTALL,
            "--candidate-sha256",
            SHA,
        ],
        &[
            "runtime",
            "replace-installation",
            "--install-dir",
            INSTALL,
            "--candidate-sha256",
            SHA,
            "--apply",
        ],
        &[
            "runtime",
            "repair-payload",
            "--install-dir",
            INSTALL,
            "--source",
            "C:\\fixture\\bundle",
            "--approved-sha256",
            SHA,
        ],
        &["runtime", "lifecycle", "--install-dir", INSTALL],
        &[
            "runtime",
            "abandon-lifecycle",
            "--install-dir",
            INSTALL,
            "--request-id",
            SHA,
        ],
        &[
            "runtime",
            "abort-activation",
            "--install-dir",
            INSTALL,
            "--expected-candidate",
            SHA,
            "--expected-active",
            SHA,
        ],
        &[
            "runtime",
            "purge",
            "--install-dir",
            INSTALL,
            "--select",
            "logs",
        ],
        &[
            "runtime",
            "purge",
            "--install-dir",
            INSTALL,
            "--select",
            "logs",
            "--confirm",
            SHA,
        ],
        &[
            "runtime",
            "http-connection",
            "--install-dir",
            INSTALL,
            "--selection-file",
            "C:\\fixture\\s.json",
            "--connection-id",
            ID,
            "--client",
            "codex",
        ],
        &[
            "runtime",
            "http-trust",
            "--install-dir",
            INSTALL,
            "--action",
            "inspect",
        ],
        &[
            "runtime",
            "http-issuer-rotation",
            "--install-dir",
            INSTALL,
            "--action",
            "status",
        ],
        &[
            "runtime",
            "retire-previous",
            "--install-dir",
            INSTALL,
            "--expected-active",
            SHA,
        ],
        &[
            "runtime",
            "retire-legacy-launchers",
            "--install-dir",
            INSTALL,
            "--expected-active",
            SHA,
        ],
        &[
            "runtime",
            "prepare-legacy-caches",
            "--install-dir",
            INSTALL,
            "--expected-active",
            SHA,
            "--global-storage",
            "C:\\fixture\\storage",
        ],
        &[
            "runtime",
            "retire-legacy-caches",
            "--install-dir",
            INSTALL,
            "--expected-active",
            SHA,
            "--expected-inspection-sha256",
            SHA,
        ],
        &["runtime", "uninstall", "--install-dir", INSTALL],
        &[
            "service",
            "prepare-storage",
            "--runtime-dir",
            CONTROL,
            "--project",
            SHA,
        ],
        &[
            "service",
            "prepare-configuration",
            "--runtime-dir",
            CONTROL,
            "--project",
            SHA,
        ],
        &[
            "service",
            "record-error",
            "--runtime-dir",
            CONTROL,
            "--operation",
            "fixture",
        ],
        &[
            "service",
            "legacy",
            "remove-startup",
            "--runtime-dir",
            CONTROL,
            "--root",
            "C:/fixture/old",
            "--data-dir",
            "C:/fixture/old-data",
            "--collection",
            "default",
            "--executable",
            "C:/fixture/old/airk.exe",
            "--approval",
            SHA,
        ],
    ];
    for argv in exclusive {
        assert_eq!(admitted(argv), Some(Admission::Exclusive), "{argv:?}");
    }
}

#[test]
fn long_lived_interactive_and_credential_bearing_commands_are_refused() {
    let refused: &[&[&str]] = &[
        &["serve", "--runtime-dir", CONTROL],
        &["runtime", "managed-serve", "--install-dir", INSTALL],
        &["runtime", "foreground", "--install-dir", INSTALL],
        &[
            "runtime",
            "launch",
            "--install-dir",
            INSTALL,
            "--",
            "status",
        ],
        &["host"],
        &["mcp"],
        &["bridge", "--runtime-dir", CONTROL, "--project", SHA],
        &["tools", "shell"],
        &["setup", "--launch"],
        &["setup"],
        &["service", "console", "--runtime-dir", CONTROL],
        &["service", "session"],
        &["service", "shutdown", "--runtime-dir", CONTROL],
        // A bearer token must never be returned in a session response.
        &[
            "runtime",
            "http-credential",
            "--install-dir",
            INSTALL,
            "--connection-id",
            ID,
            "--format",
            "header-json",
        ],
        &[
            "runtime",
            "http-revoke",
            "--install-dir",
            INSTALL,
            "--connection-id",
            ID,
        ],
        &["runtime", "rollback", "--install-dir", INSTALL],
        &["project-worker"],
        &["status"],
        &["index"],
        &["uninstall", "--yes"],
    ];
    for argv in refused {
        assert_eq!(admitted(argv), None, "{argv:?}");
    }
}
