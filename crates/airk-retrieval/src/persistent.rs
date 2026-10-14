// SPDX-License-Identifier: Apache-2.0
//! Immutable Tantivy/DiskANN components, selected by the complete revision digest.
use crate::{
    bounded::{BoundedDirectory, BudgetWriter, Charge},
    model::EmbeddingModel,
    terms, truncate_utf8,
    vector::DiskIndex,
};
use airk_core::{ArtifactSource, Citation, Hit, SearchQuery, SearchResponse, Snapshot};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use tantivy::{
    Index, IndexReader, Order, ReloadPolicy, TantivyDocument, Term,
    collector::TopDocs,
    query::{BooleanQuery, Occur, Query, RegexQuery, TermQuery},
    schema::{FAST, Field, INDEXED, IndexRecordOption, STORED, STRING, Schema, Value},
};

const ENGINE: &str = "tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-3";

fn read_manifest(directory: &Path) -> Result<ReadManifest> {
    let file = File::open(directory.join("manifest.json"))?;
    ensure!(file.metadata()?.len() <= 16_384, "oversized index manifest");
    let mut bytes = Vec::new();
    file.take(16_385).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 16_384, "oversized index manifest");
    Ok(serde_json::from_slice(&bytes)?)
}

#[derive(Serialize)]
pub struct Manifest {
    pub schema: u32,
    pub engine: String,
    pub revision: String,
    pub generation: u64,
    pub collection: String,
    pub chunks: usize,
    pub vectors: usize,
    pub model_sha256: Option<String>,
    pub embedding: &'static str,
}

// An explicit owned representation keeps manifests readable without static lifetimes.
#[derive(Deserialize)]
struct ReadManifest {
    schema: u32,
    engine: String,
    revision: String,
    generation: u64,
    collection: String,
    chunks: usize,
    vectors: usize,
    model_sha256: Option<String>,
}

/// Includes parser/chunker output and source binding; imported analysis creates a new
/// component identity too. A reused numerical generation cannot pair stale citations.
pub fn revision_id(snapshot: &Snapshot) -> Result<String> {
    struct HashWriter(Sha256);
    impl Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut hash = HashWriter(Sha256::new());
    hash.write_all(ENGINE.as_bytes())?;
    serde_json::to_writer(&mut hash, snapshot)?;
    Ok(format!("{:x}", hash.0.finalize()))
}

pub fn component_directory(base: &Path, snapshot: &Snapshot) -> Result<PathBuf> {
    Ok(base.join("generations").join(revision_id(snapshot)?))
}

fn schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let fields = Fields {
        terms: builder.add_text_field("terms", STRING),
        path: builder.add_text_field("path", STRING),
        ordinal: builder.add_u64_field("ordinal", FAST),
        vector: builder.add_u64_field("vector", INDEXED),
        payload: builder.add_text_field("payload", STORED),
    };
    (builder.build(), fields)
}

fn reader_lease_path(base: &Path, revision: &str) -> Result<PathBuf> {
    let root = base
        .parent()
        .context("collection parent missing")?
        .join(".reader-locks");
    let collection = root.join(base.file_name().context("collection name missing")?);
    let lease = collection.join(revision);
    for path in [&root, &collection, &lease] {
        if let Ok(metadata) = fs::symlink_metadata(path) {
            ensure!(
                !metadata.file_type().is_symlink(),
                "linked generation lease state"
            );
        }
    }
    Ok(lease)
}

struct Fields {
    terms: Field,
    path: Field,
    ordinal: Field,
    vector: Field,
    payload: Field,
}

/// Build and reopen every component before publishing an immutable directory. The
/// caller publishes SQLite's revision only after this succeeds; orphan builds are safe.
pub fn prepare(base: &Path, snapshot: &Snapshot, model_directory: Option<&Path>) -> Result<()> {
    prepare_streaming(
        base,
        snapshot,
        model_directory,
        snapshot,
        None,
        None,
        |_| Ok(()),
    )
}

