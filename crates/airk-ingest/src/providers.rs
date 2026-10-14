// SPDX-License-Identifier: Apache-2.0
//! Explicit local provider adapters, bound to indexed sources and declared build inputs.
pub use crate::provider_process::ProviderCommand;
use crate::semantic;
use airk_core::*;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[path = "providers_lsp.rs"]
mod lsp;
pub use lsp::run_lsp;

const MAX_RECORDS: usize = 10_000;
const MAX_OUTPUT: usize = 16_777_216;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveRequest {
    pub command: ProviderCommand,
    pub producer: String,
    pub producer_version: String,
    pub build_inputs: Vec<String>,
    pub paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderAnalysis {
    pub manifest: AnalysisManifest,
    pub facts: Vec<FactHit>,
    pub relationships: Vec<RelationshipHit>,
    pub coverage: Vec<String>,
}
impl ProviderAnalysis {
    fn digest(&self) -> Result<String> {
        let bytes = serde_json::to_vec(&(&self.facts, &self.relationships, &self.coverage))?;
        ensure!(
            bytes.len() <= MAX_OUTPUT,
            "provider evidence exceeds 16 MiB"
        );
        Ok(semantic::digest(&bytes))
    }
    fn finish(mut self, root: &Path, snapshot: &Snapshot) -> Result<Self> {
        ensure!(
            self.facts.len() + self.relationships.len() <= MAX_RECORDS,
            "provider record limit exceeded"
        );
        let mut edges = BTreeSet::new();
        self.relationships.retain(|edge| {
            edges.insert((
                edge.source.clone(),
                edge.target.clone(),
                edge.kind.clone(),
                edge.citation.path.clone(),
                edge.citation.start_line,
                edge.citation.end_line,
            ))
        });
        validate_live_sources(root, snapshot)?;
        semantic::validate_build_inputs(root, &self.manifest)?;
        self.manifest.index_sha256 = Some(self.digest()?);
        Ok(self)
    }
}

fn live_path(root: &Path, relative: &str) -> Result<PathBuf> {
    semantic::validate_path(relative)?;
    let root = root.canonicalize()?;
    let mut path = root.clone();
    for component in relative.split('/') {
        path.push(component);
        ensure!(
            !path.symlink_metadata()?.file_type().is_symlink(),
            "linked provider input rejected"
        );
    }
    ensure!(
        path.is_file() && path.canonicalize()?.starts_with(&root),
        "provider input escaped source root"
    );
    Ok(path)
}

fn validate_live_sources(root: &Path, snapshot: &Snapshot) -> Result<()> {
    ensure!(
        root.canonicalize()?.to_string_lossy() == snapshot.root && snapshot.roots.is_empty(),
        "live providers currently require the bound single owner root"
    );
    ensure!(
        snapshot.artifacts.len() <= 10_000,
        "live provider inventory exceeds its independent 10000-file copy budget"
    );
    let mut total = 0usize;
    for (path, entry) in &snapshot.artifacts {
        let bytes = semantic::read_bounded(&live_path(root, path)?, crate::MAX_FILE_BYTES)?;
        total += bytes.len();
        ensure!(total <= 67_108_864, "provider source copy exceeds 64 MiB");
        ensure!(
            semantic::digest(&bytes) == entry.content_hash,
            "provider source changed: {path}"
        );
    }
    Ok(())
}

struct Workspace {
    directory: tempfile::TempDir,
    artifacts: BTreeMap<String, Artifact>,
    manifest: AnalysisManifest,
    paths: Vec<String>,
    executable_digest: String,
}
impl Workspace {
    fn prepare(
        root: &Path,
        snapshot: &Snapshot,
        source: &dyn ArtifactSource,
        request: &LiveRequest,
    ) -> Result<Self> {
        validate_live_sources(root, snapshot)?;
        ensure!(
            !request.paths.is_empty() && request.paths.len() <= 64,
            "select 1..64 indexed provider paths explicitly"
        );
        let selected: BTreeSet<_> = request.paths.iter().cloned().collect();
        ensure!(
            selected.len() == request.paths.len(),
            "duplicate provider path"
        );
        let manifest = semantic::prepare(
            root,
            snapshot,
            &request.producer,
            &request.producer_version,
            &request.build_inputs,
        )?;
        let executable_digest = semantic::digest(&semantic::read_bounded(
            &request.command.validate()?,
            268_435_456,
        )?);
        let directory = tempfile::Builder::new()
            .prefix("airk-provider-")
            .tempdir()?;
        let mut artifacts = BTreeMap::new();
        let mut total = 0;
        // Copy the indexed inventory, including configuration, so cross-file locations
        // can be checked. Never expose the live root as the server workspace.
        for (path, entry) in &snapshot.artifacts {
            let bytes = semantic::read_bounded(&live_path(root, path)?, crate::MAX_FILE_BYTES)?;
            ensure!(
                semantic::digest(&bytes) == entry.content_hash,
                "provider source changed while copying"
            );
            total += bytes.len();
            ensure!(total <= 67_108_864, "provider source copy exceeds 64 MiB");
            let destination = directory.path().join(path);
            std::fs::create_dir_all(destination.parent().context("provider input parent")?)?;
            std::fs::write(destination, &bytes)?;
            let artifact = source.load_artifact(path)?;
            ensure!(
                artifact.path == *path && artifact.content_hash == entry.content_hash,
                "provider retained artifact binding mismatch"
            );
            if artifact.extraction_method == "utf8" {
                let text = artifact
                    .chunks
                    .iter()
                    .map(|chunk| chunk.text.as_str())
                    .collect::<String>();
                ensure!(
                    semantic::digest(text.as_bytes()) == entry.content_hash,
                    "provider requires exact retained UTF-8 bytes"
                );
                artifacts.insert(path.clone(), artifact);
            }
        }
        for (path, hash) in &manifest.build_inputs {
            let bytes = semantic::read_bounded(&live_path(root, path)?, 8_388_608)?;
            ensure!(
                semantic::digest(&bytes) == *hash,
                "provider build input changed while copying"
            );
            total += bytes.len();
            ensure!(total <= 134_217_728, "provider input copies exceed 128 MiB");
            let destination = directory.path().join(path);
            std::fs::create_dir_all(destination.parent().context("build input parent")?)?;
            std::fs::write(destination, bytes)?;
        }
        ensure!(
            selected.iter().all(|path| artifacts.contains_key(path)),
            "provider path is not an indexed exact UTF-8 source"
        );
        Ok(Self {
            directory,
            artifacts,
            manifest,
            paths: request.paths.clone(),
            executable_digest,
        })
    }
    fn empty(&self, adapter: &str) -> ProviderAnalysis {
        ProviderAnalysis {
            manifest: self.manifest.clone(),
            facts: Vec::new(),
            relationships: Vec::new(),
            coverage: vec![
                format!(
                    "{adapter}; explicitly selected paths; source/build-bound provider output; no universal resolution guarantee"
                ),
                format!("executable_sha256={}", self.executable_digest),
            ],
        }
    }
    fn text(&self, path: &str) -> Result<String> {
        Ok(self
            .artifacts
            .get(path)
            .context("provider location is outside indexed UTF-8 inventory")?
            .chunks
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect())
    }
    fn finish(
        &self,
        output: ProviderAnalysis,
        root: &Path,
        snapshot: &Snapshot,
        request: &LiveRequest,
    ) -> Result<ProviderAnalysis> {
        ensure!(
            semantic::digest(&semantic::read_bounded(
                &request.command.validate()?,
                268_435_456
            )?) == self.executable_digest,
            "provider executable changed during analysis"
        );
        for (path, expected) in self
            .manifest
            .source_hashes
            .iter()
            .chain(&self.manifest.build_inputs)
        {
            let bytes =
                semantic::read_bounded(&live_path(self.directory.path(), path)?, 16_777_216)?;
            ensure!(
                semantic::digest(&bytes) == *expected,
                "provider modified its bound source/build copy"
            );
        }
        output.finish(root, snapshot)
    }
    fn path(&self, value: &str) -> Result<String> {
        let candidate = if value.starts_with("file:") {
            url::Url::parse(value)?
                .to_file_path()
                .map_err(|_| anyhow::anyhow!("provider URI is not a local file"))?
        } else {
            PathBuf::from(value)
        };
        let candidate = if candidate.is_absolute() {
            candidate
        } else {
            self.directory.path().join(candidate)
        };
        let canonical = candidate
            .canonicalize()
            .context("provider location does not exist")?;
        let base = self.directory.path().canonicalize()?;
        let relative = canonical
            .strip_prefix(base)
            .context("external provider location rejected")?
            .to_string_lossy()
            .replace('\\', "/");
        semantic::validate_path(&relative)?;
        ensure!(
            self.artifacts.contains_key(&relative),
            "provider location is not an indexed UTF-8 source"
        );
        // No symlink introduced by a provider can redirect a cited file.
        let disk = semantic::read_bounded(
            &live_path(self.directory.path(), &relative)?,
            crate::MAX_FILE_BYTES,
        )?;
        ensure!(
            semantic::digest(&disk) == self.artifacts[&relative].content_hash,
            "provider modified its source copy"
        );
        Ok(relative)
    }
    fn citation(&self, path: &str, first: usize, last: usize) -> Result<Citation> {
        let artifact = self.artifacts.get(path).context("provider path missing")?;
        let text = self.text(path)?;
        ensure!(
            first > 0 && last >= first && last <= text.split_inclusive('\n').count().max(1),
            "provider line range outside indexed source"
        );
        Ok(Citation {
            path: path.into(),
            content_hash: artifact.content_hash.clone(),
            start_line: first,
            end_line: last,
            location: DocumentLocation::Lines {
                start: first,
                end: last,
            },
        })
    }
}

fn evidence(manifest: &AnalysisManifest, id: String, name: String, coverage: &str) -> CodeEvidence {
    CodeEvidence {
        symbol_id: id,
        qualified_name: name,
        scope: Vec::new(),
        owner_id: None,
        signature: None,
        span: None,
        signature_span: None,
        parameters: None,
        return_type: None,
        modifiers: Vec::new(),
        documentation: None,
        target: None,
        basis: EvidenceBasis::Imported,
        producer: manifest.producer.clone(),
        producer_version: manifest.producer_version.clone(),
        coverage: coverage.into(),
        details_truncated: false,
    }
}

/// Execute Universal Ctags with no project/user option loading and JSON stdout.
pub fn run_ctags(
    root: &Path,
    snapshot: &Snapshot,
    source: &dyn ArtifactSource,
    request: &LiveRequest,
) -> Result<ProviderAnalysis> {
    ensure!(
        request.command.args.is_empty(),
        "Ctags accepts no extra arguments; AIRK controls output/configuration flags"
    );
    let workspace = Workspace::prepare(root, snapshot, source, request)?;
    let mut args: Vec<String> = [
        "--options=NONE",
        "--output-format=json",
        "--fields=+nKSeZ",
        "--extras=-p",
        "--sort=no",
        "--excmd=number",
        "-o",
        "-",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    args.extend(workspace.paths.iter().map(|path| format!("./{path}")));
    let bytes = crate::provider_process::run(
        &request.command,
        workspace.directory.path(),
        &args,
        MAX_OUTPUT,
    )?;
    let mut output = parse_ctags(&workspace, &bytes)?;
    output.coverage.push("Ctags declarations preserve provider line/end coordinates; no inferred references or resolved edges".into());
    workspace.finish(output, root, snapshot, request)
}

fn parse_ctags(workspace: &Workspace, bytes: &[u8]) -> Result<ProviderAnalysis> {
    ensure!(bytes.len() <= MAX_OUTPUT, "Ctags output exceeds 16 MiB");
    let mut output = workspace.empty("universal-ctags-json/1");
    for line in std::str::from_utf8(bytes)?
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        ensure!(line.len() <= 65_536, "Ctags record exceeds 64 KiB");
        let record: Value = serde_json::from_str(line)?;
        if record.get("_type").and_then(Value::as_str) == Some("ptag") {
            continue;
        }
        ensure!(
            record.get("_type").and_then(Value::as_str) == Some("tag"),
            "unknown Ctags record type"
        );
        let name = record
            .get("name")
            .and_then(Value::as_str)
            .context("Ctags name missing")?;
        ensure!(
            !name.is_empty() && name.len() <= 1024,
            "Ctags name outside bounds"
        );
        let path = workspace.path(
            record
                .get("path")
                .and_then(Value::as_str)
                .context("Ctags path missing")?,
        )?;
        let first = usize::try_from(
            record
                .get("line")
                .and_then(Value::as_u64)
                .context("Ctags numeric line required")?,
        )?;
        let last = usize::try_from(
            record
                .get("end")
                .and_then(Value::as_u64)
                .unwrap_or(first as u64),
        )?;
        let citation = workspace.citation(&path, first, last)?;
        let kind = record
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("symbol");
        ensure!(kind.len() <= 128, "Ctags kind too long");
        let mut code = evidence(
            &output.manifest,
            format!("ctags:{}:{path}:{first}:{name}", output.manifest.producer),
            name.into(),
            "Ctags syntax declaration; not compiler-resolved; line coordinates only",
        );
        if let Some(scope) = record.get("scope").and_then(Value::as_str) {
            ensure!(scope.len() <= 1024, "Ctags scope too long");
            code.scope.push(scope.into());
            code.qualified_name = format!("{scope}::{name}");
        }
        if let Some(signature) = record.get("signature").and_then(Value::as_str) {
            code.details_truncated = signature.len() > 1024;
            code.signature = Some(crate::tables::bounded(signature, 1024));
        }
        output.facts.push(FactHit {
            name: name.into(),
            kind: "definition".into(),
            syntax_kind: format!("ctags/{kind}"),
            language: workspace.artifacts[&path].language.clone(),
            citation,
            document: None,
            code: Some(code),
        });
        ensure!(
            output.facts.len() <= MAX_RECORDS,
            "Ctags record limit exceeded"
        );
    }
    Ok(output)
}

/// Validate a complete provider domain before replacing imported evidence. Native
/// artifacts/facts are unchanged. The normal indexer invalidates this manifest when
/// any source or declared build input changes.
pub fn apply(
    snapshot: &mut Snapshot,
    analysis: &ProviderAnalysis,
    source: &dyn ArtifactSource,
) -> Result<()> {
    let manifest = &analysis.manifest;
    ensure!(
        manifest.version == 1
            && manifest.root == snapshot.root
            && manifest.collection == snapshot.collection
            && manifest.extractor_version == snapshot.extractor_version
            && manifest.source_hashes == semantic::source_inventory(snapshot),
        "provider analysis source/root/collection binding mismatch"
    );
    ensure!(
        manifest.index_sha256.as_ref() == Some(&analysis.digest()?),
        "provider output digest mismatch"
    );
    ensure!(
        analysis.facts.len() + analysis.relationships.len() <= MAX_RECORDS
            && analysis.coverage.len() <= 32,
        "provider evidence count exceeded"
    );
    ensure!(
        !manifest.producer.is_empty()
            && manifest.producer.len() <= 128
            && !manifest.producer_version.is_empty()
            && manifest.producer_version.len() <= 128,
        "invalid provider identity"
    );
    semantic::validate_build_inputs(Path::new(&snapshot.root), manifest)?;
    validate_live_sources(Path::new(&snapshot.root), snapshot)?;
    let mut texts = BTreeMap::<String, String>::new();
    let mut total = 0;
    for citation in analysis
        .facts
        .iter()
        .map(|fact| &fact.citation)
        .chain(analysis.relationships.iter().map(|edge| &edge.citation))
    {
        semantic::validate_path(&citation.path)?;
        let entry = snapshot
            .artifacts
            .get(&citation.path)
            .context("external provider citation rejected")?;
        ensure!(
            citation.content_hash == entry.content_hash,
            "stale provider citation rejected"
        );
        if !texts.contains_key(&citation.path) {
            let artifact = source.load_artifact(&citation.path)?;
            ensure!(
                artifact.content_hash == entry.content_hash && artifact.extraction_method == "utf8",
                "provider citation source mismatch"
            );
            let text = artifact
                .chunks
                .iter()
                .map(|chunk| chunk.text.as_str())
                .collect::<String>();
            total += text.len();
            ensure!(
                total <= 67_108_864 && semantic::digest(text.as_bytes()) == entry.content_hash,
                "provider retained text binding/budget exceeded"
            );
            texts.insert(citation.path.clone(), text);
        }
        let lines = texts[&citation.path].split_inclusive('\n').count();
        ensure!(
            citation.start_line > 0
                && citation.end_line >= citation.start_line
                && citation.end_line <= lines
                && citation.location
                    == DocumentLocation::Lines {
                        start: citation.start_line,
                        end: citation.end_line
                    },
            "invalid provider citation range"
        );
    }
    for fact in &analysis.facts {
        let code = fact
            .code
            .as_ref()
            .context("provider code evidence missing")?;
        ensure!(
            code.basis == EvidenceBasis::Imported
                && code.producer == manifest.producer
                && code.producer_version == manifest.producer_version,
            "provider fact identity mismatch"
        );
        if let Some(span) = &code.span {
            let text = &texts[&fact.citation.path];
            ensure!(
                span.start_byte < span.end_byte
                    && text.get(span.start_byte..span.end_byte).is_some(),
                "provider byte range outside indexed source"
            );
        }
    }
    for edge in &analysis.relationships {
        ensure!(
            edge.basis == EvidenceBasis::Imported
                && edge.producer == manifest.producer
                && edge.producer_version == manifest.producer_version
                && edge.resolution == "provider_asserted",
            "provider relationship identity mismatch"
        );
    }
    snapshot.imported_facts = analysis.facts.clone();
    snapshot.imported_relationships = analysis.relationships.clone();
    snapshot.analysis = Some(manifest.clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(program: PathBuf) -> Result<(tempfile::TempDir, Snapshot, LiveRequest)> {
        let root = tempfile::tempdir()?;
        std::fs::create_dir(root.path().join("src"))?;
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname = \"airk_provider_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[lib]\npath = \"src/lib.rs\"\n",
        )?;
        std::fs::write(
            root.path().join("src/lib.rs"),
            "pub fn one() -> u32 { 1 }\npub fn two() -> u32 { one() }\n",
        )?;
        let state = tempfile::tempdir()?;
        let snapshot = crate::scan(
            root.path(),
            state.path(),
            &Snapshot::empty(root.path().canonicalize()?.to_string_lossy().into_owned()),
        )?
        .0;
        let request = LiveRequest {
            command: ProviderCommand {
                program,
                args: Vec::new(),
                timeout_ms: 30_000,
            },
            producer: "fixture-provider".into(),
            producer_version: "1".into(),
            build_inputs: vec!["Cargo.toml".into()],
            paths: vec!["src/lib.rs".into()],
        };
        Ok((root, snapshot, request))
    }
    #[test]
    fn ctags_locations_binding_and_native_fact_preservation() -> Result<()> {
        let (root, snapshot, request) = fixture(std::env::current_exe()?)?;
        let root = root.path().canonicalize()?;
        let mut excessive = snapshot.clone();
        let mut empty = snapshot.artifacts["src/lib.rs"].clone();
        empty.chunks.clear();
        empty.facts.clear();
        excessive.artifacts = (0..10_001)
            .map(|index| (format!("empty-{index}.rs"), empty.clone()))
            .collect();
        assert!(
            validate_live_sources(&root, &excessive)
                .unwrap_err()
                .to_string()
                .contains("10000-file copy budget")
        );
        let workspace = Workspace::prepare(&root, &snapshot, &snapshot, &request)?;
        let bytes = br#"{"_type":"tag","name":"one","path":"src/lib.rs","line":1,"end":1,"kind":"function","signature":"() -> u32"}
"#;
        let output = parse_ctags(&workspace, bytes)?.finish(&root, &snapshot)?;
        let mut next = snapshot.clone();
        apply(&mut next, &output, &snapshot)?;
        assert_eq!(
            next.artifacts["src/lib.rs"].facts.len(),
            snapshot.artifacts["src/lib.rs"].facts.len()
        );
        assert_eq!(next.imported_facts.len(), 1);
        assert!(next.imported_relationships.is_empty());
        assert_eq!(next.imported_facts[0].citation.start_line, 1);
        assert!(
            parse_ctags(
                &workspace,
                br#"{"_type":"tag","name":"one","path":"../outside.rs","line":1}"#
            )
            .is_err()
        );
        assert!(
            parse_ctags(
                &workspace,
                br#"{"_type":"tag","name":"one","path":"src/lib.rs","line":999}"#
            )
            .is_err()
        );
        let mut altered = output.clone();
        altered.facts[0].citation.content_hash = "00".repeat(32);
        altered.manifest.index_sha256 = Some(altered.digest()?);
        assert!(apply(&mut next, &altered, &snapshot).is_err());
        std::fs::write(root.join("Cargo.toml"), "changed build")?;
        assert!(apply(&mut next, &output, &snapshot).is_err());
        Ok(())
    }
    #[test]
    fn provisioned_ctags_runs_on_private_source_copy() -> Result<()> {
        let Some(program) = std::env::var_os("AIRK_TEST_CTAGS") else {
            eprintln!("AIRK_TEST_CTAGS absent: external Ctags qualification not run");
            return Ok(());
        };
        let (root, snapshot, request) = fixture(program.into())?;
        let output = run_ctags(&root.path().canonicalize()?, &snapshot, &snapshot, &request)?;
        assert!(output.facts.iter().any(|fact| fact.name == "one"));
        assert!(output.facts.iter().any(|fact| fact.name == "two"));
        assert!(output.relationships.is_empty());
        assert!(!root.path().join("tags").exists());
        Ok(())
    }
    #[test]
    fn provisioned_lsp_supplies_bound_symbols_references_and_calls() -> Result<()> {
        let Some(program) = std::env::var_os("AIRK_TEST_LSP") else {
            eprintln!("AIRK_TEST_LSP absent: external LSP qualification not run");
            return Ok(());
        };
        let (root, snapshot, mut request) = fixture(program.into())?;
        request.command.timeout_ms = 120_000;
        let output = run_lsp(&root.path().canonicalize()?, &snapshot, &snapshot, &request)?;
        assert!(
            output
                .facts
                .iter()
                .any(|fact| fact.kind == "definition" && fact.name == "one"),
            "{:?}",
            output.coverage
        );
        assert!(
            output
                .relationships
                .iter()
                .any(|edge| edge.kind == "reference"),
            "{:?}; {:?}",
            output.coverage,
            output.relationships
        );
        assert!(output.relationships.iter().any(|edge| edge.kind == "call"));
        let mut next = snapshot.clone();
        apply(&mut next, &output, &snapshot)?;
        assert!(!root.path().join("target").exists());
        assert!(!root.path().join("Cargo.lock").exists());
        Ok(())
    }
}
