// SPDX-License-Identifier: Apache-2.0
//! Native code/document extraction and UTF-8 fallback, with explicit exclusions.
use airk_core::{Artifact, Chunk, DocumentLocation, EXTRACTOR_VERSION, Snapshot};
use anyhow::{Context, Result, ensure};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};

pub mod adapters;

pub const MAX_FILE_BYTES: usize = 16_777_216;
pub const MAX_CORPUS_BYTES: usize = 67_108_864;
pub const MAX_EXTRACTED_CORPUS_BYTES: usize = 134_217_728;
pub const MAX_FILES: usize = 10_000;
pub const MAX_DISCOVERY_ENTRIES: usize = 100_000;
pub const MAX_SCAN_READ_BYTES: usize = 268_435_456;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanLimits {
    pub file_bytes: usize,
    pub corpus_bytes: usize,
    pub extracted_bytes: usize,
    pub files: usize,
    pub discovery_entries: usize,
    pub read_bytes: usize,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            file_bytes: MAX_FILE_BYTES,
            corpus_bytes: MAX_CORPUS_BYTES,
            extracted_bytes: MAX_EXTRACTED_CORPUS_BYTES,
            files: MAX_FILES,
            discovery_entries: MAX_DISCOVERY_ENTRIES,
            read_bytes: MAX_SCAN_READ_BYTES,
        }
    }
}

