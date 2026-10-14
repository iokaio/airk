// SPDX-License-Identifier: Apache-2.0
//! A single workspace boundary and one application service for CLI and MCP.
use airk_core::{
    Artifact, ArtifactSource, Citation, DocumentLocation, FactHit, FactsResponse, Hit, SearchQuery,
    SearchResponse, Snapshot, validate_budget,
};
pub use airk_core::{FactQuery, RelationshipQuery};
use airk_ingest::IndexReport;

pub mod coverage;
pub mod roots;
pub mod scheduler;
pub mod storage;
pub mod usage;
use airk_retrieval::{
    LexicalIndex,
    persistent::{self, PersistentIndex},
    truncate_utf8,
};
use airk_store::{CollectionUsage, FileStore};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct Kernel {
    root: PathBuf,
    collection: String,
    data_root: PathBuf,
    store: FileStore,
    cache: Arc<Mutex<Option<CachedIndex>>>,
}

struct CachedIndex {
    revision: String,
    index: CachedBackend,
}

#[cfg(windows)]
fn managed_data_path(path: PathBuf) -> Result<PathBuf> {
    // Resolve the nearest existing directory without creating managed state on
    // read-only open. Canonical Windows paths carry the extended prefix needed
    // by native dependencies when later component filenames cross MAX_PATH.
    let mut existing = std::path::absolute(path)?;
    let mut missing = Vec::new();
    loop {
        match std::fs::metadata(&existing) {
            Ok(metadata) => {
                ensure!(
                    metadata.is_dir(),
                    "data directory ancestor is not a directory"
                );
                let mut resolved = existing.canonicalize()?;
                for component in missing.into_iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    existing
                        .file_name()
                        .context("data directory has no existing ancestor")?
                        .to_os_string(),
                );
                ensure!(existing.pop(), "data directory has no existing ancestor");
            }
            Err(error) => return Err(error).context("resolve managed data directory"),
        }
    }
}

enum CachedBackend {
    Persistent(PersistentIndex),
    Legacy(LexicalIndex),
}

#[derive(Serialize)]
pub struct Status {
    pub source_roots: std::collections::BTreeMap<String, String>,
    pub generation: u64,
    pub collection: String,
    pub files: usize,
    pub chunks: usize,
    pub source_bytes: usize,
    pub extractor_version: String,
    pub exact_fact_backend: &'static str,
    pub lexical_backend: &'static str,
    pub vector_backend: &'static str,
    pub local_model_ready: bool,
    pub indexed_vectors: usize,
    pub semantic_producer: Option<String>,
    pub imported_facts: usize,
    pub imported_relationships: usize,
}

#[derive(Serialize)]
pub struct FileList {
    pub generation: u64,
    pub collection: String,
    pub paths: Vec<String>,
    pub truncated: bool,
}

#[derive(Serialize)]
pub struct FileInfo {
    pub source: airk_store::roots::SourceLocation,
    pub generation: u64,
    pub collection: String,
    pub path: String,
    pub content_hash: String,
    pub source_bytes: usize,
    pub language: String,
    pub extraction_method: String,
    pub parse_status: String,
    pub chunks: usize,
    pub facts: usize,
    pub indexed_lines: usize,
    pub line_basis: &'static str,
}

#[derive(Serialize)]
pub struct ReferencesResponse {
    pub resolution: &'static str,
    pub evidence: SearchResponse,
}

impl Kernel {
    pub fn open(root: impl AsRef<Path>, data_dir: Option<PathBuf>) -> Result<Self> {
        Self::open_collection(root, data_dir, "default")
    }

    pub fn open_collection(
        root: impl AsRef<Path>,
        data_dir: Option<PathBuf>,
        collection: &str,
    ) -> Result<Self> {
        airk_store::validate_collection(collection)?;
        let root = root
            .as_ref()
            .canonicalize()
            .context("resolve workspace root")?;
        ensure!(
            root.to_str().is_some(),
            "non-UTF-8 workspace root is unsupported; collection identities must be lossless"
        );
        ensure!(root.is_dir(), "workspace root must be a directory");
        let directory = data_dir.unwrap_or_else(|| root.join(".airk"));
        #[cfg(windows)]
        let directory = managed_data_path(directory)?;
        airk_store::relocation::check_root(&directory)?;
        ensure!(
            directory != root,
            "data directory must not be the workspace root"
        );
        if directory.exists() {
            ensure!(
                !root.starts_with(directory.canonicalize()?),
                "data directory must not contain the workspace root"
            );
        }
        Ok(Self {
            root,
            collection: collection.into(),
            data_root: directory.clone(),
            store: FileStore::new(directory.join("collections").join(collection)),
            cache: Arc::new(Mutex::new(None)),
        })
    }

