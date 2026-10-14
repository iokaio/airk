# V2 adapter and dependency baseline

Phase 0 inventory, 14 October 2026. Prerequisite: schema 3 and `native-facts/7`. This records inspected repository behavior, not new language qualification. All 58 v2 adapter rows remain pending; no new interfaces or dependencies are introduced.

## Source evidence

**S** is [syntax.rs](../crates/airk-ingest/src/syntax.rs): `profile` is the exhaustive grammar selection table; `analyze`, `call_kind`, `call_target`, `code_evidence` and `symbol_id` define bounded facts and identities. **D** is [documents.rs](../crates/airk-ingest/src/documents.rs): `extract` is the exhaustive document/format dispatch; `xml_facts`, `line_facts` and `delimited_facts` supply structural adapters. **P** is [prose.rs](../crates/airk-ingest/src/prose.rs), [tables.rs](../crates/airk-ingest/src/tables.rs) and [spreadsheets.rs](../crates/airk-ingest/src/spreadsheets.rs).

[lib.rs](../crates/airk-ingest/src/lib.rs) reads/hashes sources, reuses same-path hashes under the global extractor version, and partitions source text into 40-line chunks. [inventory.rs](../crates/airk-ingest/src/inventory.rs) owns coverage discovery. Current selection lowercases ordinary extensions; there are no effective profile overrides, path-free native payloads, composite maps or continuity handles. Syntax IDs contain path, byte bounds and node kind. Code producer metadata uses `native-facts/4`, separately from global extraction cache version `native-facts/7`; migration must preserve or explicitly version both identities.

## Existing adapters to preserve

There are 23 grammar branches from 22 grammar packages: TS and TSX are separate languages in one package. [Formats](formats.md) remains the public selected-fact/suffix matrix.

| Family | Current implementation | Boundary |
|---|---|---|
| Rust/Python/JS/JSX/TS/TSX/Go/Java | S: declarations, imports, selected calls | Scope/signature/span/owner evidence, no compiler resolution |
| C/C++/C#/Ruby/PHP | S: selected declarations/imports/calls | Ruby import list empty; header dispatch and PHP/Ruby repairs remain |
| Bash/PowerShell/Lua/Swift/Kotlin | S: selected declarations/imports/calls | Zsh/dialect completeness not implied |
| SQL | S: CREATE table/view/function/index | No general query/dialect qualification |
| JSON/JSONC/YAML/TOML | S: local keys/tables | No schema overlays; JSONC can be partial |
| HTML/Vue/Svelte; CSS/SCSS | S: start tags or CSS rule sets | No dedicated component/SCSS semantics or embedded maps |
| Markdown/MDX; plain text | D/P: headings, requirements, dates, quantities, design cues | Source lines, bounded candidates, Markdown code exclusions |
| CSV/TSV | D: records/headers with consistency checks | Header assumption, source lines |
| XML/SVG/XSD/XSL/csproj/props/targets | D: elements and selected attributes | No external entities/resources/rendering/build evaluation |
| Dockerfile/INI/cfg/conf/properties | D: instruction or section/key lines | No multiline build model or universal config dialect |
| PDF | D: native text-layer pages | Output caps follow library allocation; no implicit OCR |
| DOCX | D/P: paragraphs/headings and table rows/cells/merges | Original blocks/table rows, no page layout/header/footer |
| XLSX | P: stored cells/formulas/results/strings/rows/merges | Original sheet/address; no calculation or sparse expansion |
| PPTX | D: presentation-order slides/title context | No notes/charts/image extraction |
| Other UTF-8 | D/S/P: lexical chunks and conservative prose | Not language admission; binary/NUL/invalid UTF-8 rejected |

[semantic.rs](../crates/airk-ingest/src/semantic.rs), [providers.rs](../crates/airk-ingest/src/providers.rs), [providers_lsp.rs](../crates/airk-ingest/src/providers_lsp.rs), [provider_process.rs](../crates/airk-ingest/src/provider_process.rs) and [ocr.rs](../crates/airk-ingest/src/ocr.rs) retain separate explicit SCIP/LSP/Ctags/OCR producer domains and source/build/model binding. They are not native resolution or automatic fallback.

Syntax visits at most 500,000 nodes and emits at most 10,000 facts per file. Error-containing declarations are suppressed; tree errors report partial syntax. Preserve all [ingestion](ingestion-limits.md) and [format](formats.md) limits, including 4 MiB extracted text, document-unit caps, per-file omission versus pass failure, and original page/block/sheet/slide locations. New child regions must share host budgets.

## Dependency and legal baseline

[Ingest Cargo.toml](../crates/airk-ingest/Cargo.toml) declares direct pins; [Cargo.lock](../Cargo.lock) records exact transitive versions/checksums. Native grammars compile into the binary; there are no runtime grammar downloads.

