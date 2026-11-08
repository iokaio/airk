// SPDX-License-Identifier: Apache-2.0
//! Historical Current is read only through its native authenticated reader,
//! on a separately inventoried managed-layout working copy.
use super::*;
use crate::current::{
    CurrentHead, CurrentRead, CurrentToken,
    collection::CollectionState,
    postings::Lookup,
    resources::{KEYS, ResourceReference},
};
use std::sync::Arc;

/// Original evidence, not permission to reuse a model or producer in the new
/// namespace. Resources and graph remain in the immutable retained archive.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalCurrentPublication {
    pub token: CurrentToken,
    pub head: CurrentHead,
    pub collection: CollectionState,
    pub resources: BTreeMap<String, ResourceReference>,
    pub resource_conversion: String,
}
impl OriginalCurrentPublication {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.head.token()? == self.token && self.collection.version == 1,
            "original Current token or configuration changed"
        );
        self.collection.configuration.validate()?;
        ensure!(
            self.resources.len() <= KEYS.len()
                && self
                    .resources
                    .keys()
                    .all(|key| KEYS.contains(&key.as_str()))
                && self.resource_conversion
                    == "retained_original_archive; only validated analysis and artifact evidence are converted; graph, model, policies and other resource execution are not adopted",
            "original Current resource provenance changed"
        );
        Ok(())
    }
}

pub(super) struct CurrentArchive {
    pub original: OriginalCurrentPublication,
    pub snapshot: Snapshot,
    read: Arc<CurrentRead>,
}
impl CurrentArchive {
    pub fn revalidate(&self) -> Result<()> {
        ensure!(
            self.read.token()? == self.original.token && !self.read.has_prepared_update()?,
            "original Current publication changed or requires recovery"
        );
        Ok(())
    }
}

pub(super) fn present(directory: &Path) -> Result<bool> {
    let mut present = false;
    for name in [
        "current-owner.json",
        "current.sqlite3",
        "current-bootstrap.json",
        "current.sqlite3-wal",
        "current.sqlite3-shm",
        "current.sqlite3-journal",
    ] {
        match std::fs::symlink_metadata(directory.join(name)) {
            Ok(_) => present = true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    if present {
        for name in [
            "index.sqlite3",
            "index.sqlite3-wal",
            "index.sqlite3-shm",
            "index.sqlite3-journal",
            "snapshot.json",
            "generations",
        ] {
            ensure!(
                matches!(std::fs::symlink_metadata(directory.join(name)), Err(error) if error.kind()==std::io::ErrorKind::NotFound),
                "mixed historical Current and catalog evidence"
            );
        }
    }
    Ok(present)
}

pub(super) fn open(
    store: &FileStore,
    limits: CaptureLimits,
    until: Instant,
) -> Result<CurrentArchive> {
    ensure!(
        Instant::now() < until,
        "historical Current capture deadline exhausted"
    );
    let scope = store
        .current_owned_scope()?
        .context("historical Current owner missing")?;
    let read = store.current_read(&scope)?;
    ensure!(
        !read.has_prepared_update()?,
        "historical Current has an unfinished writer; explicit recovery required"
    );
    let token = read.token()?;
    let collection = read
        .collection_state()?
        .context("historical Current collection configuration missing")?;
    let mut snapshot = Snapshot::empty(scope.root.clone());
    snapshot.collection = scope.collection.clone();
    snapshot.generation = token.sequence;
    snapshot.storage_format = 120;
    snapshot.persistent_indexes = true;
    snapshot.roots = collection.configuration.roots.clone();
    snapshot.extractor_version = collection.configuration.extractor_version.clone();
    snapshot.embedding_model = collection
        .configuration
        .model
        .as_ref()
        .map(|model| model.pipeline_digest.to_string());
    let mut after = None;
    let mut bytes = 0u64;
    loop {
        ensure!(
            Instant::now() < until,
            "historical Current capture deadline exhausted"
        );
        let page = read.lookup_paths(Lookup::Files, "", after.as_deref(), 256)?;
        for path in page.paths {
            let artifact = read
                .artifact(&path)?
                .context("historical Current inventory artifact missing")?;
            bytes = bytes
                .checked_add(serde_json::to_vec(&artifact)?.len() as u64)
                .context("historical Current inventory overflow")?;
            ensure!(
                bytes <= limits.max_snapshot_bytes && snapshot.artifacts.len() < limits.max_files,
                "historical Current inventory exceeds capture bounds"
            );
            // Authenticate retained file facts/projections too; full extraction
            // objects are independently verified by archive.artifact().
            let _ = read.file_metadata(store, &path)?;
            ensure!(
                snapshot.artifacts.insert(path, artifact).is_none(),
                "duplicate historical Current artifact"
            );
        }
        after = page.next_after;
        if after.is_none() {
            break;
        }
    }
    ensure!(
        snapshot.artifacts.len() as u64 == collection.counts.files,
        "historical Current artifact census differs"
    );
    let mut resources = BTreeMap::new();
    for key in KEYS {
        ensure!(
            Instant::now() < until,
            "historical Current resource deadline exhausted"
        );
        if let Some(reference) = read.resource_reference(key)? {
            bytes = bytes
                .checked_add(reference.bytes as u64)
                .context("historical Current resource overflow")?;
            ensure!(
                bytes <= limits.max_snapshot_bytes,
                "historical Current resources exceed capture bounds"
            );
            let _ = store.read_current_resource(&reference)?;
            resources.insert(key.into(), reference);
        }
    }
    if let Some(analysis) = read.analysis(store)? {
        snapshot.analysis = Some(analysis.manifest);
        snapshot.imported_facts = analysis.facts;
        snapshot.imported_relationships = analysis.relationships;
    }
    ensure!(
        serde_json::to_vec(&snapshot)?.len() as u64 <= limits.max_snapshot_bytes,
        "historical Current projection exceeds capture bounds"
    );
    let original = OriginalCurrentPublication { token,head:read.head().clone(),collection,resources,
        resource_conversion:"retained_original_archive; only validated analysis and artifact evidence are converted; graph, model, policies and other resource execution are not adopted".into() };
    original.validate()?;
    let value = CurrentArchive {
        original,
        snapshot,
        read,
    };
    value.revalidate()?;
    Ok(value)
}
