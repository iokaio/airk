// SPDX-License-Identifier: Apache-2.0
//! Explicit source-root configuration and revision-bound original locations.
use crate::Kernel;
use airk_store::roots::RootConfig;
use anyhow::Result;
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Serialize)]
pub struct RootsReport {
    pub collection: String,
    pub primary_root: String,
    pub configured: BTreeMap<String, String>,
    pub indexed: BTreeMap<String, String>,
    pub generation: Option<u64>,
    pub pending_index: bool,
}

#[derive(Debug, Serialize)]
pub struct LocatedSource {
    pub collection: String,
    pub generation: u64,
    pub alias: String,
    pub root: String,
    pub relative_path: String,
}

impl Kernel {
    pub fn configured_roots(&self) -> Result<BTreeMap<String, String>> {
        let config = RootConfig::load(self.store.directory(), &self.root_id())?;
        // Empty legacy collections do not need to create storage for read-only inspection.
        if !config.roots.is_empty() {
            airk_store::roots::validate_sources(&self.root, &self.data_root, &config.roots)?;
        }
        Ok(config.roots)
    }

    pub fn roots(&self) -> Result<RootsReport> {
        let config = RootConfig::load(self.store.directory(), &self.root_id())?;
        let snapshot = self.load_optional()?;
        let indexed = snapshot
            .as_ref()
            .map(|snapshot| snapshot.roots.clone())
            .unwrap_or_default();
        Ok(RootsReport {
            collection: self.collection.clone(),
            primary_root: self.root_id(),
            pending_index: indexed != config.roots,
            configured: config.roots,
            indexed,
            generation: snapshot.map(|snapshot| snapshot.generation),
        })
    }

    pub fn add_root(&self, alias: &str, path: &Path) -> Result<RootsReport> {
        if let Some(selected) = self.active_service_view()? {
            return selected.add_root(alias, path);
        }
        let lock = self.lock_current_writer()?;
        self.load_optional()?; // validate the collection owner before mutating configuration
        let mut config = RootConfig::load(self.store.directory(), &self.root_id())?;
        config.add(alias, path, &self.root, &self.data_root)?;
        let reservation = self
            .collection_quota()?
            .reserve(&self.collection, 96 * 1024)?;
        reservation.charge(96 * 1024)?;
        config.save(self.store.directory())?;
        drop(lock);
        self.roots()
    }

    pub fn remove_root(&self, alias: &str) -> Result<RootsReport> {
        if let Some(selected) = self.active_service_view()? {
            return selected.remove_root(alias);
        }
        let lock = self.lock_current_writer()?;
        self.load_optional()?;
        let mut config = RootConfig::load(self.store.directory(), &self.root_id())?;
        config.remove(alias)?;
        let reservation = self
            .collection_quota()?
            .reserve(&self.collection, 96 * 1024)?;
        reservation.charge(96 * 1024)?;
        config.save(self.store.directory())?;
        drop(lock);
        self.roots()
    }

    pub fn source_location(&self, path: &str) -> Result<LocatedSource> {
        let snapshot = self.load()?;
        anyhow::ensure!(
            snapshot.artifacts.contains_key(path),
            "source is not indexed"
        );
        let source = airk_store::roots::source_location(&snapshot.root, &snapshot.roots, path)?;
        Ok(LocatedSource {
            collection: snapshot.collection,
            generation: snapshot.generation,
            alias: source.alias,
            root: source.root,
            relative_path: source.relative_path,
        })
    }
}
