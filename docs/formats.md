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
| Ruby | rb | Classes/modules/methods, literal require/include/extend, written mixins and receiver calls; dynamic dispatch remains unknown |
| PHP | php | Classes/interfaces/traits/functions/methods/namespaces, imports/trait use and written receiver/member/static call spellings |
| Bash-family | sh, bash, zsh | Bash grammar function definitions; Zsh-specific constructs may be partial |
| PowerShell | ps1, psm1, psd1 | Selected function/class and using statements |
| Lua | lua | Function declarations; dynamic require resolution unavailable |
| Swift | swift | Selected function/class/protocol/typealias and import declarations |
| Kotlin | kt, kts | Selected function/class/object/typealias and import declarations |
| Dart | dart | Libraries/imports/parts, classes/mixins/extensions, selected members and written calls |
| Scala | scala, sc | Packages/imports, classes/objects/traits, defs/vals/types and selected calls; braced and selected Scala 3 forms |
| Elixir | ex, exs | Structural literal module/function/macro heads, written arity/guards, aliases/imports/uses/requires and calls; quoted macro bodies excluded |
| Objective-C / Objective-C++ | qualified m; mm | Interfaces/implementations/protocols/categories, selectors/properties/imports and messages; mixed C++ is explicitly partial |
| R | r, R | Written assignments/function bindings, selected calls and literal package-load candidates; no R evaluation |
| Perl | qualified pl, pm, t | Packages, named subroutines/methods, use/literal require and selected calls; ambiguous suffixes require a strong discriminator or policy |
| Groovy | groovy | Packages/imports, classes/methods, selected closures and written calls |
| Nix | nix | Attribute bindings, named lambdas, applications and written import arguments; no evaluation |
| Zig | zig | Functions, container declarations, variables, tests and written builtins/imports; no comptime execution |
| Julia | jl | Modules, functions/types/macros, selected short methods, using/import/export and calls; no macro expansion or dispatch resolution |
| Clojure | clj, cljs, cljc | Structural namespace/require and written definition/call forms; reader-conditional feature context retained without branch selection |
| OCaml | ml, mli | Modules/module types/types/values/functions/exceptions/open/include; separate interface grammar and written signatures |
| Haskell | hs | Modules/imports/signatures/data/type aliases/classes/instances/bindings; no typeclass resolution |
| Erlang | erl, hrl | Modules/exports/imports/includes/specs/records/functions and written arities/calls/macros; no preprocessor expansion |
| F# | fs, fsi, fsx | Namespaces/modules/open/types/members/let bindings, separate signature grammar and written script directives |
| SQL | sql; explicit `sql-postgresql` / `sql-mysql` overrides | SELECT/query scopes, CTEs, joins, aliases, written references and selected DDL; dialect semantics are not validated |

