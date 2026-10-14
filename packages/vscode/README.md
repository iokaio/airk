# AIRK for VS Code — Beta 0.1.0

AIRK supplies local code and document evidence to coding agents through fourteen read-only MCP tools. This preview bundles its native helper; installation requires no Docker, Rust, Python, service account or API key.

Document facts now include heading context, dates, requirement IDs and modality, numeric bounds/units, qualitative cues and design/architecture/feature/decision candidates across Markdown, text and extracted PDF/DOCX/PPTX content. Use **Index Now** once after updating to populate the new facts in existing collections. Categories are evidence cues, not inferred compliance; ambiguous dates and original page/block/slide locations remain explicit.

Extension identity: **iokaio.airk**. Display name: **AIRK — Artifact Indexing and Retrieval Kernel** (beta). Version **0.1.0** is packaged in the VS Code pre-release channel. The Windows download is **airk-0.1.0-win32-x64.vsix**; Linux and macOS use the matching platform suffix.

Install the VSIX for your platform through **Extensions: Install from VSIX**. Open a folder and grant workspace trust. Select the **AIRK** status item to open its controls. Each workspace folder gets an isolated collection. VS Code discovers the collection through AIRK's MCP provider; use **MCP: List Servers** to enable it.

- **Index Now / Reindex Collection**: incremental indexing or full re-extraction.
- **Choose Indexing Mode**: after edits settle (default), when idle, on save, or manual.
- **Pause / Resume Auto-indexing**: preserve queued edits; an in-progress write finishes.
- **Choose Index Storage Folder**: put indexes on a drive of your choice. Old indexes remain in their old location; this changes storage without migrating or deleting them.
- **Manage Collections and Disk Space**: inspect collection bytes, available drive space, known roots, last indexed time and generation. Remove an unused collection through the separate removal command; source files remain intact.
- **Set Up Coding Agent**: preview a scoped configuration for Claude Code, Codex, Cursor, Gemini CLI, Cline, Roo, Junie, Copilot CLI, OpenCode or Kilo. Amazon Q and Windsurf/Devin get connection details and version-specific setup guidance. AIRK never overwrites their settings or enables blanket tool approval.
- **Verify Native MCP Connection**: check the actual native handshake, fourteen tools and collection status. This does not prove another agent has enabled the server.

`airk.storage.path` and `airk.storage.maxBytes` are machine settings. The optional byte budget pauses automatic indexing after existing collection usage reaches the limit; manual commands may exceed it. It is a soft limit, not a filesystem quota. No collections are deleted automatically. Changing storage requires reconnecting already-running agents. Helper paths cached per version/digest survive extension directory replacement; refresh agent configuration after helper upgrades.

Indexing jobs run serially within an extension host. Native writer locks prevent overlapping collection writes across windows, but there is no machine-wide scheduler yet. Filesystem events are coalesced; changes arriving during indexing are queued for another pass. On-save mode does not automatically index external agent edits. Idle mode waits for editor activity as well as filesystem quiet.

Current retrieval uses embedded SQLite exact facts/relationships and an in-memory lexical map. Code evidence includes scopes/signatures/call/framework candidates; optional producer-bound SCIP imports add reference/implementation assertions. Use Index Now after upgrading to migrate legacy JSON state. DiskANN, local embeddings, OCR, live LSP/Ctags, old-generation pruning and automatic storage migration remain planned. The bootstrap corpus limits remain 1 MiB per source, 64 MiB per workspace and 10,000 files. Skipped files appear in AIRK Output. Workspace roots and index contents stay local; there is no telemetry or network service in the extension runtime.

Windows x64, Linux x64 and both Mac architectures have separate packages. Remote workspaces run the extension/helper on their workspace host and require a matching platform build; remote editor and oldest-OS qualification remain open. Browser-only workspaces cannot run this native helper. Signing, notarization and Marketplace publication are separate release work.

Source, local build instructions and support limits: [AIRK repository](https://github.com/iokaio/airk), [extension documentation](https://github.com/iokaio/airk/blob/main/docs/vscode-extension.md). Licensed under Apache-2.0; bundled dependency notices retain their respective terms.
