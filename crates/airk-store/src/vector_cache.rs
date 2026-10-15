// SPDX-License-Identifier: Apache-2.0
//! Collection-local immutable embedding objects. Caller holds the collection writer.
use airk_core::{
    embedding::{CachedEmbedding, EmbeddingCache, MAX_CACHED_EMBEDDING_BYTES},
    identity::Sha256Digest,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub type CacheCharge = Arc<dyn Fn(u64) -> Result<()> + Send + Sync>;
pub const MAX_CACHE_REFERENCES: usize = 65_536;
pub const MAX_CACHE_READ_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_CACHE_MANIFEST_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorCacheManifest {
    pub version: u32,
    pub references: Vec<VectorCacheReference>,
}

impl VectorCacheManifest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && self.references.len() <= MAX_CACHE_REFERENCES,
            "unsupported or oversized embedding cache manifest"
        );
        ensure!(
            self.references
                .windows(2)
                .all(|pair| pair[0].key < pair[1].key),
            "embedding cache references must be sorted and unique"
        );
        ensure!(
            self.references.iter().all(|reference| reference.bytes > 0
                && reference.bytes <= MAX_CACHED_EMBEDDING_BYTES as u64),
            "invalid embedding cache reference length"
        );
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_CACHE_MANIFEST_BYTES,
            "embedding cache manifest exceeds byte bound"
        );
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_CACHE_MANIFEST_BYTES,
            "embedding cache manifest exceeds byte bound"
        );
        let manifest: Self = serde_json::from_slice(bytes)?;
        manifest.validate()?;
        ensure!(
            manifest.canonical_bytes()? == bytes,
            "noncanonical embedding cache manifest"
        );
        Ok(manifest)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorCacheReference {
    pub key: Sha256Digest,
    pub sha256: Sha256Digest,
    pub bytes: u64,
}

#[derive(Default)]
struct Observed {
    references: BTreeMap<Sha256Digest, VectorCacheReference>,
    read_bytes: u64,
}

pub struct VectorCache {
    collection: PathBuf,
    charge: CacheCharge,
    trusted: BTreeMap<Sha256Digest, VectorCacheReference>,
    observed: Mutex<Observed>,
}

impl VectorCache {
    pub fn new(collection_directory: &Path, charge: CacheCharge) -> Result<Self> {
        let cache = Self {
            collection: collection_directory.to_owned(),
            charge,
            trusted: BTreeMap::new(),
            observed: Mutex::new(Observed::default()),
        };
        cache.path(&Sha256Digest::from_bytes([0; 32]))?;
        Ok(cache)
    }

    /// Only pass references loaded from a receipt-verified generation sidecar.
    pub fn with_trusted_references(mut self, references: &[VectorCacheReference]) -> Result<Self> {
        ensure!(
            references.len() <= MAX_CACHE_REFERENCES,
            "embedding reference budget exceeded"
        );
        VectorCacheManifest {
            version: 1,
            references: references.to_vec(),
        }
        .validate()?;
        self.trusted = references
            .iter()
            .map(|reference| (reference.key, reference.clone()))
            .collect();
        Ok(self)
    }

