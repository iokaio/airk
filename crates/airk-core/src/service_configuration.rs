// SPDX-License-Identifier: Apache-2.0
//! Trusted project desired-state work. Payloads contain bounded configuration
//! and exact file selectors; source text and model inputs never enter control IPC.
use crate::{
    extraction::IndexDetail,
    identity::{Incarnation, Sha256Digest},
    usage::{ContentKind, RecordingMode},
    vector_selection::VectorSelectionPolicy,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_CONFIGURATION_BYTES: usize = 16_384;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputFile {
    pub path: String,
    pub sha256: Sha256Digest,
}
impl InputFile {
    pub fn validate(&self) -> Result<()> {
        path(&self.path)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectConfiguration {
    Unregister,
    BundledModel,
    Vectors {
        enabled: bool,
    },
    Detail {
        level: IndexDetail,
    },
    VectorSelection {
        policy: VectorSelectionPolicy,
    },
    AddRoot {
        alias: String,
        path: String,
    },
    RemoveRoot {
        alias: String,
    },
    InstallModel {
        directory: String,
        manifest_sha256: Sha256Digest,
    },
    Usage {
        mode: RecordingMode,
    },
    ClearUsage,
    UsageFeedback {
        request_id: String,
        usefulness: crate::usage::Usefulness,
    },
    ImportUsage {
        input: InputFile,
    },
    ExtractionPolicy {
        input: InputFile,
    },
    ScanLimits {
        input: InputFile,
    },
    QuotaPolicy {
        input: InputFile,
        expected_policy_digest: Sha256Digest,
    },
    Calibrate {
        tokenizer: InputFile,
        baseline: InputFile,
        returned: InputFile,
        generation: u64,
        content_kind: ContentKind,
        record: bool,
    },
}
fn path(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control),
        "invalid configuration input path"
    );
    Ok(())
}
fn alias(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte)),
        "invalid source root alias"
    );
    Ok(())
}
impl ProjectConfiguration {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::ImportUsage { input }
            | Self::ExtractionPolicy { input }
            | Self::ScanLimits { input }
            | Self::QuotaPolicy { input, .. } => input.validate()?,
            Self::UsageFeedback { request_id, .. } => crate::usage::validate_hex(request_id, 16)?,
            Self::VectorSelection { policy } => policy.validate()?,
            Self::AddRoot {
                alias: name,
                path: value,
            } => {
                alias(name)?;
                path(value)?;
            }
            Self::RemoveRoot { alias: name } => alias(name)?,
            Self::InstallModel { directory, .. } => path(directory)?,
            Self::Calibrate {
                tokenizer,
                baseline,
                returned,
                generation,
                ..
            } => {
                tokenizer.validate()?;
                baseline.validate()?;
                returned.validate()?;
                ensure!(
                    *generation > 0,
                    "calibration requires an explicit indexed generation"
                );
            }
            Self::Unregister
            | Self::BundledModel
            | Self::Vectors { .. }
            | Self::Detail { .. }
            | Self::Usage { .. }
            | Self::ClearUsage => {}
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_CONFIGURATION_BYTES,
            "project configuration exceeds job budget"
        );
        Ok(())
    }
    pub fn needs_inference(&self) -> bool {
        matches!(
            self,
            Self::BundledModel | Self::InstallModel { .. } | Self::Calibrate { .. }
        )
    }
    pub fn requests_source_pass(&self) -> bool {
        matches!(
            self,
            Self::BundledModel
                | Self::Vectors { .. }
                | Self::Detail { .. }
                | Self::ExtractionPolicy { .. }
                | Self::ScanLimits { .. }
                | Self::VectorSelection { .. }
                | Self::AddRoot { .. }
                | Self::RemoveRoot { .. }
                | Self::InstallModel { .. }
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationRequest {
    pub incarnation: Incarnation,
    pub settings: Sha256Digest,
    pub configuration: ProjectConfiguration,
}
impl ConfigurationRequest {
    pub fn validate(&self) -> Result<()> {
        self.configuration.validate()?;
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_CONFIGURATION_BYTES,
            "project configuration request exceeds job budget"
        );
        Ok(())
    }
}

/// The accepting native service stamps its own engine identity. A later runtime
/// must fail old queued configuration rather than reinterpret accepted work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationWork {
    pub engine: Sha256Digest,
    pub request: ConfigurationRequest,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_bounds_file_selectors_without_accepting_raw_inputs() -> Result<()> {
        let file = InputFile {
            path: "/explicit/input.json".into(),
            sha256: Sha256Digest::from_bytes([1; 32]),
        };
        let input = ProjectConfiguration::Calibrate {
            tokenizer: file.clone(),
            baseline: file.clone(),
            returned: file,
            generation: 1,
            content_kind: ContentKind::Code,
            record: false,
        };
        input.validate()?;
        assert!(input.needs_inference());
        let mut value = serde_json::to_value(input)?;
        value["baseline_text"] = "Private input must never enter control IPC".into();
        assert!(serde_json::from_value::<ProjectConfiguration>(value).is_err());
        assert!(
            ProjectConfiguration::AddRoot {
                alias: "../escape".into(),
                path: "/source".into()
            }
            .validate()
            .is_err()
        );
        assert!(
            ProjectConfiguration::InstallModel {
                directory: "x".repeat(4097),
                manifest_sha256: Sha256Digest::from_bytes([1; 32])
            }
            .validate()
            .is_err()
        );
        assert!(
            ProjectConfiguration::RemoveRoot {
                alias: "root\0".into()
            }
            .validate()
            .is_err()
        );
        Ok(())
    }
}
