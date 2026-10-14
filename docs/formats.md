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
| DOCX | Main-body/table paragraphs, run/entity decoding, built-in heading styles and shared prose candidates | Original paragraph block ordinal, no page-layout claim |
| PPTX | OOXML slide text, title placeholders and shared prose candidates; relationship order | Original presentation-order slide number; title context isolated per slide |

PDF scans, image-only slides, OCR, encrypted documents requiring passwords, legacy DOC/PPT/XLS, XLSX, ODT, EPUB and RTF decoding are not currently implemented. DOCX headers/footers/comments/embedded objects and PPTX notes/charts/image contents are not extracted. SVG path geometry is retrievable source but is not interpreted as visual meaning. Titles/descriptions remain searchable XML source text.

OOXML entries are read in memory, not unpacked to the filesystem. External slide relationships and XML document type declarations are refused. Extraction skips are explicit, rather than replacement-character text pretending to be evidence.

[Document indexing](document-indexing.md) specifies heading context, requirement IDs/modality, written numeric bounds/units, qualitative/design/feature candidates, date ambiguity and category-filtered retrieval. These cues retain source evidence rather than asserting compliance or semantic understanding.

## Bootstrap limits

- Source file: 1 MiB; extracted text/XML part: 4 MiB.
- Corpus: 64 MiB of source processed under the scan limit, 10,000 indexed files.
- PDF/PPTX: 512 pages/slides; OOXML archive: 10,000 entries/paragraphs.
- Native syntax: 500,000 visited nodes and 10,000 facts per file.
- Response: 1–100 results and up to 65,536 payload bytes, metadata additional.

These are scaffold bounds, not the final large-repository design. PDF output limits are checked after library extraction, so they do not bound its peak memory. Use trusted local inputs until worker/resource isolation is qualified.
