// SPDX-License-Identifier: Apache-2.0
//! Opt-in SCIP adapter. Producer assertions are hash-bound, not compiler-certified.
use airk_core::*;
use anyhow::{Context, Result, bail, ensure};
use protobuf::Message;
use scip::types::{Document, Metadata, Occurrence, occurrence::Typed_range};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const MAX_IMPORT_BYTES: usize = 67_108_864;
const MAX_RECORDS: usize = 100_000;

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "input exceeds {limit} bytes");
    Ok(bytes)
}

pub fn source_inventory(snapshot: &Snapshot) -> BTreeMap<String, String> {
    snapshot
        .artifacts
        .iter()
        .map(|(p, a)| (p.clone(), a.content_hash.clone()))
        .collect()
}

pub fn validate_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && path.len() <= 4096
            && !path.contains(['\\', ':', '\0'])
            && path
                .split('/')
                .all(|c| !c.is_empty() && c != "." && c != ".."),
        "invalid collection-relative analysis path"
    );
    Ok(())
}

fn bound_path(root: &Path, path: &str) -> Result<PathBuf> {
    validate_path(path)?;
    let mut current = root.to_path_buf();
    for component in path.split('/') {
        current.push(component);
        ensure!(
            !current.symlink_metadata()?.file_type().is_symlink(),
            "linked analysis input rejected"
        );
    }
    ensure!(
        current.canonicalize()?.starts_with(root),
        "analysis input escaped root"
    );
    Ok(current)
}

pub fn prepare(
    root: &Path,
    snapshot: &Snapshot,
    producer: &str,
    version: &str,
    build_inputs: &[String],
) -> Result<AnalysisManifest> {
    ensure!(
        !producer.trim().is_empty()
            && producer.len() <= 128
            && !version.trim().is_empty()
            && version.len() <= 128,
        "producer name/version required, at most 128 bytes"
    );
    ensure!(
        !build_inputs.is_empty() && build_inputs.len() <= 1000,
        "supply 1..1000 build/configuration/lock inputs"
    );
    let mut hashes = BTreeMap::new();
    let mut total = 0;
    for input in build_inputs {
        let bytes = read_bounded(&bound_path(root, input)?, 8_388_608)?;
        total += bytes.len();
        ensure!(total <= MAX_IMPORT_BYTES, "build inputs exceed 64 MiB");
        hashes.insert(input.clone(), digest(&bytes));
    }
    Ok(AnalysisManifest {
        version: 1,
        root: snapshot.root.clone(),
        collection: snapshot.collection.clone(),
        extractor_version: snapshot.extractor_version.clone(),
        source_hashes: source_inventory(snapshot),
        build_inputs: hashes,
        producer: producer.into(),
        producer_version: version.into(),
        index_sha256: None,
    })
}

pub fn validate_build_inputs(root: &Path, m: &AnalysisManifest) -> Result<()> {
    ensure!(
        !m.build_inputs.is_empty() && m.build_inputs.len() <= 1000,
        "invalid build inventory"
    );
    let mut total = 0;
    for (path, expected) in &m.build_inputs {
        let bytes = read_bounded(&bound_path(root, path)?, 8_388_608)?;
        total += bytes.len();
        ensure!(total <= MAX_IMPORT_BYTES, "build inputs exceed 64 MiB");
        ensure!(digest(&bytes) == *expected, "build input changed: {path}");
    }
    Ok(())
}

pub fn import(snapshot: &mut Snapshot, manifest: AnalysisManifest, bytes: &[u8]) -> Result<()> {
    let source = snapshot.clone();
    import_with_source(snapshot, manifest, bytes, &source)
}

