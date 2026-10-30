// SPDX-License-Identifier: Apache-2.0
//! Native document adapters share extraction limits and original location labels.
use airk_core::{
    Chunk, DocumentLocation, Fact,
    extraction::{IndexDetail, LocalSpan},
};
use anyhow::{Context, Result, ensure};
use quick_xml::{Reader, events::Event};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
};

// Bound per-element work even with a linear-time duplicate-attribute parser.
const MAX_XML_ATTRIBUTES: usize = 1024;

/// A deterministic per-document work budget rejected this input. Kept distinct
/// from malformed or empty extraction so reports can name the cause.
#[derive(Debug)]
pub struct BudgetExceeded(pub &'static str);
impl std::fmt::Display for BudgetExceeded {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}
impl std::error::Error for BudgetExceeded {}

pub(crate) fn within(condition: bool, limit: &'static str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(BudgetExceeded(limit).into())
    }
}

/// True when any cause in the chain is a document work-budget rejection.
pub fn budget_exceeded(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| cause.is::<BudgetExceeded>())
}

pub(crate) const MAX_EXTRACTED: usize = airk_core::extraction::MAX_TEXT_BYTES;
/// Keep costly syntax/structured parsing within the established input bound.
/// Larger UTF-8 sources retain exact text and locations, with no claimed facts.
pub(crate) fn large_text(
    bytes: &[u8],
    profile: &airk_core::extraction::ExtractionProfile,
) -> Result<Option<Extracted>> {
    if bytes.len() <= crate::MAX_STRUCTURED_BYTES
        || matches!(
            profile.adapter_id.as_str(),
            "pdf" | "docx" | "xlsx" | "pptx"
        )
    {
        return Ok(None);
    }
    within(
        bytes.len() <= MAX_EXTRACTED,
        "text exceeds 32 MiB extraction limit",
    )?;
    let text = std::str::from_utf8(bytes).context("binary or non-UTF-8; extractor unavailable")?;
    ensure!(!text.contains('\0'), "binary data; extractor unavailable");
    Ok(Some(Extracted {
        chunks: crate::chunk(text),
        language: profile.language.clone(),
        method: "utf8".into(),
        parse_status: "partial-text-only: source exceeds 4 MiB structured parsing budget; symbols, relationships and document facts unavailable".into(),
        facts: vec![],
        semantic_seeds: Some(crate::semantic_projection::SemanticSeeds::default()),
    }))
}
pub struct Extracted {
    pub chunks: Vec<Chunk>,
    pub language: String,
    pub method: String,
    pub parse_status: String,
    pub facts: Vec<Fact>,
    pub(crate) semantic_seeds: Option<crate::semantic_projection::SemanticSeeds>,
}

pub fn extract(path: &str, bytes: &[u8]) -> Result<Extracted> {
    let registry = crate::adapters::registry::AdapterRegistry::baseline();
    let policy = crate::adapters::profile::ExtractionPolicy {
        detail: None,
        version: 1,
        rules: vec![],
    };
    let selected = policy.select_with_prefix(&registry, path, bytes)?;
    extract_selected(path, bytes, &selected.profile)
}

