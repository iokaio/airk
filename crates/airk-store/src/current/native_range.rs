// SPDX-License-Identifier: Apache-2.0
//! Sparse native line boundaries over the original immutable retained source.
use super::{CurrentRead, map::MapDomain, metadata::FileMetadataRef, resources::*};
use crate::FileStore;
use airk_core::{Artifact, compression::CompressionMode, identity::Sha256Digest};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const SOURCE_BLOCK_BYTES: usize = 65_536;
pub const MAX_LINE_INDEX_BYTES: usize = 1 << 20;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Block {
    bytes: usize,
    newlines: usize,
    sha256: Sha256Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeLineIndex {
    version: u32,
    source: Sha256Digest,
    payload: Sha256Digest,
    profile: Sha256Digest,
    bytes: usize,
    lines: usize,
    final_newline: bool,
    blocks: Vec<Block>,
}
impl NativeLineIndex {
    pub fn lines(&self) -> usize {
        self.lines
    }
    fn validate(&self, artifact: &Artifact) -> Result<()> {
        let retained = artifact
            .retained
            .as_ref()
            .context("line index extraction missing")?;
        ensure!(
            supported(artifact)
                && self.version == 1
                && self.source.to_string() == artifact.content_hash
                && self.payload.to_string() == retained.object
                && self.profile
                    == retained
                        .profile
                        .as_ref()
                        .context("line index profile missing")?
                        .identity()?
                && self.bytes == artifact.bytes
                && self.bytes <= airk_core::extraction::MAX_SOURCE_BYTES
                && self.blocks.len() == self.bytes.div_ceil(SOURCE_BLOCK_BYTES),
            "native line index binding mismatch"
        );
        let mut newlines = 0usize;
        for (ordinal, block) in self.blocks.iter().enumerate() {
            ensure!(
                block.bytes == (self.bytes - ordinal * SOURCE_BLOCK_BYTES).min(SOURCE_BLOCK_BYTES)
                    && block.newlines <= block.bytes,
                "invalid native line index block"
            );
            newlines = newlines
                .checked_add(block.newlines)
                .context("line index count overflow")?;
        }
        ensure!(
            self.lines == newlines + usize::from(self.bytes > 0 && !self.final_newline)
                && (!self.final_newline || newlines > 0),
            "native line index count mismatch"
        );
        Ok(())
    }
}
fn supported(artifact: &Artifact) -> bool {
    artifact.extraction_method == "utf8"
        && artifact
            .retained
            .as_ref()
            .and_then(|r| r.profile.as_ref())
            .is_some_and(|p| p.contract_version == 2)
}
#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    hash: Sha256,
    bytes: usize,
    newlines: usize,
    last: Option<u8>,
}
impl Builder {
    fn push(&mut self, mut bytes: &[u8]) -> Result<()> {
        while !bytes.is_empty() {
            let take = (SOURCE_BLOCK_BYTES - self.bytes).min(bytes.len());
            let part = &bytes[..take];
            self.hash.update(part);
            self.bytes += take;
            self.newlines += part.iter().filter(|byte| **byte == b'\n').count();
            self.last = part.last().copied();
            bytes = &bytes[take..];
            if self.bytes == SOURCE_BLOCK_BYTES {
                self.flush()?;
            }
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        if self.bytes == 0 {
            return Ok(());
        }
        ensure!(
            self.blocks.len()
                < airk_core::extraction::MAX_SOURCE_BYTES.div_ceil(SOURCE_BLOCK_BYTES),
            "native line index exceeds source block bound"
        );
        self.blocks.push(Block {
            bytes: self.bytes,
            newlines: self.newlines,
            sha256: Sha256Digest::from_bytes(std::mem::take(&mut self.hash).finalize().into()),
        });
        self.bytes = 0;
        self.newlines = 0;
        Ok(())
    }
}
impl FileStore {
    fn derive_native_line_index(&self, artifact: &Artifact) -> Result<NativeLineIndex> {
        ensure!(
            supported(artifact),
            "native line projection is not applicable"
        );
        let mut builder = Builder::default();
        let lines =
            self.verify_retained_source_utf8_observed(artifact, &mut |bytes| builder.push(bytes))?;
        builder.flush()?;
        let retained = artifact
            .retained
            .as_ref()
            .context("line projection extraction missing")?;
        let index = NativeLineIndex {
            version: 1,
            source: artifact.content_hash.parse()?,
            payload: retained.object.parse()?,
            profile: retained
                .profile
                .as_ref()
                .context("line projection profile missing")?
                .identity()?,
            bytes: artifact.bytes,
            lines,
            final_newline: builder.last == Some(b'\n'),
            blocks: builder.blocks,
        };
        index.validate(artifact)?;
        Ok(index)
    }
    pub(crate) fn prepare_native_line_projection(
        &self,
        artifact: &Artifact,
        mode: CompressionMode,
    ) -> Result<Option<PreparedResource>> {
        if !supported(artifact) {
            return Ok(None);
        }
        let index = self.derive_native_line_index(artifact)?;
        let bytes = super::native_projection::bounded_encoding(&index)?;
        ensure!(
            bytes.len() <= MAX_LINE_INDEX_BYTES,
            "native line index exceeds representation bound"
        );
        Ok(Some(prepare_resource(bytes, mode)?))
    }
    fn load_native_line_index(
        &self,
        reference: &ResourceReference,
        artifact: &Artifact,
    ) -> Result<NativeLineIndex> {
        ensure!(
            reference.bytes <= MAX_LINE_INDEX_BYTES && reference.projection.is_none(),
            "native line index reference exceeds bound"
        );
        let bytes = self.read_current_resource(reference)?;
        (|| -> Result<NativeLineIndex> {
            let index: NativeLineIndex = serde_json::from_slice(&bytes)?;
            ensure!(
                serde_json::to_vec(&index)? == bytes,
                "noncanonical native line index"
            );
            index.validate(artifact)?;
            Ok(index)
        })()
        .context(super::PrivateIntegrityFailure)
    }
    /// Logical/source proof and physical verification of the entire bounded index
    /// occur together. There are no separately resumable line-index leaf objects.
    pub(crate) fn verify_native_line_projection(
        &self,
        reference: &ResourceReference,
        artifact: &Artifact,
    ) -> Result<usize> {
        let actual = self.load_native_line_index(reference, artifact)?;
        let expected = self.derive_native_line_index(artifact)?;
        ensure!(
            actual == expected,
            "native line index differs from original source"
        );
        Ok(actual.lines)
    }
}

pub struct NativeLineRead<'a> {
    _read: &'a CurrentRead,
    store: &'a FileStore,
    artifact: Artifact,
    reference: ResourceReference,
    index: NativeLineIndex,
}
#[derive(Debug)]
pub struct SelectedSourceLines {
    pub text: String,
    pub truncated: bool,
}
impl CurrentRead {
    pub(crate) fn native_line_reference(&self, path: &str) -> Result<Option<ResourceReference>> {
        let Some(bytes) = self.map_get(MapDomain::FileMetadata, path.as_bytes())? else {
            return Ok(None);
        };
        let reference: FileMetadataRef = serde_json::from_slice(&bytes)?;
        reference.validate_projection_declaration()?;
        ensure!(
            reference.path == path && serde_json::to_vec(&reference)? == bytes,
            "native line metadata reference mismatch"
        );
        if let Some(lines) = &reference.native_lines {
            ensure!(
                self.query_projection_version()?
                    .is_some_and(|version| version >= 3)
                    && reference.scalars.is_some()
                    && lines.bytes <= MAX_LINE_INDEX_BYTES,
                "native line index lacks its scalar capability"
            );
        }
        Ok(reference.native_lines)
    }
    /// Selecting this authenticated index is navigation, not authority to skip
    /// full integrity checks. Public extraction through it stays strict.
    pub fn native_line_projection<'a>(
        &'a self,
        store: &'a FileStore,
        path: &str,
    ) -> Result<Option<NativeLineRead<'a>>> {
        let Some(reference) = self.native_line_reference(path)? else {
            return Ok(None);
        };
        let artifact = self
            .artifact(path)?
            .context("native line artifact missing")?;
        let index = store.load_native_line_index(&reference, &artifact)?;
        Ok(Some(NativeLineRead {
            _read: self,
            store,
            artifact,
            reference,
            index,
        }))
    }
}
impl NativeLineRead<'_> {
    pub fn lines(&self) -> usize {
        self.index.lines
    }
    pub fn read_strict(
        &self,
        start: usize,
        end: usize,
        max_bytes: usize,
    ) -> Result<SelectedSourceLines> {
        self._read.require_path(&self.artifact.path)?;
        self.store.load_retained(&self.artifact)?;
        self.store
            .verify_native_line_projection(&self.reference, &self.artifact)?;
        self.read_selected(start, end, max_bytes)
    }
    /// Read selected authenticated pages only under this exact read's sealed owner policy.
    pub fn read_private(
        &self,
        start: usize,
        end: usize,
        max_bytes: usize,
    ) -> Result<SelectedSourceLines> {
        ensure!(
            self._read.private_query_eligible()?,
            "private query authority unavailable"
        );
        airk_core::validate_budget(1, max_bytes)?;
        ensure!(start > 0 && end >= start, "invalid line range");
        ensure!(end <= self.index.lines, "line range exceeds indexed source");
        let selected = self
            ._read
            .observe_private_integrity(self.read_selected(start, end, max_bytes))?;
        ensure!(
            self._read.private_query_eligible()?,
            "private query authority revoked"
        );
        Ok(selected)
    }
    pub(super) fn read_selected(
        &self,
        start: usize,
        end: usize,
        max_bytes: usize,
    ) -> Result<SelectedSourceLines> {
        self._read.require_path(&self.artifact.path)?;
        airk_core::validate_budget(1, max_bytes)?;
        ensure!(start > 0 && end >= start, "invalid line range");
        ensure!(end <= self.index.lines, "line range exceeds indexed source");
        let mut cache = BTreeMap::new();
        let begin = self.after_newline(start - 1, &mut cache)?;
        let finish = if end == self.index.lines && !self.index.final_newline {
            self.index.bytes
        } else {
            self.after_newline(end, &mut cache)?
        };
        ensure!(begin <= finish, "native line boundary order mismatch");
        let through = begin.saturating_add(max_bytes + 3).min(finish);
        let mut selected = Vec::with_capacity(through - begin);
        let mut offset = begin;
        while offset < through {
            let ordinal = offset / SOURCE_BLOCK_BYTES;
            let block = self.block(ordinal, &mut cache)?;
            let from = offset % SOURCE_BLOCK_BYTES;
            let take = (through - offset).min(block.len() - from);
            selected.extend_from_slice(&block[from..from + take]);
            offset += take;
        }
        let valid = match std::str::from_utf8(&selected) {
            Ok(text) => text,
            Err(error) if error.error_len().is_none() && through < finish => {
                std::str::from_utf8(&selected[..error.valid_up_to()])?
            }
            Err(error) => {
                return Err(anyhow::Error::new(error).context(super::PrivateIntegrityFailure));
            }
        };
        let mut length = valid.len().min(max_bytes);
        while !valid.is_char_boundary(length) {
            length -= 1;
        }
        if self._read.read_scope_fingerprint().is_some() {
            self._read.token()?;
        }
        Ok(SelectedSourceLines {
            text: valid[..length].into(),
            truncated: finish - begin > length,
        })
    }
    fn block<'b>(
        &self,
        ordinal: usize,
        cache: &'b mut BTreeMap<usize, Vec<u8>>,
    ) -> Result<&'b [u8]> {
        self._read.require_path(&self.artifact.path)?;
        if let std::collections::btree_map::Entry::Vacant(entry) = cache.entry(ordinal) {
            let block = self
                .index
                .blocks
                .get(ordinal)
                .context("native line block missing")?;
            let begin = ordinal * SOURCE_BLOCK_BYTES;
            let bytes = self.store.read_retained_source_block(
                &self.artifact,
                begin..begin + block.bytes,
                block.sha256,
            )?;
            ensure!(
                bytes.iter().filter(|byte| **byte == b'\n').count() == block.newlines,
                anyhow::Error::new(super::PrivateIntegrityFailure)
                    .context("native line block newline mismatch")
            );
            entry.insert(bytes);
        }
        cache
            .get(&ordinal)
            .map(Vec::as_slice)
            .context("native line block cache missing")
    }
    fn after_newline(
        &self,
        mut rank: usize,
        cache: &mut BTreeMap<usize, Vec<u8>>,
    ) -> Result<usize> {
        if rank == 0 {
            return Ok(0);
        }
        for (ordinal, block) in self.index.blocks.iter().enumerate() {
            if rank > block.newlines {
                rank -= block.newlines;
                continue;
            }
            let bytes = self.block(ordinal, cache)?;
            for (offset, byte) in bytes.iter().enumerate() {
                if *byte == b'\n' {
                    rank -= 1;
                    if rank == 0 {
                        return Ok(ordinal * SOURCE_BLOCK_BYTES + offset + 1);
                    }
                }
            }
            anyhow::bail!(
                anyhow::Error::new(super::PrivateIntegrityFailure)
                    .context("native line boundary missing")
            );
        }
        anyhow::bail!("native line boundary exceeds indexed source")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maximum_native_source_uses_bounded_block_metadata_even_for_dense_newlines() -> Result<()> {
        let digest = Sha256Digest::from_bytes([255; 32]);
        let bytes = airk_core::extraction::MAX_SOURCE_BYTES;
        let index = NativeLineIndex {
            version: 1,
            source: digest,
            payload: digest,
            profile: digest,
            bytes,
            lines: bytes,
            final_newline: true,
            blocks: vec![
                Block {
                    bytes: SOURCE_BLOCK_BYTES,
                    newlines: SOURCE_BLOCK_BYTES,
                    sha256: digest
                };
                bytes.div_ceil(SOURCE_BLOCK_BYTES)
            ],
        };
        assert_eq!(index.blocks.len(), 4096);
        assert!(serde_json::to_vec(&index)?.len() <= MAX_LINE_INDEX_BYTES);
        Ok(())
    }
}
