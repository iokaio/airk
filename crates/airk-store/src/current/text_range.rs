// SPDX-License-Identifier: Apache-2.0
//! Ordered retained display text and original locations in bounded owned pages.
use super::{CurrentRead, map::MapDomain, metadata::FileMetadataRef, resources::*};
use crate::FileStore;
use airk_core::{
    Artifact, DocumentLocation,
    compression::CompressionMode,
    identity::{Sha256Digest, framed_digest},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
const BLOCK: usize = 65_536;
pub const MAX_HEADER: usize = 8 << 20;
const MAX_TOTAL: usize = 64 << 20;
const MAX_PAGES: usize = 65_536;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextRangeReference {
    pub root: ResourceReference,
    pub data_bytes: usize,
    pub pages: usize,
}
impl TextRangeReference {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.root.bytes <= MAX_HEADER
                && self.root.projection.is_none()
                && self
                    .root
                    .bytes
                    .checked_add(self.data_bytes)
                    .is_some_and(|bytes| bytes <= MAX_TOTAL)
                && self.pages <= MAX_PAGES
                && self.pages <= self.data_bytes,
            "text range reference exceeds bound"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    source: Sha256Digest,
    payload: Sha256Digest,
    profile: Sha256Digest,
}
fn binding(artifact: &Artifact) -> Result<Binding> {
    let retained = artifact
        .retained
        .as_ref()
        .context("text range extraction missing")?;
    Ok(Binding {
        source: artifact.content_hash.parse()?,
        payload: retained.object.parse()?,
        profile: retained
            .profile
            .as_ref()
            .context("text range profile missing")?
            .identity()?,
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TextBlock {
    newlines: usize,
    resource: ResourceReference,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TextChunk {
    start_line: usize,
    end_line: usize,
    location: DocumentLocation,
    lines: usize,
    final_newline: bool,
    bytes: usize,
    blocks: Vec<TextBlock>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    version: u32,
    binding: Binding,
    chunks: Vec<TextChunk>,
}
pub(crate) struct PreparedTextRanges {
    pub reference: TextRangeReference,
    pub resources: Vec<PreparedResource>,
    pub memory: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextRangeCertificate {
    pub version: u32,
    pub manifest: Sha256Digest,
    binding: Binding,
    pub pages: usize,
    pub data_bytes: usize,
}
impl TextRangeCertificate {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1
                && self.pages <= MAX_PAGES
                && self.pages <= self.data_bytes
                && self.data_bytes <= MAX_TOTAL,
            "invalid text range certificate"
        );
        Ok(())
    }
}
fn applicable(artifact: &Artifact) -> bool {
    !(artifact.extraction_method == "utf8"
        && artifact
            .retained
            .as_ref()
            .and_then(|r| r.profile.as_ref())
            .is_some_and(|profile| profile.contract_version == 2))
}
fn limited() -> anyhow::Error {
    super::native_projection::ProjectionPreparationLimit.into()
}
impl FileStore {
    pub(crate) fn prepare_text_ranges(
        &self,
        artifact: &Artifact,
        mode: CompressionMode,
    ) -> Result<Option<PreparedTextRanges>> {
        if !applicable(artifact) {
            return Ok(None);
        }
        let mut header = Header {
            version: 1,
            binding: binding(artifact)?,
            chunks: Vec::new(),
        };
        let mut resources = Vec::new();
        let mut data_bytes = 0usize;
        let mut metadata_bytes = 0usize;
        let mut memory = 0usize;
        for chunk in &artifact.chunks {
            let mut blocks = Vec::new();
            for text in chunk.text.as_bytes().chunks(BLOCK) {
                data_bytes = data_bytes
                    .checked_add(text.len())
                    .context("text range size overflow")?;
                if data_bytes > MAX_TOTAL || resources.len() == MAX_PAGES {
                    return Err(limited());
                }
                let resource = prepare_resource(text.to_vec(), mode)?;
                memory = memory
                    .checked_add(resource.memory_bytes())
                    .context("text preparation memory overflow")?;
                if memory > MAX_TOTAL {
                    return Err(limited());
                }
                blocks.push(TextBlock {
                    newlines: text.iter().filter(|byte| **byte == b'\n').count(),
                    resource: resource.reference.clone(),
                });
                resources.push(resource);
            }
            let row = TextChunk {
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                location: chunk.location.clone(),
                lines: chunk.text.split_inclusive('\n').count(),
                final_newline: chunk.text.ends_with('\n'),
                bytes: chunk.text.len(),
                blocks,
            };
            metadata_bytes = metadata_bytes
                .checked_add(super::native_projection::bounded_encoding(&row)?.len() + 1)
                .context("text metadata size overflow")?;
            if metadata_bytes > MAX_HEADER {
                return Err(limited());
            }
            header.chunks.push(row);
        }
        let bytes = super::native_projection::bounded_encoding(&header)?;
        if bytes.len() > MAX_HEADER || bytes.len().saturating_add(data_bytes) > MAX_TOTAL {
            return Err(limited());
        }
        let root = prepare_resource(bytes, mode)?;
        memory = memory
            .checked_add(root.memory_bytes())
            .context("text preparation memory overflow")?;
        if memory > MAX_TOTAL {
            return Err(limited());
        }
        let reference = TextRangeReference {
            root: root.reference.clone(),
            data_bytes,
            pages: resources.len(),
        };
        reference.validate()?;
        resources.push(root);
        Ok(Some(PreparedTextRanges {
            reference,
            resources,
            memory,
        }))
    }
    fn load_text_header(
        &self,
        reference: &TextRangeReference,
        artifact: &Artifact,
    ) -> Result<Header> {
        reference.validate()?;
        let bytes = self.read_current_resource(&reference.root)?;
        (|| -> Result<Header> {
            let header: Header = serde_json::from_slice(&bytes)?;
            ensure!(
                header.version == 1
                    && header.binding == binding(artifact)?
                    && applicable(artifact)
                    && serde_json::to_vec(&header)? == bytes,
                "text range header binding mismatch"
            );
            let mut pages = 0usize;
            let mut data_bytes = 0usize;
            ensure!(
                header.chunks.len() <= airk_core::extraction::MAX_EXTRACTION_CHUNKS,
                "text range chunk bound exceeded"
            );
            for chunk in &header.chunks {
                ensure!(
                    chunk.blocks.len() == chunk.bytes.div_ceil(BLOCK),
                    "text range block count mismatch"
                );
                let mut newlines = 0usize;
                for (index, block) in chunk.blocks.iter().enumerate() {
                    ensure!(
                        block.resource.bytes == (chunk.bytes - index * BLOCK).min(BLOCK)
                            && block.resource.projection.is_none()
                            && block.newlines <= block.resource.bytes,
                        "text range block bound mismatch"
                    );
                    newlines = newlines
                        .checked_add(block.newlines)
                        .context("text line count overflow")?;
                    data_bytes = data_bytes
                        .checked_add(block.resource.bytes)
                        .context("text data count overflow")?;
                    pages = pages.checked_add(1).context("text page count overflow")?;
                }
                ensure!(
                    chunk.lines == newlines + usize::from(chunk.bytes > 0 && !chunk.final_newline)
                        && (!chunk.final_newline || newlines > 0),
                    "text range line count mismatch"
                );
            }
            ensure!(
                pages == reference.pages && data_bytes == reference.data_bytes,
                "text range summary mismatch"
            );
            Ok(header)
        })()
        .context(super::PrivateIntegrityFailure)
    }
    pub(crate) fn certify_text_ranges(
        &self,
        reference: &TextRangeReference,
        artifact: &Artifact,
    ) -> Result<TextRangeCertificate> {
        let header = self.load_text_header(reference, artifact)?;
        ensure!(
            header.chunks.len() == artifact.chunks.len(),
            "text range chunk omission"
        );
        for (projected, chunk) in header.chunks.iter().zip(&artifact.chunks) {
            ensure!(
                projected.start_line == chunk.start_line
                    && projected.end_line == chunk.end_line
                    && projected.location == chunk.location
                    && projected.bytes == chunk.text.len()
                    && projected.lines == chunk.text.split_inclusive('\n').count()
                    && projected.final_newline == chunk.text.ends_with('\n'),
                "text range chunk differs from extraction"
            );
            for (block, text) in projected
                .blocks
                .iter()
                .zip(chunk.text.as_bytes().chunks(BLOCK))
            {
                ensure!(
                    block.newlines == text.iter().filter(|byte| **byte == b'\n').count()
                        && block.resource.bytes == text.len()
                        && block.resource.digest == framed_digest(b"current-resource/1", &[text]),
                    "text range page differs from extraction"
                );
            }
        }
        Ok(TextRangeCertificate {
            version: 1,
            manifest: reference.root.digest,
            binding: header.binding,
            pages: reference.pages,
            data_bytes: reference.data_bytes,
        })
    }
    pub(crate) fn verify_text_ranges(
        &self,
        reference: &TextRangeReference,
        artifact: &Artifact,
    ) -> Result<()> {
        self.certify_text_ranges(reference, artifact)?;
        let header = self.load_text_header(reference, artifact)?;
        for block in header.chunks.iter().flat_map(|chunk| &chunk.blocks) {
            self.read_current_resource(&block.resource)?;
        }
        Ok(())
    }
    pub(crate) fn retain_text_range_paths(
        &self,
        reference: &TextRangeReference,
        artifact: &Artifact,
        paths: &mut std::collections::BTreeSet<std::path::PathBuf>,
    ) -> Result<()> {
        let header = self.load_text_header(reference, artifact)?;
        paths.insert(
            std::path::PathBuf::from("current-resources")
                .join(format!("{}.json", reference.root.object_digest()?)),
        );
        for block in header.chunks.iter().flat_map(|chunk| &chunk.blocks) {
            paths.insert(
                std::path::PathBuf::from("current-resources")
                    .join(format!("{}.json", block.resource.object_digest()?)),
            );
        }
        Ok(())
    }
}
pub struct TextRangeRead<'a> {
    _read: &'a CurrentRead,
    store: &'a FileStore,
    artifact: Artifact,
    reference: TextRangeReference,
    header: Header,
    pages: Vec<(usize, usize)>,
}
pub struct TextRangeAuditTurn {
    pub next_page: usize,
    pub checked_pages: usize,
    pub checked_bytes: u64,
    pub data_bytes: u64,
    pub complete: bool,
    pub required_bytes: Option<u64>,
}
pub struct SelectedTextRange {
    pub text: String,
    pub location: DocumentLocation,
    pub truncated: bool,
}
impl CurrentRead {
    pub(crate) fn text_range_reference(&self, path: &str) -> Result<Option<TextRangeReference>> {
        let Some(bytes) = self.map_get(MapDomain::FileMetadata, path.as_bytes())? else {
            return Ok(None);
        };
        let reference: FileMetadataRef = serde_json::from_slice(&bytes)?;
        reference.validate_projection_declaration()?;
        ensure!(
            reference.path == path && serde_json::to_vec(&reference)? == bytes,
            "text range metadata mismatch"
        );
        if let Some(text) = &reference.text_ranges {
            ensure!(
                self.query_projection_version()?
                    .is_some_and(|version| version >= 4),
                "unsupported text range capability"
            );
            text.validate()?;
        }
        Ok(reference.text_ranges)
    }
    pub fn text_range_projection<'a>(
        &'a self,
        store: &'a FileStore,
        path: &str,
    ) -> Result<Option<TextRangeRead<'a>>> {
        let Some(reference) = self.text_range_reference(path)? else {
            return Ok(None);
        };
        let artifact = self
            .artifact(path)?
            .context("text range artifact missing")?;
        let header = store.load_text_header(&reference, &artifact)?;
        let pages = header
            .chunks
            .iter()
            .enumerate()
            .flat_map(|(chunk, row)| (0..row.blocks.len()).map(move |block| (chunk, block)))
            .collect();
        Ok(Some(TextRangeRead {
            _read: self,
            store,
            artifact,
            reference,
            header,
            pages,
        }))
    }
}
impl TextRangeRead<'_> {
    pub fn indexed_lines(&self) -> usize {
        self.header.chunks.last().map_or(0, |chunk| chunk.end_line)
    }
    pub fn maximum_line_end(&self) -> usize {
        self.header
            .chunks
            .iter()
            .map(|chunk| chunk.end_line)
            .max()
            .unwrap_or(0)
    }
    pub fn reference(&self) -> &TextRangeReference {
        &self.reference
    }
    pub fn check_certificate(&self, certificate: &TextRangeCertificate) -> Result<()> {
        certificate.validate()?;
        ensure!(
            certificate.manifest == self.reference.root.digest
                && certificate.binding == self.header.binding
                && certificate.pages == self.reference.pages
                && certificate.data_bytes == self.reference.data_bytes,
            "text range certificate changed"
        );
        Ok(())
    }
    pub fn audit_physical(
        &self,
        certificate: &TextRangeCertificate,
        after: usize,
        maximum_pages: usize,
        maximum_bytes: u64,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<TextRangeAuditTurn> {
        self.check_certificate(certificate)?;
        ensure!(
            after <= self.pages.len() && (1..=128).contains(&maximum_pages),
            "invalid text audit page admission"
        );
        let metadata = self.reference.root.bytes as u64;
        let mut turn = TextRangeAuditTurn {
            next_page: after,
            checked_pages: 0,
            checked_bytes: metadata,
            data_bytes: 0,
            complete: after == self.pages.len(),
            required_bytes: None,
        };
        if metadata > maximum_bytes {
            turn.checked_bytes = 0;
            turn.required_bytes = Some(metadata);
            return Ok(turn);
        }
        while turn.next_page < self.pages.len() && turn.checked_pages < maximum_pages {
            checkpoint()?;
            let (chunk, block) = self.pages[turn.next_page];
            let reference = &self.header.chunks[chunk].blocks[block].resource;
            if turn.checked_bytes.saturating_add(reference.bytes as u64) > maximum_bytes {
                turn.required_bytes = Some(metadata + reference.bytes as u64);
                break;
            }
            self.store.read_current_resource(reference)?;
            turn.checked_bytes += reference.bytes as u64;
            turn.data_bytes += reference.bytes as u64;
            turn.checked_pages += 1;
            turn.next_page += 1;
            checkpoint()?;
        }
        turn.complete = turn.next_page == self.pages.len();
        Ok(turn)
    }
    pub fn read_strict(
        &self,
        start: usize,
        end: usize,
        maximum: usize,
    ) -> Result<SelectedTextRange> {
        self._read.require_path(&self.artifact.path)?;
        self.store.verify_retained_source_deep(&self.artifact)?;
        let artifact = self.store.load_projection_artifact(&self.artifact)?;
        self.store.verify_text_ranges(&self.reference, &artifact)?;
        self.read_selected(start, end, maximum)
    }
    /// Materialize one original extraction chunk for an admitted lexical
    /// candidate. Reading by ordinal preserves overlapping/document locations;
    /// line-range concatenation would duplicate text from neighboring chunks.
    pub fn chunk_private(&self, ordinal: usize) -> Result<airk_core::Chunk> {
        self._read.require_path(&self.artifact.path)?;
        ensure!(
            self._read.private_query_eligible()?,
            "private query authority unavailable"
        );
        let chunk = self
            .header
            .chunks
            .get(ordinal)
            .context("text range chunk unavailable")?;
        let result = (|| -> Result<airk_core::Chunk> {
            let mut bytes = Vec::with_capacity(chunk.bytes);
            for block in &chunk.blocks {
                bytes.extend(self.store.read_current_resource(&block.resource)?);
            }
            ensure!(
                bytes.len() == chunk.bytes,
                anyhow::Error::new(super::PrivateIntegrityFailure)
                    .context("text range chunk byte mismatch")
            );
            let text = String::from_utf8(bytes).context(super::PrivateIntegrityFailure)?;
            Ok(airk_core::Chunk {
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                text,
                location: chunk.location.clone(),
            })
        })();
        let chunk = self._read.observe_private_integrity(result)?;
        self._read.token()?;
        Ok(chunk)
    }
    /// Read selected authenticated pages only under this exact read's sealed owner policy.
    pub fn read_private(
        &self,
        start: usize,
        end: usize,
        maximum: usize,
    ) -> Result<SelectedTextRange> {
        ensure!(
            self._read.private_query_eligible()?,
            "private query authority unavailable"
        );
        airk_core::validate_budget(1, maximum)?;
        ensure!(start > 0 && end >= start, "invalid line range");
        ensure!(
            end <= self.indexed_lines(),
            "line range exceeds indexed source"
        );
        // A cross-location request is a caller error, not damaged storage.
        self.request_location(start, end)?;
        let selected = self
            ._read
            .observe_private_integrity(self.read_selected(start, end, maximum))?;
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
        maximum: usize,
    ) -> Result<SelectedTextRange> {
        self._read.require_path(&self.artifact.path)?;
        airk_core::validate_budget(1, maximum)?;
        ensure!(start > 0 && end >= start, "invalid line range");
        ensure!(
            end <= self.indexed_lines(),
            "line range exceeds indexed source"
        );
        let location = self.request_location(start, end)?;
        let mut text = String::new();
        let mut truncated = false;
        for chunk in &self.header.chunks {
            if chunk.lines == 0 || chunk.start_line > end {
                continue;
            }
            let first = start.saturating_sub(chunk.start_line);
            let last = (end - chunk.start_line).min(chunk.lines - 1);
            if first > last {
                continue;
            }
            let mut cache = BTreeMap::new();
            let begin = self.after_newline(chunk, first, &mut cache)?;
            let finish = if last + 1 == chunk.lines && !chunk.final_newline {
                chunk.bytes
            } else {
                self.after_newline(chunk, last + 1, &mut cache)?
            };
            let allowance = maximum - text.len();
            let through = begin.saturating_add(allowance + 3).min(finish);
            let mut bytes = Vec::with_capacity(through - begin);
            let mut offset = begin;
            while offset < through {
                let block = self.block(chunk, offset / BLOCK, &mut cache)?;
                let from = offset % BLOCK;
                let take = (through - offset).min(block.len() - from);
                bytes.extend_from_slice(&block[from..from + take]);
                offset += take;
            }
            let valid = match std::str::from_utf8(&bytes) {
                Ok(text) => text,
                Err(error) if error.error_len().is_none() && through < finish => {
                    std::str::from_utf8(&bytes[..error.valid_up_to()])?
                }
                Err(error) => {
                    return Err(anyhow::Error::new(error).context(super::PrivateIntegrityFailure));
                }
            };
            let mut length = valid.len().min(allowance);
            while !valid.is_char_boundary(length) {
                length -= 1;
            }
            text.push_str(&valid[..length]);
            if finish - begin > length {
                truncated = true;
                break;
            }
        }
        if self._read.read_scope_fingerprint().is_some() {
            self._read.token()?;
        }
        Ok(SelectedTextRange {
            text,
            location,
            truncated,
        })
    }
    fn request_location(&self, start: usize, end: usize) -> Result<DocumentLocation> {
        let location = if self.artifact.extraction_method == "utf8" {
            DocumentLocation::Lines { start, end }
        } else {
            let mut locations = self
                .header
                .chunks
                .iter()
                .filter(|chunk| chunk.end_line >= start && chunk.start_line <= end)
                .map(|chunk| &chunk.location);
            let first = locations.next().context("document location unavailable")?;
            ensure!(
                locations.all(|location| location == first),
                "range crosses document locations; request one page/block/slide at a time"
            );
            first.clone()
        };
        Ok(location)
    }
    fn block<'b>(
        &self,
        chunk: &TextChunk,
        index: usize,
        cache: &'b mut BTreeMap<usize, Vec<u8>>,
    ) -> Result<&'b [u8]> {
        self._read.require_path(&self.artifact.path)?;
        if let std::collections::btree_map::Entry::Vacant(entry) = cache.entry(index) {
            let block = chunk
                .blocks
                .get(index)
                .context("text range block missing")?;
            let bytes = self.store.read_current_resource(&block.resource)?;
            ensure!(
                bytes.iter().filter(|byte| **byte == b'\n').count() == block.newlines,
                anyhow::Error::new(super::PrivateIntegrityFailure)
                    .context("text page newline mismatch")
            );
            entry.insert(bytes);
        }
        cache
            .get(&index)
            .map(Vec::as_slice)
            .context("text range page cache missing")
    }
    fn after_newline(
        &self,
        chunk: &TextChunk,
        mut rank: usize,
        cache: &mut BTreeMap<usize, Vec<u8>>,
    ) -> Result<usize> {
        if rank == 0 {
            return Ok(0);
        }
        for (index, block) in chunk.blocks.iter().enumerate() {
            if rank > block.newlines {
                rank -= block.newlines;
                continue;
            }
            for (offset, byte) in self.block(chunk, index, cache)?.iter().enumerate() {
                if *byte == b'\n' {
                    rank -= 1;
                    if rank == 0 {
                        return Ok(index * BLOCK + offset + 1);
                    }
                }
            }
            anyhow::bail!(
                anyhow::Error::new(super::PrivateIntegrityFailure)
                    .context("text range line boundary missing")
            );
        }
        anyhow::bail!(
            anyhow::Error::new(super::PrivateIntegrityFailure)
                .context("text range line boundary outside chunk")
        )
    }
}
