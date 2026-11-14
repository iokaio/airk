// SPDX-License-Identifier: Apache-2.0
use super::*;
use clap::Parser;
use std::{cell::RefCell, ffi::OsString, path::PathBuf, rc::Rc};

fn failure(class: ExitClass, code: &str) -> ErrorLine {
    ErrorLine::new(class, code, false, code)
}

#[test]
fn online_admission_preserves_running_exited_stop_and_mismatch_outcomes() {
    for (name, probe, start, expected, starts) in [
        ("running", None, None, None, 0),
        ("exited", Some(ExitClass::ServiceMissing), None, None, 1),
        (
            "explicit_stop",
            Some(ExitClass::ServiceMissing),
            Some(ExitClass::Refused),
            Some(ExitClass::Refused),
            1,
        ),
        (
            "pair_mismatch",
            Some(ExitClass::ServiceMissing),
            Some(ExitClass::IncompatiblePair),
            Some(ExitClass::IncompatiblePair),
            1,
        ),
        (
            "incompatible_health",
            Some(ExitClass::IncompatiblePair),
            None,
            Some(ExitClass::IncompatiblePair),
            0,
        ),
        (
            "uncertain_transport",
            Some(ExitClass::Internal),
            None,
            Some(ExitClass::Internal),
            0,
        ),
        (
            "native_refusal",
            Some(ExitClass::Refused),
            None,
            Some(ExitClass::Refused),
            0,
        ),
    ] {
        let probes = std::cell::Cell::new(0);
        let called = std::cell::Cell::new(0);
        let result = admit(
            || {
                probes.set(probes.get() + 1);
                probe.map_or(Ok(()), |class| Err(failure(class, name)))
            },
            |mode, original| {
                called.set(called.get() + 1);
                assert!(matches!(mode, StartMode::Automatic));
                assert_eq!(original.class, ExitClass::ServiceMissing);
                start.map_or(Ok(()), |class| Err(failure(class, name)))
            },
        );
        assert_eq!(probes.get(), 1, "{name}");
        assert_eq!(called.get(), starts, "{name}");
        assert_eq!(
            result.as_ref().err().map(|line| line.class),
            expected,
            "{name}"
        );
        if let Err(line) = result {
            assert_eq!(line.code, name, "original refusal must survive");
        }
    }
}

struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        PROBE.with_borrow_mut(|probe| *probe = None);
    }
}
fn invoke(arguments: &[String]) -> (u8, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut stdin = std::io::empty();
    let mut io = super::super::Io {
        stdin: &mut stdin,
        stdout: &mut stdout,
        stderr: &mut stderr,
        full_responses: false,
        window: None,
    };
    let code = super::super::run(
        std::iter::once(OsString::from("airkctl"))
            .chain(arguments.iter().map(OsString::from))
            .collect(),
        &mut io,
    );
    (code, String::from_utf8(stderr).unwrap())
}

fn parsed(arguments: &[String], allow_help: bool) {
    let result = super::super::Cli::try_parse_from(
        std::iter::once(OsString::from("airkctl")).chain(arguments.iter().map(OsString::from)),
    );
    if let Err(error) = result {
        assert!(
            allow_help
                && matches!(
                    error.kind(),
                    clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
                ),
            "roster contains unsupported argv {arguments:?}: {error}"
        );
    }
}

