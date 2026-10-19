// SPDX-License-Identifier: Apache-2.0
//! Stable unit boundaries with bounded exact partitions; citations remain separate.
use airk_core::{
    DocumentLocation,
    extraction::{ExtractionPayload, LocalChunk, LocalSpan},
};
use anyhow::{Result, ensure};
use std::collections::BTreeSet;

pub const MAX_CHUNK_BYTES: usize = 8192;
pub const MAX_CHUNK_LINES: usize = 160;

pub fn partition(payload: &ExtractionPayload, hints: &[LocalChunk]) -> Result<Vec<LocalChunk>> {
    let text = &payload.text;
    let native = payload.extraction_method == "utf8";
    let mut boundaries = BTreeSet::from([0, text.len()]);
    // Semantic document coordinates cannot be merged across page/table/sheet units.
    if !native {
        for chunk in payload.chunks.iter().filter(|chunk| chunk.region.is_none()) {
            boundaries.extend([chunk.span.start, chunk.span.end]);
        }
    }
    let mut ordered: Vec<_> = hints.iter().filter(|hint| hint.region.is_none()).collect();
    ordered.sort_by_key(|hint| (hint.span.start, std::cmp::Reverse(hint.span.end)));
    let mut prior_end = 0;
    for hint in ordered {
        let span = &hint.span;
        ensure!(
            span.start <= span.end
                && span.end <= text.len()
                && text.is_char_boundary(span.start)
                && text.is_char_boundary(span.end),
            "invalid chunk hint span"
        );
        if span.start < prior_end || span.start == span.end {
            continue;
        }
        boundaries.extend([span.start, span.end]);
        prior_end = span.end;
    }
    let mut starts = vec![0];
    starts.extend(
        text.bytes()
            .enumerate()
            .filter(|(_, byte)| *byte == b'\n')
            .map(|(index, _)| index + 1),
    );
    let line = |offset| starts.partition_point(|start| *start <= offset);
    let boundaries: Vec<_> = boundaries.into_iter().collect();
    let mut chunks = Vec::new();
    let mut document_cursor = 0;
    for window in boundaries.windows(2) {
        let mut start = window[0];
        while start < window[1] {
            let mut end = window[1].min(start.saturating_add(MAX_CHUNK_BYTES));
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            if let Some(line_end) = starts.get(line(start) - 1 + MAX_CHUNK_LINES) {
                end = end.min(*line_end);
            }
            if end < window[1]
                && let Some(newline) = text[start..end].rfind('\n')
            {
                end = start + newline + 1;
            }
            // A byte cap can land between the two bytes of one CRLF. Keep
            // that complete line ending in the next chunk without exceeding
            // the hard byte allowance or changing any source bytes.
            if end < window[1]
                && text.as_bytes().get(end) == Some(&b'\n')
                && text.as_bytes().get(end.wrapping_sub(1)) == Some(&b'\r')
            {
                end -= 1;
            }
            ensure!(end > start, "chunk partition made no progress");
            let start_line = line(start);
            let end_line = line(end - 1);
            let location = if native {
                DocumentLocation::Lines {
                    start: start_line,
                    end: end_line,
                }
            } else {
                while payload
                    .chunks
                    .get(document_cursor)
                    .is_some_and(|chunk| chunk.span.end <= start || chunk.region.is_some())
                {
                    document_cursor += 1;
                }
                let origin = payload
                    .chunks
                    .get(document_cursor)
                    .ok_or_else(|| anyhow::anyhow!("document chunk location missing"))?;
                ensure!(
                    origin.span.start <= start && origin.span.end >= end,
                    "document partition crosses native location"
                );
                origin.location.clone()
            };
            chunks.push(LocalChunk {
                span: LocalSpan { start, end },
                start_line,
                end_line,
                location,
                region: None,
            });
            start = end;
        }
    }
    // Existing mapped children retain their qualified map/citation partition;
    // new composite hosts must provide their own qualified region hints.
    chunks.extend(
        payload
            .chunks
            .iter()
            .filter(|chunk| chunk.region.is_some())
            .cloned(),
    );
    ensure!(
        chunks.len() <= airk_core::extraction::MAX_EXTRACTION_CHUNKS,
        "chunk count exceeds bound"
    );
    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn payload(text: &str) -> ExtractionPayload {
        ExtractionPayload {
            semantic_projection: Default::default(),
            contract_version: 1,
            extraction_method: "utf8".into(),
            parse_status: "plain".into(),
            provenance: vec![],
            text: text.into(),
            facts: vec![],
            syntax_identities: vec![],
            chunks: vec![],
            regions: vec![],
            relationships: vec![],
            continuity: vec![],
            continuity_coverage: BTreeMap::new(),
            diagnostics: vec![],
            partial: false,
        }
    }
    fn hint(start: usize, end: usize, location: DocumentLocation) -> LocalChunk {
        LocalChunk {
            span: LocalSpan { start, end },
            start_line: 1,
            end_line: 1,
            location,
            region: None,
        }
    }
    fn lines(start: usize, end: usize) -> LocalChunk {
        hint(start, end, DocumentLocation::Lines { start: 1, end: 1 })
    }
    fn assert_partition(p: &ExtractionPayload, chunks: &[LocalChunk]) {
        let mut cursor = 0;
        let mut joined = String::new();
        for chunk in chunks.iter().filter(|chunk| chunk.region.is_none()) {
            assert_eq!(chunk.span.start, cursor, "partition has a gap or overlap");
            assert!(chunk.span.end > chunk.span.start);
            assert!(
                p.text.is_char_boundary(chunk.span.start)
                    && p.text.is_char_boundary(chunk.span.end)
            );
            assert!(chunk.span.end - chunk.span.start <= MAX_CHUNK_BYTES);
            assert!(chunk.end_line - chunk.start_line < MAX_CHUNK_LINES);
            joined.push_str(&p.text[chunk.span.start..chunk.span.end]);
            cursor = chunk.span.end;
        }
        assert_eq!(cursor, p.text.len());
        assert_eq!(joined, p.text);
    }

    #[test]
    fn overlapping_nested_and_unsorted_hints_still_partition_every_byte_once() -> Result<()> {
        let p = payload("prefix\nfunction original() {\n  return café;\n}\nsuffix\n");
        let outer_start = p.text.find("function").unwrap();
        let inner_start = p.text.find("return").unwrap();
        let end = p.text.find("suffix").unwrap();
        let hints = [
            lines(inner_start, end - 2),
            lines(outer_start, end),
            lines(0, 0),
            lines(outer_start, end),
        ];
        let chunks = partition(&p, &hints)?;
        assert_partition(&p, &chunks);
        assert_eq!(chunks.len(), 3);
        assert_eq!(
            &p.text[chunks[1].span.start..chunks[1].span.end],
            "function original() {\n  return café;\n}\n"
        );
        Ok(())
    }

    #[test]
    fn utf8_and_crlf_boundaries_keep_complete_characters_and_line_endings() -> Result<()> {
        let text = format!(
            "{}\r\n{}\r\n",
            "a".repeat(MAX_CHUNK_BYTES - 1),
            "é🦀".repeat(2000)
        );
        let p = payload(&text);
        let chunks = partition(&p, &[])?;
        assert_partition(&p, &chunks);
        for chunk in &chunks {
            assert!(
                !(p.text[..chunk.span.end].ends_with('\r')
                    && p.text[chunk.span.end..].starts_with('\n')),
                "chunk splits CRLF"
            );
        }
        Ok(())
    }

    #[test]
    fn oversized_units_obey_both_byte_and_line_limits() -> Result<()> {
        for text in [
            "original café evidence\n".repeat(501),
            "🦀".repeat(MAX_CHUNK_BYTES),
            "x".repeat(MAX_CHUNK_BYTES * 3 + 17),
        ] {
            let p = payload(&text);
            let chunks = partition(&p, &[lines(0, text.len())])?;
            assert_partition(&p, &chunks);
            assert!(chunks.len() > 1);
        }
        Ok(())
    }

    #[test]
    fn native_page_table_and_worksheet_boundaries_never_merge() -> Result<()> {
        let mut p = payload("Original page.\nOriginal table row.\nOriginal worksheet row.\n");
        p.extraction_method = "original-native-fixture".into();
        let first = p.text.find("Original table").unwrap();
        let second = p.text.find("Original worksheet").unwrap();
        p.chunks = vec![
            hint(0, first, DocumentLocation::Page { number: 7 }),
            hint(
                first,
                second,
                DocumentLocation::TableRow { table: 2, row: 9 },
            ),
            hint(
                second,
                p.text.len(),
                DocumentLocation::WorksheetRow {
                    sheet: "Original sheet".into(),
                    row: 11,
                },
            ),
        ];
        let chunks = partition(&p, &[lines(0, p.text.len())])?;
        assert_partition(&p, &chunks);
        assert_eq!(chunks.len(), p.chunks.len());
        for (actual, original) in chunks.iter().zip(&p.chunks) {
            assert_eq!(actual.location, original.location);
            assert_eq!(actual.span, original.span);
        }
        Ok(())
    }

    #[test]
    fn split_native_page_retains_original_page_label() -> Result<()> {
        let mut p = payload(&"Original page evidence.\n".repeat(400));
        p.extraction_method = "pdf-text-layer".into();
        p.chunks
            .push(hint(0, p.text.len(), DocumentLocation::Page { number: 42 }));
        let chunks = partition(&p, &[])?;
        assert_partition(&p, &chunks);
        assert!(chunks.len() > 1);
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.location == DocumentLocation::Page { number: 42 })
        );
        Ok(())
    }

    #[test]
    fn unknown_native_location_and_invalid_hint_fail_explicitly() {
        let mut native = payload("Unbound native bytes.\n");
        native.extraction_method = "pdf-text-layer".into();
        assert!(partition(&native, &[]).is_err());
        let p = payload("café\n");
        assert!(partition(&p, &[lines(0, 4)]).is_err());
        assert!(partition(&p, &[lines(2, 1)]).is_err());
        assert!(partition(&p, &[lines(0, p.text.len() + 1)]).is_err());
        assert!(partition(&payload(""), &[]).unwrap().is_empty());
    }

    #[test]
    fn declaration_chunk_content_is_stable_after_prefix_insertion() -> Result<()> {
        let declaration = "fn original(value: u32) -> u32 {\n    value + 17\n}\n";
        let p = payload(&format!("// old header\n{declaration}"));
        let prefix = "// inserted original documentation\n// second inserted line\n";
        let changed = payload(&format!("{prefix}{}", p.text));
        let before_start = p.text.find("fn original").unwrap();
        let after_start = changed.text.find("fn original").unwrap();
        let before = partition(&p, &[lines(before_start, p.text.len())])?;
        let after = partition(&changed, &[lines(after_start, changed.text.len())])?;
        assert_partition(&p, &before);
        assert_partition(&changed, &after);
        let old = before
            .iter()
            .find(|chunk| chunk.span.start == before_start)
            .unwrap();
        let new = after
            .iter()
            .find(|chunk| chunk.span.start == after_start)
            .unwrap();
        assert_eq!(&p.text[old.span.start..old.span.end], declaration);
        assert_eq!(&changed.text[new.span.start..new.span.end], declaration);
        assert_eq!(new.start_line, old.start_line + 2);
        assert_ne!(old.location, new.location);
        Ok(())
    }
}
