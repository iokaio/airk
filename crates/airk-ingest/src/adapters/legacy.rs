// SPDX-License-Identifier: Apache-2.0
//! Strict, pure conversion of hydrated legacy evidence. Never reparses source,
//! runs a producer, rewrites ID prefixes or changes extraction interpretation.
use super::{profile::validate_relative_path, registry::AdapterRegistry};
use airk_core::extraction::{
    ExtractionPayload, ExtractionProfile, ExtractionProvenance, LocalChunk, LocalCodeEvidence,
    LocalFact, LocalSpan, LocalSyntaxId, LocalSyntaxIdentity,
};
use airk_core::identity::Sha256Digest;
use airk_core::{Artifact, CodeEvidence, EvidenceBasis};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// The exact profile whose behavior a legacy artifact can demonstrate.
/// A destination with other effective options/configuration must re-extract.
/// OCR's complete original provenance participates even when facts are empty.
pub fn legacy_profile(artifact: &Artifact) -> Result<ExtractionProfile> {
    let registry = AdapterRegistry::baseline();
    if artifact.language == "ocr" {
        validate_ocr(artifact)?;
        let mut profile = registry.get("text").expect("baseline text").profile.clone();
        profile.adapter_id = "ocr".into();
        profile.language = "ocr".into();
        profile.options.insert(
            "legacy_ocr_provenance_sha256".into(),
            digest(artifact.parse_status.as_bytes()).to_string(),
        );
        return Ok(profile);
    }
    registry
        .get(&artifact.language)
        .map(|descriptor| descriptor.profile.clone())
        .context("legacy artifact has no compatible native profile")
}

