// SPDX-License-Identifier: Apache-2.0
//! Bounded SpreadsheetML extraction. No Excel, formula execution or external reads.
use crate::{
    documents::{Extracted, MAX_EXTRACTED, attribute, entity, read_part, require_root},
    tables::{MAX_HEADER_BYTES, append_row, bounded},
};
use airk_core::{DocumentLocation, TableCellEvidence, TableEvidence};
use anyhow::{Context, Result, ensure};
use quick_xml::{Reader, events::Event};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
};

const MAX_XML: usize = 16 * 1024 * 1024;
const MAX_CELLS: usize = 100_000;

struct Relationship {
    target: String,
    kind: String,
}

fn relationships(xml: &str) -> Result<BTreeMap<String, Relationship>> {
    require_root(xml, b"Relationships")?;
    let mut reader = Reader::from_str(xml);
    let mut values = BTreeMap::new();
    let mut ids = BTreeSet::new();
    loop {
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == b"Relationship" => {
                let id = attribute(&e, b"Id")?.context("missing XLSX relationship ID")?;
                ensure!(
                    !id.is_empty() && ids.insert(id.clone()),
                    "invalid/duplicate XLSX relationship ID"
                );
                let mode = attribute(&e, b"TargetMode")?;
                ensure!(
                    matches!(mode.as_deref(), None | Some("Internal" | "External")),
                    "invalid XLSX relationship target mode"
                );
                if mode.as_deref() == Some("External") {
                    continue;
                }
                let target =
                    attribute(&e, b"Target")?.context("missing XLSX relationship target")?;
                let kind = attribute(&e, b"Type")?.context("missing XLSX relationship type")?;
                ensure!(
                    values.insert(id, Relationship { target, kind }).is_none(),
                    "duplicate XLSX relationship"
                );
            }
            Event::DocType(_) => anyhow::bail!("XLSX DTDs are unsupported"),
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(values)
}

fn part_path(base: &str, target: &str) -> Result<String> {
    ensure!(
        !target.is_empty() && !target.contains(['\\', ':', '\0', '?', '#', '%']),
        "unsupported XLSX relationship path"
    );
    let mut parts: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        base.rsplit_once('/')
            .map_or(Vec::new(), |(parent, _)| parent.split('/').collect())
    };
    for part in target.trim_start_matches('/').split('/') {
        match part {
            "" | "." => {}
            ".." => {
                ensure!(parts.pop().is_some(), "XLSX relationship escapes package");
            }
            value => parts.push(value),
        }
    }
    ensure!(!parts.is_empty(), "empty XLSX part path");
    Ok(parts.join("/"))
}

fn read_xml(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    path: &str,
    total: &mut usize,
) -> Result<String> {
    let text = read_part(archive, path)?;
    *total += text.len();
    ensure!(*total <= MAX_XML, "XLSX parts exceed 16 MiB XML budget");
    Ok(text)
}

