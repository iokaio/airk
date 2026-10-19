// SPDX-License-Identifier: Apache-2.0
//! Fixed-endian embedding pack encoding and bounded authenticated reads.
use super::{MAX_PACK_BYTES, MAX_PACK_RECORDS, PackReference, VectorCacheStatistics};
use airk_core::{
    embedding::{CachedEmbedding, MAX_EMBEDDING_DIMENSIONS},
    identity::Sha256Digest,
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

pub const PACK_HEADER_BYTES: u64 = 80;
pub(super) const INDEX_ENTRY_BYTES: u64 = 68;
const MAGIC: &[u8; 8] = b"AIRKEP01";

pub fn record_bytes(dimensions: u32) -> Result<u64> {
    ensure!(
        (1..=MAX_EMBEDDING_DIMENSIONS as u32).contains(&dimensions),
        "invalid embedding pack dimensions"
    );
    Ok(68 + u64::from(dimensions) * 4)
}

pub(super) fn pack_bytes(dimensions: u32, records: u32) -> Result<u64> {
    ensure!(
        (1..=MAX_PACK_RECORDS).contains(&records),
        "invalid embedding pack record count"
    );
    Ok(PACK_HEADER_BYTES + u64::from(records) * (record_bytes(dimensions)? + INDEX_ENTRY_BYTES))
}

pub(super) fn header(reference: &PackReference) -> Result<Vec<u8>> {
    reference.validate()?;
    let mut bytes = Vec::with_capacity(PACK_HEADER_BYTES as usize);
    bytes.extend_from_slice(MAGIC);
    for value in [1, reference.dimensions, reference.records, 0] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes.extend_from_slice(reference.pipeline_digest.as_bytes());
    bytes.extend_from_slice(&(record_bytes(reference.dimensions)? as u32).to_be_bytes());
    bytes.extend_from_slice(&(INDEX_ENTRY_BYTES as u32).to_be_bytes());
    bytes.extend_from_slice(&PACK_HEADER_BYTES.to_be_bytes());
    bytes.extend_from_slice(
        &(PACK_HEADER_BYTES + u64::from(reference.records) * record_bytes(reference.dimensions)?)
            .to_be_bytes(),
    );
    Ok(bytes)
}

#[derive(Clone, Debug)]
pub(super) struct IndexEntry {
    pub key: Sha256Digest,
    pub ordinal: u32,
    pub digest: Sha256Digest,
}

pub(super) fn index_bytes(entries: &[IndexEntry]) -> Result<Vec<u8>> {
    ensure!(
        !entries.is_empty() && entries.len() <= MAX_PACK_RECORDS as usize,
        "embedding index exceeds bound"
    );
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|entry| entry.key);
    ensure!(
        sorted.windows(2).all(|pair| pair[0].key < pair[1].key),
        "duplicate embedding pack key"
    );
    let mut bytes = Vec::with_capacity(sorted.len() * INDEX_ENTRY_BYTES as usize);
    for entry in sorted {
        bytes.extend_from_slice(entry.key.as_bytes());
        bytes.extend_from_slice(&entry.ordinal.to_be_bytes());
        bytes.extend_from_slice(entry.digest.as_bytes());
    }
    Ok(bytes)
}

pub(super) fn hash(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

pub(super) fn index_hash(header: &[u8], index: &[u8]) -> Sha256Digest {
    let mut hash = Sha256::new();
    hash.update(header);
    hash.update(index);
    Sha256Digest::from_bytes(hash.finalize().into())
}

pub(super) fn encode_record(entry: &CachedEmbedding, dimensions: u32) -> Result<Vec<u8>> {
    entry.validate(&entry.key, dimensions)?;
    let mut bytes = Vec::with_capacity(record_bytes(dimensions)? as usize);
    bytes.extend_from_slice(&entry.version.to_be_bytes());
    bytes.extend_from_slice(entry.key.as_bytes());
    bytes.extend_from_slice(entry.checksum.as_bytes());
    for value in &entry.vector {
        bytes.extend_from_slice(&value.to_bits().to_be_bytes());
    }
    Ok(bytes)
}

pub(super) fn decode_record(
    bytes: &[u8],
    index: &IndexEntry,
    dimensions: u32,
) -> Result<CachedEmbedding> {
    ensure!(
        bytes.len() as u64 == record_bytes(dimensions)? && hash(bytes) == index.digest,
        "authenticated embedding record digest mismatch"
    );
    // Preserve the stored checksum and float bits. Recomputing a replacement
    // checksum before validation would conceal an altered record.
    let entry = CachedEmbedding {
        version: u32::from_be_bytes(bytes[..4].try_into()?),
        key: Sha256Digest::from_bytes(bytes[4..36].try_into()?),
        checksum: Sha256Digest::from_bytes(bytes[36..68].try_into()?),
        vector: bytes[68..]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|part| f32::from_bits(u32::from_be_bytes(*part)))
            .collect(),
    };
    entry.validate(&index.key, dimensions)?;
    Ok(entry)
}

