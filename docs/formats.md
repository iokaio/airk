# Implemented format support

Native support means the shipped binary contains the parser/extractor. No Python, Office installation, browser, OCR service or remote provider is needed at runtime. Each eligible file retains its original-byte SHA-256 hash. Generic UTF-8 lexical retrieval remains available for unknown extensions.

## Code grammar profiles

| Language | Extensions | Selected native facts |
|---|---|---|
| Rust | rs | Functions, types, traits, constants/modules/macros, use/extern crate |
| Python | py, pyi | Functions/classes and import/from declarations |
| JavaScript / JSX | js, jsx, mjs, cjs | Functions/classes/methods/variables and static imports |
| TypeScript / TSX | ts, mts, cts, tsx | Functions/types/interfaces/classes/enums/variables and imports |
| Go | go | Functions/methods/type specs and imports |
| Java | java | Classes/interfaces/records/enums/methods/constructors and imports/packages |
| C | c, h | Function/type/struct/enum declarations selected by profile, includes |
| C++ | cpp, cc, cxx, hpp, hh, hxx | Functions/classes/structs/enums/namespaces and includes/using |
| C# | cs | Classes/structs/interfaces/records/enums/methods/namespaces and using |
| Ruby | rb | Classes/modules/methods; dynamic require resolution unavailable |
| PHP | php | Classes/interfaces/traits/functions/methods, selected use/include/require |
| Bash-family | sh, bash, zsh | Bash grammar function definitions; Zsh-specific constructs may be partial |
| PowerShell | ps1, psm1, psd1 | Selected function/class and using statements |
| Lua | lua | Function declarations; dynamic require resolution unavailable |
| Swift | swift | Selected function/class/protocol/typealias and import declarations |
| Kotlin | kt, kts | Selected function/class/object/typealias and import declarations |
| SQL | sql | Selected create table/view/function/index nodes; dialect coverage varies |

These are initial grammar profiles, not complete compiler/type-system semantics. Original fixtures exercise a definition in every listed language. Declarations containing grammar errors are suppressed and file_info reports partial-syntax. Macros, overloaded/dynamic names, cross-file references and all framework-specific behavior need future adapters. A .h file uses C; C++ headers should use an explicit C++ extension until language overrides exist. Grammar-recognized syntax does not guarantee that every declaration has a named fact extractor.

## Structured text and documents

| Format | Native extraction/facts | Citation and scope |
|---|---|---|
| Markdown / MDX | Heading hierarchy, tables, requirement/date/design/architecture/feature/decision candidates; lexical source | Source lines; code examples excluded from prose cues |
| JSON / JSONC | Tree-sitter object keys | Source lines; comments/nonstandard syntax may produce partial-syntax |
| YAML / TOML | Native mapping keys/tables | Source lines; key names are local, not fully qualified paths |
| HTML | Element/tag facts and raw text retrieval | Source lines; Vue/Svelte markup uses HTML grammar, not full component semantics |
| CSS / SCSS | Selector facts and source retrieval | Source lines; SCSS-only syntax may be partial |
| XML / SVG | Elements and selected identity/reference/drawing attributes | Source lines; no rendering, scripts, external resource/entity loading |
| CSV / TSV | Parsed records and column headers | Source lines; header assumption, validates record consistency |
| INI/config/properties | Sections and simple key/value lines | Source lines; initial line adapter, not every configuration dialect |
| Dockerfile | Instruction facts | Source lines; initial line adapter, no image/build execution |
| Plain/other UTF-8 | Deterministic text chunks, conservative headings and prose candidates | Source lines; English cue parsing, not inferred semantics |
| PDF | Native text-layer extraction by page plus shared prose candidates | Original page; normalized extracted-text lines are secondary coordinates |
| DOCX | Main-body paragraphs, run/entity decoding, built-in headings; table rows/cells, header mapping, merge markers and shared prose candidates | Original paragraph block or table/row ordinal, no page-layout claim |
| XLSX | Worksheet rows/cells, shared/inline rich strings, header candidates, requirement/date/quantity cues, stored formula results and merge markers | Original worksheet name, row and cell addresses; formulas never evaluated |
| PPTX | OOXML slide text, title placeholders and shared prose candidates; relationship order | Original presentation-order slide number; title context isolated per slide |

Explicit local OCR supports selected PDF pages and PNG/JPEG/BMP inputs using provisioned Tesseract models and, for PDFs, Poppler. It is not run automatically and does not promise table/layout reconstruction. Image-only slides, encrypted documents requiring passwords, legacy DOC/PPT/XLS, XLSB/XLSM, ODT, EPUB and RTF decoding are not implemented. DOCX headers/footers/comments/embedded objects and PPTX notes/charts/image contents are not extracted. XLSX charts, drawings, comments, pivot caches and embedded objects are not extracted. SVG path geometry is retrievable source but is not interpreted as visual meaning.

OOXML entries are read in memory, not unpacked to the filesystem. External slide/worksheet relationships and XML document type declarations are refused; unused external links are never followed. Extraction skips are explicit, rather than replacement-character text pretending to be evidence. Spreadsheet formulas and cached results are labeled separately; caches can be stale or absent. AIRK does not launch Office, recalculate formulas or fetch linked workbooks.

[Document indexing](document-indexing.md) specifies heading context, requirement IDs/modality, written numeric bounds/units, qualitative/design/feature candidates, date ambiguity and category-filtered retrieval. These cues retain source evidence rather than asserting compliance or semantic understanding.

## Current safety limits

- Source file: configurable up to 16 MiB; extracted text/XML part: 4 MiB.
- Default corpus: 64 MiB eligible source, 128 MiB extracted text and 10,000 files. [Persisted collection limits](ingestion-limits.md) permit explicit larger profiles within validated ceilings.
- Default index scan: 100,000 visited non-ignored entries (including directories) and 256 MiB source reads, including unsupported files. Known oversized files are skipped before reading. Exhausting configured pass budgets preserves the previous generation.
- PDF/PPTX: 512 pages/slides; OOXML archive: 10,000 entries/paragraphs.
- Native syntax: 500,000 visited nodes and 10,000 facts per file.
- DOCX: 256 tables, 32 nested table levels, 256 cells per row; table rows count toward the document fact/text budgets.
- XLSX: 128 sheets, 10,000 rows per sheet, 100,000 stored cells/shared strings per workbook, 16 MiB of selected XML parts; sparse/merged ranges are never expanded. The shared 10,000-fact limit can be reached before the cell limit.
- Response: 1–100 results and up to 65,536 payload bytes, metadata additional.

These are retained beta safety bounds pending large-repository qualification. PDF output limits are checked after library extraction, so they do not bound its peak memory. Use trusted local inputs until worker/resource isolation is qualified.
