# Architecture v2 execution ledger

Plan: [architecture-v2.md](architecture-v2.md). Baseline revision: `4cbe157468e47990280f296836d4e0982b9fa16b`; branch: `feat/architecturev2`. Started 14 October 2026. This ledger records implementation separately from qualification; pending rows make no support claim.

Status vocabulary: `pending`, `in_progress`, `implemented`, `qualified`, `blocked`. Every row inherits the baseline prerequisite until its numbered gate supplies a frozen interface revision. Qualification applies only to explicitly recorded environments; native macOS/Linux and installed real-client trials remain unperformed. Phase 0 baseline gate passed on Windows. Phase 1 contract gate passed on native Windows and in the Linux container; contract revision 1 is frozen. Phase 2 publication, migration and path-free extraction passed the native Windows and Linux-container integration gate. Detailed change reporting and subsequent runtime phases remain pending.

## Requirements

| ID | Scope | Status | Owner | Prerequisite / next action | Changed files / receipts | Platforms / outstanding failures |
|---|---|---|---|---|---|---|
| A1 | Versioned adapter registry, effective configuration identities, source maps and explicit capabilities | in_progress | integrator | Contract revision 1; integrate at owning phase | P1 contract modules; runtime implementation still pending | Unqualified runtime; contract checks passed; runtime implementation pending |
| A2 | Path-free extraction, bounded chunks, code/document continuity descriptors and content reuse | in_progress | integrator | Contract revision 1; integrate at owning phase | P1 contract modules; runtime implementation still pending | Unqualified runtime; contract checks passed; runtime implementation pending |
| C1 | Collection incarnation, exact publication tokens, monotonic lineage and per-dimension change stamps | in_progress | integrator | Contract revision 1; integrate at owning phase | P1 contract modules; runtime implementation still pending | Unqualified runtime; contract checks passed; runtime implementation pending |
| C2 | Bounded observed-change window, gaps, conservative path resolution and indexed metadata | in_progress | integrator | Contract revision 1; integrate at owning phase | P1 contract modules; runtime implementation still pending | Unqualified runtime; contract checks passed; runtime implementation pending |
| C3 | `airk_changes`, remembered-evidence validation, conditional reads and freshness diagnostics | in_progress | integrator | Contract revision 1; integrate at owning phase | P1 contract modules; runtime implementation still pending | Unqualified runtime; contract checks passed; runtime implementation pending |
| R1 | Bounded searchable retention, recoverable cleanup, quota retry and visible protected state | pending | integrator | Phase 0, then owning gate | None | Unqualified; implementation pending |
| R2 | Optional short historical content, checkpoints, exact historical reads and logical purge | pending | integrator | Phase 0, then owning gate | None | Unqualified; implementation pending |
| O1 | Verified embedding reuse and stable chunking; collection-local shared model objects | in_progress | integrator | Contract revision 1; integrate at owning phase | P1 contract modules; runtime implementation still pending | Unqualified runtime; contract checks passed; runtime implementation pending |
| O2 | Qualified incremental catalog/lexical builds or a recorded measured decision retaining full builds | pending | integrator | Phase 0, then owning gate | None | Unqualified; implementation pending |
| Q1 | Migration, fault, isolation, performance, actual agent-task and native-platform evidence | in_progress | integrator | Contract revision 1; integrate at owning phase | P1 contract modules; runtime implementation still pending | Unqualified runtime; contract checks passed; runtime implementation pending |

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
| P0 | baseline and work ledger | qualified | integrator + inventory/benchmark workers | Baseline 4cbe157; proceed to contract gate | Adapter/lifecycle inventories, benchmark harness and Windows cold/warm/move receipt linked below; full baseline checks passed | Windows baseline; other native platforms unqualified; supplemental 200-edit run passed |
| P1 | shared contracts and harness | qualified | integrator + contract workers | P0 c4a6804; frozen contract revision 1, proceed to P2 | Core identity/changes/extraction/location/embedding, ingest registry/policy, adversarial tests and contract decisions below | Runtime schema unchanged; Windows and Linux-container gate passed; macOS unperformed |
| P2 | publication/storage and existing adapters | qualified | integrator + adapter/store/retrieval workers | P1 435d6fc; publication contract D4 frozen, proceed to P3 | Authoritative inventory, path-free conversion, finalizer, receipts, indexed metadata and mixed lifecycle; final receipts below | Native Windows and Linux-container gate passed; macOS/mixed installed clients remain unqualified |
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
- Python WindowsApps alias and default `py` selection cannot launch in sandbox. Explicit installed Python 3.13.12 works; use `$env:LOCALAPPDATA/Programs/Python/Python313/python.exe`. Rust 1.98.0 and Node 24.15.0 are available.
- License bundling generated notices for 430 packages; existing separate-license-text review warnings remain for adobe-cmap-parser, htmlescape, pdf-extract and type1-encoding-parser.
- Docker daemon access required sandbox escalation and then worked (29.8.0). Test-stage and runtime image builds passed using cached build/test layers; isolated smoke passed: stdio MCP, two isolated collections, offline/read-only runtime, persistent restart, notices, usage and safe removal. Native macOS/Linux, installed helper/client trials and power-loss qualification are unavailable on this Windows host.

