// SPDX-License-Identifier: Apache-2.0
//! Independently checked lossless blocks with an authenticated, bounded index.
use airk_core::compression::{CompressionMode, EncodedObject};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

const MAGIC: &[u8; 8] = b"AIRKCF01";
const BLOCK: usize = 1 << 20;
const MAX_INDEX: usize = 512 << 10;
const MAX_BLOCKS: usize = 8192;
pub const MAX_ENCODE_BYTES: usize = 64 << 20;

impl crate::FileStore {
    pub fn compression_mode(&self, root: &str) -> Result<CompressionMode> {
        self.owned_policy_bytes(root, "compression-policy.json")?
            .map(|bytes| serde_json::from_slice(&bytes).map_err(Into::into))
            .unwrap_or(Ok(CompressionMode::Off))
    }
    pub fn configure_compression(
        &self,
        writer: &crate::WriterLock,
        quota: &crate::quota::Quota,
        root: &str,
        mode: CompressionMode,
    ) -> Result<()> {
        self.configure_owned_policy_bytes(
            writer,
            quota,
            root,
            "compression-policy.json",
            &serde_json::to_vec(&mode)?,
        )
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Block {
    offset: u64,
    encoded: usize,
    decoded: usize,
    compressed: bool,
    stored_digest: String,
    logical_digest: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Index {
    version: u32,
    codec: String,
    logical_digest: String,
    decoded: usize,
    blocks: Vec<Block>,
}

pub(crate) fn encode(
    bytes: &[u8],
    mode: CompressionMode,
) -> Result<Option<(Vec<u8>, EncodedObject)>> {
    if mode == CompressionMode::Off || bytes.len() < 4096 || bytes.len() > MAX_ENCODE_BYTES {
        return Ok(None);
    }
    let mut payload = Vec::new();
    let mut blocks = Vec::new();
    for chunk in bytes.chunks(BLOCK) {
        ensure!(
            blocks.len() < MAX_BLOCKS,
            "compression block count exceeded"
        );
        let compressed = lz4_flex::block::compress(chunk);
        let use_compressed = compressed.len() < chunk.len();
        let stored = if use_compressed {
            compressed.as_slice()
        } else {
            chunk
        };
        blocks.push(Block {
            offset: payload.len() as u64,
            encoded: stored.len(),
            decoded: chunk.len(),
            compressed: use_compressed,
            stored_digest: digest(stored),
            logical_digest: digest(chunk),
        });
        payload.extend_from_slice(stored);
    }
    let index = serde_json::to_vec(&Index {
        version: 1,
        codec: "lz4-block/1".into(),
        logical_digest: digest(bytes),
        decoded: bytes.len(),
        blocks,
    })?;
    ensure!(index.len() <= MAX_INDEX, "compression index exceeds bound");
    let mut encoded = Vec::with_capacity(12 + index.len() + payload.len());
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&(index.len() as u32).to_le_bytes());
    encoded.extend_from_slice(&index);
    encoded.extend_from_slice(&payload);
    let required_savings = if mode == CompressionMode::Automatic {
        bytes.len() / 10
    } else {
        256
    };
    if encoded.len().saturating_add(required_savings) >= bytes.len() {
        return Ok(None);
    }
    let reference = EncodedObject {
        version: 1,
        logical_digest: digest(bytes),
        stored_digest: digest(&encoded),
        stored_bytes: encoded.len() as u64,
        index_digest: digest(&index),
    };
    Ok(Some((encoded, reference)))
}

fn open(
    path: &Path,
    reference: &EncodedObject,
    logical: &str,
    maximum: u64,
) -> Result<(File, Index, u64)> {
    ensure!(
        reference.version == 1
            && reference.logical_digest == logical
            && crate::blobs::valid_digest(&reference.stored_digest)
            && crate::blobs::valid_digest(&reference.index_digest),
        "unsupported encoded object"
    );
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() == reference.stored_bytes,
        "encoded object length or type mismatch"
    );
    let mut file = File::open(path)?;
    let mut header = [0; 12];
    file.read_exact(&mut header)?;
    ensure!(&header[..8] == MAGIC, "unsupported compression frame");
    let length = u32::from_le_bytes(header[8..].try_into()?) as usize;
    ensure!(
        length <= MAX_INDEX && 12 + length as u64 <= reference.stored_bytes,
        "compression index length exceeds bound"
    );
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes)?;
    ensure!(
        digest(&bytes) == reference.index_digest,
        "compression index digest mismatch"
    );
    let index: Index = serde_json::from_slice(&bytes)?;
    ensure!(
        index.version == 1
            && index.codec == "lz4-block/1"
            && index.logical_digest == logical
            && index.decoded as u64 <= maximum
            && index.blocks.len() <= MAX_BLOCKS,
        "compression index identity or decoded bound mismatch"
    );
    let mut encoded = 0u64;
    let mut decoded = 0usize;
    for block in &index.blocks {
        ensure!(
            block.offset == encoded
                && (1..=BLOCK).contains(&block.decoded)
                && block.encoded > 0
                && block.encoded <= block.decoded
                && (block.compressed || block.encoded == block.decoded)
                && crate::blobs::valid_digest(&block.stored_digest)
                && crate::blobs::valid_digest(&block.logical_digest),
            "invalid compression block index"
        );
        encoded = encoded
            .checked_add(block.encoded as u64)
            .context("encoded length overflow")?;
        decoded = decoded
            .checked_add(block.decoded)
            .context("decoded length overflow")?;
    }
    let start = 12 + length as u64;
    ensure!(
        start.checked_add(encoded) == Some(reference.stored_bytes) && decoded == index.decoded,
        "compression block totals mismatch"
    );
    Ok((file, index, start))
}