pub(crate) fn extract(bytes: &[u8]) -> Result<Extracted> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("invalid XLSX archive")?;
    ensure!(
        archive.len() <= 10_000,
        "XLSX archive exceeds 10000 entries"
    );
    ensure!(
        !archive.has_duplicate_names(),
        "duplicate XLSX archive parts"
    );
    let names: BTreeSet<_> = archive
        .file_names()
        .map(|name| name.map(|name| name.into_owned()))
        .collect::<zip::result::ZipResult<_>>()
        .context("invalid XLSX archive part name")?;
    ensure!(names.len() == archive.len(), "duplicate XLSX archive parts");
    let mut total = 0;
    let workbook_path = if names.contains("_rels/.rels") {
        let root = relationships(&read_xml(&mut archive, "_rels/.rels", &mut total)?)?;
        let offices: Vec<_> = root
            .values()
            .filter(|rel| rel.kind.ends_with("/officeDocument"))
            .collect();
        ensure!(
            offices.len() == 1,
            "XLSX requires one internal workbook relationship"
        );
        part_path("", &offices[0].target)?
    } else {
        "xl/workbook.xml".into()
    };
    let workbook = read_xml(&mut archive, &workbook_path, &mut total)?;
    require_root(&workbook, b"workbook")?;
    let (parent, filename) = workbook_path
        .rsplit_once('/')
        .unwrap_or(("", &workbook_path));
    let rel_path = if parent.is_empty() {
        format!("_rels/{filename}.rels")
    } else {
        format!("{parent}/_rels/{filename}.rels")
    };
    let rels = relationships(&read_xml(&mut archive, &rel_path, &mut total)?)?;
    let mut reader = Reader::from_str(&workbook);
    let mut sheets = Vec::new();
    let mut sheet_names = BTreeSet::new();
    let mut date1904 = false;
    loop {
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == b"workbookPr" => {
                date1904 = matches!(attribute(&e, b"date1904")?.as_deref(), Some("1" | "true"))
            }
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == b"sheet" => {
                let name = attribute(&e, b"name")?.context("XLSX sheet name missing")?;
                ensure!(
                    !name.is_empty() && name.len() <= 256 && sheet_names.insert(name.clone()),
                    "invalid/duplicate XLSX sheet name"
                );
                let id = attribute(&e, b"r:id")?.context("XLSX sheet relationship missing")?;
                let rel = rels
                    .get(&id)
                    .context("XLSX sheet relationship unresolved/external")?;
                ensure!(
                    rel.kind.ends_with("/worksheet"),
                    "XLSX non-worksheet sheet is unsupported"
                );
                let hidden = matches!(
                    attribute(&e, b"state")?.as_deref(),
                    Some("hidden" | "veryHidden")
                );
                sheets.push((name, part_path(&workbook_path, &rel.target)?, hidden));
                ensure!(sheets.len() <= 128, "XLSX exceeds 128 sheets");
            }
            Event::DocType(_) => anyhow::bail!("XLSX DTDs are unsupported"),
            Event::Eof => break,
            _ => {}
        }
    }
    ensure!(!sheets.is_empty(), "XLSX workbook has no worksheets");
    let mut shared = Vec::new();
    let mut formats = Vec::new();
    for (suffix, destination) in [("/sharedStrings", 0), ("/styles", 1)] {
        let matches: Vec<_> = rels
            .values()
            .filter(|rel| rel.kind.ends_with(suffix))
            .collect();
        ensure!(matches.len() <= 1, "duplicate XLSX global part");
        if let Some(rel) = matches.first() {
            let xml = read_xml(
                &mut archive,
                &part_path(&workbook_path, &rel.target)?,
                &mut total,
            )?;
            if destination == 0 {
                shared = shared_strings(&xml)?;
            } else {
                formats = styles(&xml)?;
            }
        }
    }
    let mut output = Extracted {
        chunks: Vec::new(),
        facts: Vec::new(),
        semantic_seeds: None,
        language: "xlsx".into(),
        method: "xlsx-worksheet-rows".into(),
        parse_status: "extracted-spreadsheet; formulas-not-evaluated".into(),
    };
    let mut count = 0;
    let mut decoded_bytes = 0usize;
    for (sheet, filename, hidden) in sheets {
        let xml = read_xml(&mut archive, &filename, &mut total)?;
        let mut rows = worksheet(
            &xml,
            &shared,
            &formats,
            date1904,
            &mut count,
            &mut decoded_bytes,
        )?;
        let header_cells = rows
            .iter()
            .find(|(_, (_, cells))| {
                cells
                    .iter()
                    .any(|cell| cell.formula.is_none() && !cell.value.trim().is_empty())
            })
            .map(|(_, (_, cells))| cells);
        let headers_truncated = header_cells.is_some_and(|cells| {
            cells
                .iter()
                .any(|cell| cell.formula.is_none() && cell.value.len() > MAX_HEADER_BYTES)
        });
        let headers: BTreeMap<_, _> = header_cells
            .map(|cells| {
                cells
                    .iter()
                    .filter(|cell| cell.formula.is_none() && !cell.value.trim().is_empty())
                    .map(|cell| (cell.column, bounded(&cell.value, MAX_HEADER_BYTES)))
                    .collect()
            })
            .unwrap_or_default();
        for (row, (row_hidden, cells)) in &mut rows {
            for cell in cells.iter_mut() {
                cell.header = headers.get(&cell.column).cloned();
            }
            append_row(
                &mut output,
                TableEvidence {
                    sheet: Some(sheet.clone()),
                    row: *row,
                    header_basis: "first-nonempty-row-candidate".into(),
                    hidden: hidden || *row_hidden,
                    cells: std::mem::take(cells),
                    headers_truncated,
                    ..Default::default()
                },
                DocumentLocation::WorksheetRow {
                    sheet: sheet.clone(),
                    row: *row,
                },
            )?;
        }
    }
    ensure!(
        !output.chunks.is_empty(),
        "XLSX contains no stored cell evidence"
    );
    Ok(output)
}

fn shared_strings(xml: &str) -> Result<Vec<String>> {
    require_root(xml, b"sst")?;
    let mut reader = Reader::from_str(xml);
    let mut strings = Vec::new();
    let mut value = None::<String>;
    let mut text = false;
    let mut phonetic = false;
    loop {
        match reader.read_event()? {
            Event::Start(e) if e.local_name().as_ref() == b"si" => {
                ensure!(value.is_none(), "nested shared string");
                value = Some(String::new());
            }
            Event::Empty(e) if e.local_name().as_ref() == b"si" => strings.push(String::new()),
            Event::Start(e) if e.local_name().as_ref() == b"rPh" => phonetic = true,
            Event::End(e) if e.local_name().as_ref() == b"rPh" => phonetic = false,
            Event::Start(e) if e.local_name().as_ref() == b"t" => text = !phonetic,
            Event::End(e) if e.local_name().as_ref() == b"t" => text = false,
            Event::Text(e) if text => value
                .as_mut()
                .context("shared text outside string")?
                .push_str(&e.decode()?),
            Event::CData(e) if text => value
                .as_mut()
                .context("shared text outside string")?
                .push_str(&e.decode()?),
            Event::GeneralRef(e) if text => value
                .as_mut()
                .context("shared entity outside string")?
                .push_str(&entity(&e.decode()?)?),
            Event::End(e) if e.local_name().as_ref() == b"si" => {
                strings.push(value.take().context("shared string end without start")?)
            }
            Event::DocType(_) => anyhow::bail!("XLSX DTDs are unsupported"),
            Event::Eof => break,
            _ => {}
        }
        ensure!(
            strings.len() <= MAX_CELLS,
            "XLSX shared-string limit exceeded"
        );
    }
    ensure!(value.is_none(), "unterminated shared string");
    Ok(strings)
}