/// Caller must hydrate retained chunks/facts before invoking this conversion.
/// Reusable output contains no public source path or path-bearing symbol IDs.
pub fn convert_native_legacy(
    artifact: &Artifact,
    original_bytes: &[u8],
    profile: &ExtractionProfile,
) -> Result<ExtractionPayload> {
    validate_relative_path(&artifact.path)?;
    ensure!(
        artifact.retained.is_none(),
        "legacy artifact must be hydrated before conversion"
    );
    ensure!(
        original_bytes.len() <= crate::MAX_FILE_BYTES && artifact.bytes == original_bytes.len(),
        "legacy source size mismatch"
    );
    let source_hash: Sha256Digest = artifact.content_hash.parse()?;
    ensure!(
        digest(original_bytes) == source_hash,
        "legacy source hash mismatch"
    );
    profile.identity()?;
    ensure!(
        *profile == legacy_profile(artifact)?,
        "legacy interpretation differs from effective profile; re-extraction required"
    );
    ensure!(
        artifact.facts.len() <= 10_000
            && artifact.chunks.len() <= airk_core::extraction::MAX_EXTRACTION_CHUNKS,
        "legacy record limit exceeded"
    );
    ensure!(
        artifact.parse_status.len() <= 65_536 && artifact.extraction_method.len() <= 256,
        "legacy provenance limit exceeded"
    );
    let mut text = String::new();
    let mut chunks = Vec::with_capacity(artifact.chunks.len());
    let mut next_line = 1usize;
    for chunk in &artifact.chunks {
        ensure!(
            text.len()
                .checked_add(chunk.text.len())
                .is_some_and(|n| n <= 16_777_216),
            "legacy text limit exceeded"
        );
        let line_count = chunk.text.split_inclusive('\n').count();
        ensure!(
            line_count > 0
                && chunk.start_line == next_line
                && chunk.end_line == next_line + line_count - 1,
            "legacy chunk line sequence is inconsistent"
        );
        let start = text.len();
        text.push_str(&chunk.text);
        chunks.push(LocalChunk {
            span: LocalSpan {
                start,
                end: text.len(),
            },
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            location: chunk.location.clone(),
            region: None,
        });
        next_line = chunk.end_line + 1;
    }
    let native_text = artifact.extraction_method == "utf8";
    if native_text {
        ensure!(
            text.as_bytes() == original_bytes,
            "native legacy chunks do not reproduce original source"
        );
    }
    let mut line_starts = vec![0usize];
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            line_starts.push(index + 1);
        }
    }
    let mut identities = BTreeMap::new();
    for fact in &artifact.facts {
        if let Some(code) = &fact.code {
            ensure!(
                native_text && code.basis != EvidenceBasis::Imported,
                "external producer code requires its separately bound domain"
            );
            ensure!(
                code.producer.starts_with("airk-tree-sitter/")
                    || code.producer == "airk-framework-cues"
                    || (matches!(
                        profile.adapter_id.as_str(),
                        "cmake" | "make" | "msbuild" | "github-actions" | "compose"
                    ) && code.basis == EvidenceBasis::StructuralRule
                        && code.producer == format!("airk-build-structure/{}", profile.adapter_id)),
                "unknown code producer requires separate domain"
            );
            for id in std::iter::once(&code.symbol_id).chain(code.owner_id.iter()) {
                let identity = parse_identity(id, &artifact.path, &text)?;
                identities.entry(identity).or_insert(LocalSyntaxId(0));
            }
        }
    }
    ensure!(
        identities.len() <= 20_000,
        "legacy syntax identity limit exceeded"
    );
    for (index, id) in identities.values_mut().enumerate() {
        *id = LocalSyntaxId(index as u32);
    }
    let syntax_identities = identities
        .iter()
        .map(|((start, end, kind), id)| LocalSyntaxIdentity {
            id: *id,
            span: LocalSpan {
                start: *start,
                end: *end,
            },
            syntax_kind: kind.clone(),
            region: None,
        })
        .collect();
    let mut facts = Vec::with_capacity(artifact.facts.len());
    for (index, fact) in artifact.facts.iter().enumerate() {
        ensure!(
            fact.start_line > 0 && fact.start_line <= fact.end_line && fact.end_line < next_line,
            "legacy fact lines outside extracted text"
        );
        let span = LocalSpan {
            start: *line_starts
                .get(fact.start_line - 1)
                .context("legacy fact start line missing")?,
            end: line_starts
                .get(fact.end_line)
                .copied()
                .unwrap_or(text.len()),
        };
        let code = fact
            .code
            .as_ref()
            .map(|code| convert_code(code, &artifact.path, &text, &identities))
            .transpose()?;
        facts.push(LocalFact {
            id: index as u32,
            owner: None,
            kind: fact.kind.clone(),
            name: fact.name.clone(),
            span,
            syntax_kind: fact.syntax_kind.clone(),
            start_line: fact.start_line,
            end_line: fact.end_line,
            location: fact.location.clone(),
            document: fact.document.clone(),
            code,
            region: None,
            attributes: BTreeMap::new(),
        });
    }
    let provenance = if artifact.language == "ocr" {
        vec![ocr_provenance(artifact)?]
    } else {
        vec![]
    };
    let payload = ExtractionPayload {
        continuity_coverage: BTreeMap::new(),
        contract_version: 1,
        extraction_method: artifact.extraction_method.clone(),
        parse_status: artifact.parse_status.clone(),
        provenance,
        text,
        facts,
        syntax_identities,
        chunks,
        regions: vec![],
        relationships: vec![],
        continuity: vec![],
        diagnostics: vec![],
        partial: artifact.parse_status == "partial-syntax",
    };
    payload.validate_with_text_limit(if native_text { 16_777_216 } else { 4_194_304 })?;
    Ok(payload)
}

type IdentityTuple = (usize, usize, String);

fn parse_identity(value: &str, path: &str, text: &str) -> Result<IdentityTuple> {
    // Parse the complete known grammar, then reconstruct it for exact equality.
    // This is never replacement of an arbitrary string prefix.
    let mut fields = value.rsplitn(4, ':');
    let kind = fields.next().context("missing syntax kind")?;
    let end_raw = fields.next().context("missing syntax end")?;
    let start_raw = fields.next().context("missing syntax start")?;
    let source = fields.next().context("missing syntax source")?;
    ensure!(
        source == format!("syntax:{path}") && !kind.is_empty() && kind.len() <= 128,
        "unknown or foreign syntax identity"
    );
    let start: usize = start_raw.parse()?;
    let end: usize = end_raw.parse()?;
    ensure!(
        value == format!("syntax:{path}:{start}:{end}:{kind}"),
        "noncanonical syntax identity"
    );
    airk_core::location::ByteSpan { start, end }.validate(text)?;
    Ok((start, end, kind.into()))
}

