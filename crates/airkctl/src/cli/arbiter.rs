// SPDX-License-Identifier: Apache-2.0
//! Explicit native optional-model management; no implicit enrollment or downloads.
use super::*;
use airk_core::{
    arbiter::{ManagementRequest, Stage},
    identity::Sha256Digest,
};
#[derive(Subcommand)]
pub(super) enum Command {
    /// Inspect optional model readiness, selection and exact per-index enrollment.
    Status,
    /// Inspect retained CPU configuration/3 and status/2 without device migration.
    StatusLegacy,
    /// Explicitly create disabled Focus/Context state, preserving retained old consent and assets.
    Initialize {
        /// Use retained CPU configuration/3 only; current device state refuses this path.
        #[arg(long)]
        legacy: bool,
        #[arg(long)]
        expected_legacy_configuration: Option<Sha256Digest>,
    },
    /// Migrate the exact current CPU configuration to the device configuration under native custody.
    Migrate {
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        expected_configuration: Sha256Digest,
    },
    /// Apply an exact current or retained CPU native Configure request.
    Configure {
        /// Use retained CPU configuration/3 only; current device state refuses this path.
        #[arg(long)]
        legacy: bool,
        #[arg(long)]
        file: PathBuf,
    },
    /// Explicitly acquire the compiled qualified profile asset closure.
    Provision {
        /// Use retained CPU configuration/3 only; current device state refuses this path.
        #[arg(long)]
        legacy: bool,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        profile: Sha256Digest,
    },
    /// Import the exact compiled qualified asset closure from a read-only local bundle.
    Import {
        /// Use retained CPU configuration/3 only; current device state refuses this path.
        #[arg(long)]
        legacy: bool,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        profile: Sha256Digest,
        #[arg(long)]
        directory: String,
    },
    /// Cancel this native optional operation; running native calls drain before removal.
    Cancel {
        /// Use retained CPU configuration/3 only; current device state refuses this path.
        #[arg(long)]
        legacy: bool,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long, value_parser = stage)]
        stage: Stage,
    },
    /// Explicitly remove one optional profile after actual native pins drain.
    Remove {
        /// Use retained CPU configuration/3 only; current device state refuses this path.
        #[arg(long)]
        legacy: bool,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        profile: Sha256Digest,
    },
    /// Recover a retained optional model operation under the current configuration revision.
    Recover {
        /// Use retained CPU configuration/3 only; current device state refuses this path.
        #[arg(long)]
        legacy: bool,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long, value_parser = stage)]
        stage: Stage,
    },
}
fn stage(value: &str) -> Result<Stage, String> {
    serde_json::from_value(serde_json::Value::String(value.into()))
        .map_err(|error| format!("invalid native Arbiter stage (focus or context): {error}"))
}
fn request(action: Command) -> Result<ManagementRequest, String> {
    Ok(match action {
        Command::Import { .. } => {
            return Err("local import uses its private native control operation".into());
        }
        Command::Status => ManagementRequest::StatusDevice,
        Command::StatusLegacy => ManagementRequest::Status,
        Command::Migrate {
            expected_revision,
            expected_configuration,
        } => ManagementRequest::MigrateDevice {
            expected_revision,
            expected_configuration,
        },
        Command::Initialize {
            expected_legacy_configuration,
            legacy,
        } => {
            if legacy {
                ManagementRequest::Initialize {
                    expected_legacy_configuration,
                }
            } else {
                ManagementRequest::InitializeDevice {
                    expected_legacy_configuration,
                }
            }
        }
        Command::Configure { file, legacy } => {
            use std::io::Read;
            let mut bytes = Vec::new();
            std::fs::File::open(&file)
                .map_err(|e| e.to_string())?
                .take(airk_core::arbiter::MAX_CONFIGURATION_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > airk_core::arbiter::MAX_CONFIGURATION_BYTES {
                return Err("native Arbiter configuration request exceeds its byte limit".into());
            }
            let request: ManagementRequest = serde_json::from_slice(&bytes)
                .map_err(|e| format!("invalid native Arbiter Configure request: {e}"))?;
            match &request {
                ManagementRequest::Configure { .. } if legacy => {}
                ManagementRequest::ConfigureDevice {
                    stage,
                    configuration,
                    ..
                } if !legacy => configuration.validate(*stage).map_err(|error| {
                    format!("invalid native Arbiter device Configure request: {error:#}")
                })?,
                _ => {
                    return Err(
                        "arbiter configure --file requires configure_device; --legacy requires the retained CPU configure operation"
                            .into(),
                    );
                }
            }
            request
        }
        Command::Provision {
            expected_revision,
            profile,
            legacy,
        } => {
            if legacy {
                ManagementRequest::Provision {
                    expected_revision,
                    profile,
                }
            } else {
                ManagementRequest::ProvisionDevice {
                    expected_revision,
                    profile,
                }
            }
        }
        Command::Cancel {
            expected_revision,
            stage,
            legacy,
        } => {
            if legacy {
                ManagementRequest::Cancel {
                    expected_revision,
                    stage,
                }
            } else {
                ManagementRequest::CancelDevice {
                    expected_revision,
                    stage,
                }
            }
        }
        Command::Remove {
            expected_revision,
            profile,
            legacy,
        } => {
            if legacy {
                ManagementRequest::Remove {
                    expected_revision,
                    profile,
                }
            } else {
                ManagementRequest::RemoveDevice {
                    expected_revision,
                    profile,
                }
            }
        }
        Command::Recover {
            expected_revision,
            stage,
            legacy,
        } => {
            if legacy {
                ManagementRequest::Recover {
                    expected_revision,
                    stage,
                }
            } else {
                ManagementRequest::RecoverDevice {
                    expected_revision,
                    stage,
                }
            }
        }
    })
}
pub(super) fn run(cx: &mut Context, selection: Selection, action: Command) -> u8 {
    if let Command::Import {
        expected_revision,
        profile,
        directory,
        legacy,
    } = action
    {
        let request = airk_core::native_offline_import::NativeOfflineImportRequest {
            expected_revision,
            profile,
            directory,
        };
        if let Err(error) = request.validate() {
            return usage(cx, &format!("invalid local import request: {error:#}"));
        }
        let control = match control(cx, &selection) {
            Ok(control) => control,
            Err(code) => return code,
        };
        return match crate::service::request(
            &control,
            if legacy {
                ControlMethod::ArbiterImport {
                    request: Box::new(request),
                }
            } else {
                ControlMethod::ArbiterImportDevice {
                    request: Box::new(request),
                }
            },
            VERSION,
        ) {
            Ok(ControlResponse::ArbiterImport { observation }) if legacy => {
                json_line(cx, &observation)
            }
            Ok(ControlResponse::ArbiterDevice { observation }) if !legacy => {
                json_line(cx, &observation)
            }
            Ok(other) => fail(cx, &crate::service::refusal(&other)),
            Err(line) => fail(cx, &line),
        };
    }
    let request = match request(action) {
        Ok(request) => request,
        Err(message) => return usage(cx, &message),
    };
    let control = match control(cx, &selection) {
        Ok(control) => control,
        Err(code) => return code,
    };
    let device_response = matches!(
        &request,
        ManagementRequest::StatusDevice
            | ManagementRequest::InitializeDevice { .. }
            | ManagementRequest::MigrateDevice { .. }
            | ManagementRequest::ConfigureDevice { .. }
            | ManagementRequest::ProvisionDevice { .. }
            | ManagementRequest::CancelDevice { .. }
            | ManagementRequest::RemoveDevice { .. }
            | ManagementRequest::RecoverDevice { .. }
    );
    match crate::service::request(&control, ControlMethod::Arbiter { request }, VERSION) {
        Ok(ControlResponse::ArbiterDevice { observation }) if device_response => {
            json_line(cx, &observation)
        }
        Ok(ControlResponse::Arbiter { observation }) if !device_response => {
            json_line(cx, &observation)
        }
        Ok(other) => fail(cx, &crate::service::refusal(&other)),
        Err(line) => fail(cx, &line),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_retains_original_native_directory_and_cannot_become_public_management_json() {
        let directory = if cfg!(windows) {
            r"\\?\D:\AIRK Models\verified"
        } else {
            "/opt/airk models/verified"
        };
        let digest = "ab".repeat(32);
        let cli = Cli::try_parse_from([
            "airkctl",
            "arbiter",
            "import",
            "--expected-revision",
            "7",
            "--profile",
            &digest,
            "--directory",
            directory,
        ])
        .unwrap();
        let super::super::Command::Arbiter { action } = cli.command else {
            panic!("wrong command")
        };
        let Command::Import {
            legacy,
            expected_revision,
            profile,
            directory: actual,
        } = action
        else {
            panic!("wrong native action")
        };
        let native = airk_core::native_offline_import::NativeOfflineImportRequest {
            expected_revision,
            profile,
            directory: actual,
        };
        native.validate().unwrap();
        assert!(!legacy);
        assert_eq!(native.directory, directory);
        assert_eq!(native.expected_revision, 7);
        assert_eq!(native.profile.to_string(), digest);
        assert!(
            request(Command::Import {
                legacy: false,
                expected_revision,
                profile,
                directory: directory.into()
            })
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "airkctl",
                "arbiter",
                "import",
                "--expected-revision",
                "7",
                "--profile",
                &digest
            ])
            .is_err()
        );
    }
    #[test]
    fn management_commands_preserve_native_revision_and_explicit_profile() {
        let digest = "11".repeat(32);
        for command in ["provision", "remove"] {
            let cli = Cli::try_parse_from([
                "airkctl",
                "arbiter",
                command,
                "--expected-revision",
                "7",
                "--profile",
                &digest,
            ])
            .unwrap();
            let super::super::Command::Arbiter { action } = cli.command else {
                panic!("wrong command")
            };
            let value = serde_json::to_value(request(action).unwrap()).unwrap();
            assert_eq!(value["operation"], format!("{command}_device"));
            assert_eq!(value["expected_revision"], 7);
            assert_eq!(value["profile"], digest);
        }
        assert!(
            Cli::try_parse_from(["airkctl", "arbiter", "provision", "--profile", &digest]).is_err()
        );
        let cli = Cli::try_parse_from(["airkctl", "arbiter", "status"]).unwrap();
        let super::super::Command::Arbiter { action } = cli.command else {
            panic!("wrong command")
        };
        assert_eq!(
            serde_json::to_value(request(action).unwrap()).unwrap(),
            serde_json::json!({"operation":"status_device"})
        );
    }
    #[test]
    fn configure_file_cannot_dispatch_a_different_management_operation() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), br#"{"operation":"status"}"#).unwrap();
        assert!(
            request(Command::Configure {
                legacy: false,
                file: file.path().into()
            })
            .is_err()
        );
    }
}