    fn root_id(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn data_dir(&self) -> &Path {
        &self.data_root
    }
    pub fn collection(&self) -> &str {
        &self.collection
    }
    pub fn collections(&self) -> Result<Vec<CollectionUsage>> {
        airk_store::list_collections(&self.data_root)
    }
    pub fn remove_collection(&self, id: &str) -> Result<CollectionUsage> {
        if id == self.collection {
            *self
                .cache
                .lock()
                .map_err(|_| anyhow::anyhow!("index cache poisoned"))? = None;
        }
        airk_store::remove_collection(&self.data_root, id)
    }
    pub fn prune_indexes(&self, dry_run: bool) -> Result<airk_store::retention::PruneReport> {
        let _lock = self.store.lock()?;
        let snapshot = self.load()?;
        ensure!(
            snapshot.persistent_indexes,
            "reindex before pruning legacy state"
        );
        let active = persistent::revision_id(&snapshot)?;
        airk_store::retention::prune(self.store.directory(), &active, dry_run)
    }
    fn empty_snapshot(&self) -> Snapshot {
        let mut snapshot = Snapshot::empty(self.root_id());
        snapshot.collection = self.collection.clone();
        snapshot.persistent_indexes = true;
        snapshot.storage_format = 1;
        snapshot
    }

    fn load_optional(&self) -> Result<Option<Snapshot>> {
        let snapshot = self.store.catalog()?.map(|catalog| catalog.snapshot);
        if let Some(snapshot) = &snapshot {
            ensure!(
                snapshot.root == self.root_id(),
                "index belongs to another root; use a separate data directory"
            );
            ensure!(
                snapshot.collection == self.collection,
                "snapshot belongs to another collection"
            );
        }
        Ok(snapshot)
    }

    fn load(&self) -> Result<Snapshot> {
        self.load_optional()?
            .context("workspace is not indexed; run airk index first")
    }

    fn catalog(&self) -> Result<airk_store::blobs::Catalog> {
        let catalog = self
            .store
            .catalog()?
            .context("workspace is not indexed; run airk index first")?;
        ensure!(
            catalog.snapshot.root == self.root_id()
                && catalog.snapshot.collection == self.collection,
            "index belongs to another root or collection"
        );
        Ok(catalog)
    }

    pub fn initialize(&self) -> Result<Status> {
        let _lock = self.store.lock()?;
        ensure!(
            !self.root.starts_with(self.data_root.canonicalize()?),
            "data directory must not contain the workspace root"
        );
        if self.load_optional()?.is_none() {
            let mut snapshot = self.empty_snapshot();
            snapshot.embedding_model = self.configured_model()?;
            let reservation = self.publication_reservation()?;
            self.publish(&snapshot, &reservation)?;
        }
        self.status()
    }

    pub fn index(&self) -> Result<IndexReport> {
        self.index_with_mode(false)
    }

    /// Re-extract all sources without removing the last successful snapshot.
    pub fn reindex(&self) -> Result<IndexReport> {
        self.index_with_mode(true)
    }

    fn index_with_mode(&self, full: bool) -> Result<IndexReport> {
        let _lock = self.store.lock()?;
        ensure!(
            !self.root.starts_with(self.data_root.canonicalize()?),
            "data directory must not contain the workspace root"
        );
        let mut previous = self
            .load_optional()?
            .unwrap_or_else(|| self.empty_snapshot());
        if full {
            previous.extractor_version.clear();
        }
        let desired_model = self.configured_model()?;
        let roots = self.configured_roots()?;
        let reservation = self.publication_reservation()?;
        let scan_started = std::time::Instant::now();
        let (mut snapshot, mut report) = airk_ingest::scan_streaming(
            &self.root,
            &self.data_root,
            &previous,
            &roots,
            |artifact, bytes| {
                self.store
                    .retain_artifact_with_budget(artifact, Some(bytes), Some(&reservation))
            },
        )?;
        report.scan_extract_us =
            u64::try_from(scan_started.elapsed().as_micros()).unwrap_or(u64::MAX);
        snapshot.persistent_indexes = true;
        snapshot.storage_format = 1;
        snapshot.embedding_model = desired_model;
        if (snapshot.embedding_model != previous.embedding_model || previous.storage_format != 1)
            && snapshot.generation == previous.generation
        {
            snapshot.generation = snapshot
                .generation
                .checked_add(1)
                .context("generation overflow")?;
            report.generation = snapshot.generation;
        }
        let publish_started = std::time::Instant::now();
        if snapshot.generation != previous.generation
            || !self.store.directory().join("index.sqlite3").exists()
            || self.store.persistent_revision()?.is_none()
            || !persistent::component_directory(self.store.directory(), &snapshot)?.exists()
        {
            self.publish(&snapshot, &reservation)?;
        } else {
            // Reuse must not report success for a corrupt retained component.
            PersistentIndex::open(self.store.directory(), &snapshot)?;
        }
        report.publish_us =
            u64::try_from(publish_started.elapsed().as_micros()).unwrap_or(u64::MAX);
        *self
            .cache
            .lock()
            .map_err(|_| anyhow::anyhow!("index cache poisoned"))? = None;
        Ok(report)
    }

    pub fn status(&self) -> Result<Status> {
        if let Some(revision) = self.persistent_revision()? {
            let (revision, index) = self.pin_revision(revision)?;
            return Ok(Status {
                source_roots: revision
                    .roots
                    .clone()
                    .into_iter()
                    .chain([(String::new(), revision.root.clone())])
                    .collect(),
                generation: revision.generation,
                collection: revision.collection,
                files: revision.files,
                chunks: revision.chunks,
                source_bytes: revision.source_bytes,
                extractor_version: revision.extractor_version,
                exact_fact_backend: "sqlite",
                lexical_backend: "tantivy",
                vector_backend: "microsoft-diskann-0.59.0/airk-file-provider-1",
                local_model_ready: index.model_sha256.is_some(),
                indexed_vectors: index.vectors,
                semantic_producer: revision.semantic_producer,
                imported_facts: revision.imported_facts,
                imported_relationships: revision.imported_relationships,
            });
        }
        let snapshot = self.load()?;
        let persistent = if snapshot.persistent_indexes {
            Some(PersistentIndex::open(self.store.directory(), &snapshot)?)
        } else {
            None
        };
        Ok(Status {
            source_roots: snapshot
                .roots
                .clone()
                .into_iter()
                .chain([(String::new(), snapshot.root.clone())])
                .collect(),
            generation: snapshot.generation,
            collection: snapshot.collection,
            files: snapshot.artifacts.len(),
            chunks: snapshot.artifacts.values().map(Artifact::chunk_count).sum(),
            source_bytes: snapshot.artifacts.values().map(|a| a.bytes).sum(),
            extractor_version: snapshot.extractor_version,
            exact_fact_backend: if self.store.directory().join("index.sqlite3").exists() {
                "sqlite"
            } else {
                "legacy-json"
            },
            lexical_backend: if persistent.is_some() {
                "tantivy"
            } else {
                "legacy-memory"
            },
            vector_backend: "microsoft-diskann-0.59.0/airk-file-provider-1",
            local_model_ready: persistent
                .as_ref()
                .is_some_and(|index| index.model_sha256.is_some()),
            indexed_vectors: persistent.as_ref().map_or(0, |index| index.vectors),
            semantic_producer: snapshot.analysis.as_ref().map(|m| m.producer.clone()),
            imported_facts: snapshot.imported_facts.len(),
            imported_relationships: snapshot.imported_relationships.len(),
        })
    }

    pub fn search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        query.validate()?;
        if let Some(revision) = self.persistent_revision()? {
            let mut cache = self
                .cache
                .lock()
                .map_err(|_| anyhow::anyhow!("index cache poisoned"))?;
            if cache
                .as_ref()
                .is_none_or(|cached| cached.revision != revision.revision)
            {
                let (revision, index) = self.pin_revision(revision)?;
                *cache = Some(CachedIndex {
                    revision: revision.revision.clone(),
                    index: CachedBackend::Persistent(index),
                });
            }
            if let CachedBackend::Persistent(index) =
                &cache.as_ref().context("index cache unavailable")?.index
            {
                return index.search(query);
            }
            anyhow::bail!("persistent cache identity mismatch");
        }
        let snapshot = self.load()?;
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| anyhow::anyhow!("index cache poisoned"))?;
        let revision = persistent::revision_id(&snapshot)?;
        if cache
            .as_ref()
            .is_none_or(|cached| cached.revision != revision)
        {
            *cache = Some(CachedIndex {
                revision,
                index: if snapshot.persistent_indexes {
                    CachedBackend::Persistent(PersistentIndex::open(
                        self.store.directory(),
                        &snapshot,
                    )?)
                } else {
                    CachedBackend::Legacy(LexicalIndex::build(&snapshot))
                },
            });
        }
        match &cache.as_ref().context("index cache unavailable")?.index {
            CachedBackend::Persistent(index) => index.search(query),
            CachedBackend::Legacy(index) => index.search(&snapshot, query),
        }
    }

    pub fn semantic_search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        query.validate()?;
        let revision = self
            .persistent_revision()?
            .context("semantic retrieval not ready: reindex with a provisioned local model")?;
        self.pin_revision(revision)
            .context("semantic retrieval not ready: reindex with a provisioned local model")?
            .1
            .semantic_search(query)
    }

    pub fn hybrid_search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        query.validate()?;
        let revision = self
            .persistent_revision()?
            .context("hybrid retrieval not ready: reindex with a provisioned local model")?;
        self.pin_revision(revision)?.1.hybrid_search(query)
    }

    /// A writer may publish and prune between the metadata read and lease open.
    /// Retry only when the active identity changed; stable corruption fails closed.
    fn pin_revision(
        &self,
        mut revision: airk_store::PersistentRevision,
    ) -> Result<(airk_store::PersistentRevision, PersistentIndex)> {
        for _ in 0..3 {
            match PersistentIndex::open_revision(
                self.store.directory(),
                &revision.revision,
                revision.generation,
                &revision.collection,
            ) {
                Ok(index) => return Ok((revision, index)),
                Err(error) => {
                    let Some(current) = self.persistent_revision()? else {
                        return Err(error);
                    };
                    if current.revision == revision.revision {
                        return Err(error);
                    }
                    revision = current;
                }
            }
        }
        anyhow::bail!(
            "active generation changed repeatedly while acquiring reader lease; retry request"
        )
    }

    fn configured_model(&self) -> Result<Option<String>> {
        let directory = self.data_root.join("models/local");
        if directory.exists() {
            Ok(Some(
                airk_retrieval::vector::LocalModel::open(&directory)?
                    .manifest
                    .sha256,
            ))
        } else {
            Ok(None)
        }
    }

    fn persistent_revision(&self) -> Result<Option<airk_store::PersistentRevision>> {
        let revision = self.store.persistent_revision()?;
        if let Some(revision) = &revision {
            ensure!(
                revision.root == self.root_id() && revision.collection == self.collection,
                "index belongs to another root or collection"
            );
        }
        Ok(revision)
    }

    fn publication_reservation(&self) -> Result<airk_store::quota::Reservation> {
        let quota = airk_store::quota::Quota::open(&self.data_root)?;
        let policy = quota.policy()?;
        let status = quota.status()?;
        let root_available = status
            .root_limit_bytes
            .saturating_sub(status.used_bytes)
            .saturating_sub(status.reserved_bytes);
        let collection_available = status.collections.get(&self.collection).map_or_else(
            || {
                policy
                    .collections
                    .get(&self.collection)
                    .copied()
                    .unwrap_or(policy.default_collection_bytes)
            },
            |item| {
                item.limit_bytes
                    .saturating_sub(item.used_bytes)
                    .saturating_sub(item.reserved_bytes)
            },
        );
        // The reservation includes temporary and retained bytes together. Each
        // provider charges before writes; exhausting it leaves the active pin intact.
        quota.reserve(
            &self.collection,
            root_available
                .min(collection_available)
                .min(4 * 1024 * 1024 * 1024),
        )
    }

    fn publish(
        &self,
        snapshot: &Snapshot,
        reservation: &airk_store::quota::Reservation,
    ) -> Result<()> {
        ensure!(
            snapshot.persistent_indexes,
            "published revisions must declare persistent components"
        );
        let model = self.data_root.join("models").join("local");
        ensure!(
            snapshot.storage_format == 1,
            "new publications require compact retained inventories"
        );
        let source = airk_store::blobs::RetainedSource {
            store: &self.store,
            snapshot,
        };
        let revision = persistent::revision_id(snapshot)?;
        let staging = reservation.staging_directory()?;
        let charge_reservation = reservation.clone();
        let charge: airk_retrieval::bounded::Charge =
            Arc::new(move |bytes| charge_reservation.charge(bytes));
        let inventory_bytes = serde_json::to_vec(snapshot)?.len() as u64;
        let artifact_bytes = snapshot
            .artifacts
            .values()
            .filter_map(|a| a.retained.as_ref())
            .try_fold(0u64, |sum, retained| {
                sum.checked_add(retained.object_bytes as u64)
                    .context("catalog byte estimate overflow")
            })?;
        let database_budget = artifact_bytes
            .saturating_mul(3)
            .saturating_add(inventory_bytes.saturating_mul(3))
            .saturating_add(1024 * 1024)
            .min(1024 * 1024 * 1024)
            .min(reservation.remaining() / 2);
        persistent::prepare_streaming(
            self.store.directory(),
            snapshot,
            model.exists().then_some(model.as_path()),
            &source,
            Some(charge),
            Some(&staging),
            |directory| {
                self.store.prepare_catalog(
                    directory,
                    snapshot,
                    &revision,
                    &source,
                    Some(reservation),
                    database_budget,
                )
            },
        )?;
        self.store
            .publish_catalog(snapshot, &revision, Some(reservation))
    }

    pub fn list_files(&self, prefix: &str, limit: usize, max_bytes: usize) -> Result<FileList> {
        validate_budget(limit, max_bytes)?;
        ensure!(prefix.len() <= 4096, "prefix exceeds 4096 bytes");
        let snapshot = self.load()?;
        let mut response = FileList {
            generation: snapshot.generation,
            collection: snapshot.collection.clone(),
            paths: Vec::new(),
            truncated: false,
        };
        let mut remaining = max_bytes;
        for path in snapshot.artifacts.keys().filter(|p| p.starts_with(prefix)) {
            if response.paths.len() >= limit || path.len() > remaining {
                response.truncated = true;
                break;
            }
            remaining -= path.len();
            response.paths.push(path.clone());
        }
        Ok(response)
    }

    pub fn read_file(
        &self,
        path: &str,
        start_line: usize,
        end_line: usize,
        max_bytes: usize,
    ) -> Result<SearchResponse> {
        let catalog = self.catalog()?;
        let artifact = catalog.load_artifact(path)?;
        Self::read_snapshot(
            &catalog.snapshot,
            &artifact,
            path,
            start_line,
            end_line,
            max_bytes,
        )
    }

    fn read_snapshot(
        snapshot: &Snapshot,
        artifact: &Artifact,
        path: &str,
        start_line: usize,
        end_line: usize,
        max_bytes: usize,
    ) -> Result<SearchResponse> {
        validate_budget(1, max_bytes)?;
        ensure!(
            start_line > 0 && end_line >= start_line,
            "invalid line range"
        );
        // Lookup only in inventory: caller paths never become filesystem reads.
        let last_line = artifact.chunks.last().map_or(0, |c| c.end_line);
        ensure!(end_line <= last_line, "line range exceeds indexed source");
        let locations: Vec<_> = artifact
            .chunks
            .iter()
            .filter(|c| c.end_line >= start_line && c.start_line <= end_line)
            .map(|c| &c.location)
            .collect();
        let location = if artifact.extraction_method == "utf8" {
            DocumentLocation::Lines {
                start: start_line,
                end: end_line,
            }
        } else {
            let location = locations.first().context("document location unavailable")?;
            ensure!(
                locations.iter().all(|other| *other == *location),
                "range crosses document locations; request one page/block/slide at a time"
            );
            (*location).clone()
        };
        let mut remaining = max_bytes;
        let mut text = String::new();
        let mut truncated = false;
        for chunk in &artifact.chunks {
            for (offset, line) in chunk.text.split_inclusive('\n').enumerate() {
                let number = chunk.start_line + offset;
                if number < start_line || number > end_line {
                    continue;
                }
                let (part, cut) = truncate_utf8(line, remaining);
                remaining -= part.len();
                text.push_str(&part);
                if cut {
                    truncated = true;
                    break;
                }
            }
            if truncated {
                break;
            }
        }
        Ok(SearchResponse {
            generation: snapshot.generation,
            collection: snapshot.collection.clone(),
            truncated,
            hits: vec![Hit {
                citation: Citation {
                    path: path.into(),
                    content_hash: artifact.content_hash.clone(),
                    start_line,
                    end_line,
                    location,
                },
                text,
                truncated,
            }],
        })
    }

    pub fn file_info(&self, path: &str) -> Result<FileInfo> {
        let catalog = self.catalog()?;
        let snapshot = &catalog.snapshot;
        let a = catalog.load_artifact(path)?;
        Ok(FileInfo {
            source: airk_store::roots::source_location(&snapshot.root, &snapshot.roots, path)?,
            generation: snapshot.generation,
            collection: snapshot.collection.clone(),
            path: a.path.clone(),
            content_hash: a.content_hash.clone(),
            source_bytes: a.bytes,
            language: a.language.clone(),
            extraction_method: a.extraction_method.clone(),
            parse_status: a.parse_status.clone(),
            chunks: a.chunks.len(),
            facts: a.facts.len(),
            indexed_lines: a.chunks.last().map_or(0, |c| c.end_line),
            line_basis: if a.extraction_method == "utf8" {
                "source"
            } else {
                "extracted-text; cite original location"
            },
        })
    }

    pub fn facts(&self, query: &FactQuery) -> Result<FactsResponse> {
        validate_budget(query.limit, query.max_bytes)?;
        ensure!(
            query.path_prefix.len() <= 4096
                && query.name.as_ref().is_none_or(|n| n.len() <= 1024)
                && query.kind.as_ref().is_none_or(|kind| kind.len() <= 128)
                && query
                    .language
                    .as_ref()
                    .is_none_or(|language| language.len() <= 128),
            "fact filter too long"
        );
        if let Some(response) = self
            .store
            .facts(&self.root_id(), &self.collection, query, false)?
        {
            return Ok(response);
        }
        let snapshot = self.load()?;
        let mut response = FactsResponse {
            generation: snapshot.generation,
            collection: snapshot.collection.clone(),
            facts: Vec::new(),
            truncated: false,
        };
        let mut remaining = query.max_bytes;
        'files: for (path, a) in &snapshot.artifacts {
            if (if query.path_exact {
                path != &query.path_prefix
            } else {
                !path.starts_with(&query.path_prefix)
            }) || query.language.as_ref().is_some_and(|l| l != &a.language)
            {
                continue;
            }
            for fact in &a.facts {
                if query.document_only
                    && (fact.code.is_some()
                        || matches!(fact.kind.as_str(), "definition" | "import"))
                {
                    continue;
                }
                if query.kind.as_ref().is_some_and(|k| k != &fact.kind) {
                    continue;
                }
                if query.name.as_ref().is_some_and(|n| {
                    if query.exact_name {
                        n != &fact.name
                    } else {
                        !fact.name.to_lowercase().contains(&n.to_lowercase())
                    }
                }) {
                    continue;
                }
                let bytes = fact.name.len()
                    + path.len()
                    + fact
                        .document
                        .as_ref()
                        .map(serde_json::to_vec)
                        .transpose()?
                        .map_or(0, |value| value.len())
                    + fact
                        .code
                        .as_ref()
                        .map(serde_json::to_vec)
                        .transpose()?
                        .map_or(0, |v| v.len());
                if response.facts.len() == query.limit || bytes > remaining {
                    response.truncated = true;
                    break 'files;
                }
                remaining -= bytes;
                response.facts.push(FactHit {
                    name: fact.name.clone(),
                    kind: fact.kind.clone(),
                    syntax_kind: fact.syntax_kind.clone(),
                    language: a.language.clone(),
                    document: fact.document.clone(),
                    code: fact.code.clone(),
                    citation: Citation {
                        path: path.clone(),
                        content_hash: a.content_hash.clone(),
                        start_line: fact.start_line,
                        end_line: fact.end_line,
                        location: fact.location.clone(),
                    },
                });
            }
        }
        Ok(response)
    }

    pub fn code_facts(&self, query: &FactQuery) -> Result<FactsResponse> {
        validate_budget(query.limit, query.max_bytes)?;
        ensure!(
            query.path_prefix.len() <= 4096
                && query.name.as_ref().is_none_or(|n| n.len() <= 1024)
                && query.kind.as_ref().is_none_or(|k| k.len() <= 128)
                && query.language.as_ref().is_none_or(|l| l.len() <= 128),
            "code fact filter too long"
        );
        self.store
            .facts(&self.root_id(), &self.collection, query, true)?
            .context("run airk index to migrate legacy facts to SQLite")
    }

    pub fn relationships(
        &self,
        query: &RelationshipQuery,
    ) -> Result<airk_core::RelationshipsResponse> {
        validate_budget(query.limit, query.max_bytes)?;
        ensure!(
            !query.symbol.is_empty()
                && query.symbol.len() <= 2048
                && query.path_prefix.len() <= 4096
                && query.kind.as_ref().is_none_or(|k| k.len() <= 128),
            "relationship filter invalid/too long"
        );
        ensure!(
            matches!(query.direction.as_str(), "incoming" | "outgoing"),
            "direction must be incoming or outgoing"
        );
        ensure!((1..=3).contains(&query.hops), "hops must be 1..3");
        self.store
            .relationships(&self.root_id(), &self.collection, query)
    }

    /// Prepare binding before the external producer runs; outputs JSON through CLI.
    fn verify_source_inventory(&self, snapshot: &Snapshot) -> Result<Snapshot> {
        let (current, _) = airk_ingest::scan_streaming(
            &self.root,
            &self.data_root,
            snapshot,
            &snapshot.roots,
            |artifact, _| {
                // Verification needs identities, never a second in-memory corpus.
                let mut metadata = artifact.clone();
                metadata.chunks.clear();
                metadata.facts.clear();
                Ok(metadata)
            },
        )?;
        Ok(current)
    }

    pub fn prepare_analysis(
        &self,
        producer: &str,
        version: &str,
        build_inputs: &[String],
    ) -> Result<airk_core::AnalysisManifest> {
        let _lock = self.store.lock()?;
        let snapshot = self.load()?;
        ensure!(
            snapshot.roots.is_empty(),
            "SCIP imports require a single primary root; additional source aliases are not supported by this adapter"
        );
        let current = self.verify_source_inventory(&snapshot)?;
        ensure!(
            airk_ingest::semantic::source_inventory(&snapshot)
                == airk_ingest::semantic::source_inventory(&current)
                && snapshot.extractor_version == airk_core::EXTRACTOR_VERSION,
            "reindex before preparing analysis"
        );
        airk_ingest::semantic::prepare(&self.root, &snapshot, producer, version, build_inputs)
    }

    pub fn import_scip(&self, index: &Path, manifest: &Path) -> Result<Status> {
        let _lock = self.store.lock()?;
        let catalog = self.catalog()?;
        let mut snapshot = catalog.snapshot.clone();
        ensure!(
            snapshot.roots.is_empty(),
            "SCIP imports require a single primary root; additional source aliases are not supported by this adapter"
        );
        let binding: airk_core::AnalysisManifest =
            serde_json::from_slice(&airk_ingest::semantic::read_bounded(manifest, 8_388_608)?)?;
        airk_ingest::semantic::validate_build_inputs(&self.root, &binding)?;
        let current = self.verify_source_inventory(&snapshot)?;
        ensure!(
            airk_ingest::semantic::source_inventory(&snapshot)
                == airk_ingest::semantic::source_inventory(&current)
                && snapshot.extractor_version == airk_core::EXTRACTOR_VERSION,
            "source changed; reindex and regenerate analysis"
        );
        let bytes =
            airk_ingest::semantic::read_bounded(index, airk_ingest::semantic::MAX_IMPORT_BYTES)?;
        airk_ingest::semantic::import_with_source(
            &mut snapshot,
            binding.clone(),
            &bytes,
            &catalog,
        )?;
        airk_ingest::semantic::validate_build_inputs(&self.root, &binding)?;
        snapshot.generation = snapshot
            .generation
            .checked_add(1)
            .context("generation overflow")?;
        snapshot.persistent_indexes = true;
        let reservation = self.publication_reservation()?;
        for artifact in snapshot.artifacts.values_mut() {
            *artifact =
                self.store
                    .retain_artifact_with_budget(artifact, None, Some(&reservation))?;
        }
        snapshot.storage_format = 1;
        snapshot.embedding_model = self.configured_model()?;
        self.publish(&snapshot, &reservation)?;
        self.status()
    }

    pub fn outline(&self, path: &str, limit: usize, max_bytes: usize) -> Result<FactsResponse> {
        self.file_info(path)?;
        // Full exact file lookup, not a prefix that could include neighboring files.
        self.facts(&FactQuery {
            path_prefix: path.into(),
            name: None,
            kind: None,
            language: None,
            exact_name: false,
            limit,
            max_bytes,
            path_exact: true,
            document_only: false,
        })
    }

    pub fn context(
        &self,
        path: &str,
        line: usize,
        before: usize,
        after: usize,
        max_bytes: usize,
    ) -> Result<SearchResponse> {
        ensure!(
            before <= 100 && after <= 100,
            "context radius must be 0..100"
        );
        let catalog = self.catalog()?;
        let snapshot = &catalog.snapshot;
        let artifact = catalog.load_artifact(path)?;
        let indexed_lines = artifact.chunks.last().map_or(0, |chunk| chunk.end_line);
        ensure!(
            line > 0 && line <= indexed_lines,
            "line is outside indexed source"
        );
        Self::read_snapshot(
            snapshot,
            &artifact,
            path,
            line.saturating_sub(before).max(1),
            line.saturating_add(after).min(indexed_lines),
            max_bytes,
        )
    }

    pub fn references(
        &self,
        name: &str,
        path_prefix: &str,
        limit: usize,
        max_bytes: usize,
    ) -> Result<ReferencesResponse> {
        validate_budget(limit, max_bytes)?;
        ensure!(
            !name.is_empty()
                && name.len() <= 1024
                && path_prefix.len() <= 4096
                && name.chars().all(|c| c.is_alphanumeric() || c == '_'),
            "reference name must be a single identifier"
        );
        let catalog = self.catalog()?;
        let snapshot = &catalog.snapshot;
        let mut evidence = SearchResponse {
            generation: snapshot.generation,
            collection: snapshot.collection.clone(),
            hits: Vec::new(),
            truncated: false,
        };
        let mut remaining = max_bytes;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
        let mut read_bytes = 0usize;
        'files: for (path, entry) in &snapshot.artifacts {
            if !path.starts_with(path_prefix) {
                continue;
            }
            read_bytes = read_bytes.saturating_add(entry.extracted_bytes());
            if read_bytes > 16_777_216 || std::time::Instant::now() >= deadline {
                evidence.truncated = true;
                break;
            }
            let a = catalog.load_artifact(path)?;
            for chunk in &a.chunks {
                for (offset, line) in chunk.text.split_inclusive('\n').enumerate() {
                    if !line
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .any(|word| word == name)
                    {
                        continue;
                    }
                    if evidence.hits.len() == limit || remaining == 0 {
                        evidence.truncated = true;
                        break 'files;
                    }
                    let (text, truncated) = truncate_utf8(line, remaining);
                    remaining -= text.len();
                    evidence.truncated |= truncated;
                    let number = chunk.start_line + offset;
                    let location = if a.extraction_method == "utf8" {
                        DocumentLocation::Lines {
                            start: number,
                            end: number,
                        }
                    } else {
                        chunk.location.clone()
                    };
                    evidence.hits.push(Hit {
                        text,
                        truncated,
                        citation: Citation {
                            path: path.clone(),
                            content_hash: a.content_hash.clone(),
                            start_line: number,
                            end_line: number,
                            location,
                        },
                    });
                }
            }
        }
        Ok(ReferencesResponse {
            resolution: "textual-occurrences; includes comments, strings and definitions; no name resolution",
            evidence,
        })
    }
}