impl ScanLimits {
    pub fn validate(self) -> Result<()> {
        ensure!(
            (1..=MAX_FILE_BYTES).contains(&self.file_bytes),
            "file limit must be 1..16 MiB"
        );
        ensure!(
            (1..=64 * 1024 * 1024 * 1024usize).contains(&self.corpus_bytes),
            "corpus limit must be 1 byte..64 GiB"
        );
        ensure!(
            (1..=128 * 1024 * 1024 * 1024usize).contains(&self.extracted_bytes),
            "extracted limit must be 1 byte..128 GiB"
        );
        ensure!(
            (1..=1_000_000).contains(&self.files),
            "file count limit must be 1..1000000"
        );
        ensure!(
            (1..=10_000_000).contains(&self.discovery_entries),
            "discovery limit must be 1..10000000"
        );
        ensure!(
            (1..=128 * 1024 * 1024 * 1024usize).contains(&self.read_bytes),
            "read limit must be 1 byte..128 GiB"
        );
        Ok(())
    }
}
mod documents;
pub mod inventory;
pub mod ocr;
mod prose;
mod provider_process;
pub mod providers;
pub mod semantic;
mod spreadsheets;
pub mod syntax;
mod tables;

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct IndexReport {
    #[serde(default)]
    pub scan_extract_us: u64,
    #[serde(default)]
    pub publish_us: u64,
    pub generation: u64,
    pub collection: String,
    pub indexed: usize,
    pub reused: usize,
    pub removed: usize,
    pub source_bytes: usize,
    pub skipped_truncated: usize,
    pub skipped: Vec<SkippedFile>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SkippedFile {
    pub path: String,
    pub reason: String,
}

pub fn scan(root: &Path, data_dir: &Path, previous: &Snapshot) -> Result<(Snapshot, IndexReport)> {
    scan_with_limits(
        root,
        data_dir,
        previous,
        MAX_DISCOVERY_ENTRIES,
        MAX_SCAN_READ_BYTES,
    )
}

fn scan_with_limits(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    max_entries: usize,
    max_read_bytes: usize,
) -> Result<(Snapshot, IndexReport)> {
    scan_streaming_with_limits(
        root,
        data_dir,
        previous,
        &previous.roots,
        max_entries,
        max_read_bytes,
        |artifact, _| Ok(artifact.clone()),
    )
}

/// Emit one bounded artifact and its original source bytes at a time. The callback
/// may retain payloads and return compact inventory metadata instead of source text.
pub fn scan_streaming(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    emit: impl FnMut(&Artifact, &[u8]) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport)> {
    scan_streaming_with_limits(
        root,
        data_dir,
        previous,
        roots,
        MAX_DISCOVERY_ENTRIES,
        MAX_SCAN_READ_BYTES,
        emit,
    )
}

fn scan_streaming_with_limits(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    max_entries: usize,
    max_read_bytes: usize,
    emit: impl FnMut(&Artifact, &[u8]) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport)> {
    scan_streaming_configured(
        root,
        data_dir,
        previous,
        roots,
        ScanLimits {
            discovery_entries: max_entries,
            read_bytes: max_read_bytes,
            ..ScanLimits::default()
        },
        emit,
    )
}

pub fn scan_streaming_with_budgets(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    limits: ScanLimits,
    emit: impl FnMut(&Artifact, &[u8]) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport)> {
    limits.validate()?;
    scan_streaming_configured(root, data_dir, previous, roots, limits, emit)
}

fn scan_streaming_configured(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    limits: ScanLimits,
    mut emit: impl FnMut(&Artifact, &[u8]) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport)> {
    let policy = adapters::profile::ExtractionPolicy {
        version: 1,
        rules: vec![],
    };
    scan_streaming_profiled(
        root,
        data_dir,
        previous,
        roots,
        limits,
        &policy,
        false,
        |path, hash, _profile| {
            Ok(previous
                .artifacts
                .get(path)
                .filter(|old| {
                    old.content_hash == hash.to_string()
                        && previous.extractor_version == EXTRACTOR_VERSION
                })
                .cloned())
        },
        |_, artifact, bytes| emit(artifact, bytes),
    )
}

/// V2 profile-aware scan. Lookup is collection-scoped and returns verified,
/// correctly rebound artifacts; selection precedes lookup and full rebuilds
/// bypass it. Failed lookups abort rather than silently omitting evidence.
#[allow(clippy::too_many_arguments)]
pub fn scan_streaming_v2_with_budgets(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    limits: ScanLimits,
    policy: &adapters::profile::ExtractionPolicy,
    full_reindex: bool,
    lookup: impl FnMut(
        &str,
        &airk_core::identity::Sha256Digest,
        &airk_core::extraction::ExtractionProfile,
    ) -> Result<Option<Artifact>>,
    emit: impl FnMut(&airk_core::extraction::ExtractionProfile, &Artifact, &[u8]) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport)> {
    limits.validate()?;
    scan_streaming_profiled(
        root,
        data_dir,
        previous,
        roots,
        limits,
        policy,
        full_reindex,
        lookup,
        emit,
    )
}

#[allow(clippy::too_many_arguments)]
fn scan_streaming_profiled(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    limits: ScanLimits,
    policy: &adapters::profile::ExtractionPolicy,
    full_reindex: bool,
    mut lookup: impl FnMut(
        &str,
        &airk_core::identity::Sha256Digest,
        &airk_core::extraction::ExtractionProfile,
    ) -> Result<Option<Artifact>>,
    mut emit: impl FnMut(
        &airk_core::extraction::ExtractionProfile,
        &Artifact,
        &[u8],
    ) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport)> {
    let registry = adapters::registry::AdapterRegistry::baseline();
    policy.validate(&registry)?;
    let root = root.canonicalize().context("resolve workspace root")?;
    ensure!(
        root.to_string_lossy() == previous.root,
        "workspace root identity changed; collection binding must be preserved"
    );
    let data_dir = data_dir.canonicalize().context("resolve index directory")?;
    let mut report = IndexReport {
        collection: previous.collection.clone(),
        ..Default::default()
    };
    let mut artifacts = BTreeMap::new();
    let mut total_bytes = 0usize;
    let mut extracted_bytes = 0usize;
    let mut read_bytes = 0usize;
    let mut sources = vec![(String::new(), root.clone())];
    for (alias, value) in roots {
        let source = Path::new(value).canonicalize()?;
        ensure!(
            source.to_str() == Some(value.as_str()),
            "additional source root identity changed"
        );
        sources.push((format!("@{alias}/"), source));
    }
    let walker = sources.iter().flat_map(|(prefix, source)| {
        discovery(source, &data_dir).map(move |entry| (prefix, source, entry))
    });
    for (visited, (prefix, source, entry)) in walker.enumerate() {
        ensure!(
            visited < limits.discovery_entries,
            "workspace discovery entry budget exhausted; previous generation retained"
        );
        let entry = entry.context("workspace discovery failed; previous generation retained")?;
        let Some(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_file() {
            continue;
        }
        let path = entry
            .path()
            .strip_prefix(source)?
            .to_str()
            .context("non-UTF-8 source path is unsupported")?
            .replace('\\', "/");
        let path = format!("{prefix}{path}");
        if path.len() > 4096 {
            report.skip(SkippedFile {
                path,
                reason: "path exceeds 4096 bytes".into(),
            });
            continue;
        }
        // Exclusions still consume discovery budget, but known oversized files
        // need no source read. Recheck identity/size inside the bounded reader.
        let source_size = entry.path().symlink_metadata()?.len();
        if source_size > limits.file_bytes as u64 {
            report.skip(SkippedFile {
                path,
                reason: format!("file exceeds {} byte configured limit", limits.file_bytes),
            });
            continue;
        }
        let remaining = limits.read_bytes.saturating_sub(read_bytes);
        ensure!(
            source_size <= remaining as u64,
            "workspace raw source read budget exhausted; previous generation retained"
        );
        let bytes = read_source_with_budget(source, entry.path(), remaining)?;
        read_bytes += bytes.len();
        if bytes.len() > limits.file_bytes {
            report.skip(SkippedFile {
                path,
                reason: format!("file exceeds {} byte configured limit", limits.file_bytes),
            });
            continue;
        }
        let content_hash = format!("{:x}", Sha256::digest(&bytes));
        let hash = content_hash.parse::<airk_core::identity::Sha256Digest>()?;
        let selection = policy.select_with_prefix(&registry, &path, &bytes)?;
        let mut profile = selection.profile;
        // A deliberate OCR operation is retained for unchanged source bytes.
        // Explicit policy overrides and full rebuilds select native extraction.
        if !full_reindex
            && !matches!(
                selection.basis.as_str(),
                "exact_override" | "scoped_override"
            )
            && let Some(old) = previous
                .artifacts
                .get(&path)
                .filter(|old| old.content_hash == content_hash && old.language == "ocr")
        {
            profile = adapters::legacy::legacy_profile(old)?;
        }
        profile.identity()?;
        let cached = if full_reindex {
            None
        } else {
            lookup(&path, &hash, &profile)?
        };
        let artifact = match cached {
            Some(old) => {
                ensure!(
                    old.path == path
                        && old.content_hash == content_hash
                        && old.language == profile.language,
                    "cache returned incompatible artifact binding"
                );
                ensure!(
                    old.retained
                        .as_ref()
                        .and_then(|reference| reference.profile.as_ref())
                        .is_none_or(|cached_profile| cached_profile == &profile),
                    "cache returned incompatible extraction profile"
                );
                report.reused += 1;
                old
            }
            _ => {
                ensure!(
                    profile.language != "ocr",
                    "retained OCR payload unavailable; explicit OCR operation required"
                );
                let extracted = match documents::extract_selected(&path, &bytes, &profile) {
                    Ok(extracted) => extracted,
                    Err(error) => {
                        report.skip(SkippedFile {
                            path,
                            reason: error.to_string(),
                        });
                        continue;
                    }
                };
                report.indexed += 1;
                Artifact {
                    path: path.clone(),
                    content_hash: content_hash.clone(),
                    bytes: bytes.len(),
                    chunks: extracted.chunks,
                    language: extracted.language,
                    extraction_method: extracted.method,
                    parse_status: extracted.parse_status,
                    facts: extracted.facts,
                    retained: None,
                }
            }
        };
        // Unsupported binary sources do not consume eligible corpus capacity.
        total_bytes += bytes.len();
        extracted_bytes = extracted_bytes
            .checked_add(artifact.extracted_bytes())
            .context("extracted corpus byte count overflow")?;
        ensure!(
            extracted_bytes <= limits.extracted_bytes,
            "extracted corpus exceeds configured byte limit; previous generation retained"
        );
        ensure!(
            total_bytes <= limits.corpus_bytes,
            "corpus exceeds configured byte limit; previous generation retained"
        );
        ensure!(
            artifacts.len() < limits.files,
            "corpus exceeds configured file count; previous generation retained"
        );
        let artifact = emit(&profile, &artifact, &bytes)?;
        ensure!(
            artifact.path == path && artifact.content_hash == content_hash,
            "retention changed artifact identity"
        );
        artifacts.insert(path, artifact);
    }
    report.removed = previous
        .artifacts
        .keys()
        .filter(|path| !artifacts.contains_key(*path))
        .count();
    let changed = report.indexed > 0
        || report.removed > 0
        || previous.extractor_version != EXTRACTOR_VERSION
        || previous.roots != *roots;
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
    next.roots = roots.clone();
    next.storage_format = previous.storage_format;
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
    report.source_bytes = total_bytes;
    Ok((next, report))
}

impl IndexReport {
    fn skip(&mut self, file: SkippedFile) {
        // Bound diagnostic payloads even when a tree contains many unsupported files.
        if self.skipped.len() < 1000 {
            self.skipped.push(file);
        } else {
            self.skipped_truncated += 1;
        }
    }
}

fn discovery(root: &Path, data_dir: &Path) -> ignore::Walk {
    let data_dir = data_dir.to_path_buf();
    WalkBuilder::new(root)
        .require_git(false)
        .hidden(false)
        .follow_links(false)
        .add_custom_ignore_filename(".airkignore")
        .filter_entry(move |entry| {
            let name = entry.file_name().to_string_lossy();
            entry.depth() == 0 || (!excluded(&name) && !entry.path().starts_with(&data_dir))
        })
        .build()
}

fn read_source(root: &Path, path: &Path) -> Result<Vec<u8>> {
    read_source_with_budget(root, path, MAX_FILE_BYTES + 1)
}

/// Explicit extraction still respects normal ignore rules and source boundaries.
pub fn read_selected_source(root: &Path, data_dir: &Path, relative: &str) -> Result<Vec<u8>> {
    semantic::validate_path(relative)?;
    let root = root.canonicalize()?;
    let data_dir = data_dir.canonicalize()?;
    let expected = root.join(relative);
    for (visited, entry) in discovery(&root, &data_dir).enumerate() {
        ensure!(
            visited < MAX_DISCOVERY_ENTRIES,
            "selected source discovery budget exhausted"
        );
        let entry = entry?;
        if entry.path() == expected {
            ensure!(
                entry.file_type().is_some_and(|kind| kind.is_file()),
                "selected source is not a regular file"
            );
            let bytes = read_source(&root, &expected)?;
            ensure!(
                bytes.len() <= MAX_FILE_BYTES,
                "selected source exceeds file limit"
            );
            return Ok(bytes);
        }
    }
    anyhow::bail!("selected source is missing or excluded by source discovery policy")
}

fn read_source_with_budget(root: &Path, path: &Path, remaining: usize) -> Result<Vec<u8>> {
    let before = path.symlink_metadata()?;
    ensure!(
        before.is_file() && !before.file_type().is_symlink(),
        "source changed to symlink or non-file during scan"
    );
    ensure!(
        path.canonicalize()?.starts_with(root),
        "source escaped workspace"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take((MAX_FILE_BYTES + 1).min(remaining) as u64)
        .read_to_end(&mut bytes)?;
    let after = path.symlink_metadata()?;
    ensure!(
        after.is_file()
            && !after.file_type().is_symlink()
            && path.canonicalize()?.starts_with(root),
        "source changed during scan"
    );
    ensure!(
        before.len() == after.len() && before.modified()? == after.modified()?,
        "source changed during scan; retry indexing"
    );
    ensure!(
        bytes.len() as u64 == before.len() || bytes.len() > MAX_FILE_BYTES,
        "workspace raw source read budget exhausted; previous generation retained"
    );
    Ok(bytes)
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

#[cfg(test)]
mod scan_budget_tests {
    use super::*;

    #[test]
    fn selected_profile_precedes_reuse_and_full_rebuild_bypasses_lookup() -> Result<()> {
        use adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(
            root.path().join("header.h"),
            "class Widget { public: void run() {} };\n",
        )?;
        std::fs::write(root.path().join("stable.rs"), "fn stable() {}\n")?;
        let empty = Snapshot::empty(root.path().canonicalize()?.to_string_lossy().into_owned());
        let (previous, _) = scan(root.path(), data.path(), &empty)?;
        assert_eq!(previous.artifacts["header.h"].language, "c");
        let policy = ExtractionPolicy {
            version: 1,
            rules: vec![SelectionRule {
                selector: RuleMatch::Scoped {
                    prefix: String::new(),
                    suffix: Some(".h".into()),
                    basename: None,
                },
                case_sensitive: true,
                adapter_id: "cpp".into(),
            }],
        };
        let mut calls = 0;
        let (next, report) = scan_streaming_v2_with_budgets(
            root.path(),
            data.path(),
            &previous,
            &BTreeMap::new(),
            ScanLimits::default(),
            &policy,
            false,
            |path, hash, profile| {
                calls += 1;
                let old = &previous.artifacts[path];
                assert_eq!(old.content_hash, hash.to_string());
                if path == "header.h" {
                    assert_eq!(profile.language, "cpp");
                }
                Ok((adapters::legacy::legacy_profile(old)? == *profile).then(|| old.clone()))
            },
            |_, artifact, _| Ok(artifact.clone()),
        )?;
        assert_eq!(calls, 2);
        assert_eq!((report.indexed, report.reused), (1, 1));
        assert_eq!(next.artifacts["header.h"].language, "cpp");
        assert!(
            next.artifacts["header.h"]
                .facts
                .iter()
                .any(|fact| fact.name == "Widget" && fact.syntax_kind == "class_specifier")
        );
        let (_, rebuilt) = scan_streaming_v2_with_budgets(
            root.path(),
            data.path(),
            &next,
            &BTreeMap::new(),
            ScanLimits::default(),
            &policy,
            true,
            |_, _, _| anyhow::bail!("full rebuild must not read cache"),
            |_, artifact, _| Ok(artifact.clone()),
        )?;
        assert_eq!((rebuilt.indexed, rebuilt.reused), (2, 0));
        assert!(
            scan_streaming_v2_with_budgets(
                root.path(),
                data.path(),
                &next,
                &BTreeMap::new(),
                ScanLimits::default(),
                &policy,
                false,
                |_, _, _| anyhow::bail!("corrupt verified cache"),
                |_, artifact, _| Ok(artifact.clone())
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn unchanged_ocr_keeps_explicit_profile_without_rerunning_and_full_resets() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let bytes = b"source text only when explicitly re-extracted\n";
        std::fs::write(root.path().join("scan.png"), bytes)?;
        let hash = format!("{:x}", Sha256::digest(bytes));
        let provenance = ocr::OcrProvenance {
            schema: 1,
            source_sha256: hash.clone(),
            tesseract_sha256: hash.clone(),
            renderer_sha256: None,
            traineddata_sha256: BTreeMap::from([("eng".into(), hash.clone())]),
            language: "eng".into(),
            first_page: 1,
            last_page: 1,
            pages_with_text: 0,
            dpi: 150,
            coverage: "fictional fixture".into(),
        };
        let status = serde_json::to_string(&provenance)?;
        let artifact = Artifact {
            path: "scan.png".into(),
            content_hash: hash,
            bytes: bytes.len(),
            chunks: vec![],
            language: "ocr".into(),
            extraction_method: format!(
                "local-tesseract-ocr/1:{:x}",
                Sha256::digest(status.as_bytes())
            ),
            parse_status: status,
            facts: vec![],
            retained: None,
        };
        let mut previous =
            Snapshot::empty(root.path().canonicalize()?.to_string_lossy().into_owned());
        previous
            .artifacts
            .insert(artifact.path.clone(), artifact.clone());
        let policy = adapters::profile::ExtractionPolicy {
            version: 1,
            rules: vec![],
        };
        let (retained, report) = scan_streaming_v2_with_budgets(
            root.path(),
            data.path(),
            &previous,
            &BTreeMap::new(),
            ScanLimits::default(),
            &policy,
            false,
            |_, _, profile| {
                assert_eq!(*profile, adapters::legacy::legacy_profile(&artifact)?);
                Ok(Some(artifact.clone()))
            },
            |_, artifact, _| Ok(artifact.clone()),
        )?;
        assert_eq!(report.reused, 1);
        assert_eq!(
            retained.artifacts["scan.png"].parse_status,
            artifact.parse_status
        );
        let (reset, _) = scan_streaming_v2_with_budgets(
            root.path(),
            data.path(),
            &previous,
            &BTreeMap::new(),
            ScanLimits::default(),
            &policy,
            true,
            |_, _, _| anyhow::bail!("full rebuild must bypass OCR cache"),
            |_, artifact, _| Ok(artifact.clone()),
        )?;
        assert_eq!(reset.artifacts["scan.png"].language, "text");
        Ok(())
    }

    #[test]
    fn unsupported_sources_consume_raw_read_budget_without_partial_success() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(root.path().join("one.bin"), [0, 0])?;
        std::fs::write(root.path().join("two.bin"), [0, 0])?;
        let mut previous =
            Snapshot::empty(root.path().canonicalize()?.to_string_lossy().into_owned());
        previous.generation = 7;
        let error = scan_with_limits(root.path(), data.path(), &previous, 100, 3)
            .expect_err("raw read budget");
        assert!(error.to_string().contains("raw source read budget"));
        let (snapshot, report) = scan_with_limits(root.path(), data.path(), &previous, 100, 4)?;
        assert_eq!(snapshot.generation, 7);
        assert_eq!(report.skipped.len(), 2);
        assert_eq!(
            report.source_bytes, 0,
            "eligible byte accounting stays independent"
        );
        File::create(root.path().join("oversized.bin"))?.set_len((MAX_FILE_BYTES + 1) as u64)?;
        let (_, report) = scan_with_limits(root.path(), data.path(), &previous, 100, 4)?;
        assert_eq!(
            report.skipped.len(),
            3,
            "known oversized sources require no read budget"
        );
        Ok(())
    }

    #[test]
    fn directory_only_trees_consume_discovery_budget() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::create_dir_all(root.path().join("one/two/three"))?;
        let previous = Snapshot::empty(root.path().canonicalize()?.to_string_lossy().into_owned());
        let error = scan_with_limits(root.path(), data.path(), &previous, 2, 0)
            .expect_err("directory budget");
        assert!(error.to_string().contains("discovery entry budget"));
        assert!(scan_with_limits(root.path(), data.path(), &previous, 4, 0).is_ok());
        Ok(())
    }
}
