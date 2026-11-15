// SPDX-License-Identifier: Apache-2.0
//! PRIVATE draft: mount as vector::mutable::self_probe, cfg(test-support) only.
//! No model API, writable overlay, insertion, or graph rebuild is used.
use super::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, time::Instant};

pub mod regeneration_cache;

/// Diagnostic-only immutable raw records, still retaining the exact snapshot reader.
/// This is not a production cache, proof substitute, decoder, metric or index.
struct AuthenticatedRecords {
    source: Arc<dyn GraphRecordReader>,
    scope: airk_core::graph_storage::GraphScope,
    counts: airk_core::graph_storage::GraphCounts,
    records: Vec<GraphRecord>,
}
impl AuthenticatedRecords {
    fn capture(
        source: Arc<dyn GraphRecordReader>,
        dimensions: usize,
        pipeline: Sha256Digest,
        deadline: Instant,
        trace: &mut dyn FnMut(&'static str, u64) -> Result<()>,
    ) -> Result<Arc<Self>> {
        validate_scope(source.as_ref(), dimensions, pipeline)?;
        let scope = source.scope().clone();
        let counts = source.counts()?;
        counts.validate()?;
        ensure!(
            counts.allocated_count > 0 && counts.allocated_count <= 4097 && counts.live_nodes > 0,
            "bounded nonempty diagnostic graph required"
        );
        trace("cache_capture_start", counts.allocated_count)?;
        let mut records = Vec::with_capacity(usize::try_from(counts.allocated_count)?);
        for id in 0..u32::try_from(counts.allocated_count)? {
            ensure!(
                Instant::now() < deadline,
                "original diagnostic deadline exceeded"
            );
            trace("authenticated_read_start", u64::from(id))?;
            let record = source.read(id)?;
            trace("authenticated_read_done", u64::from(id))?;
            record.validate(&scope, id)?;
            codec::decode(dimensions, counts.allocated_count, id, &record.bytes)?;
            trace("decode_done", u64::from(id))?;
            records.push(record);
        }
        ensure!(
            source.counts()? == counts && source.scope() == &scope,
            "source snapshot changed during cache capture"
        );
        trace("cache_capture_done", counts.allocated_count)?;
        Ok(Arc::new(Self {
            source,
            scope,
            counts,
            records,
        }))
    }
}
impl GraphRecordReader for AuthenticatedRecords {
    fn scope(&self) -> &airk_core::graph_storage::GraphScope {
        &self.scope
    }
    fn counts(&self) -> Result<airk_core::graph_storage::GraphCounts> {
        ensure!(
            self.source.counts()? == self.counts && self.source.scope() == &self.scope,
            "retained source snapshot no longer live"
        );
        Ok(self.counts)
    }
    fn read(&self, id: u32) -> Result<GraphRecord> {
        self.counts()?;
        self.records
            .get(usize::try_from(id)?)
            .cloned()
            .context("cached physical record missing")
    }
}

pub fn compare_self_vectors(
    reader: Arc<dyn GraphRecordReader>,
    dimensions: usize,
    pipeline: Sha256Digest,
    deadline: Instant,
) -> Result<Value> {
    compare_self_vectors_traced(reader, dimensions, pipeline, deadline, &mut |_, _| Ok(()))
}

pub fn compare_self_vectors_traced(
    reader: Arc<dyn GraphRecordReader>,
    dimensions: usize,
    pipeline: Sha256Digest,
    deadline: Instant,
    trace: &mut dyn FnMut(&'static str, u64) -> Result<()>,
) -> Result<Value> {
    let source_counts = reader.counts()?;
    let reader = AuthenticatedRecords::capture(reader, dimensions, pipeline, deadline, trace)?;
    trace("mutable_open_start", 0)?;
    let graph = MutableDiskIndex::open(reader.clone(), dimensions, pipeline)?;
    trace("mutable_open_done", 0)?;
    let counts = reader.counts()?;
    ensure!(
        counts.allocated_count <= 4097 && counts.live_nodes > 0,
        "bounded nonempty diagnostic graph required"
    );
    let mut vectors = Vec::new();
    let mut digest = Sha256::new();
    digest.update(b"airk-private-ann-self-records/1\0");
    trace("enumerate_start", counts.allocated_count)?;
    for id in 0..u32::try_from(counts.allocated_count)? {
        ensure!(
            Instant::now() < deadline,
            "original diagnostic deadline exceeded"
        );
        trace("cached_read_start", u64::from(id))?;
        let record = reader.read(id)?;
        trace("cached_read_done", u64::from(id))?;
        record.validate(reader.scope(), id)?;
        let decoded = codec::decode(dimensions, counts.allocated_count, id, &record.bytes)?;
        trace("decode_done", u64::from(id))?;
        digest.update(id.to_be_bytes());
        digest.update([u8::from(record.live)]);
        digest.update(record.embedding_key.map_or([0; 32], |key| *key.as_bytes()));
        digest.update(&record.bytes);
        if record.live {
            ensure!(id != 0, "reserved seed cannot be live");
            vectors.push((id, decoded.vector));
        }
    }
    trace("enumerate_done", u64::try_from(vectors.len())?)?;
    ensure!(
        u64::try_from(vectors.len())? == counts.live_nodes,
        "authenticated live census mismatch"
    );
    let ids: Vec<_> = vectors.iter().map(|(id, _)| *id).collect();
    let mut rows = Vec::new();
    let mut aggregates = Vec::new();
    for k in [10usize, 64] {
        let mut self_hits = 0usize;
        let mut overlap = 0usize;
        let mut denominator = 0usize;
        for (ordinal, (id, vector)) in vectors.iter().enumerate() {
            ensure!(
                Instant::now() < deadline,
                "original diagnostic deadline exceeded"
            );
            let trace_ordinal = (u64::try_from(k)? << 32) | u64::try_from(ordinal)?;
            trace("exact_query_start", trace_ordinal)?;
            let exact = graph.search_subset(vector, &ids, k)?;
            trace("exact_query_done", trace_ordinal)?;
            trace("ann_query_start", trace_ordinal)?;
            let ann = graph.search(vector, k)?;
            trace("ann_query_done", trace_ordinal)?;
            ensure!(
                Instant::now() < deadline,
                "original diagnostic deadline exceeded"
            );
            ensure!(
                !exact.partial && exact.hits.len() == k.min(ids.len()),
                "incomplete exact diagnostic oracle"
            );
            let distances: Vec<_> = vectors
                .iter()
                .map(|(other, candidate)| (*other, L2.evaluate_similarity(vector, candidate)))
                .collect();
            ensure!(
                distances.iter().all(|(_, distance)| distance.is_finite()),
                "nonfinite exact distance"
            );
            let zero_tie_ids: Vec<_> = distances
                .iter()
                .filter(|(_, distance)| *distance == 0.0)
                .map(|(id, _)| *id)
                .collect();
            ensure!(
                zero_tie_ids.contains(id),
                "own vector not at exact zero distance"
            );
            let cutoff = exact.hits.last().context("empty exact prefix")?.1;
            let strictly_closer: BTreeSet<_> = distances
                .iter()
                .filter(|(_, distance)| *distance < cutoff)
                .map(|(id, _)| *id)
                .collect();
            let cutoff_tie_ids: Vec<_> = distances
                .iter()
                .filter(|(_, distance)| *distance == cutoff)
                .map(|(id, _)| *id)
                .collect();
            let exact_ids: BTreeSet<_> = exact.hits.iter().map(|(found, _)| *found).collect();
            let ann_ids: BTreeSet<_> = ann.hits.iter().map(|(found, _)| *found).collect();
            ensure!(
                ann_ids.len() == ann.hits.len()
                    && ann_ids.is_subset(&ids.iter().copied().collect()),
                "ANN returned duplicate or foreign live ID"
            );
            self_hits += usize::from(ann_ids.contains(id));
            let found = ann_ids.intersection(&exact_ids).count();
            overlap += found;
            denominator += exact_ids.len();
            let missing_strictly_closer: Vec<_> =
                strictly_closer.difference(&ann_ids).copied().collect();
            let within_cutoff = ann
                .hits
                .iter()
                .filter(|(_, distance)| *distance <= cutoff)
                .count();
            let distance_mismatches:Vec<_>=ann.hits.iter().filter_map(|(id,observed)|distances.iter().find(|(found,_)|found==id).filter(|(_,expected)|*expected!=*observed).map(|(_,expected)|json!({"id":id,"observed":observed,"expected":expected}))).collect();
            rows.push(json!({"query_node_id":id,"k":k,"exact":exact.hits,"ann":ann.hits,"ann_partial":ann.partial,"ann_record_reads":ann.record_reads,"ann_passes":ann.passes,"exact_record_reads":exact.record_reads,"overlap":found,"denominator":exact_ids.len(),"exact_cutoff_distance":cutoff,"cutoff_tie_ids":cutoff_tie_ids,"zero_distance_tie_ids":zero_tie_ids,"missing_strictly_closer_ids":missing_strictly_closer,"ann_hits_within_exact_distance_cutoff":within_cutoff,"ann_distance_mismatches":distance_mismatches,"self_id_in_exact_prefix":exact_ids.contains(id),"self_id_in_ann":ann_ids.contains(id)}));
        }
        aggregates.push(json!({"k":k,"queries":vectors.len(),"self_hits":self_hits,"self_recall":self_hits as f64/vectors.len() as f64,"recall_numerator":overlap,"recall_denominator":denominator,"recall":overlap as f64/denominator as f64}));
    }
    ensure!(
        reader.counts()? == counts,
        "graph counts changed during diagnostic"
    );
    Ok(
        json!({"graph_scope":reader.scope(),"counts":counts,"live_ids":ids,"authenticated_record_census_sha256":format!("{:x}",digest.finalize()),"aggregates":aggregates,"rows":rows,"embedding_actual_forwards":0,"embedding_inferences":0,"diagnostic_cache":{"raw_records_authenticated_once":source_counts.allocated_count,"authority":"same retained CurrentRead snapshot; native token/head fences outside helper","read_counters":"logical cached record requests, not physical I/O","latency_qualification":false},"scope":"self-vector graph diagnostic only; does not establish human-query recall or relevance"}),
    )
}

/// Localize actual-reader open/read/exact-single/ANN boundaries, not a recall metric.
pub fn first_live_reader_smoke(
    reader: Arc<dyn GraphRecordReader>,
    dimensions: usize,
    pipeline: Sha256Digest,
    deadline: Instant,
    trace: &mut dyn FnMut(&'static str, u64) -> Result<()>,
) -> Result<Value> {
    trace("mutable_open_start", 0)?;
    let graph = MutableDiskIndex::open(reader.clone(), dimensions, pipeline)?;
    trace("mutable_open_done", 0)?;
    let counts = reader.counts()?;
    ensure!(
        counts.live_nodes > 0 && counts.allocated_count <= 4097,
        "bounded nonempty reader required"
    );
    for id in 1..u32::try_from(counts.allocated_count)? {
        ensure!(
            Instant::now() < deadline,
            "original smoke deadline exceeded"
        );
        trace("authenticated_read_start", u64::from(id))?;
        let record = reader.read(id)?;
        trace("authenticated_read_done", u64::from(id))?;
        record.validate(reader.scope(), id)?;
        if !record.live {
            continue;
        }
        let decoded = codec::decode(dimensions, counts.allocated_count, id, &record.bytes)?;
        trace("decode_done", u64::from(id))?;
        trace("exact_single_start", u64::from(id))?;
        let exact = graph.search_subset(&decoded.vector, &[id], 1)?;
        trace("exact_single_done", u64::from(id))?;
        ensure!(
            exact.hits == [(id, 0.0)],
            "actual self-only exact distance differs"
        );
        ensure!(
            Instant::now() < deadline,
            "original smoke deadline exceeded"
        );
        trace("ann_query_start", u64::from(id))?;
        let ann = graph.search(&decoded.vector, 10)?;
        trace("ann_query_done", u64::from(id))?;
        ensure!(
            Instant::now() < deadline,
            "original smoke deadline exceeded"
        );
        return Ok(
            json!({"purpose":"first-live-reader localization only, not all-live recall","graph_scope":reader.scope(),"counts":counts,"query_node_id":id,"exact_single":exact.hits,"ann":ann.hits,"ann_partial":ann.partial,"ann_record_reads":ann.record_reads,"embedding_actual_forwards":0,"embedding_inferences":0}),
        );
    }
    anyhow::bail!("authenticated live census inconsistent")
}

#[cfg(test)]
mod controls {
    use super::*;
    use airk_core::{graph_storage::GraphScope, identity::Incarnation};
    struct Fixed {
        scope: GraphScope,
        records: Vec<GraphRecord>,
    }
    impl GraphRecordReader for Fixed {
        fn scope(&self) -> &GraphScope {
            &self.scope
        }
        fn counts(&self) -> Result<GraphCounts> {
            Ok(GraphCounts {
                allocated_count: u64::try_from(self.records.len())?,
                live_nodes: u64::try_from(
                    self.records.iter().filter(|record| record.live).count(),
                )?,
            })
        }
        fn read(&self, id: u32) -> Result<GraphRecord> {
            self.records
                .get(usize::try_from(id)?)
                .cloned()
                .context("foreign fixed ID")
        }
    }
    #[test]
    fn exact_self_probe_has_independent_fp32_norm_and_tie_literals() -> Result<()> {
        let pipeline = Sha256Digest::from_bytes([7; 32]);
        let vectors = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [0.0, -1.0], [-1.0, 0.0]];
        let records = vectors
            .iter()
            .enumerate()
            .map(|(id, vector)| -> Result<_> {
                let id = u32::try_from(id)?;
                let neighbors: Vec<_> = (1..5u32).filter(|other| *other != id).collect();
                Ok(GraphRecord {
                    embedding_key: (id != 0)
                        .then_some(Sha256Digest::from_bytes([u8::try_from(id)?; 32])),
                    live: id != 0,
                    bytes: codec::encode(2, 5, id, vector, &neighbors)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let reader = Arc::new(Fixed {
            scope: GraphScope {
                collection: "fixture".into(),
                root: "fixture-root".into(),
                incarnation: Incarnation::from_bytes([1; 16]),
                graph_instance: Incarnation::from_bytes([2; 16]),
                specification: specification(2, pipeline)?,
                record_bytes: record_bytes(2)?,
            },
            records,
        });
        let graph = MutableDiskIndex::open(reader.clone(), 2, pipeline)?;
        assert_eq!(
            graph.search_subset(&[1.0, 0.0], &[1, 2, 3, 4], 4)?.hits,
            vec![(1, 0.0), (2, 2.0), (3, 2.0), (4, 4.0)]
        );
        struct Audit {
            base: Arc<Fixed>,
            reads: AtomicUsize,
            fail: Option<u32>,
            live: AtomicBool,
        }
        impl GraphRecordReader for Audit {
            fn scope(&self) -> &GraphScope {
                self.base.scope()
            }
            fn counts(&self) -> Result<GraphCounts> {
                ensure!(
                    self.live.load(Ordering::Relaxed),
                    "source authority revoked"
                );
                self.base.counts()
            }
            fn read(&self, id: u32) -> Result<GraphRecord> {
                self.reads.fetch_add(1, Ordering::Relaxed);
                ensure!(
                    self.fail != Some(id),
                    "owning reader rejected missing/tampered record"
                );
                self.base.read(id)
            }
        }
        let audit = Arc::new(Audit {
            base: reader.clone(),
            reads: AtomicUsize::new(0),
            fail: None,
            live: AtomicBool::new(true),
        });
        let cached = AuthenticatedRecords::capture(
            audit.clone(),
            2,
            pipeline,
            Instant::now() + std::time::Duration::from_secs(5),
            &mut |_, _| Ok(()),
        )?;
        assert_eq!(
            audit.reads.load(Ordering::Relaxed),
            5,
            "all original raw records including dead seed authenticated exactly once"
        );
        let cache_graph = MutableDiskIndex::open(cached.clone(), 2, pipeline)?;
        for query in [[1.0, 0.0], [0.0, 1.0], [0.0, -1.0], [-1.0, 0.0]] {
            assert_eq!(
                cache_graph.search(&query, 4)?.hits,
                graph.search(&query, 4)?.hits
            );
            assert_eq!(
                cache_graph.search_subset(&query, &[1, 2, 3, 4], 4)?.hits,
                graph.search_subset(&query, &[1, 2, 3, 4], 4)?.hits
            );
        }
        assert_eq!(
            audit.reads.load(Ordering::Relaxed),
            5,
            "cache never reopens source records"
        );
        assert!(cached.read(5).is_err());
        audit.live.store(false, Ordering::Relaxed);
        assert!(
            cached.read(1).is_err(),
            "cached bytes cannot outlive source authority"
        );
        let rejected = Arc::new(Audit {
            base: reader.clone(),
            reads: AtomicUsize::new(0),
            fail: Some(2),
            live: AtomicBool::new(true),
        });
        assert!(
            AuthenticatedRecords::capture(
                rejected,
                2,
                pipeline,
                Instant::now() + std::time::Duration::from_secs(5),
                &mut |_, _| Ok(())
            )
            .is_err()
        );
        let mut malformed = reader.records.clone();
        malformed[2].bytes.pop();
        assert!(
            AuthenticatedRecords::capture(
                Arc::new(Fixed {
                    scope: reader.scope.clone(),
                    records: malformed
                }),
                2,
                pipeline,
                Instant::now() + std::time::Duration::from_secs(5),
                &mut |_, _| Ok(())
            )
            .is_err()
        );
        assert!(graph.search_subset(&[1.0, 0.0], &[2, 1], 2).is_err());
        assert!(graph.search_subset(&[1.0, 0.0], &[0, 1], 2).is_err());
        assert!(
            MutableDiskIndex::open(reader.clone(), 2, Sha256Digest::from_bytes([8; 32])).is_err()
        );
        let mut dead_records = reader.records.clone();
        dead_records[4].live = false;
        let dead = MutableDiskIndex::open(
            Arc::new(Fixed {
                scope: reader.scope.clone(),
                records: dead_records,
            }),
            2,
            pipeline,
        )?;
        assert!(dead.search_subset(&[1.0, 0.0], &[4], 1).is_err());
        Ok(())
    }

    #[test]
    fn self_probe_reports_duplicate_zero_ties_without_demanding_every_self_id_in_top_ten()
    -> Result<()> {
        let pipeline = Sha256Digest::from_bytes([7; 32]);
        let mut records = Vec::new();
        for id in 0..18u32 {
            let vector = if id == 0 { [0.0, 0.0] } else { [1.0, 0.0] };
            let neighbors: Vec<_> = (1..18u32).filter(|other| *other != id).collect();
            records.push(GraphRecord {
                embedding_key: (id != 0)
                    .then_some(Sha256Digest::from_bytes([u8::try_from(id)?; 32])),
                live: id != 0,
                bytes: codec::encode(2, 18, id, &vector, &neighbors)?,
            });
        }
        let reader = Arc::new(Fixed {
            scope: GraphScope {
                collection: "fixture".into(),
                root: "fixture-root".into(),
                incarnation: Incarnation::from_bytes([1; 16]),
                graph_instance: Incarnation::from_bytes([2; 16]),
                specification: specification(2, pipeline)?,
                record_bytes: record_bytes(2)?,
            },
            records,
        });
        let result = compare_self_vectors(
            reader,
            2,
            pipeline,
            Instant::now() + std::time::Duration::from_secs(5),
        )?;
        let row = result["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["query_node_id"] == 17 && row["k"] == 10)
            .unwrap();
        assert_eq!(row["self_id_in_exact_prefix"], false);
        assert_eq!(row["zero_distance_tie_ids"].as_array().unwrap().len(), 17);
        assert_eq!(row["exact_cutoff_distance"], 0.0);
        assert!(
            row["missing_strictly_closer_ids"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        Ok(())
    }
}
