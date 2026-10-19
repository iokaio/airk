// SPDX-License-Identifier: Apache-2.0
//! DiskANN over authenticated committed records and a private after-image overlay.
//! This foundation is not selected by the normal indexing/search runtime yet.
use super::{L2, MAX_VECTOR_READS, ann, configuration, validate_vector};
use airk_core::{
    graph_storage::{GraphCounts, GraphRecord, GraphRecordOverlay, GraphRecordReader},
    identity::Sha256Digest,
};
use anyhow::{Context, Result, ensure};
use diskann::{
    ANNError, ANNResult,
    graph::{AdjacencyList, DiskANNIndex, SearchOutputBuffer, glue, search, workingset},
    provider::{self, HasId},
};
use diskann_vector::DistanceFunction;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};

mod codec;
pub use codec::{record_bytes, specification};

/// Bounds candidate/refill memory independently of the record-read budget.
pub const MAX_SEARCH_CANDIDATES: usize = 4096;

/// A single budget covers all distance, adjacency and live-filter reads and refill passes.
#[derive(Clone, Copy, Debug)]
pub struct SearchLimits {
    pub record_reads: usize,
    pub candidates: usize,
}

impl Default for SearchLimits {
    fn default() -> Self {
        Self {
            record_reads: MAX_VECTOR_READS,
            candidates: MAX_SEARCH_CANDIDATES,
        }
    }
}

#[derive(Debug)]
pub struct SearchResult {
    pub hits: Vec<(u32, f32)>,
    /// The bounded search could not fill min(requested, live_nodes).
    pub partial: bool,
    pub record_reads: u64,
    pub passes: u32,
}

/// Counts application record requests, not storage proof reads or physical device I/O.
#[derive(Clone, Copy, Debug)]
pub struct IoStatistics {
    pub record_reads: u64,
    pub record_writes: u64,
}

pub fn initial_record(dimensions: usize) -> Result<GraphRecord> {
    record_bytes(dimensions)?;
    Ok(GraphRecord {
        embedding_key: None,
        live: false,
        bytes: codec::encode(dimensions, 1, 0, &vec![0.0; dimensions], &[])?,
    })
}

/// The reader/overlay owns the request's read lease. Drop this index before sealing
/// the overlay and acquiring a publication write gate; never retain it in an idle cache.
pub struct MutableDiskIndex {
    inner: DiskANNIndex<RecordProvider>,
}

impl MutableDiskIndex {
    pub fn create(
        overlay: Arc<dyn GraphRecordOverlay>,
        dimensions: usize,
        embedding_pipeline: Sha256Digest,
    ) -> Result<Self> {
        let result = (|| {
            validate_scope(overlay.as_ref(), dimensions, embedding_pipeline)?;
            ensure!(
                overlay.counts()?.allocated_count == 0,
                "graph already allocated"
            );
            overlay.append(0, initial_record(dimensions)?)?;
            Self::open_overlay(overlay.clone(), dimensions, embedding_pipeline)
        })();
        if result.is_err() {
            overlay.poison();
        }
        result
    }

    pub fn open(
        reader: Arc<dyn GraphRecordReader>,
        dimensions: usize,
        embedding_pipeline: Sha256Digest,
    ) -> Result<Self> {
        Self::from_backend(Backend::Committed(reader), dimensions, embedding_pipeline)
    }

    pub fn open_overlay(
        overlay: Arc<dyn GraphRecordOverlay>,
        dimensions: usize,
        embedding_pipeline: Sha256Digest,
    ) -> Result<Self> {
        Self::from_backend(Backend::Candidate(overlay), dimensions, embedding_pipeline)
    }

    fn from_backend(
        backend: Backend,
        dimensions: usize,
        embedding_pipeline: Sha256Digest,
    ) -> Result<Self> {
        let result = (|| {
            validate_scope(backend.reader(), dimensions, embedding_pipeline)?;
            let counts = backend.reader().counts()?;
            counts.validate()?;
            ensure!(
                counts.allocated_count != 0,
                "graph has no reserved search seed"
            );
            Ok(())
        })();
        if let Err(error) = result {
            backend.poison();
            return Err(error);
        }
        let provider = RecordProvider {
            backend,
            dimensions,
            reads: AtomicU64::new(0),
            writes: AtomicU64::new(0),
        };
        provider.checked(provider.read_record(0, &OperationContext::new(1)))?;
        Ok(Self {
            inner: DiskANNIndex::new(configuration()?, provider, None),
        })
    }

