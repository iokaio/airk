// SPDX-License-Identifier: Apache-2.0
//! One bounded publication's append, lookup and compaction state.
use super::{
    format::{self, IndexEntry, OpenPack},
    *,
};
use crate::quota::Reservation;
use airk_core::embedding::{CachedEmbedding, EmbeddingCache, EmbeddingDescriptor};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{self, File},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Clone)]
pub(super) struct Limits {
    pub records: u32,
    pub pack_bytes: u64,
    pub packs: usize,
    pub references: usize,
    pub index_bytes: u64,
    pub reads: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            records: MAX_PACK_RECORDS,
            pack_bytes: MAX_PACK_BYTES,
            packs: MAX_PACKS,
            references: MAX_CACHE_REFERENCES,
            index_bytes: MAX_INDEX_CACHE_BYTES,
            reads: MAX_CACHE_READ_BYTES,
        }
    }
}

#[derive(Clone)]
enum Location {
    Pending(u32),
    Packed(VectorCacheReference),
}
struct PendingPack {
    file: tempfile::NamedTempFile,
    entries: Vec<IndexEntry>,
}
struct SealedPack {
    path: tempfile::TempPath,
    reference: PackReference,
}
struct State {
    observed: BTreeMap<Sha256Digest, Location>,
    packs: BTreeMap<Sha256Digest, PackReference>,
    pending: Option<PendingPack>,
    sealed: BTreeMap<Sha256Digest, SealedPack>,
    indexes: VecDeque<(Sha256Digest, OpenPack)>,
    stats: VectorCacheStatistics,
    failed: bool,
    finalized: Option<VectorCacheManifest>,
}

pub struct VectorCache {
    collection: PathBuf,
    staging: PathBuf,
    pipeline: Sha256Digest,
    dimensions: u32,
    trusted: BTreeMap<Sha256Digest, VectorCacheReference>,
    limits: Limits,
    state: Mutex<State>,
    // Drop private files and retained readers before last-reservation cleanup.
    reservation: Reservation,
}

impl VectorCache {
    /// `trusted` must come from a receipt-authenticated sidecar in this collection.
    /// A different pipeline is an explicit miss, without opening its old packs.
    pub fn new(
        collection: &Path,
        reservation: &Reservation,
        descriptor: &EmbeddingDescriptor,
        trusted: Option<&VectorCacheManifest>,
    ) -> Result<Self> {
        Self::with_limits(
            collection,
            reservation,
            descriptor,
            trusted,
            Limits::default(),
        )
    }

    pub(super) fn with_limits(
        collection: &Path,
        reservation: &Reservation,
        descriptor: &EmbeddingDescriptor,
        trusted: Option<&VectorCacheManifest>,
        limits: Limits,
    ) -> Result<Self> {
        let pipeline = descriptor.identity()?;
        ensure!(
            (1..=MAX_PACK_RECORDS).contains(&limits.records)
                && (1..=MAX_PACKS).contains(&limits.packs)
                && (1..=MAX_CACHE_REFERENCES).contains(&limits.references)
                && limits.pack_bytes <= MAX_PACK_BYTES
                && limits.pack_bytes >= format::pack_bytes(descriptor.dimensions, 1)?
                && limits.index_bytes <= MAX_INDEX_CACHE_BYTES
                && limits.index_bytes >= std::mem::size_of::<IndexEntry>() as u64
                && limits.reads <= MAX_CACHE_READ_BYTES,
            "invalid embedding cache limits"
        );
        if let Some(manifest) = trusted {
            manifest.canonical_bytes()?;
        }
        let staging = reservation.staging_directory()?;
        ensure!(
            staging
                .parent()
                .context("embedding cache staging parent missing")?
                .canonicalize()?
                == collection.canonicalize()?,
            "embedding cache reservation belongs to another collection"
        );
        format::pack_path(collection, &Sha256Digest::from_bytes([0; 32]))?;
        let trusted = trusted.filter(|manifest| {
            manifest.pipeline_digest == pipeline && manifest.dimensions == descriptor.dimensions
        });
        Ok(Self {
            collection: collection.to_owned(),
            staging,
            reservation: reservation.clone(),
            pipeline,
            dimensions: descriptor.dimensions,
            limits,
            trusted: trusted
                .into_iter()
                .flat_map(|manifest| &manifest.references)
                .map(|reference| (reference.key, reference.clone()))
                .collect(),
            state: Mutex::new(State {
                observed: BTreeMap::new(),
                packs: trusted
                    .into_iter()
                    .flat_map(|manifest| &manifest.packs)
                    .map(|pack| (pack.sha256, pack.clone()))
                    .collect(),
                pending: None,
                sealed: BTreeMap::new(),
                indexes: VecDeque::new(),
                stats: Default::default(),
                failed: false,
                finalized: None,
            }),
        })
    }

