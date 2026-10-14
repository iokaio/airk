# Document requirements and design indexing

AIRK adds deterministic document facts alongside lexical text. Markdown, plain/other UTF-8 text, text-layer PDF, DOCX, XLSX and PPTX share a prose-cue parser. Code grammar profiles and structured configuration/XML/SVG adapters retain their own syntax facts; strings and comments in recognized code files do not become prose requirements. This is local parsing with no model, provider call or document execution.

## What is extracted

| Evidence | Parsing and returned details |
|---|---|
| Headers | Markdown heading hierarchy, DOCX built-in Heading1–Heading9/Title styles or direct outline levels, PPTX title/centered-title placeholders, and conservative plain-text section cues |
| Requirements | Paragraph/list/table-row candidates carrying source statements, heading paths, REQ/FR/NFR/AC/R identifiers when present, and explicit must/shall/should/required-to/need-to wording, including negative forms |
| Numeric requirements | Written bounds such as less than, at most, at least, <= and >=; ranges such as 5–10 MiB; counts, durations, storage, frequencies and percentages |
| Qualitative requirements | Recognized wording such as accessible, intuitive, maintainable, secure, reliable, scalable, easy to use and loosely coupled; mixed quantitative/qualitative candidates retain both |
| Design and architecture | Heading context and explicit design/component/interface/layer/boundary/pipeline/rationale/trade-off wording |
| Features and user stories | Feature/capability headings or wording, supports/allows/enables/provides, and the “As a … I want …” pattern |
| Decisions and acceptance | Decision-heading/choice wording and acceptance headings or Given/When/Then cues |
| Dates | Explicit ISO calendar dates and English month-name dates; source spelling plus a validated ISO normalization when possible |

These are evidence categories, not proof of mandatory obligations, implemented features, accepted architectural decisions or measured compliance. A paragraph can have several categories. Requirements under a requirements/constraints heading may be candidates even without a modal verb. The parser preserves the source statement; an agent should inspect its cited context before interpreting it. Classification is currently English-oriented.

Markdown parsing follows its native syntax for headings, paragraphs, list items and table rows. Fenced/indented code and raw HTML do not contribute prose cues; inline code does not contribute modality/date/quantity cues. Table headers do not become requirement statements. Markdown source text remains available through lexical search and read/context tools. Text-only documents use blank-line/list boundaries and a small section-title vocabulary; ordinary typography alone does not establish a heading.

DOCX heading context persists across main-body paragraph blocks. Built-in style names and direct outline levels are supported; resolving custom styles through styles.xml is not implemented. PPTX title context is isolated per slide and comes from explicit title placeholders when available. PDF headings are text cues rather than recovered layout/font semantics. Original page/block/slide provenance remains attached to every extracted fact; normalized line ranges are secondary coordinates and may cover the containing paragraph or row.

## DOCX tables and XLSX requirements

DOCX tables add `table_row` and `table_cell` facts alongside searchable paragraph blocks. Each row retains its original table/row ordinal, logical column positions and header mapping. Explicit `tblHeader` rows supply headers and carry `is_header: true`; these rows do not become requirement candidates. Table prose cues are classified once from structured rows, so table headers cannot change later body-paragraph heading context. Otherwise the first row is labeled a header **candidate** and remains eligible for prose cues. Horizontal spans and vertical merge restarts/continuations remain evidence. Empty cells keep their positions; continuation cells are never populated with an inferred value. Nested tables have distinct table ordinals.

XLSX extraction follows workbook relationships and sheet order. It reads stored shared strings, inline rich text, numbers, booleans, errors, dates and formula text/results without Office. A row citation contains the original worksheet name and row number; `document.table.cells` includes original addresses such as `B14`. Hidden sheets/rows remain searchable and carry a hidden marker. Sparse coordinates and merged ranges are annotated without expanding missing cells. The first nonempty row supplies header candidates; named-table definitions and arbitrary multirow header inference are not implemented.

Cell fact names retain their A1 addresses even when a header supplies the human-readable label. For example, `airk facts requirements.xlsx --kind table_cell --name B14` retrieves matching indexed cell facts; inspect the returned worksheet name to distinguish the same address on multiple sheets.

