// SPDX-License-Identifier: Apache-2.0
//! Project configuration executes only inside the fenced native worker.
use crate::Kernel;
use airk_core::{
    service::ProjectBinding,
    service_configuration::{ConfigurationRequest, ProjectConfiguration},
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::path::Path;
pub fn verified_input(
    input: &airk_core::service_configuration::InputFile,
    limit: usize,
) -> Result<Vec<u8>> {
    input.validate()?;
    use sha2::{Digest, Sha256};
    let bytes = crate::usage::read_usage_input(input_path(&input.path)?, limit)?;
    ensure!(
        airk_core::identity::Sha256Digest::from_bytes(Sha256::digest(&bytes).into())
            == input.sha256,
        "configuration input changed since preparation"
    );
    Ok(bytes)
}

#[derive(Debug)]
pub struct ConfigurationChanged;
impl std::fmt::Display for ConfigurationChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "configuration changed; inspect current settings before submitting a new request",
        )
    }
}
impl std::error::Error for ConfigurationChanged {}
fn input_path(value: &str) -> Result<&Path> {
    let value = Path::new(value);
    ensure!(
        value.is_absolute(),
        "configuration inputs require explicit absolute paths"
    );
    Ok(value)
}
impl Kernel {
    pub fn validate_service_configuration(
        &self,
        project: ProjectBinding,
        request: &ConfigurationRequest,
    ) -> Result<()> {
        request.validate()?;
        let observed = self.inspect_service(project)?;
        ensure!(
            observed.incarnation == request.incarnation
                && observed.settings_digest == Some(request.settings),
            ConfigurationChanged
        );
        Ok(())
    }
    /// Reserve proposed source boundaries in the registry before writing roots.
    pub fn service_configuration_roots(
        &self,
        request: &ConfigurationRequest,
    ) -> Result<Option<Vec<String>>> {
        let mut roots = match &request.configuration {
            ProjectConfiguration::AddRoot { .. } | ProjectConfiguration::RemoveRoot { .. } => {
                airk_store::roots::RootConfig::load(self.store.directory(), &self.root_id())?
            }
            _ => return Ok(None),
        };
        match &request.configuration {
            ProjectConfiguration::AddRoot { alias, path } => {
                roots.add(alias, input_path(path)?, &self.root, &self.data_root)?
            }
            ProjectConfiguration::RemoveRoot { alias } => roots.remove(alias)?,
            _ => unreachable!(),
        }
        Ok(Some(roots.roots.into_values().collect()))
    }
    pub fn configure_service_project(
        &self,
        project: ProjectBinding,
        request: &ConfigurationRequest,
    ) -> Result<Value> {
        if let Some(selected) = self.active_service_view()? {
            return selected.configure_service_project(project, request);
        }

        self.validate_service_configuration(project, request)?;
        crate::service_execution::checkpoint()?;
        let result = match &request.configuration {
            // Native registry completion performs the actual unregister and
            // writes its final receipt atomically. This only validates the guard.
            ProjectConfiguration::Unregister => json!({"unregistration_requested":true}),
            ProjectConfiguration::PrepareDerivedCleanup
            | ProjectConfiguration::DeleteDerived { .. } => {
                anyhow::bail!("derived cleanup requires the service-owned inactive-project worker")
            }
            ProjectConfiguration::Vectors { enabled } => {
                serde_json::to_value(self.configure_vectors(*enabled)?)?
            }
            ProjectConfiguration::Detail { level } => {
                self.configure_index_detail(*level)?;
                json!({"configured_level":level,"source_pass_requested":true})
            }
            ProjectConfiguration::VectorSelection { policy } => {
                self.configure_vector_selection(policy.clone())?;
                json!({"selection_digest":policy.digest()?,"source_pass_requested":true})
            }
            ProjectConfiguration::AddRoot { alias, path } => {
                self.add_root(alias, input_path(path)?)?;
                json!({"updated":true,"source_pass_requested":true})
            }
            ProjectConfiguration::RemoveRoot { alias } => {
                self.remove_root(alias)?;
                json!({"updated":true,"source_pass_requested":true})
            }
            ProjectConfiguration::InstallModel {
                directory,
                manifest_sha256,
            } => serde_json::to_value(
                self.install_service_vector_model(input_path(directory)?, *manifest_sha256)?,
            )?,
            ProjectConfiguration::BundledModel => {
                serde_json::to_value(self.select_service_bundled_model()?)?
            }
            ProjectConfiguration::Usage { mode } => {
                serde_json::to_value(self.configure_usage(*mode)?)?
            }
            ProjectConfiguration::ClearUsage => self.clear_usage()?,
            ProjectConfiguration::UsageFeedback {
                request_id,
                usefulness,
            } => self.usage_feedback(request_id, *usefulness)?,
            ProjectConfiguration::ImportUsage { input } => {
                let measurement = serde_json::from_slice(&verified_input(input, 65_536)?)?;
                self.usage_import(&measurement)?
            }
            ProjectConfiguration::ExtractionPolicy { input } => {
                let policy = serde_json::from_slice(&verified_input(input, 131_072)?)?;
                let configured = self.configure_extraction_policy(policy)?;
                json!({"extraction_policy_digest":configured.digest,"source_pass_requested":true})
            }
            ProjectConfiguration::ScanLimits { input } => {
                let limits = serde_json::from_slice(&verified_input(input, 65_536)?)?;
                serde_json::to_value(self.configure_scan_limits(limits)?)?
            }
            ProjectConfiguration::ResetScanLimits => {
                serde_json::to_value(self.reset_scan_limits()?)?
            }
            ProjectConfiguration::QuotaPolicy {
                input,
                expected_policy_digest,
            } => {
                use sha2::{Digest, Sha256};
                let policy = serde_json::from_slice(&verified_input(input, 16_384)?)?;
                let current = self.quota_policy()?;
                ensure!(
                    airk_core::identity::Sha256Digest::from_bytes(
                        Sha256::digest(serde_json::to_vec(&current)?).into()
                    ) == *expected_policy_digest,
                    ConfigurationChanged
                );
                let status = self.configure_quota_if_current(policy, Some(&current))?;
                json!({"updated":true,"scope":"shared_data_root","collections":status.collections.len(),"root_limit_bytes":status.root_limit_bytes})
            }
            ProjectConfiguration::Calibrate {
                tokenizer,
                baseline,
                returned,
                generation,
                content_kind,
                record,
            } => self.usage_calibrate_verified(
                crate::usage::CalibrationRequest {
                    tokenizer: input_path(&tokenizer.path)?,
                    baseline: input_path(&baseline.path)?,
                    returned: input_path(&returned.path)?,
                    generation: *generation,
                    content_kind: *content_kind,
                    record: *record,
                },
                [tokenizer.sha256, baseline.sha256, returned.sha256],
            )?,
        };
        ensure!(
            serde_json::to_vec(&result)?.len()
                <= airk_core::service_configuration::MAX_CONFIGURATION_BYTES,
            "configuration result exceeds budget"
        );
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    #[test]
    fn guarded_scan_limit_reset_removes_only_the_override_and_invalidates_the_old_guard()
    -> Result<()> {
        let source = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(
            source.path().join("original.txt"),
            b"original scan reset fixture",
        )?;
        let kernel =
            Kernel::open_service_collection(source.path(), Some(data.path().into()), "scan-reset")?;
        kernel.configure_vectors(false)?;
        let automatic = kernel.scan_limits()?;
        let mut explicit = automatic;
        explicit.file_bytes = 1;
        kernel.configure_scan_limits(explicit)?;
        let project =
            crate::service_projects::registration(source.path(), data.path(), "scan-reset")?;
        let snapshot = kernel.inspect_service(project.binding)?;
        let request = ConfigurationRequest {
            incarnation: snapshot.incarnation,
            settings: snapshot.settings_digest.context("settings missing")?,
            configuration: ProjectConfiguration::ResetScanLimits,
        };
        assert!(request.configuration.requests_source_pass());
        kernel.configure_service_project(project.binding, &request)?;
        assert_eq!(
            serde_json::to_value(kernel.scan_limits()?)?,
            serde_json::to_value(automatic)?
        );
        assert!(
            !kernel
                .store
                .directory()
                .join("scan-limits.json")
                .try_exists()?
        );
        assert!(
            kernel
                .configure_service_project(project.binding, &request)
                .unwrap_err()
                .is::<ConfigurationChanged>()
        );
        assert_eq!(
            std::fs::read(source.path().join("original.txt"))?,
            b"original scan reset fixture"
        );
        Ok(())
    }
    #[test]
    fn calibration_verifies_exact_inputs_before_counting_and_returns_no_sample_text() -> Result<()>
    {
        use airk_core::{
            identity::Sha256Digest, model_objects::ModelAssetName, service_configuration::InputFile,
        };
        use sha2::{Digest, Sha256};
        let owner = tempfile::tempdir()?;
        let source = owner.path().join("source");
        let data = owner.path().join("data");
        let runtime = owner.path().join("runtime");
        let model = owner.path().join("model");
        for path in [&source, &data, &runtime, &model] {
            std::fs::create_dir(path)?;
        }
        let vocabulary = b"{\"amber\":[1.0,0.0]}";
        std::fs::write(model.join("vocabulary.json"), vocabulary)?;
        std::fs::write(
            model.join(ModelAssetName::ModelJson.filename()),
            serde_json::to_vec(
                &json!({"schema":1,"name":"original calibration fixture","license":"Apache-2.0", "sha256":format!("{:x}",Sha256::digest(vocabulary)),"dimensions":2}),
            )?,
        )?;
        let plan = airk_retrieval::model::inspect_object_plan(&model)?;
        let kernel = Kernel::open_service_collection(&source, Some(data.clone()), "fixture")?
            .with_service_models(&runtime, plan)?;
        let project = crate::service_projects::registration(&source, &data, "fixture")?;
        let tokenizer = owner.path().join("tokenizer.json");
        let baseline = owner.path().join("baseline.json");
        let returned = owner.path().join("returned.json");
        std::fs::write(
            &tokenizer,
            serde_json::to_vec(
                &json!({"schema":1,"model_revision_sha256":"12".repeat(32),"merges":[]}),
            )?,
        )?;
        let original = b"{\"value\":\"Private amber sample must stay out of results\"}";
        std::fs::write(&baseline, original)?;
        std::fs::write(&returned, b"{\"value\":\"short\"}")?;
        let reference = |path: &std::path::Path| -> Result<InputFile> {
            Ok(InputFile {
                path: path.to_str().unwrap().into(),
                sha256: Sha256Digest::from_bytes(Sha256::digest(std::fs::read(path)?).into()),
            })
        };
        let snapshot = kernel.inspect_service(project.binding)?;
        let request = ConfigurationRequest {
            incarnation: snapshot.incarnation,
            settings: snapshot.settings_digest.unwrap(),
            configuration: ProjectConfiguration::Calibrate {
                tokenizer: reference(&tokenizer)?,
                baseline: reference(&baseline)?,
                returned: reference(&returned)?,
                generation: 1,
                content_kind: airk_core::usage::ContentKind::PlainText,
                record: false,
            },
        };
        let result = kernel.configure_service_project(project.binding, &request)?;
        assert!(result.get("summary").is_some());
        assert!(!serde_json::to_string(&result)?.contains("Private amber sample"));
        assert!(kernel.usage_report(1)?.supplemental.is_empty());
        std::fs::write(&baseline, b"{\"value\":\"Changed after preparation\"}")?;
        assert!(
            kernel
                .configure_service_project(project.binding, &request)
                .is_err()
        );
        std::fs::write(&baseline, original)?;
        std::fs::write(
            &tokenizer,
            b"{\"schema\":1,\"model_revision_sha256\":\"changed\",\"merges\":[]}",
        )?;
        assert!(
            kernel
                .configure_service_project(project.binding, &request)
                .is_err()
        );
        assert_eq!(std::fs::read(&baseline)?, original);
        assert!(kernel.usage_report(1)?.supplemental.is_empty());
        Ok(())
    }
    #[test]
    fn settings_guard_refuses_stale_configuration_and_preserves_original_sources() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let source = owner.path().join("source");
        let data = owner.path().join("data");
        std::fs::create_dir(&source)?;
        std::fs::create_dir(&data)?;
        let original = b"Original amber service configuration fixture.\n";
        std::fs::write(source.join("memo.txt"), original)?;
        let project =
            crate::service_projects::registration(&source, &data, "configuration-fixture")?;
        let kernel = Kernel::open_service_collection(&source, Some(data), "configuration-fixture")?;
        let snapshot = kernel.inspect_service(project.binding)?;
        let request = ConfigurationRequest {
            incarnation: snapshot.incarnation,
            settings: snapshot.settings_digest.unwrap(),
            configuration: ProjectConfiguration::Vectors { enabled: false },
        };
        kernel.configure_service_project(project.binding, &request)?;
        assert!(!kernel.vectors_enabled()?);
        assert!(
            kernel
                .configure_service_project(project.binding, &request)
                .unwrap_err()
                .is::<ConfigurationChanged>()
        );
        assert_eq!(std::fs::read(source.join("memo.txt"))?, original);
        Ok(())
    }
}
