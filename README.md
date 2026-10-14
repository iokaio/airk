# AIRK

**AIRK — Artifact Indexing and Retrieval Kernel** (beta)

Local code and document evidence for coding agents. AIRK indexes files and returns bounded facts and passages with source paths, hashes and original locations. The agent supplies the reasoning. Runtime storage uses memory and local disk.

[Document indexing](docs/document-indexing.md) specializes headings, dates, requirements, numeric bounds/units, qualitative cues and design/architecture/feature/decision language for Markdown, text and extracted PDF/DOCX/PPTX content. These are cited evidence candidates, with category-filtered retrieval through MCP and the CLI.

This is an independent, runnable open-source scaffold. Munarium, VCP and local reference files inform the design; their implementations are not dependencies. DiskANN with a real disk provider is the selected final vector architecture. The current working slice uses embedded SQLite for revision state, exact facts and relationships, plus an in-memory lexical index; local embeddings and DiskANN integration are still to implement.

[Code facts and relationships](docs/code-facts-and-relationships.md) add scopes, signatures, written types, call/framework candidates and optional source/build-bound SCIP imports. Fourteen read-only MCP tools expose bounded evidence with explicit syntax versus producer provenance. Native indexing needs no external compiler/indexer installation.

## Try the native binary

The native product targets **Windows, macOS and Linux**. Developer builds require Git, the pinned Rust toolchain and native C/C++ build tools. Runtime requires only the built binary.

[GitHub CI](docs/ci-and-builds.md) is configured to test and package Windows x64, Linux x64 and both Mac architectures using standard hosted runners. Successful runs retain matching unsigned candidate ZIPs/VSIX packages, notices and checksums for seven days.

For a local Windows app and extension build:

~~~powershell
pwsh -NoProfile -File scripts/build-windows.ps1
~~~

Add `-OpenReview` to install the generated VSIX in an isolated editor profile. The extension provides indexing presets, pause/resume, storage selection, collection maintenance, scoped VS Code MCP registration and guided agent setup. See [Windows build/review instructions](docs/windows-local-build.md) and [extension behavior and limits](docs/vscode-extension.md).

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

Configure each workspace's agent with its own root and collection, using [the Docker MCP example](examples/mcp/docker.json). The client launches a stdio worker using docker exec inside the existing container. This scaffold has one container host and multiple scoped worker processes; a shared daemon cache is future work. No public image or released package is advertised yet.

## Implemented retrieval

Twelve read-only MCP tools provide search, file listing, bounded reads, file metadata, context, outlines, native syntax definitions/imports, structured document facts, textual identifier occurrences, status and capability inspection.

Native grammar profiles cover Rust, Python, JavaScript/JSX, TypeScript/TSX, Go, Java, C/C++, C#, Ruby, PHP, Bash, PowerShell, Lua, Swift, Kotlin and SQL. Structured adapters cover Markdown, JSON/YAML/TOML, HTML/CSS, XML/SVG, CSV/TSV, INI-style configuration and Dockerfiles. Native extraction supports text-layer PDF, DOCX body paragraphs/table cells and PPTX slide text in presentation order. Other UTF-8 files retain lexical retrieval without an extension allowlist.

See the [format matrix](docs/formats.md) for exact limits. Optional manifest-bound SCIP imports provide producer reference/implementation assertions; native compiler resolution, resolved caller graphs, OCR, full framework analysis, embedded languages and machine-wide scheduling remain open. The extension coalesces filesystem edits and schedules indexing. An original-corpus correctness/measurement smoke is included; large-repository performance and practical token savings remain unqualified.

## Storage and maintenance

[Local usage and estimated savings](docs/usage-reporting.md) now has code contracts and tested report/estimation helpers. The plan defaults to local aggregate counters without query/source text, tracks performance and indexing reuse, and separates counterfactual context reduction from actual agent/provider billing. Persistent recording and user-facing reports are the next implementation increments.

~~~console
airk --data-dir /path/to/airk-state collections list
airk --data-dir /path/to/airk-state collections remove obsolete-project
~~~

On Windows use a path such as D:/indexes/airk. Removal deletes only a named, validated managed collection, preserving its source workspace. Disconnect workers for that collection before removal. See [collections](docs/collections.md) and [operations](docs/operations.md).

The initial limits are 1 MiB per source file, 4 MiB of extracted text per document, 64 MiB of eligible source and 10,000 indexed files. This is a bootstrap capacity, not the intended large-repository scale. Ignore rules and common secret/build exclusions apply; indexes themselves contain source text. Read [SECURITY.md](SECURITY.md).

## Code organization

~~~text
crates/
  airk-core/       Artifact, fact, query, citation and provider contracts
  airk-ingest/     Discovery, hashing, native grammars and document adapters
  airk-store/      Memory/disk snapshots, writer locks and collection maintenance
  airk-retrieval/  Lexical candidates, response budgets and future DiskANN adapter
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
