# AIRK — Artifact Indexing and Retrieval Kernel, the Local MCP Power Tool for Coding Agents

**Version 1.1.3 · Development candidate.** [Usage & Savings changes](docs/release-1.1.3.md). The latest published packages remain 1.1.2. Download [native ZIPs and platform VSIX packages](https://github.com/iokaio/airk/releases/tag/v1.1.2). The [October 17 release summary](docs/releases-2026-10-17.md) combines the changes in 1.1.0, 1.1.1 and 1.1.2, with upgrade guidance and qualification limits.

**Give Claude Code, Codex, Gemini CLI/Antigravity, GitHub Copilot and other MCP-capable agents a shared, local index of your workspace.** AIRK indexes files and returns bounded facts and passages with source paths, hashes and original locations. The agent supplies the reasoning. Runtime storage uses memory and local disk.

[Document indexing](docs/document-indexing.md) specializes headings, dates, requirements, numeric bounds/units, qualitative cues and design/architecture/feature/decision language for Markdown, text and extracted documents. DOCX tables and XLSX worksheets retain row/cell evidence and original locations. These are cited evidence candidates, with category-filtered retrieval through MCP and the CLI.

This is an independent, runnable open-source project. Munarium, VCP and local reference files inform the design; their implementations are not dependencies. Persistent Tantivy lexical indexes, SQLite exact facts and an independent disk file provider for Microsoft DiskANN publish coherent revisions. Optional [local models](docs/local-models.md), including [CPU ONNX transformer inference](docs/onnx-models.md), enable vector and hybrid retrieval. AIRK does not download models or use remote inference; model quality must be evaluated for the intended corpus.

[Code facts and relationships](docs/code-facts-and-relationships.md) add scopes, signatures, written types, call/framework candidates and optional source/build-bound SCIP imports. Sixteen read-only MCP tools expose bounded evidence with explicit syntax versus producer provenance. Native indexing needs no external compiler/indexer installation.

## What's new in 1.1.3

Open **Usage & Savings** on any collection to see how much smaller AIRK results were than complete cited files, daily activity, optional session/task measurements and explicit cost assumptions. The report explains estimates in plain language and keeps technical counters expandable. [Measurement and evaluation](docs/usage-evaluation.md) covers content-free client adapters, matched-task comparisons and cumulative native totals. These features do not establish provider billing savings.

## What's new in 1.1.0–1.1.2

- **Workstation indexing:** automatic source allowances scale with machine memory, up to 2 GiB per collection. Coverage reports measured bytes and large files; reviewed limit increases preserve existing policies and retry once. [Ingestion limits](docs/ingestion-limits.md) distinguish raw-file admission from extracted-text and parser limits.
- **Collection dashboard:** inspect coverage and MCP helper health, index or reindex a collection, pause automatic work, choose storage and select Essential, Balanced or Detailed evidence. New collections default to Balanced; existing collections retain their previous detail until changed.
- **Storage and recovery:** new quota defaults are 100 GiB per collection and 500 GiB per storage root. Configurable retention targets 1–9 searchable generations, with active-only retention for new collections. Smaller fact catalogs, unchanged-row reuse and bounded verification reuse reduce storage and repeated-query work. Publication recovery handles interrupted builds while preserving committed evidence and live readers.
- **Coding-agent connections:** a persistent setup guide creates missing Codex project configuration, installs Claude Code/Gemini CLI/Antigravity project entries while preserving other settings, or opens Copilot's explicit registration controls. Helper health and actual agent discovery are checked separately.

See the [combined release notes](docs/releases-2026-10-17.md) for release provenance, measured Windows results and changes that require an explicit upgrade action.

## Try the native binary

The native product targets **Windows, macOS and Linux**. Developer builds require Git, the pinned Rust toolchain and native C/C++ build tools. Runtime requires only the built binary.

[GitHub CI](docs/ci-and-builds.md) is configured to test and package Windows x64, Linux x64 and both Mac architectures using standard hosted runners. Successful runs retain matching unsigned candidate ZIPs/VSIX packages, notices and checksums for seven days.

The manual [GitHub release workflow](docs/release-workflow.md#versioned-github-releases) adds versioned releases, Azure-signed Windows helpers, signed/notarized Mac helpers, and native ZIP/VSIX downloads hosted on [GitHub Releases](https://github.com/iokaio/airk/releases). Linux downloads are unsigned. Publication is selected explicitly when dispatching the workflow.

The [downloads website workflow](docs/release-workflow.md#downloads-website) publishes an AIRK overview, installation steps and direct platform downloads to **airk.ioka.io** using GitHub Pages. It refreshes after release publication and can also be run manually. GitHub releases provide native ZIPs and matching VSIX files; Marketplace publication is a separate workflow.

For the extension, install the matching VSIX, open and trust a workspace, then use **AIRK: Index Now** and **AIRK: Connect Coding Agent…**. Keep the generated helper path, root, data directory and collection together. Reload the selected agent and ask it to run `airk_status`, `airk_capabilities` and a known-phrase search. **Check AIRK MCP Helper** verifies the native server; the agent must confirm its own tool discovery. See [agent setup](docs/agent-integration.md) for client-specific routes.

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

Configure each workspace's agent with its own root and collection, using [the Docker MCP example](examples/mcp/docker.json). The client launches a stdio worker using docker exec inside the existing container. Workers share immutable disk generations and retain separate caches. The [native scheduler](docs/native-scheduler.md) coordinates submitted indexing jobs across workers sharing a data root; no system service is automatically installed. No public container image is published; native and extension downloads are distributed separately.

## Implemented retrieval

Sixteen read-only MCP tools provide search, file listing, bounded reads, file metadata, context, outlines, native syntax definitions/imports, structured document facts, code facts/relationships, textual identifier occurrences, indexed changes, retained revision metadata, status and capability inspection. Results describe published evidence; freshness requires indexing, and counts alone do not prove live workspace coverage.

Native grammar profiles cover Rust, Python, JavaScript/JSX, TypeScript/TSX, Go, Java, C/C++, C#, Ruby, PHP, Bash, PowerShell, Lua, Swift, Kotlin and SQL. Structured adapters cover Markdown, JSON/YAML/TOML, HTML/CSS, XML/SVG, CSV/TSV, INI-style configuration and Dockerfiles. Native extraction supports text-layer PDF, DOCX paragraphs and structured tables, XLSX stored worksheet rows/cells, and PPTX slide text in presentation order. Spreadsheet formulas are retained with any stored result; AIRK does not calculate formulas. Other UTF-8 files retain lexical retrieval without an extension allowlist.

See the [format matrix](docs/formats.md) for the broader language, build-file and composite-format profiles and their exact limits. Optional local LSP/Ctags execution and manifest-bound SCIP imports provide separately labeled producer evidence; explicit local OCR retains recognition and provenance limits. Supported composite formats map bounded child syntax to original host locations. These features do not establish universal compiler resolution, complete caller graphs or full framework understanding. The extension coalesces filesystem edits and submits durable native indexing requests. [Scale qualification](docs/scale-qualification.md) records measured workloads; larger-than-memory performance, production semantic quality and practical token savings remain unqualified.

### Choose indexing detail

The AIRK sidebar offers **Essential**, **Balanced** (the default for new collections), and **Detailed** for each collection. Essential keeps searchable text and exact citations. Balanced adds declarations, imports, useful structural/document facts and declaration change tracking. Detailed also retains individual calls and document-unit change tracking. Existing collections keep their previous Detailed behavior until changed.

The panel shows configured and published levels separately. Changing a level rebuilds evidence; the previous index stays available until the rebuild succeeds. Native fact and relationship responses explicitly report omitted detail. Storage quotas, retained generations and optional models are separate controls. See [index detail](docs/ingestion-limits.md#index-detail) for CLI equivalents and costs.

## Storage and maintenance

[Local usage and estimated savings](docs/usage-reporting.md) records bounded daily aggregates without query/source text, including response bytes, latency, indexing reuse, index stages, logical disk growth and supported OS CPU/I/O/RSS counters. JSON, CSV and Markdown reports preserve missing and overlapping measurements. Optional commands import content-free client/session and paired-task measurements, calibrate digest-bound BPE/WordPiece/Unigram/WordLevel pipelines with explicit framing, and apply supplied input-price scenarios. Imported counters remain self-reported and hypothetical price equivalents are not verified bills. Reports remain partial because historical recording loss cannot be certified; Windows transfer bytes are distinct from OS storage I/O and hardware physical I/O.

~~~console
airk --data-dir /path/to/airk-state collections list
airk --data-dir /path/to/airk-state --collection project-a collections prune
airk --data-dir /path/to/airk-state --collection project-a collections prune --apply
airk --data-dir /path/to/airk-state collections remove obsolete-project
~~~

On Windows use a path such as D:/indexes/airk. Pruning previews obsolete immutable generations and protects active or leased evidence. Removal deletes only a named, validated managed collection, preserving its source workspace; active native readers prevent removal. Disconnect workers for that collection first. See [collections](docs/collections.md) and [operations](docs/operations.md).

Default [storage quotas](docs/storage-quotas.md) are **100 GiB per collection** within **500 GiB per data root**; these are ceilings, not preallocated space. Existing saved quotas remain authoritative. New collections use automatic [retention](docs/retention.md) targeting the active generation only; existing saved policies are preserved, and legacy collections without a policy remain manual. Readers and protected history may retain additional data. Storage failures stop automatic retry loops until the cause is addressed.

Indexing streams one artifact at a time into content-addressed source/extraction storage. Compact inventories, immutable exact-fact catalogs and retrieval generations preserve revision identity without loading all source text for each request. [Source aliases](docs/source-roots.md) add independent folders to one collection; their original locations are pinned with each revision. Shared logical-byte quotas cover participating writers, including temporary builds and retained generations. Explicit relocation copies and verifies a data root before activating it, retaining the old managed state.

Automatic [ingestion limits](docs/ingestion-limits.md) allow up to 256 MiB per source file, 32 MiB extracted text per file and 100,000 files. Eligible source is sized to one eighth of detected machine memory, capped at 2 GiB, with twice that allowance for extracted text. Unknown memory uses a 512 MiB source allowance; saved policies remain unchanged. Collection policies explicitly permit larger workloads within validated ceilings and storage quotas; [scale qualification](docs/scale-qualification.md) records measured profiles. Ignore rules and common secret/build exclusions apply; indexes themselves contain source text. Read [SECURITY.md](SECURITY.md).

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

Start with the [documentation index](docs/README.md), [architecture](docs/architecture.md) and [roadmap](docs/roadmap.md). The optional [VS Code extension](docs/vscode-extension.md) implements native installation, client setup, configurable storage, indexing schedules and collection maintenance. The stable release uses the existing `iokaio.airk` Marketplace identity; see the release checklist for publication and installed-editor verification.

Review the [illustrated extension content guide](docs/extension-content.md), [manual build/publishing workflow and PR rules](docs/release-workflow.md), and [release summary and qualification links](docs/releases-2026-10-17.md) before preparing a public candidate.

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
