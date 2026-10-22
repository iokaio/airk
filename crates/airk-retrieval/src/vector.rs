// SPDX-License-Identifier: Apache-2.0
//! Microsoft DiskANN algorithms with AIRK's portable, bounded file provider.
//! Graphs and full precision vectors remain on disk during construction and search.
use anyhow::{Context, Result, ensure};
use diskann::{
    ANNError, ANNResult,
    graph::{AdjacencyList, DiskANNIndex, config, glue, search, workingset},
    provider::{self, HasId},
};
use diskann_vector::DistanceFunction;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
};

const HEADER: u64 = 32;
const DEGREE: usize = 48;
const MAX_DIMENSIONS: usize = 2048;
const MAX_VECTOR_READS: usize = 65_536;
pub(crate) const MAX_SCOPED_VECTOR_READS: usize = 4096;
const MAGIC: &[u8; 8] = b"AIRKANN1";

/// Authenticated mutable-storage foundation; normal indexing still uses `DiskIndex`.
pub mod mutable;

/// A deliberately provisioned local word-vector model. No network or hashed fallback.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelManifest {
    pub schema: u32,
    pub name: String,
    pub license: String,
    pub sha256: String,
    pub dimensions: usize,
}

pub struct LocalModel {
    pub manifest: ModelManifest,
    vocabulary: BTreeMap<String, Vec<f32>>,
}

impl LocalModel {
    /// `model.json` declares the digest of `vocabulary.json` in this directory.
    /// Vocabulary is a JSON map of lowercase tokens to finite pretrained vectors.
    pub fn open(directory: &Path) -> Result<Self> {
        Self::open_assets(&crate::model::DirectoryAssets(directory))
    }
    pub(crate) fn open_assets(
        source: &dyn airk_core::model_objects::ModelAssetSource,
    ) -> Result<Self> {
        use airk_core::model_objects::ModelAssetName;
        let manifest: ModelManifest =
            serde_json::from_slice(&source.read_asset(ModelAssetName::ModelJson, 16_384)?)
                .context("local embeddings not ready: missing/invalid model manifest")?;
        ensure!(
            manifest.schema == 1 && (1..=MAX_DIMENSIONS).contains(&manifest.dimensions),
            "unsupported local embedding specification"
        );
        ensure!(
            !manifest.name.trim().is_empty() && !manifest.license.trim().is_empty(),
            "model name and license required"
        );
        let bytes = source.read_asset(ModelAssetName::VocabularyJson, 268_435_456)?;
        ensure!(
            format!("{:x}", Sha256::digest(&bytes)) == manifest.sha256,
            "local model digest mismatch"
        );
        let vocabulary: BTreeMap<String, Vec<f32>> = serde_json::from_slice(&bytes)?;
        ensure!(
            !vocabulary.is_empty() && vocabulary.len() <= 1_000_000,
            "model vocabulary size invalid"
        );
        for (word, vector) in &vocabulary {
            ensure!(
                !word.is_empty() && word.len() <= 1024 && *word == word.to_lowercase(),
                "invalid model token"
            );
            validate_vector(vector, manifest.dimensions)?;
        }
        Ok(Self {
            manifest,
            vocabulary,
        })
    }

    /// Mean word vectors followed by L2 normalization; unknown words are skipped.
    /// None means there was no model evidence, never a fabricated fallback vector.
    pub fn embed_text(&self, text: &str) -> Result<Option<Vec<f32>>> {
        let mut sum = vec![0.0f64; self.manifest.dimensions];
        let mut count = 0u64;
        for word in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
            if let Some(vector) = self.vocabulary.get(&word.to_lowercase()) {
                for (value, component) in sum.iter_mut().zip(vector) {
                    *value += f64::from(*component);
                }
                count += 1;
            }
        }
        if count == 0 {
            return Ok(None);
        }
        let norm = sum.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm == 0.0 {
            return Ok(None);
        }
        let vector: Vec<f32> = sum.into_iter().map(|v| (v / norm) as f32).collect();
        validate_vector(&vector, self.manifest.dimensions)?;
        Ok(Some(vector))
    }
}