pub fn import_with_source(
    snapshot: &mut Snapshot,
    manifest: AnalysisManifest,
    bytes: &[u8],
    source: &dyn ArtifactSource,
) -> Result<()> {
    ensure!(bytes.len() <= MAX_IMPORT_BYTES, "SCIP index exceeds 64 MiB");
    ensure!(
        manifest.version == 1
            && manifest.root == snapshot.root
            && manifest.collection == snapshot.collection
            && manifest.extractor_version == snapshot.extractor_version
            && manifest.source_hashes == source_inventory(snapshot),
        "analysis manifest does not match indexed source/root/collection/extractor"
    );
    ensure!(
        manifest.index_sha256.as_ref() == Some(&digest(bytes)),
        "producer must bind the exact SCIP output digest in the manifest"
    );
    ensure!(
        !manifest.producer.is_empty()
            && manifest.producer.len() <= 128
            && !manifest.producer_version.is_empty()
            && manifest.producer_version.len() <= 128,
        "invalid producer identity"
    );
    snapshot.imported_facts.clear();
    snapshot.imported_relationships.clear();
    let mut position = 0;
    let mut metadata_seen = false;
    let mut seen = BTreeSet::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    // Decode one bounded top-level field/document at a time, never a whole Index.
    while position < bytes.len() {
        ensure!(
            Instant::now() < deadline,
            "SCIP import exceeded 30 seconds; previous generation retained"
        );
        let tag = varint(bytes, &mut position)?;
        let field = tag >> 3;
        ensure!(
            tag & 7 == 2 && (1..=3).contains(&field),
            "unsupported SCIP top-level field"
        );
        let len = usize::try_from(varint(bytes, &mut position)?)?;
        ensure!(len <= 8_388_608, "SCIP field/document exceeds 8 MiB");
        let end = position.checked_add(len).context("SCIP length overflow")?;
        let payload = bytes.get(position..end).context("truncated SCIP payload")?;
        position = end;
        if field == 1 {
            ensure!(
                !metadata_seen && seen.is_empty(),
                "SCIP metadata must appear once, first"
            );
            let metadata = Metadata::parse_from_bytes(payload)?;
            let uri = url::Url::parse(&metadata.project_root)?;
            let producer_root = uri
                .to_file_path()
                .map_err(|_| anyhow::anyhow!("SCIP root must be a local file URI"))?
                .canonicalize()?;
            ensure!(
                producer_root == Path::new(&snapshot.root),
                "SCIP project root mismatch"
            );
            ensure!(
                metadata.text_document_encoding.value() == 1,
                "SCIP source encoding must explicitly be UTF-8"
            );
            let tool = metadata
                .tool_info
                .as_ref()
                .context("SCIP producer metadata missing")?;
            ensure!(
                tool.name == manifest.producer && tool.version == manifest.producer_version,
                "SCIP producer does not match manifest"
            );
            metadata_seen = true;
        } else {
            ensure!(metadata_seen, "SCIP metadata required first");
            if field == 2 {
                let document = Document::parse_from_bytes(payload)?;
                ensure!(
                    seen.insert(document.relative_path.clone()) && seen.len() <= 10_000,
                    "duplicate/excess SCIP documents"
                );
                add_document(snapshot, &manifest, document, deadline, source)?;
            }
            // External symbol documentation is deliberately not retained/read.
        }
    }
    ensure!(
        metadata_seen && !seen.is_empty(),
        "SCIP contains no supported documents"
    );
    snapshot.analysis = Some(manifest);
    Ok(())
}

fn varint(bytes: &[u8], position: &mut usize) -> Result<u64> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let byte = *bytes.get(*position).context("truncated SCIP varint")?;
        *position += 1;
        ensure!(shift < 63 || byte <= 1, "SCIP varint overflow");
        value |= u64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    bail!("SCIP varint too long")
}

fn provider_symbol(m: &AnalysisManifest, path: &str, symbol: &str) -> Result<String> {
    ensure!(
        !symbol.is_empty() && symbol.len() <= 1024,
        "invalid SCIP symbol length"
    );
    scip::symbol::parse_symbol(symbol)
        .map_err(|e| anyhow::anyhow!("invalid SCIP symbol: {e:?}"))?;
    Ok(if symbol.starts_with("local ") {
        format!("scip:{}:{path}:{symbol}", m.producer)
    } else {
        format!("scip:{}:{symbol}", m.producer)
    })
}