    pub fn counts(&self) -> Result<GraphCounts> {
        let provider = self.inner.provider();
        provider.checked(provider.counts())
    }

    pub fn io_statistics(&self) -> IoStatistics {
        let provider = self.inner.provider();
        IoStatistics {
            record_reads: provider.reads.load(Ordering::Relaxed),
            record_writes: provider.writes.load(Ordering::Relaxed),
        }
    }

    /// Appends one never-reused physical ID through the actual DiskANN insertion algorithm.
    /// On any failure the whole private candidate is poisoned and cannot be sealed.
    pub fn insert(&mut self, key: Sha256Digest, vector: &[f32]) -> Result<u32> {
        let provider = self.inner.provider();
        provider.attempt(|| {
            provider.backend.overlay()?;
            validate_vector(vector, provider.dimensions)?;
            let id = u32::try_from(provider.counts()?.allocated_count)
                .context("mutable DiskANN physical IDs exhausted")?;
            let context = OperationContext::new(MAX_VECTOR_READS);
            futures::executor::block_on(self.inner.insert(
                &Strategy,
                &context,
                &id,
                Input { key, vector },
            ))?;
            Ok(id)
        })
    }

    /// Tombstones or resurrects the same logical key without changing graph edges or IDs.
    pub fn set_live(&mut self, id: u32, expected_key: Sha256Digest, live: bool) -> Result<()> {
        let provider = self.inner.provider();
        provider.attempt(|| {
            let overlay = provider.backend.overlay()?;
            ensure!(
                id != 0,
                "reserved DiskANN seed cannot be made live or deleted"
            );
            let (record, _) = provider.read_record(id, &OperationContext::new(1))?;
            ensure!(
                record.embedding_key == Some(expected_key),
                "mutable DiskANN embedding key mismatch"
            );
            overlay.set_live(id, live)?;
            provider.writes.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
    }

    pub fn search(&self, query: &[f32], limit: usize) -> Result<SearchResult> {
        self.search_with_limits(query, limit, SearchLimits::default())
    }

    /// Rank an authenticated, bounded path subset using this same disk graph's
    /// distances. Filtering global ANN neighbors would miss an otherwise valid
    /// nearest result in a narrow scope. IDs must be sorted and unique; tombstones
    /// or a reserved ID indicate a stale/corrupt caller mapping, not an empty hit.
    pub fn search_subset(&self, query: &[f32], ids: &[u32], limit: usize) -> Result<SearchResult> {
        let provider = self.inner.provider();
        provider.checked((|| {
            ensure!((1..=1000).contains(&limit), "vector limit must be 1..1000");
            ensure!(
                ids.len() <= super::MAX_SCOPED_VECTOR_READS
                    && ids.first().is_none_or(|id| *id != 0)
                    && ids.windows(2).all(|pair| pair[0] < pair[1]),
                "invalid current scoped vector IDs or read budget"
            );
            validate_vector(query, provider.dimensions)?;
            let context = OperationContext::new(super::MAX_SCOPED_VECTOR_READS);
            // Keep only the requested prefix, independent of the scope size.
            let mut hits: Vec<(u32, f32)> = Vec::with_capacity(limit + 1);
            for &id in ids {
                let (record, decoded) = provider.read_record(id, &context)?;
                ensure!(record.live, "current scoped vector is not live");
                let distance = L2.evaluate_similarity(query, &decoded.vector);
                ensure!(distance.is_finite(), "nonfinite current vector distance");
                let at = hits.partition_point(|&(old_id, old_distance)| {
                    old_distance
                        .total_cmp(&distance)
                        .then(old_id.cmp(&id))
                        .is_lt()
                });
                hits.insert(at, (id, distance));
                hits.truncate(limit);
            }
            Ok(SearchResult {
                hits,
                partial: false,
                record_reads: context.reads.load(Ordering::Relaxed) as u64,
                passes: u32::from(!ids.is_empty()),
            })
        })())
    }

    pub fn search_with_limits(
        &self,
        query: &[f32],
        limit: usize,
        limits: SearchLimits,
    ) -> Result<SearchResult> {
        let provider = self.inner.provider();
        let result = self.search_inner(query, limit, limits);
        provider.checked(result)
    }

    fn search_inner(
        &self,
        query: &[f32],
        limit: usize,
        limits: SearchLimits,
    ) -> Result<SearchResult> {
        ensure!((1..=1000).contains(&limit), "vector limit must be 1..1000");
        ensure!(
            (1..=MAX_VECTOR_READS).contains(&limits.record_reads)
                && (limit..=MAX_SEARCH_CANDIDATES).contains(&limits.candidates),
            "mutable DiskANN search budget outside bounds"
        );
        let provider = self.inner.provider();
        validate_vector(query, provider.dimensions)?;
        let counts = provider.counts()?;
        let wanted = limit.min(counts.live_nodes as usize);
        let context = OperationContext::new(limits.record_reads);
        let mut result = SearchResult {
            hits: Vec::new(),
            partial: wanted != 0,
            record_reads: 0,
            passes: 0,
        };
        if wanted == 0 {
            return Ok(result);
        }
        let ceiling = limits
            .candidates
            .min(usize::try_from(counts.allocated_count.saturating_sub(1)).unwrap_or(usize::MAX));
        let mut capacity = limit.max(64).min(ceiling);
        loop {
            result.passes += 1;
            let mut ids = vec![0; capacity];
            let mut distances = vec![0.0; capacity];
            let mut output =
                diskann::graph::search_output_buffer::IdDistance::new(&mut ids, &mut distances);
            let searched = futures::executor::block_on(self.inner.search(
                search::Knn::new((capacity * 4).clamp(64, MAX_SEARCH_CANDIDATES), None)?,
                &Strategy,
                &context,
                query,
                &mut output,
            ));
            let found = output.current_len();
            if let Err(error) = searched {
                if context.exhausted.load(Ordering::Relaxed) {
                    provider.backend.poison();
                    break;
                }
                return Err(error.into());
            }
            let mut live = Vec::with_capacity(wanted);
            for (id, distance) in ids.into_iter().zip(distances).take(found) {
                match provider.read_record(id, &context) {
                    Ok((record, _)) if record.live => live.push((id, distance)),
                    Ok(_) => (),
                    Err(_) if context.exhausted.load(Ordering::Relaxed) => {
                        provider.backend.poison();
                        break;
                    }
                    Err(error) => return Err(error),
                }
                if live.len() == wanted {
                    break;
                }
            }
            live.sort_by(|left, right| left.1.total_cmp(&right.1).then(left.0.cmp(&right.0)));
            if live.len() >= result.hits.len() {
                result.hits = live;
            }
            if result.hits.len() == wanted {
                result.partial = false;
                break;
            }
            if capacity == ceiling || context.exhausted.load(Ordering::Relaxed) {
                break;
            }
            capacity = (capacity * 2).min(ceiling);
        }
        result.record_reads = context.reads.load(Ordering::Relaxed) as u64;
        Ok(result)
    }
}

fn validate_scope(
    reader: &dyn GraphRecordReader,
    dimensions: usize,
    embedding_pipeline: Sha256Digest,
) -> Result<()> {
    reader.scope().validate()?;
    ensure!(
        reader.scope().record_bytes == record_bytes(dimensions)?
            && reader.scope().specification == specification(dimensions, embedding_pipeline)?,
        "mutable DiskANN storage specification mismatch"
    );
    Ok(())
}

enum Backend {
    Committed(Arc<dyn GraphRecordReader>),
    Candidate(Arc<dyn GraphRecordOverlay>),
}

impl Backend {
    fn reader(&self) -> &dyn GraphRecordReader {
        match self {
            Self::Committed(reader) => reader.as_ref(),
            Self::Candidate(overlay) => overlay.as_ref(),
        }
    }
    fn overlay(&self) -> Result<&dyn GraphRecordOverlay> {
        match self {
            Self::Committed(_) => anyhow::bail!("committed mutable DiskANN graph is read-only"),
            Self::Candidate(overlay) => Ok(overlay.as_ref()),
        }
    }
    fn poison(&self) {
        if let Self::Candidate(overlay) = self {
            overlay.poison();
        }
    }
}

struct RecordProvider {
    backend: Backend,
    dimensions: usize,
    reads: AtomicU64,
    writes: AtomicU64,
}

impl RecordProvider {
    fn attempt<T>(&self, operation: impl FnOnce() -> Result<T>) -> Result<T> {
        let mut guard = CandidateAttempt {
            backend: &self.backend,
            succeeded: false,
        };
        let result = operation();
        guard.succeeded = result.is_ok();
        result
    }
    fn checked<T>(&self, result: Result<T>) -> Result<T> {
        if result.is_err() {
            self.backend.poison();
        }
        result
    }
    fn counts(&self) -> Result<GraphCounts> {
        let counts = self.backend.reader().counts()?;
        counts.validate()?;
        Ok(counts)
    }
    fn read_record(
        &self,
        id: u32,
        context: &OperationContext,
    ) -> Result<(GraphRecord, codec::Decoded)> {
        context.consume()?;
        self.reads.fetch_add(1, Ordering::Relaxed);
        let counts = self.counts()?;
        ensure!(
            u64::from(id) < counts.allocated_count,
            "unknown mutable DiskANN ID"
        );
        let record = self.backend.reader().read(id)?;
        record.validate(self.backend.reader().scope(), id)?;
        let decoded = codec::decode(self.dimensions, counts.allocated_count, id, &record.bytes)?;
        Ok((record, decoded))
    }
    fn write_neighbors(
        &self,
        id: u32,
        neighbors: &[u32],
        context: &OperationContext,
    ) -> ANNResult<()> {
        let (mut record, decoded) = self.read_record(id, context).map_err(ann)?;
        record.bytes = codec::encode(
            self.dimensions,
            self.counts().map_err(ann)?.allocated_count,
            id,
            &decoded.vector,
            neighbors,
        )
        .map_err(ann)?;
        self.backend
            .overlay()
            .map_err(ann)?
            .put(id, record)
            .map_err(ann)?;
        self.writes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

/// A caller catching an unexpected insertion panic must not be able to seal its
/// partially written after-images. Ordinary errors take the same discard path.
struct CandidateAttempt<'a> {
    backend: &'a Backend,
    succeeded: bool,
}

impl Drop for CandidateAttempt<'_> {
    fn drop(&mut self) {
        if !self.succeeded {
            self.backend.poison();
        }
    }
}

#[derive(Clone)]
struct OperationContext {
    reads: Arc<AtomicUsize>,
    exhausted: Arc<AtomicBool>,
    limit: usize,
}

impl provider::ExecutionContext for OperationContext {}

impl OperationContext {
    fn new(limit: usize) -> Self {
        Self {
            reads: Arc::new(AtomicUsize::new(0)),
            exhausted: Arc::new(AtomicBool::new(false)),
            limit,
        }
    }
    fn consume(&self) -> Result<()> {
        if self
            .reads
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |prior| {
                (prior < self.limit).then_some(prior + 1)
            })
            .is_err()
        {
            self.exhausted.store(true, Ordering::Relaxed);
            anyhow::bail!("mutable DiskANN record-read budget exhausted");
        }
        Ok(())
    }
}

impl provider::DataProvider for RecordProvider {
    type Context = OperationContext;
    type InternalId = u32;
    type ExternalId = u32;
    type Error = ANNError;
    type Guard = provider::NoopGuard<u32>;
    fn to_internal_id(&self, _: &OperationContext, id: &u32) -> ANNResult<u32> {
        if u64::from(*id) >= self.counts().map_err(ann)?.allocated_count {
            return Err(ann("unknown mutable DiskANN ID"));
        }
        Ok(*id)
    }
    fn to_external_id(&self, context: &OperationContext, id: u32) -> ANNResult<u32> {
        self.to_internal_id(context, &id)
    }
}

#[derive(Clone, Copy)]
struct Input<'a> {
    key: Sha256Digest,
    vector: &'a [f32],
}

