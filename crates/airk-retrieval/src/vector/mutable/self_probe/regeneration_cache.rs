// SPDX-License-Identifier: Apache-2.0
//! PRIVATE draft; child of existing vector::mutable::self_probe, test-support only.
//! Decode original authenticated records with the actual owning codec. Never copy edges.
use super::*;
use airk_core::embedding::CachedEmbedding;

pub fn authenticated_embeddings(
    reader: Arc<dyn GraphRecordReader>,
    dimensions: usize,
    expected_allocated: u64,
    expected_live: u64,
    deadline: Instant,
) -> Result<Vec<(u32, CachedEmbedding)>> {
    let counts = reader.counts()?;
    let original_scope = reader.scope().clone();
    ensure!(
        counts.allocated_count == expected_allocated
            && counts.live_nodes == expected_live
            && expected_allocated <= 4097,
        "original authenticated graph census changed"
    );
    let mut entries = Vec::new();
    let mut keys = BTreeSet::new();
    for id in 0..u32::try_from(counts.allocated_count)? {
        ensure!(
            Instant::now() < deadline,
            "original regeneration deadline exceeded"
        );
        let record = reader.read(id)?;
        record.validate(reader.scope(), id)?;
        let decoded = codec::decode(dimensions, counts.allocated_count, id, &record.bytes)?;
        if record.live {
            ensure!(id != 0, "reserved seed cannot become cached evidence");
            let key = record
                .embedding_key
                .context("live vector has no embedding key")?;
            ensure!(keys.insert(key), "original duplicate embedding key");
            let entry = CachedEmbedding::new(key, decoded.vector, u32::try_from(dimensions)?)?;
            entry.validate(&key, u32::try_from(dimensions)?)?;
            entries.push((id, entry));
        }
    }
    ensure!(
        entries.len() as u64 == expected_live
            && reader.counts()? == counts
            && reader.scope() == &original_scope,
        "authenticated vector population changed during extraction"
    );
    Ok(entries)
}

#[cfg(test)]
mod controls {
    use super::*;
    use airk_core::{
        graph_storage::{GraphCounts, GraphRecord, GraphScope},
        identity::Incarnation,
    };
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
                allocated_count: self.records.len() as u64,
                live_nodes: self.records.iter().filter(|r| r.live).count() as u64,
            })
        }
        fn read(&self, id: u32) -> Result<GraphRecord> {
            self.records
                .get(id as usize)
                .cloned()
                .context("foreign fixture ID")
        }
    }
    fn fixture() -> Result<Fixed> {
        let scope = GraphScope {
            root: "literal-old-root".into(),
            collection: "default".into(),
            incarnation: Incarnation::from_bytes([1; 16]),
            graph_instance: Incarnation::from_bytes([2; 16]),
            specification: Sha256Digest::from_bytes([8; 32]),
            record_bytes: codec::record_bytes(2)?,
        };
        let records = [[0.0, 0.0], [1.0, -0.0], [0.0, 1.0]]
            .iter()
            .enumerate()
            .map(|(id, vector)| -> Result<_> {
                Ok(GraphRecord {
                    embedding_key: (id != 0).then_some(Sha256Digest::from_bytes([id as u8; 32])),
                    live: id != 0,
                    bytes: codec::encode(2, 3, id as u32, vector, &[])?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Fixed { scope, records })
    }
    #[test]
    fn authenticated_cache_import_preserves_literal_vector_bits_keys_and_old_scope() -> Result<()> {
        let original = fixture()?;
        let old_scope = original.scope.clone();
        let values = authenticated_embeddings(
            Arc::new(original),
            2,
            3,
            2,
            Instant::now() + std::time::Duration::from_secs(1),
        )?;
        assert_eq!(values.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(
            values[0]
                .1
                .vector
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            [1.0_f32.to_bits(), (-0.0_f32).to_bits()]
        );
        assert_eq!(values[1].1.vector, [0.0, 1.0]);
        assert_eq!(values[0].1.key, Sha256Digest::from_bytes([1; 32]));
        assert_eq!(old_scope.specification, Sha256Digest::from_bytes([8; 32]));
        values[0].1.validate(&values[0].1.key, 2)?;
        Ok(())
    }
    #[test]
    fn authenticated_cache_import_refuses_nonfinite_duplicate_or_foreign_census() -> Result<()> {
        let deadline = Instant::now() + std::time::Duration::from_secs(1);
        assert!(authenticated_embeddings(Arc::new(fixture()?), 2, 4, 2, deadline).is_err());
        let mut bad = fixture()?;
        bad.records[1].bytes[..4].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
        assert!(authenticated_embeddings(Arc::new(bad), 2, 3, 2, deadline).is_err());
        let mut bad = fixture()?;
        bad.records[2].embedding_key = bad.records[1].embedding_key;
        assert!(authenticated_embeddings(Arc::new(bad), 2, 3, 2, deadline).is_err());
        assert!(authenticated_embeddings(Arc::new(fixture()?), 2, 3, 2, Instant::now()).is_err());
        Ok(())
    }
}