| Dependency | Direct pin/constraint |
|---|---|
| tree-sitter | =0.25.10 |
| tree-sitter-rust / python / javascript | =0.24.2 / =0.25.0 / =0.25.0 |
| tree-sitter-typescript / go / java | =0.23.2 / =0.25.0 / =0.23.5 |
| tree-sitter-c / cpp / c-sharp | =0.24.2 / =0.23.4 / =0.23.5 |
| tree-sitter-ruby / php / bash | =0.23.1 / =0.25.1 / =0.25.1 |
| tree-sitter-powershell / lua / swift | =0.26.4 / =0.5.0 / =0.7.4 |
| tree-sitter-kotlin-ng / json / html | =1.1.0 / =0.24.8 / =0.23.2 |
| tree-sitter-css / yaml / toml-ng / sequel | =0.25.0 / =0.7.2 / =0.7.0 / =0.3.11 |
| quick-xml / zip | 0.39 / 4, ZIP deflate only |
| pdf-extract / csv / pulldown-cmark / regex | =0.12.1 / 1 / 0.10 / =1.13.1 |
| ignore / sha2 / serde / serde_json / anyhow | Workspace constraints: discovery, identity, serialization, errors |
| scip / protobuf / url / tempfile | Workspace constraints: producer formats/URIs/process staging |
| libc / windows-sys | Workspace target constraints: native process isolation/Windows Job Objects |

[Third-party notices](../THIRD_PARTY_NOTICES.md), [pinned legal sources](../third_party/licenses/sources.json) and [license-notices.py](../scripts/license-notices.py) record package/version/license/source evidence and copied legal texts/SBOM. PDF-Extract, Adobe-CMap-Parser, Type1-Encoding-Parser and htmlescape remain flagged for release review because separate legal texts are unavailable; do not invent replacements. Existing legal records are not release approval.

No new dependency is selected here. Each new grammar needs primary upstream/license/scanner/build evidence, exact pin, Tree-sitter ABI admission and native architecture tests under plan section 5.4. Existing runtime-loading tests do not establish every architecture or dialect. No maintenance-status or upstream-compatibility claim is inferred without that work.

## Original fixtures and verification

Existing original fixtures are inline; preserve them for differential migration rather than duplicating unexecuted data:

- S: `common_language_definitions_have_real_native_grammar_fixtures`, `call_profiles_extract_written_targets_across_common_languages`, `structured_profiles_extract_keys_and_source_spans`, `qualified_scopes_signatures_calls_and_framework_patterns_remain_syntax`, annotation and error tests.
- D: `structured_documents_and_svg_have_native_facts`, `structured_header_locations_and_configuration_limits`, `malformed_documents_are_not_lossily_indexed`, PDF page, DOCX block/table and PPTX order fixtures.
- P: Markdown literal/code/header negatives; dates/quantities/Unicode bounds; XLSX malformed paths/packages, sparse/merged/formula/date/shared-string bounds; DOCX CDATA/XML/header bounds.

Integrator's local phase 0 receipt: ingest tests 46 passed, one ignored subprocess fixture; workspace build, formatting and clippy passed. The worker's documentation checks verify all 58 unique manifest IDs against the plan, local links and whitespace. Full workspace/platform/package receipts remain integrator-owned; this inventory does not claim v2 qualification on Windows, macOS or Linux.

Next: freeze profiles/maps/budgets and identity contracts in phase 1; preserve fixture expectations during phase 2 path-free conversion. New adapter activation remains gated on phase 5. Each row also requires profile invalidation/reuse, continuity, index/store/query, budgets and packaging evidence; fallback never completes a row.

## Complete expansion manifest baseline

S/D/P refer to exhaustive source dispatch above. Fallback means no selected dedicated adapter, not that arbitrary contents parse successfully. All rows have v2 status `pending`.

