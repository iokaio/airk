# AIRK — Artifact Indexing and Retrieval Kernel, the Local MCP Power Tool for Coding Agents

**Give Claude Code, Codex, Gemini CLI, GitHub Copilot and other MCP-capable agents a shared, local index of your workspace.** AIRK returns small, cited passages and facts, each with its file path, content hash and original location, instead of whole files. Your agent does the reasoning; you can check exactly what it used.

Everything runs on your machine. There is no cloud service, API key, Docker, Rust or Python to install: the native helper is bundled in the extension.

> **Beta 0.2.1.** Packages target Windows x64, Linux x64 and macOS (Intel and Apple Silicon). Large-repository performance, macOS and remote-host qualification, and semantic-search quality are still being measured. See [Known limits](#known-limits).

![Workspace code and documents flow through AIRK's local collection to independently configured Claude Code, Codex, Gemini CLI and other MCP clients.](https://raw.githubusercontent.com/iokaio/airk/main/docs/images/local-evidence.png)

## What your agent can do with it

| You ask your agent… | AIRK returns | Why that helps |
|---|---|---|
| "Where is authentication implemented?" | Matching passages, syntax definitions, scopes and signatures, each with a path and line range | The agent starts from named locations instead of reading the repository. Syntax facts are labeled as syntax, not as resolved compiler references. |
| "What response-time limit does this design require?" | Requirement candidates from Markdown, PDF, DOCX, XLSX or PPTX with the original page, paragraph, table row, worksheet cell or slide | You can open the exact source and check the wording. A requirement cue is evidence, not an inferred obligation. |
| "Is this answer based on current code?" | The indexed revision and source hashes, recent indexed changes, and live coverage in the status bar | You can tell when evidence came from an older revision and reindex. |

AIRK gives each agent **sixteen read-only MCP tools**:

| Tool | Purpose |
|---|---|
| `airk_search` | Lexical search, plus optional local vector or hybrid search |
| `airk_read_file`, `airk_context` | Bounded line ranges and the lines around a hit |
| `airk_list_files`, `airk_file_info` | Indexed paths; language, parse status, hash and counts for one file |
| `airk_outline`, `airk_symbols`, `airk_imports` | Definitions and imports in a file or across the collection |
| `airk_references` | Exact textual occurrences of an identifier, explicitly not resolved |
| `airk_code_facts`, `airk_relationships` | Scopes, signatures and caller/callee/dependency candidates |
| `airk_document_facts` | Requirements, dates, quantities and design/decision candidates from documents |
| `airk_changes`, `airk_history` | What changed between indexed revisions; retained revision identities |
| `airk_status`, `airk_capabilities` | Collection revision and counts; supported formats and their limits |

Every response is size-bounded. The tools cannot write files, run commands, switch collections or reach the network. Full reference: [MCP tools](https://github.com/iokaio/airk/blob/main/docs/mcp.md).

## Supported content

- **Code:** native grammar profiles for Rust, Python, JavaScript/TypeScript (including JSX/TSX), Go, Java, Kotlin, C, C++, C#, Swift, Ruby, PHP, Dart, Scala, SQL, Bash and PowerShell, many other general-purpose languages, and specialist profiles such as COBOL, Fortran, PL/SQL and T-SQL.
- **Markup, configuration and build files:** Markdown/MDX, Jupyter notebooks, JSON/YAML/TOML, HTML with Vue/Svelte/Astro/Razor components, CSS/SCSS, XML/SVG, CSV/TSV, INI, Dockerfile, Terraform/HCL, Protobuf, GraphQL, CMake, Make, MSBuild, GitHub Actions, Compose, Bazel, Gradle, Kubernetes, OpenAPI and common package manifests.
- **Documents:** PDF (text layer), DOCX (paragraphs and tables), XLSX (worksheet rows and cells, with stored formula results) and PPTX (slide text).
- **Anything else** that is UTF-8 text gets plain lexical search.

Document extraction keeps heading context, dates, requirement IDs and modality (must/should/may), numeric bounds with units, and design, architecture, feature and decision candidates. Spreadsheet formulas are stored, never recalculated. See the [format matrix](https://github.com/iokaio/airk/blob/main/docs/formats.md) for per-format details and limits.

## Get started

1. **Install the package for your platform** with **Extensions: Install from VSIX…**. Files are named `airk-0.2.1-<platform>.vsix` (`win32-x64`, `linux-x64`, `darwin-x64` or `darwin-arm64`). Check whether the package is from the preview or stable channel before installing.
2. **Open a folder and trust it.** AIRK does not start its helper or read files in an untrusted window.
3. **Index.** Click the **AIRK** status-bar item and choose **Index Now**, or run **AIRK: Index Now**. The status item then shows measured coverage, for example `AIRK: 97%`, and its tooltip lists stale, missing and failed files.
4. **Connect an agent.**
   - *VS Code and Copilot Chat:* AIRK registers each trusted workspace folder as an MCP server. Run **MCP: List Servers** and enable it.
   - *Claude Code, Codex, Gemini CLI, Cursor, Cline, Roo Code, Junie, Copilot CLI, OpenCode or Kilo Code:* run **AIRK: Set Up Coding Agent**, pick the client, and merge the previewed entry into that client's MCP configuration. Amazon Q and Windsurf/Devin get the command and arguments plus setup guidance. AIRK never writes another agent's settings or approves its tools.
   - *Google Antigravity:* choose `gemini` to generate compatible JSON, then use Antigravity's configuration location below. There is no dedicated Antigravity picker entry yet.
5. **Verify.** **AIRK: Verify Native MCP Connection** confirms that the helper starts, lists its tools and reports collection status. Then ask your agent about a file you know and check that its answer cites a path. Verification proves AIRK is healthy; it does not prove that a particular agent has enabled the server.

The **Get Started with AIRK** walkthrough (**Help › Welcome**) links these steps.

![Saved files are indexed into a complete published revision, then an agent retrieves bounded evidence. Coverage, controls and citations make freshness and scope visible.](https://raw.githubusercontent.com/iokaio/airk/main/docs/images/index-to-context.png)

These illustrations are explanatory diagrams, not screenshots.

## Coding agent setup

These instructions are included in the installed extension's **Details** page. The configuration routes below match AIRK's setup previews; they do not establish that every client/version has been tested end to end. Client paths and controls can change, so each route includes its official reference.

### Prepare and share the correct collection

1. Open and trust the intended workspace folder, choose storage if needed, and run **AIRK: Index Now**.
2. Run **AIRK: Set Up Coding Agent** from the Command Palette. Select the workspace folder if prompted, then the client choice listed below. AIRK indexes the selected collection and opens an untitled JSON or TOML preview. **Copy configuration** copies that preview without installing it.
3. Merge only the generated server entry into the client's configuration. Preserve other servers, settings, comments and tool policies. Do not paste a second top-level `mcpServers`, `servers` or `mcp` object into an existing file.
4. Keep the generated executable path and all `--root`, `--data-dir` and `--collection` arguments. The bundled helper need not be on PATH. Different agents share the extension's index only when these values match; a separate CLI setup using defaults can create a different collection.
5. Reload/enable the server in the client and complete its normal trust and tool-approval prompts. Then follow [Verify the agent connection](#verify-the-agent-connection).

Prefer project configuration where the client supports it. `~` below means your user home directory. For user-wide settings, retain the generated collection-specific server name so another workspace cannot overwrite its entry. AIRK always reads the configured collection; changing the editor's active folder does not redirect an existing MCP connection. After changing AIRK's storage location, regenerate the affected client entries and reconnect.

### VS Code / GitHub Copilot Chat

AIRK registers a native stdio MCP definition for each trusted workspace folder. Run **MCP: List Servers**, choose the AIRK entry for the intended folder, and start/enable it using VS Code's controls. Complete any trust or tool-approval prompts in Copilot Chat.

If automatic registration is unavailable, choose `vscode` in **Set Up Coding Agent** and merge the generated entry under `servers` in the workspace's `.vscode/mcp.json`. Avoid enabling both a manual duplicate and the registered provider for the same collection. Registration in VS Code does not configure a separate Codex, Claude Code or other agent extension. [VS Code MCP guide](https://code.visualstudio.com/docs/copilot/customization/mcp-servers)

### Claude Code

Choose `claude`. Merge the generated entry under `mcpServers` in `.mcp.json` at the project root. Load/approve the project's server in Claude Code, then use its `/mcp` controls to confirm the connection. For user or local scope, follow Claude Code's own registration controls rather than moving the project file blindly. [Claude Code MCP guide](https://code.claude.com/docs/en/mcp)

### Codex CLI / IDE extension / desktop

Choose `codex`. Merge the generated `[mcp_servers.<name>]` table into a trusted project's `.codex/config.toml` or your user `~/.codex/config.toml`. Keep its `command` string and `args` array together; this preview is TOML, not JSON. Reload the client and check its MCP server list. Codex reads its own configuration independently of VS Code's MCP registry. [Codex MCP guide](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)

### Gemini CLI

Choose `gemini`. Merge the generated entry under `mcpServers` in the project's `.gemini/settings.json`, or the user counterpart `~/.gemini/settings.json`. Preserve the rest of Gemini's settings and trust/allowlist policy. Reload the client and use `/mcp` to inspect AIRK's connection. [Gemini CLI MCP guide](https://geminicli.com/docs/tools/mcp-server/)

### Google Antigravity IDE / CLI

Choose `gemini` to obtain the compatible `mcpServers` JSON. This choice only selects the output format: ignore the Gemini-specific destination shown by that picker notification. Merge the entry into the workspace's `.agents/mcp_config.json` or the global `~/.gemini/config/mcp_config.json`, not Gemini CLI's settings file.

In Antigravity IDE, use the agent panel's **… → MCP Servers → Manage MCP Servers → View raw config** to locate the active file. If the installed version uses a different location, follow that view. Reload/enable AIRK; Antigravity CLI provides status, reload and connection logs through `/mcp`. Keep the client's approval policy.

No HTTP bridge, API key or Google Cloud credentials are needed for AIRK's local stdio server. There is no dedicated Antigravity entry in AIRK's picker yet. This manual configuration has not been qualified in an installed Antigravity session; use the connection checks below. [Google MCP guide](https://antigravity.google/docs/mcp)

### Cursor

Choose `cursor`. Merge the generated `mcpServers` entry into the project's `.cursor/mcp.json` or your user `~/.cursor/mcp.json`. Enable/reload AIRK in Cursor's MCP settings and inspect its tools. AIRK currently provides a configuration preview for Cursor; it does not directly register through Cursor's separate extension API. [Cursor MCP guide](https://prod.cursor.com/docs/mcp)

### Cline CLI / IDE extension

Choose `cline`. In the IDE extension, open **Configure MCP Servers** in Cline and merge the `mcpServers` entry into the file it opens. Do not guess a VS Code extension-storage path. Cline CLI's documented default is `~/.cline/data/settings/cline_mcp_settings.json`, with a configurable path. Reload/enable the server in the client you actually use, then check its connection and tool list. [Cline MCP guide](https://docs.cline.bot/mcp/mcp-overview)

### Roo Code

Choose `roo`. Use Roo's **Edit Project MCP** controls and merge the entry under `mcpServers` in `.roo/mcp.json`. Project configuration overrides the global `mcp_settings.json`; check for a same-named global entry before troubleshooting an unexpected connection. Enable/reload AIRK in Roo and confirm the intended collection. [Roo Code MCP guide](https://roocodeinc.github.io/Roo-Code/features/mcp/using-mcp-in-roo/)

### JetBrains Junie / Junie CLI

Choose `junie`. Merge the `mcpServers` entry into the project's `.junie/mcp/mcp.json` or the user counterpart. Use Junie's MCP controls, including `/mcp` where available, to reload and verify it. AIRK's VS Code extension supplies the launch configuration; registration and approval remain in Junie. [Junie MCP guide](https://junie.jetbrains.com/docs/junie-cli-mcp-configuration.html)

### GitHub Copilot CLI

Choose `copilot-cli`. Merge the generated entry into the active MCP configuration, such as `~/.copilot/mcp-config.json`. Supported project configuration includes `.mcp.json` and `.github/mcp.json`; check the installed CLI's precedence before creating a duplicate. Preserve the generated local transport fields and review its `tools` selection under your existing policy. Reload/check the CLI's MCP servers; Copilot Chat's VS Code registration does not configure this separate CLI. [Copilot CLI MCP guide](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers)

### OpenCode / OpenCode V2

Check the installed major version before choosing the preview:

- Choose `opencode` for the stable schema: merge into `opencode.json` under `mcp.<name>`.
- Choose `opencode-v2` when the installed version uses the V2 schema: merge under `mcp.servers.<name>` in that version's configuration.

Both previews use `type: "local"` and a `command` array containing the executable followed by each argument. Keep that array intact; do not convert it to a single shell command or a `mcpServers` entry. Reload and check AIRK in the client's MCP controls. [Stable guide](https://opencode.ai/docs/en/mcp-servers/) · [V2 guide](https://dev.opencode.ai/v2/docs/mcp-servers/)

### Kilo Code

Choose `kilo`. Merge the local entry under `mcp` in the active `kilo.json`/`kilo.jsonc` or `.kilo/kilo.json`/`.kilo/kilo.jsonc`. Preserve the executable-plus-arguments `command` array and existing JSONC comments. Check **Settings → MCP** to enable/verify AIRK. Older Roo-derived paths are not interchangeable with this configuration; confirm the installed version. [Kilo Code MCP guide](https://kilo.ai/docs/automate/mcp/using-in-kilo-code)

### Amazon Q Developer IDE

Choose `amazon-q`. The preview contains **connection details**, not a complete agent settings file. In Amazon Q, use **Tools → Add stdio server**, give the server a collection-specific name, and supply the preview's executable and separate arguments. Select project scope where appropriate and save/enable it through the UI.

Amazon Q manages project `.amazonq/default.json` or user `~/.aws/amazonq/default.json`; do not replace either with AIRK's connection-details JSON. Legacy `mcp.json` support is separately controlled. Confirm tool discovery in Amazon Q after saving. [Amazon Q MCP guide](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/mcp-ide.html)

### Windsurf legacy Cascade / Devin Desktop

Choose `windsurf` to get the stdio executable and arguments. Identify the installed agent/version, open its MCP configuration UI, and register a local server using those details. This preview is not a complete client configuration.

Legacy Cascade uses `mcpServers` in its own `mcp_config.json`; Devin Local uses a different CLI configuration. Use the file/schema opened or documented by the installed agent rather than assuming an old `~/.codeium` path or copying one agent's settings into another. Enable/reload the server there and verify its tools. [Current official Cascade guide](https://docs.devin.ai/desktop/cascade/mcp)

### Verify the agent connection

Run **AIRK: Verify Native MCP Connection** for the intended folder. A successful result confirms AIRK's stdio handshake, sixteen-tool inventory and collection status. It does not prove the selected coding agent has loaded or approved the entry.

In that agent, explicitly request `airk_status` and `airk_capabilities` from the configured AIRK server. Confirm the collection, then request a lexical `airk_search` for a known indexed phrase and check its source path and original location. Use **AIRK: Index Now** after edits if automatic indexing is paused or set to Manual; MCP retrieval does not initiate reindexing.

If no tools appear, check the active client config, server enablement, trust, executable permissions and connection logs. If results come from the wrong folder or are stale, compare the preview's root/storage/collection arguments and AIRK's coverage report. After an extension upgrade, regenerate the preview if the helper path changed. For SSH/WSL/container clients, the command and index paths must be accessible on the host running that MCP process; those setups remain unqualified.

## How indexing works

- **One collection per folder.** Each workspace folder gets its own isolated collection. Several agents can share it by using the same root, storage folder and collection ID. Agents keep reading the current index while a new revision is being built.
- **Edits are batched.** By default AIRK waits until file changes have been quiet for 2 seconds, then indexes incrementally. That includes edits made by coding agents outside the editor. **Reindex Collection** re-extracts everything.
- **Only saved files are indexed.** The status tooltip counts open documents with unsaved changes.
- **Ignore rules are respected.** AIRK honors `.gitignore` and an optional `.airkignore` in the same syntax. It always skips common dependency, VCS and secret paths such as `.git`, `node_modules`, `target`, `.venv`, `.env*`, `.ssh`, `*.pem` and `*.key`. It is not a secret scanner; review what you index.
- **Storage is yours to place.** Indexes live in VS Code's per-user extension storage unless you choose another folder. Changing the folder builds new indexes there and leaves old ones in place until you remove them.

The **AIRK** view in the Activity Bar lists active and stored collections with indexing state, coverage, disk use and last index time. Inline buttons index or reindex an active collection, pause auto-indexing for the window, or remove an inactive collection after confirmation.

## Commands

Clicking the status-bar item or running **AIRK: Open Controls** opens all commands in one grouped menu. Every command is also in the Command Palette under **AIRK:**.

**Indexing**

| Command | What it does |
|---|---|
| Index Now | Incremental update of the active folder's collection. Repeated runs join the pass already in progress. Works while paused or in Manual mode. |
| Reindex Collection (Full Re-extraction) | Re-extracts every eligible file. Use after upgrading AIRK or when facts look stale. |
| Choose Indexing Mode… | After edits settle (default), When idle, On save, or Manual. |
| Pause / Resume Auto-indexing | Stops automatic jobs while keeping queued changes; a running write finishes first. Pause state survives restarts. |

**Setup and reports**

| Command | What it does |
|---|---|
| Set Up Coding Agent (Claude Code, Codex, Gemini CLI…) | Indexes the folder if needed, then opens a configuration preview for the chosen client and tells you where to merge it. You can copy it to the clipboard; nothing is written for you. |
| Verify Native MCP Connection | Runs a real stdio handshake with the helper, lists its tools and reads collection status. |
| Show Collection Status and Coverage | Report of current, stale, missing, extraction-failed and capacity-blocked files, scan time and completeness. An incomplete scan shows coverage as unknown. |
| Show Local Usage Report | Content-free local aggregates: calls, bytes returned, latency, indexing stages and disk growth. Context-reduction figures are estimates against a counterfactual, not measured provider tokens or billed savings. |
| Show Output Log | Skipped files with reasons, extraction failures and each published revision. |

Reports open as formatted panels; **Open as JSON** shows the full native output.

**Collections and storage**

| Command | What it does |
|---|---|
| Choose Index Storage Folder… | Pick a folder, for example on a larger or faster drive. Existing indexes are not moved or deleted. Restart connected agents afterwards. |
| Manage Collections and Disk Space | Every collection in a storage folder with size, root, last index time and revision, plus free space on the drive. |
| Remove Unneeded Collection… | Deletes one collection's derived index; source files are untouched. Refused while the collection is open in this window or still in use by an agent. |
| Trim Obsolete Index Generations… | Previews reclaimable space from superseded index builds, then trims after you confirm. The current index and anything an agent is still reading are kept. |

**Retention, history and extraction** (advanced)

| Command | What it does |
|---|---|
| Show Retention Status · Choose Retention Policy… · Run Retention Maintenance… | Inspect and set whether superseded searchable revisions are cleaned up manually or automatically. Every change is previewed and confirmed. New collections default to automatic; collections from older versions stay manual until you change them. |
| Show History Status · Choose History Policy… · Run History Maintenance… | Inspect and bound the retained history of committed index revisions. |
| List Retained Publication Identities · Show / Create / Remove Checkpoint… | Pin a committed revision under a label so it is kept, or release a pin. Checkpoints pin indexed evidence, not live source files. |
| Preview Logical History Purge… | Plan removal of selected retained revisions, then confirm. This is a logical purge, not secure erasure. |
| Show Extraction Policy · Configure Extraction Policy… | Inspect, or validate and save, a JSON policy that chooses which built-in extractor handles which files. A saved policy applies on the next successful index. |

## Settings

| Setting | Default | Purpose |
|---|---|---|
| `airk.index.mode` | `debounced` | When automatic indexing runs: after edits settle, when idle, on save, or manually. |
| `airk.index.quietMs` | `2000` | How long file changes must be quiet before an automatic batch starts. |
| `airk.index.idleMs` | `10000` | *When idle* mode: how long the editor must also be free of typing and selection changes. |
| `airk.index.maxDelayMs` | `30000` | *After edits settle* and *On save*: the longest continuous changes can postpone a batch. |
| `airk.index.minIntervalMs` | `5000` | Minimum gap between automatic jobs. |
| `airk.storage.path` | per-user storage | Machine-scoped index folder. Read from user settings only. |
| `airk.storage.maxBytes` | `0` (off) | Soft budget for index data in the storage folder. At the limit automatic indexing stops; Index Now can still exceed it; nothing is deleted automatically. |

## Privacy and network

AIRK has no telemetry and no network service. Indexes and usage aggregates stay on this machine. Optional vector search uses only model files you provision through the AIRK CLI; nothing is downloaded, and there is no remote fallback.

What happens after AIRK returns evidence is up to the agent you connected. That agent decides whether to send retrieved text to its model provider, under its own settings. AIRK cannot see or control that.

## Requirements

- VS Code 1.140 or later, in a trusted local folder.
- Windows x64, Linux x64, or macOS on Intel or Apple Silicon. Install the package that matches the machine hosting the workspace.
- Remote workspaces (SSH, WSL, Dev Containers, Codespaces) run the helper on the remote host, which needs its matching package. These setups are not yet qualified.
- Browser-only VS Code (vscode.dev, github.dev) cannot run the native helper.

## Known limits

- **Size.** Default collection bounds are 16 MiB per source file, 64 MiB of eligible source, 128 MiB of extracted text and 10,000 files. Larger limits can be configured through the CLI but have not been performance-qualified.
- **Code facts are syntax, not semantics.** Definitions, imports, references and relationships come from grammar parsing and are labeled as candidates. Resolved references require an optional SCIP import or a local LSP run through the CLI. Textual references include matches in strings and comments.
- **Vector search is optional and unqualified.** It needs local model files you provision. Relevance on real codebases has not been measured.
- **Not extracted:** scanned or image-only PDFs, encrypted documents, legacy DOC/PPT/XLS, DOCX headers, footers and comments, PPTX notes, and XLSX charts and pivot caches. OCR is available through the CLI for selected pages; the extension does not run it.
- **Unsigned preview builds.** Signing, macOS notarization and Marketplace publication are pending.

More detail: [extension behavior and limits](https://github.com/iokaio/airk/blob/main/docs/vscode-extension.md) · [roadmap](https://github.com/iokaio/airk/blob/main/docs/roadmap.md).

## Troubleshooting

| You see | Likely cause | What to do |
|---|---|---|
| **AIRK: Trust required** | The workspace is not trusted | Run **Workspaces: Manage Workspace Trust** and trust the folder. |
| **AIRK: Coverage unknown** | A scan has not finished, or files changed since the last scan | Wait a few seconds, or run **Index Now**. |
| **AIRK: Not indexed** | The folder has no published index yet | Run **Index Now**. |
| **AIRK: Storage budget** | `airk.storage.maxBytes` was reached | Trim generations, remove unused collections or raise the budget, then **Resume Auto-indexing**. |
| **AIRK: Paused** | Auto-indexing is paused | Run **Resume Auto-indexing**, or use **Index Now**. |
| **AIRK: Failed** | The last indexing job failed | Open **Show Output Log** for the reason. The previous index stays available. |
| An agent's AIRK server will not start or shows an old index | Its configuration points at an earlier helper or storage folder | Run **Set Up Coding Agent** again, replace the old entry and restart the agent. |
| A file never appears as indexed | Too large, ignored, unsupported or extraction failed | Check **Show Output Log** for the skip reason. |

## Learn more

- [Repository](https://github.com/iokaio/airk) · [Documentation](https://github.com/iokaio/airk/blob/main/docs/README.md) · [Agent setup matrix](https://github.com/iokaio/airk/blob/main/docs/agent-integration.md)
- [Report a bug](https://github.com/iokaio/airk/issues) · [Support](https://github.com/iokaio/airk/blob/main/SUPPORT.md) · [Security policy](https://github.com/iokaio/airk/blob/main/SECURITY.md)

AIRK stands for Artifact Indexing and Retrieval Kernel. Licensed under Apache-2.0; bundled dependency notices in THIRD_PARTY_NOTICES.md retain their respective terms.