fn convert_code(
    code: &CodeEvidence,
    path: &str,
    text: &str,
    identities: &BTreeMap<IdentityTuple, LocalSyntaxId>,
) -> Result<LocalCodeEvidence> {
    let syntax_id = identities[&parse_identity(&code.symbol_id, path, text)?];
    let owner_id = code
        .owner_id
        .as_ref()
        .map(|id| parse_identity(id, path, text).map(|key| identities[&key]))
        .transpose()?;
    Ok(LocalCodeEvidence {
        fingerprints: code.fingerprints.clone(),
        syntax_id,
        owner_id,
        qualified_name: code.qualified_name.clone(),
        scope: code.scope.clone(),
        signature: code.signature.clone(),
        span: code.span.clone(),
        signature_span: code.signature_span.clone(),
        parameters: code.parameters.clone(),
        return_type: code.return_type.clone(),
        modifiers: code.modifiers.clone(),
        documentation: code.documentation.clone(),
        target: code.target.clone(),
        basis: code.basis.clone(),
        producer: code.producer.clone(),
        producer_version: code.producer_version.clone(),
        coverage: code.coverage.clone(),
        details_truncated: code.details_truncated,
    })
}

fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

fn validate_ocr(artifact: &Artifact) -> Result<crate::ocr::OcrProvenance> {
    ensure!(
        artifact.parse_status.len() <= 65_536,
        "legacy OCR provenance exceeds limit"
    );
    let provenance: crate::ocr::OcrProvenance = serde_json::from_str(&artifact.parse_status)?;
    ensure!(
        provenance.schema == 1 && provenance.source_sha256 == artifact.content_hash,
        "legacy OCR source/provenance mismatch"
    );
    ensure!(
        artifact.extraction_method
            == format!(
                "local-tesseract-ocr/1:{}",
                digest(artifact.parse_status.as_bytes())
            ),
        "legacy OCR method/provenance mismatch"
    );
    ensure!(
        provenance.first_page > 0
            && provenance.first_page <= provenance.last_page
            && provenance.last_page <= 512
            && provenance.pages_with_text <= provenance.last_page - provenance.first_page + 1,
        "invalid legacy OCR page selection"
    );
    ensure!(
        provenance.traineddata_sha256.len() <= 254,
        "legacy OCR asset limit exceeded"
    );
    Ok(provenance)
}