pub(crate) fn extract_selected(
    path: &str,
    bytes: &[u8],
    profile: &airk_core::extraction::ExtractionProfile,
) -> Result<Extracted> {
    let registry = crate::adapters::registry::AdapterRegistry::baseline();
    registry.validate_effective_profile(path, profile)?;
    if let Some(extracted) = large_text(bytes, profile)? {
        return Ok(extracted);
    }
    let adapter = profile.adapter_id.as_str();
    let detail = IndexDetail::from_profile(profile)?;
    match adapter {
        "pdf" => {
            let pages = crate::pdf_worker::extract(bytes)?;
            from_blocks_selected(
                "pdf",
                "pdf-text-layer",
                pages
                    .into_iter()
                    .enumerate()
                    .map(|(i, t)| (t, DocumentLocation::Page { number: i + 1 }))
                    .collect(),
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeSet::new(),
                detail,
            )
        }
        "docx" => {
            let mut archive =
                zip::ZipArchive::new(Cursor::new(bytes)).context("invalid DOCX archive")?;
            within(
                archive.len() <= 10_000,
                "OOXML archive exceeds 10000 entries",
            )?;
            let xml = read_part(&mut archive, "word/document.xml")?;
            require_root(&xml, b"document")?;
            let paragraphs = ooxml_paragraphs(&xml)?;
            let mut extracted = from_blocks(
                "docx",
                "docx-paragraphs-and-tables",
                paragraphs
                    .into_iter()
                    .enumerate()
                    .map(|(i, t)| (t, DocumentLocation::Block { number: i + 1 }))
                    .collect(),
                docx_heading_levels(&xml)?,
                BTreeMap::new(),
                crate::tables::docx_table_paragraphs(&xml)?,
            )?;
            crate::tables::append_docx_tables(&xml, &mut extracted)?;
            // Table rows append both original-location text and prose facts. Until
            // that producer has its own seed sink, derive all hints from its facts.
            extracted.semantic_seeds = None;
            Ok(extracted)
        }
        "xlsx" => crate::spreadsheets::extract(bytes),
        "pptx" => {
            let mut archive =
                zip::ZipArchive::new(Cursor::new(bytes)).context("invalid PPTX archive")?;
            within(
                archive.len() <= 10_000,
                "OOXML archive exceeds 10000 entries",
            )?;
            let presentation = read_part(&mut archive, "ppt/presentation.xml")?;
            let relationships = read_part(&mut archive, "ppt/_rels/presentation.xml.rels")?;
            let targets = relationship_targets(&relationships)?;
            let mut reader = Reader::from_str(&presentation);
            let mut slides = Vec::new();
            let mut titles = BTreeMap::new();
            let mut total = 0;
            loop {
                match reader.read_event()? {
                    Event::Empty(e) | Event::Start(e) if e.local_name().as_ref() == b"sldId" => {
                        let id = attribute(&e, b"r:id")?.context("slide relationship missing")?;
                        let target = targets.get(&id).context("slide relationship unresolved")?;
                        ensure!(
                            target.starts_with("slides/")
                                && !target.contains("..")
                                && !target.contains('\\'),
                            "unsupported slide relationship target"
                        );
                        let xml = read_part(&mut archive, &format!("ppt/{target}"))?;
                        total += xml.len();
                        within(
                            total <= MAX_EXTRACTED,
                            "PPTX XML exceeds 32 MiB extraction limit",
                        )?;
                        let text = ooxml_paragraphs(&xml)?.join("\n");
                        if let Some(title) = pptx_title(&xml)? {
                            titles.insert(slides.len() + 1, title);
                        }
                        slides.push((
                            text,
                            DocumentLocation::Slide {
                                number: slides.len() + 1,
                            },
                        ));
                        within(slides.len() <= 512, "PPTX exceeds 512 slides")?;
                    }
                    Event::Eof => break,
                    _ => {}
                }
            }
            from_blocks_selected(
                "pptx",
                "pptx-slide-text",
                slides,
                BTreeMap::new(),
                titles,
                BTreeSet::new(),
                detail,
            )
        }
        _ => {
            let text =
                std::str::from_utf8(bytes).context("binary or non-UTF-8; extractor unavailable")?;
            ensure!(!text.contains('\0'), "binary data; extractor unavailable");
            within(
                text.len() <= MAX_EXTRACTED,
                "text exceeds 32 MiB extraction limit",
            )?;
            let selected =
                crate::syntax::analyze_selected_with_detail(path, text, adapter, detail)?;
            let mut analysis = selected.analysis;
            let mut semantic_seeds = selected.seeds;
            if matches!(adapter, "markdown" | "instruction-markdown") {
                analysis.language = adapter.into();
                analysis.status = "structured-markdown".into();
                let prose = crate::prose::markdown_selected(text, detail)?;
                semantic_seeds = Some(prose.semantic_seeds(0)?);
                analysis.facts = prose.facts;
                if adapter == "instruction-markdown" && detail != IndexDetail::Essential {
                    crate::instruction_documents::enrich(path, text, &mut analysis.facts)?;
                }
            } else if matches!(adapter, "csv" | "tsv") {
                analysis.language = adapter.into();
                analysis.status = "structured-delimited".into();
                analysis.facts = delimited_facts(text, adapter == "tsv")?;
                semantic_seeds = None;
            } else if matches!(adapter, "xml" | "svg") {
                analysis.language = adapter.into();
                analysis.status = "structured-xml".into();
                analysis.facts = xml_facts(text)?;
                semantic_seeds = None;
            } else if adapter == "ini" {
                analysis.language = adapter.into();
                analysis.status = "line-structured".into();
                analysis.facts = line_facts(text, &analysis.language)?;
                semantic_seeds = None;
            }
            if analysis.language == "text" {
                let prose = crate::prose::plain_selected(text, &[], None, detail)?;
                semantic_seeds = Some(prose.semantic_seeds(0)?);
                analysis.facts = prose.facts;
                analysis.status = "prose-candidates".into();
            }
            Ok(Extracted {
                chunks: crate::chunk(text),
                language: analysis.language,
                method: "utf8".into(),
                parse_status: analysis.status,
                facts: analysis.facts,
                semantic_seeds,
            })
        }
    }
}

pub(crate) fn from_blocks(
    language: &str,
    method: &str,
    blocks: Vec<(String, DocumentLocation)>,
    heading_levels: BTreeMap<usize, u8>,
    titles: BTreeMap<usize, String>,
    excluded_prose: BTreeSet<usize>,
) -> Result<Extracted> {
    from_blocks_selected(
        language,
        method,
        blocks,
        heading_levels,
        titles,
        excluded_prose,
        IndexDetail::Detailed,
    )
}

fn from_blocks_selected(
    language: &str,
    method: &str,
    blocks: Vec<(String, DocumentLocation)>,
    heading_levels: BTreeMap<usize, u8>,
    titles: BTreeMap<usize, String>,
    excluded_prose: BTreeSet<usize>,
    detail: IndexDetail,
) -> Result<Extracted> {
    let mut chunks = Vec::new();
    let mut facts = Vec::new();
    let mut line = 1;
    let mut total = 0;
    let mut heading_context = Vec::new();
    let mut semantic_seeds = crate::semantic_projection::SemanticSeeds::default();
    for (ordinal, (text, location)) in blocks.into_iter().enumerate() {
        if text.trim().is_empty() {
            continue;
        }
        let text = format!("{}\n", text.trim_end());
        let block_offset = total;
        total += text.len();
        within(
            total <= MAX_EXTRACTED,
            "document exceeds 32 MiB extracted text",
        )?;
        let block_start = line;
        for mut chunk in crate::chunk(&text) {
            chunk.start_line += block_start - 1;
            chunk.end_line += block_start - 1;
            chunk.location = location.clone();
            line = chunk.end_line + 1;
            chunks.push(chunk);
        }
        if language == "pptx" {
            heading_context.clear();
        }
        if let Some(title) = titles.get(&(ordinal + 1))
            && let Some(offset) = text.find(title)
        {
            heading_context = vec![(1, title.clone())];
            let start = text[..offset].bytes().filter(|b| *b == b'\n').count() + block_start;
            let end = start + title.bytes().filter(|b| *b == b'\n').count();
            let local_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
            let title_end = offset + title.len();
            let local_end = text[title_end..]
                .find('\n')
                .map_or(text.len(), |index| title_end + index + 1);
            semantic_seeds.heading(
                None,
                LocalSpan {
                    start: block_offset + local_start,
                    end: block_offset + local_end,
                },
                1,
            )?;
            if detail != IndexDetail::Essential {
                facts.push(Fact {
                    code: None,
                    name: title.clone(),
                    kind: "section".into(),
                    syntax_kind: "pptx-title-placeholder".into(),
                    start_line: start,
                    end_line: end,
                    location: location.clone(),
                    document: Some(airk_core::DocumentEvidence {
                        parser_version: "document-cues/1".into(),
                        basis: "explicit-heading".into(),
                        heading_path: vec![title.clone()],
                        ..Default::default()
                    }),
                });
            }
        }
        if !excluded_prose.contains(&(ordinal + 1)) {
            let prose = crate::prose::plain_selected(
                &text,
                &heading_context,
                heading_levels.get(&(ordinal + 1)).copied(),
                detail,
            )?;
            semantic_seeds.extend(prose.semantic_seeds(block_offset)?)?;
            heading_context = prose.headings;
            for mut fact in prose.facts {
                fact.start_line += block_start - 1;
                fact.end_line += block_start - 1;
                fact.location = location.clone();
                facts.push(fact);
            }
        }
        if detail != IndexDetail::Essential {
            facts.push(Fact {
                code: None,
                name: text.chars().take(120).collect::<String>().trim().into(),
                kind: "section".into(),
                syntax_kind: method.into(),
                start_line: block_start,
                end_line: line - 1,
                location,
                document: None,
            });
        }
        within(facts.len() <= 10_000, "document exceeds 10000 facts")?;
    }
    ensure!(
        !chunks.is_empty(),
        "document contains no extractable text (OCR unavailable)"
    );
    Ok(Extracted {
        chunks,
        facts,
        language: language.into(),
        method: method.into(),
        parse_status: "extracted-document".into(),
        semantic_seeds: Some(semantic_seeds),
    })
}

