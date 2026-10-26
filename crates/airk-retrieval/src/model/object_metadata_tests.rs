// SPDX-License-Identifier: Apache-2.0
use super::*;

struct Fixture {
    collection: tempfile::TempDir,
    set: ModelObjectSet,
    descriptor: EmbeddingDescriptor,
}

impl Fixture {
    fn new(onnx: bool) -> Result<Self> {
        let collection =
            tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?;
        let mut assets = if onnx {
            BTreeMap::from([
                (
                    ModelAssetName::ModelOnnx,
                    b"original opaque model fixture".to_vec(),
                ),
                (
                    ModelAssetName::TokenizerJson,
                    b"original opaque tokenizer fixture".to_vec(),
                ),
                (
                    ModelAssetName::License,
                    b"original fixture license".to_vec(),
                ),
            ])
        } else {
            BTreeMap::from([(
                ModelAssetName::VocabularyJson,
                br#"{"original":[1.0,0.0],"beacon":[0.0,1.0]}"#.to_vec(),
            )])
        };
        let asset_hash = |name| format!("{:x}", Sha256::digest(&assets[&name]));
        let (bytes, descriptor, identity) = if onnx {
            let manifest = OnnxManifest {
                schema: 2,
                name: "original metadata fixture".into(),
                license: "Apache-2.0".into(),
                source: "https://example.invalid/original-metadata".into(),
                revision: "original-v1".into(),
                model_sha256: asset_hash(ModelAssetName::ModelOnnx),
                tokenizer_sha256: asset_hash(ModelAssetName::TokenizerJson),
                license_sha256: asset_hash(ModelAssetName::License),
                dimensions: 2,
                max_tokens: 256,
                pooling: "attention-mask-mean-l2".into(),
                input_ids: "input_ids".into(),
                attention_mask: "attention_mask".into(),
                token_type_ids: None,
                output: "last_hidden_state".into(),
            };
            let bytes = serde_json::to_vec(&manifest)?;
            let descriptor = descriptor_from_manifest(DescriptorManifest::Onnx(&manifest))?;
            let identity = onnx_manifest(&bytes)?.1;
            (bytes, descriptor, identity)
        } else {
            let manifest = crate::vector::ModelManifest {
                schema: 1,
                name: "original metadata fixture".into(),
                license: "Apache-2.0".into(),
                sha256: asset_hash(ModelAssetName::VocabularyJson),
                dimensions: 2,
            };
            let bytes = serde_json::to_vec(&manifest)?;
            let descriptor = descriptor_from_manifest(DescriptorManifest::Word(&manifest))?;
            (bytes, descriptor, manifest.sha256)
        };
        assets.insert(ModelAssetName::ModelJson, bytes);
        let directory = collection.path().join("objects/models");
        fs::create_dir_all(&directory)?;
        let mut references = BTreeMap::new();
        for (name, bytes) in assets {
            let sha256 = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
            fs::write(directory.join(sha256.to_string()), &bytes)?;
            references.insert(
                name,
                ModelObjectRef {
                    sha256,
                    bytes: bytes.len() as u64,
                },
            );
        }
        let set = ModelObjectSet {
            version: 1,
            shared: None,
            pipeline_digest: descriptor.identity()?,
            legacy_identity: identity,
            assets: references,
        };
        Ok(Self {
            collection,
            set,
            descriptor,
        })
    }

    fn path(&self, name: ModelAssetName) -> std::path::PathBuf {
        self.collection
            .path()
            .join("objects/models")
            .join(self.set.assets[&name].sha256.to_string())
    }

    fn replace_metadata(&mut self, change: impl FnOnce(&mut serde_json::Value)) -> Result<()> {
        let mut value = serde_json::from_slice(&fs::read(self.path(ModelAssetName::ModelJson))?)?;
        change(&mut value);
        let bytes = serde_json::to_vec(&value)?;
        let reference = self.set.assets.get_mut(&ModelAssetName::ModelJson).unwrap();
        reference.sha256 = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
        reference.bytes = bytes.len() as u64;
        fs::write(self.path(ModelAssetName::ModelJson), bytes)?;
        Ok(())
    }
}

#[test]
fn metadata_descriptor_matches_full_inspection_without_reading_other_assets() -> Result<()> {
    for onnx in [false, true] {
        let fixture = Fixture::new(onnx)?;
        assert_eq!(
            inspect_objects_cache_descriptor(fixture.collection.path(), &fixture.set)?,
            fixture.descriptor
        );
        assert_eq!(
            descriptor_from_objects_metadata(fixture.collection.path(), &fixture.set)?,
            fixture.descriptor
        );
        if !onnx {
            assert!(
                EmbeddingModel::open_objects(fixture.collection.path(), &fixture.set)?
                    .embed_text("original")?
                    .is_some()
            );
        }
        for name in fixture
            .set
            .assets
            .keys()
            .filter(|name| **name != ModelAssetName::ModelJson)
        {
            fs::remove_file(fixture.path(*name))?;
        }
        assert_eq!(
            descriptor_from_objects_metadata(fixture.collection.path(), &fixture.set)?,
            fixture.descriptor
        );
        assert!(inspect_objects_cache_descriptor(fixture.collection.path(), &fixture.set).is_err());
        assert!(EmbeddingModel::open_objects(fixture.collection.path(), &fixture.set).is_err());
    }
    Ok(())
}