Phase 0 gate: all 58 adapter rows, 10 system requirements, 13 gates and 6 cross-phase packages are recorded. Original executable inline fixtures cover the current adapters; the new benchmark generates only original sources. The completed 302-file cold/warm/move baseline satisfies this gate. The supplemental 200-edit baseline completed successfully (retained logical state 182,926 to 12,257,049 bytes); phase 4 plateau qualification is still pending. Seven focused benchmark tests passed. The plan itself is committed unchanged as the execution specification; pre-existing documentation-index and change-tracing-plan edits remain outside this commit.

## Phase 1 decisions and integration receipt

- D1: [Identity, lineage and embedding contract revision 1](v2-identity-contract.md). Full canonical tokens and framed digests; older publication headers preserve age evidence; typed artifact/symbol/document/global subjects; independent cursor list positions. Existing runtime versions remain unchanged. Proposed migration versions are recorded explicitly, not activated.
- D2: [Extraction and profile contract revision 1](v2-extraction-contract.md). Preserve native code/document/table metadata and artifact-level OCR provenance in path-free payload contracts; effective configuration digests are typed. Registry additions do not invalidate unrelated profile identities. Root-wide/scoped/case-specific overrides are deterministic; conflicting decisions fail.
- D3: [Source-map and shared-budget contract revision 1](v2-location-contract.md). UTF-8 exact pieces, conservative decoded/unmapped regions, validated cell/region metadata, 16 MiB native ceiling and adapter-specific lower document allowance. No new composite format enabled.
- Two independent read-only contract audits found missing old publication timestamps, symbol/document event subjects, OCR artifact provenance and region metadata/profile validation. These blockers were corrected before freeze, with regression fixtures. One source-map test was corrected from the document-only 4 MiB ceiling to the existing native 16 MiB ceiling; document adapters must still select 4 MiB.
- Dependencies: core now uses already-pinned workspace sha2/serde_json; Cargo.lock only adds those dependency edges. No grammar/runtime/dependency version changed; no source/index state is included.
- Final integration gate passed. Formatting, warnings-denied workspace clippy, 46 Python packaging tests (one Linux-only skip), notice bundling, repository checks and whitespace checks have passed. The earlier Linux-container contract snapshot passed. An overlapping Windows test/build run reached a rustdoc failure while contract artifacts were being rebuilt; the final frozen-source build/test run passed and supersedes that run.
- Phase 2 ownership handoff after this gate: integrator owns core format activation/exports, common finalizer, retrieval publication integration, manifests and top-level docs; adapter worker owns new ingest compatibility/conversion modules and private fixtures; store worker owns store encoding/catalog/migration modules; independent fixture worker owns new migration/reopen/fault test files. Freeze exact allowed files in each work order before editing. No concurrent ownership of a shared module.
- Next concrete action: implement P2 common publication/storage migration and existing-adapter conversion. Phases 2–12 and all language/history/retention runtime requirements remain incomplete. No push or PR is permitted before the complete plan.

