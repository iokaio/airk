// SPDX-License-Identifier: Apache-2.0
//! A single workspace boundary and one application service for CLI and MCP.
use airk_core::{
    Citation, DocumentLocation, FactHit, FactsResponse, Hit, SearchQuery, SearchResponse, Snapshot,
    SnapshotStore, validate_budget,
};
pub use airk_core::{FactQuery, RelationshipQuery};
use airk_ingest::IndexReport;

pub mod usage;
use airk_retrieval::{LexicalIndex, truncate_utf8};
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
    generation: u64,
    extractor_version: String,
    inventory: Vec<(String, String)>,
    index: LexicalIndex,
}

#[derive(Serialize)]
pub struct Status {
    pub generation: u64,
    pub collection: String,
    pub files: usize,
    pub chunks: usize,
    pub source_bytes: usize,
    pub extractor_version: String,
    pub exact_fact_backend: &'static str,
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
        ensure!(root.is_dir(), "workspace root must be a directory");
        let directory = data_dir.unwrap_or_else(|| root.join(".airk"));
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
        airk_store::remove_collection(&self.data_root, id)
    }
    fn empty_snapshot(&self) -> Snapshot {
        let mut snapshot = Snapshot::empty(self.root_id());
        snapshot.collection = self.collection.clone();
        snapshot
    }

    fn load_optional(&self) -> Result<Option<Snapshot>> {
        let snapshot = self.store.load()?;
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

    pub fn initialize(&self) -> Result<Status> {
        let _lock = self.store.lock()?;
        ensure!(
            !self.root.starts_with(self.data_root.canonicalize()?),
            "data directory must not contain the workspace root"
        );
        if self.load_optional()?.is_none() {
            self.store.save(&self.empty_snapshot())?;
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
        let (snapshot, report) = airk_ingest::scan(&self.root, &self.data_root, &previous)?;
        if snapshot.generation != previous.generation
            || !self.store.directory().join("index.sqlite3").exists()
        {
            self.store.save(&snapshot)?;
        }
        *self
            .cache
            .lock()
            .map_err(|_| anyhow::anyhow!("index cache poisoned"))? = None;
        Ok(report)
    }

    pub fn status(&self) -> Result<Status> {
        let snapshot = self.load()?;
        Ok(Status {
            generation: snapshot.generation,
            collection: snapshot.collection,
            files: snapshot.artifacts.len(),
            chunks: snapshot.artifacts.values().map(|a| a.chunks.len()).sum(),
            source_bytes: snapshot.artifacts.values().map(|a| a.bytes).sum(),
            extractor_version: snapshot.extractor_version,
            exact_fact_backend: if self.store.directory().join("index.sqlite3").exists() {
                "sqlite"
            } else {
                "legacy-json"
            },
            semantic_producer: snapshot.analysis.as_ref().map(|m| m.producer.clone()),
            imported_facts: snapshot.imported_facts.len(),
            imported_relationships: snapshot.imported_relationships.len(),
        })
    }

    pub fn search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        query.validate()?;
        let snapshot = self.load()?;
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| anyhow::anyhow!("index cache poisoned"))?;
        if cache.as_ref().is_none_or(|cached| {
            cached.generation != snapshot.generation
                || cached.extractor_version != snapshot.extractor_version
                || cached.inventory.len() != snapshot.artifacts.len()
                || cached
                    .inventory
                    .iter()
                    .zip(&snapshot.artifacts)
                    .any(|((path, hash), (current, a))| path != current || hash != &a.content_hash)
        }) {
            *cache = Some(CachedIndex {
                generation: snapshot.generation,
                extractor_version: snapshot.extractor_version.clone(),
                inventory: snapshot
                    .artifacts
                    .iter()
                    .map(|(path, a)| (path.clone(), a.content_hash.clone()))
                    .collect(),
                index: LexicalIndex::build(&snapshot),
            });
        }
        cache
            .as_ref()
            .context("index cache unavailable")?
            .index
            .search(&snapshot, query)
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
        validate_budget(1, max_bytes)?;
        ensure!(
            start_line > 0 && end_line >= start_line,
            "invalid line range"
        );
        let snapshot = self.load()?;
        // Lookup only in inventory: caller paths never become filesystem reads.
        let artifact = snapshot
            .artifacts
            .get(path)
            .context("path is not in indexed inventory")?;
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
            collection: snapshot.collection,
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
        let snapshot = self.load()?;
        let a = snapshot
            .artifacts
            .get(path)
            .context("path is not in indexed inventory")?;
        Ok(FileInfo {
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
            query.path_prefix.len() <= 4096 && query.name.as_ref().is_none_or(|n| n.len() <= 1024),
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
    pub fn prepare_analysis(
        &self,
        producer: &str,
        version: &str,
        build_inputs: &[String],
    ) -> Result<airk_core::AnalysisManifest> {
        let _lock = self.store.lock()?;
        let snapshot = self.load()?;
        let (current, _) = airk_ingest::scan(&self.root, &self.data_root, &snapshot)?;
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
        let mut snapshot = self.load()?;
        let binding: airk_core::AnalysisManifest =
            serde_json::from_slice(&airk_ingest::semantic::read_bounded(manifest, 8_388_608)?)?;
        airk_ingest::semantic::validate_build_inputs(&self.root, &binding)?;
        let (current, _) = airk_ingest::scan(&self.root, &self.data_root, &snapshot)?;
        ensure!(
            airk_ingest::semantic::source_inventory(&snapshot)
                == airk_ingest::semantic::source_inventory(&current)
                && snapshot.extractor_version == airk_core::EXTRACTOR_VERSION,
            "source changed; reindex and regenerate analysis"
        );
        let bytes =
            airk_ingest::semantic::read_bounded(index, airk_ingest::semantic::MAX_IMPORT_BYTES)?;
        airk_ingest::semantic::import(&mut snapshot, binding.clone(), &bytes)?;
        airk_ingest::semantic::validate_build_inputs(&self.root, &binding)?;
        snapshot.generation = snapshot
            .generation
            .checked_add(1)
            .context("generation overflow")?;
        self.store.save(&snapshot)?;
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
        let info = self.file_info(path)?;
        ensure!(
            line > 0 && line <= info.indexed_lines,
            "line is outside indexed source"
        );
        self.read_file(
            path,
            line.saturating_sub(before).max(1),
            line.saturating_add(after).min(info.indexed_lines),
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
                && name.chars().all(|c| c.is_alphanumeric() || c == '_'),
            "reference name must be a single identifier"
        );
        let snapshot = self.load()?;
        let mut evidence = SearchResponse {
            generation: snapshot.generation,
            collection: snapshot.collection.clone(),
            hits: Vec::new(),
            truncated: false,
        };
        let mut remaining = max_bytes;
        'files: for (path, a) in &snapshot.artifacts {
            if !path.starts_with(path_prefix) {
                continue;
            }
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