#[cfg(test)]
mod stage_tests {
    use super::*;
    #[test]
    fn cancel_and_recover_require_an_explicit_native_stage() {
        for command in ["cancel", "recover"] {
            assert!(
                Cli::try_parse_from(["airkctl", "arbiter", command, "--expected-revision", "7"])
                    .is_err()
            );
            for stage in ["focus", "context"] {
                let cli = Cli::try_parse_from([
                    "airkctl",
                    "arbiter",
                    command,
                    "--expected-revision",
                    "7",
                    "--stage",
                    stage,
                ])
                .unwrap();
                let super::super::Command::Arbiter { action } = cli.command else {
                    panic!("wrong command")
                };
                let value = serde_json::to_value(request(action).unwrap()).unwrap();
                assert_eq!(value["operation"], format!("{command}_device"));
                assert_eq!(value["stage"], stage);
                assert_eq!(value["expected_revision"], 7);
            }
            assert!(
                Cli::try_parse_from([
                    "airkctl",
                    "arbiter",
                    command,
                    "--expected-revision",
                    "7",
                    "--stage",
                    "all"
                ])
                .is_err()
            );
        }
    }
    #[test]
    fn initialize_retains_the_exact_reviewed_legacy_digest_or_explicit_absence() {
        let digest = "ab".repeat(32);
        let cli = Cli::try_parse_from([
            "airkctl",
            "arbiter",
            "initialize",
            "--expected-legacy-configuration",
            &digest,
        ])
        .unwrap();
        let super::super::Command::Arbiter { action } = cli.command else {
            panic!("wrong command")
        };
        assert_eq!(
            serde_json::to_value(request(action).unwrap()).unwrap(),
            serde_json::json!({"operation":"initialize_device","expected_legacy_configuration":digest})
        );
        assert_eq!(
            serde_json::to_value(
                request(Command::Initialize {
                    legacy: false,
                    expected_legacy_configuration: None
                })
                .unwrap()
            )
            .unwrap(),
            serde_json::json!({"operation":"initialize_device","expected_legacy_configuration":null})
        );
    }
}

