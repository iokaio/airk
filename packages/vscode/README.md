# AIRK for VS Code — Beta 0.1.0

AIRK supplies local code and document evidence to coding agents through fourteen read-only MCP tools. This preview bundles its native helper; installation requires no Docker, Rust, Python, service account or API key.

Your agent can find a definition, inspect nearby source lines or retrieve a document requirement through a shared workspace index. AIRK returns bounded evidence with source paths, hashes and original locations, so you can inspect what the agent used. The agent supplies the reasoning; smaller responses do not by themselves prove lower bills or better answers.

![Workspace code and documents flow through AIRK's local collection to independently configured Claude Code, Codex, Gemini CLI and other MCP clients.](https://raw.githubusercontent.com/iokaio/airk/main/docs/images/local-evidence.png)

**Claude Code, Codex and Gemini CLI:** choose **AIRK: Set Up Coding Agent** to preview the selected client's configuration. Claude Code uses a project `.mcp.json` entry; Codex uses a `mcp_servers` TOML table; Gemini CLI uses `mcpServers` in `.gemini/settings.json`. Merge the scoped entry, then enable it in that client. Several agents can share the same root, data directory and collection. VS Code's MCP registration does not configure the other clients automatically.

![Saved files are indexed into a complete published revision, then an agent retrieves bounded evidence. Coverage, controls and citations make freshness and scope visible.](https://raw.githubusercontent.com/iokaio/airk/main/docs/images/index-to-context.png)

These are explanatory illustrations. AIRK's local runtime does not control whether your selected agent sends returned evidence to its model provider.

Document facts include heading context, dates, requirement IDs and modality, numeric bounds/units, qualitative cues and design/architecture/feature/decision candidates. DOCX tables and XLSX worksheets retain structured row/cell evidence with original locations; stored spreadsheet formulas are never recalculated. Use **Index Now** after updating to populate new facts in existing collections. Categories are evidence cues, not inferred compliance; ambiguous dates and source locations remain explicit.

Extension identity: **iokaio.airk**. Display name: **AIRK — Artifact Indexing and Retrieval Kernel** (beta). Version **0.1.0** has platform-specific packages; check the package's preview/stable channel before installing. The Windows package is **airk-0.1.0-win32-x64.vsix**; Linux and macOS use the matching platform suffix.

Install the VSIX for your platform through **Extensions: Install from VSIX**. Open a folder and grant workspace trust. The **AIRK** status item shows measured coverage for the active folder; click it to index. Use **AIRK: Open Controls** in the command palette for other controls. Each workspace folder gets an isolated collection. VS Code discovers the collection through AIRK's MCP provider; use **MCP: List Servers** to enable it.

- **Index Now / Reindex Collection**: incremental indexing or full re-extraction.
- **Choose Indexing Mode**: after edits settle (default), when idle, on save, or manual.
- **Pause / Resume Auto-indexing**: preserve queued edits; an in-progress write finishes.
- **Choose Index Storage Folder**: put indexes on a drive of your choice. Old indexes remain in their old location; this changes storage without migrating or deleting them.
- **Manage Collections and Disk Space**: inspect collection bytes, available drive space, known roots, last indexed time and generation. Remove an unused collection through the separate removal command; source files remain intact.
- **Trim Obsolete Index Generations**: review reclaimable generations, then confirm native pruning; active evidence and reader leases remain protected.
- **Show Collection Status**: inspect current, stale, missing, extraction-failed and capacity-blocked eligible files. Incomplete scans show unknown coverage; unsaved buffers are separate from indexed disk bytes.
- **Show Local Usage and Estimated Savings**: inspect content-free local aggregates, measured indexing stages and supported host resource counters, including process CPU. Overlapping/unavailable measurements stay unknown. Signed context estimates are not billed savings; optional CLI imports, local tokenizers and supplied price scenarios retain their labels.
- **Set Up Coding Agent**: preview a scoped configuration for Claude Code, Codex, Cursor, Gemini CLI, Cline, Roo, Junie, Copilot CLI, OpenCode or Kilo. Amazon Q and Windsurf/Devin get connection details and version-specific setup guidance. AIRK never overwrites their settings or enables blanket tool approval.
- **Verify Native MCP Connection**: check the actual native handshake, fourteen tools and collection status. This does not prove another agent has enabled the server.

`airk.storage.path` and `airk.storage.maxBytes` are machine settings. The optional byte budget pauses automatic indexing after existing collection usage reaches the limit; manual commands may exceed it. It is a soft limit, not a filesystem quota. No collections are deleted automatically. Changing storage requires reconnecting already-running agents. Helper paths cached per version/digest survive extension directory replacement; refresh agent configuration after helper upgrades.

Filesystem events coalesce in the editor; submitted batches enter a durable native queue shared by hosts using the same data root. Pending/completed revisions, pause and failure state survive helper exits. Repeated clicks join the active pass; later changes remain queued. On-save mode does not automatically index external agent edits. Idle mode waits for editor activity as well as filesystem quiet. Coverage scans run separately after changes, after indexing and periodically; they do not restart MCP clients.

Retrieval uses persistent Tantivy lexical indexes, SQLite exact facts/relationships and optional Microsoft DiskANN search through AIRK's disk file provider. Deliberately provisioned, digest-checked word-vector assets or CPU ONNX transformer models enable vector/hybrid queries. The ONNX adapter uses a local BERT WordPiece tokenizer; a pinned MiniLM recipe and small real-model tests are documented. Runtime performs no model download or remote fallback, and production-corpus semantic quality still requires qualification. Code evidence includes scopes/signatures/call/framework candidates. Optional source/build-bound SCIP imports and explicit local LSP runs can add provider reference/type/implementation/call assertions; Ctags adds syntax declarations. These native CLI operations require separately provisioned providers and do not establish universal resolution. Use Index Now after upgrading to migrate older state.

The native helper streams artifacts into content-addressed storage and supports source aliases, shared logical-byte quotas, configured ingestion bounds and explicit verified data-root relocation through the CLI. Default collection bounds are 16 MiB per source, 64 MiB eligible source, 128 MiB extracted text and 10,000 files; larger settings require appropriate storage and qualification. The editor's storage picker retains its existing selection behavior. Explicit CLI OCR uses local Tesseract assets and Poppler for selected PDF pages, or PNG/JPEG/BMP input, retaining source hashes and page/model provenance. OCR can misrecognize text and layout. The editor does not automatically run OCR, LSP or Ctags and has no dedicated provider-setup controls. Skipped-file details appear in AIRK Output. Index contents and aggregate usage stay local; the extension has no runtime telemetry or network service. Your selected agent controls any subsequent provider requests.

Packages target Windows x64, Linux x64 and both Mac architectures. Remote workspaces run the extension/helper on their workspace host and require a matching platform build; remote editor and oldest-OS qualification remain open. Browser-only workspaces cannot run this native helper. Signing, notarization and Marketplace publication are separate release work.

Source, local build instructions and support limits: [AIRK repository](https://github.com/iokaio/airk), [extension documentation](https://github.com/iokaio/airk/blob/main/docs/vscode-extension.md). Licensed under Apache-2.0; bundled dependency notices retain their respective terms.
