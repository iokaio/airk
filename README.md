# AIRK — Artifact Indexing and Retrieval Kernel, the Local MCP Power Tool for Coding Agents

**Beta.**

Local code and document evidence for coding agents. AIRK indexes files and returns bounded facts and passages with source paths, hashes and original locations. The agent supplies the reasoning. Runtime storage uses memory and local disk.

[Document indexing](docs/document-indexing.md) specializes headings, dates, requirements, numeric bounds/units, qualitative cues and design/architecture/feature/decision language for Markdown, text and extracted documents. DOCX tables and XLSX worksheets retain row/cell evidence and original locations. These are cited evidence candidates, with category-filtered retrieval through MCP and the CLI.

This is an independent, runnable open-source beta. Munarium, VCP and local reference files inform the design; their implementations are not dependencies. Persistent Tantivy lexical indexes, SQLite exact facts and an independent disk file provider for Microsoft DiskANN publish coherent revisions. Optional [local models](docs/local-models.md), including [CPU ONNX transformer inference](docs/onnx-models.md), enable vector and hybrid retrieval. AIRK does not download models or use remote inference; model quality must be evaluated for the intended corpus.

[Code facts and relationships](docs/code-facts-and-relationships.md) add scopes, signatures, written types, call/framework candidates and optional source/build-bound SCIP imports. Sixteen read-only MCP tools expose bounded evidence with explicit syntax versus producer provenance. Native indexing needs no external compiler/indexer installation.

## Try the native binary

The native product targets **Windows, macOS and Linux**. Developer builds require Git, the pinned Rust toolchain and native C/C++ build tools. Runtime requires only the built binary.

[GitHub CI](docs/ci-and-builds.md) is configured to test and package Windows x64, Linux x64 and both Mac architectures using standard hosted runners. Successful runs retain matching unsigned candidate ZIPs/VSIX packages, notices and checksums for seven days.

