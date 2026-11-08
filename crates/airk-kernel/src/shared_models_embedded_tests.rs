// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::model_objects::{ModelAssetName, ModelAssetSource};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, sync::Mutex};

struct MetadataSource {
    assets: BTreeMap<ModelAssetName, Vec<u8>>,
    reads: Mutex<Vec<ModelAssetName>>,
}
impl ModelAssetSource for MetadataSource {
    fn read_asset(&self, name: ModelAssetName, maximum: u64) -> Result<Vec<u8>> {
        self.reads.lock().unwrap().push(name);
        ensure!(
            name == ModelAssetName::ModelJson,
            "metadata path read inference assets"
        );
        let bytes = self.assets.get(&name).context("fixture asset missing")?;
        ensure!(bytes.len() as u64 <= maximum, "fixture budget exceeded");
        Ok(bytes.clone())
    }
}

#[test]
fn embedded_metadata_survives_refresh_without_files_connector_or_source_changes() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    let runtime = owner.path().join("runtime");
    let model = owner.path().join("model");
    for path in [&source, &data, &runtime, &model] {
        fs::create_dir(path)?;
    }
    let original = b"# Harbor beacon\nOriginal harbor source stays unchanged.\n";
    fs::write(source.join("memo.md"), original)?;
    let vocabulary = br#"{"harbor":[1.0,0.0],"beacon":[0.0,1.0]}"#;
    let manifest = serde_json::to_vec(
        &serde_json::json!({"schema":1,"name":"embedded metadata fixture","license":"Apache-2.0","sha256":format!("{:x}",Sha256::digest(vocabulary)),"dimensions":2}),
    )?;
    fs::write(model.join("model.json"), &manifest)?;
    fs::write(model.join("vocabulary.json"), vocabulary)?;
    let plan = airk_retrieval::model::inspect_object_plan(&model)?;
    let assets = Arc::new(MetadataSource {
        assets: BTreeMap::from([
            (ModelAssetName::ModelJson, manifest.clone()),
            (ModelAssetName::VocabularyJson, vocabulary.to_vec()),
        ]),
        reads: Mutex::new(Vec::new()),
    });
    let kernel = Kernel::open_service_collection(&source, Some(data.clone()), "fixture")?
        .with_embedded_service_models(&runtime, plan.clone(), assets.clone())?;
    let objects = kernel
        .store
        .embedded_default_reference()?
        .context("embedded reference missing")?;
    assert_eq!(objects.local_set(), plan);
    assert!(
        kernel.store.installed_model(&objects).is_err(),
        "fixture accidentally has extracted assets"
    );
    kernel.configure_vectors(true)?;
    assert_eq!(
        kernel.configured_model_objects(false)?,
        Some(objects.clone())
    );
    assert_eq!(
        descriptor(&kernel.store, &objects)?,
        airk_retrieval::model::descriptor_from_asset_metadata(&*assets, &objects)?
    );
    let remote = kernel.open_configured_model(&objects)?;
    assert!(
        matches!(remote, EmbeddingModel::Remote(_)),
        "embedded metadata initialized a local inference engine"
    );
    let policy_before = fs::read(kernel.store.directory().join("vector-policy.json"))?;
    assert!(
        kernel.index_current(false, true).is_err(),
        "unqualified embedded publication bypassed runtime compatibility admission"
    );
    assert_eq!(
        fs::read(kernel.store.directory().join("vector-policy.json"))?,
        policy_before
    );
    if let Some(scope) = kernel.current_scope()? {
        assert!(
            kernel
                .store
                .current_read(&scope)?
                .collection_state()?
                .is_none_or(|state| state.configuration.model.is_none()),
            "rejected write published an embedded model reference"
        );
    }
    assert!(
        !runtime.join("service-1.4.sqlite3").exists(),
        "mutation guard silently initialized its registry"
    );
    let refreshed = kernel.refresh_service_snapshot()?;
    assert_eq!(
        refreshed.configured_model_objects(false)?,
        Some(objects.clone())
    );
    let mut crossed = plan;
    crossed.pipeline_digest = airk_core::identity::Sha256Digest::from_bytes([7; 32]);
    let crossed = crossed.shared_with(airk_store::service::model_files::model_scope(&runtime)?)?;
    assert!(
        refreshed.store.shared_model_source(&crossed).is_err(),
        "foreign model substituted embedded default"
    );
    assert!(!runtime.join("model-files-1.4").exists());
    assert!(!data.join("models").exists());
    assert_eq!(fs::read(source.join("memo.md"))?, original);
    assert_eq!(fs::read(model.join("model.json"))?, manifest);
    assert!(
        assets
            .reads
            .lock()
            .unwrap()
            .iter()
            .all(|name| *name == ModelAssetName::ModelJson)
    );
    Ok(())
}
#[test]
fn embedded_mutation_rejects_foreign_data_namespace_before_registry_promotion() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    let runtime = owner.path().join("runtime");
    let foreign = owner.path().join("foreign");
    let model = owner.path().join("model");
    for path in [&source, &data, &runtime, &foreign, &model] {
        fs::create_dir(path)?;
    }
    fs::write(source.join("memo.md"), b"original harbor")?;
    let current = airk_core::identity::Incarnation::from_bytes([1; 16]);
    let registry = airk_store::service::registry::ServiceRegistry::open(&runtime, current)?;
    airk_store::service::data_scope::bind(
        &data,
        &foreign,
        airk_core::identity::Incarnation::from_bytes([2; 16]),
    )?;
    let vocabulary = br#"{"harbor":[1.0,0.0],"beacon":[0.0,1.0]}"#;
    let manifest = serde_json::to_vec(
        &serde_json::json!({"schema":1,"name":"namespace fixture","license":"Apache-2.0","sha256":format!("{:x}",Sha256::digest(vocabulary)),"dimensions":2}),
    )?;
    fs::write(model.join("model.json"), &manifest)?;
    fs::write(model.join("vocabulary.json"), vocabulary)?;
    let plan = airk_retrieval::model::inspect_object_plan(&model)?;
    let assets = Arc::new(MetadataSource {
        assets: BTreeMap::from([(ModelAssetName::ModelJson, manifest)]),
        reads: Mutex::new(Vec::new()),
    });
    let kernel = Kernel::open_service_collection(&source, Some(data), "fixture")?
        .with_embedded_service_models(&runtime, plan, assets)?;
    let objects = kernel
        .store
        .embedded_default_reference()?
        .context("missing embedded reference")?;
    let before = airk_store::service::registry::runtime_contract::required_capabilities(&runtime)?;
    assert!(kernel.require_embedded_default_write(&objects).is_err());
    assert_eq!(
        airk_store::service::registry::runtime_contract::required_capabilities(&runtime)?,
        before
    );
    assert_eq!(
        registry.incarnation()?,
        airk_store::service::registry::retained_namespace(&runtime)?
    );
    assert_eq!(fs::read(source.join("memo.md"))?, b"original harbor");
    Ok(())
}