pub fn pack_path(collection: &Path, digest: &Sha256Digest) -> Result<PathBuf> {
    let objects = collection.join("objects");
    let directory = objects.join("embedding-packs");
    let path = directory.join(digest.to_string());
    for component in [
        collection,
        objects.as_path(),
        directory.as_path(),
        path.as_path(),
    ] {
        match fs::symlink_metadata(component) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink()
                    && if component == path {
                        metadata.is_file()
                    } else {
                        metadata.is_dir()
                    },
                "invalid embedding pack component"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}

pub(super) fn read_exact(
    file: &mut File,
    bytes: &mut [u8],
    stats: &mut VectorCacheStatistics,
    maximum: u64,
) -> Result<()> {
    let total = stats
        .read_bytes
        .checked_add(bytes.len() as u64)
        .context("embedding cache read budget overflow")?;
    ensure!(total <= maximum, "embedding cache read budget exceeded");
    stats.read_bytes = total;
    file.read_exact(bytes).context("read embedding pack")
}

pub(super) struct OpenPack {
    pub file: File,
    /// Entries are held in physical ordinal order after authenticating the sorted index.
    pub entries: Vec<IndexEntry>,
}

impl OpenPack {
    pub fn allocation_bytes(&self) -> u64 {
        (self.entries.capacity() * std::mem::size_of::<IndexEntry>()) as u64
    }
}

pub(super) fn open_index(
    path: &Path,
    reference: &PackReference,
    stats: &mut VectorCacheStatistics,
    maximum: u64,
) -> Result<OpenPack> {
    reference.validate()?;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() == reference.bytes,
        "embedding pack type or length mismatch"
    );
    let mut file = File::open(path)?;
    stats.file_opens += 1;
    ensure!(
        file.metadata()?.len() == reference.bytes,
        "embedding pack changed before read"
    );
    let mut stored_header = [0u8; PACK_HEADER_BYTES as usize];
    read_exact(&mut file, &mut stored_header, stats, maximum)?;
    ensure!(
        stored_header.as_slice() == header(reference)?,
        "embedding pack header binding mismatch"
    );
    let offset =
        PACK_HEADER_BYTES + u64::from(reference.records) * record_bytes(reference.dimensions)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0u8; reference.records as usize * INDEX_ENTRY_BYTES as usize];
    read_exact(&mut file, &mut bytes, stats, maximum)?;
    stats.index_verification_bytes += PACK_HEADER_BYTES + bytes.len() as u64;
    ensure!(
        index_hash(&stored_header, &bytes) == reference.index_sha256,
        "authenticated embedding pack header/index digest mismatch"
    );
    let mut entries = Vec::with_capacity(reference.records as usize);
    let mut seen = vec![false; reference.records as usize];
    for part in bytes.as_chunks::<{ INDEX_ENTRY_BYTES as usize }>().0 {
        let entry = IndexEntry {
            key: Sha256Digest::from_bytes(part[..32].try_into()?),
            ordinal: u32::from_be_bytes(part[32..36].try_into()?),
            digest: Sha256Digest::from_bytes(part[36..].try_into()?),
        };
        ensure!(
            entry.ordinal < reference.records && !seen[entry.ordinal as usize],
            "invalid embedding pack record ordinal"
        );
        ensure!(
            entries
                .last()
                .is_none_or(|prior: &IndexEntry| prior.key < entry.key),
            "embedding pack index is not sorted and unique"
        );
        seen[entry.ordinal as usize] = true;
        entries.push(entry);
    }
    entries.sort_by_key(|entry| entry.ordinal);
    Ok(OpenPack { file, entries })
}