pub fn prepare_streaming(
    base: &Path,
    snapshot: &Snapshot,
    model_directory: Option<&Path>,
    source: &dyn ArtifactSource,
    charge: Option<Charge>,
    staging_parent: Option<&Path>,
    prepare_catalog: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let target = component_directory(base, snapshot)?;
    if target.exists() {
        PersistentIndex::open(base, snapshot)?;
        return Ok(());
    }
    let generations = base.join("generations");
    fs::create_dir_all(&generations)?;
    let charge = charge.unwrap_or_else(|| std::sync::Arc::new(|_| Ok(())));
    let staging = tempfile::Builder::new()
        .prefix("building-")
        .tempdir_in(staging_parent.unwrap_or(&generations))?;
    let lexical = staging.path().join("lexical");
    fs::create_dir(&lexical)?;
    let (schema, fields) = schema();
    let index = Index::create(
        BoundedDirectory::new(
            tantivy::directory::MmapDirectory::open(&lexical)?,
            charge.clone(),
        ),
        schema,
        tantivy::IndexSettings::default(),
    )?;
    let mut writer = index.writer_with_num_threads::<TantivyDocument>(1, 20_000_000)?;
    let model = model_directory.map(EmbeddingModel::open).transpose()?;
    ensure!(
        snapshot.embedding_model.as_deref() == model.as_ref().map(EmbeddingModel::identity),
        "model identity changed while preparing components"
    );
    let mut graph = model
        .as_ref()
        .map(|model| {
            DiskIndex::create_with_budget(
                &staging.path().join("vectors.bin"),
                model.dimensions(),
                charge.clone(),
            )
        })
        .transpose()?;
    let mut ordinal = 0u64;
    let mut vectors = 0;
    for path in snapshot.artifacts.keys() {
        let artifact = source.load_artifact(path)?;
        for chunk in &artifact.chunks {
            let mut document = TantivyDocument::default();
            for term in terms(&format!("{path} {}", chunk.text)) {
                document.add_text(fields.terms, term);
            }
            document.add_text(fields.path, path);
            document.add_u64(fields.ordinal, ordinal);
            if let (Some(model), Some(graph)) = (&model, &mut graph)
                && let Some(vector) = model.embed_text(&chunk.text)?
            {
                document.add_u64(fields.vector, u64::from(graph.insert(&vector)?));
                vectors += 1;
            }
            let hit = Hit {
                citation: Citation {
                    path: path.clone(),
                    content_hash: artifact.content_hash.clone(),
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    location: chunk.location.clone(),
                },
                text: chunk.text.clone(),
                truncated: false,
            };
            document.add_text(fields.payload, serde_json::to_string(&hit)?);
            writer.add_document(document)?;
            ordinal += 1;
        }
    }
    writer.commit()?;
    writer.wait_merging_threads()?;
    drop(index);
    if let Some(mut graph) = graph {
        graph.finish()?;
        drop(graph);
        DiskIndex::open(&staging.path().join("vectors.bin"))?;
    }
    if let Some(model_directory) = model_directory {
        fs::create_dir(staging.path().join("model"))?;
        for name in model
            .as_ref()
            .context("model adapter missing")?
            .asset_names()
        {
            let destination = staging.path().join("model").join(name);
            let mut input = File::open(model_directory.join(name))?;
            let mut output = BudgetWriter::new(File::create(destination)?, charge.clone());
            std::io::copy(&mut input, &mut output)?;
            output.get_ref().sync_all()?;
        }
        let copied = crate::model::inspect(&staging.path().join("model"))?;
        ensure!(
            Some(copied.identity.as_str()) == model.as_ref().map(EmbeddingModel::identity),
            "model changed while building revision"
        );
    }
    let manifest = Manifest {
        schema: 1,
        engine: ENGINE.into(),
        revision: revision_id(snapshot)?,
        generation: snapshot.generation,
        collection: snapshot.collection.clone(),
        chunks: ordinal as usize,
        vectors,
        model_sha256: model.as_ref().map(|m| m.identity().to_owned()),
        embedding: model.as_ref().map_or("none", EmbeddingModel::specification),
    };
    let mut inventory = BudgetWriter::new(
        File::create(staging.path().join("inventory.json"))?,
        charge.clone(),
    );
    serde_json::to_writer(&mut inventory, snapshot)?;
    inventory.get_ref().sync_all()?;
    drop(inventory);
    let manifest_bytes = serde_json::to_vec(&manifest)?;
    ensure!(manifest_bytes.len() <= 16_384, "oversized index manifest");
    let mut file = BudgetWriter::new(
        File::create(staging.path().join("manifest.json"))?,
        charge.clone(),
    );
    file.write_all(&manifest_bytes)?;
    file.get_ref().sync_all()?;
    drop(file);
    // The catalog can consume the remaining generation quota: every other
    // nonempty component has already been written and charged.
    prepare_catalog(staging.path())?;
    // Windows cannot rename directories containing open OS lease handles.
    let lease_path = reader_lease_path(base, &manifest.revision)?;
    fs::create_dir_all(lease_path.parent().context("lease parent missing")?)?;
    let lease = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lease_path)?;
    lease.sync_all()?;
    drop(lease);
    // Reopen Tantivy before publication to detect incomplete commits.
    let probe: IndexReader = Index::open_in_dir(&lexical)?
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()?;
    drop(probe);
    let directory = staging.keep();
    fs::rename(&directory, &target).context("publish immutable index components")?;
    Ok(())
}