impl airk_core::EmbeddingProvider for LocalModel {
    fn specification(&self) -> &str {
        &self.manifest.sha256
    }
    fn embed(&self, text: &[String]) -> Result<Vec<Vec<f32>>> {
        text.iter()
            .map(|text| {
                self.embed_text(text)?
                    .context("local embedding not ready: no recognized model tokens")
            })
            .collect()
    }
}

fn validate_vector(vector: &[f32], dimensions: usize) -> Result<()> {
    ensure!(
        vector.len() == dimensions && vector.iter().all(|x| x.is_finite() && x.abs() <= 1.0e10),
        "invalid embedding dimensions or components"
    );
    Ok(())
}

fn ann(error: impl std::fmt::Display) -> ANNError {
    ANNError::message(error.to_string())
}

pub struct DiskIndex {
    inner: DiskANNIndex<FileProvider>,
}

/// Application file-read requests, not physical disk I/O or OS page-cache misses.
#[derive(Debug, Serialize)]
pub struct IoStats {
    pub record_reads: u64,
    pub requested_bytes: u64,
}

impl DiskIndex {
    pub fn io_stats(&self) -> IoStats {
        let provider = self.inner.provider();
        let record_reads = provider.reads.load(Ordering::Relaxed);
        IoStats {
            record_reads,
            requested_bytes: record_reads.saturating_mul(provider.record_size() as u64),
        }
    }
    pub fn len(&self) -> usize {
        self.inner.provider().count.load(Ordering::Relaxed) as usize
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn create(path: &Path, dimensions: usize) -> Result<Self> {
        Self::create_with_budget(path, dimensions, Arc::new(|_| Ok(())))
    }

    pub fn create_with_budget(
        path: &Path,
        dimensions: usize,
        charge: crate::bounded::Charge,
    ) -> Result<Self> {
        ensure!(
            (1..=MAX_DIMENSIONS).contains(&dimensions),
            "invalid vector dimensions"
        );
        charge(HEADER)?;
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        let mut header = [0u8; HEADER as usize];
        header[..8].copy_from_slice(MAGIC);
        header[8..12].copy_from_slice(&(dimensions as u32).to_le_bytes());
        header[12..16].copy_from_slice(&(DEGREE as u32).to_le_bytes());
        file.write_all(&header)?;
        let provider = FileProvider {
            file: Mutex::new(file),
            dimensions,
            count: AtomicU32::new(0),
            writable: true,
            reads: AtomicU64::new(0),
            charge,
        };
        provider.write_record(0, &vec![0.0; dimensions], &[])?;
        Ok(Self {
            inner: DiskANNIndex::new(configuration()?, provider, None),
        })
    }

    pub fn open(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let mut header = [0u8; HEADER as usize];
        file.read_exact(&mut header)?;
        ensure!(&header[..8] == MAGIC, "invalid DiskANN file header");
        let dimensions = u32::from_le_bytes(header[8..12].try_into()?) as usize;
        let degree = u32::from_le_bytes(header[12..16].try_into()?) as usize;
        let count = u32::from_le_bytes(header[16..20].try_into()?);
        ensure!(
            (1..=MAX_DIMENSIONS).contains(&dimensions) && degree == DEGREE,
            "unsupported DiskANN file specification"
        );
        let provider = FileProvider {
            file: Mutex::new(file),
            dimensions,
            count: AtomicU32::new(count),
            writable: false,
            reads: AtomicU64::new(0),
            charge: Arc::new(|_| Ok(())),
        };
        ensure!(
            provider.file.lock().map_err(ann)?.metadata()?.len()
                == HEADER + (u64::from(count) + 1) * provider.record_size() as u64,
            "truncated or oversized DiskANN graph"
        );
        provider.read_record(0)?;
        Ok(Self {
            inner: DiskANNIndex::new(configuration()?, provider, None),
        })
    }

    /// Sequential stable IDs start at one. Insertion mutates private generations only.
    pub fn insert(&mut self, vector: &[f32]) -> Result<u32> {
        let id = self
            .inner
            .provider()
            .count
            .load(Ordering::Relaxed)
            .checked_add(1)
            .context("vector id overflow")?;
        futures::executor::block_on(self.inner.insert(
            &Strategy,
            &provider::DefaultContext,
            &id,
            vector,
        ))?;
        Ok(id)
    }

    pub fn finish(&mut self) -> Result<()> {
        let provider = self.inner.provider();
        ensure!(provider.writable, "cannot modify published DiskANN index");
        let mut file = provider.file.lock().map_err(ann)?;
        file.seek(SeekFrom::Start(16))?;
        file.write_all(&provider.count.load(Ordering::Relaxed).to_le_bytes())?;
        file.sync_all()?;
        Ok(())
    }

    pub fn search(&self, query: &[f32], limit: usize) -> Result<Vec<(u32, f32)>> {
        ensure!((1..=1000).contains(&limit), "vector limit must be 1..1000");
        validate_vector(query, self.inner.provider().dimensions)?;
        let mut ids = vec![0; limit];
        let mut distances = vec![0.0; limit];
        let mut output =
            diskann::graph::search_output_buffer::IdDistance::new(&mut ids, &mut distances);
        futures::executor::block_on(self.inner.search(
            search::Knn::new((limit * 4).max(64), None)?,
            &Strategy,
            &provider::DefaultContext,
            query,
            &mut output,
        ))?;
        use diskann::graph::SearchOutputBuffer;
        let len = output.current_len();
        Ok(ids.into_iter().zip(distances).take(len).collect())
    }
    /// Exact comparisons for a bounded path-filtered subset of this same disk
    /// graph. This does not replace DiskANN construction or unfiltered ANN.
    #[cfg(test)]
    fn search_subset(
        &self,
        query: &[f32],
        ids: impl IntoIterator<Item = u32>,
        limit: usize,
    ) -> Result<Vec<(u32, f32)>> {
        ensure!((1..=1000).contains(&limit), "vector limit must be 1..1000");
        validate_vector(query, self.inner.provider().dimensions)?;
        let mut ranked = Vec::new();
        for (read, id) in ids.into_iter().enumerate() {
            ensure!(
                read < MAX_SCOPED_VECTOR_READS,
                "scoped vector-read budget exceeded"
            );
            ensure!(id != 0, "reserved vector id in scoped search");
            let record = self.inner.provider().read_record(id)?;
            ranked.push((id, L2.evaluate_similarity(query, &record.0)));
        }
        ranked.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        ranked.truncate(limit);
        Ok(ranked)
    }
}

fn configuration() -> Result<config::Config> {
    Ok(config::Builder::new(
        32,
        config::MaxDegree::Value(DEGREE),
        64,
        config::PruneKind::TriangleInequality,
    )
    .build()?)
}

struct FileProvider {
    file: Mutex<File>,
    dimensions: usize,
    count: AtomicU32,
    writable: bool,
    reads: AtomicU64,
    charge: crate::bounded::Charge,
}

impl FileProvider {
    fn record_size(&self) -> usize {
        4 * (self.dimensions + DEGREE + 1) + 32
    }

