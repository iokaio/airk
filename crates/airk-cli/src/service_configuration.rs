// SPDX-License-Identifier: Apache-2.0
//! Trusted preparation reads explicit bounded input files; control receives only selectors.
use airk_core::{
    extraction::IndexDetail,
    service::{ControlMethod, ControlResponse, ProjectBinding},
    service_configuration::{ConfigurationRequest, InputFile, ProjectConfiguration},
    usage::{ContentKind, RecordingMode},
    vector_selection::VectorSelectionPolicy,
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Draft {
    Unregister,
    PrepareDerivedCleanup,
    DeleteDerived {
        preview: airk_core::identity::Sha256Digest,
    },
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
        path: PathBuf,
    },
    RemoveRoot {
        alias: String,
    },
    InstallModel {
        directory: PathBuf,
    },
    BundledModel,
    Usage {
        mode: RecordingMode,
    },
    ClearUsage,
    UsageFeedback {
        request_id: String,
        usefulness: airk_core::usage::Usefulness,
    },
    ImportUsage {
        path: PathBuf,
        expected_sha256: Option<airk_core::identity::Sha256Digest>,
    },
    ExtractionPolicy {
        path: PathBuf,
    },
    ScanLimits {
        path: PathBuf,
    },
    ResetScanLimits,
    QuotaPolicy {
        path: PathBuf,
    },
    Calibrate {
        tokenizer: PathBuf,
        baseline: PathBuf,
        returned: PathBuf,
        generation: u64,
        content_kind: ContentKind,
        record: bool,
    },
}
fn selected(path: &Path) -> Result<PathBuf> {
    let text = path.to_str().context("configuration input must be UTF-8")?;
    ensure!(
        path.is_absolute() && text.len() <= 4096 && !text.chars().any(char::is_control),
        "configuration input requires a bounded absolute path"
    );
    Ok(path.canonicalize()?)
}
pub fn input(path: &Path, limit: u64) -> Result<InputFile> {
    let path = selected(path)?;
    crate::runtime_install::safe_file(&path)?;
    let mut file = std::fs::File::open(&path)?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() <= limit,
        "configuration input exceeds budget"
    );
    let mut bytes = Vec::new();
    (&mut file).take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit
            && bytes.len() as u64 == before.len()
            && file.metadata()?.len() == before.len(),
        "configuration input changed during preparation"
    );
    let reference = InputFile {
        path: path
            .to_str()
            .context("configuration input must be UTF-8")?
            .into(),
        sha256: airk_core::identity::Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
    };
    reference.validate()?;
    Ok(reference)
}
impl Draft {
    pub fn prepare(self) -> Result<ProjectConfiguration> {
        let configuration = match self {
            Self::Vectors { enabled } => ProjectConfiguration::Vectors { enabled },
            Self::Detail { level } => ProjectConfiguration::Detail { level },
            Self::VectorSelection { policy } => ProjectConfiguration::VectorSelection { policy },
            Self::AddRoot { alias, path } => ProjectConfiguration::AddRoot {
                alias,
                path: selected(&path)?
                    .to_str()
                    .context("configuration root must be UTF-8")?
                    .into(),
            },
            Self::RemoveRoot { alias } => ProjectConfiguration::RemoveRoot { alias },
            Self::InstallModel { directory } => {
                let directory = selected(&directory)?;
                ensure!(directory.is_dir(), "selected model must be a directory");
                let manifest = input(&directory.join("model.json"), 16_384)?;
                ProjectConfiguration::InstallModel {
                    directory: directory
                        .to_str()
                        .context("model directory must be UTF-8")?
                        .into(),
                    manifest_sha256: manifest.sha256,
                }
            }
            Self::Unregister => ProjectConfiguration::Unregister,
            Self::ResetScanLimits => ProjectConfiguration::ResetScanLimits,
            Self::PrepareDerivedCleanup => ProjectConfiguration::PrepareDerivedCleanup,
            Self::DeleteDerived { preview } => ProjectConfiguration::DeleteDerived { preview },
            Self::BundledModel => ProjectConfiguration::BundledModel,
            Self::Usage { mode } => ProjectConfiguration::Usage { mode },
            Self::ClearUsage => ProjectConfiguration::ClearUsage,
            Self::UsageFeedback {
                request_id,
                usefulness,
            } => ProjectConfiguration::UsageFeedback {
                request_id,
                usefulness,
            },
            Self::ImportUsage {
                path,
                expected_sha256,
            } => {
                let input = input(&path, 65_536)?;
                ensure!(
                    expected_sha256.is_none_or(|expected| expected == input.sha256),
                    "measurement changed since preview"
                );
                ProjectConfiguration::ImportUsage { input }
            }
            Self::ExtractionPolicy { path } => ProjectConfiguration::ExtractionPolicy {
                input: input(&path, 131_072)?,
            },
            Self::ScanLimits { path } => ProjectConfiguration::ScanLimits {
                input: input(&path, 65_536)?,
            },
            Self::QuotaPolicy { .. } => {
                anyhow::bail!("quota preparation requires native project inspection")
            }
            Self::Calibrate {
                tokenizer,
                baseline,
                returned,
                generation,
                content_kind,
                record,
            } => ProjectConfiguration::Calibrate {
                tokenizer: input(&tokenizer, 1_048_576)?,
                baseline: input(&baseline, 262_144)?,
                returned: input(&returned, 262_144)?,
                generation,
                content_kind,
                record,
            },
        };
        configuration.validate()?;
        Ok(configuration)
    }
}
pub async fn prepare(
    runtime: &Path,
    project: ProjectBinding,
    draft: Draft,
) -> Result<ConfigurationRequest> {
    if matches!(
        &draft,
        Draft::PrepareDerivedCleanup | Draft::DeleteDerived { .. }
    ) {
        let ControlResponse::RequestEpoch { namespace, .. } =
            crate::service_runtime::request(runtime, ControlMethod::RequestEpoch).await?
        else {
            anyhow::bail!("native cleanup authority is unavailable")
        };
        let request = ConfigurationRequest {
            prepare: false,
            incarnation: namespace,
            settings: project,
            configuration: draft.prepare()?,
        };
        request.validate()?;
        return Ok(request);
    }
    let ControlResponse::Inspection {
        observation: Some(observation),
        ..
    } = crate::service_runtime::request(runtime, ControlMethod::Inspection { project }).await?
    else {
        anyhow::bail!("inspect the selected project before configuring it")
    };
    ensure!(
        observation.snapshot.project == project,
        "configuration inspection belongs to another project"
    );
    let configuration = match draft {
        Draft::QuotaPolicy { path } => ProjectConfiguration::QuotaPolicy {
            input: input(&path, 16_384)?,
            expected_policy_digest: observation
                .snapshot
                .quota_policy_digest
                .context("inspect the current storage quota policy before changing it")?,
        },
        draft => draft.prepare()?,
    };
    let request = ConfigurationRequest {
        prepare: false,
        incarnation: observation.snapshot.incarnation,
        settings: observation
            .snapshot
            .settings_digest
            .context("project settings observation unavailable; inspect again")?,
        configuration,
    };
    request.validate()?;
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preparation_pins_bounded_files_without_returning_their_contents() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let sample = directory.path().join("sample.json");
        let text = b"{\"private\":\"Original silver sample, never forward this input\"}";
        std::fs::write(&sample, text)?;
        let configuration = Draft::Calibrate {
            tokenizer: sample.clone(),
            baseline: sample.clone(),
            returned: sample.clone(),
            generation: 1,
            content_kind: ContentKind::PlainText,
            record: false,
        }
        .prepare()?;
        let bytes = serde_json::to_string(&configuration)?;
        assert!(!bytes.contains("Original silver sample"));
        assert!(bytes.contains(&format!("{:x}", Sha256::digest(text))));
        assert_eq!(std::fs::read(&sample)?, text);
        std::fs::write(&sample, vec![b'x'; 262145])?;
        assert!(input(&sample, 262144).is_err());
        assert!(input(Path::new("relative.json"), 262144).is_err());
        Ok(())
    }
    #[test]
    fn usage_import_preparation_rejects_bytes_changed_after_the_reviewed_preview() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let sample = directory.path().join("measurement.json");
        std::fs::write(&sample, b"{\"original\":true}")?;
        let expected = input(&sample, 65_536)?.sha256;
        let prepared = Draft::ImportUsage {
            path: sample.clone(),
            expected_sha256: Some(expected),
        }
        .prepare()?;
        assert!(
            matches!(prepared, ProjectConfiguration::ImportUsage { input } if input.sha256 == expected)
        );
        std::fs::write(&sample, b"{\"changed\":true}")?;
        assert!(
            Draft::ImportUsage {
                path: sample.clone(),
                expected_sha256: Some(expected)
            }
            .prepare()
            .is_err()
        );
        assert_eq!(std::fs::read(&sample)?, b"{\"changed\":true}");
        Ok(())
    }
}
