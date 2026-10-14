# Architecture v2 execution ledger

Plan: [architecture-v2.md](architecture-v2.md). Baseline revision: `4cbe157468e47990280f296836d4e0982b9fa16b`; branch: `feat/architecturev2`. Started 14 October 2026. This ledger records implementation separately from qualification; pending rows make no support claim.

Status vocabulary: `pending`, `in_progress`, `implemented`, `qualified`, `blocked`. Every row inherits the baseline prerequisite until its numbered gate supplies a frozen interface revision. Platform qualification is Windows-only when explicitly recorded; macOS/Linux and installed real-client trials remain unperformed. Phase 0 baseline gate passed on Windows. No v2 runtime behavior is enabled yet.

## Requirements

| ID | Scope | Status | Owner | Prerequisite / next action | Changed files / receipts | Platforms / outstanding failures |
|---|---|---|---|---|---|---|
| A1 | Versioned adapter registry, effective configuration identities, source maps and explicit capabilities | pending | integrator | Phase 0, then owning gate | None | Unqualified; implementation pending |
| A2 | Path-free extraction, bounded chunks, code/document continuity descriptors and content reuse | pending | integrator | Phase 0, then owning gate | None | Unqualified; implementation pending |
| C1 | Collection incarnation, exact publication tokens, monotonic lineage and per-dimension change stamps | pending | integrator | Phase 0, then owning gate | None | Unqualified; implementation pending |
| C2 | Bounded observed-change window, gaps, conservative path resolution and indexed metadata | pending | integrator | Phase 0, then owning gate | None | Unqualified; implementation pending |
| C3 | `airk_changes`, remembered-evidence validation, conditional reads and freshness diagnostics | pending | integrator | Phase 0, then owning gate | None | Unqualified; implementation pending |
| R1 | Bounded searchable retention, recoverable cleanup, quota retry and visible protected state | pending | integrator | Phase 0, then owning gate | None | Unqualified; implementation pending |
| R2 | Optional short historical content, checkpoints, exact historical reads and logical purge | pending | integrator | Phase 0, then owning gate | None | Unqualified; implementation pending |
| O1 | Verified embedding reuse and stable chunking; collection-local shared model objects | pending | integrator | Phase 0, then owning gate | None | Unqualified; implementation pending |
| O2 | Qualified incremental catalog/lexical builds or a recorded measured decision retaining full builds | pending | integrator | Phase 0, then owning gate | None | Unqualified; implementation pending |
| Q1 | Migration, fault, isolation, performance, actual agent-task and native-platform evidence | pending | integrator | Phase 0, then owning gate | None | Unqualified; implementation pending |

## Adapter rows

All rows require section 15 acceptance, including profile/move reuse, original citations, resource failures and end-to-end persistence/query evidence. Assigned adapter workers become owners only through a bounded work order; the integrator owns their admission and shared wiring.