    fn read_record(&self, id: u32) -> ANNResult<(Vec<f32>, Vec<u32>)> {
        if id > self.count.load(Ordering::Relaxed) {
            return Err(ann("DiskANN node outside inventory"));
        }
        let mut bytes = vec![0; self.record_size()];
        let mut file = self.file.lock().map_err(ann)?;
        file.seek(SeekFrom::Start(
            HEADER + u64::from(id) * self.record_size() as u64,
        ))
        .map_err(ann)?;
        file.read_exact(&mut bytes).map_err(ann)?;
        self.reads.fetch_add(1, Ordering::Relaxed);
        drop(file);
        let payload_len = bytes.len() - 32;
        if Sha256::digest(&bytes[..payload_len])[..] != bytes[payload_len..] {
            return Err(ann("DiskANN node checksum mismatch"));
        }
        let mut words = bytes[..payload_len]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]));
        let vector: Vec<f32> = words
            .by_ref()
            .take(self.dimensions)
            .map(f32::from_bits)
            .collect();
        validate_vector(&vector, self.dimensions).map_err(ann)?;
        let degree = words
            .next()
            .ok_or_else(|| ann("missing adjacency length"))? as usize;
        if degree > DEGREE {
            return Err(ann("DiskANN degree exceeds record capacity"));
        }
        let neighbors: Vec<u32> = words.take(degree).collect();
        if neighbors
            .iter()
            .any(|n| *n > self.count.load(Ordering::Relaxed))
        {
            return Err(ann("DiskANN neighbor outside inventory"));
        }
        Ok((vector, neighbors))
    }

    fn write_record(&self, id: u32, vector: &[f32], neighbors: &[u32]) -> ANNResult<()> {
        if !self.writable {
            return Err(ann("published DiskANN index is read-only"));
        }
        validate_vector(vector, self.dimensions).map_err(ann)?;
        if neighbors.len() > DEGREE {
            return Err(ann("DiskANN adjacency limit exceeded"));
        }
        let mut bytes = Vec::with_capacity(self.record_size());
        for value in vector {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&(neighbors.len() as u32).to_le_bytes());
        for neighbor in neighbors {
            bytes.extend_from_slice(&neighbor.to_le_bytes());
        }
        bytes.resize(self.record_size() - 32, 0);
        let hash = Sha256::digest(&bytes);
        bytes.extend_from_slice(&hash);
        let mut file = self.file.lock().map_err(ann)?;
        let end = HEADER + (u64::from(id) + 1) * self.record_size() as u64;
        (self.charge)(end.saturating_sub(file.metadata().map_err(ann)?.len())).map_err(ann)?;
        file.seek(SeekFrom::Start(
            HEADER + u64::from(id) * self.record_size() as u64,
        ))
        .map_err(ann)?;
        file.write_all(&bytes).map_err(ann)?;
        Ok(())
    }
}

