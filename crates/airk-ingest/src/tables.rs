// SPDX-License-Identifier: Apache-2.0
//! Table-aware evidence shared by native DOCX and XLSX extraction.
use crate::documents::{Extracted, MAX_EXTRACTED, attribute, entity};
use airk_core::{DocumentEvidence, DocumentLocation, Fact, TableCellEvidence, TableEvidence};
use anyhow::{Context, Result, ensure};
use quick_xml::{Reader, events::Event};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const MAX_HEADER_BYTES: usize = 160;

pub(crate) fn bounded(text: &str, limit: usize) -> String {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].into()
}

/// Append an explicitly normalized row representation; original row/cell coordinates
/// remain in evidence. Cell boundaries cannot accidentally join numbers and units.
pub(crate) fn append_row(
    output: &mut Extracted,
    mut table: TableEvidence,
    location: DocumentLocation,
) -> Result<()> {
    if table
        .cells
        .iter()
        .all(|cell| cell.value.trim().is_empty() && cell.formula.is_none())
    {
        return Ok(());
    }
    let text = table
        .cells
        .iter()
        .filter(|cell| !cell.value.is_empty() || cell.formula.is_some())
        .map(|cell| {
            let label = cell.header.clone().unwrap_or_else(|| {
                cell.address
                    .clone()
                    .unwrap_or_else(|| format!("column {}", cell.column))
            });
            let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
            let value = cell.value.split_whitespace().collect::<Vec<_>>().join(" ");
            let value = cell
                .normalized_value
                .as_ref()
                .map_or(value.clone(), |normalized| {
                    format!("{normalized} [stored: {value}]")
                });
            if let Some(formula) = &cell.formula {
                let formula = formula.split_whitespace().collect::<Vec<_>>().join(" ");
                let result = if cell.cached_value {
                    "cached result, not recalculated"
                } else {
                    "no cached result"
                };
                format!("{label}: {value} [stored formula={formula}; {result}]")
            } else {
                format!("{label}: {value}")
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
        + "\n";
    let prior_bytes: usize = output.chunks.iter().map(|chunk| chunk.text.len()).sum();
    ensure!(
        prior_bytes + text.len() <= MAX_EXTRACTED,
        "table extraction exceeds 4 MiB"
    );
    let start = output.chunks.last().map_or(1, |chunk| chunk.end_line + 1);
    let full_cells = table.cells.clone();
    let mut omitted = table.cells.len() > 32 || table.headers_truncated;
    table.cells.truncate(32);
    for cell in &mut table.cells {
        omitted |= cell.value.len() > 256
            || cell.header.as_ref().is_some_and(|value| value.len() > 160)
            || cell.formula.as_ref().is_some_and(|value| value.len() > 256)
            || cell
                .number_format
                .as_ref()
                .is_some_and(|value| value.len() > 160);
        cell.value = bounded(&cell.value, 256);
        cell.header = cell.header.as_ref().map(|value| bounded(value, 160));
        cell.formula = cell.formula.as_ref().map(|value| bounded(value, 256));
        cell.number_format = cell.number_format.as_ref().map(|value| bounded(value, 160));
    }
    let syntax = if table.sheet.is_some() {
        "xlsx-table-row"
    } else {
        "docx-table-row"
    };
    let details = DocumentEvidence {
        parser_version: "table-cues/1".into(),
        basis: "written-table-structure".into(),
        table: Some(table.clone()),
        details_truncated: omitted,
        statement_truncated: text.len() > 1024,
        ..Default::default()
    };
    output.facts.push(Fact {
        name: bounded(text.trim_end(), 1024),
        kind: "table_row".into(),
        syntax_kind: syntax.into(),
        start_line: start,
        end_line: start,
        location: location.clone(),
        document: Some(details),
        code: None,
    });
    for mut cell in full_cells {
        if cell.value.is_empty() && cell.formula.is_none() {
            continue;
        }
        ensure!(output.facts.len() < 10_000, "document exceeds 10000 facts");
        let label = cell
            .header
            .as_deref()
            .or(cell.address.as_deref())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("column {}", cell.column));
        // Keep A1 addresses searchable even when a header supplies the label.
        // The original address also remains structured evidence, never a source path.
        let label = match (&cell.address, &cell.header) {
            (Some(address), Some(_)) => format!("{address} {label}"),
            _ => label,
        };
        let name = format!("{label}: {}", cell.value);
        let truncated = cell.value.len() > 256
            || cell.header.as_ref().is_some_and(|v| v.len() > 160)
            || cell.formula.as_ref().is_some_and(|v| v.len() > 256)
            || cell.number_format.as_ref().is_some_and(|v| v.len() > 160);
        cell.value = bounded(&cell.value, 256);
        cell.header = cell.header.as_ref().map(|v| bounded(v, 160));
        cell.formula = cell.formula.as_ref().map(|v| bounded(v, 256));
        cell.number_format = cell.number_format.as_ref().map(|v| bounded(v, 160));
        let mut cell_table = table.clone();
        cell_table.cells = vec![cell];
        output.facts.push(Fact {
            name: bounded(&name, 1024),
            kind: "table_cell".into(),
            syntax_kind: syntax.into(),
            start_line: start,
            end_line: start,
            location: location.clone(),
            code: None,
            document: Some(DocumentEvidence {
                parser_version: "table-cues/1".into(),
                basis: "written-table-structure".into(),
                table: Some(cell_table),
                details_truncated: truncated || table.headers_truncated,
                statement_truncated: name.len() > 1024,
                ..Default::default()
            }),
        });
    }
    // Reuse shared requirement/date/quantity rules across the whole row, preserving its
    // header mapping. Structural context does not turn a numeric cell into a requirement.
    let context = table
        .sheet
        .as_ref()
        .map(|sheet| vec![(1, sheet.clone())])
        .unwrap_or_default();
    let facts = if table.is_header {
        Vec::new()
    } else {
        crate::prose::plain_with_context(&text, &context, None)?.0
    };
    for mut fact in facts {
        fact.start_line = start;
        fact.end_line = start;
        fact.location = location.clone();
        fact.syntax_kind = syntax.into();
        if let Some(details) = &mut fact.document {
            details.table = Some(table.clone());
            details.details_truncated |= omitted;
        }
        output.facts.push(fact);
    }
    ensure!(output.facts.len() <= 10_000, "document exceeds 10000 facts");
    output.chunks.push(airk_core::Chunk {
        text,
        start_line: start,
        end_line: start,
        location,
    });
    Ok(())
}

#[derive(Default)]
struct Row {
    ordinal: usize,
    header: bool,
    next_column: usize,
    cells: Vec<TableCellEvidence>,
}
#[derive(Default)]
struct Table {
    ordinal: usize,
    rows: Vec<Row>,
    row: Option<Row>,
    cell: Option<TableCellEvidence>,
}

pub(crate) fn append_docx_tables(xml: &str, output: &mut Extracted) -> Result<()> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().expand_empty_elements = true;
    let mut stack: Vec<Table> = Vec::new();
    let mut tables = Vec::new();
    let mut ordinal = 0;
    let mut total_rows = 0;
    let mut in_text = false;
    loop {
        match reader.read_event()? {
            Event::Start(e) if e.local_name().as_ref() == b"tbl" => {
                ordinal += 1;
                ensure!(
                    ordinal <= 256 && stack.len() < 32,
                    "DOCX table/depth limit exceeded"
                );
                stack.push(Table {
                    ordinal,
                    ..Default::default()
                });
            }
            Event::Start(e) if e.local_name().as_ref() == b"tr" => {
                let table = stack.last_mut().context("DOCX row outside table")?;
                ensure!(table.row.is_none(), "nested DOCX row without table");
                table.row = Some(Row {
                    ordinal: table.rows.len() + 1,
                    next_column: 1,
                    ..Default::default()
                });
            }
            Event::Start(e) if e.local_name().as_ref() == b"tc" => {
                let table = stack.last_mut().context("DOCX cell outside table")?;
                let row = table.row.as_ref().context("DOCX cell outside row")?;
                ensure!(table.cell.is_none(), "nested DOCX cell without table");
                table.cell = Some(TableCellEvidence {
                    column: row.next_column,
                    column_span: 1,
                    value_type: "text".into(),
                    ..Default::default()
                });
            }
            Event::Empty(e) | Event::Start(e)
                if matches!(
                    e.local_name().as_ref(),
                    b"gridBefore" | b"gridSpan" | b"vMerge" | b"tblHeader"
                ) =>
            {
                if let Some(table) = stack.last_mut() {
                    let value = attribute(&e, b"w:val")?;
                    match e.local_name().as_ref() {
                        b"tblHeader" => {
                            if let Some(row) = &mut table.row {
                                row.header =
                                    !matches!(value.as_deref(), Some("0" | "false" | "off"));
                            }
                        }
                        b"gridBefore" => {
                            if let Some(row) = &mut table.row {
                                ensure!(
                                    row.cells.is_empty() && table.cell.is_none(),
                                    "DOCX grid offset after cells"
                                );
                                let before: usize =
                                    value.context("missing grid offset")?.parse()?;
                                ensure!(before <= 1024, "DOCX grid offset too large");
                                row.next_column = before + 1;
                            }
                        }
                        b"gridSpan" => {
                            if let Some(cell) = &mut table.cell {
                                cell.column_span = value.context("missing grid span")?.parse()?;
                                ensure!(
                                    (1..=1024).contains(&cell.column_span),
                                    "invalid DOCX grid span"
                                );
                            }
                        }
                        b"vMerge" => {
                            if let Some(cell) = &mut table.cell {
                                let value = value.unwrap_or_else(|| "continue".into());
                                ensure!(
                                    matches!(value.as_str(), "restart" | "continue"),
                                    "invalid DOCX vertical merge"
                                );
                                cell.vertical_merge = Some(value);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::Start(e) if e.local_name().as_ref() == b"t" => in_text = true,
            Event::End(e) if e.local_name().as_ref() == b"t" => in_text = false,
            Event::Text(e) if in_text => {
                if let Some(cell) = stack.last_mut().and_then(|table| table.cell.as_mut()) {
                    cell.value.push_str(&e.decode()?);
                }
            }
            Event::CData(e) if in_text => {
                if let Some(cell) = stack.last_mut().and_then(|table| table.cell.as_mut()) {
                    cell.value.push_str(&e.decode()?);
                }
            }
            Event::GeneralRef(e) if in_text => {
                if let Some(cell) = stack.last_mut().and_then(|table| table.cell.as_mut()) {
                    cell.value.push_str(&entity(&e.decode()?)?);
                }
            }
            Event::End(e) if e.local_name().as_ref() == b"p" => {
                if let Some(cell) = stack.last_mut().and_then(|table| table.cell.as_mut()) {
                    cell.value.push('\n');
                }
            }
            Event::Start(e) | Event::Empty(e)
                if matches!(e.local_name().as_ref(), b"br" | b"tab") =>
            {
                if let Some(cell) = stack.last_mut().and_then(|table| table.cell.as_mut()) {
                    cell.value.push(' ');
                }
            }
            Event::End(e) if e.local_name().as_ref() == b"tc" => {
                let table = stack.last_mut().context("DOCX cell outside table")?;
                let mut cell = table.cell.take().context("DOCX cell end without start")?;
                cell.value = cell.value.trim().into();
                let row = table.row.as_mut().context("DOCX cell outside row")?;
                row.next_column += cell.column_span;
                ensure!(
                    row.next_column <= 16_385 && row.cells.len() < 256,
                    "DOCX column limit exceeded"
                );
                row.cells.push(cell);
            }
            Event::End(e) if e.local_name().as_ref() == b"tr" => {
                total_rows += 1;
                ensure!(total_rows <= 10_000, "DOCX row limit exceeded");
                let table = stack.last_mut().context("DOCX row outside table")?;
                table
                    .rows
                    .push(table.row.take().context("DOCX row end without start")?);
            }
            Event::End(e) if e.local_name().as_ref() == b"tbl" => {
                tables.push(stack.pop().context("DOCX table end without start")?)
            }
            Event::DocType(_) => anyhow::bail!("DOCX document type declarations are unsupported"),
            Event::Eof => break,
            _ => {}
        }
    }
    ensure!(stack.is_empty(), "unterminated DOCX table");
    tables.sort_by_key(|table| table.ordinal);
    for table in tables {
        let first = table
            .rows
            .iter()
            .find(|row| row.cells.iter().any(|cell| !cell.value.trim().is_empty()));
        let explicit = table.rows.iter().any(|row| row.header);
        let mut headers = BTreeMap::new();
        let mut headers_truncated = false;
        for row in table.rows.iter().filter(|row| {
            row.header || (!explicit && first.is_some_and(|first| first.ordinal == row.ordinal))
        }) {
            for cell in &row.cells {
                if cell.value.is_empty() || cell.vertical_merge.as_deref() == Some("continue") {
                    continue;
                }
                for column in cell.column..cell.column + cell.column_span {
                    let header = headers.entry(column).or_insert_with(String::new);
                    if !header.is_empty() {
                        let separator = bounded(" / ", MAX_HEADER_BYTES - header.len());
                        headers_truncated |= separator.len() < 3;
                        header.push_str(&separator);
                    }
                    let fragment = bounded(&cell.value, MAX_HEADER_BYTES - header.len());
                    headers_truncated |= fragment.len() < cell.value.len();
                    header.push_str(&fragment);
                }
            }
        }
        for mut row in table.rows {
            for cell in &mut row.cells {
                cell.header = headers.get(&cell.column).cloned();
            }
            append_row(
                output,
                TableEvidence {
                    table: Some(table.ordinal),
                    row: row.ordinal,
                    header_basis: if explicit {
                        "explicit-header-row"
                    } else {
                        "first-row-candidate"
                    }
                    .into(),
                    cells: row.cells,
                    is_header: row.header,
                    headers_truncated,
                    ..Default::default()
                },
                DocumentLocation::TableRow {
                    table: table.ordinal,
                    row: row.ordinal,
                },
            )?;
        }
    }
    Ok(())
}

/// Table paragraphs keep their original searchable blocks, but their prose cues
/// belong to structured rows so headers cannot change later body heading context.
pub(crate) fn docx_table_paragraphs(xml: &str) -> Result<BTreeSet<usize>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().expand_empty_elements = true;
    let mut depth = 0usize;
    let mut ordinal = 0;
    let mut paragraphs = BTreeSet::new();
    loop {
        match reader.read_event()? {
            Event::Start(element) if element.local_name().as_ref() == b"tbl" => {
                depth += 1;
            }
            Event::End(element) if element.local_name().as_ref() == b"tbl" => {
                depth = depth
                    .checked_sub(1)
                    .context("DOCX table end without start")?;
            }
            Event::Start(element) if element.local_name().as_ref() == b"p" => {
                ordinal += 1;
                if depth > 0 {
                    paragraphs.insert(ordinal);
                }
            }
            Event::Eof => break,
            _ => (),
        }
    }
    Ok(paragraphs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    fn package(xml: &str) -> Result<Vec<u8>> {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )?;
        archive.write_all(xml.as_bytes())?;
        Ok(archive.finish()?.into_inner())
    }

    fn empty_docx() -> Extracted {
        Extracted {
            chunks: Vec::new(),
            facts: Vec::new(),
            semantic_seeds: None,
            language: "docx".into(),
            method: "fixture".into(),
            parse_status: "fixture".into(),
        }
    }

    #[test]
    fn tabs_and_breaks_separate_table_words_and_units() -> Result<()> {
        let mut output = empty_docx();
        append_docx_tables(
            r#"<w:document xmlns:w="word"><w:tbl><w:tr><w:tc><w:p><w:r><w:t>The service must</w:t><w:tab/><w:t><![CDATA[not exceed]]></w:t><w:br/><w:t>200</w:t><w:tab/><w:t>ms.</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:document>"#,
            &mut output,
        )?;
        assert!(output.chunks[0].text.contains("must not exceed 200 ms."));
        assert!(
            output
                .facts
                .iter()
                .filter_map(|fact| fact.document.as_ref())
                .flat_map(|evidence| &evidence.quantities)
                .any(|quantity| quantity.value == "200" && quantity.unit == "ms")
        );
        Ok(())
    }

    #[test]
    fn propagated_headers_are_bounded_before_spanning_many_columns() -> Result<()> {
        let mut output = empty_docx();
        let header = "original α header ".repeat(1000);
        let xml = format!(
            r#"<w:document xmlns:w="word"><w:tbl><w:tr><w:trPr><w:tblHeader/></w:trPr><w:tc><w:tcPr><w:gridSpan w:val="1024"/></w:tcPr><w:p><w:r><w:t>{header}</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>value</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:document>"#
        );
        append_docx_tables(&xml, &mut output)?;
        let details = output
            .facts
            .iter()
            .find(|fact| {
                fact.kind == "table_row"
                    && matches!(fact.location, DocumentLocation::TableRow { row: 2, .. })
            })
            .unwrap()
            .document
            .as_ref()
            .unwrap();
        let table = details.table.as_ref().unwrap();
        assert!(table.headers_truncated);
        assert!(details.details_truncated);
        assert!(table.cells[0].header.as_ref().unwrap().len() <= MAX_HEADER_BYTES);
        assert!(output.chunks[0].text.contains(header.trim_end()));
        Ok(())
    }

    #[test]
    fn explicit_table_headers_do_not_classify_or_change_body_paragraph_context() -> Result<()> {
        let xml = r#"<w:document xmlns:w="word"><w:body><w:p/><w:tbl><w:tr><w:trPr><w:tblHeader/></w:trPr><w:tc><w:p><w:r><w:t>Requirements</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>The service must finish in 20 ms.</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>Ordinary background information.</w:t></w:r></w:p></w:body></w:document>"#;
        let result = crate::documents::extract("fixture.docx", &package(xml)?)?;
        let requirements: Vec<_> = result
            .facts
            .iter()
            .filter(|fact| fact.kind == "requirement")
            .collect();
        assert_eq!(requirements.len(), 1);
        assert_eq!(
            requirements[0].location,
            DocumentLocation::TableRow { table: 1, row: 2 }
        );
        let header = result
            .facts
            .iter()
            .find(|fact| {
                fact.kind == "table_row"
                    && matches!(fact.location, DocumentLocation::TableRow { row: 1, .. })
            })
            .unwrap();
        assert!(
            header
                .document
                .as_ref()
                .unwrap()
                .table
                .as_ref()
                .unwrap()
                .is_header
        );
        for (number, expected) in [
            (2, "Requirements"),
            (3, "The service must finish"),
            (4, "Ordinary background"),
        ] {
            assert!(result.chunks.iter().any(|chunk| chunk.location
                == (DocumentLocation::Block { number })
                && chunk.text.contains(expected)));
        }
        assert!(crate::documents::extract("fixture.docx", &package(r#"<w:comments xmlns:w="word"><w:p><w:r><w:t>wrong package part</w:t></w:r></w:p></w:comments>"#)?).is_err());
        Ok(())
    }

    #[test]
    fn docx_cdata_reaches_paragraph_and_table_views_and_malformed_xml_fails() -> Result<()> {
        let xml = r#"<w:document xmlns:w="word"><w:body><w:tbl><w:tr><w:tc><w:p><w:r><w:t><![CDATA[original & literal <value>]]></w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#;
        let result = crate::documents::extract("fixture.docx", &package(xml)?)?;
        assert!(result.chunks.iter().any(|chunk| matches!(
            chunk.location,
            DocumentLocation::Block { .. }
        )
            && chunk.text.contains("original & literal <value>")));
        assert!(result.chunks.iter().any(|chunk| matches!(
            chunk.location,
            DocumentLocation::TableRow { .. }
        )
            && chunk.text.contains("original & literal <value>")));
        for invalid in [
            r#"<![CDATA[bad]]><w:document xmlns:w="word"/>"#,
            r#"&amp;<w:document xmlns:w="word"/>"#,
            r#"<w:document xmlns:w="word"><w:tbl><w:tr><w:tc><w:tcPr><w:gridSpan w:val="1" w:val="2"/></w:tcPr><w:p><w:r><w:t>fixture</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:document>"#,
        ] {
            assert!(crate::documents::extract("fixture.docx", &package(invalid)?).is_err());
        }
        Ok(())
    }
}
