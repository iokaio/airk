// SPDX-License-Identifier: Apache-2.0
//! Collection policy selects opaque references; the native adapter binds their service owner.
use crate::Kernel;
use airk_core::{embedding::EmbeddingDescriptor, model_objects::ModelObjectSet};
use airk_retrieval::model::EmbeddingModel;
use airk_store::{FileStore, service::model_files::ServiceModelFiles};
use anyhow::{Context, Result, ensure};
use std::path::Path;

pub(crate) fn open_model(store: &FileStore, objects: &ModelObjectSet) -> Result<EmbeddingModel> {
    if objects.shared.is_some() {
        EmbeddingModel::open_shared(&store.installed_model(objects)?, objects)
    } else {
        EmbeddingModel::open_objects(store.directory(), objects)
    }
}
pub(crate) fn descriptor(
    store: &FileStore,
    objects: &ModelObjectSet,
) -> Result<EmbeddingDescriptor> {
    if objects.shared.is_some() {
        airk_retrieval::model::descriptor_from_asset_metadata(
            &store.installed_model(objects)?,
            objects,
        )
    } else {
        airk_retrieval::model::descriptor_from_objects_metadata(store.directory(), objects)
    }
}
impl Kernel {
    pub fn with_service_models(mut self, runtime: &Path, default: ModelObjectSet) -> Result<Self> {
        ensure!(
            self.service_collection.is_some(),
            "shared model binding requires a service collection"
        );
        let runtime = runtime.canonicalize()?;
        let data = self.data_root.canonicalize()?;
        ensure!(
            !data.starts_with(&runtime)
                && !runtime.starts_with(&data)
                && !self.root.starts_with(&runtime)
                && !runtime.starts_with(&self.root),
            "shared model runtime must be disjoint from project source/data"
        );
        self.store = self.store.with_service_models(&runtime, default)?;
        Ok(self)
    }
    pub(crate) fn configured_model_objects(
        &self,
        provision: bool,
    ) -> Result<Option<ModelObjectSet>> {
        self.model_objects_for_vectors(self.vectors_enabled()?, provision)
    }
    /// Prepare the reviewed desired state before saving it. Preserve custom models.
    pub(crate) fn model_objects_for_vectors(
        &self,
        enabled: bool,
        provision: bool,
    ) -> Result<Option<ModelObjectSet>> {
        if !enabled {
            return Ok(None);
        }
        let Some(runtime) = self.store.shared_model_runtime() else {
            let directory = self.data_root.join("models/local");
            return if directory.try_exists()? {
                Ok(Some(airk_retrieval::model::inspect_object_plan(
                    &directory,
                )?))
            } else {
                Ok(None)
            };
        };
        if let Some(objects) = self.selected_shared_model()? {
            ensure!(
                objects.shared.is_some(),
                "service model policy requires an installed reference"
            );
            self.store.installed_model(&objects)?;
            return Ok(Some(objects));
        }
        let expected = self
            .store
            .shared_model_default()
            .context("service default model metadata missing")?;
        if let Some(objects) = ServiceModelFiles::default_reference(runtime, expected)? {
            return Ok(Some(objects));
        }
        if !provision {
            return Ok(None);
        }
        let objects = airk_retrieval::embedding_worker::connector()?.provision_bundled()?;
        ensure!(
            objects.shared.is_some() && objects.local_set() == *expected,
            "shared helper default model differs from paired metadata"
        );
        self.store.installed_model(&objects)?;
        Ok(Some(objects))
    }
    pub(crate) fn open_configured_model(&self, objects: &ModelObjectSet) -> Result<EmbeddingModel> {
        open_model(&self.store, objects)
    }
    pub(crate) fn retain_configured_model(
        &self,
        objects: &ModelObjectSet,
        reservation: &airk_store::quota::Reservation,
    ) -> Result<()> {
        if objects.shared.is_none() {
            airk_store::model_objects::ModelObjects::new(self.store.directory()).retain(
                &self.data_root.join("models/local"),
                objects,
                reservation,
            )?;
        } else {
            self.store.validate_model_reference(objects, 0)?;
        }
        Ok(())
    }
    pub(crate) fn inherit_shared_models(&self, store: FileStore) -> Result<FileStore> {
        match (
            self.store.shared_model_runtime(),
            self.store.shared_model_default(),
        ) {
            (Some(runtime), Some(default)) => store.with_service_models(runtime, default.clone()),
            (None, None) => Ok(store),
            _ => anyhow::bail!("incomplete service model binding"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{SearchQuery, readiness::SemanticState};
    use std::fs;

    #[test]
    fn first_lexical_publish_and_shared_metadata_work_without_an_inference_connector() -> Result<()>
    {
        let owner = crate::unlinked_tempdir()?;
        let source = owner.path().join("source");
        let data = owner.path().join("data");
        let runtime = owner.path().join("runtime");
        let selected = owner.path().join("selected-model");
        for path in [&source, &data, &runtime, &selected] {
            fs::create_dir(path)?;
        }
        let original = "# Copper beacon\nOriginal copper beacon source.\n";
        fs::write(source.join("memo.md"), original)?;
        let vocabulary = br#"{"copper":[1.0,0.0],"beacon":[0.0,1.0]}"#;
        use sha2::{Digest, Sha256};
        fs::write(selected.join("vocabulary.json"), vocabulary)?;
        fs::write(
            selected.join("model.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema":1,"name":"shared original fixture","license":"Apache-2.0",
                "sha256":format!("{:x}",Sha256::digest(vocabulary)),"dimensions":2
            }))?,
        )?;
        let plan = airk_retrieval::model::inspect_object_plan(&selected)?;
        let kernel = Kernel::open_service_collection(&source, Some(data.clone()), "fixture")?
            .with_service_models(&runtime, plan.clone())?;
        kernel.index_current(false, true)?;
        assert!(kernel.vector_status()?.pending_reindex);
        assert_eq!(
            kernel.status()?.semantic_readiness.state,
            SemanticState::Disabled
        );
        let query = SearchQuery {
            text: "copper".into(),
            path_prefix: None,
            limit: 4,
            max_bytes: 2048,
        };
        assert!(!kernel.search(&query)?.hits.is_empty());
        assert!(
            !runtime.join("model-files-1.4").exists(),
            "source indexing provisioned model files"
        );
        // Enabling used to acknowledge only the switch, then source-only
        // indexing could publish Disabled forever without a prepared model.
        kernel.configure_vectors(false)?;
        let project = crate::service_projects::registration(&source, &data, "fixture")?;
        let enable = || -> Result<_> {
            let inspected = kernel.inspect_service(project.binding)?;
            Ok(airk_core::service_configuration::ConfigurationRequest {
                prepare: false,
                incarnation: inspected.incarnation,
                settings: inspected.settings_digest.context("settings missing")?,
                configuration: airk_core::service_configuration::ProjectConfiguration::Vectors {
                    enabled: true,
                },
            })
        };
        assert!(
            kernel
                .configure_service_project(project.binding, &enable()?)
                .is_err(),
            "enabling without a model or provisioning connector must fail"
        );
        assert!(
            !kernel.vectors_enabled()?,
            "failed setup changed saved policy"
        );
        let mut files = ServiceModelFiles::open(&runtime)?;
        let lease = files.install_readers(&plan, |name| {
            Ok(Box::new(fs::File::open(selected.join(name.filename()))?))
        })?;
        let objects = files.publish_default(&plan)?;
        let result = kernel.configure_service_project(project.binding, &enable()?)?;
        assert_eq!(result["enabled"], true);
        assert_eq!(result["source_pass_requested"], true);
        assert!(kernel.vector_status()?.model_pending_reindex);
        kernel.index_current(false, true)?;
        assert!(!kernel.vector_status()?.model_pending_reindex);
        assert_eq!(
            kernel.status()?.semantic_readiness.state,
            SemanticState::Pending
        );
        assert!(!data.join("models").exists());
        assert!(!kernel.store.directory().join("objects/models").exists());
        let retained = kernel.refresh_service_snapshot()?;
        assert_eq!(retained.configured_model_objects(false)?, Some(objects));
        assert!(
            retained.resume_current_enrichment().is_err(),
            "native inference bypassed absent connector"
        );
        // Lexical evidence and new source publication survive unavailable weights.
        fs::remove_file(lease.directory().join("vocabulary.json"))?;
        assert!(!retained.search(&query)?.hits.is_empty());
        retained.index_current(false, true)?;
        assert_eq!(fs::read_to_string(source.join("memo.md"))?, original);
        assert_eq!(fs::read(selected.join("vocabulary.json"))?, vocabulary);
        Ok(())
    }
}