impl provider::SetElement<Input<'_>> for RecordProvider {
    type SetError = ANNError;
    async fn set_element(
        &self,
        _: &OperationContext,
        id: &u32,
        input: Input<'_>,
    ) -> ANNResult<Self::Guard> {
        let count = self.counts().map_err(ann)?.allocated_count;
        if u64::from(*id) != count || *id == 0 {
            return Err(ann(
                "mutable DiskANN insertion requires sequential positive IDs",
            ));
        }
        let record = GraphRecord {
            embedding_key: Some(input.key),
            live: true,
            bytes: codec::encode(self.dimensions, count + 1, *id, input.vector, &[])
                .map_err(ann)?,
        };
        self.backend
            .overlay()
            .map_err(ann)?
            .append(*id, record)
            .map_err(ann)?;
        self.writes.fetch_add(1, Ordering::Relaxed);
        Ok(provider::NoopGuard::new(*id))
    }
}

struct Accessor<'a> {
    provider: &'a RecordProvider,
    query: &'a [f32],
    context: &'a OperationContext,
}

impl HasId for Accessor<'_> {
    type Id = u32;
}

impl Accessor<'_> {
    fn distance(&self, id: u32) -> ANNResult<f32> {
        Ok(L2.evaluate_similarity(
            self.query,
            &self
                .provider
                .read_record(id, self.context)
                .map_err(ann)?
                .1
                .vector,
        ))
    }
}