Migration-preparation review also found syntax owners can exist without emitted facts, and framework facts can share syntax IDs. P1 now has an independent typed syntax identity table with serialization/validation fixtures; emitted fact IDs remain distinct. This preserves existing evidence without phantom facts or string-prefix rewriting. Local commits do not invent human DCO certification; authorized contributor sign-off remains required before eventual submission.

### Phase 2 implementation handoff

1. Introduce an authoritative v2 inventory envelope binding effective profiles, file states, path-free payload references, producer domains and lineage digest. Keep legacy Snapshot as a compatibility projection; lineage sidecars alone are not the migration.
2. Retrieval preparation must accept canonical inventory bytes and the externally finalized revision. Its current manifest is written before the catalog callback; v2 must close/checksum the catalog and required components before creating/verifying the final manifest and pointer receipt.
3. Store dispatches pointer/inventory/catalog versions independently, commits pointer plus receipt atomically against the expected parent, adds indexed file/hash/profile/handle and ordered-change projections, and rejects legacy writes after upgrade. Preserve original legacy bytes for engine-plus-inventory digest verification.
4. Every kernel writer goes through the finalizer: initialization, ordinary/full index, SCIP, LSP/Ctags, OCR and future policy/lineage maintenance. It owns incarnation/sequence/stamps, failed-publication atomicity and exact catalog cache identity.
5. Ingest order is bounded read/hash, effective profile selection, verified source/profile cache lookup, extraction on miss, then typed local-identity binding. Full reindex bypasses extraction reuse. Preserve chunk bytes/native locations, suppressed-owner syntax identities, shared framework identities and exact OCR status/assets. Never force old dispatch using synthetic paths or substitute ID string prefixes. Unknown legacy IDs require re-extraction or their separate provider domain.
6. Mixed-format GC/prune/relocation/removal must understand v2 inventories and retained receipts before enabling writes. Receipt roots must remain independently valid after ancestry expires. Keep existing manual retention until phase 4. Tests must cover every writer/reopen, original-byte migration, quota/failure rollback, old-binary rejection, pinned coherence and warm metadata without full inventory decoding.

### Accepted phase 1 gate receipts

- Native Windows: locked workspace build, `cargo fmt --all --check`, warnings-denied all-target workspace clippy, full locked workspace tests and doc tests passed on the final source. New core tests: 20; adversarial location tests: 4; ingest tests: 52 passed plus one deliberately ignored subprocess fixture. The separately provisioned ONNX model test remains ignored because assets were not supplied.
- Python repository/packaging suite: 46 tests passed with one Linux-only accounting skip on Windows. License bundling: 430 packages, with the four pre-existing separate-license-text review warnings documented above. Repository/SPDX/link, AGENTS/CLAUDE equality, staged/untracked whitespace checks passed.
- Linux container: release build and full workspace tests passed. A shared Cargo-cache timestamp issue initially reused a stale 19-test core binary. The accepted run used a temporary, ignored copy of Dockerfile whose test RUN prepended `find crates -type f -exec touch {} + &&` before the existing offline workspace test command. This touches only the container source copy and forced compilation; the accepted run explicitly executed all 20 core tests and the full regression suite. Reproduce with fresh Cargo artifacts or the same forced source-copy rebuild; do not accept the stale cached receipt. Permanent cache-hardening remains Q1 qualification work.
- Runtime image build and isolated `scripts/check-container.py --image airk:dev` passed: stdio MCP, two isolated collections, offline/read-only runtime, restart persistence, notices, usage and safe removal. No remote CI, push, PR, release, signing-account action or deployment was performed.
- P1 qualifies these contract primitives on the exercised environments only. No v2 publication/migration, changes API, automatic retention, historical reads, new language adapter or inference reuse is enabled. macOS, installed mixed-version helpers and real-client trials remain pending at their owning gates.

## Phase 2 integration in progress

