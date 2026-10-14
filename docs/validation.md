# Scaffold validation record

Local observations on 13 October 2026; no hosted CI run, signed/public release, production deployment or complete installed-editor lifecycle qualification is claimed. The local packaged-extension smoke below is narrower. Changes remain uncommitted.

| Check | Observed result |
|---|---|
| Windows, Rust 1.98.0, workspace tests | 46 tests passed: native document/code facts, sixteen language call fixtures, explicit byte spans/scopes/attributes, SQLite migration/rollback/WAL revision pinning, exact fact/graph budgets/isolation, SCIP source/build/digest/producer/range validation and invalidation, real fourteen-tool MCP/concurrent readers, collection maintenance and usage contracts |
| Windows formatting and all-target clippy with warnings denied | Passed |
| Native/VSIX packaging contracts | Four Python tests passed: inventory/tamper checks, Unix executable permissions, VSIX platform/version/identity, pre-release/display labeling and path-escape refusal |
| Windows x64 release candidate | Explicit-target release build passed; allowlisted ZIP verified entry-by-entry; archived executable version/help passed; 243 dependency notice packages included |
| Extension compilation/contracts, Node 24.15.0 / TypeScript 5.9.3 | 13 tests passed: rapid-edit batching, pending edits, serial writes, pause/manual/idle modes, budgets, trust, storage override refusal, MCP reconnect, fourteen agent formats/guidance routes, helper integrity and actual native MCP/collection isolation |
| Windows platform VSIX, VSCE 4.0.0 | Verified full archive payload, matching native receipt, helper/version/platform, beta display label, pre-release flag and checksums; no development dependencies, tests, source maps, references or index state included |
| Windows local PowerShell build | Full SQLite/code-aware checks, release build, benchmark, packaging and isolated editor run passed with 46 Rust, 13 extension and four packaging tests; checksSkipped=false/editorTests=true in local build receipt |
| Packaged extension-host smoke, VS Code 1.141.1 x64 | Private profile/public fixture: commands, MCP definitions, stable cached helper, incremental/full indexing, fourteen-tool stdio connection, storage selection and preservation of old/new SQLite state passed |
| Original code measurement smoke | 64 TS files: qualified definitions, call candidates, reuse, source-change/deletion invalidation passed. Fresh-process exact queries about 17–20 ms including startup; derived logical bytes 819200 after deletion. Fixture numbers are not large-repo or token-saving qualification; Windows CPU/RSS explicitly unavailable |
| GitHub workflow syntax and expressions | Passed actionlint 1.7.12; downloaded official release and verified its checksum |
| Repository public-file/SPDX/relative-link/paired-guidance checks and whitespace | Passed; temp/ is ignored and absent from Git's public file list |
| Linux amd64 Docker test stage, Rust 1.98.0 | Earlier scaffold revision: 21 tests passed, including the Unix-only external symlink exclusion test; new full-reindex change awaits hosted Linux coverage |
| Linux runtime image build | Passed; local tag airk:scaffold-check, non-root runtime and bundled notices |
| Compose configuration validation | Passed without starting the user's named AIRK deployment |
| Isolated one-container smoke | Passed: two collections; actual docker-exec stdio handshake/tool listing/definition retrieval; no network; read-only sources; persistent restart; usage and managed removal |
| Test lifecycle | Uniquely named fixture containers and volumes removed by the smoke script |
| Locked dependency notice bundle and pinned legal-text digests | Generated/verified for 243 dependency packages; six publisher-only standalone-text gaps remain release review items, including new rsqlite-vfs alongside the five prior declarations |
| macOS / hosted Windows-Linux-macOS CI | Explicit four-target native/VSIX build/test/package workflow configured; not executed from this session |

Use [CONTRIBUTING.md](../CONTRIBUTING.md) to reproduce checks. The public tiny fixtures are original and contain no user documents or local temp/ material.

The final checked local run is `artifacts/local/20261013-223003-9a7759ef`, with a Windows ZIP, `airk-0.1.0-win32-x64.vsix`, checksums, `code-benchmark.json` and `result.json`. These ignored local artifacts are review candidates, not published releases. The SCIP producer wrapper passed PowerShell syntax parsing; format/import behavior is exercised with original Rust-generated SCIP fixtures, not a qualified production indexer installation.

These checks establish the initial behavior, not release readiness, comprehensive language correctness, disk-backed DiskANN operation, semantic quality, machine-wide index scheduling/retention, crash/power-loss durability, large-repository performance or token savings. Those outcomes retain explicit acceptance criteria in [the roadmap](roadmap.md).