impl glue::SearchAccessor for Accessor<'_> {
    async fn starting_points(&self) -> ANNResult<Vec<u32>> {
        Ok(vec![0])
    }
    async fn start_point_distances<F>(&mut self, mut emit: F) -> ANNResult<()>
    where
        F: FnMut(u32, f32) + Send,
    {
        emit(0, self.distance(0)?);
        Ok(())
    }
    async fn expand_beam<I, P, F>(&mut self, ids: I, mut predicate: P, mut emit: F) -> ANNResult<()>
    where
        I: Iterator<Item = u32> + Send,
        P: glue::HybridPredicate<u32> + Send + Sync,
        F: FnMut(u32, f32) + Send,
    {
        for id in ids {
            for neighbor in self
                .provider
                .read_record(id, self.context)
                .map_err(ann)?
                .1
                .neighbors
            {
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

impl<'a> glue::SearchStrategy<'a, RecordProvider, &'a [f32]> for Strategy {
    type SearchAccessorError = ANNError;
    type SearchAccessor = Accessor<'a>;
    fn search_accessor(
        &'a self,
        provider: &'a RecordProvider,
        context: &'a OperationContext,
        query: &'a [f32],
    ) -> ANNResult<Accessor<'a>> {
        validate_vector(query, provider.dimensions).map_err(ann)?;
        Ok(Accessor {
            provider,
            query,
            context,
        })
    }
}

impl<'a> glue::SearchStrategy<'a, RecordProvider, Input<'a>> for Strategy {
    type SearchAccessorError = ANNError;
    type SearchAccessor = Accessor<'a>;
    fn search_accessor(
        &'a self,
        provider: &'a RecordProvider,
        context: &'a OperationContext,
        input: Input<'a>,
    ) -> ANNResult<Accessor<'a>> {
        <Self as glue::SearchStrategy<'a, RecordProvider, &'a [f32]>>::search_accessor(
            self,
            provider,
            context,
            input.vector,
        )
    }
}