pub(crate) fn read_part(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<String> {
    let file = archive
        .by_name(name)
        .with_context(|| format!("missing OOXML part {name}"))?;
    within(
        file.size() <= MAX_EXTRACTED as u64,
        "OOXML part exceeds 32 MiB",
    )?;
    let mut bytes = Vec::new();
    file.take((MAX_EXTRACTED + 1) as u64)
        .read_to_end(&mut bytes)?;
    within(
        bytes.len() <= MAX_EXTRACTED,
        "OOXML expanded part exceeds 32 MiB",
    )?;
    let xml = String::from_utf8(bytes).context("OOXML part is not UTF-8")?;
    let mut reader = Reader::from_str(&xml);
    let mut depth = 0usize;
    let mut roots = 0usize;
    loop {
        match reader.read_event()? {
            Event::Start(element) => {
                for (index, value) in element.attributes().enumerate() {
                    within(
                        index < MAX_XML_ATTRIBUTES,
                        "XML element exceeds 1024 attributes",
                    )?;
                    value?;
                }
                if depth == 0 {
                    roots += 1;
                }
                depth += 1;
                within(depth <= 128, "OOXML nesting exceeds 128 levels")?;
            }
            Event::Empty(element) => {
                for (index, value) in element.attributes().enumerate() {
                    within(
                        index < MAX_XML_ATTRIBUTES,
                        "XML element exceeds 1024 attributes",
                    )?;
                    value?;
                }
                if depth == 0 {
                    roots += 1;
                }
            }
            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .context("unexpected OOXML closing element")?;
            }
            Event::Text(text) if depth == 0 => ensure!(
                text.decode()?.trim().is_empty(),
                "OOXML text outside document root"
            ),
            Event::CData(_) | Event::GeneralRef(_) if depth == 0 => {
                anyhow::bail!("OOXML content outside document root")
            }
            Event::DocType(_) => anyhow::bail!("OOXML document type declarations are unsupported"),
            Event::Eof => break,
            _ => {}
        }
    }
    ensure!(
        depth == 0 && roots == 1,
        "incomplete or multiple OOXML document roots"
    );
    Ok(xml)
}

#[cfg(test)]
mod selected_tests {
    use super::*;
    use airk_core::{
        Artifact,
        extraction::{ExtractionPayload, ExtractionProfile},
    };
    use sha2::{Digest, Sha256};

    fn prepare(
        path: &str,
        bytes: &[u8],
        profile: &ExtractionProfile,
        extracted: Extracted,
        explicit: bool,
    ) -> Result<ExtractionPayload> {
        let seeds = extracted.semantic_seeds;
        let artifact = Artifact {
            path: path.into(),
            content_hash: format!("{:x}", Sha256::digest(bytes)),
            bytes: bytes.len(),
            chunks: extracted.chunks,
            language: extracted.language,
            extraction_method: extracted.method,
            parse_status: extracted.parse_status,
            facts: extracted.facts,
            retained: None,
        };
        crate::continuity::prepare_native_with_seeds(
            &artifact,
            bytes,
            profile,
            if explicit { seeds.as_ref() } else { None },
        )
    }