#[test]
fn online_command_families_enter_admission_while_observation_roster_does_not() {
    let temporary = tempfile::tempdir().unwrap();
    let control = temporary.path().join("absent-control");
    let data = temporary.path().join("data");
    let alternate = temporary.path().join("alternate-control");
    let control_text = control.to_str().unwrap();
    let data_text = data.to_str().unwrap();
    let alternate_text = alternate.to_str().unwrap();
    let binding = "4b".repeat(32);
    // Independent actual argv roster. Do not derive this set from command catalogs.
    for argv in [
        vec!["tools", "list"],
        vec!["tools", "help", "search"],
        vec!["tools", "call", "airk_help", "--args", "{}"],
        vec!["tools", "call", "list_indexes", "--args", "{}"],
        vec![
            "tools",
            "call",
            "search",
            "--index",
            "idx_01010101010101010101010101010101",
            "--args",
            r#"{"query":"scope"}"#,
        ],
        vec!["library", "list"],
        vec![
            "library",
            "show",
            "--index",
            "idx_01010101010101010101010101010101",
        ],
        vec![
            "library",
            "setup",
            "--name",
            "Fixture",
            "--data-dir",
            data_text,
        ],
        vec!["service", "inventory"],
        vec!["service", "purge-plan", "--kind", "cache"],
        vec!["folders"],
        vec!["projects"],
        vec!["status", "--service-project", binding.as_str()],
        vec!["usage", "report", "--service-project", binding.as_str()],
        vec!["pause", "--service-project", binding.as_str()],
        vec!["jobs", "get", binding.as_str()],
        vec!["arbiter", "status"],
    ]
    .into_iter()
    .map(|argv| (argv, control.clone()))
    .chain([
        (
            vec!["library", "list", "--runtime-dir", alternate_text],
            alternate.clone(),
        ),
        (
            vec!["folders", "--runtime-dir", alternate_text],
            alternate.clone(),
        ),
        (
            vec!["projects", "--runtime-dir", alternate_text],
            alternate.clone(),
        ),
        (
            vec!["service", "inventory", "--runtime-dir", alternate_text],
            alternate.clone(),
        ),
    ]) {
        let (argv, expected_control) = argv;
        let observed = Rc::new(RefCell::new(Vec::new()));
        let log = observed.clone();
        PROBE.with_borrow_mut(|probe| {
            *probe = Some(Box::new(move |path| {
                log.borrow_mut().push(path.to_owned());
                Err(failure(ExitClass::Refused, "admission_probe"))
            }))
        });
        let _reset = Reset;
        let arguments = ["--service-runtime-dir", control_text, "--output", "json"]
            .into_iter()
            .chain(argv.iter().copied())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        parsed(&arguments, false);
        let (code, stderr) = invoke(&arguments);
        assert_eq!(
            *observed.borrow(),
            vec![expected_control],
            "{argv:?}: {stderr}"
        );
        assert_eq!(code, 1, "{argv:?}: {stderr}");
        assert!(stderr.contains("admission_probe"), "{argv:?}: {stderr}");
    }
    for argv in [
        vec!["version"],
        vec!["--help"],
        vec!["service", "status"],
        vec!["service", "health"],
        vec!["service", "shutdown"],
        vec!["service", "stop"],
        vec!["service", "restart"],
        vec!["service", "logs", "--all"],
        vec!["service", "login", "status"],
        vec!["service", "setup-receipt"],
        vec!["service", "setup", "--portable", "--program", control_text],
        vec!["service", "setup", "--resume", control_text],
    ] {
        let observed = Rc::new(RefCell::new(Vec::<PathBuf>::new()));
        let log = observed.clone();
        PROBE.with_borrow_mut(|probe| {
            *probe = Some(Box::new(move |path| {
                log.borrow_mut().push(path.to_owned());
                Err(failure(ExitClass::Refused, "admission_probe"))
            }))
        });
        let _reset = Reset;
        let arguments = ["--service-runtime-dir", control_text, "--output", "json"]
            .into_iter()
            .chain(argv.iter().copied())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        parsed(&arguments, true);
        let _ = invoke(&arguments);
        assert!(
            observed.borrow().is_empty(),
            "nonstarting observation/lifecycle {argv:?} entered automatic admission"
        );
    }
}

#[test]
fn online_window_admission_preserves_original_bound_selection() {
    use super::super::*;
    use airk_core::{identity::Incarnation, service::WindowLease};
    for permitted in [true, false] {
        let control = PathBuf::from("window-control-fixture");
        let observed = Rc::new(RefCell::new(Vec::new()));
        let log = observed.clone();
        PROBE.with_borrow_mut(|probe| {
            *probe = Some(Box::new(move |path| {
                log.borrow_mut().push(path.to_owned());
                if permitted {
                    Ok(())
                } else {
                    Err(failure(ExitClass::Refused, "service_explicitly_stopped"))
                }
            }))
        });
        let _reset = Reset;
        let id = Incarnation::from_bytes([3; 16]);
        let lease = WindowLease::from_bytes([4; 32]);
        let instance = Incarnation::from_bytes([5; 16]);
        let mut route = WindowRoute {
            window_id: id,
            lease: lease.clone(),
            instance,
            refused: None,
        };
        let mut stdin = std::io::empty();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut io = Io {
            stdin: &mut stdin,
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: false,
            window: Some(&mut route),
        };
        let mut cx = Context {
            output: Output::Json,
            io: &mut io,
        };
        let selection = Selection {
            control: Some(control.clone()),
            project: None,
            catalog: None,
            combined: None,
            project_ids: None,
            index_ids: None,
            member_availability: None,
            root: PathBuf::from("."),
            data_dir: None,
            collection: "fixture".into(),
        };
        let resolved = resolve_call(&mut cx, &selection, "search");
        if permitted {
            let (target, selected) = resolved.unwrap();
            assert_eq!(target, control);
            let ToolSelection::Window {
                window_id,
                lease: selected_lease,
            } = selected
            else {
                panic!("window must preserve its original authority")
            };
            assert_eq!(window_id, id);
            assert!(selected_lease == lease);
        } else {
            assert!(resolved.is_err());
        }
        assert_eq!(*observed.borrow(), vec![control]);
        assert_eq!(route.window_id, id);
        assert_eq!(route.instance, instance);
        assert!(route.lease == lease);
        assert!(route.refused.is_none());
        assert!(!String::from_utf8(stderr).unwrap().contains(lease.as_str()));
        assert!(stdout.is_empty());
    }
}
