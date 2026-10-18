# AIRK — Artifact Indexing and Retrieval Kernel, the Local MCP Power Tool for Coding Agents

**Give Claude Code, Codex, Gemini CLI/Antigravity, GitHub Copilot and other MCP-capable agents a shared, local index of your workspace.** AIRK returns small, cited passages and facts, each with its file path, content hash and original location, instead of whole files. Your agent does the reasoning; you can check exactly what it used.

Everything runs on your machine. There is no cloud service, API key, Docker, Rust or Python to install: the native helper is bundled in the extension.

> **Version 1.1.2 · Stable channel.** Packages target Windows x64, Linux x64 and macOS (Intel and Apple Silicon). Large-repository performance, macOS and remote-host qualification, and semantic-search quality are still being measured. See [Known limits](#known-limits).

## What's new in 1.1.0–1.1.2

- **Larger workspaces:** machine-aware ingestion limits, measured source/extracted sizes and reviewed limit increases with a single retry.
- **Collection controls:** a dashboard with coverage, MCP helper checks, indexing and pause controls, and Essential/Balanced/Detailed evidence selection. New collections default to Balanced.
- **Storage and recovery:** smaller fact catalogs, reuse of unchanged native rows and faster repeated exact queries; adjustable 100 GiB collection/500 GiB root quota defaults; retention targeting 1–9 generations, active-only for new collections; recovery for interrupted publication. Existing saved policies remain in force.
- **Agent setup:** persistent guides can create missing Codex project configuration, install Claude Code/Gemini CLI/Antigravity entries while preserving settings, or enable Copilot registration. Verify the agent's actual tools separately from helper health.

The [combined October 17 release notes](https://github.com/iokaio/airk/blob/main/docs/releases-2026-10-17.md) include upgrade steps, measured Windows results and remaining qualification limits. Download matching packages from [GitHub Releases](https://github.com/iokaio/airk/releases/tag/v1.1.2); Marketplace publication is a separate process.

![Workspace code and documents flow through AIRK's local collection to independently configured Claude Code, Codex, Gemini CLI/Antigravity and other MCP clients.](https://raw.githubusercontent.com/iokaio/airk/main/docs/images/local-evidence.png)

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

1. **Install the package for your platform** with **Extensions: Install from VSIX…**. Files are named `airk-1.1.2-<platform>.vsix` (`win32-x64`, `linux-x64`, `darwin-x64` or `darwin-arm64`). Choose the stable 1.1.2 package. If you previously opted into pre-release versions, use **Switch to Release Version** in the extension menu to stay on stable updates.
2. **Open a folder and trust it.** AIRK does not start its helper or read files in an untrusted window.
3. **Index.** Click the **AIRK** status-bar item and choose **Index Now**, or run **AIRK: Index Now**. The status item then shows measured coverage, for example `AIRK: 97%`, and its tooltip lists stale, missing and failed files.
4. **Connect an agent.**
   - *VS Code and Copilot Chat:* choose **VS Code / GitHub Copilot Chat** in **AIRK: Connect Coding Agent…**, then **Enable AIRK for Copilot**. Select the collection in the opened MCP controls and complete any start/trust prompts.
   - *Claude Code, Codex, Gemini CLI/Antigravity, Cursor, Cline, Roo Code, Junie, Copilot CLI, OpenCode or Kilo Code:* run **AIRK: Connect Coding Agent…** and choose the agent by name. The persistent guide shows its configuration, reload instructions and a verification prompt. Codex can create a missing project configuration. Claude Code, Gemini CLI and Google Antigravity offer project configuration installation that preserves existing settings. Other clients receive configuration to merge. AIRK does not approve tools for another agent.
5. **Verify.** **AIRK: Check AIRK MCP Helper** confirms that the helper starts, lists its tools and reports collection status. Then ask your agent about a file you know and check that its answer cites a path. Verification proves AIRK is healthy; it does not prove that a particular agent has enabled the server.

The **Get Started with AIRK** walkthrough (**Help › Welcome**) links these steps.

Automatic indexing starts in trusted workspaces using per-user storage by default. To use another drive, run **AIRK: Choose Index Storage Folder…**. Opening the picker holds new indexing jobs. If a pass is already running, AIRK shows progress while it finishes before switching folders. Cancelling the picker keeps the current location and restores scheduling; an existing pause remains paused. Existing indexes stay in their previous location. Restart connected agents after changing storage.

![Saved files are indexed into a complete published revision, then an agent retrieves bounded evidence. Coverage, controls and citations make freshness and scope visible.](https://raw.githubusercontent.com/iokaio/airk/main/docs/images/index-to-context.png)

These illustrations are explanatory diagrams, not screenshots.

## Coding agent setup

These instructions are included in the installed extension's **Details** page. The configuration routes below match AIRK's setup previews; they do not establish that every client/version has been tested end to end. Client paths and controls can change, so each route includes its official reference.

### Prepare and share the correct collection

1. Open and trust the intended workspace folder, choose storage if needed, and run **AIRK: Index Now**.
2. Run **AIRK: Connect Coding Agent…** from the Command Palette. Choose the agent, then the workspace folder if prompted. The guide shows the selected root, storage directory, collection, configuration and reload steps. Opening the guide does not force indexing. **Copy configuration** copies the entry without installing it. Use **Check AIRK helper** to index and check the native server.
3. For Codex, **Create Codex project configuration** creates a missing `.codex/config.toml` in the selected workspace. Existing files, unsaved edits and redirected `.codex` directories are preserved; use **Open existing Codex project configuration** and merge the entry yourself. On remote extension hosts, use manual setup on the host running Codex. For Claude Code, Gemini CLI and Google Antigravity, **Install for…** adds the entry to the project file, preserving comments, other servers and approval policies. A conflicting AIRK entry, unsaved file or redirected path requires manual review. Remote hosts use manual setup. For remaining clients, merge only the generated server entry into the client's configuration. Preserve other servers, settings, comments and tool policies. Do not paste a second top-level `mcpServers`, `servers` or `mcp` object into an existing file.
4. Keep the generated executable path and all `--root`, `--data-dir` and `--collection` arguments. The bundled helper need not be on PATH. Different agents share the extension's index only when these values match; a separate CLI setup using defaults can create a different collection.
5. Reload/enable the server in the client and complete its normal trust and tool-approval prompts. Then follow [Verify the agent connection](#verify-the-agent-connection).

Prefer project configuration where the client supports it. `~` below means your user home directory. For user-wide settings, retain the generated collection-specific server name so another workspace cannot overwrite its entry. AIRK always reads the configured collection; changing the editor's active folder does not redirect an existing MCP connection. After changing AIRK's storage location, regenerate the affected client entries and reconnect.

### VS Code / GitHub Copilot Chat

Choose **VS Code / GitHub Copilot Chat**, then **Enable AIRK for Copilot**. This enables AIRK’s registration for open workspace collections and opens **MCP: List Servers**. There, choose the AIRK entry for the intended folder, and start/enable it using VS Code's controls. Complete any trust or tool-approval prompts in Copilot Chat.

If automatic registration is unavailable, choose **VS Code / GitHub Copilot Chat** in **Connect Coding Agent…** and merge the generated entry under `servers` in the workspace's `.vscode/mcp.json`. Avoid enabling both a manual duplicate and the registered provider for the same collection. Registration in VS Code does not configure a separate Codex, Claude Code or other agent extension. [VS Code MCP guide](https://code.visualstudio.com/docs/copilot/customization/mcp-servers)

### Claude Code

Choose **Claude Code**, then **Install for Claude Code** to add AIRK under `mcpServers` in the project’s `.mcp.json`, preserving existing settings. Restart Claude Code. Load/approve the project's server in Claude Code, then use its `/mcp` controls to confirm the connection. For user or local scope, follow Claude Code's own registration controls rather than moving the project file blindly. [Claude Code MCP guide](https://code.claude.com/docs/en/mcp)

### Codex CLI / IDE extension / desktop

Choose **Codex**. Use **Create Codex project configuration** for a missing project file, or open an existing project file beside the guide. Merge the generated `[mcp_servers.<name>]` table into a trusted project's `.codex/config.toml` or your user `~/.codex/config.toml`. Keep its `command` string and `args` array together; this preview is TOML, not JSON. Reload the client and check its MCP server list. Codex reads its own configuration independently of VS Code's MCP registry. [Codex MCP guide](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)

### Gemini CLI

Choose **Gemini CLI**, then **Install for Gemini CLI** to add AIRK under `mcpServers` in the project's `.gemini/settings.json`. For user scope, merge manually into `~/.gemini/settings.json`. Preserve the rest of Gemini's settings and trust/allowlist policy. Reload the client and use `/mcp` to inspect AIRK's connection. [Gemini CLI MCP guide](https://geminicli.com/docs/tools/mcp-server/)

### Google Antigravity IDE / CLI

Choose **Google Antigravity**, then **Install for Google Antigravity** to add AIRK to the workspace's `.agents/mcp_config.json`. For global scope, manually merge the preview into `~/.gemini/config/mcp_config.json`. Gemini CLI uses a separate configuration file.

In Antigravity IDE, use the agent panel's **… → MCP Servers → Manage MCP Servers → View raw config** to locate the active file. If the installed version uses a different location, follow that view. Reload/enable AIRK; Antigravity CLI provides status, reload and connection logs through `/mcp`. Keep the client's approval policy.

No HTTP bridge, API key or Google Cloud credentials are needed for AIRK's local stdio server. Configuration installation does not establish a live connection. An installed Antigravity session has not yet been qualified; use the connection checks below. [Google MCP guide](https://antigravity.google/docs/mcp)

### Cursor

Choose **Cursor**. Merge the generated `mcpServers` entry into the project's `.cursor/mcp.json` or your user `~/.cursor/mcp.json`. Enable/reload AIRK in Cursor's MCP settings and inspect its tools. AIRK currently provides a configuration preview for Cursor; it does not directly register through Cursor's separate extension API. [Cursor MCP guide](https://prod.cursor.com/docs/mcp)

### Cline CLI / IDE extension

Choose **Cline**. In the IDE extension, open **Configure MCP Servers** in Cline and merge the `mcpServers` entry into the file it opens. Do not guess a VS Code extension-storage path. Cline CLI's documented default is `~/.cline/data/settings/cline_mcp_settings.json`, with a configurable path. Reload/enable the server in the client you actually use, then check its connection and tool list. [Cline MCP guide](https://docs.cline.bot/mcp/mcp-overview)

### Roo Code

Choose **Roo Code**. Use Roo's **Edit Project MCP** controls and merge the entry under `mcpServers` in `.roo/mcp.json`. Project configuration overrides the global `mcp_settings.json`; check for a same-named global entry before troubleshooting an unexpected connection. Enable/reload AIRK in Roo and confirm the intended collection. [Roo Code MCP guide](https://roocodeinc.github.io/Roo-Code/features/mcp/using-mcp-in-roo/)

### JetBrains Junie / Junie CLI

Choose **JetBrains Junie**. Merge the `mcpServers` entry into the project's `.junie/mcp/mcp.json` or the user counterpart. Use Junie's MCP controls, including `/mcp` where available, to reload and verify it. AIRK's VS Code extension supplies the launch configuration; registration and approval remain in Junie. [Junie MCP guide](https://junie.jetbrains.com/docs/junie-cli-mcp-configuration.html)

### GitHub Copilot CLI

Choose **GitHub Copilot CLI**. Merge the generated entry into the active MCP configuration, such as `~/.copilot/mcp-config.json`. Supported project configuration includes `.mcp.json` and `.github/mcp.json`; check the installed CLI's precedence before creating a duplicate. Preserve the generated local transport fields and review its `tools` selection under your existing policy. Reload/check the CLI's MCP servers; Copilot Chat's VS Code registration does not configure this separate CLI. [Copilot CLI MCP guide](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers)

### OpenCode / OpenCode V2

Check the installed major version before choosing the preview:

- Choose **OpenCode** for the stable schema: merge into `opencode.json` under `mcp.<name>`.
- Choose **OpenCode V2** when the installed version uses the V2 schema: merge under `mcp.servers.<name>` in that version's configuration.

Both previews use `type: "local"` and a `command` array containing the executable followed by each argument. Keep that array intact; do not convert it to a single shell command or a `mcpServers` entry. Reload and check AIRK in the client's MCP controls. [Stable guide](https://opencode.ai/docs/en/mcp-servers/) · [V2 guide](https://dev.opencode.ai/v2/docs/mcp-servers/)

### Kilo Code

Choose **Kilo Code**. Merge the local entry under `mcp` in the active `kilo.json`/`kilo.jsonc` or `.kilo/kilo.json`/`.kilo/kilo.jsonc`. Preserve the executable-plus-arguments `command` array and existing JSONC comments. Check **Settings → MCP** to enable/verify AIRK. Older Roo-derived paths are not interchangeable with this configuration; confirm the installed version. [Kilo Code MCP guide](https://kilo.ai/docs/automate/mcp/using-in-kilo-code)

### Amazon Q Developer IDE

Choose **Amazon Q Developer**. The preview contains **connection details**, not a complete agent settings file. In Amazon Q, use **Tools → Add stdio server**, give the server a collection-specific name, and supply the preview's executable and separate arguments. Select project scope where appropriate and save/enable it through the UI.

Amazon Q manages project `.amazonq/default.json` or user `~/.aws/amazonq/default.json`; do not replace either with AIRK's connection-details JSON. Legacy `mcp.json` support is separately controlled. Confirm tool discovery in Amazon Q after saving. [Amazon Q MCP guide](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/mcp-ide.html)

### Windsurf legacy Cascade / Devin Desktop

Choose **Windsurf / Devin** to get the stdio executable and arguments. Identify the installed agent/version, open its MCP configuration UI, and register a local server using those details. This preview is not a complete client configuration.

Legacy Cascade uses `mcpServers` in its own `mcp_config.json`; Devin Local uses a different CLI configuration. Use the file/schema opened or documented by the installed agent rather than assuming an old `~/.codeium` path or copying one agent's settings into another. Enable/reload the server there and verify its tools. [Current official Cascade guide](https://docs.devin.ai/desktop/cascade/mcp)

### Verify the agent connection

Each collection’s **AIRK helper check** section shows Not checked, Preparing collection, Checking MCP, Verified with a tool count, or Verification failed with an error. **Check AIRK helper** updates that collection’s index before checking the native handshake, tool inventory and status. The result and last-check time stay in the panel for this editor session; **View details** opens the cached diagnostic result. This verifies the helper, not an agent’s live connection.

Under **Agent connections**, the **VS Code registration** switch controls automatic AIRK server definitions on this extension host (`airk.mcp.registrationEnabled`, a machine-scoped user setting). Turning it off prevents new provider resolutions; it does not pause indexing, remove manual server entries or disable external agents. **Connect coding agent…** (also available through **Manage Agent Connections**) first asks which agent to configure. **VS Code / Copilot MCP controls** opens only VS Code’s server controls. Codex, Claude Code and other coding agents use their own configurations. The guide never labels an agent connected just because configuration was copied/created or its helper check passed.

Run **AIRK: Check AIRK MCP Helper** for the intended folder. A successful result confirms AIRK's stdio handshake, sixteen-tool inventory and collection status. It does not prove the selected coding agent has loaded or approved the entry.

In that agent, explicitly request `airk_status` and `airk_capabilities` from the configured AIRK server. Confirm the collection, then request a lexical `airk_search` for a known indexed phrase and check its source path and original location. Use **AIRK: Index Now** after edits if automatic indexing is paused or set to Manual; MCP retrieval does not initiate reindexing.

If no tools appear, check the active client config, server enablement, trust, executable permissions and connection logs. If results come from the wrong folder or are stale, compare the preview's root/storage/collection arguments and AIRK's coverage report. After an extension upgrade, regenerate the preview if the helper path changed. For SSH/WSL/container clients, the command and index paths must be accessible on the host running that MCP process; those setups remain unqualified.

## How indexing works

- **One collection per folder.** Each workspace folder gets its own isolated collection. Several agents can share it by using the same root, storage folder and collection ID. Agents keep reading the current index while a new revision is being built.
- **Edits are batched.** By default AIRK waits until file changes have been quiet for 2 seconds, then indexes incrementally. That includes edits made by coding agents outside the editor. **Reindex Collection** re-extracts everything.
- **Only saved files are indexed.** The status tooltip counts open documents with unsaved changes.
- **Ignore rules are respected.** AIRK honors `.gitignore` and an optional `.airkignore` in the same syntax. It always skips common dependency, VCS and secret paths such as `.git`, `node_modules`, `target`, `.venv`, `.env*`, `.ssh`, `*.pem` and `*.key`. It is not a secret scanner; review what you index.
- **Storage is yours to place.** Indexes live in VS Code's per-user extension storage unless you choose another folder. Changing the folder builds new indexes there and leaves old ones in place until you remove them.

Open **AIRK** in the Activity Bar for the persistent **Dashboard**. Each workspace card shows indexing state, measured coverage and current/eligible/stale/missing file counts, with **Index now**, **Reindex** and **Details** buttons for that collection. Expand **Storage and last index** for its folder, disk use and last successful index time.

Choose the indexing mode, pause/resume automatic work, choose storage, set up coding agents and open reports directly in the sidebar. Maintenance and extraction controls are grouped in expandable sections. Coverage measures saved-file freshness; it is not an indexing progress estimate. The collapsed **Collections** view includes inactive stored collections and confirmed removal controls.

### Storage budgets and generation retention

The sidebar’s **Storage limits…** control edits the selected collection quota (default **100 GiB**) and shared storage quota (default **500 GiB**). These are ceilings, not disk reservations. Existing saved quotas are preserved. **Generations and cleanup…** chooses automatic or manual retention and **1–9 completed generations**, default **1 (active only)** for new collections. Live readers, checkpoints and selected history can retain additional data. The active index remains intact until a new build commits; reducing retention does not remove that failure protection. Previously retired generations cannot be restored by increasing the count.

Automatic retention trims eligible old generations after publication and before builds with low estimated headroom; storage pressure allows one bounded cleanup and retry. **Reclaim eligible space…** previews manual maintenance using the same protections. If protected data or work budgets prevent recovery, indexing stops with the previous index retained. Review storage limits/retention, then Resume or Index now. New file events do not restart a capacity-blocked job.

### Choose indexing detail

The AIRK sidebar offers **Essential**, **Balanced** (the default for new collections), and **Detailed** for each collection. Essential keeps searchable text and exact citations. Balanced adds declarations, imports, useful structural/document facts and declaration change tracking. Detailed also retains individual calls and document-unit change tracking. Existing collections keep their previous Detailed behavior until changed.

The panel shows the configured and published levels separately. Changing a level rebuilds evidence; the previous index stays available until the rebuild succeeds. Storage quotas, retained generations and optional models are separate controls. Native fact and relationship responses explicitly report omitted detail. See [ingestion limits and index detail](https://github.com/iokaio/airk/blob/main/docs/ingestion-limits.md) for limits and CLI equivalents.

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
| Connect Coding Agent… / Manage Agent Connections | Opens the agent picker and persistent setup guide. Codex can create a missing project config on request; existing settings require manual merging. |
| VS Code / Copilot MCP Controls | Opens VS Code’s own server controls; does not configure other agents. |
| Check AIRK MCP Helper | Runs a real stdio handshake with the helper, lists its tools and reads collection status. |
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

- **Size.** Automatic limits allow up to 256 MiB per source file, 32 MiB extracted text per file and 100,000 files. Eligible source uses one eighth of detected machine memory, capped at 2 GiB (512 MiB when detection is unavailable); extracted text gets twice that allowance. Existing saved policies remain unchanged. UTF-8 files above 4 MiB retain searchable text with partial text-only status, without structural facts. Disk quotas and format limits still apply; larger permitted workloads are not performance-qualified. Use **AIRK: Show Ingestion Limits** to inspect effective limits, memory detection, the storage path, quota and cached coverage totals/largest files. Use **AIRK: Show Collection Status and Coverage** for fresh measurements and **AIRK: Configure Ingestion Limits…** to restore automatic sizing or review/import a six-field limits JSON. When indexing hits a collection budget, **Review larger limit** lets you enter a larger allowance up to the displayed hard ceiling. **Save limit and retry** explains that larger limits may increase indexing time, memory and disk usage, then saves the approved budget and retries once. Cancellation preserves the existing limits; per-file extraction bounds and disk quotas remain enforced. See [ingestion limits](https://github.com/iokaio/airk/blob/main/docs/ingestion-limits.md) for the JSON format, CLI commands and independent extraction/storage limits.
- **Code facts are syntax, not semantics.** Definitions, imports, references and relationships come from grammar parsing and are labeled as candidates. Resolved references require an optional SCIP import or a local LSP run through the CLI. Textual references include matches in strings and comments.
- **Vector search is optional and unqualified.** It needs local model files you provision. Relevance on real codebases has not been measured.
- **Not extracted:** scanned or image-only PDFs, encrypted documents, legacy DOC/PPT/XLS, DOCX headers, footers and comments, PPTX notes, and XLSX charts and pivot caches. OCR is available through the CLI for selected pages; the extension does not run it.
- **Unsigned local and CI helpers.** Ordinary CI and Marketplace packaging use unsigned native helpers. The separate GitHub release workflow signs Windows helpers and signs/notarizes Mac helpers; check the provenance of the package you install.

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
| An agent's AIRK server will not start or shows an old index | Its configuration points at an earlier helper or storage folder | Run **Connect Coding Agent…** again, replace the old entry and restart the agent. |
| A file never appears as indexed | Too large, ignored, unsupported or extraction failed | Check **Show Output Log** for the skip reason. |

## Learn more

- [Repository](https://github.com/iokaio/airk) · [Documentation](https://github.com/iokaio/airk/blob/main/docs/README.md) · [Agent setup matrix](https://github.com/iokaio/airk/blob/main/docs/agent-integration.md)
- [Report a bug](https://github.com/iokaio/airk/issues) · [Support](https://github.com/iokaio/airk/blob/main/SUPPORT.md) · [Security policy](https://github.com/iokaio/airk/blob/main/SECURITY.md)

AIRK stands for Artifact Indexing and Retrieval Kernel. Licensed under Apache-2.0; bundled dependency notices in THIRD_PARTY_NOTICES.md retain their respective terms.