    #[test]
    fn selected_prose_payloads_match_full_fact_reference_at_every_detail() -> Result<()> {
        let registry = crate::adapters::registry::AdapterRegistry::baseline();
        let text = "  # Café `API`\r\n\r\n### Requirements 2026-10-13\r\n\r\nREQ-1: The system must use 200 ms.\r\n\r\n## Sibling\r\n\r\n| ID | Value |\r\n|---|---|\r\n| X | 10 MiB |\r\n\r\n```text\r\n# Hidden\r\n```\r\n\r\nTail 界.\r\n";
        for path in ["a.md", "a.txt", "AGENTS.md"] {
            let base = crate::adapters::profile::ExtractionPolicy {
                version: 3,
                detail: None,
                rules: vec![],
            }
            .select_with_prefix(&registry, path, text.as_bytes())?
            .profile;
            for detail in [
                IndexDetail::Essential,
                IndexDetail::Balanced,
                IndexDetail::Detailed,
            ] {
                let mut profile = base.clone();
                profile
                    .options
                    .insert("index_detail".into(), detail.as_str().into());
                let reference = prepare(
                    path,
                    text.as_bytes(),
                    &profile,
                    extract_selected(path, text.as_bytes(), &base)?,
                    false,
                )?;
                let selected = extract_selected(path, text.as_bytes(), &profile)?;
                assert!(selected.semantic_seeds.is_some());
                if detail == IndexDetail::Essential {
                    assert!(selected.facts.is_empty());
                }
                let selected = prepare(path, text.as_bytes(), &profile, selected, true)?;
                assert_eq!(
                    serde_json::to_value(&selected)?,
                    serde_json::to_value(&reference)?,
                    "{path} {detail:?}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn native_seed_offsets_titles_and_hard_locations_match_reference() -> Result<()> {
        let registry = crate::adapters::registry::AdapterRegistry::baseline();
        for language in ["pdf", "pptx"] {
            let blocks = vec![
                (
                    "  Café\r\nRequirements\r\nThe service must use 20 ms.\r\n".into(),
                    if language == "pdf" {
                        DocumentLocation::Page { number: 2 }
                    } else {
                        DocumentLocation::Slide { number: 2 }
                    },
                ),
                (
                    "Second\n## Deep\nTail 界.\n".into(),
                    if language == "pdf" {
                        DocumentLocation::Page { number: 4 }
                    } else {
                        DocumentLocation::Slide { number: 4 }
                    },
                ),
            ];
            let titles = if language == "pptx" {
                BTreeMap::from([(1, "Café".into()), (2, "Second".into())])
            } else {
                BTreeMap::new()
            };
            let base = registry.get(language).unwrap().profile.clone();
            for detail in [
                IndexDetail::Essential,
                IndexDetail::Balanced,
                IndexDetail::Detailed,
            ] {
                let mut profile = base.clone();
                profile
                    .options
                    .insert("index_detail".into(), detail.as_str().into());
                let full = from_blocks(
                    language,
                    "fixture",
                    blocks.clone(),
                    BTreeMap::new(),
                    titles.clone(),
                    BTreeSet::new(),
                )?;
                let selected = from_blocks_selected(
                    language,
                    "fixture",
                    blocks.clone(),
                    BTreeMap::new(),
                    titles.clone(),
                    BTreeSet::new(),
                    detail,
                )?;
                let reference = prepare(
                    &format!("a.{language}"),
                    b"native fixture",
                    &profile,
                    full,
                    false,
                )?;
                let selected = prepare(
                    &format!("a.{language}"),
                    b"native fixture",
                    &profile,
                    selected,
                    true,
                )?;
                assert_eq!(
                    serde_json::to_value(&selected)?,
                    serde_json::to_value(&reference)?,
                    "{language} {detail:?}"
                );
                assert!(
                    selected
                        .chunks
                        .iter()
                        .all(|chunk| !matches!(chunk.location, DocumentLocation::Lines { .. }))
                );
            }
        }
        Ok(())
    }

    #[test]
    fn essential_docx_fallback_keeps_table_text_and_native_locations() -> Result<()> {
        use std::io::Write;
        let xml = r#"<w:document xmlns:w="word"><w:body><w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Requirements</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>Identifier</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Requirement</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>REQ-2</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>The API must use 20 ms.</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#;
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )?;
        writer.write_all(xml.as_bytes())?;
        let bytes = writer.finish()?.into_inner();
        let registry = crate::adapters::registry::AdapterRegistry::baseline();
        let base = registry.get("docx").unwrap().profile.clone();
        let mut profile = base.clone();
        profile
            .options
            .insert("index_detail".into(), "essential".into());
        let selected = extract_selected("a.docx", &bytes, &profile)?;
        assert!(selected.semantic_seeds.is_none());
        let reference = prepare(
            "a.docx",
            &bytes,
            &profile,
            extract_selected("a.docx", &bytes, &base)?,
            false,
        )?;
        let selected = prepare("a.docx", &bytes, &profile, selected, true)?;
        assert_eq!(
            serde_json::to_value(&selected)?,
            serde_json::to_value(&reference)?
        );
        assert!(selected.text.contains("REQ-2"));
        assert!(selected.text.contains("The API must use 20 ms."));
        assert!(
            selected
                .chunks
                .iter()
                .any(|chunk| matches!(chunk.location, DocumentLocation::TableRow { .. }))
        );
        Ok(())
    }
}

pub(crate) fn require_root(xml: &str, expected: &[u8]) -> Result<()> {
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event()? {
            Event::Start(element) | Event::Empty(element) => {
                ensure!(
                    element.local_name().as_ref() == expected,
                    "OOXML part has the wrong document root"
                );
                return Ok(());
            }
            Event::DocType(_) => anyhow::bail!("OOXML DTDs are unsupported"),
            Event::Eof => anyhow::bail!("OOXML part has no document root"),
            _ => (),
        }
    }
}

fn ooxml_paragraphs(xml: &str) -> Result<Vec<String>> {
    let mut reader = Reader::from_str(xml);
    let mut paragraphs = Vec::new();
    let mut paragraph = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event()? {
            Event::Start(e) if e.local_name().as_ref() == b"p" => paragraph.clear(),
            Event::Empty(e) if e.local_name().as_ref() == b"p" => {
                paragraphs.push(String::new());
                within(
                    paragraphs.len() <= 10_000,
                    "document exceeds 10000 paragraphs",
                )?;
            }
            Event::Start(e) if e.local_name().as_ref() == b"t" => in_text = true,
            Event::Text(e) if in_text => paragraph.push_str(&e.decode()?),
            Event::CData(e) if in_text => paragraph.push_str(&e.decode()?),
            Event::GeneralRef(e) if in_text => paragraph.push_str(&entity(&e.decode()?)?),
            Event::Empty(e) if e.local_name().as_ref() == b"tab" => paragraph.push('\t'),
            Event::Empty(e) if e.local_name().as_ref() == b"br" => paragraph.push('\n'),
            Event::End(e) if e.local_name().as_ref() == b"t" => in_text = false,
            Event::End(e) if e.local_name().as_ref() == b"p" => {
                paragraphs.push(std::mem::take(&mut paragraph));
                within(
                    paragraphs.len() <= 10_000,
                    "document exceeds 10000 paragraphs",
                )?;
            }
            Event::DocType(_) => anyhow::bail!("document type declarations are unsupported"),
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(paragraphs)
}

fn docx_heading_levels(xml: &str) -> Result<BTreeMap<usize, u8>> {
    let mut reader = Reader::from_str(xml);
    let mut ordinal = 0;
    let mut headings = BTreeMap::new();
    loop {
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == b"p" => ordinal += 1,
            Event::Empty(e) | Event::Start(e)
                if matches!(e.local_name().as_ref(), b"pStyle" | b"outlineLvl") =>
            {
                let Some(value) = attribute(&e, b"w:val")? else {
                    continue;
                };
                let style = value.to_ascii_lowercase().replace(' ', "");
                let level = if e.local_name().as_ref() == b"outlineLvl" {
                    value
                        .parse::<u8>()
                        .ok()
                        .filter(|level| *level <= 8)
                        .map(|level| level + 1)
                } else if style == "title" {
                    Some(1)
                } else {
                    style
                        .strip_prefix("heading")
                        .and_then(|level| level.parse::<u8>().ok())
                        .filter(|level| (1..=9).contains(level))
                };
                if let Some(level) = level {
                    headings.insert(ordinal, level);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(headings)
}

fn pptx_title(xml: &str) -> Result<Option<String>> {
    let mut reader = Reader::from_str(xml);
    let mut in_shape = false;
    let mut title = false;
    let mut in_text = false;
    let mut value = String::new();
    loop {
        match reader.read_event()? {
            Event::Start(e) if e.local_name().as_ref() == b"sp" => {
                in_shape = true;
                title = false;
                value.clear();
            }
            Event::Empty(e) | Event::Start(e) if in_shape && e.local_name().as_ref() == b"ph" => {
                title = matches!(
                    attribute(&e, b"type")?.as_deref(),
                    Some("title" | "ctrTitle")
                );
            }
            Event::Start(e) if in_shape && e.local_name().as_ref() == b"t" => in_text = true,
            Event::Text(e) if in_text => value.push_str(&e.decode()?),
            Event::CData(e) if in_text => value.push_str(&e.decode()?),
            Event::GeneralRef(e) if in_text => value.push_str(&entity(&e.decode()?)?),
            Event::Empty(e) if in_shape && e.local_name().as_ref() == b"br" => value.push('\n'),
            Event::Empty(e) if in_shape && e.local_name().as_ref() == b"tab" => value.push('\t'),
            Event::End(e) if e.local_name().as_ref() == b"t" => in_text = false,
            Event::End(e) if in_shape && e.local_name().as_ref() == b"p" => value.push('\n'),
            Event::End(e) if e.local_name().as_ref() == b"sp" => {
                if title && !value.trim().is_empty() && value.len() <= 160 {
                    return Ok(Some(value.trim().into()));
                }
                in_shape = false;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(None)
}

pub(crate) fn entity(name: &str) -> Result<String> {
    Ok(quick_xml::escape::unescape(&format!("&{name};"))?.into_owned())
}
/// Preserve the pre-0.41 UTF-8/unescape behavior: the replacement upstream
/// attribute API also normalizes literal whitespace, changing stored evidence.
pub(crate) fn attribute_value(
    attr: &quick_xml::events::attributes::Attribute<'_>,
) -> Result<String> {
    Ok(quick_xml::escape::unescape(std::str::from_utf8(attr.value.as_ref())?)?.into_owned())
}
pub(crate) fn attribute(
    e: &quick_xml::events::BytesStart<'_>,
    name: &[u8],
) -> Result<Option<String>> {
    let mut value = None;
    for (index, attr) in e.attributes().enumerate() {
        within(
            index < MAX_XML_ATTRIBUTES,
            "XML element exceeds 1024 attributes",
        )?;
        let attr = attr?;
        if attr.key.as_ref() == name {
            value = Some(attribute_value(&attr)?);
        }
    }
    Ok(value)
}
fn relationship_targets(xml: &str) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event()? {
            Event::Empty(e) | Event::Start(e) if e.local_name().as_ref() == b"Relationship" => {
                if attribute(&e, b"TargetMode")?.as_deref() == Some("External") {
                    continue;
                }
                if let (Some(id), Some(target)) = (attribute(&e, b"Id")?, attribute(&e, b"Target")?)
                {
                    result.insert(id, target);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(result)
}

fn delimited_facts(text: &str, tsv: bool) -> Result<Vec<Fact>> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(if tsv { b'\t' } else { b',' })
        .from_reader(text.as_bytes());
    let headers = reader.headers()?.clone();
    within(
        headers.len() <= 10_000,
        "delimited document exceeds 10000 headers",
    )?;
    within(
        headers.iter().all(|name| name.len() <= 1024),
        "column header exceeds 1024 bytes",
    )?;
    let start = headers
        .position()
        .map_or(0, |position| position.byte() as usize);
    // csv records the position before ignored blank lines, so advance only over
    // newline bytes (spaces are real field contents).
    let start = start
        + text[start..]
            .bytes()
            .take_while(|b| matches!(b, b'\r' | b'\n'))
            .count();
    let end = reader.position().byte() as usize;
    // A quoted header can span several source lines, and blank rows can precede it.
    // Cite the header record rather than inventing line 1 for every column.
    let start_line = text[..start].bytes().filter(|b| *b == b'\n').count() + 1;
    let end_line = text[..end]
        .trim_end_matches(['\r', '\n'])
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1;
    for record in reader.records() {
        record?;
    }
    Ok(headers
        .iter()
        .map(|name| Fact {
            code: None,
            name: name.into(),
            kind: "key".into(),
            syntax_kind: "column-header".into(),
            start_line,
            end_line,
            location: DocumentLocation::Lines {
                start: start_line,
                end: end_line,
            },
            document: None,
        })
        .collect())
}

fn xml_facts(text: &str) -> Result<Vec<Fact>> {
    let mut reader = Reader::from_str(text);
    let mut facts = Vec::new();
    loop {
        let start = reader.buffer_position() as usize;
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let end = reader.buffer_position() as usize;
                facts.push(line_fact(text, start, end, name, "element", "xml-element"));
                for (index, attr) in e.attributes().enumerate() {
                    within(
                        index < MAX_XML_ATTRIBUTES,
                        "XML element exceeds 1024 attributes",
                    )?;
                    let attr = attr?;
                    let key = String::from_utf8_lossy(attr.key.as_ref());
                    if matches!(
                        key.as_ref(),
                        "id" | "class"
                            | "viewBox"
                            | "width"
                            | "height"
                            | "href"
                            | "xlink:href"
                            | "aria-label"
                            | "name"
                    ) {
                        let value = attribute_value(&attr)?;
                        let name: String = format!("{key}={value}").chars().take(1024).collect();
                        facts.push(line_fact(text, start, end, name, "key", "xml-attribute"));
                    }
                }
                within(facts.len() <= 10_000, "XML exceeds 10000 facts")?;
            }
            Event::DocType(_) => anyhow::bail!("XML document type declarations are unsupported"),
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(facts)
}

fn line_facts(text: &str, language: &str) -> Result<Vec<Fact>> {
    let mut facts = Vec::new();
    for (ordinal, line) in text.split_inclusive('\n').enumerate() {
        let trimmed = line.trim();
        let fact: Option<(String, &str)> =
            if language == "dockerfile" && !trimmed.starts_with('#') && !trimmed.is_empty() {
                Some((
                    trimmed
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_uppercase(),
                    "directive",
                ))
            } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
                Some((trimmed.into(), "section"))
            } else if !trimmed.starts_with(['#', ';']) {
                trimmed
                    .split_once('=')
                    .or_else(|| trimmed.split_once(':'))
                    .map(|(k, _)| (k.trim().into(), "key"))
            } else {
                None
            };
        if let Some((name, kind)) = fact {
            within(facts.len() < 10_000, "configuration exceeds 10000 facts")?;
            within(name.len() <= 1024, "configuration key exceeds 1024 bytes")?;
            let line_number = ordinal + 1;
            facts.push(Fact {
                code: None,
                name,
                kind: kind.into(),
                syntax_kind: "line-directive".into(),
                start_line: line_number,
                end_line: line_number,
                location: DocumentLocation::Lines {
                    start: line_number,
                    end: line_number,
                },
                document: None,
            });
        }
    }
    Ok(facts)
}

fn line_fact(
    text: &str,
    start: usize,
    end: usize,
    name: String,
    kind: &str,
    syntax_kind: &str,
) -> Fact {
    let start_line = text[..start.min(text.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1;
    let end_line = text[..end.min(text.len())]
        .trim_end_matches('\n')
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1;
    Fact {
        code: None,
        name,
        kind: kind.into(),
        syntax_kind: syntax_kind.into(),
        start_line,
        end_line: end_line.max(start_line),
        document: None,
        location: DocumentLocation::Lines {
            start: start_line,
            end: end_line.max(start_line),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn large_text_is_exact_partial_and_rejects_binary_or_excess_instead_of_truncating() -> Result<()>
    {
        let registry = crate::adapters::registry::AdapterRegistry::baseline();
        let profile = registry.effective_profile("large.ts", "typescript")?;
        let mut bytes = vec![b'x'; MAX_EXTRACTED];
        let extracted = large_text(&bytes, &profile)?.expect("large source uses text fallback");
        assert!(extracted.parse_status.starts_with("partial-text-only:"));
        assert!(extracted.facts.is_empty());
        assert_eq!(extracted.chunks.len(), 1);
        assert_eq!(extracted.chunks[0].text.as_bytes(), bytes);
        drop(extracted);
        bytes.push(b'x');
        assert!(
            large_text(&bytes, &profile)
                .err()
                .unwrap()
                .to_string()
                .contains("32 MiB")
        );
        bytes.truncate(crate::MAX_STRUCTURED_BYTES + 1);
        bytes[0] = 0;
        assert!(
            large_text(&bytes, &profile)
                .err()
                .unwrap()
                .to_string()
                .contains("binary")
        );
        bytes[0] = 255;
        assert!(
            large_text(&bytes, &profile)
                .err()
                .unwrap()
                .to_string()
                .contains("non-UTF-8")
        );
        Ok(())
    }

    #[test]
    fn structured_header_locations_and_configuration_limits() -> Result<()> {
        let facts = delimited_facts("\n\"first\ncolumn\",second\nvalue,other\n", false)?;
        assert_eq!(facts[0].name, "first\ncolumn");
        assert_eq!((facts[0].start_line, facts[0].end_line), (2, 3));
        assert!(delimited_facts(&"x,".repeat(10_001), false).is_err());
        assert!(line_facts(&"key=value\n".repeat(10_001), "ini").is_err());
        let facts = line_facts("; comment\n\n[section]\nname=value\n", "ini")?;
        assert_eq!(facts[1].start_line, 4);
        let docker = extract("dockerfile", b"FROM scratch\n")?;
        assert_eq!(docker.language, "dockerfile");
        assert_eq!(docker.facts[0].kind, "directive");
        Ok(())
    }

    #[test]
    fn empty_docx_paragraphs_preserve_original_block_ordinals() -> Result<()> {
        let bytes = archive(&[(
            "word/document.xml",
            "<w:document xmlns:w=\"urn:fixture\"><w:body><w:p/><w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>Overview</w:t></w:r></w:p></w:body></w:document>",
        )])?;
        let doc = extract("fixture.docx", &bytes)?;
        assert_eq!(
            doc.chunks[0].location,
            DocumentLocation::Block { number: 2 }
        );
        assert!(doc.facts.iter().any(|f| {
            f.name == "Overview"
                && f.document
                    .as_ref()
                    .is_some_and(|d| d.heading_path == ["Overview"])
        }));
        Ok(())
    }

    fn archive(parts: &[(&str, &str)]) -> Result<Vec<u8>> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in parts {
            writer.start_file(*name, zip::write::SimpleFileOptions::default())?;
            writer.write_all(content.as_bytes())?;
        }
        Ok(writer.finish()?.into_inner())
    }

    #[test]
    fn docx_runs_entities_tables_and_block_provenance() -> Result<()> {
        let bytes = archive(&[(
            "word/document.xml",
            r#"<w:document xmlns:w="word"><w:body><w:p><w:r><w:t>A &amp; B</w:t></w:r><w:r><w:t> needle</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>Table cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#,
        )])?;
        let result = extract("a.docx", &bytes)?;
        assert_eq!(result.chunks[0].text, "A & B needle\n");
        assert_eq!(result.chunks[1].text, "Table cell\n");
        assert_eq!(
            result.chunks[0].location,
            DocumentLocation::Block { number: 1 }
        );
        Ok(())
    }

    #[test]
    fn docx_heading_styles_and_document_requirements_preserve_original_blocks() -> Result<()> {
        let bytes = archive(&[(
            "word/document.xml",
            r#"<w:document xmlns:w="word"><w:body><w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Platform specification</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Heading3"/></w:pPr><w:r><w:t>Performance requirements</w:t></w:r></w:p><w:p><w:r><w:t>NFR-1: The service shall respond in at most 200 ms by 13 October 2026.</w:t></w:r></w:p></w:body></w:document>"#,
        )])?;
        let result = extract("a.docx", &bytes)?;
        let requirement = result
            .facts
            .iter()
            .find(|fact| fact.kind == "requirement")
            .expect("requirement");
        assert_eq!(requirement.location, DocumentLocation::Block { number: 3 });
        assert_eq!(requirement.start_line, 3);
        let details = requirement.document.as_ref().expect("details");
        assert_eq!(
            details.heading_path,
            ["Platform specification", "Performance requirements"]
        );
        assert_eq!(details.quantities[0].comparator, "lte");
        assert_eq!(details.dates[0].normalized.as_deref(), Some("2026-10-13"));
        assert!(
            result
                .facts
                .iter()
                .any(|fact| fact.syntax_kind == "docx-styled-heading")
        );
        Ok(())
    }

    #[test]
    fn docx_tables_preserve_headers_empty_cells_and_merge_evidence() -> Result<()> {
        let bytes = archive(&[(
            "word/document.xml",
            r#"<w:document xmlns:w="word"><w:body><w:tbl><w:tr><w:trPr><w:tblHeader/></w:trPr><w:tc><w:p><w:r><w:t>Identifier</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Owner</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>Requirement</w:t></w:r></w:p></w:tc></w:tr><w:tr/><w:tr><w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>REQ-2</w:t></w:r></w:p></w:tc><w:tc/><w:tc><w:p><w:r><w:t>The API must respond in at most 200 ms.</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc><w:tc/><w:tc><w:p><w:r><w:t>Follow up</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#,
        )])?;
        let result = extract("requirements.docx", &bytes)?;
        let fact = result
            .facts
            .iter()
            .find(|f| {
                f.kind == "requirement"
                    && f.location == DocumentLocation::TableRow { table: 1, row: 3 }
            })
            .expect("table requirement");
        let details = fact.document.as_ref().unwrap();
        assert_eq!(details.requirement_id.as_deref(), Some("REQ-2"));
        assert_eq!(details.quantities[0].value, "200");
        let table = details.table.as_ref().unwrap();
        assert_eq!(table.header_basis, "explicit-header-row");
        assert_eq!(table.cells[1].column, 2);
        assert!(table.cells[1].value.is_empty());
        assert_eq!(table.cells[2].header.as_deref(), Some("Requirement"));
        assert_eq!(table.cells[0].vertical_merge.as_deref(), Some("restart"));
        let continuation = result
            .facts
            .iter()
            .find(|f| {
                f.kind == "table_row"
                    && f.location == DocumentLocation::TableRow { table: 1, row: 4 }
            })
            .unwrap();
        let cell = &continuation
            .document
            .as_ref()
            .unwrap()
            .table
            .as_ref()
            .unwrap()
            .cells[0];
        assert!(cell.value.is_empty());
        assert_eq!(cell.vertical_merge.as_deref(), Some("continue"));
        Ok(())
    }

    #[test]
    fn pptx_uses_presentation_order_not_slide_filenames() -> Result<()> {
        let bytes = archive(&[
            (
                "ppt/presentation.xml",
                r#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId r:id="r2"/><p:sldId r:id="r1"/></p:sldIdLst></p:presentation>"#,
            ),
            (
                "ppt/_rels/presentation.xml.rels",
                r#"<Relationships><Relationship Id="r1" Target="slides/slide1.xml"/><Relationship Id="r2" Target="slides/slide2.xml"/></Relationships>"#,
            ),
            (
                "ppt/slides/slide1.xml",
                r#"<p:sld xmlns:p="p" xmlns:a="a"><a:p><a:r><a:t>Second needle feature should be intuitive.</a:t></a:r></a:p></p:sld>"#,
            ),
            (
                "ppt/slides/slide2.xml",
                r#"<p:sld xmlns:p="p" xmlns:a="a"><p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>First needle</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:txBody><a:p><a:r><a:t>The API must respond in at most 200 ms.</a:t></a:r></a:p></p:txBody></p:sp></p:sld>"#,
            ),
        ])?;
        let result = extract("a.pptx", &bytes)?;
        assert!(result.chunks[0].text.starts_with("First"));
        assert_eq!(
            result.chunks[0].location,
            DocumentLocation::Slide { number: 1 }
        );
        assert_eq!(
            result.chunks[1].location,
            DocumentLocation::Slide { number: 2 }
        );
        let title = result
            .facts
            .iter()
            .find(|fact| fact.syntax_kind == "pptx-title-placeholder")
            .expect("title");
        assert_eq!(title.name, "First needle");
        assert_eq!(title.location, DocumentLocation::Slide { number: 1 });
        let requirements = result
            .facts
            .iter()
            .filter(|fact| fact.kind == "requirement")
            .collect::<Vec<_>>();
        assert_eq!(requirements.len(), 2);
        assert_eq!(
            requirements[0]
                .document
                .as_ref()
                .expect("details")
                .heading_path,
            ["First needle"]
        );
        assert!(
            requirements[1]
                .document
                .as_ref()
                .expect("details")
                .heading_path
                .is_empty(),
            "heading context must not leak across slides"
        );
        Ok(())
    }

    #[test]
    fn pdf_text_layer_preserves_page_location() -> Result<()> {
        // Tiny original test PDF; no customer/upstream fixture material.
        let stream = "BT /F1 12 Tf 10 100 Td (Airk needle shall be secure by 2026-10-13.) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_owned(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
            format!("<< /Length {} >>\nstream\n{stream}\nendstream",stream.len()),
        ];
        let mut pdf = "%PDF-1.4\n".to_owned();
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", i + 1));
        }
        let xref = pdf.len();
        pdf.push_str("xref\n0 6\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
        ));
        let result = extract("a.pdf", pdf.as_bytes())?;
        assert!(result.chunks.iter().any(|c| c.text.contains("Airk needle")));
        assert_eq!(
            result.chunks[0].location,
            DocumentLocation::Page { number: 1 }
        );
        let requirement = result
            .facts
            .iter()
            .find(|fact| fact.kind == "requirement")
            .expect("requirement");
        assert_eq!(requirement.location, DocumentLocation::Page { number: 1 });
        assert_eq!(
            requirement.document.as_ref().expect("details").dates[0]
                .normalized
                .as_deref(),
            Some("2026-10-13")
        );
        Ok(())
    }

    #[test]
    fn structured_documents_and_svg_have_native_facts() -> Result<()> {
        let md = extract("a.md", b"# Heading\n\n```python\n# fake heading\n```\n")?;
        assert_eq!(md.facts.len(), 1);
        assert_eq!(md.facts[0].name, "Heading");
        let csv = extract("a.csv", b"name,description\nAlice,\"multi\nline\"\n")?;
        assert!(csv.facts.iter().any(|f| f.name == "description"));
        let svg=extract("a.svg",br#"<svg viewBox="0 0 10 10"><title>Mark</title><path id="logo" d="M0 0 L10 10"/></svg>"#)?;
        assert!(svg.facts.iter().any(|f| f.name == "id=logo"));
        assert!(svg.facts.iter().any(|f| f.name == "viewBox=0 0 10 10"));
        assert!(extract("a.svg", b"<!DOCTYPE svg SYSTEM 'file:///private'><svg/>").is_err());
        Ok(())
    }

    #[test]
    fn xml_attribute_budgets_cover_documents_office_parts_and_lookups() -> Result<()> {
        for count in [MAX_XML_ATTRIBUTES, MAX_XML_ATTRIBUTES + 1] {
            let attrs = (0..count)
                .map(|i| format!(" a{i}=\"v\""))
                .collect::<String>();
            let xml = format!("<root{attrs}/>");
            for path in ["a.xml", "a.svg"] {
                let result = extract(path, xml.as_bytes());
                assert_eq!(
                    result.is_ok(),
                    count == MAX_XML_ATTRIBUTES,
                    "{path}: {:?}",
                    result.err()
                );
            }
            // All Office adapters validate each expanded XML part through read_part.
            for name in [
                "word/document.xml",
                "xl/workbook.xml",
                "ppt/slides/slide1.xml",
            ] {
                for xml in [format!("<root{attrs}/>"), format!("<root{attrs}></root>")] {
                    let bytes = archive(&[(name, &xml)])?;
                    let mut zipped = zip::ZipArchive::new(Cursor::new(bytes.as_slice()))?;
                    let result = read_part(&mut zipped, name);
                    assert_eq!(
                        result.is_ok(),
                        count == MAX_XML_ATTRIBUTES,
                        "{name}: {result:?}"
                    );
                }
            }
            let mut reader = Reader::from_str(&xml);
            let Event::Empty(element) = reader.read_event()? else {
                panic!("expected tag")
            };
            assert_eq!(
                attribute(&element, b"absent").is_ok(),
                count == MAX_XML_ATTRIBUTES
            );
        }
        // Keep duplicate-name rejection: disabling checks would change valid-input semantics.
        let xml = "<root a=\"1\" a=\"2\"/>";
        assert!(extract("a.xml", xml.as_bytes()).is_err());
        let bytes = archive(&[("word/document.xml", xml)])?;
        let mut zipped = zip::ZipArchive::new(Cursor::new(bytes.as_slice()))?;
        assert!(read_part(&mut zipped, "word/document.xml").is_err());
        let mut reader = Reader::from_str(xml);
        let Event::Empty(element) = reader.read_event()? else {
            panic!("expected tag")
        };
        assert!(attribute(&element, b"a").is_err());
        Ok(())
    }

    #[test]
    fn xml_attribute_values_preserve_literal_whitespace_and_unescape_entities() -> Result<()> {
        let xml = "<root id=\"A\tB\nC\rD&amp;&#10;\"/>";
        let expected = "A\tB\nC\rD&\n";
        let mut reader = Reader::from_str(xml);
        let Event::Empty(element) = reader.read_event()? else {
            panic!("expected tag")
        };
        assert_eq!(attribute(&element, b"id")?.as_deref(), Some(expected));
        for path in ["a.xml", "a.svg"] {
            assert!(
                extract(path, xml.as_bytes())?
                    .facts
                    .iter()
                    .any(|f| f.name == format!("id={expected}"))
            );
        }
        Ok(())
    }

    #[test]
    fn malformed_documents_are_not_lossily_indexed() {
        assert!(extract("a.pdf", b"not a pdf").is_err());
        assert!(extract("a.docx", b"not a zip").is_err());
        assert!(extract("a.pptx", b"not a zip").is_err());
        assert!(extract("a.unknown", &[0, 255]).is_err());
    }
}