| ID | Phase | Language/format | Observed baseline | Missing row work / next action |
|---|---|---|---|---|
| L1.01 | 7 | C/C++ `.h` | S: C for .h, C++ explicit suffixes | add configured dialect/context and ambiguity; qualification pending. |
| L1.02 | 7 | PHP `.php` | S: declarations, imports and call nodes | qualify namespaces/receiver/static spellings; qualification pending. |
| L1.03 | 7 | Ruby `.rb` | S: classes/modules/methods and calls | add require/include/extend and mixin relationships; qualification pending. |
| L1.04 | 7 | SQL `.sql` | S: four CREATE forms | add queries/CTEs/joins/aliases and qualified dialects; qualification pending. |
| L2.01 | 6 | Terraform/HCL `.tf`, `.tfvars`, `.hcl` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L2.02 | 6 | Dockerfile, `Dockerfile.*`, `.dockerfile` | D: Dockerfile instruction lines | add stages/dependencies/multiline spans; qualification pending. |
| L2.03 | 6 | Protocol Buffers `.proto` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L2.04 | 6 | GraphQL `.graphql`, `.gql` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L2.05 | 6 | CMake `CMakeLists.txt`, `.cmake` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L2.06 | 6 | Make `Makefile`, `.mk` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L2.07 | 6 | MSBuild/.NET `.csproj`, `.fsproj`, `.vbproj`, `.props`, `.targets`, `.sln`, `.slnx` | D: XML on csproj/props/targets only | add other suffixes/solutions and MSBuild overlay; qualification pending. |
| L2.08 | 6 | GitHub Actions `.github/workflows/*.yml`, `*.yaml` | S: YAML keys | add path-selected Actions overlay; qualification pending. |
| L2.09 | 6 | Compose `compose.yaml`, `compose.yml`, `docker-compose.yml`, `docker-compose.yaml` | S: YAML keys | add Compose basename/schema overlay; qualification pending. |
| L2.10 | 7 | Bazel/Starlark `BUILD`, `BUILD.bazel`, `.bzl` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L2.11 | 7 | Gradle Kotlin DSL `.gradle.kts` | S: Kotlin on kts | add distinct Gradle profile/project facts; qualification pending. |
| L2.12 | 7 | Kubernetes recognized YAML documents | S: YAML keys | add Kubernetes discriminator and multi-document overlay; qualification pending. |
| L2.13 | 7 | OpenAPI recognized YAML/JSON | S: JSON/YAML keys | add OpenAPI discriminator and bounded refs; qualification pending. |
| L2.14 | 7 | GitLab CI `.gitlab-ci.yml` | S: YAML keys | add GitLab basename overlay; qualification pending. |
| L2.15 | 7 | `AGENTS.md`, `CLAUDE.md`, `.github/copilot-instructions.md` | D/P: Markdown/prose facts | add explicit path scope and literal constraints; qualification pending. |
| L2.16 | 7 | Selected JSON/YAML/TOML manifests | S: JSON/JSONC/YAML/TOML keys | add finite manifest schema registry; qualification pending. |
| L3.01 | 8 | HTML `.html`, `.htm` | S: HTML tags | add mapped supported script/style regions; qualification pending. |
| L3.02 | 8 | Vue `.vue` | S: HTML tags | add Vue host and mapped children; qualification pending. |
| L3.03 | 8 | Svelte `.svelte` | S: HTML tags | add Svelte host and mapped children; qualification pending. |
| L3.04 | 8 | SCSS `.scss` | S: CSS rules | admit SCSS grammar and Sass facts; qualification pending. |
| L3.05 | 8 | Astro `.astro` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L3.06 | 8 | Razor/Blazor `.razor`, `.cshtml` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L3.07 | 8 | MDX `.mdx` | D/P: Markdown prose | add MDX host/component/expression maps; qualification pending. |
| L3.08 | 8 | Jupyter `.ipynb` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L4.01 | 9 | Dart `.dart` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L4.02 | 9 | Scala `.scala`, `.sc` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L4.03 | 9 | Elixir `.ex`, `.exs` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L4.04 | 9 | Objective-C/Objective-C++ `.m`, `.mm` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L4.05 | 10 | R `.r`, `.R`; R Markdown `.Rmd` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L4.06 | 10 | Perl `.pl`, `.pm`, `.t` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L4.07 | 10 | Groovy `.groovy`; Gradle `.gradle` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L4.08 | 10 | Nix `.nix` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L4.09 | 10 | Zig `.zig` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L4.10 | 10 | Julia `.jl` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L4.11 | 10 | Clojure `.clj`, `.cljs`, `.cljc`; EDN `.edn` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L4.12 | 10 | OCaml `.ml`, `.mli` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L4.13 | 10 | Haskell `.hs`, `.lhs` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L4.14 | 10 | Erlang `.erl`, `.hrl` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L4.15 | 10 | F# `.fs`, `.fsi`, `.fsx` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L5.01 | 11 | Fortran `.f`, `.f90`, `.f95` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L5.02 | 11 | COBOL `.cob`, `.cbl`, `.cpy` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L5.03 | 11 | Visual Basic .NET `.vb` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L5.04 | 11 | Delphi/Object Pascal `.pas`, `.dpr`, `.dpk` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L5.05 | 11 | PL/SQL `.pks`, `.pkb`, `.pls`, configured `.sql` | S: SQL CREATE subset on sql, fallback on pks/pkb/pls | admit PL/SQL selection and facts; qualification pending. |
| L5.06 | 11 | T-SQL configured/qualified `.sql` | S: SQL CREATE subset | admit explicit T-SQL profile and facts; qualification pending. |
| L5.07 | 11 | ABAP `.abap` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L5.08 | 11 | Apex `.cls`, `.trigger` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L5.09 | 11 | Solidity `.sol` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L5.10 | 11 | VHDL `.vhd`, `.vhdl` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L5.11 | 11 | Verilog/SystemVerilog `.v`, `.sv`, `.svh` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L5.12 | 11 | CUDA `.cu`, `.cuh` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L5.13 | 11 | WGSL `.wgsl` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L5.14 | 11 | GLSL `.vert`, `.frag`, `.glsl` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
| L5.15 | 11 | Assembly `.asm`, `.s`, `.S` | D/S: UTF-8 lexical/prose fallback, no dedicated adapter | admit implementation/dependency and deliver full manifest row; qualification pending. |
