---
id: document-types
title: Document types and extraction
route:
area: guidance
aliases: formats, file types, pdf, scanned pdf, image, OCR, Markdown, docx, xlsx, spreadsheet, pptx, slides, requirements, source code
related_airk_help: documents, formats, indexing-choices, code, configuration
---
# Document types and extraction

Begin with airk_capabilities for the running engine's supported adapters and optional local providers. Inspect representative files with airk_file_info or Evidence > Inspector; a filename extension alone is not proof of successful or complete extraction.

## Choose for the source

- **Code and configuration**: use Agent Lean for selected declarations, imports and syntax candidates. Rich Structure can add supported structural/framework evidence. Written targets and text references remain unresolved candidates unless an explicitly identified producer supplies stronger evidence. Ambiguous extensions may need reviewed adapter overrides through Extraction policy file; selecting an adapter never executes a build or expands its supported dialects.
- **Markdown and plain text**: useful for requirements, ADRs, README files and agent instructions. Search Only keeps text and heading boundaries; Agent Lean or richer detail is needed for supported requirement/decision/acceptance cues. English-oriented prose cues are candidates to inspect, not accepted obligations. Markdown code fences and recognized code-file strings/comments are not prose requirements.
- **Text-layer PDF**: native extraction retains the original PDF and page provenance. Extracted text lines are not printed page line numbers. Tables, layout, typography, figures and reading order may be incomplete. Check the cited page in the original document before making a claim.
- **DOCX**: retains supported paragraphs, built-in headings and table rows/cells with original block/table locations. Custom style inheritance and arbitrary layout semantics are not complete. Richer detail helps when the task needs broader table or document inventories.
- **XLSX**: preserves worksheet/row/cell locations, stored formula text and cached values. Hidden data can remain searchable and is labeled. Cached formula results may be stale; AIRK does not recalculate, run macros or load linked workbooks. Inspect the worksheet as well as the A1 cell address. Large workbooks can hit fact/byte limits before every cell is returned.
- **PPTX**: uses supported slide text and explicit title placeholders with original slide provenance. Do not infer complete visual reading order, chart meaning or image contents from extracted text alone.
- **Scanned PDF and raster images**: ordinary indexing does not automatically OCR them. Explicit local OCR requires provisioned Tesseract/language assets and, for PDFs, a local renderer. Ask airk_help command=ocr and topic=documents for current requirements. Selected pages, confidence limitations and OCR-derived provenance must remain visible. Unselected pages remain unknown; later OCR can replace the selected extraction for the artifact.

## Steps

1. Select the purpose and source formats with the user, then read Choose indexing by purpose.
2. Inspect adapter/profile, extraction method, line basis, parse quality and source hash for representative files.
3. Use Documents > Facts for requirement, decision, acceptance, heading or table candidates; use Search for exact text or concepts. Keep an original document location with every quotation.
4. Follow result pages and inspect truncation, skipped-file/scan diagnostics and native failures. An empty fact list can mean the chosen detail or adapter omitted it.
5. Change extraction or scan policy only for a demonstrated need, review the native job and re-check the published profile. Keep source files read-only throughout ingestion.

## Limits

Increasing byte or scan limits cannot create text that is absent from a scan or recover unsupported layout semantics. Vector similarity does not cure failed extraction. Never silently substitute OCR for source text, table caches for recalculated values or a cue classifier for a verified requirement.