pub struct PersistentIndex {
    model: Mutex<Option<EmbeddingModel>>,
    _lease: File,
    reader: IndexReader,
    fields: Fields,
    directory: PathBuf,
    generation: u64,
    collection: String,
    pub vectors: usize,
    pub model_sha256: Option<String>,
}

impl PersistentIndex {
    pub fn open(base: &Path, snapshot: &Snapshot) -> Result<Self> {
        Self::open_revision(
            base,
            &revision_id(snapshot)?,
            snapshot.generation,
            &snapshot.collection,
        )
    }

    pub fn open_revision(
        base: &Path,
        revision: &str,
        generation: u64,
        collection: &str,
    ) -> Result<Self> {
        ensure!(
            revision.len() == 64 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "invalid component identity"
        );
        let directory = base.join("generations").join(revision);
        let lease = File::open(reader_lease_path(base, revision)?)?;
        lease.lock_shared()?;
        let manifest = read_manifest(&directory)?;
        ensure!(
            manifest.schema == 1
                && manifest.engine == ENGINE
                && manifest.revision == revision
                && manifest.generation == generation
                && manifest.collection == collection,
            "persistent index revision mismatch"
        );
        let (expected, fields) = schema();
        let index = Index::open_in_dir(directory.join("lexical"))?;
        ensure!(index.schema() == expected, "unsupported lexical schema");
        let reader: IndexReader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        ensure!(
            reader.searcher().num_docs() == manifest.chunks as u64,
            "lexical inventory mismatch"
        );
        ensure!(
            manifest.vectors <= manifest.chunks,
            "vector inventory exceeds lexical inventory"
        );
        if manifest.model_sha256.is_some() {
            ensure!(
                DiskIndex::open(&directory.join("vectors.bin"))?.len() == manifest.vectors,
                "vector inventory mismatch"
            );
        } else {
            ensure!(manifest.vectors == 0, "vectors are missing a model binding");
        }
        Ok(Self {
            model: Mutex::new(None),
            _lease: lease,
            reader,
            fields,
            directory,
            generation,
            collection: collection.into(),
            vectors: manifest.vectors,
            model_sha256: manifest.model_sha256,
        })
    }