#[test]
fn metadata_descriptor_authenticates_manifest_and_every_declared_asset() -> Result<()> {
    for onnx in [false, true] {
        let fixture = Fixture::new(onnx)?;
        for name in fixture
            .set
            .assets
            .keys()
            .filter(|name| **name != ModelAssetName::ModelJson)
        {
            let mut changed = fixture.set.clone();
            changed.assets.get_mut(name).unwrap().sha256 = Sha256Digest::from_bytes([9; 32]);
            assert!(
                descriptor_from_objects_metadata(fixture.collection.path(), &changed).is_err(),
                "{name:?}"
            );
        }
        let path = fixture.path(ModelAssetName::ModelJson);
        let bytes = fs::read(&path)?;
        let mut changed = bytes.clone();
        let offset = changed
            .windows(8)
            .position(|window| window == b"original")
            .unwrap();
        changed[offset] = b'O';
        fs::write(&path, changed)?;
        assert!(descriptor_from_objects_metadata(fixture.collection.path(), &fixture.set).is_err());
        fs::write(&path, bytes)?;
        for change_pipeline in [false, true] {
            let mut changed = fixture.set.clone();
            if change_pipeline {
                changed.pipeline_digest = Sha256Digest::from_bytes([8; 32]);
            } else {
                changed.legacy_identity = Sha256Digest::from_bytes([7; 32]).to_string();
            }
            assert!(descriptor_from_objects_metadata(fixture.collection.path(), &changed).is_err());
        }
    }
    Ok(())
}

#[test]
fn metadata_descriptor_rejects_invalid_or_changed_contracts_and_bounds() -> Result<()> {
    for onnx in [false, true] {
        for (key, value) in [
            ("dimensions", serde_json::json!(0)),
            ("dimensions", serde_json::json!(2049)),
            ("dimensions", serde_json::json!(3)),
            ("name", serde_json::json!(" ")),
            ("license", serde_json::json!(" ")),
            ("schema", serde_json::json!(9)),
            ("unknown", serde_json::json!(true)),
        ] {
            let mut fixture = Fixture::new(onnx)?;
            fixture.replace_metadata(|metadata| metadata[key] = value)?;
            assert!(
                descriptor_from_objects_metadata(fixture.collection.path(), &fixture.set).is_err(),
                "{key} (onnx={onnx})"
            );
        }
        let mut fixture = Fixture::new(onnx)?;
        fixture
            .set
            .assets
            .get_mut(&ModelAssetName::ModelJson)
            .unwrap()
            .bytes = 16_385;
        assert!(descriptor_from_objects_metadata(fixture.collection.path(), &fixture.set).is_err());
    }
    let mut word = Fixture::new(false)?;
    let onnx = Fixture::new(true)?;
    let metadata = word.set.assets[&ModelAssetName::ModelJson].clone();
    word.set.assets = onnx.set.assets.clone();
    word.set.assets.insert(ModelAssetName::ModelJson, metadata);
    assert!(descriptor_from_objects_metadata(word.collection.path(), &word.set).is_err());
    Ok(())
}

#[test]
fn verified_byte_source_planning_matches_existing_assets_without_directory_staging() -> Result<()> {
    for onnx in [false, true] {
        let fixture = Fixture::new(onnx)?;
        let source = ObjectAssets {
            collection: fixture.collection.path(),
            set: &fixture.set,
        };
        assert_eq!(inspect_asset_plan(&source)?, fixture.set);
        assert_eq!(
            descriptor_from_asset_metadata(&source, &fixture.set)?,
            fixture.descriptor
        );
    }
    Ok(())
}

#[test]
fn shared_model_metadata_is_lazy_even_with_native_inference_enabled() -> Result<()> {
    struct MetadataOnly(Vec<u8>);
    impl ModelAssetSource for MetadataOnly {
        fn read_asset(&self, name: ModelAssetName, maximum: u64) -> Result<Vec<u8>> {
            ensure!(
                name == ModelAssetName::ModelJson,
                "weights opened during metadata setup"
            );
            ensure!(self.0.len() as u64 <= maximum, "metadata exceeds budget");
            Ok(self.0.clone())
        }
    }
    for onnx in [false, true] {
        let fixture = Fixture::new(onnx)?;
        let source = MetadataOnly(fs::read(fixture.path(ModelAssetName::ModelJson))?);
        let shared = fixture.set.shared_with(Sha256Digest::from_bytes([7; 32]))?;
        let model = EmbeddingModel::open_shared(&source, &shared)?;
        assert!(matches!(model, EmbeddingModel::Remote(_)));
        assert_eq!(model.cache_descriptor()?, fixture.descriptor);
        assert!(
            model.embed_text("original beacon").is_err(),
            "inference bypassed absent connector"
        );
        assert!(EmbeddingModel::open_shared(&source, &fixture.set).is_err());
        let mut tampered = source.0.clone();
        tampered[0] = b' ';
        assert!(EmbeddingModel::open_shared(&MetadataOnly(tampered), &shared).is_err());
    }
    Ok(())
}