These are bounded grammar profiles, not complete compiler/type-system semantics. Declarations containing grammar errors are suppressed and file_info reports partial-syntax. Macros, overloaded/dynamic names and cross-file references remain unresolved. A `.h` file defaults to C; explicit overrides or qualified [captured header context](extraction-policy.md#captured-header-context) can select C++. Grammar-recognized syntax does not guarantee that every declaration has a named fact extractor.

P10 focused qualification passed 18 adapter unit tests and native grammar builds on Windows, and 19 kernel pipeline cases on Linux. Those cases cover original retained bytes, warm and compatible-move reuse, bounded key queries where native continuity is qualified, and profile changes without unrelated-file reparsing. Final P12 platform qualification remains pending. Clojure/EDN and Gradle structural cues do not imply native declaration continuity; unknown body/documentation dimensions remain unknown. See the [first](v2-general-languages-b-first.md), [data](v2-general-languages-b-data.md), and [functional](v2-general-languages-b-functional.md) batch receipts for exact dependencies and limits.

Explicit PostgreSQL/MySQL profiles add separately labeled structural procedure evidence for selected complete headers and bodies. PostgreSQL accepts qualified SQL/PLpgSQL literal or dollar-quoted bodies; MySQL accepts selected nested BEGIN/END bodies with ordinary semicolon framing. Delimiter changes, unsupported control-flow and quote modes remain partial. Oracle and SQL Server written evidence requires the separate specialist profiles below; those also retain explicit procedural limits.

### Specialist source profiles

| Language / selection | Selected evidence and explicit limits |
|---|---|
| Fortran: `.f` fixed; `.f90`, `.f95` free; explicit source-form overrides | Programs/modules/subroutines/functions, use/include, declarations and calls; original source retained, unsupported fixed continuations/columns and conditional directives partial |
| COBOL: `.cob`, `.cbl`, `.cpy` default fixed; explicit `cobol-free` | Programs/divisions/sections/paragraphs/data/COPY/written CALL targets; fixed grammar and separate structural free-source adapter; no copybook expansion |
| VB.NET: `.vb` | Structural namespaces/imports/types/modules/Sub/Function/Property and receiver calls; line-form, attribute/XML and continuation limits; no compiler resolution or native continuity |
| Delphi/Object Pascal: `.pas`, `.dpr`, `.dpk` | Units/programs/types/routines/members/uses; separately labeled structural package/requires/contains; conditional branches not evaluated |
| PL/SQL: `.pks`, `.pkb`, `.pls`; explicit `plsql` for `.sql` | Package spec/body, procedure/function/trigger/cursor headers and selected SQL queries/references; procedural body semantics remain partial |
| T-SQL: explicit `tsql` for `.sql` | GO batch boundaries, procedure/function/trigger/table/view headers and selected query/CTE/join/alias facts; no SQLCMD or database execution |
| ABAP: `.abap` | Structural period-delimited report/program/class/interface/method/form/module/include and selected references; chained declarations/templates remain partial |
| Apex: `.trigger`; strong marker or explicit policy for `.cls` | Classes/interfaces/enums/methods/constructors/triggers, receiver calls and selected SOQL/SOSL references; no Salesforce schema resolution |
| Solidity: `.sol` | Contracts/interfaces/libraries/structs/functions/events/errors/imports and written inheritance/calls; Yul partial, no execution or security analysis |
| VHDL: `.vhd`, `.vhdl` | Entities/architectures/packages/components, named processes, ports/signals/library/use and instantiations; no elaboration/simulation |
| Verilog/SystemVerilog: qualified `.v`; `.sv`, `.svh` | Modules/interfaces/packages/classes/programs/functions/tasks/ports/signals/includes/instantiations; macros/directives/generate partial |
| CUDA: `.cu`, `.cuh` | Written C++ declarations, kernel qualifiers, launch bounds/attributes and kernel launch syntax; no NVCC, preprocessing evaluation or device execution |
| WGSL: `.wgsl` | Structs/functions/variables, entry-point stage attributes and written group/binding/location attributes; Bevy preprocessing/import extensions remain partial and unexpanded |
| GLSL: `.vert`, `.frag`, `.glsl` | Declarations, interface blocks, uniforms/storage qualifiers, layouts, functions and written directives; filename-bound vertex/fragment/unknown stage, no shader validation or preprocessing evaluation |
| Assembly: `.asm`, `.s`, `.S` | Labels/constants, directives/sections/linkage and written instructions/operands; default ISA unknown with partial coverage, explicit x86 AT&T/Intel or AArch64 policy profiles enable finite mnemonic classification; `.S` remains unexpanded |

Specialist dialect/source-form choices enter effective profile identity. Ambiguous `.cls` and `.v` use bounded visible markers or explicit overrides; unconfigured `.sql` retains generic SQL. Syntax and structural provenance are distinct, references remain written candidates, and source is never executed. See [business](v2-specialist-business.md), [vendor](v2-specialist-vendor.md), [hardware](v2-specialist-hardware.md) and [GPU/assembly](v2-specialist-gpu.md) receipts for native/integration evidence and remaining platform limits. These profiles do not imply universal dialect or toolchain support.

## Structured text and documents

| Format | Native extraction/facts | Citation and scope |
|---|---|---|
| Markdown | Heading hierarchy, tables, requirement/date/design/architecture/feature/decision candidates; lexical source | Source lines; code examples excluded from prose cues |
| MDX | Preserved Markdown prose plus bounded standalone ESM/JSX/expression islands | Original host spans and region-local code; mixed inline syntax remains partial |
| R Markdown `.Rmd` | Preserved prose and supported labeled code chunks under shared host budgets | Exact original raw body spans; no knitr execution; unsupported engines/indented transformations remain partial |
| Literate Haskell `.lhs` | Preserved prose and bounded Bird/LaTeX Haskell regions | Bird prefix gaps map to original bytes; CRLF retained, no generated wrappers; separate regions have independent parsing scopes |
| EDN `.edn` | Structural data maps and keys | Data list values never become executable Clojure declarations or calls; unsupported reader forms remain partial |
| JSON / JSONC | Tree-sitter object keys | Source lines; comments/nonstandard syntax may produce partial-syntax |
| YAML / TOML | Native mapping keys/tables | Source lines; key names are local, not fully qualified paths |
| HTML | Tags plus supported JS/TS and CSS/SCSS script/style blocks | Exact mapped child spans; linked resources are not fetched |
| Vue / Svelte | Component/directive/slot/prop/event and setup/module/style candidates plus mapped children | No template compiler, framework execution or resolved bindings |
| Astro | Leading TypeScript frontmatter and explicit script/style blocks, component candidates | Original spans; general template expressions remain partial |
| Razor / Blazor | Directives/routes/injection, component candidates and contiguous C# member blocks | Mapped original spans; mixed template blocks remain partial |
| Jupyter format 4 | Supported code and Markdown cell sources and stored cell metadata | Exact decoded JSON/cell locations; outputs/attachments excluded from retrieval |
| CSS | Selector facts and source retrieval | Source lines |
| SCSS | Selectors, variables, mixins/includes, functions, use/import/forward | Original spans; non-ASCII selectors/interpolation remain partial; no Sass evaluation |
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
| Bazel/Starlark: `BUILD`, `BUILD.bazel`, `.bzl` | Written loads, rules/targets, labels/dependencies and macros; bounded Python-grammar subset |
| Gradle Kotlin: `.gradle.kts` | Kotlin declarations plus selected plugins/tasks/modules/configurations/dependencies; arbitrary computed receivers remain unknown |
| Gradle Groovy: `.gradle` | Groovy declarations plus structural plugins/tasks/modules/configurations/dependencies; no build evaluation or dependency resolution |
| Kubernetes: recognized YAML documents | apiVersion/kind/metadata, resources, selectors and written references across bounded multiple documents |
| OpenAPI: recognized YAML/JSON | Paths/methods/operations/parameters/schema/request/response facts and reference spellings; bounded same-document traversal |
| GitLab: recognized `.gitlab-ci.yml` | Jobs/stages/needs/dependencies/artifacts/includes, without fetching includes |
| Finite manifests | `package.json`, `tsconfig*.json`, `composer.json`, `Cargo.toml`, `pyproject.toml`: written package/dependency/script/compiler/workspace/build/tool fields |
| Instruction Markdown | `AGENTS.md`, `CLAUDE.md`, source-root `.github/copilot-instructions.md`: Markdown evidence plus untrusted written constraints, inline-code path literals and filename-derived scope |

These profiles preserve original source locations and never evaluate builds, resolve references, expand variables or fetch dependencies. The four repository grammars expose conservative declaration continuity; CMake, Make, MSBuild, Actions and Compose expose structural-rule evidence without continuity. Explicit extraction-policy overrides take precedence, including for additional source roots.

Unsupported constructs remain explicit: Docker CRLF line continuations and Make custom recipe prefixes are partial syntax; malformed declarations are omitted. Workflow/Compose YAML aliases, anchors, tags, merges, duplicate keys and multiple documents produce diagnostics without expansion or key overwriting. MSBuild XML forbids DTDs and limits nesting to 128, events to 500,000 and attributes to 256 per element with 4,096-byte attribute values. YAML limits structure to 100,000 nodes, depth 128 and 64 documents; all profiles retain file/fact/query budgets. Display truncation preserves the full original span. Platform qualification is recorded in the [v2 ledger](architecture-v2-ledger.md), independently of implementation.

Project schema detection inspects at most 4,096 captured bytes and requires recognizable structure; incomplete or unknown schemas retain generic key extraction. Ordered JSON/JSONC/YAML/TOML views preserve duplicate evidence and source offsets. OpenAPI traversal is capped at 1,024 local reference edges, depth 16 and 100,000 steps; cycles, external/unresolved/unsupported encoded references are partial and never fetched. TOML array tables and unsupported multiline forms remain partial. Schema/parser flavor enters profile identity. Instruction documents never become an obedience engine: code blocks and HTML comments are excluded from new instruction cues, client precedence is not inferred, and literal paths are never followed. New instruction cues do not receive continuity keys; existing Markdown units retain conservative continuity.

OOXML entries are read in memory, not unpacked to the filesystem. External slide/worksheet relationships and XML document type declarations are refused; unused external links are never followed. Extraction skips are explicit, rather than replacement-character text pretending to be evidence. Spreadsheet formulas and cached results are labeled separately; caches can be stale or absent. AIRK does not launch Office, recalculate formulas or fetch linked workbooks.

[Document indexing](document-indexing.md) specifies heading context, requirement IDs/modality, written numeric bounds/units, qualitative/design/feature candidates, date ambiguity and category-filtered retrieval. These cues retain source evidence rather than asserting compliance or semantic understanding.

## Current safety limits

- Source file: configurable up to 256 MiB; extracted text/XML part: 32 MiB. UTF-8 sources above 4 MiB retain exact text with partial text-only status, without structural facts or continuity.
- Automatic corpus: one eighth of detected memory, capped at 4 GiB eligible source (512 MiB if unknown), twice that for extracted text and 100,000 files. [Persisted collection limits](ingestion-limits.md) permit explicit larger profiles within validated ceilings.
- Default index scan: 1,000,000 visited non-ignored entries (including directories) and four times the automatic eligible-source allowance for source reads, including unsupported files. Known oversized files are skipped before reading. Exhausting configured pass budgets preserves the previous generation.
- PDF/PPTX: 512 pages/slides; OOXML archive: 10,000 entries/paragraphs.
- Native syntax: 500,000 visited nodes and 10,000 facts per file.
- DOCX: 256 tables, 32 nested table levels, 256 cells per row; table rows count toward the document fact/text budgets.
- XLSX: 128 sheets, 10,000 rows per sheet, 100,000 stored cells/shared strings per workbook, 16 MiB of selected XML parts; sparse/merged ranges are never expanded. The shared 10,000-fact limit can be reached before the cell limit.
- Response: 1–100 results and up to 65,536 payload bytes, metadata additional.

These are retained extraction safety bounds pending large-repository qualification. PDF output limits are checked after library extraction, so they do not bound its peak memory. Use trusted local inputs until worker/resource isolation is qualified.
