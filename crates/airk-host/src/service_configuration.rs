// SPDX-License-Identifier: Apache-2.0
//! Trusted preparation reads explicit bounded input files; control receives only selectors.
#[cfg(test)]
use airk_core::usage::ContentKind;
use airk_core::{
    identity::Incarnation,
    service::{ControlResponse, ProjectBinding},
    service_configuration::{ConfigurationRequest, InputFile, ProjectConfiguration},
};
use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};

/// The draft `configure` reads and the `usage` commands build; owned by
/// `airk_core` so every client sends the same JSON (split step 3c.5a).
pub use airk_core::service_configuration::ConfigurationDraft as Draft;

/// The service calls preparation makes, injected so this module reaches no
/// control client (split step 3c.5b-1). The `client_requests` module
/// implements it for a control root (`Path`) with the CLI's own requests, so
/// every caller passes the control root as before.
pub trait ConfigurationReads {
    /// The request epoch's namespace and epoch (derived-cleanup drafts).
    fn epoch(&self) -> impl std::future::Future<Output = Result<(Incarnation, u64)>> + Send;
    /// The project's current `Inspection` response.
    fn inspection(
        &self,
        project: ProjectBinding,
    ) -> impl std::future::Future<Output = Result<ControlResponse>> + Send;
    /// One administrative native read for the project.
    fn administrative_read(
        &self,
        project: ProjectBinding,
        read: airk_core::native_reads::NativeRead,
    ) -> impl std::future::Future<Output = Result<serde_json::Value>> + Send;
}

fn selected(path: &Path) -> Result<PathBuf> {
    let text = path.to_str().context("configuration input must be UTF-8")?;
    ensure!(
        path.is_absolute() && text.len() <= 4096 && !text.chars().any(char::is_control),
        "configuration input requires a bounded absolute path"
    );
    Ok(path.canonicalize()?)
}
#[cfg(any(test, feature = "test-support"))]
pub fn input(path: &Path, limit: u64) -> Result<InputFile> {
    airk_kernel::service_configuration::prepare_input(path, limit)
}

/// Source-bearing preparation runs inside the same native owner as other
/// administrative reads. A local hash before admission could outlive a trim
/// fence and capture a permanently excluded source.
async fn native_input<R: ConfigurationReads + ?Sized>(
    runtime: &R,
    project: ProjectBinding,
    path: &Path,
    limit: u64,
) -> Result<InputFile> {
    let path = path.to_str().context("configuration input must be UTF-8")?;
    ensure!(
        Path::new(path).is_absolute()
            && path.len() <= 4096
            && !path.chars().any(char::is_control)
            && (1..=1_048_576).contains(&limit),
        "invalid bounded native configuration input"
    );
    let result = runtime
        .administrative_read(
            project,
            airk_core::native_reads::NativeRead::PrepareInputFile {
                path: path.into(),
                max_source_bytes: limit,
            },
        )
        .await?;
    let input: InputFile = serde_json::from_value(result)
        .context("native input preparation omitted its complete pin")?;
    input.validate()?;
    Ok(input)
}

