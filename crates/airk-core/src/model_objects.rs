// SPDX-License-Identifier: Apache-2.0
//! Neutral references to collection-owned, user-provisioned model assets.
use crate::identity::Sha256Digest;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_MODEL_OBJECT_SET_BYTES: usize = 16_384;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ModelAssetName {
    #[serde(rename = "model.json")]
    ModelJson,
    #[serde(rename = "vocabulary.json")]
    VocabularyJson,
    #[serde(rename = "model.onnx")]
    ModelOnnx,
    #[serde(rename = "tokenizer.json")]
    TokenizerJson,
    #[serde(rename = "LICENSE")]
    License,
}
impl ModelAssetName {
    pub fn filename(self) -> &'static str {
        match self {
            Self::ModelJson => "model.json",
            Self::VocabularyJson => "vocabulary.json",
            Self::ModelOnnx => "model.onnx",
            Self::TokenizerJson => "tokenizer.json",
            Self::License => "LICENSE",
        }
    }
    pub fn maximum_bytes(self) -> u64 {
        match self {
            Self::ModelJson => 16_384,
            Self::VocabularyJson | Self::ModelOnnx => 268_435_456,
            Self::TokenizerJson => 4_194_304,
            Self::License => 65_536,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelObjectRef {
    pub sha256: Sha256Digest,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelObjectSet {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared: Option<SharedModelRef>,
    pub pipeline_digest: Sha256Digest,
    pub legacy_identity: String,
    pub assets: BTreeMap<ModelAssetName, ModelObjectRef>,
}
/// Content/owner identity only. Native adapters supply the trusted store root;
/// project configuration cannot choose an arbitrary filesystem model location.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedModelRef {
    pub owner: Sha256Digest,
    pub set: Sha256Digest,
}
impl ModelObjectSet {
    pub fn local_set(&self) -> Self {
        let mut set = self.clone();
        set.shared = None;
        set
    }
    pub fn shared_with(&self, owner: Sha256Digest) -> Result<Self> {
        let mut set = self.local_set();
        let digest = Sha256Digest::from_bytes(Sha256::digest(set.canonical_bytes()?).into());
        set.shared = Some(SharedModelRef { owner, set: digest });
        set.validate()?;
        Ok(set)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "unsupported model object set");
        self.legacy_identity.parse::<Sha256Digest>()?;
        if let Some(shared) = &self.shared {
            ensure!(
                shared.owner != Sha256Digest::from_bytes([0; 32])
                    && shared.set
                        == Sha256Digest::from_bytes(
                            Sha256::digest(self.local_set().canonical_bytes()?).into()
                        ),
                "shared model reference owner or content mismatch"
            );
        }
        let names: BTreeSet<_> = self.assets.keys().copied().collect();
        ensure!(
            names == BTreeSet::from([ModelAssetName::ModelJson, ModelAssetName::VocabularyJson])
                || names
                    == BTreeSet::from([
                        ModelAssetName::ModelJson,
                        ModelAssetName::ModelOnnx,
                        ModelAssetName::TokenizerJson,
                        ModelAssetName::License
                    ]),
            "model asset set is incomplete or unsupported"
        );
        let mut lengths = BTreeMap::new();
        for (name, reference) in &self.assets {
            ensure!(
                reference.bytes > 0 && reference.bytes <= name.maximum_bytes(),
                "model asset size exceeds limit"
            );
            if let Some(prior) = lengths.insert(reference.sha256, reference.bytes) {
                ensure!(prior == reference.bytes, "conflicting model object lengths");
            }
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_MODEL_OBJECT_SET_BYTES,
            "model object set exceeds byte limit"
        );
        Ok(bytes)
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_MODEL_OBJECT_SET_BYTES,
            "model object set exceeds byte limit"
        );
        let value: Self = serde_json::from_slice(bytes)?;
        ensure!(
            value.canonical_bytes()? == bytes,
            "noncanonical model object set"
        );
        Ok(value)
    }
}

/// Implementations verify bytes before returning them. They never fetch assets.
pub trait ModelAssetSource {
    fn read_asset(&self, name: ModelAssetName, maximum: u64) -> Result<Vec<u8>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn original() -> ModelObjectSet {
        ModelObjectSet {
            version: 1,
            shared: None,
            pipeline_digest: Sha256Digest::from_bytes([3; 32]),
            legacy_identity: Sha256Digest::from_bytes([4; 32]).to_string(),
            assets: BTreeMap::from([
                (
                    ModelAssetName::ModelJson,
                    ModelObjectRef {
                        sha256: Sha256Digest::from_bytes([1; 32]),
                        bytes: 12,
                    },
                ),
                (
                    ModelAssetName::VocabularyJson,
                    ModelObjectRef {
                        sha256: Sha256Digest::from_bytes([2; 32]),
                        bytes: 24,
                    },
                ),
            ]),
        }
    }
    #[test]
    fn bounded_exact_asset_sets_roundtrip_and_reject_unknown_names() -> Result<()> {
        let set = original();
        assert_eq!(ModelObjectSet::from_bytes(&set.canonical_bytes()?)?, set);
        let mut invalid = set.clone();
        invalid.assets.remove(&ModelAssetName::VocabularyJson);
        assert!(invalid.validate().is_err());
        let mut invalid = set.clone();
        invalid
            .assets
            .get_mut(&ModelAssetName::ModelJson)
            .unwrap()
            .bytes = 16_385;
        assert!(invalid.validate().is_err());
        let mut bytes = set.canonical_bytes()?;
        bytes.push(b' ');
        assert!(ModelObjectSet::from_bytes(&bytes).is_err());
        assert!(serde_json::from_str::<ModelAssetName>("\"../model.json\"").is_err());
        Ok(())
    }
    #[test]
    fn shared_model_reference_binds_exact_assets_and_has_no_filesystem_location() -> Result<()> {
        let local = original();
        assert!(serde_json::to_value(&local)?.get("shared").is_none());
        let shared = local.shared_with(Sha256Digest::from_bytes([7; 32]))?;
        assert_eq!(shared.local_set(), local);
        assert_eq!(
            ModelObjectSet::from_bytes(&shared.canonical_bytes()?)?,
            shared
        );
        let mut changed = shared.clone();
        changed
            .assets
            .get_mut(&ModelAssetName::ModelJson)
            .unwrap()
            .bytes += 1;
        assert!(changed.validate().is_err());
        changed = shared;
        changed.shared.as_mut().unwrap().set = Sha256Digest::from_bytes([8; 32]);
        assert!(changed.validate().is_err());
        assert!(
            local
                .shared_with(Sha256Digest::from_bytes([0; 32]))
                .is_err()
        );
        Ok(())
    }
}