pub(super) fn read_record(
    pack: &mut OpenPack,
    reference: &PackReference,
    ordinal: u32,
    key: Sha256Digest,
    stats: &mut VectorCacheStatistics,
    maximum: u64,
) -> Result<CachedEmbedding> {
    let entry = pack
        .entries
        .get(ordinal as usize)
        .context("embedding record ordinal outside pack")?;
    ensure!(
        entry.key == key && entry.ordinal == ordinal,
        "embedding reference key differs from authenticated index"
    );
    ensure!(
        pack.file.metadata()?.len() == reference.bytes,
        "embedding pack changed length"
    );
    let size = record_bytes(reference.dimensions)?;
    pack.file.seek(SeekFrom::Start(
        PACK_HEADER_BYTES + u64::from(ordinal) * size,
    ))?;
    let mut bytes = vec![0; size as usize];
    read_exact(&mut pack.file, &mut bytes, stats, maximum)?;
    stats.record_verification_bytes += size;
    decode_record(&bytes, entry, reference.dimensions)
}

/// Upper bound for one complete verification, including its preliminary authenticated index read.
pub fn verification_read_bound(reference: &PackReference) -> Result<u64> {
    reference.validate()?;
    Ok(reference.bytes + PACK_HEADER_BYTES + u64::from(reference.records) * INDEX_ENTRY_BYTES)
}

pub fn verify_pack(collection: &Path, reference: &PackReference) -> Result<()> {
    let mut stats = VectorCacheStatistics::default();
    verify_full(
        &pack_path(collection, &reference.sha256)?,
        reference,
        true,
        &mut stats,
        verification_read_bound(reference)?,
    )?;
    Ok(())
}

/// New packs have an authenticated header/index assembled before writing but do
/// not yet have a filename digest. Validate every record while deriving it once.
pub(super) fn verify_full(
    path: &Path,
    reference: &PackReference,
    verify_digest: bool,
    stats: &mut VectorCacheStatistics,
    maximum: u64,
) -> Result<Sha256Digest> {
    ensure!(
        reference.bytes <= MAX_PACK_BYTES,
        "embedding pack exceeds byte bound"
    );
    let mut pack = open_index(path, reference, stats, maximum)?;
    pack.file.seek(SeekFrom::Start(0))?;
    let mut full = Sha256::new();
    let mut header_index = Sha256::new();
    let mut stored_header = [0u8; PACK_HEADER_BYTES as usize];
    read_exact(&mut pack.file, &mut stored_header, stats, maximum)?;
    stats.full_pack_verification_bytes += PACK_HEADER_BYTES;
    full.update(stored_header);
    header_index.update(stored_header);
    let size = record_bytes(reference.dimensions)?;
    let mut record = vec![0u8; size as usize];
    for index in &pack.entries {
        read_exact(&mut pack.file, &mut record, stats, maximum)?;
        stats.full_pack_verification_bytes += size;
        stats.record_verification_bytes += size;
        decode_record(&record, index, reference.dimensions)?;
        full.update(&record);
    }
    let mut remaining = u64::from(reference.records) * INDEX_ENTRY_BYTES;
    let mut buffer = [0u8; 16 * 1024];
    while remaining > 0 {
        let count = remaining.min(buffer.len() as u64) as usize;
        read_exact(&mut pack.file, &mut buffer[..count], stats, maximum)?;
        stats.full_pack_verification_bytes += count as u64;
        full.update(&buffer[..count]);
        header_index.update(&buffer[..count]);
        remaining -= count as u64;
    }
    ensure!(
        pack.file.metadata()?.len() == reference.bytes
            && Sha256Digest::from_bytes(header_index.finalize().into()) == reference.index_sha256,
        "embedding pack changed during verification"
    );
    let digest = Sha256Digest::from_bytes(full.finalize().into());
    ensure!(
        !verify_digest || digest == reference.sha256,
        "embedding pack full digest mismatch"
    );
    Ok(digest)
}