fn usage_import(
    input: InputFile,
    expected: Option<airk_core::identity::Sha256Digest>,
) -> Result<ProjectConfiguration> {
    input.validate()?;
    ensure!(
        expected.is_none_or(|expected| expected == input.sha256),
        "measurement changed since preview"
    );
    Ok(ProjectConfiguration::ImportUsage { input })
}
/// The draft's preparation into a project configuration (CLI-side checks).
pub trait PrepareDraft {
    fn prepare(self) -> Result<ProjectConfiguration>;
}
impl PrepareDraft for Draft {
    fn prepare(self) -> Result<ProjectConfiguration> {
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
            Self::RetainedProfile { .. }
            | Self::InstallModel { .. }
            | Self::ImportUsage { .. }
            | Self::ExtractionPolicy { .. }
            | Self::ScanLimits { .. }
            | Self::QuotaPolicy { .. }
            | Self::Calibrate { .. } => {
                anyhow::bail!("source-bearing preparation requires its native administrative owner")
            }
        };
        configuration.validate()?;
        Ok(configuration)
    }
}
pub async fn prepare<R: ConfigurationReads + ?Sized>(
    runtime: &R,
    project: ProjectBinding,
    draft: Draft,
) -> Result<ConfigurationRequest> {
    if matches!(
        &draft,
        Draft::PrepareDerivedCleanup | Draft::DeleteDerived { .. }
    ) {
        let (namespace, _) = runtime.epoch().await?;
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
    } = runtime.inspection(project).await?
    else {
        anyhow::bail!("inspect the selected project before configuring it")
    };
    ensure!(
        observation.snapshot.project == project,
        "configuration inspection belongs to another project"
    );
    let configuration = match draft {
        Draft::RetainedProfile { target } => {
            let native = runtime
                .administrative_read(
                    project,
                    airk_core::native_reads::NativeRead::RetentionMigrationPreview { target },
                )
                .await?;
            let review: airk_core::retention::RetentionMigrationReview =
                serde_json::from_value(native)?;
            review.validate()?;
            ensure!(review.target == target, "crossed retained migration target");
            ProjectConfiguration::RetainedProfile {
                review: Box::new(review),
            }
        }
        Draft::InstallModel { directory } => {
            // Preserve the user's original path until the native policy guard
            // has checked both its spelling and resolved source location.
            let manifest =
                native_input(runtime, project, &directory.join("model.json"), 16_384).await?;
            let directory = Path::new(&manifest.path)
                .parent()
                .context("native model manifest has no directory")?
                .to_str()
                .context("model directory must be UTF-8")?
                .to_owned();
            ProjectConfiguration::InstallModel {
                directory,
                manifest_sha256: manifest.sha256,
            }
        }
        Draft::ImportUsage {
            path,
            expected_sha256,
        } => {
            let input = native_input(runtime, project, &path, 65_536).await?;
            usage_import(input, expected_sha256)?
        }
        Draft::ExtractionPolicy { path } => ProjectConfiguration::ExtractionPolicy {
            input: native_input(runtime, project, &path, 131_072).await?,
        },
        Draft::ScanLimits { path } => ProjectConfiguration::ScanLimits {
            input: native_input(runtime, project, &path, 65_536).await?,
        },
        Draft::QuotaPolicy { path } => ProjectConfiguration::QuotaPolicy {
            input: native_input(runtime, project, &path, 16_384).await?,
            expected_policy_digest: observation
                .snapshot
                .quota_policy_digest
                .context("inspect the current storage quota policy before changing it")?,
        },
        Draft::Calibrate {
            tokenizer,
            baseline,
            returned,
            generation,
            content_kind,
            record,
        } => ProjectConfiguration::Calibrate {
            tokenizer: native_input(runtime, project, &tokenizer, 1_048_576).await?,
            baseline: native_input(runtime, project, &baseline, 262_144).await?,
            returned: native_input(runtime, project, &returned, 262_144).await?,
            generation,
            content_kind,
            record,
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
    fn source_bearing_drafts_cannot_bypass_the_native_preparation_owner() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let sample = directory.path().join("never-open.json");
        for draft in [
            Draft::Calibrate {
                tokenizer: sample.clone(),
                baseline: sample.clone(),
                returned: sample.clone(),
                generation: 1,
                content_kind: ContentKind::PlainText,
                record: false,
            },
            Draft::ImportUsage {
                path: sample.clone(),
                expected_sha256: None,
            },
            Draft::ExtractionPolicy {
                path: sample.clone(),
            },
            Draft::ScanLimits {
                path: sample.clone(),
            },
            Draft::QuotaPolicy {
                path: sample.clone(),
            },
            Draft::InstallModel {
                directory: directory.path().into(),
            },
        ] {
            assert!(
                draft
                    .prepare()
                    .unwrap_err()
                    .to_string()
                    .contains("native administrative owner")
            );
        }
        assert!(!sample.exists());
        assert!(std::fs::read_dir(directory.path())?.next().is_none());
        Ok(())
    }
    #[test]
    fn usage_import_preparation_rejects_bytes_changed_after_the_reviewed_preview() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let sample = directory.path().join("measurement.json");
        std::fs::write(&sample, b"{\"original\":true}")?;
        let expected = input(&sample, 65_536)?.sha256;
        let prepared = usage_import(input(&sample, 65_536)?, Some(expected))?;
        assert!(
            matches!(prepared, ProjectConfiguration::ImportUsage { input } if input.sha256 == expected)
        );
        std::fs::write(&sample, b"{\"changed\":true}")?;
        assert!(usage_import(input(&sample, 65_536)?, Some(expected)).is_err());
        assert_eq!(std::fs::read(&sample)?, b"{\"changed\":true}");
        Ok(())
    }
}
