// SPDX-License-Identifier: Apache-2.0
//! Neutral collection-local embedding cache contracts. No inference or disk implementation.
use crate::identity::{Sha256Digest, framed_digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const EMBEDDING_CACHE_CONTRACT_VERSION: u32 = 1;
pub const MAX_EMBEDDING_DIMENSIONS: usize = 2048;
pub const MAX_EMBEDDING_INPUT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_CACHED_EMBEDDING_BYTES: usize = 128 * 1024;

/// Digests cover complete canonical configuration and all referenced immutable assets.
/// Identity strings/display names alone must never stand in for these digests.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingDescriptor {
    pub version: u32,
    pub model: Sha256Digest,
    pub tokenizer: Sha256Digest,
    pub preprocessing: Sha256Digest,
    pub pooling: Sha256Digest,
    pub normalization: Sha256Digest,
    /// Provider implementation/runtime/options affecting the returned vector.
    pub inference: Sha256Digest,
    pub dimensions: u32,
}

impl EmbeddingDescriptor {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == EMBEDDING_CACHE_CONTRACT_VERSION,
            "unsupported embedding cache descriptor"
        );
        ensure!(
            (1..=MAX_EMBEDDING_DIMENSIONS as u32).contains(&self.dimensions),
            "embedding dimensions exceed bound"
        );
        Ok(())
    }

    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"embedding-descriptor/1",
            &[
                &self.version.to_be_bytes(),
                self.model.as_bytes(),
                self.tokenizer.as_bytes(),
                self.preprocessing.as_bytes(),
                self.pooling.as_bytes(),
                self.normalization.as_bytes(),
                self.inference.as_bytes(),
                &self.dimensions.to_be_bytes(),
            ],
        ))
    }
}

/// Exact bytes supplied to the model, after chunk formatting and before tokenization.
pub fn embedding_key(descriptor: &EmbeddingDescriptor, input: &[u8]) -> Result<Sha256Digest> {
    ensure!(
        input.len() <= MAX_EMBEDDING_INPUT_BYTES,
        "embedding input exceeds byte budget"
    );
    Ok(framed_digest(
        b"embedding-input/1",
        &[descriptor.identity()?.as_bytes(), input],
    ))
}

/// Cache adapters bound serialized input before deserializing and validate every lookup.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CachedEmbedding {
    pub version: u32,
    pub key: Sha256Digest,
    pub vector: Vec<f32>,
    pub checksum: Sha256Digest,
}

fn vector_checksum(key: &Sha256Digest, vector: &[f32]) -> Sha256Digest {
    let bytes: Vec<u8> = vector
        .iter()
        .flat_map(|value| value.to_bits().to_be_bytes())
        .collect();
    framed_digest(b"embedding-vector/1", &[key.as_bytes(), &bytes])
}

fn validate_vector(vector: &[f32], dimensions: u32) -> Result<()> {
    ensure!(
        (1..=MAX_EMBEDDING_DIMENSIONS as u32).contains(&dimensions),
        "invalid embedding dimension bound"
    );
    ensure!(
        vector.len() == dimensions as usize
            && vector
                .iter()
                .all(|value| value.is_finite() && value.abs() <= 1.0e10),
        "invalid cached embedding values or dimensions"
    );
    Ok(())
}

impl CachedEmbedding {
    pub fn from_json(bytes: &[u8], key: &Sha256Digest, dimensions: u32) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_CACHED_EMBEDDING_BYTES,
            "cached embedding exceeds serialized byte bound"
        );
        let entry: Self = serde_json::from_slice(bytes)?;
        entry.validate(key, dimensions)?;
        Ok(entry)
    }

    pub fn new(key: Sha256Digest, vector: Vec<f32>, dimensions: u32) -> Result<Self> {
        validate_vector(&vector, dimensions)?;
        let checksum = vector_checksum(&key, &vector);
        Ok(Self {
            version: EMBEDDING_CACHE_CONTRACT_VERSION,
            key,
            vector,
            checksum,
        })
    }

    pub fn validate(&self, key: &Sha256Digest, dimensions: u32) -> Result<()> {
        ensure!(
            self.version == EMBEDDING_CACHE_CONTRACT_VERSION && &self.key == key,
            "cached embedding identity mismatch"
        );
        validate_vector(&self.vector, dimensions)?;
        ensure!(
            self.checksum == vector_checksum(key, &self.vector),
            "cached embedding checksum mismatch"
        );
        Ok(())
    }
}

/// One instance is scoped to one collection by kernel. Adapters own quota, leases,
/// persistence and corruption handling; misses never authorize remote inference.
pub trait EmbeddingCache: Send + Sync {
    fn lookup(&self, key: &Sha256Digest) -> Result<Option<CachedEmbedding>>;
    fn store(&self, entry: &CachedEmbedding) -> Result<()>;
}