| ID | Adapter | Status | Owner | Prerequisite / next action | Changed files / receipts | Platforms / outstanding failures |
|---|---|---|---|---|---|---|
| L1.01 | C/C++ `.h` | pending | integrator / adapter worker TBD | Gate 7; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L1.02 | PHP `.php` | pending | integrator / adapter worker TBD | Gate 7; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L1.03 | Ruby `.rb` | pending | integrator / adapter worker TBD | Gate 7; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L1.04 | SQL `.sql` | pending | integrator / adapter worker TBD | Gate 7; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.01 | Terraform/HCL `.tf`, `.tfvars`, `.hcl` | pending | integrator / adapter worker TBD | Gate 6; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.02 | Dockerfile, `Dockerfile.*`, `.dockerfile` | pending | integrator / adapter worker TBD | Gate 6; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.03 | Protocol Buffers `.proto` | pending | integrator / adapter worker TBD | Gate 6; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.04 | GraphQL `.graphql`, `.gql` | pending | integrator / adapter worker TBD | Gate 6; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.05 | CMake `CMakeLists.txt`, `.cmake` | pending | integrator / adapter worker TBD | Gate 6; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.06 | Make `Makefile`, `.mk` | pending | integrator / adapter worker TBD | Gate 6; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.07 | MSBuild/.NET `.csproj`, `.fsproj`, `.vbproj`, `.props`, `.targets`, `.sln`, `.slnx` | pending | integrator / adapter worker TBD | Gate 6; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.08 | GitHub Actions `.github/workflows/*.yml`, `*.yaml` | pending | integrator / adapter worker TBD | Gate 6; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.09 | Compose `compose.yaml`, `compose.yml`, `docker-compose.yml`, `docker-compose.yaml` | pending | integrator / adapter worker TBD | Gate 6; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.10 | Bazel/Starlark `BUILD`, `BUILD.bazel`, `.bzl` | pending | integrator / adapter worker TBD | Gate 7; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.11 | Gradle Kotlin DSL `.gradle.kts` | pending | integrator / adapter worker TBD | Gate 7; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.12 | Kubernetes recognized YAML documents | pending | integrator / adapter worker TBD | Gate 7; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.13 | OpenAPI recognized YAML/JSON | pending | integrator / adapter worker TBD | Gate 7; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.14 | GitLab CI `.gitlab-ci.yml` | pending | integrator / adapter worker TBD | Gate 7; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.15 | `AGENTS.md`, `CLAUDE.md`, `.github/copilot-instructions.md` | pending | integrator / adapter worker TBD | Gate 7; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L2.16 | Selected JSON/YAML/TOML manifests | pending | integrator / adapter worker TBD | Gate 7; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L3.01 | HTML `.html`, `.htm` | pending | integrator / adapter worker TBD | Gate 8; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L3.02 | Vue `.vue` | pending | integrator / adapter worker TBD | Gate 8; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L3.03 | Svelte `.svelte` | pending | integrator / adapter worker TBD | Gate 8; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L3.04 | SCSS `.scss` | pending | integrator / adapter worker TBD | Gate 8; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L3.05 | Astro `.astro` | pending | integrator / adapter worker TBD | Gate 8; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L3.06 | Razor/Blazor `.razor`, `.cshtml` | pending | integrator / adapter worker TBD | Gate 8; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L3.07 | MDX `.mdx` | pending | integrator / adapter worker TBD | Gate 8; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L3.08 | Jupyter `.ipynb` | pending | integrator / adapter worker TBD | Gate 8; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L4.01 | Dart `.dart` | pending | integrator / adapter worker TBD | Gate 9; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L4.02 | Scala `.scala`, `.sc` | pending | integrator / adapter worker TBD | Gate 9; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L4.03 | Elixir `.ex`, `.exs` | pending | integrator / adapter worker TBD | Gate 9; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L4.04 | Objective-C/Objective-C++ `.m`, `.mm` | pending | integrator / adapter worker TBD | Gate 9; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L4.05 | R `.r`, `.R`; R Markdown `.Rmd` | pending | integrator / adapter worker TBD | Gate 10; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L4.06 | Perl `.pl`, `.pm`, `.t` | pending | integrator / adapter worker TBD | Gate 10; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L4.07 | Groovy `.groovy`; Gradle `.gradle` | pending | integrator / adapter worker TBD | Gate 10; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L4.08 | Nix `.nix` | pending | integrator / adapter worker TBD | Gate 10; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L4.09 | Zig `.zig` | pending | integrator / adapter worker TBD | Gate 10; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L4.10 | Julia `.jl` | pending | integrator / adapter worker TBD | Gate 10; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L4.11 | Clojure `.clj`, `.cljs`, `.cljc`; EDN `.edn` | pending | integrator / adapter worker TBD | Gate 10; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L4.12 | OCaml `.ml`, `.mli` | pending | integrator / adapter worker TBD | Gate 10; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L4.13 | Haskell `.hs`, `.lhs` | pending | integrator / adapter worker TBD | Gate 10; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L4.14 | Erlang `.erl`, `.hrl` | pending | integrator / adapter worker TBD | Gate 10; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L4.15 | F# `.fs`, `.fsi`, `.fsx` | pending | integrator / adapter worker TBD | Gate 10; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L5.01 | Fortran `.f`, `.f90`, `.f95` | pending | integrator / adapter worker TBD | Gate 11; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L5.02 | COBOL `.cob`, `.cbl`, `.cpy` | pending | integrator / adapter worker TBD | Gate 11; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L5.03 | Visual Basic .NET `.vb` | pending | integrator / adapter worker TBD | Gate 11; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L5.04 | Delphi/Object Pascal `.pas`, `.dpr`, `.dpk` | pending | integrator / adapter worker TBD | Gate 11; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L5.05 | PL/SQL `.pks`, `.pkb`, `.pls`, configured `.sql` | pending | integrator / adapter worker TBD | Gate 11; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L5.06 | T-SQL configured/qualified `.sql` | pending | integrator / adapter worker TBD | Gate 11; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L5.07 | ABAP `.abap` | pending | integrator / adapter worker TBD | Gate 11; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L5.08 | Apex `.cls`, `.trigger` | pending | integrator / adapter worker TBD | Gate 11; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L5.09 | Solidity `.sol` | pending | integrator / adapter worker TBD | Gate 11; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L5.10 | VHDL `.vhd`, `.vhdl` | pending | integrator / adapter worker TBD | Gate 11; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L5.11 | Verilog/SystemVerilog `.v`, `.sv`, `.svh` | pending | integrator / adapter worker TBD | Gate 11; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L5.12 | CUDA `.cu`, `.cuh` | pending | integrator / adapter worker TBD | Gate 11; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L5.13 | WGSL `.wgsl` | pending | integrator / adapter worker TBD | Gate 11; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L5.14 | GLSL `.vert`, `.frag`, `.glsl` | pending | integrator / adapter worker TBD | Gate 11; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |
| L5.15 | Assembly `.asm`, `.s`, `.S` | pending | integrator / adapter worker TBD | Gate 11; qualify dependency and original fixtures | None | No v2 dialect/platform qualification |