/// The authenticated reference binds the index and each block; a scoped read
/// decodes only intersecting blocks, with a separate decoded-work allowance.
pub fn read_range(
    path: &Path,
    reference: &EncodedObject,
    logical: &str,
    maximum: u64,
    range: std::ops::Range<usize>,
    work_limit: usize,
) -> Result<Vec<u8>> {
    let (mut file, index, start) = open(path, reference, logical, maximum)?;
    ensure!(
        range.start <= range.end && range.end <= index.decoded,
        "compressed range outside object"
    );
    let mut result = Vec::with_capacity(range.len().min(work_limit));
    let mut position = 0usize;
    let mut work = 0usize;
    for block in &index.blocks {
        let end = position + block.decoded;
        if position < range.end && end > range.start {
            work = work
                .checked_add(block.decoded)
                .context("decode work overflow")?;
            ensure!(
                work <= work_limit,
                "compression decode work allowance exceeded"
            );
            file.seek(SeekFrom::Start(start + block.offset))?;
            let mut bytes = vec![0; block.encoded];
            file.read_exact(&mut bytes)?;
            ensure!(
                digest(&bytes) == block.stored_digest,
                "encoded block digest mismatch"
            );
            let decoded = if block.compressed {
                lz4_flex::block::decompress(&bytes, block.decoded)?
            } else {
                bytes
            };
            ensure!(
                decoded.len() == block.decoded && digest(&decoded) == block.logical_digest,
                "decoded block digest mismatch"
            );
            result.extend_from_slice(
                &decoded[range.start.saturating_sub(position)..range.end.min(end) - position],
            );
        }
        position = end;
        if position >= range.end {
            break;
        }
    }
    ensure!(result.len() == range.len(), "compressed range incomplete");
    Ok(result)
}

pub(crate) fn read_all(
    path: &Path,
    reference: &EncodedObject,
    logical: &str,
    maximum: u64,
) -> Result<Vec<u8>> {
    let (_, index, _) = open(path, reference, logical, maximum)?;
    // Full reads also verify stored-file identity, in addition to index/blocks.
    crate::blobs::read_verified(path, &reference.stored_digest, reference.stored_bytes)?;
    let decoded = read_range(
        path,
        reference,
        logical,
        maximum,
        0..index.decoded,
        index.decoded,
    )?;
    ensure!(
        digest(&decoded) == logical,
        "decoded object digest mismatch"
    );
    Ok(decoded)
}

pub(crate) fn verify(
    path: &Path,
    reference: &EncodedObject,
    logical: &str,
    maximum: u64,
) -> Result<usize> {
    let (mut file, index, _) = open(path, reference, logical, maximum)?;
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    ensure!(
        format!("{:x}", hash.finalize()) == reference.stored_digest,
        "encoded object digest mismatch"
    );
    Ok(index.decoded)
}

#[cfg(test)]
mod tests;