- D4: [Authoritative publication and migration contract](v2-publication-contract.md). Inventory 2, projection schema 4, root pointer 3 and immutable catalog 3 are independently checked. Legacy inventories retain original byte verification. Source/profile selection precedes verified reuse; existing evidence is converted to path-free payloads and typed identities.
- All current kernel writers now call one finalizer. Current file stamps and bounded publication headers are retained, while detailed change/resolution coverage remains explicitly unavailable until phase 3. Searchable retention remains manual.
- Independent review identified and corrected legacy OCR cache migration, missing location/handle fields in evidence identity and exact parent verification. Warm catalog metadata must compare indexed rows against the receipt-bound pinned inventory. Cache mutexes must not surround catalog I/O. A newly created root database must not shadow legacy JSON after a failed migration; failure fixtures are part of this gate.
- The original 10,000-chunk draft ceiling could not represent legacy files with more than 400,000 short lines. The cap is now 500,000 with the existing 64 MiB encoded-payload bound; a 10,002-chunk original fixture preserves every byte/location. Existing document extraction keeps its 4 MiB limit.
- Initial focused receipts: 58 ingest tests passed with one ignored subprocess fixture, followed by six conversion tests including XLSX and large chunk-count roundtrips; six retrieval publication tests passed, with a model-bound fixture added for the final run; five initial store publication/lifecycle tests passed. Two integrated kernel publication tests passed and four streaming regressions passed; the remaining streaming assertion expected the superseded storage version and has been updated. These are intermediate receipts, not the final P2 gate.
- getrandom 0.3.4 was already present in Cargo.lock; kernel now directly uses that pinned dependency for full-width random identities. No new dependency version or external source was introduced. No push or PR performed.
- Concurrency qualification found Windows SQLite `SQLITE_PROTOCOL` during root WAL connection churn. Root opens now avoid redundant WAL-mode assignments and use at most three attempts for protocol failures in connection setup or complete read-only operations; mutation/commit operations are never replayed. The four focused publication tests passed, including concurrent metadata/search reads and idle-cache lease release. A final audit also found receipt retirement could bypass search's changed-head retry; receipt lookup now participates in that retry, with a deterministic retired-receipt fixture. Full gates are being rerun on these corrections.
- A subsequent Windows run exposed OS error 33 while hashing an exclusively locked, empty Tantivy coordination file. A deterministic store fixture reproduced the error. Empty components now retain open-handle length checks and exact empty SHA-256 verification without a conflicting byte read; unexpected nonempty content is rejected. The Linux run passed concurrent publication but exposed two old workspace fixtures that attempted unsupported legacy writes to v2 state; their legacy corruption/refresh scenarios are being separated from v2 writer rejection. These failed runs are not acceptance receipts.

### Accepted phase 2 gate receipts

- Frozen-source native Windows: locked workspace build, formatting, warnings-denied all-target clippy, full locked workspace tests and doc tests passed. New focused suites include four kernel publication/cache/concurrency tests, three legacy/OCR migration tests, one public writer-path fixture, eight retrieval publication tests and nine store publication/lifecycle tests. The retired-receipt race and bounded SQLite protocol retry also have passing unit regressions.
- Linux container: release build, full workspace tests and doc tests passed from the final source. The temporary ignored test recipe again forces source-copy timestamps before the offline test build, preventing stale Cargo-cache receipts. Its eight store publication tests exclude the Windows-only byte-lock fixture. Runtime image build and isolated stdio/two-collection/offline/read-only/restart/notices/usage/removal smoke passed.
- Python packaging/repository suite: 46 tests with one Linux-only skip on Windows. License bundling covered 430 packages with the same four existing separate-text review warnings. Repository checks, tracked/new-file whitespace and byte-identical AGENTS/CLAUDE checks passed. No extension code changed.
- Two intentionally ignored Rust fixtures remain: the subprocess helper and separately provisioned real ONNX model. Environment-gated external LSP/Ctags/Tesseract qualification was not provisioned. The new controlled provider executable tests publication behavior, not real provider qualification. Native macOS, independent native Linux packaging, installed mixed-version helpers and real Claude Code/Codex trials remain pending under Q1.
- P2 preserves manual retention and explicitly unavailable detailed change/resolver coverage. P3 must add complete scan observations, cross-path reuse/continuity, bounded events/resolvers, changes/validation/conditional-read APIs and exact response envelopes. No language expansion, automatic retention or historical-read completion is implied. No push or PR occurred.