/// Retrieval calls this neutral helper to reject corruption instead of using stale vectors.
pub fn lookup_embedding(
    cache: &dyn EmbeddingCache,
    descriptor: &EmbeddingDescriptor,
    input: &[u8],
) -> Result<Option<Vec<f32>>> {
    let key = embedding_key(descriptor, input)?;
    cache
        .lookup(&key)?
        .map(|entry| {
            entry.validate(&key, descriptor.dimensions)?;
            Ok(entry.vector)
        })
        .transpose()
}

pub fn store_embedding(
    cache: &dyn EmbeddingCache,
    descriptor: &EmbeddingDescriptor,
    input: &[u8],
    vector: Vec<f32>,
) -> Result<()> {
    let key = embedding_key(descriptor, input)?;
    let entry = CachedEmbedding::new(key, vector, descriptor.dimensions)?;
    cache.store(&entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn descriptor() -> EmbeddingDescriptor {
        let digest = Sha256Digest::from_bytes([1; 32]);
        EmbeddingDescriptor {
            version: 1,
            model: digest,
            tokenizer: digest,
            preprocessing: digest,
            pooling: digest,
            normalization: digest,
            inference: digest,
            dimensions: 2,
        }
    }

    #[test]
    fn every_pipeline_dimension_and_exact_input_affect_keys() -> Result<()> {
        let original = descriptor();
        let key = embedding_key(&original, b"a\nb")?;
        assert_ne!(key, embedding_key(&original, b"a b")?);
        assert_ne!(key, embedding_key(&original, b"a\nb\n")?);
        for field in 0..7 {
            let mut changed = original.clone();
            let other = Sha256Digest::from_bytes([2; 32]);
            match field {
                0 => changed.model = other,
                1 => changed.tokenizer = other,
                2 => changed.preprocessing = other,
                3 => changed.pooling = other,
                4 => changed.normalization = other,
                5 => changed.inference = other,
                _ => changed.dimensions = 3,
            }
            assert_ne!(key, embedding_key(&changed, b"a\nb")?);
        }
        assert!(embedding_key(&original, &vec![0; MAX_EMBEDDING_INPUT_BYTES + 1]).is_err());
        Ok(())
    }

    #[test]
    fn malformed_vectors_and_corrupt_cache_entries_fail_closed() -> Result<()> {
        let key = embedding_key(&descriptor(), b"fixture")?;
        for vector in [
            vec![1.0],
            vec![1.0, f32::NAN],
            vec![1.0, f32::INFINITY],
            vec![1.0, 1.0e11],
        ] {
            assert!(CachedEmbedding::new(key, vector, 2).is_err());
        }
        assert!(CachedEmbedding::new(key, vec![], 0).is_err());
        assert!(CachedEmbedding::new(key, vec![0.0; 2049], 2049).is_err());
        let mut entry = CachedEmbedding::new(key, vec![0.1, -0.2], 2)?;
        entry.validate(&key, 2)?;
        assert_eq!(
            CachedEmbedding::from_json(&serde_json::to_vec(&entry)?, &key, 2)?,
            entry
        );
        assert!(
            CachedEmbedding::from_json(&vec![b' '; MAX_CACHED_EMBEDDING_BYTES + 1], &key, 2)
                .is_err()
        );
        assert!(
            entry
                .validate(&Sha256Digest::from_bytes([0; 32]), 2)
                .is_err()
        );
        entry.vector[1] = 0.2;
        assert!(entry.validate(&key, 2).is_err());
        Ok(())
    }

    #[derive(Default)]
    struct FixtureCache(Mutex<Option<CachedEmbedding>>);
    impl EmbeddingCache for FixtureCache {
        fn lookup(&self, key: &Sha256Digest) -> Result<Option<CachedEmbedding>> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .as_ref()
                .filter(|entry| &entry.key == key)
                .cloned())
        }
        fn store(&self, entry: &CachedEmbedding) -> Result<()> {
            *self.0.lock().unwrap() = Some(entry.clone());
            Ok(())
        }
    }

    #[test]
    fn neutral_helpers_reuse_only_verified_exact_input() -> Result<()> {
        let cache = FixtureCache::default();
        let descriptor = descriptor();
        assert!(lookup_embedding(&cache, &descriptor, b"fixture")?.is_none());
        store_embedding(&cache, &descriptor, b"fixture", vec![0.1, 0.2])?;
        assert_eq!(
            lookup_embedding(&cache, &descriptor, b"fixture")?,
            Some(vec![0.1, 0.2])
        );
        assert!(lookup_embedding(&cache, &descriptor, b"other")?.is_none());
        cache.0.lock().unwrap().as_mut().unwrap().vector[0] = 0.9;
        assert!(lookup_embedding(&cache, &descriptor, b"fixture").is_err());
        Ok(())
    }
}
