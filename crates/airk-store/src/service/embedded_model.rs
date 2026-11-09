// SPDX-License-Identifier: Apache-2.0
//! Explicit native-adapter binding for executable-owned model assets.
use super::model_files::{InstalledModel, model_scope};
use airk_core::{
    identity::Sha256Digest,
    model_objects::{ModelAssetName, ModelAssetSource, ModelObjectSet},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc};

/// Native adapters authenticate the executable/catalog before supplying this
/// source. A project configuration cannot create an embedded binding. It is
/// metadata/source ownership, never permission to instantiate an inference engine.
#[derive(Clone)]
pub struct EmbeddedModelBinding {
    plan: ModelObjectSet,
    source: Arc<dyn ModelAssetSource + Send + Sync>,
}
impl std::fmt::Debug for EmbeddedModelBinding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EmbeddedModelBinding")
            .field("plan", &self.plan)
            .finish_non_exhaustive()
    }
}
impl EmbeddedModelBinding {
    pub fn new(
        plan: ModelObjectSet,
        source: Arc<dyn ModelAssetSource + Send + Sync>,
    ) -> Result<Self> {
        plan.validate()?;
        ensure!(
            plan.shared.is_none(),
            "embedded model plan must be local authenticated metadata"
        );
        let binding = Self { plan, source };
        // Metadata reads must not hash/copy weights or initialize inference.
        binding.read_asset(
            ModelAssetName::ModelJson,
            ModelAssetName::ModelJson.maximum_bytes(),
        )?;
        Ok(binding)
    }
    pub fn reference(&self, runtime: &Path) -> Result<ModelObjectSet> {
        self.plan.shared_with(model_scope(runtime)?)
    }
    pub fn matches(&self, runtime: &Path, objects: &ModelObjectSet) -> Result<bool> {
        objects.validate()?;
        Ok(*objects == self.reference(runtime)?)
    }
    pub fn plan(&self) -> &ModelObjectSet {
        &self.plan
    }
    pub fn source(&self) -> Arc<dyn ModelAssetSource + Send + Sync> {
        self.source.clone()
    }
}
impl ModelAssetSource for EmbeddedModelBinding {
    fn borrowed_asset(&self, name: ModelAssetName, maximum: u64) -> Result<Option<&[u8]>> {
        let asset = self
            .plan
            .assets
            .get(&name)
            .context("embedded model asset absent")?;
        ensure!(
            asset.bytes <= maximum.min(name.maximum_bytes()),
            "embedded model asset exceeds read budget"
        );
        let Some(bytes) = self.source.borrowed_asset(name, asset.bytes)? else {
            return Ok(None);
        };
        ensure!(
            bytes.len() as u64 == asset.bytes
                && Sha256Digest::from_bytes(Sha256::digest(bytes).into()) == asset.sha256,
            "embedded model asset differs from authenticated metadata"
        );
        Ok(Some(bytes))
    }
    fn read_asset(&self, name: ModelAssetName, maximum: u64) -> Result<Vec<u8>> {
        let asset = self
            .plan
            .assets
            .get(&name)
            .context("embedded model asset absent")?;
        ensure!(
            asset.bytes <= maximum.min(name.maximum_bytes()),
            "embedded model asset exceeds read budget"
        );
        let bytes = self.source.read_asset(name, asset.bytes)?;
        ensure!(
            bytes.len() as u64 == asset.bytes
                && Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == asset.sha256,
            "embedded model asset differs from authenticated metadata"
        );
        Ok(bytes)
    }
}

pub enum SharedModelSource<'a> {
    Embedded(&'a EmbeddedModelBinding),
    Installed(InstalledModel),
}
impl ModelAssetSource for SharedModelSource<'_> {
    fn borrowed_asset(&self, name: ModelAssetName, maximum: u64) -> Result<Option<&[u8]>> {
        match self {
            Self::Embedded(source) => source.borrowed_asset(name, maximum),
            Self::Installed(source) => source.borrowed_asset(name, maximum),
        }
    }
    fn read_asset(&self, name: ModelAssetName, maximum: u64) -> Result<Vec<u8>> {
        match self {
            Self::Embedded(source) => source.read_asset(name, maximum),
            Self::Installed(source) => source.read_asset(name, maximum),
        }
    }
}

#[cfg(test)]
mod tests;