## Integration gates

| ID | Scope | Status | Owner | Prerequisite / next action | Changed files / receipts | Platforms / outstanding failures |
|---|---|---|---|---|---|---|
| P0 | baseline and work ledger | qualified | integrator + inventory/benchmark workers | Baseline 4cbe157; proceed to contract gate | Adapter/lifecycle inventories, benchmark harness and Windows cold/warm/move receipt linked below; full baseline checks passed | Windows baseline; other native platforms unqualified; supplemental 200-edit run pending |
| P1 | shared contracts and harness | pending | integrator | Phase 0; implement exit gate | None yet | Native Windows available; other platforms unqualified |
| P2 | publication/storage and existing adapters | pending | integrator | Phase 1; implement exit gate | None yet | Native Windows available; other platforms unqualified |
| P3 | file changes and API | pending | integrator | Phase 2; implement exit gate | None yet | Native Windows available; other platforms unqualified |
| P4 | searchable retention | pending | integrator | Phase 3; implement exit gate | None yet | Native Windows available; other platforms unqualified |
| P5 | development continuity and reuse | pending | integrator | Phase 4; implement exit gate | None yet | Native Windows available; other platforms unqualified |
| P6 | repository essentials | pending | integrator | Phase 5; implement exit gate | None yet | Native Windows available; other platforms unqualified |
| P7 | project overlays and repairs | pending | integrator | Phase 6; implement exit gate | None yet | Native Windows available; other platforms unqualified |
| P8 | composite web/notebooks | pending | integrator | Phase 7; implement exit gate | None yet | Native Windows available; other platforms unqualified |
| P9 | general languages A | pending | integrator | Phase 8; implement exit gate | None yet | Native Windows available; other platforms unqualified |
| P10 | general languages B | pending | integrator | Phase 9; implement exit gate | None yet | Native Windows available; other platforms unqualified |
| P11 | enterprise/specialists | pending | integrator | Phase 10; implement exit gate | None yet | Native Windows available; other platforms unqualified |
| P12 | integration and qualification | pending | integrator | Phase 11; implement exit gate | None yet | Native Windows available; other platforms unqualified |