    pub fn search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        query.validate()?;
        self.verify_retained_manifest()?;
        let query_terms = terms(&query.text);
        ensure!(
            !query_terms.is_empty(),
            "query must contain a letter or number"
        );
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = query_terms
            .into_iter()
            .map(|term| {
                (
                    Occur::Must,
                    Box::new(TermQuery::new(
                        Term::from_field_text(self.fields.terms, &term),
                        IndexRecordOption::Basic,
                    )) as Box<dyn Query>,
                )
            })
            .collect();
        if let Some(prefix) = &query.path_prefix {
            let escaped: String = prefix
                .chars()
                .flat_map(|ch| {
                    if ".+*?()|[]{}^$\\".contains(ch) {
                        vec!['\\', ch]
                    } else {
                        vec![ch]
                    }
                })
                .collect();
            clauses.push((
                Occur::Must,
                Box::new(RegexQuery::from_pattern(
                    &format!("(?s:{escaped}.*)"),
                    self.fields.path,
                )?),
            ));
        }
        let searcher = self.reader.searcher();
        let candidates = searcher.search(
            &BooleanQuery::new(clauses),
            &TopDocs::with_limit(query.limit + 1).order_by_u64_field("ordinal", Order::Asc),
        )?;
        let hits = candidates
            .into_iter()
            .map(|(_, address)| self.read_hit(address))
            .collect::<Result<Vec<_>>>()?;
        Ok(self.bound(hits, query))
    }

    pub fn semantic_search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        query.validate()?;
        self.verify_retained_manifest()?;
        ensure!(
            self.model_sha256.is_some(),
            "semantic retrieval not ready: provision a local model and reindex"
        );
        let mut cached = self
            .model
            .lock()
            .map_err(|_| anyhow::anyhow!("local model cache poisoned"))?;
        if cached.is_none() {
            *cached = Some(EmbeddingModel::open(&self.directory.join("model"))?);
        }
        let model = cached.as_ref().context("local model unavailable")?;
        ensure!(
            Some(model.identity()) == self.model_sha256.as_deref(),
            "generation model identity mismatch"
        );
        let vector = model
            .embed_text(&query.text)?
            .context("semantic retrieval not ready: no recognized model tokens")?;
        drop(cached);
        let graph = DiskIndex::open(&self.directory.join("vectors.bin"))?;
        let candidates = graph.search(&vector, (query.limit * 4).clamp(64, 1000))?;
        let incomplete = candidates.len() < self.vectors;
        let mut hits = Vec::new();
        for (id, _) in candidates {
            let matches = self.reader.searcher().search(
                &TermQuery::new(
                    Term::from_field_u64(self.fields.vector, u64::from(id)),
                    IndexRecordOption::Basic,
                ),
                &TopDocs::with_limit(2).order_by_u64_field("ordinal", Order::Asc),
            )?;
            ensure!(matches.len() == 1, "vector/lexical identity mismatch");
            let hit = self.read_hit(matches[0].1)?;
            if query
                .path_prefix
                .as_ref()
                .is_none_or(|prefix| hit.citation.path.starts_with(prefix))
            {
                hits.push(hit);
            }
        }
        let mut response = self.bound(hits, query);
        response.truncated |= incomplete;
        Ok(response)
    }

    /// Reciprocal-rank fusion of lexical and approximate vector candidates, both
    /// pinned by this generation handle. Missing model state is an explicit error.
    pub fn hybrid_search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        query.validate()?;
        let candidates = SearchQuery {
            text: query.text.clone(),
            path_prefix: query.path_prefix.clone(),
            limit: 100,
            max_bytes: airk_core::MAX_RESPONSE_BYTES,
        };
        let lexical = self.search(&candidates)?;
        let semantic = self.semantic_search(&candidates)?;
        let incomplete = lexical.truncated || semantic.truncated;
        let mut merged = std::collections::BTreeMap::<(String, usize, usize), (f64, Hit)>::new();
        for response in [lexical, semantic] {
            for (rank, hit) in response.hits.into_iter().enumerate() {
                let key = (
                    hit.citation.path.clone(),
                    hit.citation.start_line,
                    hit.citation.end_line,
                );
                let score = 1.0 / (61.0 + rank as f64);
                if let Some((current, existing)) = merged.get_mut(&key) {
                    *current += score;
                    if existing.truncated && !hit.truncated {
                        *existing = hit;
                    }
                } else {
                    merged.insert(key, (score, hit));
                }
            }
        }
        let mut ranked: Vec<_> = merged.into_values().collect();
        ranked.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then_with(|| a.1.citation.path.cmp(&b.1.citation.path))
                .then_with(|| a.1.citation.start_line.cmp(&b.1.citation.start_line))
        });
        let mut response = self.bound(ranked.into_iter().map(|(_, hit)| hit).collect(), query);
        response.truncated |= incomplete;
        Ok(response)
    }

    // Cached/mapped handles can outlive a removed or replaced directory. Keep
    // failure reporting consistent with fresh readers without reloading a model.
    fn verify_retained_manifest(&self) -> Result<()> {
        let manifest = read_manifest(&self.directory)?;
        ensure!(
            manifest.schema == 1
                && manifest.engine == ENGINE
                && Some(manifest.revision.as_str())
                    == self.directory.file_name().and_then(|name| name.to_str())
                && manifest.generation == self.generation
                && manifest.collection == self.collection
                && manifest.chunks as u64 == self.reader.searcher().num_docs()
                && manifest.vectors == self.vectors
                && manifest.model_sha256 == self.model_sha256,
            "cached generation manifest identity mismatch"
        );
        Ok(())
    }

    fn read_hit(&self, address: tantivy::DocAddress) -> Result<Hit> {
        let document: TantivyDocument = self.reader.searcher().doc(address)?;
        let payload = document
            .get_first(self.fields.payload)
            .and_then(|value| value.as_str())
            .context("missing lexical citation")?;
        Ok(serde_json::from_str(payload)?)
    }

    fn bound(&self, hits: Vec<Hit>, query: &SearchQuery) -> SearchResponse {
        let mut response = SearchResponse {
            generation: self.generation,
            collection: self.collection.clone(),
            hits: Vec::new(),
            truncated: false,
        };
        let mut remaining = query.max_bytes;
        for mut hit in hits {
            if remaining == 0 || response.hits.len() == query.limit {
                response.truncated = true;
                break;
            }
            let (text, truncated) = truncate_utf8(&hit.text, remaining);
            hit.text = text;
            hit.truncated |= truncated;
            remaining -= hit.text.len();
            response.truncated |= hit.truncated;
            response.hits.push(hit);
        }
        response
    }
}