impl provider::DataProvider for FileProvider {
    type Context = provider::DefaultContext;
    type InternalId = u32;
    type ExternalId = u32;
    type Error = ANNError;
    type Guard = provider::NoopGuard<u32>;
    fn to_internal_id(&self, _: &Self::Context, id: &u32) -> ANNResult<u32> {
        if *id > self.count.load(Ordering::Relaxed) {
            return Err(ann("unknown vector ID"));
        }
        Ok(*id)
    }
    fn to_external_id(&self, context: &Self::Context, id: u32) -> ANNResult<u32> {
        self.to_internal_id(context, &id)
    }
}

impl provider::SetElement<&[f32]> for FileProvider {
    type SetError = ANNError;
    async fn set_element(
        &self,
        _: &Self::Context,
        id: &u32,
        vector: &[f32],
    ) -> ANNResult<Self::Guard> {
        if *id != self.count.load(Ordering::Relaxed) + 1 {
            return Err(ann("DiskANN insertion requires sequential IDs"));
        }
        self.write_record(*id, vector, &[])?;
        self.count.store(*id, Ordering::Relaxed);
        Ok(provider::NoopGuard::new(*id))
    }
}

struct Accessor<'a> {
    provider: &'a FileProvider,
    query: &'a [f32],
    reads: usize,
}
impl HasId for Accessor<'_> {
    type Id = u32;
}
impl Accessor<'_> {
    fn distance(&mut self, id: u32) -> ANNResult<f32> {
        self.reads += 1;
        if self.reads > MAX_VECTOR_READS {
            return Err(ann("DiskANN query exceeded vector-read budget"));
        }
        Ok(L2.evaluate_similarity(self.query, &self.provider.read_record(id)?.0))
    }
}
impl glue::SearchAccessor for Accessor<'_> {
    async fn starting_points(&self) -> ANNResult<Vec<u32>> {
        Ok(vec![0])
    }
    async fn start_point_distances<F>(&mut self, mut f: F) -> ANNResult<()>
    where
        F: FnMut(u32, f32) + Send,
    {
        f(0, self.distance(0)?);
        Ok(())
    }
    async fn expand_beam<I, P, F>(&mut self, ids: I, mut predicate: P, mut emit: F) -> ANNResult<()>
    where
        I: Iterator<Item = u32> + Send,
        P: glue::HybridPredicate<u32> + Send + Sync,
        F: FnMut(u32, f32) + Send,
    {
        for id in ids {
            for neighbor in self.provider.read_record(id)?.1 {
                if predicate.eval_mut(&neighbor) {
                    emit(neighbor, self.distance(neighbor)?);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct Strategy;
impl<'a> glue::SearchStrategy<'a, FileProvider, &'a [f32]> for Strategy {
    type SearchAccessorError = ANNError;
    type SearchAccessor = Accessor<'a>;
    fn search_accessor(
        &'a self,
        provider: &'a FileProvider,
        _: &'a provider::DefaultContext,
        query: &'a [f32],
    ) -> ANNResult<Self::SearchAccessor> {
        validate_vector(query, provider.dimensions).map_err(ann)?;
        Ok(Accessor {
            provider,
            query,
            reads: 0,
        })
    }
}
impl<'a> glue::DefaultPostProcessor<'a, FileProvider, &'a [f32]> for Strategy {
    diskann::default_post_processor!(glue::Pipeline<glue::FilterStartPoints, glue::CopyIds>);
}
impl<'a> glue::InsertStrategy<'a, FileProvider, &'a [f32]> for Strategy {
    type PruneStrategy = Self;
    fn prune_strategy(&self) -> Self {
        self.clone()
    }
}

struct Neighbors<'a>(&'a FileProvider);
impl HasId for Neighbors<'_> {
    type Id = u32;
}
impl provider::NeighborAccessor for Neighbors<'_> {
    async fn get_neighbors(
        &mut self,
        id: u32,
        neighbors: &mut AdjacencyList<u32>,
    ) -> ANNResult<()> {
        neighbors.clear();
        for neighbor in self.0.read_record(id)?.1 {
            neighbors.push(neighbor);
        }
        Ok(())
    }
}
impl provider::NeighborAccessorMut for Neighbors<'_> {
    async fn set_neighbors(&mut self, id: u32, neighbors: &[u32]) -> ANNResult<()> {
        self.0
            .write_record(id, &self.0.read_record(id)?.0, neighbors)
    }
    async fn append_vector(&mut self, id: u32, neighbors: &[u32]) -> ANNResult<()> {
        let (vector, mut current) = self.0.read_record(id)?;
        for id in neighbors {
            if !current.contains(id) {
                current.push(*id);
            }
        }
        self.0.write_record(id, &vector, &current)
    }
}