## Cross-phase packages

| ID | Scope | Status | Owner | Prerequisite / next action | Changed files / receipts | Platforms / outstanding failures |
|---|---|---|---|---|---|---|
| H1 | historical storage | pending | integrator / worker TBD | After 5 / by 8; freeze prerequisite contracts | None | Unqualified |
| H2 | historical API/controls | pending | integrator / worker TBD | After H1 / by 8; freeze prerequisite contracts | None | Unqualified |
| O1b | shared model ownership | pending | integrator / worker TBD | After 5 and verified object-reference contract / by 8; freeze prerequisite contracts | None | Unqualified |
| O2a | incremental catalog | pending | integrator / worker TBD | After 7 and stable catalog schema / by 11; freeze prerequisite contracts | None | Unqualified |
| O2b | incremental lexical | pending | integrator / worker TBD | After O2a's stable changed-file/input contract / by 11; freeze prerequisite contracts | None | Unqualified |
| F | similarity and directory candidates | pending | integrator / worker TBD | After 5 / by 11; freeze prerequisite contracts | None | Unqualified |

## Evidence records

- [Adapter and dependency inventory](v2-adapter-inventory.md)
- [Publication/lifecycle inventory](v2-lifecycle-inventory.md)
- [Original-fixture measurements](v2-baseline-measurements.md)

## Decisions and handoffs

- D0: Preserve the current schema 3 / native-facts/7 runtime during baseline measurement. Phase 1 must freeze independently versioned contracts before phase 2 migration changes. No scope item is removed.
- Phase 0 file ownership: adapter worker owns `docs/v2-adapter-inventory.md`; lifecycle worker owns `docs/v2-lifecycle-inventory.md`; benchmark worker owns `scripts/benchmark-v2.py`, its test module and `docs/v2-baseline-measurements.md`; integrator owns this ledger and shared wiring. No concurrent shared-module edits.
- Existing user changes at start: `docs/README.md`, `docs/architecture-v2.md`, `docs/change-tracing-plan.md`. Preserve them; stage implementation files explicitly.

## Local baseline checks

- Passed: locked workspace build, formatting, warnings-denied workspace clippy, initial repository and tracked whitespace checks. Locked workspace tests passed (two intentional ignored fixtures: subprocess helper and explicitly provisioned ONNX model). Python packaging contracts passed: 39 tests, one Linux-only skip.
- Python WindowsApps alias and default `py` selection cannot launch in sandbox. Explicit installed Python 3.13.12 works; use `C:/Users/tdotn/AppData/Local/Programs/Python/Python313/python.exe`. Rust 1.98.0 and Node 24.15.0 are available.
- License bundling generated notices for 430 packages; existing separate-license-text review warnings remain for adobe-cmap-parser, htmlescape, pdf-extract and type1-encoding-parser.
- Docker daemon access required sandbox escalation and then worked (29.8.0). Test-stage and runtime image builds passed using cached build/test layers; isolated smoke passed: stdio MCP, two isolated collections, offline/read-only runtime, persistent restart, notices, usage and safe removal. Native macOS/Linux, installed helper/client trials and power-loss qualification are unavailable on this Windows host.

Phase 0 gate: all 58 adapter rows, 10 system requirements, 13 gates and 6 cross-phase packages are recorded. Original executable inline fixtures cover the current adapters; the new benchmark generates only original sources. The completed 302-file cold/warm/move baseline satisfies this gate. The optional supplemental 200-edit baseline remains running; phase 4 plateau qualification is still pending. Seven focused benchmark tests passed. The plan itself is committed unchanged as the execution specification; pre-existing documentation-index and change-tracing-plan edits remain outside this commit.
