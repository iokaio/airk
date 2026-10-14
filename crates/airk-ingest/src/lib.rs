// SPDX-License-Identifier: Apache-2.0
//! Native code/document extraction and UTF-8 fallback, with explicit exclusions.
use airk_core::{Artifact, Chunk, DocumentLocation, EXTRACTOR_VERSION, Snapshot};
use anyhow::{Context, Result, ensure};
use ignore::WalkBuilder;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};

pub const MAX_FILE_BYTES: usize = 1_048_576;
pub const MAX_CORPUS_BYTES: usize = 67_108_864;
pub const MAX_FILES: usize = 10_000;
mod documents;
mod prose;
pub mod semantic;
pub mod syntax;

#[derive(Default, Debug, Serialize)]
pub struct IndexReport {
    pub generation: u64,
    pub collection: String,
    pub indexed: usize,
    pub reused: usize,
    pub removed: usize,
    pub skipped: Vec<SkippedFile>,
}

#[derive(Debug, Serialize)]
pub struct SkippedFile {
    pub path: String,
    pub reason: String,
}

pub fn scan(root: &Path, data_dir: &Path, previous: &Snapshot) -> Result<(Snapshot, IndexReport)> {
    let root = root.canonicalize().context("resolve workspace root")?;
    let data_dir = data_dir.canonicalize().context("resolve index directory")?;
    let mut report = IndexReport {
        collection: previous.collection.clone(),
        ..Default::default()
    };
    let mut artifacts = BTreeMap::new();
    let mut total_bytes = 0usize;
    let walker = WalkBuilder::new(&root)
        .require_git(false)
        .hidden(false)
        .follow_links(false)
        .add_custom_ignore_filename(".airkignore")
        .filter_entry(move |entry| {
            let name = entry.file_name().to_string_lossy();
            entry.depth() == 0 || (!excluded(&name) && !entry.path().starts_with(&data_dir))
        })
        .build();
    for entry in walker {
        let entry = entry.context("workspace discovery failed; previous generation retained")?;
        let Some(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_file() {
            continue;
        }
        let path = entry
            .path()
            .strip_prefix(&root)?
            .to_str()
            .context("non-UTF-8 source path is unsupported")?
            .replace('\\', "/");
        if path.len() > 4096 {
            report.skipped.push(SkippedFile {
                path,
                reason: "path exceeds 4096 bytes".into(),
            });
            continue;
        }
        // Recheck links and containment before opening; do not intentionally follow links.
        ensure!(
            !entry.path().symlink_metadata()?.file_type().is_symlink(),
            "source changed to symlink during scan"
        );
        ensure!(
            entry.path().canonicalize()?.starts_with(&root),
            "source escaped workspace"
        );
        let mut bytes = Vec::new();
        File::open(entry.path())?
            .take((MAX_FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_FILE_BYTES {
            report.skipped.push(SkippedFile {
                path,
                reason: "file exceeds 1 MiB".into(),
            });
            continue;
        }
        total_bytes += bytes.len();
        ensure!(
            total_bytes <= MAX_CORPUS_BYTES,
            "corpus exceeds 64 MiB scaffold limit; previous generation retained"
        );
        ensure!(
            artifacts.len() < MAX_FILES,
            "corpus exceeds 10000 files; previous generation retained"
        );
        let content_hash = format!("{:x}", Sha256::digest(&bytes));
        let artifact = match previous.artifacts.get(&path) {
            Some(old)
                if old.content_hash == content_hash
                    && previous.extractor_version == EXTRACTOR_VERSION =>
            {
                report.reused += 1;
                old.clone()
            }
            _ => {
                let extracted = match documents::extract(&path, &bytes) {
                    Ok(extracted) => extracted,
                    Err(error) => {
                        report.skipped.push(SkippedFile {
                            path,
                            reason: error.to_string(),
                        });
                        continue;
                    }
                };
                report.indexed += 1;
                Artifact {
                    path: path.clone(),
                    content_hash,
                    bytes: bytes.len(),
                    chunks: extracted.chunks,
                    language: extracted.language,
                    extraction_method: extracted.method,
                    parse_status: extracted.parse_status,
                    facts: extracted.facts,
                }
            }
        };
        artifacts.insert(path, artifact);
    }
    report.removed = previous
        .artifacts
        .keys()
        .filter(|path| !artifacts.contains_key(*path))
        .count();
    let changed =
        report.indexed > 0 || report.removed > 0 || previous.extractor_version != EXTRACTOR_VERSION;
    report.generation = if changed {
        previous
            .generation
            .checked_add(1)
            .context("generation overflow")?
    } else {
        previous.generation
    };
    let mut next = Snapshot::empty(previous.root.clone());
    next.collection = previous.collection.clone();
    next.generation = report.generation;
    next.artifacts = artifacts;
    if !changed && let Some(manifest) = &previous.analysis {
        if semantic::validate_build_inputs(&root, manifest).is_ok() {
            next.analysis = previous.analysis.clone();
            next.imported_facts = previous.imported_facts.clone();
            next.imported_relationships = previous.imported_relationships.clone();
        } else {
            next.generation = next
                .generation
                .checked_add(1)
                .context("generation overflow")?;
            report.generation = next.generation;
        }
    }
    Ok((next, report))
}

fn excluded(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        ".git"
            | ".airk"
            | "node_modules"
            | "target"
            | ".venv"
            | "__pycache__"
            | ".aws"
            | ".ssh"
            | ".env"
    ) || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
}

pub fn chunk(text: &str) -> Vec<Chunk> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    lines
        .chunks(40)
        .enumerate()
        .map(|(i, lines)| Chunk {
            start_line: i * 40 + 1,
            end_line: i * 40 + lines.len(),
            text: lines.concat(),
            location: DocumentLocation::Lines {
                start: i * 40 + 1,
                end: i * 40 + lines.len(),
            },
        })
        .collect()
}
