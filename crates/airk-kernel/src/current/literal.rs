// SPDX-License-Identifier: Apache-2.0
//! Exact literal matches in one admitted current publication's retained text.

use super::runtime::revision;
use crate::Kernel;
use airk_store::current::{CurrentRead, postings::Lookup};
use anyhow::{Context, Result, ensure};

const PAGE_FILES: usize = 64;
const MAX_INVENTORY_CHECKPOINTS: usize = 32_768;
const SEARCH_WINDOW_BYTES: usize = 16_384;

#[derive(Clone, Copy, Debug)]
pub(crate) struct LiteralLimits {
    pub(crate) max_files: usize,
    pub(crate) max_bytes: usize,
    pub(crate) max_hits: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LiteralStop {
    #[default]
    Complete,
    FileLimit,
    ByteLimit,
    HitLimit,
    Interrupted,
    UnsupportedRetainedText,
    Stale,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LiteralHit {
    pub(crate) path: String,
    pub(crate) content_hash: String,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) start_byte: usize,
    pub(crate) end_byte: usize,
}

#[derive(Debug, Default)]
pub(crate) struct LiteralScan {
    pub(crate) files_examined: usize,
    pub(crate) bytes_examined: usize,
    pub(crate) retained_bytes_loaded: usize,
    pub(crate) files_excluded: usize,
    pub(crate) hits: Vec<LiteralHit>,
    pub(crate) stop: LiteralStop,
}

#[derive(Debug)]
struct InventoryStopped(LiteralStop);

impl std::fmt::Display for InventoryStopped {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "literal inventory stopped: {:?}", self.0)
    }
}
impl std::error::Error for InventoryStopped {}

impl Kernel {
    pub(crate) fn scan_retained_literal(
        &self,
        literal: &str,
        prefix: &str,
        expected_revision: &str,
        limits: LiteralLimits,
        mut checkpoint: impl FnMut() -> Result<bool>,
    ) -> Result<LiteralScan> {
        ensure!(
            !literal.is_empty() && literal.len() <= 1024 && !literal.contains('\0'),
            "invalid retained literal length or content"
        );
        airk_core::index_catalog::relative_path(
            prefix.strip_suffix('/').unwrap_or(prefix),
            prefix.is_empty(),
        )?;
        ensure!(
            !prefix.chars().any(char::is_control),
            "invalid retained literal path prefix"
        );
        ensure!(
            (1..=4096).contains(&limits.max_files)
                && (1..=16 * 1024 * 1024).contains(&limits.max_bytes)
                && (1..=256).contains(&limits.max_hits),
            "invalid retained literal limits"
        );

        let read = self.current_reader()?;
        let pinned = read.token()?;
        let stale = || LiteralScan {
            stop: LiteralStop::Stale,
            ..LiteralScan::default()
        };
        if revision(pinned).to_string() != expected_revision {
            return Ok(stale());
        }
        let mut scan = LiteralScan::default();
        scan.stop = scan_publication(&read, literal, prefix, limits, &mut checkpoint, &mut scan)?;
        read.require_token(&pinned)?;
        drop(read);
        let current = self.current_reader()?;
        if current.token()? != pinned {
            return Ok(stale());
        }
        Ok(scan)
    }
}