    fn with_state<T>(&self, operation: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("embedding cache lock poisoned"))?;
        ensure!(!state.failed, "embedding cache failed; discard this build");
        let result = operation(&mut state);
        if result.is_err() {
            state.failed = true;
        }
        result
    }

    pub fn statistics(&self) -> Result<VectorCacheStatistics> {
        Ok(self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("embedding cache lock poisoned"))?
            .stats
            .clone())
    }

    fn packed_path(&self, state: &State, digest: Sha256Digest) -> Result<PathBuf> {
        if let Some(sealed) = state.sealed.get(&digest) {
            Ok(sealed.path.to_path_buf())
        } else {
            format::pack_path(&self.collection, &digest)
        }
    }

    fn read_packed(
        &self,
        state: &mut State,
        reference: &VectorCacheReference,
    ) -> Result<CachedEmbedding> {
        let pack_ref = state
            .packs
            .get(&reference.pack)
            .context("embedding pack ownership missing")?
            .clone();
        let allocation = u64::from(pack_ref.records) * std::mem::size_of::<IndexEntry>() as u64;
        ensure!(
            allocation <= self.limits.index_bytes,
            "embedding pack index exceeds cache allocation bound"
        );
        let mut pack = if let Some(position) = state
            .indexes
            .iter()
            .position(|(digest, _)| *digest == reference.pack)
        {
            state.stats.index_cache_hits += 1;
            state.indexes.remove(position).unwrap().1
        } else {
            // Leave two handle slots for the private writer and one transient reader.
            while state.indexes.len() >= MAX_OPEN_PACK_HANDLES - 2 {
                state.indexes.pop_front();
            }
            while state
                .indexes
                .iter()
                .map(|(_, pack)| pack.allocation_bytes())
                .sum::<u64>()
                + allocation
                > self.limits.index_bytes
            {
                state.indexes.pop_front();
            }
            let path = self.packed_path(state, reference.pack)?;
            format::open_index(&path, &pack_ref, &mut state.stats, self.limits.reads)?
        };
        let result = format::read_record(
            &mut pack,
            &pack_ref,
            reference.record,
            reference.key,
            &mut state.stats,
            self.limits.reads,
        );
        let allocation = pack.allocation_bytes();
        ensure!(
            allocation <= self.limits.index_bytes,
            "embedding pack index exceeds cache allocation bound"
        );
        while state
            .indexes
            .iter()
            .map(|(_, pack)| pack.allocation_bytes())
            .sum::<u64>()
            + allocation
            > self.limits.index_bytes
        {
            state.indexes.pop_front();
        }
        state.indexes.push_back((reference.pack, pack));
        let allocated = state
            .indexes
            .iter()
            .map(|(_, pack)| pack.allocation_bytes())
            .sum();
        state.stats.index_cache_peak_bytes = state.stats.index_cache_peak_bytes.max(allocated);
        result
    }

    fn read_location(
        &self,
        state: &mut State,
        key: Sha256Digest,
        location: &Location,
    ) -> Result<CachedEmbedding> {
        match location {
            Location::Packed(reference) => self.read_packed(state, reference),
            Location::Pending(ordinal) => {
                let pending = state
                    .pending
                    .as_ref()
                    .context("pending embedding pack missing")?;
                let index = pending
                    .entries
                    .get(*ordinal as usize)
                    .context("pending embedding ordinal missing")?
                    .clone();
                ensure!(index.key == key, "pending embedding key mismatch");
                let mut file = File::open(pending.file.path())?;
                state.stats.file_opens += 1;
                let size = format::record_bytes(self.dimensions)?;
                file.seek(SeekFrom::Start(
                    PACK_HEADER_BYTES + u64::from(*ordinal) * size,
                ))?;
                let mut bytes = vec![0u8; size as usize];
                format::read_exact(&mut file, &mut bytes, &mut state.stats, self.limits.reads)?;
                state.stats.record_verification_bytes += size;
                format::decode_record(&bytes, &index, self.dimensions)
            }
        }
    }

    fn write(
        &self,
        file: &mut File,
        bytes: &[u8],
        growth: bool,
        stats: &mut VectorCacheStatistics,
    ) -> Result<()> {
        if growth {
            self.reservation.charge(bytes.len() as u64)?;
        }
        file.write_all(bytes)?;
        stats.written_bytes += bytes.len() as u64;
        Ok(())
    }

    fn append(&self, state: &mut State, entry: &CachedEmbedding) -> Result<Location> {
        let count = state
            .pending
            .as_ref()
            .map_or(0, |pack| pack.entries.len() as u32);
        if count > 0
            && (count == self.limits.records
                || format::pack_bytes(self.dimensions, count + 1)? > self.limits.pack_bytes)
        {
            self.seal(state)?;
        }
        if state.pending.is_none() {
            let mut file = tempfile::NamedTempFile::new_in(&self.staging)?;
            state.stats.file_opens += 1;
            self.write(
                file.as_file_mut(),
                &[0; PACK_HEADER_BYTES as usize],
                true,
                &mut state.stats,
            )?;
            state.pending = Some(PendingPack {
                file,
                entries: Vec::new(),
            });
        }
        let bytes = format::encode_record(entry, self.dimensions)?;
        let pending = state.pending.as_mut().unwrap();
        let ordinal = pending.entries.len() as u32;
        self.write(pending.file.as_file_mut(), &bytes, true, &mut state.stats)?;
        pending.entries.push(IndexEntry {
            key: entry.key,
            ordinal,
            digest: format::hash(&bytes),
        });
        Ok(Location::Pending(ordinal))
    }

    fn seal(&self, state: &mut State) -> Result<()> {
        let Some(mut pending) = state.pending.take() else {
            return Ok(());
        };
        ensure!(!pending.entries.is_empty(), "empty pending embedding pack");
        let mut reference = PackReference {
            sha256: Sha256Digest::from_bytes([0; 32]),
            bytes: format::pack_bytes(self.dimensions, pending.entries.len() as u32)?,
            index_sha256: Sha256Digest::from_bytes([0; 32]),
            pipeline_digest: self.pipeline,
            dimensions: self.dimensions,
            records: pending.entries.len() as u32,
        };
        let header = format::header(&reference)?;
        let index = format::index_bytes(&pending.entries)?;
        reference.index_sha256 = format::index_hash(&header, &index);
        self.write(pending.file.as_file_mut(), &index, true, &mut state.stats)?;
        pending.file.as_file_mut().seek(SeekFrom::Start(0))?;
        self.write(pending.file.as_file_mut(), &header, false, &mut state.stats)?;
        pending.file.as_file().sync_all()?;
        state.stats.syncs += 1;
        let path = pending.file.into_temp_path();
        reference.sha256 = format::verify_full(
            &path,
            &reference,
            false,
            &mut state.stats,
            self.limits.reads,
        )?;
        state.stats.packs_written += 1;
        for entry in pending.entries {
            let location = state
                .observed
                .get_mut(&entry.key)
                .context("unowned pending embedding record")?;
            ensure!(
                matches!(location, Location::Pending(ordinal) if *ordinal == entry.ordinal),
                "pending embedding record ownership changed"
            );
            *location = Location::Packed(VectorCacheReference {
                key: entry.key,
                pack: reference.sha256,
                record: entry.ordinal,
            });
        }
        // At most the old manifest plus bounded new records and one compaction
        // pass can exist privately. Final ownership is limited to MAX_PACKS.
        ensure!(
            state.sealed.len() < MAX_PACKS * 3,
            "private embedding pack count exceeded"
        );
        if let Some(previous) = state.packs.insert(reference.sha256, reference.clone()) {
            ensure!(
                previous == reference,
                "embedding pack digest collision metadata mismatch"
            );
        }
        state
            .sealed
            .entry(reference.sha256)
            .or_insert(SealedPack { path, reference });
        Ok(())
    }

    fn references(state: &State) -> Result<Vec<VectorCacheReference>> {
        state
            .observed
            .values()
            .map(|location| match location {
                Location::Packed(reference) => Ok(reference.clone()),
                Location::Pending(_) => anyhow::bail!("embedding pack not sealed"),
            })
            .collect()
    }

    pub fn finalize(&self) -> Result<VectorCacheManifest> {
        self.with_state(|state| {
            if let Some(manifest) = &state.finalized {
                return Ok(manifest.clone());
            }
            self.seal(state)?;
            let mut references = Self::references(state)?;
            let mut live = BTreeMap::<Sha256Digest, usize>::new();
            for reference in &references {
                *live.entry(reference.pack).or_default() += 1;
            }
            let pressure = live.len() > self.limits.packs;
            let compact: BTreeSet<_> = live
                .iter()
                .filter_map(|(digest, count)| {
                    let pack = &state.packs[digest];
                    (pressure || *count * 2 < pack.records as usize).then_some(*digest)
                })
                .collect();
            // Stream each old pack's physical records together. Global hashed-key
            // order would thrash the bounded index cache during large compactions.
            references.sort_by_key(|reference| (reference.pack, reference.record));
            for reference in references
                .iter()
                .filter(|reference| compact.contains(&reference.pack))
            {
                let entry = self.read_packed(state, reference)?;
                let location = self.append(state, &entry)?;
                state.observed.insert(reference.key, location);
                state.stats.compacted_records += 1;
            }
            self.seal(state)?;
            let references = Self::references(state)?;
            let owned: BTreeSet<_> = references.iter().map(|reference| reference.pack).collect();
            ensure!(
                owned.len() <= self.limits.packs,
                "embedding pack count exceeds bound after compaction"
            );
            let manifest = VectorCacheManifest {
                version: 2,
                pipeline_digest: self.pipeline,
                dimensions: self.dimensions,
                packs: owned
                    .iter()
                    .map(|digest| state.packs[digest].clone())
                    .collect(),
                references,
            };
            manifest.canonical_bytes()?;
            // Close retained readers before moving files on Windows.
            state.indexes.clear();
            for digest in &owned {
                if let Some(sealed) = state.sealed.remove(digest) {
                    let target = format::pack_path(&self.collection, digest)?;
                    fs::create_dir_all(target.parent().context("embedding pack parent missing")?)?;
                    format::pack_path(&self.collection, digest)?;
                    match sealed.path.persist_noclobber(&target) {
                        Ok(()) => {}
                        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                            format::verify_full(
                                &target,
                                &sealed.reference,
                                true,
                                &mut state.stats,
                                self.limits.reads,
                            )?;
                            // The existing immutable bytes are fully verified;
                            // dropping the error's TempPath removes only our copy.
                        }
                        Err(error) => return Err(error.error.into()),
                    }
                } else {
                    state.stats.packs_reused += 1;
                }
            }
            // No unreferenced private pack is promoted; owned staging cleans up
            // private fragments after success or an interrupted publication.
            state.sealed.clear();
            #[cfg(unix)]
            if !owned.is_empty() {
                File::open(self.collection.join("objects/embedding-packs"))?.sync_all()?;
                state.stats.syncs += 1;
            }
            state.finalized = Some(manifest.clone());
            Ok(manifest)
        })
    }
}

