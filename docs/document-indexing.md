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

For MCP, use the existing read-only tool; the fourteen-tool inventory is unchanged:

~~~json
{"name":"airk_document_facts","arguments":{"kind":"requirement","path_prefix":"docs/","name":"latency","limit":10,"max_bytes":8192}}
~~~

The `kind` filter is exact; `name` matches a case-insensitive substring of the source statement or fact name, and path_prefix is literal. Date fact names expose normalized dates when available, so a date substring can find English dates with an ISO representation. Unknown categories return no matches; there is no date-range, unit-converting numeric predicate, project-wide requirements graph or conflict-resolution service yet. Outline also returns these facts and their document metadata.

The extractor version is now `native-facts/7`, including table/spreadsheet evidence and corrected header/cache handling. Existing schema-3 snapshots remain readable because document/code/table metadata is optional. The next index pass re-extracts unchanged files when their extractor version differs and migrates legacy JSON state to SQLite, then publishes a new generation. Existing snapshots gain facts only after that pass succeeds. [MCP contracts](mcp.md), [code storage](code-facts-and-relationships.md) and [format support](formats.md) describe the shared limits.