fn scan_publication(
    read: &CurrentRead,
    literal: &str,
    prefix: &str,
    limits: LiteralLimits,
    checkpoint: &mut impl FnMut() -> Result<bool>,
    result: &mut LiteralScan,
) -> Result<LiteralStop> {
    let mut after = None;
    let mut inventory_checkpoints = 0;
    loop {
        let remaining = limits
            .max_files
            .saturating_sub(result.files_examined + result.files_excluded);
        if remaining == 0 {
            return Ok(LiteralStop::FileLimit);
        }
        let page = read.lookup_paths_filtered_checked(
            Lookup::Files,
            prefix,
            after.as_deref(),
            remaining.min(PAGE_FILES),
            |_| Ok(true),
            || {
                if !checkpoint()? {
                    return Err(InventoryStopped(LiteralStop::Interrupted).into());
                }
                inventory_checkpoints += 1;
                if inventory_checkpoints > MAX_INVENTORY_CHECKPOINTS {
                    return Err(InventoryStopped(LiteralStop::FileLimit).into());
                }
                Ok(())
            },
        );
        let page = match page {
            Ok(page) => page,
            Err(error) => match error.downcast_ref::<InventoryStopped>() {
                Some(stop) => return Ok(stop.0),
                None => return Err(error),
            },
        };
        if after.is_none() && page.paths.is_empty() && !prefix.is_empty() {
            // The selector is allowed to omit paths outside the grant. A
            // directly inaccessible prefix must not look like an empty scan.
            read.require_path(prefix.trim_end_matches('/'))?;
        }
        for path in page.paths {
            if !checkpoint()? {
                return Ok(LiteralStop::Interrupted);
            }
            let artifact = read
                .artifact(&path)?
                .context("literal inventory path missing")?;
            if !read.retained_source_available(&artifact) {
                result.files_excluded += 1;
                continue;
            }
            let object_bytes = artifact
                .retained
                .as_ref()
                .context("literal inventory retained reference missing")?
                .object_bytes;
            let remaining = limits.max_bytes - result.retained_bytes_loaded;
            if object_bytes > remaining || artifact.bytes > limits.max_bytes - result.bytes_examined
            {
                return Ok(LiteralStop::ByteLimit);
            }
            let source = read.read_retained_source_bounded(&artifact, remaining as u64)?;
            result.retained_bytes_loaded += object_bytes;
            result.files_examined += 1;
            let text = std::str::from_utf8(&source)?;
            let (examined, stop) = scan_text(text, literal, checkpoint, |position| {
                if result.hits.len() == limits.max_hits {
                    return false;
                }
                result.hits.push(LiteralHit {
                    path: path.clone(),
                    content_hash: artifact.content_hash.clone(),
                    start_line: position.start_line,
                    end_line: position.end_line,
                    start_byte: position.start_byte,
                    end_byte: position.end_byte,
                });
                true
            })?;
            result.bytes_examined += examined;
            if stop != LiteralStop::Complete {
                return Ok(stop);
            }
        }
        after = page.next_after;
        if after.is_none() {
            return Ok(if result.files_excluded == 0 {
                LiteralStop::Complete
            } else {
                LiteralStop::UnsupportedRetainedText
            });
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct MatchPosition {
    start_line: usize,
    end_line: usize,
    start_byte: usize,
    end_byte: usize,
}

fn newline_count(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&byte| byte == b'\n').count()
}

fn scan_text(
    text: &str,
    literal: &str,
    checkpoint: &mut impl FnMut() -> Result<bool>,
    mut hit: impl FnMut(MatchPosition) -> bool,
) -> Result<(usize, LiteralStop)> {
    let mut offset = 0;
    let mut examined = 0;
    let mut line = 1;
    let lines_in_match = newline_count(literal.as_bytes());
    let trailing_newline = usize::from(literal.ends_with('\n'));
    while offset < text.len() {
        if !checkpoint()? {
            return Ok((examined, LiteralStop::Interrupted));
        }
        let boundary = text.floor_char_boundary((offset + SEARCH_WINDOW_BYTES).min(text.len()));
        // Examine far enough for a literal beginning just before the boundary
        // to finish in this window. The next window starts at the boundary.
        let end = text.ceil_char_boundary((boundary + literal.len() - 1).min(text.len()));
        if let Some(found) = text[offset..end].find(literal) {
            let start = offset + found;
            line += newline_count(&text.as_bytes()[offset..start]);
            let end = start + literal.len();
            examined = examined.max(end);
            if !hit(MatchPosition {
                start_line: line,
                end_line: line + lines_in_match - trailing_newline,
                start_byte: start,
                end_byte: end,
            }) {
                return Ok((examined, LiteralStop::HitLimit));
            }
            line += lines_in_match;
            offset = end;
        } else {
            examined = examined.max(end);
            line += newline_count(&text.as_bytes()[offset..boundary]);
            offset = boundary;
        }
    }
    Ok((examined, LiteralStop::Complete))
}

#[cfg(test)]
mod tests;
