// SPDX-License-Identifier: Apache-2.0
//! Process environment and exit semantics must run the actual airkctl executable.
use serde_json::Value;
use std::process::Command;

#[test]
fn real_process_constructs_the_parser_for_offline_commands_and_errors()
-> Result<(), Box<dyn std::error::Error>> {
    for (arguments, expected) in [
        (vec!["--help"], 0),
        (vec!["--version"], 0),
        (vec!["--output", "json", "search"], 2),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_airkctl"))
            .args(&arguments)
            .output()?;
        assert_eq!(
            output.status.code(),
            Some(expected),
            "{arguments:?}: {output:?}"
        );
        if expected == 0 {
            assert!(!output.stdout.is_empty());
            assert!(output.stderr.is_empty());
        } else {
            assert!(output.stdout.is_empty());
            let outcome: Value = serde_json::from_slice(&output.stderr)?;
            assert_eq!(outcome["class"], "usage");
        }
    }
    Ok(())
}

#[test]
fn service_refusal_is_backtrace_independent_in_the_real_process()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let missing = directory.path().join("missing-runtime");
    for backtrace in ["0", "1", "full"] {
        for format in ["json", "text"] {
            let output = Command::new(env!("CARGO_BIN_EXE_airkctl"))
                .arg("--service-runtime-dir")
                .arg(&missing)
                // The catalog needs no project/data discovery or auto-start;
                // it reaches only the explicitly selected missing endpoint.
                .args(["--output", format, "tools", "list"])
                .env_remove("AIRK_ERROR_FORMAT")
                .env("RUST_BACKTRACE", backtrace)
                .env("RUST_LIB_BACKTRACE", backtrace)
                .output()?;
            assert_eq!(
                output.status.code(),
                Some(3),
                "{backtrace}/{format}: {output:?}"
            );
            assert!(output.stdout.is_empty(), "{backtrace}/{format}: {output:?}");
            if format == "json" {
                let outcome: Value = serde_json::from_slice(&output.stderr)?;
                assert_eq!(outcome["airkctl_error"], 1);
                assert_eq!(outcome["class"], "service_missing");
                assert_eq!(outcome["code"], "service_unavailable");
                assert_eq!(outcome["retryable"], true);
                assert!(outcome["message"].as_str().unwrap().contains("not running"));
            } else {
                assert!(String::from_utf8_lossy(&output.stderr).starts_with("airkctl: "));
            }
            assert!(!String::from_utf8_lossy(&output.stderr).contains("stack backtrace"));
            assert!(
                !missing.exists(),
                "missing service must not create runtime state"
            );
        }
    }
    Ok(())
}
