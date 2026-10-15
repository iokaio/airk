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
| Dockerfile | Instructions, stages/aliases, images, COPY sources, ARG/ENV, workdir/ports | Original byte/line spans, including supported multiline commands; no image/build execution |
| Plain/other UTF-8 | Deterministic text chunks, conservative headings and prose candidates | Source lines; English cue parsing, not inferred semantics |
| PDF | Native text-layer extraction by page plus shared prose candidates | Original page; normalized extracted-text lines are secondary coordinates |
| DOCX | Main-body paragraphs, run/entity decoding, built-in headings; table rows/cells, header mapping, merge markers and shared prose candidates | Original paragraph block or table/row ordinal, no page-layout claim |
| XLSX | Worksheet rows/cells, shared/inline rich strings, header candidates, requirement/date/quantity cues, stored formula results and merge markers | Original worksheet name, row and cell addresses; formulas never evaluated |
| PPTX | OOXML slide text, title placeholders and shared prose candidates; relationship order | Original presentation-order slide number; title context isolated per slide |

Explicit local OCR supports selected PDF pages and PNG/JPEG/BMP inputs using provisioned Tesseract models and, for PDFs, Poppler. It is not run automatically and does not promise table/layout reconstruction. Image-only slides, encrypted documents requiring passwords, legacy DOC/PPT/XLS, XLSB/XLSM, ODT, EPUB and RTF decoding are not implemented. DOCX headers/footers/comments/embedded objects and PPTX notes/charts/image contents are not extracted. XLSX charts, drawings, comments, pivot caches and embedded objects are not extracted. SVG path geometry is retrievable source but is not interpreted as visual meaning.

### Repository and build formats

| Profile / selection | Selected evidence |
|---|---|
| HCL: `.tf`, `.tfvars`, `.hcl` | Written block declarations/labels, attributes and traversal references |
| Dockerfile: `Dockerfile`, `Dockerfile.*`, `.dockerfile` | Instructions and stage-scoped configuration; LF multiline and CRLF single-line locations |
| Protobuf: `.proto` | Packages/imports, messages/enums/fields/numbers, services/RPCs and written type references |
| GraphQL: `.graphql`, `.gql` | Schema/type/field/argument declarations, operations/fragments, spreads and written references |
| CMake: `CMakeLists.txt`, `.cmake` | Targets, sources, libraries, includes, variables and unevaluated expressions |
| Make: `Makefile`, `.mk` | Targets, prerequisites, variables, includes and recipe spans |
| MSBuild: `.csproj`, `.fsproj`, `.vbproj`, `.props`, `.targets`, `.sln`, `.slnx` | Projects/packages/frameworks, imports/targets/properties/conditions and solution membership |
| Actions: source-root `.github/workflows/*.yml` or `*.yaml` | Triggers/jobs/steps/needs/uses, permissions, environment/configuration and written commands |
| Compose: `compose.yaml`, `compose.yml`, `docker-compose.yml`, `docker-compose.yaml` | Services/images/build contexts/dependencies/networks/volumes and written configuration |

These profiles preserve original source locations and never evaluate builds, resolve references, expand variables or fetch dependencies. The four repository grammars expose conservative declaration continuity; CMake, Make, MSBuild, Actions and Compose expose structural-rule evidence without continuity. Explicit extraction-policy overrides take precedence, including for additional source roots.

Unsupported constructs remain explicit: Docker CRLF line continuations and Make custom recipe prefixes are partial syntax; malformed declarations are omitted. Workflow/Compose YAML aliases, anchors, tags, merges, duplicate keys and multiple documents produce diagnostics without expansion or key overwriting. MSBuild XML forbids DTDs and limits nesting to 128, events to 500,000 and attributes to 256 per element with 4,096-byte attribute values. YAML limits structure to 100,000 nodes, depth 128 and 64 documents; all profiles retain file/fact/query budgets. Display truncation preserves the full original span. Platform qualification is recorded in the [v2 ledger](architecture-v2-ledger.md), independently of implementation.

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
