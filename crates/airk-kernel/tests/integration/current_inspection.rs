// SPDX-License-Identifier: Apache-2.0
//! Detached test observations of authenticated current metadata, never historical readers.
use airk_core::{
    Artifact, ArtifactSource, Snapshot,
    identity::{RevisionToken, Sha256Digest},
    publication::InventoryFile,
};
use airk_store::{
    FileStore,
    current::{CurrentScope, postings::Lookup},
};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

pub struct CurrentInventory {
    pub files: BTreeMap<String, InventoryFile>,
    pub profiles: BTreeMap<Sha256Digest, airk_core::extraction::ExtractionProfile>,
}
pub struct CurrentObservation {
    pub snapshot: Snapshot,
    pub inventory: Option<CurrentInventory>,
    pub token: RevisionToken,
    store: FileStore,
}

impl ArtifactSource for CurrentObservation {
    fn load_artifact(&self, path: &str) -> Result<Artifact> {
        self.store.load_retained(
            self.snapshot
                .artifacts
                .get(path)
                .context("test artifact missing")?,
        )
    }
}
pub trait InspectCurrent {
    fn current_test_catalog(&self) -> Result<Option<CurrentObservation>>;
}
impl InspectCurrent for FileStore {
    fn current_test_catalog(&self) -> Result<Option<CurrentObservation>> {
        let owner = self.directory().join("current-owner.json");
        if !owner.try_exists()? {
            return Ok(None);
        }
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(owner)?)?;
        let scope: CurrentScope = serde_json::from_value(value["scope"].clone())?;
        let read = self.current_read(&scope)?;
        let state = read.collection_state()?.context("current state missing")?;
        let mut snapshot = Snapshot::empty(scope.root);
        snapshot.collection = scope.collection;
        snapshot.generation = read.head().sequence;
        snapshot.extractor_version = state.configuration.extractor_version;
        snapshot.roots = state.configuration.roots;
        snapshot.storage_format = 120;
        snapshot.persistent_indexes = true;
        snapshot.embedding_model = state
            .configuration
            .model
            .map(|m| m.pipeline_digest.to_string());
        let page = read.lookup_paths(Lookup::Files, "", None, 100_000)?;
        ensure!(page.next_after.is_none(), "test observation too large");
        let mut files = BTreeMap::new();
        for path in page.paths {
            files.insert(
                path.clone(),
                read.file_metadata(self, &path)?
                    .context("current file metadata missing")?,
            );
            snapshot.artifacts.insert(
                path.clone(),
                read.artifact(&path)?.context("current artifact missing")?,
            );
        }
        if let Some(analysis) = read.analysis(self)? {
            snapshot.analysis = Some(analysis.manifest);
            snapshot.imported_facts = analysis.facts;
            snapshot.imported_relationships = analysis.relationships;
        }
        let profiles = snapshot
            .artifacts
            .values()
            .map(|a| {
                let profile = a
                    .retained
                    .as_ref()
                    .and_then(|r| r.profile.as_ref())
                    .context("current test profile missing")?;
                Ok((profile.identity()?, profile.clone()))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let current = read.token()?;
        let token = RevisionToken {
            incarnation: current.incarnation,
            generation: current.sequence,
            revision: Sha256Digest::from_bytes(*current.digest.as_bytes()),
        };
        Ok(Some(CurrentObservation {
            snapshot,
            inventory: Some(CurrentInventory { files, profiles }),
            token,
            store: self.clone(),
        }))
    }
}

pub fn read_current(store: &FileStore) -> Result<std::sync::Arc<airk_store::current::CurrentRead>> {
    let owner: serde_json::Value = serde_json::from_slice(&std::fs::read(
        store.directory().join("current-owner.json"),
    )?)?;
    store.current_read(&serde_json::from_value(owner["scope"].clone())?)
}