impl EmbeddingCache for VectorCache {
    fn lookup(&self, key: &Sha256Digest) -> Result<Option<CachedEmbedding>> {
        self.with_state(|state| {
            ensure!(
                state.finalized.is_none(),
                "embedding cache already finalized"
            );
            let location = state
                .observed
                .get(key)
                .cloned()
                .or_else(|| self.trusted.get(key).cloned().map(Location::Packed));
            let Some(location) = location else {
                return Ok(None);
            };
            let entry = self.read_location(state, *key, &location)?;
            if !state.observed.contains_key(key) {
                ensure!(
                    state.observed.len() < self.limits.references,
                    "embedding cache reference budget exceeded"
                );
                state.observed.insert(*key, location);
            }
            Ok(Some(entry))
        })
    }

    fn store(&self, entry: &CachedEmbedding) -> Result<()> {
        self.with_state(|state| {
            ensure!(
                state.finalized.is_none(),
                "embedding cache already finalized"
            );
            entry.validate(&entry.key, self.dimensions)?;
            let location = state
                .observed
                .get(&entry.key)
                .cloned()
                .or_else(|| self.trusted.get(&entry.key).cloned().map(Location::Packed));
            if let Some(location) = location {
                let existing = self.read_location(state, entry.key, &location)?;
                ensure!(
                    existing.version == entry.version
                        && existing.checksum == entry.checksum
                        && existing
                            .vector
                            .iter()
                            .map(|value| value.to_bits())
                            .eq(entry.vector.iter().map(|value| value.to_bits())),
                    "recomputed embedding differs from immutable cache"
                );
                if !state.observed.contains_key(&entry.key) {
                    ensure!(
                        state.observed.len() < self.limits.references,
                        "embedding cache reference budget exceeded"
                    );
                    state.observed.insert(entry.key, location);
                }
                return Ok(());
            }
            ensure!(
                state.observed.len() < self.limits.references,
                "embedding cache reference budget exceeded"
            );
            let location = self.append(state, entry)?;
            state.observed.insert(entry.key, location);
            Ok(())
        })
    }
}