impl<'a> glue::DefaultPostProcessor<'a, RecordProvider, &'a [f32]> for Strategy {
    diskann::default_post_processor!(glue::Pipeline<glue::FilterStartPoints, glue::CopyIds>);
}

impl<'a> glue::InsertStrategy<'a, RecordProvider, Input<'a>> for Strategy {
    type PruneStrategy = Self;
    fn prune_strategy(&self) -> Self {
        self.clone()
    }
}

struct Neighbors<'a> {
    provider: &'a RecordProvider,
    context: &'a OperationContext,
}
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
        for neighbor in self
            .provider
            .read_record(id, self.context)
            .map_err(ann)?
            .1
            .neighbors
        {
            neighbors.push(neighbor);
        }
        Ok(())
    }
}
impl provider::NeighborAccessorMut for Neighbors<'_> {
    async fn set_neighbors(&mut self, id: u32, neighbors: &[u32]) -> ANNResult<()> {
        self.provider.write_neighbors(id, neighbors, self.context)
    }
    async fn append_vector(&mut self, id: u32, neighbors: &[u32]) -> ANNResult<()> {
        let mut current = self
            .provider
            .read_record(id, self.context)
            .map_err(ann)?
            .1
            .neighbors;
        for neighbor in neighbors {
            if !current.contains(neighbor) {
                current.push(*neighbor);
            }
        }
        self.provider.write_neighbors(id, &current, self.context)
    }
}

struct Pruner<'a> {
    provider: &'a RecordProvider,
    context: &'a OperationContext,
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
        Neighbors {
            provider: self.provider,
            context: self.context,
        }
    }
    async fn fill<I>(&mut self, ids: I) -> ANNResult<(Self::View<'_>, L2)>
    where
        I: ExactSizeIterator<Item = u32> + Clone + Send + Sync,
    {
        let provider = self.provider;
        let context = self.context;
        let view = self.vectors.fill(ids, |id| {
            provider
                .read_record(id, context)
                .map(|record| Some(record.1.vector.into_boxed_slice()))
                .map_err(ann)
        })?;
        Ok((view, L2))
    }
}
impl glue::PruneStrategy<RecordProvider> for Strategy {
    type PruneAccessor<'a> = Pruner<'a>;
    type PruneAccessorError = ANNError;
    fn prune_accessor<'a>(
        &'a self,
        provider: &'a RecordProvider,
        context: &'a OperationContext,
        capacity: usize,
    ) -> ANNResult<Pruner<'a>> {
        Ok(Pruner {
            provider,
            context,
            vectors: workingset::map::Builder::new(workingset::map::Capacity::Default)
                .build(capacity),
        })
    }
}

#[cfg(test)]
mod tests;