fn language_alias(language: &str) -> Option<&'static str> {
    match language.to_ascii_lowercase().as_str() {
        "typescript" | "ts" => Some("typescript"),
        "typescriptreact" | "tsx" => Some("tsx"),
        "javascript" | "js" | "javascriptreact" | "jsx" => Some("javascript"),
        "rust" | "rs" => Some("rust"),
        "csharp" | "c#" | "cs" => Some("csharp"),
        _ => None,
    }
}

fn add_document(
    snapshot: &mut Snapshot,
    m: &AnalysisManifest,
    d: Document,
    deadline: Instant,
    source: &dyn ArtifactSource,
) -> Result<()> {
    validate_path(&d.relative_path)?;
    let entry = snapshot
        .artifacts
        .get(&d.relative_path)
        .context("SCIP path is not in indexed collection")?;
    let a = source.load_artifact(&d.relative_path)?;
    ensure!(
        a.path == entry.path && a.content_hash == entry.content_hash,
        "SCIP artifact binding mismatch"
    );
    let language =
        language_alias(&d.language).context("SCIP adapter currently accepts TS/JS, Rust and C#")?;
    ensure!(
        language == a.language && a.extraction_method == "utf8",
        "SCIP language/extraction mismatch"
    );
    let text = a.chunks.iter().map(|c| c.text.as_str()).collect::<String>();
    ensure!(
        d.text.is_empty() || d.text == text,
        "SCIP embedded text mismatch"
    );
    let encoding = d.position_encoding.value();
    ensure!(
        (1..=3).contains(&encoding),
        "SCIP position encoding must be explicit"
    );
    ensure!(
        d.occurrences.len() + d.symbols.len() <= MAX_RECORDS,
        "SCIP document record limit exceeded"
    );
    let metadata = d
        .symbols
        .iter()
        .map(|s| (s.symbol.as_str(), s))
        .collect::<BTreeMap<_, _>>();
    let mut definitions = BTreeMap::new();
    for occurrence in &d.occurrences {
        ensure!(
            Instant::now() < deadline,
            "SCIP import exceeded 30 seconds; previous generation retained"
        );
        if occurrence.symbol.is_empty() {
            continue;
        }
        let (start, end, start_line, end_line) = range(&text, occurrence, encoding)?;
        let id = provider_symbol(m, &d.relative_path, &occurrence.symbol)?;
        let parsed = scip::symbol::parse_symbol(&occurrence.symbol)
            .map_err(|e| anyhow::anyhow!("invalid SCIP symbol: {e:?}"))?;
        let mut scope = parsed
            .descriptors
            .iter()
            .map(|d| d.name.clone())
            .collect::<Vec<_>>();
        let qualified = scope.join("::");
        scope.pop();
        let is_definition = occurrence.symbol_roles & 1 != 0;
        let info = metadata.get(occurrence.symbol.as_str());
        let name = text[start..end].trim();
        ensure!(
            !name.is_empty() && name.len() <= 1024,
            "SCIP occurrence name missing/too long"
        );
        let citation = Citation {
            path: a.path.clone(),
            content_hash: a.content_hash.clone(),
            start_line,
            end_line,
            location: DocumentLocation::Lines {
                start: start_line,
                end: end_line,
            },
        };
        if is_definition {
            definitions
                .entry(occurrence.symbol.clone())
                .or_insert(citation.clone());
        }
        let owner = a
            .facts
            .iter()
            .filter_map(|f| f.code.as_ref().filter(|_| f.kind == "definition"))
            .filter(|c| {
                let mut parts = c.symbol_id.rsplit(':');
                parts.next();
                let b = parts
                    .next()
                    .and_then(|p| p.parse::<usize>().ok())
                    .unwrap_or(0);
                let a = parts
                    .next()
                    .and_then(|p| p.parse::<usize>().ok())
                    .unwrap_or(usize::MAX);
                a <= start && b >= end
            })
            .min_by_key(|c| {
                let mut parts = c.symbol_id.rsplit(':');
                parts.next();
                let end = parts
                    .next()
                    .and_then(|p| p.parse::<usize>().ok())
                    .unwrap_or(usize::MAX);
                let start = parts
                    .next()
                    .and_then(|p| p.parse::<usize>().ok())
                    .unwrap_or(0);
                end.saturating_sub(start)
            })
            .map(|c| c.symbol_id.clone());
        let code=CodeEvidence {fingerprints:None,span:Some(CodeSpan {start_byte:start,end_byte:end}),signature_span:None,symbol_id:id.clone(),qualified_name:qualified,scope,owner_id:owner.clone(),signature:info.and_then(|i|i.signature_documentation.as_ref()).map(|s|bounded(&s.text)),parameters:None,return_type:None,modifiers:Vec::new(),documentation:info.and_then(|i|i.documentation.first()).map(|s|bounded(s)),target:None,basis:EvidenceBasis::Imported,producer:m.producer.clone(),producer_version:m.producer_version.clone(),coverage:"Hash-bound producer assertion; precision depends on producer/build, not AIRK type analysis".into(),details_truncated:info.is_some_and(|i|i.signature_documentation.as_ref().is_some_and(|s|s.text.len()>1024) || i.documentation.iter().any(|s|s.len()>1024))};
        ensure!(
            code.qualified_name.len() <= 1024,
            "SCIP display name too long"
        );
        snapshot.imported_facts.push(FactHit {
            continuity_key: None,
            name: name.into(),
            kind: if is_definition {
                "definition"
            } else {
                "reference"
            }
            .into(),
            syntax_kind: info.map_or_else(
                || "scip-occurrence".into(),
                |s| format!("scip-kind/{}", s.kind.value()),
            ),
            language: language.into(),
            citation: citation.clone(),
            document: None,
            code: Some(code),
        });
        if !is_definition {
            snapshot.imported_relationships.push(RelationshipHit {
                source: owner.unwrap_or_else(|| format!("file:{}", a.path)),
                target: id,
                kind: "reference".into(),
                resolution: "provider_asserted".into(),
                basis: EvidenceBasis::Imported,
                producer: m.producer.clone(),
                producer_version: m.producer_version.clone(),
                citation,
            });
        }
        ensure!(
            snapshot.imported_facts.len() + snapshot.imported_relationships.len() <= MAX_RECORDS,
            "SCIP record limit exceeded"
        );
    }
    for symbol in &d.symbols {
        let Some(citation) = definitions.get(&symbol.symbol) else {
            continue;
        };
        let source = provider_symbol(m, &d.relative_path, &symbol.symbol)?;
        for rel in &symbol.relationships {
            let target = provider_symbol(m, &d.relative_path, &rel.symbol)?;
            for (enabled, kind) in [
                (rel.is_implementation, "implements"),
                (rel.is_reference, "reference"),
                (rel.is_definition, "definition"),
                (rel.is_type_definition, "type_definition"),
            ] {
                if enabled {
                    snapshot.imported_relationships.push(RelationshipHit {
                        source: source.clone(),
                        target: target.clone(),
                        kind: kind.into(),
                        resolution: "provider_asserted".into(),
                        basis: EvidenceBasis::Imported,
                        producer: m.producer.clone(),
                        producer_version: m.producer_version.clone(),
                        citation: citation.clone(),
                    });
                    ensure!(
                        snapshot.imported_facts.len() + snapshot.imported_relationships.len()
                            <= MAX_RECORDS,
                        "SCIP record limit exceeded"
                    );
                }
            }
        }
    }
    Ok(())
}