#[cfg(test)]
mod device_tests {
    use super::*;
    #[test]
    fn current_and_retained_cpu_status_have_distinct_literal_operations() {
        assert_eq!(
            serde_json::to_value(request(Command::Status).unwrap()).unwrap(),
            serde_json::json!({"operation":"status_device"})
        );
        assert_eq!(
            serde_json::to_value(request(Command::StatusLegacy).unwrap()).unwrap(),
            serde_json::json!({"operation":"status"})
        );
    }
    #[test]
    fn device_migration_keeps_exact_revision_and_original_digest() {
        let digest = Sha256Digest::from_bytes([17; 32]);
        assert_eq!(
            serde_json::to_value(
                request(Command::Migrate {
                    expected_revision: 7,
                    expected_configuration: digest
                })
                .unwrap()
            )
            .unwrap(),
            serde_json::json!({"operation":"migrate_device","expected_revision":7,"expected_configuration":"1111111111111111111111111111111111111111111111111111111111111111"})
        );
    }
    #[test]
    fn device_configure_keeps_native_precision_policy_and_entire_enrollment() {
        let literal = br#"{"operation":"configure_device","stage":"context","expected_revision":7,"configuration":{"enabled":false,"selected_model":"arbiter","device_policy":"auto","gpu_precision":"fp32","indexes":[]}}"#;
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), literal).unwrap();
        let parsed = request(Command::Configure {
            legacy: false,
            file: file.path().into(),
        })
        .unwrap();
        assert_eq!(
            serde_json::to_value(parsed).unwrap(),
            serde_json::from_slice::<serde_json::Value>(literal).unwrap()
        );
        for bytes in [
            br#"{"operation":"configure_device","stage":"context","expected_revision":7,"configuration":{"enabled":false,"selected_model":"focus","device_policy":"auto","gpu_precision":"fp32","indexes":[]}}"#.as_slice(),
            br#"{"operation":"configure_device","stage":"context","expected_revision":7,"configuration":{"enabled":false,"selected_model":"arbiter","device_policy":"auto","gpu_precision":"unknown","indexes":[]}}"#.as_slice(),
            br#"{"operation":"configure","stage":"context","expected_revision":7,"enabled":false,"selected_profile":null,"indexes":[],"device_policy":"auto"}"#.as_slice(),
        ] {std::fs::write(file.path(),bytes).unwrap();assert!(request(Command::Configure { legacy:false, file:file.path().into() }).is_err());}
    }
    #[test]
    fn retained_cpu_configure_is_never_reinterpreted_as_device_selection() {
        let literal=br#"{"operation":"configure","stage":"focus","expected_revision":7,"enabled":false,"selected_profile":null,"indexes":[]}"#;
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), literal).unwrap();
        assert_eq!(
            serde_json::to_value(
                request(Command::Configure {
                    legacy: true,
                    file: file.path().into()
                })
                .unwrap()
            )
            .unwrap(),
            serde_json::from_slice::<serde_json::Value>(literal).unwrap()
        );
    }
}