    fn path(&self, key: &Sha256Digest) -> Result<PathBuf> {
        let objects = self.collection.join("objects");
        let directory = objects.join("embeddings");
        let path = directory.join(key.to_string());
        for component in [&self.collection, &objects, &directory, &path] {
            match fs::symlink_metadata(component) {
                Ok(metadata) => ensure!(
                    !metadata.file_type().is_symlink()
                        && if component == &path {
                            metadata.is_file()
                        } else {
                            metadata.is_dir()
                        },
                    "invalid embedding cache component"
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(path)
    }

    fn record(&self, key: Sha256Digest, bytes: &[u8]) -> Result<()> {
        let reference = VectorCacheReference {
            key,
            bytes: bytes.len() as u64,
            sha256: Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
        };
        let mut observed = self
            .observed
            .lock()
            .map_err(|_| anyhow::anyhow!("embedding cache lock poisoned"))?;
        if let Some(prior) = observed.references.get(&key) {
            ensure!(
                prior == &reference,
                "immutable embedding cache changed during publication"
            );
        } else {
            ensure!(
                observed.references.len() < MAX_CACHE_REFERENCES,
                "embedding cache reference budget exceeded"
            );
            observed.references.insert(key, reference);
        }
        Ok(())
    }

    /// Sorted, deduplicated references for the authenticated generation finalizer.
    pub fn references(&self) -> Result<Vec<VectorCacheReference>> {
        Ok(self
            .observed
            .lock()
            .map_err(|_| anyhow::anyhow!("embedding cache lock poisoned"))?
            .references
            .values()
            .cloned()
            .collect())
    }
    fn read_entry(&self, key: &Sha256Digest) -> Result<Option<(CachedEmbedding, Vec<u8>)>> {
        let path = self.path(key)?;
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let length = file.metadata()?.len();
        ensure!(
            length <= MAX_CACHED_EMBEDDING_BYTES as u64,
            "oversized cached embedding"
        );
        {
            let mut observed = self
                .observed
                .lock()
                .map_err(|_| anyhow::anyhow!("embedding cache lock poisoned"))?;
            observed.read_bytes = observed
                .read_bytes
                .checked_add(length + 1)
                .context("embedding read budget overflow")?;
            ensure!(
                observed.read_bytes <= MAX_CACHE_READ_BYTES,
                "embedding cache read budget exceeded"
            );
        }
        let mut bytes = Vec::new();
        file.take(MAX_CACHED_EMBEDDING_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == length,
            "cached embedding changed during read"
        );
        let entry: CachedEmbedding =
            serde_json::from_slice(&bytes).context("invalid cached embedding")?;
        entry.validate(key, entry.vector.len() as u32)?;
        Ok(Some((entry, bytes)))
    }
}

impl EmbeddingCache for VectorCache {
    fn lookup(&self, key: &Sha256Digest) -> Result<Option<CachedEmbedding>> {
        let expected = self
            .observed
            .lock()
            .map_err(|_| anyhow::anyhow!("embedding cache lock poisoned"))?
            .references
            .get(key)
            .cloned()
            .or_else(|| self.trusted.get(key).cloned());
        let Some(expected) = expected else {
            return Ok(None);
        };
        let (entry, bytes) = self
            .read_entry(key)?
            .context("authenticated embedding cache entry missing")?;
        ensure!(
            expected.bytes == bytes.len() as u64
                && expected.sha256 == Sha256Digest::from_bytes(Sha256::digest(&bytes).into()),
            "authenticated embedding cache digest mismatch"
        );
        self.record(*key, &bytes)?;
        Ok(Some(entry))
    }

    fn store(&self, entry: &CachedEmbedding) -> Result<()> {
        entry.validate(&entry.key, entry.vector.len() as u32)?;
        if let Some((existing, bytes)) = self.read_entry(&entry.key)? {
            ensure!(
                existing == *entry,
                "recomputed embedding differs from immutable cache"
            );
            if self.trusted.contains_key(&entry.key) {
                self.lookup(&entry.key)?;
            }
            self.record(entry.key, &bytes)?;
            return Ok(());
        }
        let bytes = serde_json::to_vec(entry)?;
        ensure!(
            bytes.len() <= MAX_CACHED_EMBEDDING_BYTES,
            "oversized cached embedding"
        );
        let path = self.path(&entry.key)?;
        let directory = path.parent().context("embedding cache directory missing")?;
        fs::create_dir_all(directory)?;
        self.path(&entry.key)?;
        (self.charge)(bytes.len() as u64)?;
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist_noclobber(path)
            .context("persist immutable embedding cache")?;
        self.record(entry.key, &bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn immutable_cache_is_charged_isolated_and_corruption_is_explicit() -> Result<()> {
        let root = tempfile::tempdir()?;
        let charged = Arc::new(AtomicU64::new(0));
        let meter = charged.clone();
        let cache = VectorCache::new(
            &root.path().join("one"),
            Arc::new(move |bytes| {
                meter.fetch_add(bytes, Ordering::Relaxed);
                Ok(())
            }),
        )?;
        let key = Sha256Digest::from_bytes([1; 32]);
        let entry = CachedEmbedding::new(key, vec![0.5, -0.5], 2)?;
        assert!(cache.lookup(&key)?.is_none());
        cache.store(&entry)?;
        let first_charge = charged.load(Ordering::Relaxed);
        assert!(first_charge > 0);
        cache.store(&entry)?;
        assert_eq!(charged.load(Ordering::Relaxed), first_charge);
        assert_eq!(cache.lookup(&key)?, Some(entry.clone()));
        assert_eq!(cache.references()?.len(), 1);
        let other = VectorCache::new(&root.path().join("two"), Arc::new(|_| Ok(())))?;
        assert!(other.lookup(&key)?.is_none());
        let denied = VectorCache::new(
            &root.path().join("denied"),
            Arc::new(|_| anyhow::bail!("quota denied")),
        )?;
        assert!(denied.store(&entry).is_err());
        assert!(denied.lookup(&key)?.is_none());
        let changed = CachedEmbedding::new(key, vec![0.4, -0.5], 2)?;
        assert!(cache.store(&changed).is_err());
        fs::write(cache.path(&key)?, b"corrupt")?;
        assert!(cache.lookup(&key).is_err());
        Ok(())
    }

    #[test]
    fn manifests_are_bounded_canonical_and_entries_reject_invalid_sizes() -> Result<()> {
        let root = tempfile::tempdir()?;
        let cache = VectorCache::new(root.path(), Arc::new(|_| Ok(())))?;
        let key = Sha256Digest::from_bytes([2; 32]);
        cache.store(&CachedEmbedding::new(key, vec![1.0], 1)?)?;
        let mut manifest = VectorCacheManifest {
            version: 1,
            references: cache.references()?,
        };
        assert_eq!(
            VectorCacheManifest::from_bytes(&manifest.canonical_bytes()?)?.references,
            manifest.references
        );
        manifest.references.push(manifest.references[0].clone());
        assert!(manifest.canonical_bytes().is_err());
        manifest.references.pop();
        manifest.references[0].bytes = MAX_CACHED_EMBEDDING_BYTES as u64 + 1;
        assert!(manifest.validate().is_err());
        assert!(
            VectorCacheManifest::from_bytes(&vec![b' '; MAX_CACHE_MANIFEST_BYTES + 1]).is_err()
        );
        fs::write(
            cache.path(&key)?,
            vec![b' '; MAX_CACHED_EMBEDDING_BYTES + 1],
        )?;
        assert!(cache.lookup(&key).is_err());
        Ok(())
    }

    #[test]
    fn only_authenticated_or_recomputed_entries_can_be_reused() -> Result<()> {
        let root = tempfile::tempdir()?;
        let writer = VectorCache::new(root.path(), Arc::new(|_| Ok(())))?;
        let key = Sha256Digest::from_bytes([3; 32]);
        let entry = CachedEmbedding::new(key, vec![0.5], 1)?;
        writer.store(&entry)?;
        let references = writer.references()?;
        let unknown = VectorCache::new(
            root.path(),
            Arc::new(|_| anyhow::bail!("must not charge existing entry")),
        )?;
        assert!(
            unknown.lookup(&key)?.is_none(),
            "self-validating cache content is not authenticated"
        );
        assert!(unknown.references()?.is_empty());
        unknown.store(&entry)?;
        assert_eq!(unknown.lookup(&key)?, Some(entry.clone()));
        let trusted = VectorCache::new(root.path(), Arc::new(|_| Ok(())))?
            .with_trusted_references(&references)?;
        assert_eq!(trusted.lookup(&key)?, Some(entry));
        let forged = CachedEmbedding::new(key, vec![0.9], 1)?;
        fs::write(writer.path(&key)?, serde_json::to_vec(&forged)?)?;
        assert!(
            trusted.lookup(&key).is_err(),
            "valid forged internal checksum still fails authenticated raw digest"
        );
        Ok(())
    }
}
