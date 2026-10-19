// SPDX-License-Identifier: Apache-2.0
//! Canonical fixed-width payload; the storage layer authenticates its metadata and bytes.
use super::super::{DEGREE, MAX_DIMENSIONS, validate_vector};
use airk_core::identity::Sha256Digest;
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};

/// Payload capacity excludes the authenticated key/live metadata held by storage.
pub fn record_bytes(dimensions: usize) -> Result<u32> {
    ensure!(
        (1..=MAX_DIMENSIONS).contains(&dimensions),
        "invalid mutable DiskANN vector dimensions"
    );
    Ok((4 * (dimensions + DEGREE + 1)) as u32)
}

/// Binds the exact codec, numerical embedding pipeline and current DiskANN settings.
pub fn specification(dimensions: usize, pipeline: Sha256Digest) -> Result<Sha256Digest> {
    record_bytes(dimensions)?;
    let mut digest = Sha256::new();
    digest.update(b"airk-mutable-diskann-1\0diskann-0.59.0\0le-f32-adjacency-48\0l2\0pruned-degree-32\0build-list-64\0triangle-prune\0");
    digest.update((dimensions as u32).to_be_bytes());
    digest.update(pipeline.as_bytes());
    Ok(Sha256Digest::from_bytes(digest.finalize().into()))
}

pub(super) struct Decoded {
    pub vector: Vec<f32>,
    pub neighbors: Vec<u32>,
}

pub(super) fn encode(
    dimensions: usize,
    allocated_count: u64,
    id: u32,
    vector: &[f32],
    neighbors: &[u32],
) -> Result<Vec<u8>> {
    validate_vector(vector, dimensions)?;
    validate_neighbors(allocated_count, id, neighbors)?;
    let mut bytes = Vec::with_capacity(record_bytes(dimensions)? as usize);
    for value in vector {
        bytes.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    bytes.extend_from_slice(&(neighbors.len() as u32).to_le_bytes());
    for neighbor in neighbors {
        bytes.extend_from_slice(&neighbor.to_le_bytes());
    }
    bytes.resize(record_bytes(dimensions)? as usize, 0);
    Ok(bytes)
}

pub(super) fn decode(
    dimensions: usize,
    allocated_count: u64,
    id: u32,
    bytes: &[u8],
) -> Result<Decoded> {
    ensure!(
        bytes.len() == record_bytes(dimensions)? as usize,
        "mutable DiskANN record length mismatch"
    );
    let mut words = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|word| u32::from_le_bytes(*word));
    let vector: Vec<_> = words
        .by_ref()
        .take(dimensions)
        .map(f32::from_bits)
        .collect();
    validate_vector(&vector, dimensions)?;
    let degree = words.next().expect("validated fixed record length") as usize;
    ensure!(degree <= DEGREE, "mutable DiskANN degree exceeds capacity");
    let neighbors: Vec<_> = words.by_ref().take(degree).collect();
    validate_neighbors(allocated_count, id, &neighbors)?;
    ensure!(
        words.all(|word| word == 0),
        "noncanonical mutable DiskANN adjacency padding"
    );
    Ok(Decoded { vector, neighbors })
}

fn validate_neighbors(allocated_count: u64, id: u32, neighbors: &[u32]) -> Result<()> {
    ensure!(
        u64::from(id) < allocated_count && allocated_count <= u64::from(u32::MAX) + 1,
        "mutable DiskANN node outside allocated inventory"
    );
    ensure!(
        neighbors.len() <= DEGREE,
        "mutable DiskANN adjacency limit exceeded"
    );
    for (offset, neighbor) in neighbors.iter().enumerate() {
        ensure!(
            u64::from(*neighbor) < allocated_count,
            "mutable DiskANN neighbor outside allocated inventory"
        );
        ensure!(*neighbor != id, "mutable DiskANN self edge");
        ensure!(
            !neighbors[..offset].contains(neighbor),
            "duplicate mutable DiskANN neighbor"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_bits_and_bounds() -> Result<()> {
        let bytes = encode(2, 3, 1, &[-0.0, 1.0], &[0, 2])?;
        let record = decode(2, 3, 1, &bytes)?;
        assert_eq!(record.vector[0].to_bits(), (-0.0f32).to_bits());
        assert_eq!(record.neighbors, [0, 2]);
        assert!(encode(2, 3, 1, &[0.0, 1.0], &[1]).is_err());
        assert!(encode(2, 3, 1, &[0.0, 1.0], &[2, 2]).is_err());
        assert!(encode(2, 3, 1, &[0.0, 1.0], &[3]).is_err());
        assert!(encode(2, 3, 1, &[f32::NAN, 1.0], &[]).is_err());
        assert!(decode(2, 3, 1, &bytes[..bytes.len() - 1]).is_err());
        let mut corrupt = bytes;
        *corrupt.last_mut().unwrap() = 1;
        assert!(decode(2, 3, 1, &corrupt).is_err());
        assert_eq!(record_bytes(MAX_DIMENSIONS)?, 8388);
        assert!(record_bytes(MAX_DIMENSIONS + 1).is_err());
        assert_ne!(
            specification(2, Sha256Digest::from_bytes([1; 32]))?,
            specification(2, Sha256Digest::from_bytes([2; 32]))?
        );
        Ok(())
    }
}