fn styles(xml: &str) -> Result<Vec<(u32, Option<String>)>> {
    require_root(xml, b"styleSheet")?;
    let mut reader = Reader::from_str(xml);
    let mut custom = BTreeMap::new();
    let mut ids = Vec::new();
    let mut cell_xfs = false;
    loop {
        match reader.read_event()? {
            Event::Start(e) if e.local_name().as_ref() == b"cellXfs" => cell_xfs = true,
            Event::End(e) if e.local_name().as_ref() == b"cellXfs" => cell_xfs = false,
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == b"numFmt" => {
                let id: u32 = attribute(&e, b"numFmtId")?
                    .context("number format ID missing")?
                    .parse()?;
                let code = attribute(&e, b"formatCode")?.context("number format code missing")?;
                ensure!(
                    code.len() <= 1024 && custom.insert(id, code).is_none(),
                    "invalid/duplicate number format"
                );
            }
            Event::Start(e) | Event::Empty(e) if cell_xfs && e.local_name().as_ref() == b"xf" => {
                ids.push(
                    attribute(&e, b"numFmtId")?
                        .unwrap_or_else(|| "0".into())
                        .parse::<u32>()?,
                );
                ensure!(ids.len() <= 10_000, "XLSX style count exceeded");
            }
            Event::DocType(_) => anyhow::bail!("XLSX DTDs are unsupported"),
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(ids
        .into_iter()
        .map(|id| (id, custom.get(&id).cloned()))
        .collect())
}

fn address(value: &str) -> Result<(usize, usize)> {
    let split = value.bytes().take_while(u8::is_ascii_uppercase).count();
    ensure!((1..=3).contains(&split), "invalid XLSX cell address");
    let mut column = 0usize;
    for byte in value[..split].bytes() {
        column = column * 26 + usize::from(byte - b'A' + 1);
    }
    ensure!(
        !value[split..].starts_with('0') && value[split..].bytes().all(|b| b.is_ascii_digit()),
        "invalid XLSX row address"
    );
    let row: usize = value[split..].parse().context("invalid XLSX row address")?;
    ensure!(
        (1..=16_384).contains(&column) && (1..=1_048_576).contains(&row),
        "XLSX cell outside worksheet bounds"
    );
    Ok((row, column))
}

fn column_name(mut column: usize) -> String {
    let mut bytes = Vec::new();
    while column > 0 {
        column -= 1;
        bytes.push(b'A' + (column % 26) as u8);
        column /= 26;
    }
    bytes.reverse();
    String::from_utf8(bytes).expect("ASCII column")
}

#[derive(Default)]
struct Cell {
    address: String,
    kind: String,
    style: Option<usize>,
    value: String,
    value_seen: bool,
    inline: String,
    inline_seen: bool,
    formula: Option<String>,
}
type Rows = BTreeMap<usize, (bool, Vec<TableCellEvidence>)>;

fn worksheet(
    xml: &str,
    shared: &[String],
    formats: &[(u32, Option<String>)],
    date1904: bool,
    count: &mut usize,
    decoded_bytes: &mut usize,
) -> Result<Rows> {
    require_root(xml, b"worksheet")?;
    let mut reader = Reader::from_str(xml);
    reader.config_mut().expand_empty_elements = true;
    let mut rows = Rows::new();
    let mut row = 0usize;
    let mut in_row = false;
    let mut column = 0;
    let mut cell = None::<Cell>;
    let mut text_kind = 0;
    let mut phonetic = false;
    let mut merges = Vec::new();
    let mut path: Vec<Vec<u8>> = Vec::new();
    let mut sheet_data_seen = false;
    loop {
        let event = reader.read_event()?;
        let opening = match &event {
            Event::Start(element) => Some(element.local_name().as_ref().to_vec()),
            _ => None,
        };
        let closing = matches!(&event, Event::End(_));
        let parent = path.last().map(Vec::as_slice);
        match event {
            Event::Start(e) if e.local_name().as_ref() == b"sheetData" => {
                ensure!(
                    parent == Some(b"worksheet".as_slice()) && !sheet_data_seen,
                    "invalid/duplicate XLSX sheetData"
                );
                sheet_data_seen = true;
            }
            Event::Start(e) if e.local_name().as_ref() == b"row" => {
                ensure!(
                    !in_row && parent == Some(b"sheetData".as_slice()),
                    "XLSX row outside sheetData or nested row"
                );
                row = attribute(&e, b"r")?
                    .map(|n| n.parse())
                    .transpose()?
                    .unwrap_or(row + 1);
                ensure!((1..=1_048_576).contains(&row), "invalid XLSX row");
                let hidden = matches!(attribute(&e, b"hidden")?.as_deref(), Some("1" | "true"));
                ensure!(
                    rows.insert(row, (hidden, Vec::new())).is_none(),
                    "duplicate XLSX row"
                );
                ensure!(rows.len() <= 10_000, "XLSX row limit exceeded");
                in_row = true;
                column = 0;
            }
            Event::End(e) if e.local_name().as_ref() == b"row" => {
                ensure!(cell.is_none(), "unterminated XLSX cell");
                in_row = false;
            }
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == b"mergeCell" => {
                ensure!(
                    parent == Some(b"mergeCells".as_slice()),
                    "XLSX merged range outside mergeCells"
                );
                let value = attribute(&e, b"ref")?.context("missing merged range")?;
                let (start, end) = value.split_once(':').context("invalid merged range")?;
                let (r1, c1) = address(start)?;
                let (r2, c2) = address(end)?;
                ensure!(
                    r1 <= r2 && c1 <= c2 && merges.len() < 10_000,
                    "invalid/excess XLSX merged ranges"
                );
                merges.push((r1, c1, r2, c2, value));
            }
            Event::Start(e) if e.local_name().as_ref() == b"c" => {
                ensure!(
                    in_row && cell.is_none() && parent == Some(b"row".as_slice()),
                    "XLSX cell outside row/nested cell"
                );
                let cell_address = attribute(&e, b"r")?
                    .unwrap_or_else(|| format!("{}{row}", column_name(column + 1)));
                let (cell_row, cell_column) = address(&cell_address)?;
                ensure!(
                    cell_row == row && cell_column > column,
                    "XLSX cell address differs from row or is duplicated/unordered"
                );
                column = cell_column;
                *count += 1;
                ensure!(*count <= MAX_CELLS, "XLSX cell limit exceeded");
                cell = Some(Cell {
                    address: cell_address,
                    kind: attribute(&e, b"t")?.unwrap_or_else(|| "n".into()),
                    style: attribute(&e, b"s")?.map(|n| n.parse()).transpose()?,
                    ..Default::default()
                });
            }
            Event::Empty(e) if e.local_name().as_ref() == b"c" => {
                ensure!(in_row && cell.is_none(), "empty XLSX cell outside row");
                let cell_address = attribute(&e, b"r")?
                    .unwrap_or_else(|| format!("{}{row}", column_name(column + 1)));
                let (cell_row, cell_column) = address(&cell_address)?;
                ensure!(
                    cell_row == row && cell_column > column,
                    "invalid empty XLSX cell address"
                );
                column = cell_column;
                *count += 1;
                ensure!(*count <= MAX_CELLS, "XLSX cell limit exceeded");
            }
            Event::Start(e) if e.local_name().as_ref() == b"rPh" => phonetic = true,
            Event::End(e) if e.local_name().as_ref() == b"rPh" => phonetic = false,
            Event::Start(e) if e.local_name().as_ref() == b"is" => {
                let cell = cell.as_mut().context("inline string outside XLSX cell")?;
                ensure!(
                    parent == Some(b"c".as_slice())
                        && cell.kind == "inlineStr"
                        && !cell.inline_seen
                        && !cell.value_seen,
                    "invalid/duplicate XLSX inline string"
                );
                cell.inline_seen = true;
            }
            Event::Start(e) if e.local_name().as_ref() == b"v" => {
                let cell = cell.as_mut().context("value outside XLSX cell")?;
                ensure!(
                    parent == Some(b"c".as_slice())
                        && !cell.value_seen
                        && !cell.inline_seen
                        && text_kind == 0,
                    "invalid/duplicate XLSX cell value"
                );
                cell.value_seen = true;
                text_kind = 1;
            }
            Event::Start(e) if e.local_name().as_ref() == b"t" && !phonetic => {
                ensure!(
                    cell.as_ref().is_some_and(|cell| cell.inline_seen) && text_kind == 0,
                    "XLSX text outside inline string or nested text"
                );
                text_kind = 2;
            }
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == b"f" => {
                let cell = cell.as_mut().context("formula outside XLSX cell")?;
                ensure!(
                    parent == Some(b"c".as_slice())
                        && cell.formula.is_none()
                        && cell.kind != "inlineStr"
                        && text_kind == 0,
                    "invalid/duplicate XLSX formula"
                );
                cell.formula = Some(String::new());
                text_kind = 3;
            }
            Event::End(e) if matches!(e.local_name().as_ref(), b"v" | b"t" | b"f") => text_kind = 0,
            Event::Text(e) if text_kind > 0 => push_text(
                cell.as_mut().context("text outside XLSX cell")?,
                text_kind,
                &e.decode()?,
            ),
            Event::CData(e) if text_kind > 0 => push_text(
                cell.as_mut().context("text outside XLSX cell")?,
                text_kind,
                &e.decode()?,
            ),
            Event::GeneralRef(e) if text_kind > 0 => push_text(
                cell.as_mut().context("entity outside XLSX cell")?,
                text_kind,
                &entity(&e.decode()?)?,
            ),
            Event::End(e) if e.local_name().as_ref() == b"c" => {
                let cell = cell.take().context("XLSX cell end without start")?;
                let decoded = decode_cell(cell, shared, formats, date1904)?;
                *decoded_bytes = decoded_bytes
                    .saturating_add(decoded.value.len())
                    .saturating_add(decoded.formula.as_ref().map_or(0, String::len))
                    .saturating_add(decoded.number_format.as_ref().map_or(0, String::len))
                    .saturating_add(decoded.address.as_ref().map_or(0, String::len));
                ensure!(
                    *decoded_bytes <= MAX_EXTRACTED,
                    "XLSX decoded cell text exceeds 32 MiB"
                );
                rows.get_mut(&row)
                    .context("XLSX row missing")?
                    .1
                    .push(decoded);
                text_kind = 0;
            }
            Event::DocType(_) => anyhow::bail!("XLSX DTDs are unsupported"),
            Event::Eof => break,
            _ => {}
        }
        if let Some(name) = opening {
            path.push(name);
        }
        if closing {
            path.pop().context("unmatched XLSX closing element")?;
        }
    }
    ensure!(
        cell.is_none() && !in_row && path.is_empty(),
        "unterminated XLSX row/cell"
    );
    // Only annotate actual stored cells; never expand sparse or merged ranges.
    let mut merge_checks = 0usize;
    for (row, (_, cells)) in &mut rows {
        for cell in cells {
            for (r1, c1, r2, c2, range) in &merges {
                merge_checks += 1;
                ensure!(
                    merge_checks <= 2_000_000,
                    "XLSX merge lookup budget exceeded"
                );
                if (*r1..=*r2).contains(row) && (*c1..=*c2).contains(&cell.column) {
                    ensure!(
                        cell.merged_range.is_none(),
                        "overlapping XLSX merged ranges"
                    );
                    cell.merged_range = Some(range.clone());
                    // The range width belongs to its anchor, not every stored continuation.
                    cell.column_span = if *row == *r1 && cell.column == *c1 {
                        c2 - c1 + 1
                    } else {
                        1
                    };
                }
            }
        }
    }
    Ok(rows)
}

fn push_text(cell: &mut Cell, kind: u8, text: &str) {
    match kind {
        1 => cell.value.push_str(text),
        2 => cell.inline.push_str(text),
        3 => cell
            .formula
            .as_mut()
            .expect("formula started")
            .push_str(text),
        _ => {}
    }
}

fn decode_cell(
    cell: Cell,
    shared: &[String],
    formats: &[(u32, Option<String>)],
    date1904: bool,
) -> Result<TableCellEvidence> {
    let (_, column) = address(&cell.address)?;
    let value = match cell.kind.as_str() {
        "s" | "b"
            if !cell.value_seen || (cell.formula.is_some() && cell.value.trim().is_empty()) =>
        {
            String::new()
        }
        "s" => shared
            .get(cell.value.trim().parse::<usize>()?)
            .context("XLSX shared-string index outside inventory")?
            .clone(),
        "inlineStr" => cell.inline,
        "b" => match cell.value.trim() {
            "1" => "true".into(),
            "0" => "false".into(),
            _ => anyhow::bail!("invalid XLSX boolean"),
        },
        "n" | "str" | "e" | "d" => cell.value,
        _ => anyhow::bail!("unsupported XLSX cell type"),
    };
    let format = cell
        .style
        .map(|style| formats.get(style).context("XLSX style outside inventory"))
        .transpose()?;
    let normalized_value = format
        .filter(|(id, custom)| cell.kind == "n" && custom.is_none() && matches!(id, 14..=17 | 22))
        .and_then(|_| excel_date(&value, date1904));
    Ok(TableCellEvidence {
        column,
        address: Some(cell.address),
        cached_value: cell.formula.is_some()
            && cell.value_seen
            && (cell.kind == "str" || !value.trim().is_empty()),
        value,
        value_type: cell.kind,
        formula: cell.formula,
        column_span: 1,
        number_format: format.map(|(id, custom)| {
            format!(
                "{};date-system={}",
                custom
                    .as_deref()
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("builtin:{id}")),
                if date1904 { "1904" } else { "1900" }
            )
        }),
        normalized_value,
        ..Default::default()
    })
}

/// Explicit built-in calendar formats only. Excel's fictitious 1900 leap day stays raw.
fn excel_date(value: &str, date1904: bool) -> Option<String> {
    let serial = value.parse::<f64>().ok()?;
    if !serial.is_finite() || !(0.0..=2_958_465.0).contains(&serial) {
        return None;
    }
    let mut days = serial.floor() as usize;
    let epoch_year = if date1904 { 1904 } else { 1900 };
    if !date1904 {
        if days == 0 || days == 60 {
            return None;
        }
        days -= if days > 60 { 2 } else { 1 };
    }
    // Prefix day counts and a bounded binary search avoid thousands of iterations
    // per distant date in a worksheet containing many date-formatted numeric cells.
    let before_year = |year: usize| {
        let previous = year - 1;
        previous * 365 + previous / 4 - previous / 100 + previous / 400
    };
    let absolute = before_year(epoch_year) + days;
    if absolute >= before_year(10_000) {
        return None;
    }
    let mut year = epoch_year;
    let mut upper = 10_000;
    while year + 1 < upper {
        let middle = year + (upper - year) / 2;
        if before_year(middle) <= absolute {
            year = middle;
        } else {
            upper = middle;
        }
    }
    days = absolute - before_year(year);
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for length in months {
        if days < length {
            break;
        }
        days -= length;
        month += 1;
    }
    Some(format!("{year:04}-{month:02}-{:02}", days + 1))
}
fn leap(year: usize) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn workbook(sheet: &str) -> Result<Vec<u8>> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in [
            (
                "xl/workbook.xml",
                r#"<workbook xmlns:r="r"><sheets><sheet name="Requirements" r:id="r1" state="hidden"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="r1" Type="o/worksheet" Target="worksheets/sheet7.xml"/><Relationship Id="s" Type="o/sharedStrings" Target="sharedStrings.xml"/><Relationship Id="f" Type="o/styles" Target="styles.xml"/></Relationships>"#,
            ),
            (
                "xl/sharedStrings.xml",
                r#"<sst><si><r><t>Ident</t></r><rPh><t>ignored pronunciation</t></rPh><r><t>ifier</t></r></si><si><t>Requirement</t></si></sst>"#,
            ),
            (
                "xl/styles.xml",
                r#"<styleSheet><cellXfs><xf numFmtId="0"/><xf numFmtId="14"/></cellXfs></styleSheet>"#,
            ),
            ("xl/worksheets/sheet7.xml", sheet),
        ] {
            writer.start_file(name, zip::write::SimpleFileOptions::default())?;
            writer.write_all(content.as_bytes())?;
        }
        Ok(writer.finish()?.into_inner())
    }

    #[test]
    fn legacy_zip_names_preserve_evidence_and_duplicate_parts_fail_early() -> Result<()> {
        fn archive(names: &[&str]) -> Result<Vec<u8>> {
            let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
            for name in names {
                writer.start_file(*name, zip::write::SimpleFileOptions::default())?;
                writer.write_all(b"<root/>")?;
            }
            Ok(writer.finish()?.into_inner())
        }
        let base = workbook(
            r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Requirement</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>The service must respond.</t></is></c></row></sheetData></worksheet>"#,
        )?;
        let reference = extract(&base)?;
        let mut writer = zip::ZipWriter::new_append(Cursor::new(base))?;
        writer.start_file("\u{e9}.xml", zip::write::SimpleFileOptions::default())?;
        writer.write_all(b"<root/>")?;
        let mut legacy = writer.finish()?.into_inner();
        let offsets: Vec<_> = legacy
            .windows(2)
            .enumerate()
            .filter_map(|(i, bytes)| (bytes == [0xc3, 0xa9]).then_some(i))
            .collect();
        assert_eq!(offsets.len(), 2);
        for offset in offsets {
            // Convert the extra member name to valid CP437 in both headers.
            let flags = if &legacy[offset - 30..offset - 26] == b"PK\x03\x04" {
                offset - 24
            } else {
                assert_eq!(&legacy[offset - 46..offset - 42], b"PK\x01\x02");
                offset - 38
            };
            legacy[flags + 1] &= !0x08; // Clear the UTF-8 filename flag.
            legacy[offset..offset + 2].copy_from_slice(&[0x82, b'x']);
        }
        let result = extract(&legacy)?;
        assert_eq!(
            result.chunks.iter().map(|c| &c.text).collect::<Vec<_>>(),
            reference.chunks.iter().map(|c| &c.text).collect::<Vec<_>>()
        );
        assert_eq!(
            serde_json::to_value(result.facts)?,
            serde_json::to_value(reference.facts)?
        );

        let mut duplicate = archive(&["a.xml", "b.xml"])?;
        let offsets: Vec<_> = duplicate
            .windows(5)
            .enumerate()
            .filter_map(|(i, bytes)| (bytes == b"b.xml").then_some(i))
            .collect();
        assert_eq!(offsets.len(), 2);
        for offset in offsets {
            duplicate[offset] = b'a';
        }
        assert!(
            format!(
                "{:#}",
                extract(&duplicate).err().expect("duplicate part must fail")
            )
            .contains("duplicate XLSX archive parts")
        );
        Ok(())
    }

    #[test]
    fn requirements_keep_headers_addresses_dates_and_stored_formula_evidence() -> Result<()> {
        let bytes = workbook(
            r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row><row r="4"><c r="A4" t="inlineStr"><is><t>REQ-9</t></is></c><c r="B4" t="inlineStr"><is><r><t>The service must respond in at most </t></r><r><t>200 ms.</t></r></is></c><c r="C4" s="1"><v>61</v></c><c r="D4"><f>SUM(1,1)</f><v>2</v></c><c r="E4"><f t="shared" si="0"/> </c></row></sheetData><mergeCells><mergeCell ref="A4:A5"/></mergeCells></worksheet>"#,
        )?;
        let result = extract(&bytes)?;
        assert_eq!(result.chunks.len(), 2);
        let row = result
            .facts
            .iter()
            .find(|f| {
                f.kind == "table_row"
                    && matches!(&f.location, DocumentLocation::WorksheetRow { row: 4, .. })
            })
            .expect("row evidence");
        let table = row.document.as_ref().unwrap().table.as_ref().unwrap();
        assert!(table.hidden);
        assert_eq!(table.sheet.as_deref(), Some("Requirements"));
        assert_eq!(table.cells[0].header.as_deref(), Some("Identifier"));
        assert_eq!(table.cells[0].address.as_deref(), Some("A4"));
        assert!(
            result.facts.iter().any(|fact| {
                fact.kind == "table_cell"
                    && fact.name.starts_with("B4 ")
                    && fact.name.contains("200 ms")
            }),
            "header-mapped cells remain searchable by their A1 address"
        );
        assert_eq!(table.cells[0].merged_range.as_deref(), Some("A4:A5"));
        assert_eq!(table.cells[2].value, "61");
        assert_eq!(
            table.cells[2].normalized_value.as_deref(),
            Some("1900-03-01")
        );
        assert_eq!(table.cells[3].formula.as_deref(), Some("SUM(1,1)"));
        assert!(table.cells[3].cached_value);
        assert_eq!(table.cells[4].formula.as_deref(), Some(""));
        assert!(!table.cells[4].cached_value);
        assert!(result.chunks[1].text.contains("no cached result"));
        let requirement = result
            .facts
            .iter()
            .find(|f| {
                f.kind == "requirement"
                    && f.document
                        .as_ref()
                        .is_some_and(|d| d.requirement_id.as_deref() == Some("REQ-9"))
            })
            .expect("requirement");
        assert!(
            requirement
                .document
                .as_ref()
                .unwrap()
                .quantities
                .iter()
                .any(|q| q.value == "200" && q.comparator == "lte")
        );
        assert_eq!(
            result
                .facts
                .iter()
                .filter(|f| f.kind == "table_cell")
                .count(),
            7
        );
        Ok(())
    }

    #[test]
    fn empty_sparse_rows_and_cells_preserve_coordinates_without_expansion() -> Result<()> {
        let result = extract(&workbook(
            r#"<worksheet><sheetData><row r="99"/><row><c/><c t="inlineStr"><is><t>needle</t></is></c></row><row r="1048576"><c r="XFD1048576" t="inlineStr"><is><t>last</t></is></c></row></sheetData></worksheet>"#,
        )?)?;
        assert_eq!(result.chunks.len(), 2);
        assert!(matches!(
            &result.chunks[0].location,
            DocumentLocation::WorksheetRow { row: 100, .. }
        ));
        assert!(
            result
                .facts
                .iter()
                .filter_map(|f| f.document.as_ref()?.table.as_ref())
                .flat_map(|t| &t.cells)
                .any(|c| c.address.as_deref() == Some("B100"))
        );
        assert!(matches!(
            &result.chunks[1].location,
            DocumentLocation::WorksheetRow { row: 1048576, .. }
        ));
        Ok(())
    }

    #[test]
    fn malformed_packages_cells_and_paths_fail_closed() -> Result<()> {
        for xml in [
            r#"<worksheet><sheetData><row><c t="s"><v>9</v></c></row></sheetData></worksheet>"#,
            r#"<worksheet><sheetData><row r="1"><c r="A2"><v>1</v></c></row></sheetData></worksheet>"#,
            r#"<worksheet><sheetData><row><c r="A1"><v>1</v></c><c r="A1"/></row></sheetData></worksheet>"#,
            r#"<!DOCTYPE worksheet SYSTEM 'file:///private'><worksheet/>"#,
            r#"<worksheet><sheetData><row><c><v>1</v></c></row></sheetData>"#,
            r#"<comments><sheetData><row><c><v>1</v></c></row></sheetData></comments>"#,
            r#"<worksheet><row><c><v>1</v></c></row></worksheet>"#,
            r#"<worksheet><sheetData><row><c><v>1</v><v>2</v></c></row></sheetData></worksheet>"#,
            r#"<worksheet><sheetData><row><c><f>1+1</f><f>2+2</f><v>4</v></c></row></sheetData></worksheet>"#,
            r#"<worksheet><sheetData><row><c t="inlineStr"><v>wrong</v><is><t>value</t></is></c></row></sheetData></worksheet>"#,
        ] {
            assert!(extract(&workbook(xml)?).is_err(), "{xml}");
        }
        assert!(part_path("xl/workbook.xml", "../../private.xml").is_err());
        assert!(part_path("xl/workbook.xml", "https://example.com/x.xml").is_err());
        assert_eq!(
            part_path("xl/workbook.xml", "../worksheets/a.xml")?,
            "worksheets/a.xml"
        );
        assert!(address("A01").is_err());
        assert!(address("XFE1").is_err());
        assert!(relationships(r#"<Relationships><Relationship Id="r1" TargetMode="External" Target="https://example.com" Type="o/worksheet"/><Relationship Id="r1" Target="sheet.xml" Type="o/worksheet"/></Relationships>"#).is_err());
        assert!(relationships(r#"<Relationships><Relationship Id="r1" TargetMode="unknown" Target="sheet.xml" Type="o/worksheet"/></Relationships>"#).is_err());
        assert!(shared_strings("<comments><si><t>wrong part</t></si></comments>").is_err());
        assert!(styles("<comments><cellXfs><xf numFmtId=\"14\"/></cellXfs></comments>").is_err());
        Ok(())
    }

    #[test]
    fn formula_cache_presence_distinguishes_empty_text_missing_and_empty_numeric_results()
    -> Result<()> {
        let result = extract(&workbook(
            r#"<worksheet><sheetData><row r="1"><c r="A1" t="str"><f>IF(1=1,&quot;&quot;,&quot;x&quot;)</f><v/></c><c r="B1" t="b"><f>TRUE()</f></c><c r="C1"><f>1+1</f><v/></c><c r="D1" t="str"><f>&quot;text&quot;</f></c><c r="E1" t="b"><f>TRUE()</f><v>1</v></c></row></sheetData></worksheet>"#,
        )?)?;
        let table = result
            .facts
            .iter()
            .find(|fact| fact.kind == "table_row")
            .unwrap()
            .document
            .as_ref()
            .unwrap()
            .table
            .as_ref()
            .unwrap();
        assert!(table.cells[0].cached_value);
        assert!(table.cells[0].value.is_empty());
        assert!(!table.cells[1].cached_value);
        assert!(!table.cells[2].cached_value);
        assert!(!table.cells[3].cached_value);
        assert!(table.cells[4].cached_value);
        assert_eq!(table.cells[4].value, "true");
        assert!(table.cells.iter().all(|cell| cell.formula.is_some()));
        Ok(())
    }

    #[test]
    fn merged_continuations_do_not_extend_the_range_from_their_own_columns() -> Result<()> {
        let result = extract(&workbook(
            r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>anchor</t></is></c></row><row r="2"><c r="B2" t="inlineStr"><is><t>stored continuation</t></is></c></row></sheetData><mergeCells><mergeCell ref="A1:C2"/></mergeCells></worksheet>"#,
        )?)?;
        let cell = |row| {
            result.facts.iter().find(|fact| fact.kind == "table_row" && matches!(fact.location,DocumentLocation::WorksheetRow {row:r,..} if r==row)).unwrap().document.as_ref().unwrap().table.as_ref().unwrap().cells.first().unwrap()
        };
        assert_eq!(cell(1).column_span, 3);
        assert_eq!(cell(2).column_span, 1);
        assert_eq!(cell(2).merged_range.as_deref(), Some("A1:C2"));
        assert_eq!(cell(2).value, "stored continuation");
        Ok(())
    }

    #[test]
    fn excel_calendar_evidence_handles_both_epochs_without_inventing_leap_day() {
        assert_eq!(excel_date("1", false).as_deref(), Some("1900-01-01"));
        assert_eq!(excel_date("59", false).as_deref(), Some("1900-02-28"));
        assert_eq!(excel_date("60", false), None);
        assert_eq!(excel_date("61", false).as_deref(), Some("1900-03-01"));
        assert_eq!(excel_date("0", true).as_deref(), Some("1904-01-01"));
        assert_eq!(excel_date("NaN", true), None);
        assert_eq!(excel_date("2958465", false).as_deref(), Some("9999-12-31"));
        assert_eq!(excel_date("2958465", true), None);
    }

    #[test]
    fn cdata_preserves_literal_cell_text_and_shared_strings() -> Result<()> {
        assert_eq!(
            shared_strings("<sst><si><t><![CDATA[stored & literal <text>]]></t></si></sst>")?,
            vec!["stored & literal <text>"]
        );
        let rows = worksheet(
            "<worksheet><sheetData><row><c t=\"inlineStr\"><is><t><![CDATA[cell & literal <text>]]></t></is></c></row></sheetData></worksheet>",
            &[],
            &[],
            false,
            &mut 0,
            &mut 0,
        )?;
        assert_eq!(rows[&1].1[0].value, "cell & literal <text>");
        Ok(())
    }

    #[test]
    fn blank_styled_rows_do_not_steal_headers_and_text_is_not_a_calendar_serial() -> Result<()> {
        let result = extract(&workbook(
            r#"<worksheet><sheetData><row r="1"><c r="A1" s="0"/></row><row r="2"><c r="A2" t="inlineStr"><is><t>Identifier</t></is></c></row><row r="3"><c r="A3" t="str" s="1"><v>61</v></c><c r="B3" s="1"><v>61</v></c></row></sheetData></worksheet>"#,
        )?)?;
        let table = result
            .facts
            .iter()
            .find(|fact| {
                fact.kind == "table_row"
                    && matches!(fact.location, DocumentLocation::WorksheetRow { row: 3, .. })
            })
            .unwrap()
            .document
            .as_ref()
            .unwrap()
            .table
            .as_ref()
            .unwrap();
        assert_eq!(table.cells[0].header.as_deref(), Some("Identifier"));
        assert!(table.cells[0].normalized_value.is_none());
        assert_eq!(
            table.cells[1].normalized_value.as_deref(),
            Some("1900-03-01")
        );
        Ok(())
    }

    #[test]
    fn shared_string_expansion_is_capped_during_cell_decoding() {
        let shared = vec!["original fixture ".repeat(500_000)];
        let xml = r#"<worksheet><sheetData><row><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>0</v></c><c r="C1" t="s"><v>0</v></c><c r="D1" t="s"><v>0</v></c><c r="E1" t="s"><v>0</v></c></row></sheetData></worksheet>"#;
        let error = worksheet(xml, &shared, &[], false, &mut 0, &mut 0).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("decoded cell text exceeds 32 MiB")
        );
    }

    #[test]
    fn long_worksheet_headers_are_bounded_with_visible_truncation() -> Result<()> {
        let label = "original unicode β ".repeat(100);
        let sheet = format!(
            r#"<worksheet><sheetData><row r="1"><c t="inlineStr"><is><t>{label}</t></is></c></row><row r="2"><c t="inlineStr"><is><t>value</t></is></c></row></sheetData></worksheet>"#
        );
        let result = extract(&workbook(&sheet)?)?;
        let details = result
            .facts
            .iter()
            .find(|fact| {
                fact.kind == "table_row"
                    && matches!(fact.location, DocumentLocation::WorksheetRow { row: 2, .. })
            })
            .unwrap()
            .document
            .as_ref()
            .unwrap();
        let table = details.table.as_ref().unwrap();
        assert!(table.headers_truncated && details.details_truncated);
        assert!(table.cells[0].header.as_ref().unwrap().len() <= MAX_HEADER_BYTES);
        assert!(result.chunks[0].text.contains(label.trim_end()));
        Ok(())
    }
}
