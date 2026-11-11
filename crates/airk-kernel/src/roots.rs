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
        if self.read_scope.is_some() {
            let read = self.current_reader()?;
            let config = RootConfig::load(self.store.directory(), &self.root_id())?;
            self.require_configured_index_metadata(&read, &config.roots)?;
            self.scoped_read_fingerprint()?;
            return Ok(config.roots);
        }
        let config = RootConfig::load(self.store.directory(), &self.root_id())?;
        // Empty legacy collections do not need to create storage for read-only inspection.
        if !config.roots.is_empty() {
            airk_store::roots::validate_sources(&self.root, &self.data_root, &config.roots)?;
        }
        Ok(config.roots)
    }

    pub fn roots(&self) -> Result<RootsReport> {
        if self.read_scope.is_some() {
            let read = self.current_reader()?;
            let config = RootConfig::load(self.store.directory(), &self.root_id())?;
            self.require_configured_index_metadata(&read, &config.roots)?;
            let state = read
                .collection_state()?
                .ok_or_else(|| anyhow::anyhow!("current configuration missing"))?;
            self.scoped_read_fingerprint()?;
            return Ok(RootsReport {
                collection: self.collection.clone(),
                primary_root: read.head().scope.root.clone(),
                configured: config.roots,
                indexed: state.configuration.roots,
                generation: Some(read.head().sequence),
                pending_index: false,
            });
        }
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
        let lock = self.store.lock()?;
        self.load_optional()?; // validate the collection owner before mutating configuration
        let mut config = RootConfig::load(self.store.directory(), &self.root_id())?;
        config.add(alias, path, &self.root, &self.data_root)?;
        let reservation = self
            .collection_quota()?
            .reserve(&self.collection, 96 * 1024)?;
        reservation.charge(96 * 1024)?;
        self.store.invalidate_owned_state_before_mutation(&lock)?;
        config.save(self.store.directory())?;
        drop(lock);
        self.roots()
    }

    pub fn remove_root(&self, alias: &str) -> Result<RootsReport> {
        let lock = self.store.lock()?;
        self.load_optional()?;
        let mut config = RootConfig::load(self.store.directory(), &self.root_id())?;
        config.remove(alias)?;
        let reservation = self
            .collection_quota()?
            .reserve(&self.collection, 96 * 1024)?;
        reservation.charge(96 * 1024)?;
        self.store.invalidate_owned_state_before_mutation(&lock)?;
        config.save(self.store.directory())?;
        drop(lock);
        self.roots()
    }

    pub fn source_location(&self, path: &str) -> Result<LocatedSource> {
        let read = self.current_reader()?;
        read.require_path(path)?;
        anyhow::ensure!(read.artifact(path)?.is_some(), "source is not indexed");
        let state = read
            .collection_state()?
            .ok_or_else(|| anyhow::anyhow!("current configuration missing"))?;
        let source = airk_store::roots::source_location(
            &read.head().scope.root,
            &state.configuration.roots,
            path,
        )?;
        Ok(LocatedSource {
            collection: self.collection.clone(),
            generation: read.head().sequence,
            alias: source.alias,
            root: source.root,
            relative_path: source.relative_path,
        })
    }
}