fn ocr_provenance(artifact: &Artifact) -> Result<ExtractionProvenance> {
    let p = validate_ocr(artifact)?;
    let mut assets = BTreeMap::from([("tesseract".into(), p.tesseract_sha256.parse()?)]);
    if let Some(renderer) = p.renderer_sha256 {
        assets.insert("renderer".into(), renderer.parse()?);
    }
    for (language, hash) in p.traineddata_sha256 {
        assets.insert(format!("traineddata/{language}"), hash.parse()?);
    }
    Ok(ExtractionProvenance {
        basis: EvidenceBasis::Imported,
        producer: "local-tesseract-ocr".into(),
        producer_version: "1".into(),
        attributes: BTreeMap::from([
            ("language".into(), p.language),
            ("first_page".into(), p.first_page.to_string()),
            ("last_page".into(), p.last_page.to_string()),
            ("pages_with_text".into(), p.pages_with_text.to_string()),
            ("dpi".into(), p.dpi.to_string()),
            ("coverage".into(), p.coverage),
        ]),
        asset_digests: assets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::binding::{ArtifactBinding, bind_payload};
    use std::io::{Cursor, Write};

    fn extract(path: &str, bytes: &[u8]) -> Result<Artifact> {
        let output = crate::documents::extract(path, bytes)?;
        Ok(Artifact {
            path: path.into(),
            content_hash: digest(bytes).to_string(),
            bytes: bytes.len(),
            chunks: output.chunks,
            language: output.language,
            extraction_method: output.method,
            parse_status: output.parse_status,
            facts: output.facts,
            retained: None,
        })
    }

    fn roundtrip(artifact: &Artifact, bytes: &[u8]) -> Result<ExtractionPayload> {
        let profile = legacy_profile(artifact)?;
        let payload = convert_native_legacy(artifact, bytes, &profile)?;
        let bound = bind_payload(
            &payload,
            &profile,
            ArtifactBinding {
                path: &artifact.path,
                source_hash: artifact.content_hash.parse()?,
                source_bytes: artifact.bytes,
            },
        )?;
        assert_eq!(
            serde_json::to_value(bound)?,
            serde_json::to_value(artifact)?
        );
        Ok(payload)
    }

    #[test]
    fn legacy_short_lines_over_ten_thousand_chunks_remain_convertible() -> Result<()> {
        let bytes = vec![b'\n'; 400_041];
        let artifact = extract("blank.txt", &bytes)?;
        assert_eq!(artifact.chunks.len(), 10_002);
        assert!(artifact.facts.is_empty());
        let payload = roundtrip(&artifact, &bytes)?;
        assert_eq!(payload.chunks.len(), artifact.chunks.len());
        assert_eq!(payload.text.as_bytes(), bytes);
        Ok(())
    }

    #[test]
    fn code_roundtrip_preserves_shared_nodes_and_nonemitted_owners() -> Result<()> {
        let bytes = b"class A { run() { target(); } }\n";
        let mut artifact = extract("@alias/src/alpha.ts", bytes)?;
        artifact.facts.retain(|fact| fact.name != "A");
        let mut cue = artifact
            .facts
            .iter()
            .find(|fact| fact.name == "run")
            .context("method fixture")?
            .clone();
        cue.kind = "test_declaration".into();
        let code = cue.code.as_mut().unwrap();
        code.basis = EvidenceBasis::StructuralRule;
        code.producer = "airk-framework-cues".into();
        artifact.facts.push(cue);
        let payload = roundtrip(&artifact, bytes)?;
        let nodes: Vec<_> = payload
            .facts
            .iter()
            .filter(|fact| fact.name == "run")
            .map(|fact| fact.code.as_ref().unwrap().syntax_id)
            .collect();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0], nodes[1]);
        assert!(
            payload
                .syntax_identities
                .iter()
                .any(|node| node.syntax_kind == "class_declaration")
        );
        let profile = legacy_profile(&artifact)?;
        let moved = bind_payload(
            &payload,
            &profile,
            ArtifactBinding {
                path: "moved/alpha.ts",
                source_hash: artifact.content_hash.parse()?,
                source_bytes: artifact.bytes,
            },
        )?;
        assert!(
            moved
                .facts
                .iter()
                .filter_map(|f| f.code.as_ref())
                .all(|code| code.symbol_id.starts_with("syntax:moved/alpha.ts:")
                    && code
                        .owner_id
                        .as_ref()
                        .is_none_or(|id| id.starts_with("syntax:moved/alpha.ts:")))
        );
        assert!(!serde_json::to_string(&payload)?.contains("@alias/src/alpha.ts"));
        Ok(())
    }

    #[test]
    fn documents_keep_original_locations_and_table_metadata() -> Result<()> {
        for (path, bytes) in [
            (
                "notes.md",
                "# Release\nREQ-17 must complete within 3 seconds.\n".as_bytes(),
            ),
            ("rows.csv", "Name,Value\nfiction,42\n".as_bytes()),
            (
                "shape.svg",
                "<svg><rect id=\"fiction\" width=\"42\"/></svg>".as_bytes(),
            ),
        ] {
            roundtrip(&extract(path, bytes)?, bytes)?;
        }
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )?;
        zip.write_all(br#"<w:document xmlns:w="urn:fixture"><w:body><w:p/><w:p><w:r><w:t>REQ-19 must run.</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>Name</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Value</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>fiction</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>42</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#)?;
        let bytes = zip.finish()?.into_inner();
        let artifact = extract("notes.docx", &bytes)?;
        assert!(
            artifact
                .facts
                .iter()
                .any(|f| f.document.as_ref().is_some_and(|d| d.table.is_some()))
        );
        roundtrip(&artifact, &bytes)?;
        Ok(())
    }

    #[test]
    fn empty_ocr_keeps_source_assets_selection_and_status_without_execution() -> Result<()> {
        let bytes = b"fictional original image bytes";
        let provenance = crate::ocr::OcrProvenance {
            schema: 1,
            source_sha256: digest(bytes).to_string(),
            tesseract_sha256: digest(b"tesseract").to_string(),
            renderer_sha256: None,
            traineddata_sha256: BTreeMap::from([(
                "eng".into(),
                digest(b"traineddata").to_string(),
            )]),
            language: "eng".into(),
            first_page: 1,
            last_page: 1,
            pages_with_text: 0,
            dpi: 150,
            coverage: "fixture only".into(),
        };
        let status = serde_json::to_string(&provenance)?;
        let artifact = Artifact {
            path: "scan.png".into(),
            content_hash: digest(bytes).to_string(),
            bytes: bytes.len(),
            chunks: vec![],
            language: "ocr".into(),
            extraction_method: format!("local-tesseract-ocr/1:{}", digest(status.as_bytes())),
            parse_status: status,
            facts: vec![],
            retained: None,
        };
        let payload = roundtrip(&artifact, bytes)?;
        assert!(payload.facts.is_empty());
        assert_eq!(
            payload.provenance[0].asset_digests["traineddata/eng"],
            digest(b"traineddata")
        );
        let mut changed = artifact.clone();
        changed.parse_status = changed.parse_status.replace("150", "200");
        changed.extraction_method = format!(
            "local-tesseract-ocr/1:{}",
            digest(changed.parse_status.as_bytes())
        );
        assert_ne!(
            legacy_profile(&artifact)?.identity()?,
            legacy_profile(&changed)?.identity()?
        );
        Ok(())
    }

    #[test]
    fn spreadsheet_formula_cache_and_original_addresses_survive_conversion() -> Result<()> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (path, xml) in [
            (
                "xl/workbook.xml",
                r#"<workbook xmlns:r="r"><sheets><sheet name="Fictional budget" r:id="s1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="s1" Type="o/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>Stored total</t></is></c><c r="B2"><f>SUM(2,3)</f><v>5</v></c></row></sheetData></worksheet>"#,
            ),
        ] {
            zip.start_file(path, zip::write::SimpleFileOptions::default())?;
            zip.write_all(xml.as_bytes())?;
        }
        let bytes = zip.finish()?.into_inner();
        let artifact = extract("budget.xlsx", &bytes)?;
        assert!(
            artifact
                .facts
                .iter()
                .filter_map(|fact| fact.document.as_ref()?.table.as_ref())
                .flat_map(|table| &table.cells)
                .any(|cell| cell.address.as_deref() == Some("B2")
                    && cell.formula.as_deref() == Some("SUM(2,3)")
                    && cell.cached_value)
        );
        roundtrip(&artifact, &bytes)?;
        Ok(())
    }

    #[test]
    fn foreign_ids_source_changes_and_new_profiles_fail_closed() -> Result<()> {
        let bytes = b"fn sample() {}\n";
        let artifact = extract("source.rs", bytes)?;
        let profile = legacy_profile(&artifact)?;
        assert!(convert_native_legacy(&artifact, b"fn altered() {}\n", &profile).is_err());
        let mut changed_profile = profile.clone();
        changed_profile
            .options
            .insert("new_option".into(), "enabled".into());
        assert!(convert_native_legacy(&artifact, bytes, &changed_profile).is_err());
        for id in [
            "scip foreign symbol",
            "syntax:other.rs:0:14:function_item",
            "syntax:source.rs:00:14:function_item",
        ] {
            let mut changed = artifact.clone();
            changed.facts[0].code.as_mut().unwrap().symbol_id = id.into();
            assert!(convert_native_legacy(&changed, bytes, &profile).is_err());
        }
        let mut imported = artifact.clone();
        imported.facts[0].code.as_mut().unwrap().basis = EvidenceBasis::Imported;
        assert!(convert_native_legacy(&imported, bytes, &profile).is_err());
        let payload = roundtrip(&artifact, bytes)?;
        assert!(
            bind_payload(
                &payload,
                &profile,
                ArtifactBinding {
                    path: "../outside.rs",
                    source_hash: digest(bytes),
                    source_bytes: bytes.len()
                }
            )
            .is_err()
        );
        Ok(())
    }
}
