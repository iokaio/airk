# AIRK — Artifact Indexing and Retrieval Kernel, the Local MCP Power Tool for Coding Agents

**Version 1.3.2.** See the [1.3.2 release notes](docs/release-1.3.2.md), [1.3.1 release notes](docs/release-1.3.1.md), [1.3.0 format changes](docs/release-1.3.0.md), and [1.2.4 diagnosis and recovery notes](docs/release-1.2.4.md). Download available [native ZIPs and platform VSIX packages](https://github.com/iokaio/airk/releases); publication is verified separately by the release workflows.

The [1.4.0 service implementation](docs/airkservice-architecture.md) is in progress on this branch. Its opt-in native installer supports immutable offline payloads, separate writable state, resumable activation and approved organization-managed read-only payloads. Native maintenance has guarded durable repair previews and per-project receipts, with bounded process logs available while the service is down. MCP development responses now default to terse, with per-call Full overrides, a panel setting and cached service source observations; see [response modes](docs/mcp-response-modes.md). Development outline/symbol/import navigation now defaults to minimal rows with shared source bindings, and fact tools use `items`. Development call discovery also matches terminal method names and labels candidates; temporary read contention carries retry hints. Physical compression preserves indexed call discovery. Retained change metadata now supports delayed baselines and explicit `index-current` validation. Service quota, staging and lifecycle ownership are now separate from legacy storage. Owned login shutdown also tolerates damaged payload files. Retained reads also survive denied or redirected source roots. Fresh installations now bind native log placement to verified state ownership. Native service model setup now installs one verified shared asset set, with lazy direct inference and no collection-local weights. Transformer calibration inputs also use bounded shared storage and direct helper sessions; usage records stay per collection. Installed startup now refuses to replace a lost initialized control directory with an empty registry. Service queries retain one shared native execution budget through cancellation and client reconnects. Each 1.4 data root is now bound to one durable service registry namespace. Default extension service cutover and release qualification remain pending; see the [draft release record](docs/release-1.4.0.md).

**Give Claude Code, Codex, Gemini CLI/Antigravity, GitHub Copilot and other MCP-capable agents a shared, local index of your workspace.** AIRK indexes files and returns bounded facts and passages with source paths, hashes and original locations. The agent supplies the reasoning. Runtime storage uses memory and local disk.

[Document indexing](docs/document-indexing.md) specializes headings, dates, requirements, numeric bounds/units, qualitative cues and design/architecture/feature/decision language for Markdown, text and extracted documents. DOCX tables and XLSX worksheets retain row/cell evidence and original locations. These are cited evidence candidates, with category-filtered retrieval through MCP and the CLI.

This is an independent, runnable open-source project. Munarium, VCP and local reference files inform the design; their implementations are not dependencies. SQLite FTS5 lexical retrieval, authenticated SQLite records and mutable Microsoft DiskANN publish one coherent current revision. Vector indexing and hybrid retrieval are enabled by default. The native helper embeds the pinned [local CPU embedding model](docs/onnx-models.md) (about 91 MB of assets), which the trusted panel installs locally on first use without downloading; turn **Vector search** off per collection to opt out. Inference and source data stay local. Model quality must be evaluated for the intended corpus.

[Code facts and relationships](docs/code-facts-and-relationships.md) add scopes, signatures, written types, call/framework candidates and optional source/build-bound SCIP imports. The 1.3.2 source has fifteen read-only MCP tools; the published 1.2.4 helper has sixteen. Both expose bounded evidence with explicit syntax versus producer provenance. Native indexing needs no external compiler/indexer installation.

## What's new in 1.3.2

Initialized empty indexes now publish progressive source batches, including empty full rebuilds and first-use model/detail changes. The first batch targets 32 files, and the final publication preserves completed batches. The dashboard clarifies source-scan status and places progress beside indexing controls. See [scope and qualification](docs/release-1.3.2.md). The reported Windows two-project setup passed manual acceptance with the 1.3.2 VSIX. Publication is verified separately by the release workflows.

## What's new in 1.3.1

The helper includes the FP32 all-MiniLM-L6-v2 weights, tokenizer, manifest and Apache license. First-use model setup works offline, including on networks that block Hugging Face. Native-only setup uses `airk collections vectors install-bundled`; existing custom models are preserved.

Different projects sharing one storage root can index concurrently. Each collection has its own native worker lock; two windows on the same project still share one worker. Editor indexing requests advance only their selected collection, and foreign source requests no longer interrupt semantic batches. The existing four execution slots retain CPU/memory budgets, admit model loading before it starts, and report capacity waits separately from a busy project. An active older global worker requires restarting older AIRK workers before retrying. See [scheduling and upgrade details](docs/native-scheduler.md).

## What's new in 1.3.0

Committed lexical passages and facts can be searched while a source scan continues. Known saved-path changes can update only those paths, while periodic full inventories reconcile coverage. An older 1.2 lexical index needs a rebuild because this candidate moves to SQLite FTS5; the extension starts that rebuild on indexing, and native CLI users run `airk index --full`. See the [release summary and qualification limits](docs/release-1.3.0.md#release-summary).

The [indexing follow-up](docs/indexing-follow-up-2026-10-21.md) queues rebuilds during active scans, retains revision-specific completion reports, clears stale editor waits and labels coverage pending its five-minute rescan. Warm reads and publications can reuse guarded, bounded lexical snapshots; Linux notification guards reject raw writes even when timestamps coincide; explicit integrity diagnostics still hash the complete lexical state. The follow-up documents the cache's changed verification contract and limits.

Saved-path updates are available through `airk index --paths src/retry.rs tests/retry.rs`; the editor and native watcher submit known changed paths too. They preserve unrelated indexed bindings and leave semantic work queued after lexical/fact publication. Complete reconciliation remains the backstop. Scheduled semantic calls reuse a model across bounded batches, and status exposes scheduled source-pass timings and inventory age. See the [indexing feedback review](docs/indexing-speed-review-2026-10-21.md) for implementation decisions, integrity limits and measurements.

`airk model-catalog` reports native CPU instruction capabilities and the pinned default for new model setup without opening a workspace, index or model. New extension model setup and the offline provisioning script share one catalog. Automatic selection retains the current FP32 MiniLM model; CPU-specific INT8 selection is a [documented future option](docs/onnx-models.md#capabilities-and-qualified-model-catalog), not enabled behavior. Existing/custom models are preserved.

The former `airk_history` MCP tool is retired because AIRK 1.2 retains one current index and could only return an unsupported error. Search returns focused snippets by default, with `full_chunks: true` for complete indexed chunks. `AIRK_MCP_RESPONSE_MODE=terse` shortens repeated response metadata while retaining the exact token, citations, hashes and coverage state. MCP requests can wait briefly in a bounded queue; `known_hash` mismatches fail explicitly; textual references select indexed candidates before exact source verification. The editor can verify and start MCP while initial indexing continues, shows committed file progress, and stops a scan that makes no progress for ten minutes. Progressive scans now defer semantic inference until source discovery completes and adapt batch cadence to publication cost, keeping the first batch small and later work bounded. A bounded scheduler wait now resumes the same queued request; diagnostics distinguish a completed native request from an earlier editor wait failure. [Release notes](docs/release-1.3.0.md) describe qualification limits; release workflow records establish publication status.

## What's new in 1.2.4

**Diagnose & Repair** collects the selected collection's helper identity, current indexing/semantic progress, queue state and recent failures inside the extension. Copy a bounded report, retry indexing, resume window indexing or rebuild derived evidence from the same view. Recent failures survive reloads and are kept separate from current observations.

**Verify retained data** checks source and extraction inputs in bounded pages at one indexed revision. It reports incomplete checks and failures explicitly; saved readiness alone is not proof of intact retained data. Verification yields to active indexing and enrichment. Diagnostics also flag older helper paths in known agent configurations after an update; settings are preserved for manual review.

Diagnostics recognize the managed CLI copy when its bytes match the verified extension helper. Native failures display readable messages while preserving bounded stderr details. Missing extractions across a batch share one authenticated posting-map traversal; automatic recapture waits for readers within a bounded, pause-aware admission window.

Native writer admission also detects SQLite's silent read-only fallback. On Windows, verified temporary sharing denial permits a bounded reopen; genuine permission failures remain explicit.

Retry safely releases abandoned publication quota reservations after proving the index is Ready with no pending journal. Committed evidence and reservations needed for recovery remain protected.

Temporary publication contention retries with a capped delay using structured native outcomes that are independent of Rust backtraces. Indexing can recreate missing retained objects from validated source files and retire old postings for changed or deleted files when the prior extraction is missing. Repair remains bounded; corruption, permissions and unavailable journals remain visible failures. Slow semantic work no longer forces the next source batch to publish prematurely. Agent Lean remains the default.

Automatic scheduling rechecks what is due when a timer fires. An early timer callback cannot start unrequested semantic work and block a pending source scan.

## What's new in 1.2.3

New collections start with **Agent Lean**: selected declarations, imports and document facts with compact MCP replies. Start lean and fast, then choose Rich Structure for broader structural facts and declaration tracking, Detailed for document-unit tracking, or Search Only for text and citations. From 1.3.0, Agent Lean and Rich Structure both retain supported call occurrences as unresolved syntax candidates. Existing collections retain their saved or published detail level. Rich Structure, Search Only and Detailed remain selectable. Semantic search stays enabled; the default Agent Lean vector policy excludes reviewed lockfiles and minified JS/CSS while retaining their lexical evidence.

Progressive indexing also keeps newly captured and reusable objects alive across intermediate batches. This fixes the `retained object missing` failure that could stop a fresh scan; update and retry **Index now** without wiping the collection.

## What's new in 1.2.2

The VS Code extension continues unfinished semantic batches after **Index now** and when reopening a trusted workspace, without a separately installed service. The dashboard shows semantic indexing independently of saved-file coverage, even at 100%. Pause, failure blocking and collection/storage changes apply between batches. Manual mode still completes already requested semantic work, and previously committed evidence remains available.

## What's new in 1.2.1

The extension's Details/Marketplace description and repository documentation now explain the 1.2 behavior and current controls. One storage-budget message was corrected; indexing and retrieval behavior remain as in 1.2.0. See the [release notes](docs/release-1.2.1.md).

## What's new in 1.2.0

**Agent Lean** keeps useful code declarations, signatures, imports and document facts with compact MCP replies. **Rich Structure** and **Search Only** name the existing Balanced and Essential policies; in 1.2.0–1.2.2 new collections used Rich Structure; 1.2.3 defaults to Agent Lean. Progressive indexing publishes bounded exact-evidence and semantic batches, preserves committed work when paused, and reports discovery and semantic readiness separately. Optional lossless retained-object compression starts off.

Version 1.3.2 uses one authenticated current SQLite FTS5/DiskANN index. When the extension indexes a recognized older 1.2 collection, it replaces the obsolete derived index and starts a full rebuild using the existing ID. Source files and saved settings remain untouched. CLI users can run `airk index --full` for the same upgrade. The earlier 1.1 layout still requires fresh 1.2 state. See [current-index operation and recovery](docs/current-index-runtime.md) and the [current qualification notes](docs/release-1.3.2.md#upgrade-and-qualification).

## What's new in 1.1.4

Warm retrieval validates publication integrity, hybrid search ranks distinct chunks before applying text budgets, and scoped vector searches use bounded matching candidates. Discovery tools support revision-bound continuation cursors and compact outlines. PDF parsing runs in a local resource-limited worker, source traversal uses protected handles, and CLI shell setup rejects redirected or hard-linked profiles. [Request execution](docs/request-execution.md) keeps optional accounting off the MCP protocol thread.

Usage comparisons use compact full-source byte measurements from the response's exact indexed revision. Current source-size measurements can compare small excerpts from large repositories without the former 8 MiB inventory/retained-payload and 1 MiB text comparison limits. The report distinguishes evidence requests from background activity and explains unavailable comparisons. Index once with the new helper and restart agent MCP sessions; older missing comparisons remain unmeasured. See [the 1.2 runtime and rebuild boundary](docs/current-index-runtime.md).

## What's new in 1.1.3

Open **Usage & Savings** on any collection to see how much smaller AIRK results were than complete cited files, daily activity, optional session/task measurements and explicit cost assumptions. The report explains estimates in plain language and keeps technical counters expandable. [Measurement and evaluation](docs/usage-evaluation.md) covers content-free client adapters, matched-task comparisons and cumulative native totals. These features do not establish provider billing savings.

## What's new in 1.1.0–1.1.2

- **Workstation indexing:** automatic source allowances scale with machine memory, up to 4 GiB per collection. Coverage reports measured bytes and large files; reviewed limit increases preserve existing policies and retry once. [Ingestion limits](docs/ingestion-limits.md) distinguish raw-file admission from extracted-text and parser limits.
- **Collection dashboard:** inspect coverage and MCP helper health, index or reindex a collection, pause automatic work, choose storage and select evidence detail. The current controls are described below; the 1.1 labels were Essential, Balanced and Detailed.
- **Storage and recovery:** the 1.1 line introduced storage and recovery controls. Version 1.2 replaced historical generations with one authenticated current index. Defaults are 256 GiB per collection and 500 GiB per storage root.
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

For native-only use, [provision the local model](docs/onnx-models.md#default-setup-and-native-provisioning) before indexing to enable the default hybrid search. The extension handles this setup automatically. Explicit `--mode lexical` remains available for a model-free query.

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
airk tools list
airk tools help airk_search
airk tools shell --collection project-a
~~~

Setup indexes the workspace and prints client-specific configuration using actual absolute paths. It never overwrites client settings. Use --client codex for TOML, --client vscode or cursor for editor configurations, and other supported formats listed in [agent integration](docs/agent-integration.md). Commands return JSON; MCP uses stdio and needs no Python bridge, port, service account or API key.

Select **Open AIRK Tools** on a dashboard collection to open an interactive tools console in VS Code's Terminal panel. It includes help and examples and calls the same MCP handlers without a coding agent. The standalone CLI also supports `tools call <tool> --args-file query.json`; see [direct MCP tool access](docs/mcp.md#call-tools-without-a-coding-agent).

The extension installs its bundled CLI during the first trusted startup and updates it with the extension. Open a new terminal and run `airk --help`; restart external terminal applications to pick up PATH changes. No Cargo installation is needed. The dashboard includes a CLI getting-started note and **Install or repair CLI** button. See [CLI installation and PATH](docs/vscode-extension.md#cli-installation-and-path) for platform behavior and troubleshooting.

State defaults to .airk/collections-1.2/<id> under the workspace. Choose a shared user-controlled disk with --data-dir. Each MCP launch fixes its root and collection; retrieval tools cannot switch collections.

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

The 1.3.2 source provides fifteen read-only MCP tools for search, file listing, bounded reads, file metadata, context, outlines, native syntax definitions/imports, structured document facts, code facts/relationships, textual identifier occurrences, indexed changes, status and capability inspection. The retired `airk_history` endpoint belonged to an earlier history design; the published 1.2.4 helper still lists it but returns only an unsupported error. Results describe published evidence; freshness requires indexing, and counts alone do not prove live workspace coverage.

Native grammar profiles cover Rust, Python, JavaScript/JSX, TypeScript/TSX, Go, Java, C/C++, C#, Ruby, PHP, Bash, PowerShell, Lua, Swift, Kotlin and SQL. Structured adapters cover Markdown, JSON/YAML/TOML, HTML/CSS, XML/SVG, CSV/TSV, INI-style configuration and Dockerfiles. Native extraction supports text-layer PDF, DOCX paragraphs and structured tables, XLSX stored worksheet rows/cells, and PPTX slide text in presentation order. Spreadsheet formulas are retained with any stored result; AIRK does not calculate formulas. Other UTF-8 files retain lexical retrieval without an extension allowlist.

See the [format matrix](docs/formats.md) for the broader language, build-file and composite-format profiles and their exact limits. Optional local LSP/Ctags execution and manifest-bound SCIP imports provide separately labeled producer evidence; explicit local OCR retains recognition and provenance limits. Supported composite formats map bounded child syntax to original host locations. These features do not establish universal compiler resolution, complete caller graphs or full framework understanding. The extension coalesces filesystem edits and submits durable native indexing requests. [Scale qualification](docs/scale-qualification.md) records measured workloads; larger-than-memory performance, production semantic quality and practical token savings remain unqualified.

### Choose indexing detail

The AIRK sidebar offers **Search Only**, **Agent Lean** (the default for new collections), **Rich Structure**, and **Detailed**. Search Only keeps searchable text and exact citations. Agent Lean keeps selected declarations, imports, call occurrences and useful document facts with compact MCP replies. Rich Structure adds broader structural facts and declaration tracking while also retaining call occurrences. Detailed adds document-unit tracking. Existing saved choices keep their meanings.

The panel shows configured and published levels separately. Changing a level rebuilds evidence; preparation keeps the current head readable, and Applying requires completion or recovery before reads resume. Native fact and relationship responses explicitly report omitted detail. Storage quotas, current-object cleanup and vector search are separate controls. See [index detail](docs/ingestion-limits.md#index-detail) for CLI equivalents and costs.

## Storage and maintenance

[Local usage and estimated savings](docs/usage-reporting.md) records bounded daily aggregates without query/source text, including response bytes, latency, indexing reuse, index stages, logical disk growth and supported OS CPU/I/O/RSS counters. JSON, CSV and Markdown reports preserve missing and overlapping measurements. Optional commands import content-free client/session and paired-task measurements, calibrate digest-bound BPE/WordPiece/Unigram/WordLevel pipelines with explicit framing, and apply supplied input-price scenarios. Imported counters remain self-reported and hypothetical price equivalents are not verified bills. Reports remain partial because historical recording loss cannot be certified; Windows transfer bytes are distinct from OS storage I/O and hardware physical I/O.

~~~console
airk --data-dir /path/to/airk-state collections list
airk --data-dir /path/to/airk-state --collection project-a collections prune
airk --data-dir /path/to/airk-state --collection project-a collections prune --apply
airk --data-dir /path/to/airk-state collections remove obsolete-project
~~~

On Windows use a path such as D:/indexes/airk. Pruning previews obsolete immutable generations and protects active or leased evidence. Removal deletes only a named, validated managed collection, preserving its source workspace; active native readers prevent removal. Disconnect workers for that collection first. See [collections](docs/collections.md) and [operations](docs/operations.md).

Default [storage quotas](docs/storage-quotas.md) are **256 GiB per collection** within **500 GiB per data root**. These are ceilings, not preallocated space. AIRK 1.2 starts fresh collection/settings ownership and leaves 1.1 state untouched. Current cleanup proves ownership before removing unreferenced objects. See the [current-index contract](docs/current-index-runtime.md) for token expiry, busy states and recovery.

Indexing streams one artifact at a time into content-addressed source/extraction storage. Authenticated current catalog/postings, head-bound FTS5 content and posting digests and mutable DiskANN records preserve exact revision identity without loading all source text for each request. [Source aliases](docs/source-roots.md) add independent folders to one collection; their original locations are pinned with each revision. Shared logical-byte quotas cover participating writers, including temporary builds and durable recovery allowances. Explicit relocation copies and verifies a data root before activating it, retaining the old managed state.

Automatic [ingestion limits](docs/ingestion-limits.md) allow up to 256 MiB per source file, 32 MiB extracted text per file and 100,000 files. Eligible source is sized to one eighth of detected machine memory, capped at 4 GiB, with twice that allowance for extracted text. Unknown memory uses a 512 MiB source allowance; saved policies remain unchanged. Collection policies explicitly permit larger workloads within validated ceilings and storage quotas; [scale qualification](docs/scale-qualification.md) records measured profiles. Ignore rules and common secret/build exclusions apply; indexes themselves contain source text. Read [SECURITY.md](SECURITY.md).

## Code organization

~~~text
crates/
  airk-core/       Artifact, fact, query, citation and provider contracts
  airk-ingest/     Discovery, hashing, native grammars and document adapters
  airk-store/      SQLite state, durable queues/usage, locks and current-object cleanup
  airk-retrieval/  Disk-backed DiskANN, local vectors and retrieval primitives
  airk-kernel/     Collection-scoped services, rank fusion and response budgets
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