The same requirement/date/quantity rules analyze each normalized row, with the header mapping attached. For example, `A14=REQ-9` and `B14=The API must respond in at most 200 ms` produce a requirement candidate retaining `REQ-9`, the `lte` quantity and both cell addresses. Cell boundaries stay explicit so separate numeric/unit cells are not accidentally joined into a fabricated constraint.

Formula text and cached values are separate fields. A cached value can be stale, and an absent cache is labeled explicitly. An explicit empty string result is a cache; an absent value or empty numeric value is not. Invalid worksheet part roots, duplicate cell values/formulas and ambiguous relationship IDs fail extraction rather than producing plausible evidence. AIRK never recalculates formulas, resolves shared-formula expressions, loads external workbooks or executes macros. Supported built-in Excel calendar formats also expose an ISO date with the original serial, format and 1900/1904 epoch retained. The fictitious 1900-02-29 stays unnormalized; custom/locale-dependent formats stay raw. Date-time serials retain the raw fraction; the normalized value is a calendar date only.

Row metadata carries at most 32 cells; individual cell facts retain their own coordinate/evidence. Text and metadata truncation are explicit. All row/cell/prose facts share the file's 10,000-fact bound, so a large workbook can exceed it even with fewer than 10,000 rows. See [format limits](formats.md). The table layout follows [WordprocessingML tables](https://learn.microsoft.com/en-us/office/open-xml/word/working-with-wordprocessingml-tables) and [SpreadsheetML shared strings](https://learn.microsoft.com/en-us/office/open-xml/spreadsheet/working-with-the-shared-string-table); these are stored document structures, not inferred requirements semantics.

## Explicit local OCR

Native discovery still uses PDF text layers and never invokes OCR automatically. For a scanned PDF or a single PNG/JPEG/BMP image, use `airk ocr scans/requirements.pdf --request /indexes/ocr.json` with explicitly provisioned local Tesseract, language assets and, for PDF input, Poppler `pdftoppm`:

~~~json
{
  "tesseract": {"program": "/usr/bin/tesseract", "args": [], "timeout_ms": 120000},
  "pdf_renderer": {"program": "/usr/bin/pdftoppm", "args": [], "timeout_ms": 30000},
  "tessdata_dir": "/usr/share/tesseract-ocr/5/tessdata",
  "language": "eng",
  "first_page": 2,
  "last_page": 5,
  "dpi": 150
}
~~~

For raster input, set `pdf_renderer` to `null` and both page numbers to 1. Executables and model directories must be explicit absolute paths; provider argument lists must be empty. Language names select 1–8 existing local `.traineddata` files, without download or network fallback. Temporary source/raster copies are private and cleaned up; original files are read-only. The published artifact keeps the original source SHA-256 and original page numbers. Its extraction method binds a provenance digest; retained `parse_status` contains the full JSON provenance, including executable/model digests, selected pages, DPI and recognition limitations. Derived prose facts carry an `ocr-derived` basis.

OCR covers only the selected pages, at most 32 per operation within pages 1–512. Unselected pages remain unknown; a later OCR operation replaces that artifact's selected extraction. Raster dimensions are checked before invocation (maximum 16 million pixels), and PDF pages are rendered individually with a 2,400-pixel maximum dimension. Renderer stdout is capped at 16 MiB per page, recognized text at 1 MiB per page/4 MiB per document, and stderr at 64 KiB. The selected Tesseract timeout bounds the combined subprocess work, at most 120 seconds; each renderer also respects its own timeout. Whole child jobs/groups are terminated on failure. Executable/model hashing and filesystem cleanup are outside that process timer.

