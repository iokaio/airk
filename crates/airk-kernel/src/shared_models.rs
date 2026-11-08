// SPDX-License-Identifier: Apache-2.0
//! Collection policy selects opaque references; the native adapter binds their service owner.
use crate::Kernel;
use airk_core::{embedding::EmbeddingDescriptor, model_objects::ModelObjectSet};
use airk_retrieval::model::EmbeddingModel;
use airk_store::{FileStore, service::model_files::ServiceModelFiles};
use anyhow::{Context, Result, ensure};
use std::{path::Path, sync::Arc};

#[cfg(test)]
#[path = "shared_models_embedded_tests.rs"]
mod embedded_tests;

/// An old ONNX publication is retained evidence, not a compatible vector space.
pub(crate) fn semantic_upgrade_pending(
    store: &FileStore,
    objects: Option<&ModelObjectSet>,
) -> bool {
    objects.is_some_and(|objects| {
        objects
            .assets
            .contains_key(&airk_core::model_objects::ModelAssetName::ModelOnnx)
            && store
                .shared_model_default()
                .is_some_and(|default| objects.local_set() != *default)
    })
}

pub(crate) fn open_model(store: &FileStore, objects: &ModelObjectSet) -> Result<EmbeddingModel> {
    if objects.shared.is_some() {
        EmbeddingModel::open_shared(&store.shared_model_source(objects)?, objects)
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
            &store.shared_model_source(objects)?,
            objects,
        )
    } else {
        airk_retrieval::model::descriptor_from_objects_metadata(store.directory(), objects)
    }
}
impl Kernel {
    /// Mutation-only runtime compatibility fence. Metadata reads and binding
    /// never call this: only an actual embedded-default publication may arm it.
    pub(crate) fn require_embedded_default_write(&self, objects: &ModelObjectSet) -> Result<()> {
        let Some(binding) = self.store.embedded_model_binding() else {
            return Ok(());
        };
        let runtime = self
            .store
            .shared_model_runtime()
            .context("embedded model control root missing")?;
        if !binding.matches(runtime, objects)? {
            return Ok(());
        }
        let namespace = airk_store::service::registry::retained_namespace(runtime)?;
        ensure!(
            airk_store::service::data_scope::has_saved_binding(
                &self.data_root,
                runtime,
                namespace
            )?,
            "embedded publication lacks exact native data/control namespace binding"
        );
        airk_store::service::registry::runtime_contract::require_embedded_default_write_for_namespace(runtime,namespace)
    }
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
    /// The native adapter supplies authenticated embedded assets. This binding
    /// permits metadata reads and remote inference, not a local ORT engine.
    pub fn with_embedded_service_models(
        self,
        runtime: &Path,
        default: ModelObjectSet,
        source: Arc<dyn airk_core::model_objects::ModelAssetSource + Send + Sync>,
    ) -> Result<Self> {
        let mut kernel = self.with_service_models(runtime, default.clone())?;
        kernel.store = kernel
            .store
            .with_embedded_service_models(runtime, default, source)?;
        Ok(kernel)
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
        if let Some(objects) = self.selected_shared_model()?
            && !semantic_upgrade_pending(&self.store, Some(&objects))
        {
            ensure!(
                objects.shared.is_some(),
                "service model policy requires an installed reference"
            );
            self.store.validate_model_reference(&objects, 0)?;
            return Ok(Some(objects));
        }
        if let Some(objects) = self.store.embedded_default_reference()? {
            self.store.validate_model_reference(&objects, 0)?;
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
        self.store.validate_model_reference(&objects, 0)?;
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
            self.require_embedded_default_write(objects)?;
            self.store.validate_model_reference(objects, 0)?;
        }
        Ok(())
    }
    pub(crate) fn inherit_shared_models(&self, store: FileStore) -> Result<FileStore> {
        let store = match (
            self.store.shared_model_runtime(),
            self.store.shared_model_default(),
        ) {
            (Some(runtime), Some(default)) => match self.store.embedded_model_binding() {
                Some(binding) => {
                    store.with_embedded_service_models(runtime, default.clone(), binding.source())
                }
                None => store.with_service_models(runtime, default.clone()),
            },
            (None, None) => Ok(store),
            _ => anyhow::bail!("incomplete service model binding"),
        }?;
        if let Some(authority) = &self.split_authority {
            let store = store.with_split_access(authority.permit.clone())?;
            match &authority.funding {
                Some(funding) => store.with_operation_funding(funding.clone()),
                None => Ok(store),
            }
        } else {
            Ok(store)
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