#[cfg(test)]
mod legacy_device_management_tests {
    use super::*;
    #[test]
    fn lifecycle_legacy_switch_preserves_exact_cpu_native_operations() {
        use clap::Parser;
        for (command, extra) in [
            (
                "provision",
                vec![
                    "--profile",
                    "1111111111111111111111111111111111111111111111111111111111111111",
                ],
            ),
            (
                "remove",
                vec![
                    "--profile",
                    "1111111111111111111111111111111111111111111111111111111111111111",
                ],
            ),
            ("cancel", vec!["--stage", "focus"]),
            ("recover", vec!["--stage", "context"]),
        ] {
            let mut args = vec![
                "airkctl",
                "arbiter",
                command,
                "--expected-revision",
                "7",
                "--legacy",
            ];
            args.extend(extra);
            let cli = Cli::try_parse_from(args).unwrap();
            let super::super::Command::Arbiter { action } = cli.command else {
                panic!("wrong command")
            };
            let value = serde_json::to_value(request(action).unwrap()).unwrap();
            assert_eq!(value["operation"], command);
            assert_eq!(value["expected_revision"], 7);
        }
        let cpu = serde_json::to_value(
            request(Command::Initialize {
                legacy: true,
                expected_legacy_configuration: None,
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            cpu,
            serde_json::json!({"operation":"initialize","expected_legacy_configuration":null})
        );
    }
    #[test]
    fn current_and_legacy_configure_modes_cannot_silently_cross_versions() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(),br#"{"operation":"configure","stage":"focus","expected_revision":7,"enabled":false,"selected_profile":null,"indexes":[]}"#).unwrap();
        assert!(
            request(Command::Configure {
                file: file.path().into(),
                legacy: false
            })
            .is_err()
        );
        assert!(
            request(Command::Configure {
                file: file.path().into(),
                legacy: true
            })
            .is_ok()
        );
        std::fs::write(file.path(),br#"{"operation":"configure_device","stage":"focus","expected_revision":7,"configuration":{"enabled":false,"selected_model":null,"device_policy":"auto","gpu_precision":"auto","indexes":[]}}"#).unwrap();
        assert!(
            request(Command::Configure {
                file: file.path().into(),
                legacy: true
            })
            .is_err()
        );
        assert!(
            request(Command::Configure {
                file: file.path().into(),
                legacy: false
            })
            .is_ok()
        );
    }
}