Recognition, spelling, layout and reading order can be wrong. AIRK does not recover table geometry from OCR text or infer missing cells. An empty/malformed/failed result does not replace a published revision. Unchanged ordinary indexing can reuse the retained OCR artifact; changed sources and full native reindexing require explicitly requesting OCR again. Models and provider binaries are not bundled or globally installed by AIRK. The invocation follows [Tesseract's local command-line interface](https://tesseract-ocr.github.io/tessdoc/Command-Line-Usage.html).

The Windows original-bitmap fixture was exercised with installed Tesseract `5.4.0.20240606` (executable SHA-256 `babb405f4366b480d02cd8ff2bac8d497170f6c1711ce6f3d5d8bf0fb7fa6ed9`) and `eng.traineddata` SHA-256 `7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2`. It verifies numeric text, original page/source identity and retained model provenance; it is not an OCR accuracy benchmark. Set `AIRK_TEST_TESSERACT` and `AIRK_TEST_TESSDATA` to exercise the opt-in Rust test, or use [the public CLI smoke](../scripts/check-local-providers.py), which also supports a two-page image-only PDF when `--pdftoppm` is supplied.

The final Linux helper also passed that public CLI smoke as an unprivileged user in a network-disabled Debian container, using Tesseract `5.3.0-2` and Poppler `22.12.0-2+deb12u3`. The original image-only PDF selects page 2 and checks recognized numeric text, original page/source identity, unchanged-index reuse and read-only originals. Executable SHA-256 values were `1e8c7ce7f27d2d1c902fb648efed443483f2a8fc7b60c48a5d3b61d647a2649e` for Tesseract and `eaac98d4d950f4a25adfe55254c6cc7299429a07370181b5acfd87ac6db42f13` for `pdftoppm`; the English model digest matches the Windows fixture above. This qualifies the exercised local adapter paths, not arbitrary page layouts, fonts, languages or OCR accuracy.

## Metadata and interpretation

Facts have their existing name/kind/syntax_kind/citation fields plus optional `document` metadata. It records parser_version (`document-cues/1`), basis (explicit heading/date text or heuristic candidate), heading_path, requirement_id, modality, requirement_type, dates, quantities and truncation flags. Requirement types are numeric, qualitative, mixed or functional. A “may” cue is treated as permissive wording only in requirement context; a month name inside a recognized date does not become modality.

For `REQ-1: The API must respond in less than 200 ms by 13 October 2026`, the requirement fact retains `REQ-1`, `must`, the written `lt` bound, decimal value `200`, source unit `ms` and date `2026-10-13`. Values stay decimal strings, source units stay unchanged and ranges carry a separate upper_value. AIRK does not convert units, infer inclusive range endpoints, combine negation into a constraint expression or evaluate whether a requirement is met. A bare quantity has comparator `unspecified`; a prose comparator records recognized written wording rather than a solved logical constraint.

Numeric slash dates such as `10/11/2026` retain an unknown locale and no normalized value. Invalid calendar dates retain their source spelling and an invalid-calendar label. Relative dates, two-digit years, locale-specific month names and a deadline's relationship to an event are not inferred. Regex parsing is a bounded initial vocabulary, not complete natural-language understanding.

Facts are capped at 10,000 per file, statements at 1,024 UTF-8 bytes and date/quantity details at 32 each per unit. Long statements/details expose truncation flags. Document metadata counts against the fact payload budget along with names/paths; ordinary envelope/citation framing is additional. A file exceeding its fact/event bound is explicitly skipped by ingestion rather than publishing an apparently complete partial fact set.

## Query and upgrade

Index with the updated helper, then retrieve only the category needed:

~~~console
airk --root /projects/a --data-dir /indexes/airk --collection project-a index
airk --root /projects/a --data-dir /indexes/airk --collection project-a facts docs/ --kind requirement
airk --root /projects/a --data-dir /indexes/airk --collection project-a facts docs/ --kind date
airk --root /projects/a --data-dir /indexes/airk --collection project-a facts docs/ --kind architecture
~~~

For MCP, use the existing read-only document tool within the fifteen-tool inventory:

~~~json
{"name":"airk_document_facts","arguments":{"kind":"requirement","path_prefix":"docs/","name":"latency","limit":10,"max_bytes":8192}}
~~~

The `kind` filter is exact; `name` matches a case-insensitive substring of the source statement or fact name, and path_prefix is literal. Date fact names expose normalized dates when available, so a date substring can find English dates with an ISO representation. Unknown categories return no matches; there is no date-range, unit-converting numeric predicate, project-wide requirements graph or conflict-resolution service yet. Outline also returns these facts and their document metadata.

The extractor version is now `native-facts/7`, including table/spreadsheet evidence and corrected header/cache handling. Existing schema-3 snapshots remain readable because document/code/table metadata is optional. The next index pass re-extracts unchanged files when their extractor version differs and migrates legacy JSON state to SQLite, then publishes a new generation. Existing snapshots gain facts only after that pass succeeds. [MCP contracts](mcp.md), [code storage](code-facts-and-relationships.md) and [format support](formats.md) describe the shared limits.