#[derive(Clone, Copy)]
struct L2;
impl DistanceFunction<&[f32], &[f32]> for L2 {
    fn evaluate_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
    }
}

struct Pruner<'a> {
    provider: &'a FileProvider,
    vectors: workingset::Map<u32, Box<[f32]>, workingset::map::Ref<[f32]>>,
}
impl HasId for Pruner<'_> {
    type Id = u32;
}
impl glue::PruneAccessor for Pruner<'_> {
    type ElementRef<'a> = &'a [f32];
    type View<'a>
        = workingset::map::View<'a, u32, Box<[f32]>, workingset::map::Ref<[f32]>>
    where
        Self: 'a;
    type Distance<'a>
        = L2
    where
        Self: 'a;
    type Neighbors<'a>
        = Neighbors<'a>
    where
        Self: 'a;
    fn neighbors(&mut self) -> Self::Neighbors<'_> {
        Neighbors(self.provider)
    }
    async fn fill<I>(&mut self, ids: I) -> ANNResult<(Self::View<'_>, L2)>
    where
        I: ExactSizeIterator<Item = u32> + Clone + Send + Sync,
    {
        let provider = self.provider;
        let view = self.vectors.fill(ids, |id| {
            provider
                .read_record(id)
                .map(|v| Some(v.0.into_boxed_slice()))
        })?;
        Ok((view, L2))
    }
}
impl glue::PruneStrategy<FileProvider> for Strategy {
    type PruneAccessor<'a> = Pruner<'a>;
    type PruneAccessorError = ANNError;
    fn prune_accessor<'a>(
        &'a self,
        provider: &'a FileProvider,
        _: &'a provider::DefaultContext,
        capacity: usize,
    ) -> ANNResult<Pruner<'a>> {
        Ok(Pruner {
            provider,
            vectors: workingset::map::Builder::new(workingset::map::Capacity::Default)
                .build(capacity),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_subset_bounds_record_reads_and_uses_stable_ties() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut graph = DiskIndex::create(&directory.path().join("subset.bin"), 2)?;
        for vector in [[1.0, 0.0], [0.0, 1.0], [0.0, 1.0]] {
            graph.insert(&vector)?;
        }
        graph.finish()?;
        let before = graph.io_stats().record_reads;
        let ranked = graph.search_subset(&[0.0, 1.0], [3, 2], 2)?;
        assert_eq!(
            ranked.iter().map(|item| item.0).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(graph.io_stats().record_reads - before, 2);
        let before = graph.io_stats().record_reads;
        assert!(
            graph
                .search_subset(&[0.0, 1.0], std::iter::repeat_n(1, 4097), 1)
                .is_err()
        );
        assert_eq!(graph.io_stats().record_reads - before, 4096);
        Ok(())
    }

    #[test]
    fn diskann_reopens_with_recall_against_exact_oracle() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("graph.bin");
        let mut graph = DiskIndex::create(&path, 8)?;
        let mut seed = 7u64;
        let mut vectors = Vec::new();
        for _ in 0..200 {
            let vector: Vec<f32> = (0..8)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    ((seed >> 40) as f32) / 16_777_216.0
                })
                .collect();
            assert_eq!(graph.insert(&vector)? as usize, vectors.len() + 1);
            vectors.push(vector);
        }
        graph.finish()?;
        drop(graph);
        let mut graph = DiskIndex::open(&path)?;
        assert_eq!(
            graph.io_stats().record_reads,
            1,
            "reopen reads only the start record, not all graph/vectors"
        );
        let mut recalled = 0;
        for offset in 0..20 {
            let query: Vec<f32> = vectors[offset * 7]
                .iter()
                .enumerate()
                .map(|(i, v)| v + (i as f32 - 3.0) * 0.011)
                .collect();
            let mut exact: Vec<_> = vectors
                .iter()
                .enumerate()
                .map(|(i, v)| (i as u32 + 1, L2.evaluate_similarity(&query, v)))
                .collect();
            exact.sort_by(|a, b| a.1.total_cmp(&b.1));
            let approximate = graph.search(&query, 10)?;
            assert_eq!(approximate.len(), 10);
            recalled += approximate
                .iter()
                .filter(|item| exact[..10].iter().any(|e| e.0 == item.0))
                .count();
        }
        assert!(
            recalled >= 180,
            "fixture recall@10 {} is below 90%",
            recalled as f32 / 200.0
        );
        assert!(
            graph.io_stats().record_reads > 200,
            "search performs actual on-demand file reads"
        );
        assert!(
            graph.insert(&[0.0; 8]).is_err(),
            "reopened generation is immutable"
        );
        Ok(())
    }

    #[test]
    fn graph_growth_is_charged_before_writing_a_new_node() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("bounded.bin");
        let used = Arc::new(AtomicU64::new(0));
        let counter = used.clone();
        let initial_bytes = HEADER + (4 * (2 + DEGREE + 1) + 32) as u64;
        let mut graph = DiskIndex::create_with_budget(
            &path,
            2,
            Arc::new(move |bytes| {
                counter
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |prior| {
                        prior
                            .checked_add(bytes)
                            .filter(|next| *next <= initial_bytes)
                    })
                    .map_err(|_| anyhow::anyhow!("fixture allocation exhausted"))?;
                Ok(())
            }),
        )?;
        assert_eq!(used.load(Ordering::SeqCst), initial_bytes);
        assert!(graph.insert(&[1.0, 0.0]).is_err());
        assert_eq!(std::fs::metadata(path)?.len(), initial_bytes);
        Ok(())
    }

    #[test]
    fn corruption_dimensions_and_nonfinite_vectors_fail_closed() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("graph.bin");
        let mut graph = DiskIndex::create(&path, 2)?;
        assert!(graph.insert(&[f32::NAN, 1.0]).is_err());
        assert!(graph.insert(&[1.0]).is_err());
        graph.insert(&[1.0, 0.0])?;
        graph.finish()?;
        drop(graph);
        let mut file = OpenOptions::new().write(true).open(&path)?;
        file.seek(SeekFrom::Start(HEADER))?;
        file.write_all(&[42])?;
        drop(file);
        assert!(DiskIndex::open(&path).is_err());
        Ok(())
    }
}