The manual [GitHub release workflow](docs/release-workflow.md#numbered-github-releases) adds automatically numbered releases, Azure-signed Windows helpers, signed/notarized Mac helpers, and native ZIP/VSIX downloads hosted on [GitHub Releases](https://github.com/iokaio/airk/releases). Linux downloads are unsigned. Publication is selected explicitly when dispatching the workflow.

The [downloads website workflow](docs/release-workflow.md#downloads-website) publishes an AIRK overview, installation steps and direct platform downloads to **airk.ioka.io** using GitHub Pages. It refreshes after release publication and can also be run manually. Until the first release, it displays a pending-download notice.

For a local Windows app and extension build:

~~~powershell
pwsh -NoProfile -File scripts/build-windows.ps1
~~~

Add `-OpenReview` to install the generated VSIX in an isolated editor profile. The extension provides measured workspace coverage, grouped controls, indexing presets, persistent pause/resume, storage selection, generation trimming, local usage reports, scoped VS Code MCP registration and guided agent setup. See [Windows build/review instructions](docs/windows-local-build.md) and [extension behavior and limits](docs/vscode-extension.md).

~~~console
cargo install --path crates/airk-cli --locked
~~~

From your own workspace:

~~~console
airk setup --collection project-a --client claude
airk search "authenticate" --collection project-a
airk symbol authenticate --collection project-a
airk outline src/auth.py --collection project-a
airk refs authenticate --collection project-a
airk index . --collection project-a
airk coverage --refresh --collection project-a
airk usage report --collection project-a
~~~

Setup indexes the workspace and prints client-specific configuration using actual absolute paths. It never overwrites client settings. Use --client codex for TOML, --client vscode or cursor for editor configurations, and other supported formats listed in [agent integration](docs/agent-integration.md). Commands return JSON; MCP uses stdio and needs no Python bridge, port, service account or API key.

State defaults to .airk/collections/<id> under the workspace. Choose a shared user-controlled disk with --data-dir. Each MCP launch fixes its root and collection; retrieval tools cannot switch collections.

## One container per user and machine

Build and start one persistent container from this checkout:

~~~console
docker compose up -d --build
docker exec airk airk --root /workspaces/project-a --data-dir /data --collection project-a index
docker exec airk airk --root /workspaces/project-b --data-dir /data --collection project-b index
~~~

The default mounts the tiny public [example workspaces](examples/workspaces). Set AIRK_WORKSPACES to the absolute parent directory of your intended workspaces before startup. Source mounts are read-only. A named volume retains all collection indexes, runtime networking is disabled and no ports are exposed.

Configure each workspace's agent with its own root and collection, using [the Docker MCP example](examples/mcp/docker.json). The client launches a stdio worker using docker exec inside the existing container. Workers share immutable disk generations and retain separate caches. The [native scheduler](docs/native-scheduler.md) coordinates submitted indexing jobs across workers sharing a data root; no system service is automatically installed. No public image or released package is advertised yet.

## Implemented retrieval

Sixteen read-only MCP tools provide search, file listing, bounded reads, file metadata, context, outlines, native syntax definitions/imports, structured document facts, code facts/relationships, textual identifier occurrences, status and capability inspection.

Native grammar profiles cover Rust, Python, JavaScript/JSX, TypeScript/TSX, Go, Java, C/C++, C#, Ruby, PHP, Bash, PowerShell, Lua, Swift, Kotlin and SQL. Structured adapters cover Markdown, JSON/YAML/TOML, HTML/CSS, XML/SVG, CSV/TSV, INI-style configuration and Dockerfiles. Native extraction supports text-layer PDF, DOCX paragraphs and structured tables, XLSX stored worksheet rows/cells, and PPTX slide text in presentation order. Spreadsheet formulas are retained with any stored result; AIRK does not calculate formulas. Other UTF-8 files retain lexical retrieval without an extension allowlist.

See the [format matrix](docs/formats.md) for exact limits. Optional manifest-bound SCIP imports provide producer reference/implementation assertions; native compiler resolution, resolved caller graphs, OCR, full framework analysis and embedded languages remain outside the implemented slice. The extension coalesces filesystem edits and submits durable native indexing requests. Original-fixture correctness and DiskANN recall checks are included; large-repository performance, production semantic quality and practical token savings remain unqualified.

## Storage and maintenance

[Local usage and estimated savings](docs/usage-reporting.md) records bounded daily aggregates without query/source text, including response bytes, latency, indexing reuse, index stages, logical disk growth and supported OS CPU/I/O/RSS counters. JSON, CSV and Markdown reports preserve missing and overlapping measurements. Optional commands import content-free client/session and paired-task measurements, calibrate digest-bound BPE/WordPiece/Unigram/WordLevel pipelines with explicit framing, and apply supplied input-price scenarios. Imported counters remain self-reported and hypothetical price equivalents are not verified bills. Reports remain partial because historical recording loss cannot be certified; Windows transfer bytes are distinct from OS storage I/O and hardware physical I/O.

~~~console
airk --data-dir /path/to/airk-state collections list
airk --data-dir /path/to/airk-state --collection project-a collections prune
airk --data-dir /path/to/airk-state --collection project-a collections prune --apply
airk --data-dir /path/to/airk-state collections remove obsolete-project
~~~

On Windows use a path such as D:/indexes/airk. Pruning previews obsolete immutable generations and protects active or leased evidence. Removal deletes only a named, validated managed collection, preserving its source workspace; active native readers prevent removal. Disconnect workers for that collection first. See [collections](docs/collections.md) and [operations](docs/operations.md).

Indexing streams one artifact at a time into content-addressed source/extraction storage. Compact inventories, immutable exact-fact catalogs and retrieval generations preserve revision identity without loading all source text for each request. [Source aliases](docs/source-roots.md) add independent folders to one collection; their original locations are pinned with each revision. Shared logical-byte quotas cover participating writers, including temporary builds and retained generations. Explicit relocation copies and verifies a data root before activating it, retaining the old managed state.

Default [ingestion limits](docs/ingestion-limits.md) are 16 MiB per source file, 4 MiB extracted text per document, 64 MiB eligible source, 128 MiB extracted text per collection and 10,000 files. Collection policies explicitly permit larger workloads within validated ceilings and storage quotas; [scale qualification](docs/scale-qualification.md) records measured profiles. Ignore rules and common secret/build exclusions apply; indexes themselves contain source text. Read [SECURITY.md](SECURITY.md).

## Code organization

~~~text
crates/
  airk-core/       Artifact, fact, query, citation and provider contracts
  airk-ingest/     Discovery, hashing, native grammars and document adapters
  airk-store/      SQLite state, durable queues/usage, locks and collection retention
  airk-retrieval/  Persistent Tantivy, disk-backed DiskANN, local vectors and budgets
  airk-kernel/     Shared collection-scoped application services
  airk-mcp/        Read-only schemas and official SDK stdio transport
  airk-cli/        Binary, client setup and collection operations
docs/             Architecture, decisions, roadmap and extension research
packages/vscode/  Native-helper extension, controls and platform VSIX packaging
examples/         Public MCP configurations and original tiny workspaces
scripts/          Local app/extension builds, packaging and contributor checks
third_party/      Pinned dependency license records; no imported project source
.github/          Cross-platform CI and contribution templates
~~~

Start with the [documentation index](docs/README.md), [architecture](docs/architecture.md) and [roadmap](docs/roadmap.md). The optional [VS Code extension preview](docs/vscode-extension.md) implements native installation, client setup, configurable storage, indexing schedules and collection maintenance. It is not signed or published to Marketplace.

Review the [illustrated extension content guide](docs/extension-content.md), [manual build/publishing workflow and PR rules](docs/release-workflow.md), and [0.1.0 release checklist](docs/release-checklist-0.1.0.md) before preparing a public candidate.

## Contributing and license

Read [CONTRIBUTING.md](CONTRIBUTING.md), [SUPPORT.md](SUPPORT.md), [SECURITY.md](SECURITY.md) and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

~~~console
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
pwsh -NoProfile -File scripts/check-repo.ps1
git diff --check
~~~

Original code/documentation use the existing [Apache-2.0 license](LICENSE). Dependency notices and release-review gaps are recorded in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Local temp/ reference files are excluded from Git, indexing and Docker builds.