fn bounded(s: &str) -> String {
    let mut end = s.len().min(1024);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].into()
}

fn range(text: &str, o: &Occurrence, encoding: i32) -> Result<(usize, usize, usize, usize)> {
    let values = match &o.typed_range {
        Some(Typed_range::SingleLineRange(r)) => {
            [r.line, r.start_character, r.line, r.end_character]
        }
        Some(Typed_range::MultiLineRange(r)) => {
            [r.start_line, r.start_character, r.end_line, r.end_character]
        }
        Some(_) => bail!("unsupported typed SCIP range"),
        None => match o.range.as_slice() {
            [l, s, e] => [*l, *s, *l, *e],
            [l, s, el, e] => [*l, *s, *el, *e],
            _ => bail!("invalid SCIP range"),
        },
    };
    ensure!(values.iter().all(|v| *v >= 0), "negative SCIP coordinate");
    let start = offset(text, values[0] as usize, values[1] as usize, encoding)?;
    let end = offset(text, values[2] as usize, values[3] as usize, encoding)?;
    ensure!(start < end, "empty/reversed SCIP range");
    Ok((
        start,
        end,
        values[0] as usize + 1,
        values[2] as usize + usize::from(values[3] > 0),
    ))
}

fn offset(text: &str, line: usize, column: usize, encoding: i32) -> Result<usize> {
    let mut lines = text.split('\n');
    let mut base = 0;
    for _ in 0..line {
        base += lines
            .next()
            .context("SCIP line outside indexed source")?
            .len()
            + 1;
    }
    let raw = lines.next().context("SCIP line outside indexed source")?;
    let content = raw.strip_suffix('\r').unwrap_or(raw);
    if encoding == 1 {
        ensure!(
            column <= content.len() && content.is_char_boundary(column),
            "invalid UTF-8 SCIP coordinate"
        );
        return Ok(base + column);
    }
    let mut units = 0;
    for (byte, ch) in content.char_indices() {
        if units == column {
            return Ok(base + byte);
        }
        units += if encoding == 2 { ch.len_utf16() } else { 1 };
        ensure!(units <= column, "SCIP coordinate splits surrogate pair");
    }
    ensure!(units == column, "SCIP column outside indexed source");
    Ok(base + content.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_coordinates_half_open_ranges_and_typed_precedence() -> Result<()> {
        let text = "🚀 foo\r\nbar\n";
        assert_eq!(offset(text, 0, 5, 1)?, 5);
        assert_eq!(offset(text, 0, 3, 2)?, 5);
        assert_eq!(offset(text, 0, 2, 3)?, 5);
        assert!(offset(text, 0, 1, 1).is_err());
        assert!(offset(text, 0, 1, 2).is_err());
        assert!(offset(text, 0, 90, 3).is_err());
        let mut o = Occurrence::new();
        o.range = vec![0, 99, 1];
        o.typed_range = Some(Typed_range::MultiLineRange(scip::types::MultiLineRange {
            start_line: 0,
            start_character: 3,
            end_line: 1,
            end_character: 0,
            ..Default::default()
        }));
        let (start, end, first, last) = range(text, &o, 2)?;
        assert_eq!(&text[start..end], "foo\r\n");
        assert_eq!((first, last), (1, 1));
        o.typed_range = None;
        assert!(range(text, &o, 2).is_err());
        Ok(())
    }

    #[test]
    fn paths_varints_and_local_symbol_ids_are_bounded() -> Result<()> {
        for path in [
            "../escape.rs",
            "/root.rs",
            "C:/root.rs",
            "a//b.rs",
            "a\\b.rs",
            "a/./b.rs",
        ] {
            assert!(validate_path(path).is_err(), "{path}");
        }
        validate_path("src/知識.rs")?;
        assert!(varint(&[255; 20], &mut 0).is_err());
        assert!(varint(&[128], &mut 0).is_err());
        let m = AnalysisManifest {
            version: 1,
            root: "fixture".into(),
            collection: "default".into(),
            extractor_version: EXTRACTOR_VERSION.into(),
            source_hashes: BTreeMap::new(),
            build_inputs: BTreeMap::new(),
            producer: "fixture".into(),
            producer_version: "1".into(),
            index_sha256: None,
        };
        assert_ne!(
            provider_symbol(&m, "a.rs", "local 0")?,
            provider_symbol(&m, "b.rs", "local 0")?
        );
        assert!(provider_symbol(&m, "a.rs", "invalid symbol").is_err());
        Ok(())
    }
}
