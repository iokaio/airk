// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::model_objects::ModelObjectRef;
use std::{collections::BTreeMap, sync::Mutex};

struct MetadataOnly {
    bytes: Mutex<Vec<u8>>,
    reads: Mutex<Vec<ModelAssetName>>,
}
impl ModelAssetSource for MetadataOnly {
    fn read_asset(&self, name: ModelAssetName, _maximum: u64) -> Result<Vec<u8>> {
        self.reads.lock().unwrap().push(name);
        ensure!(
            name == ModelAssetName::ModelJson,
            "metadata path read model weights"
        );
        Ok(self.bytes.lock().unwrap().clone())
    }
}
fn fixture() -> (ModelObjectSet, Arc<MetadataOnly>) {
    let bytes = b"embedded metadata fixture".to_vec();
    let source = Arc::new(MetadataOnly {
        bytes: Mutex::new(bytes.clone()),
        reads: Mutex::new(Vec::new()),
    });
    let set = ModelObjectSet {
        version: 1,
        shared: None,
        pipeline_digest: Sha256Digest::from_bytes([3; 32]),
        legacy_identity: Sha256Digest::from_bytes([4; 32]).to_string(),
        assets: BTreeMap::from([
            (
                ModelAssetName::ModelJson,
                ModelObjectRef {
                    sha256: Sha256Digest::from_bytes(Sha256::digest(&bytes).into()),
                    bytes: bytes.len() as u64,
                },
            ),
            (
                ModelAssetName::ModelOnnx,
                ModelObjectRef {
                    sha256: Sha256Digest::from_bytes([5; 32]),
                    bytes: 200_000_000,
                },
            ),
            (
                ModelAssetName::TokenizerJson,
                ModelObjectRef {
                    sha256: Sha256Digest::from_bytes([6; 32]),
                    bytes: 100,
                },
            ),
            (
                ModelAssetName::License,
                ModelObjectRef {
                    sha256: Sha256Digest::from_bytes([7; 32]),
                    bytes: 10,
                },
            ),
        ]),
    };
    (set, source)
}

#[test]
fn embedded_metadata_keeps_content_identity_but_requires_exact_runtime_and_full_set() -> Result<()>
{
    let first = tempfile::tempdir()?;
    let second = tempfile::tempdir()?;
    let (plan, source) = fixture();
    let binding = EmbeddedModelBinding::new(plan.clone(), source.clone())?;
    let reference = binding.reference(first.path())?;
    assert_eq!(reference.local_set(), plan);
    assert!(binding.matches(first.path(), &reference)?);
    assert!(!binding.matches(second.path(), &reference)?);
    let mut changed = plan.clone();
    changed.pipeline_digest = Sha256Digest::from_bytes([9; 32]);
    assert!(!binding.matches(
        first.path(),
        &changed.shared_with(model_scope(first.path())?)?
    )?);
    changed = plan;
    changed
        .assets
        .get_mut(&ModelAssetName::ModelOnnx)
        .unwrap()
        .bytes -= 1;
    assert!(!binding.matches(
        first.path(),
        &changed.shared_with(model_scope(first.path())?)?
    )?);
    assert_eq!(
        *source.reads.lock().unwrap(),
        vec![ModelAssetName::ModelJson]
    );
    assert_eq!(
        std::fs::read_dir(first.path())?.count(),
        0,
        "embedded binding extracted files"
    );
    Ok(())
}

#[test]
fn metadata_budget_and_actual_digest_are_checked_on_every_read() -> Result<()> {
    let (plan, source) = fixture();
    let binding = EmbeddedModelBinding::new(plan, source.clone())?;
    assert!(binding.read_asset(ModelAssetName::ModelOnnx, 1024).is_err());
    assert!(binding.read_asset(ModelAssetName::ModelJson, 1).is_err());
    assert_eq!(
        source.reads.lock().unwrap().len(),
        1,
        "refused read reached source"
    );
    source.bytes.lock().unwrap()[0] ^= 1;
    assert!(
        binding
            .read_asset(ModelAssetName::ModelJson, 16_384)
            .is_err()
    );
    Ok(())
}

#[test]
fn binding_refuses_shared_plans_and_mismatched_manifest_before_publication() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (plan, source) = fixture();
    let shared = plan.shared_with(model_scope(root.path())?)?;
    assert!(EmbeddedModelBinding::new(shared, source.clone()).is_err());
    source.bytes.lock().unwrap().push(0);
    assert!(EmbeddedModelBinding::new(plan, source).is_err());
    Ok(())
}

#[test]
fn explicit_binding_preserves_installed_custom_models_and_refuses_foreign_owners() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let runtime = owner.path().join("runtime");
    let source_root = owner.path().join("source");
    let data = owner.path().join("data");
    for path in [&runtime, &source_root, &data] {
        std::fs::create_dir(path)?;
    }
    let collection = crate::service::collection::Collection::initialize(
        &data,
        source_root.to_str().unwrap(),
        "fixture",
        airk_core::identity::Incarnation::from_bytes([8; 16]),
    )?;
    let (plan, source) = fixture();
    let store = collection
        .selected_store()?
        .with_embedded_service_models(&runtime, plan, source)?;
    let default = store
        .embedded_default_reference()?
        .context("embedded default missing")?;
    assert!(matches!(
        store.shared_model_source(&default)?,
        SharedModelSource::Embedded(_)
    ));
    assert!(!runtime.join("model-files-1.4").exists());
    let custom_bytes = BTreeMap::from([
        (ModelAssetName::ModelJson, b"custom manifest".to_vec()),
        (
            ModelAssetName::VocabularyJson,
            b"custom vocabulary".to_vec(),
        ),
    ]);
    let custom = ModelObjectSet {
        version: 1,
        shared: None,
        pipeline_digest: Sha256Digest::from_bytes([9; 32]),
        legacy_identity: Sha256Digest::from_bytes([10; 32]).to_string(),
        assets: custom_bytes
            .iter()
            .map(|(name, bytes)| {
                (
                    *name,
                    ModelObjectRef {
                        sha256: Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
                        bytes: bytes.len() as u64,
                    },
                )
            })
            .collect(),
    };
    let mut files = super::super::model_files::ServiceModelFiles::open(&runtime)?;
    let _lease = files.install_readers(&custom, |name| {
        Ok(Box::new(std::io::Cursor::new(custom_bytes[&name].clone())))
    })?;
    let custom = files.installed_reference(&custom)?;
    let custom_source = store.shared_model_source(&custom)?;
    assert!(matches!(custom_source, SharedModelSource::Installed(_)));
    assert_eq!(
        custom_source.read_asset(ModelAssetName::VocabularyJson, 1024)?,
        b"custom vocabulary"
    );
    let foreign = tempfile::tempdir()?;
    assert!(
        store
            .shared_model_source(
                &default
                    .local_set()
                    .shared_with(model_scope(foreign.path())?)?
            )
            .is_err()
    );
    Ok(())
}
