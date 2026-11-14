// SPDX-License-Identifier: Apache-2.0
//! Explicit native optional-model management; no implicit enrollment or downloads.
use super::*;
use airk_core::{arbiter::ManagementRequest, identity::Sha256Digest};
#[derive(Subcommand)]
pub(super) enum Command {
    /// Inspect optional model readiness, selection and exact per-index enrollment.
    Status,
    /// Apply one complete native Configure request. Enrollments are explicit immutable index IDs.
    Configure {
        #[arg(long)]
        file: PathBuf,
    },
    /// Explicitly acquire the compiled qualified profile asset closure.
    Provision {
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        profile: Sha256Digest,
    },
    /// Cancel this native optional operation; running native calls drain before removal.
    Cancel {
        #[arg(long)]
        expected_revision: u64,
    },
    /// Explicitly remove one optional profile after actual native pins drain.
    Remove {
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        profile: Sha256Digest,
    },
    /// Recover a retained optional model operation under the current configuration revision.
    Recover {
        #[arg(long)]
        expected_revision: u64,
    },
}
fn request(action: Command) -> Result<ManagementRequest, String> {
    Ok(match action {
        Command::Status => ManagementRequest::Status,
        Command::Configure { file } => {
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
            if !matches!(request, ManagementRequest::Configure { .. }) {
                return Err("arbiter configure --file requires operation configure".into());
            }
            request
        }
        Command::Provision {
            expected_revision,
            profile,
        } => ManagementRequest::Provision {
            expected_revision,
            profile,
        },
        Command::Cancel { expected_revision } => ManagementRequest::Cancel { expected_revision },
        Command::Remove {
            expected_revision,
            profile,
        } => ManagementRequest::Remove {
            expected_revision,
            profile,
        },
        Command::Recover { expected_revision } => ManagementRequest::Recover { expected_revision },
    })
}
pub(super) fn run(cx: &mut Context, selection: Selection, action: Command) -> u8 {
    let request = match request(action) {
        Ok(request) => request,
        Err(message) => return usage(cx, &message),
    };
    let control = match control(cx, &selection) {
        Ok(control) => control,
        Err(code) => return code,
    };
    match crate::service::request(&control, ControlMethod::Arbiter { request }, VERSION) {
        Ok(ControlResponse::Arbiter { observation }) => json_line(cx, &observation),
        Ok(other) => fail(cx, &crate::service::refusal(&other)),
        Err(line) => fail(cx, &line),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
            assert_eq!(value["operation"], command);
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
            serde_json::json!({"operation":"status"})
        );
    }
    #[test]
    fn configure_file_cannot_dispatch_a_different_management_operation() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), br#"{"operation":"status"}"#).unwrap();
        assert!(
            request(Command::Configure {
                file: file.path().into()
            })
            .is_err()
        );
    }
}
