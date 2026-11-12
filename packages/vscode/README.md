# AIRK: Local Search for Coding Agents

## Requires AIRK Service

This extension is the editor front end of **AIRK Service**, a separately
installed local program that indexes your code and documents and answers
searches on this computer. The extension does not contain the service.
**Installing or updating this extension never installs, updates, repairs or
removes AIRK Service**; the service has its own installer and uninstaller.

Download the AIRK Service installer from the AIRK downloads page,
**https://airk.ioka.io/#downloads**, or from the versioned
[GitHub Releases](https://github.com/iokaio/airk/releases), then:

- **Windows:** run the AIRK Service installer (`.exe`) and follow its steps. No
  administrator rights are needed.
- **macOS:** open the AIRK Service disk image and follow its install steps.
- **Linux:** extract the archive and run its `install.sh`.

Confirm the service is running with:

```text
airkctl service status
```

The extension finds the service only through the installer's per-user install
record. It verifies the installed `airkctl` (signature on Windows and macOS,
record digest on Linux) before running it and then reaches the service only
through `airkctl`. When the service is missing, stopped or incompatible, AIRK
shows that state with **Download AIRK Service**, **Start** or **Check again**;
**AIRK: Open AIRK Admin** opens the service's management pages
(`airkctl admin open`) for indexes, storage, agents, usage and logs.

**Split preview (12 November 2026).** This source revision is the first
extension slice of the AIRK Service split ([split plan](https://github.com/iokaio/airk/blob/main/docs/split/split-plan.md) §8).
It is unpublished and not installed-editor qualified. Earlier release notes
below describe the pre-split extension, which bundled and installed the native
runtime; they are kept as history.

## Meet AIRK Workbench

Workbench is a compact evidence workspace in VS Code’s secondary sidebar. Ask about a named symbol, for example “I want to know about struct Owner.”, or use a slash command or evidence recipe. Inspect the retrieved evidence and open the original source. Copy cited results as Markdown or JSON, export a session, and return to recent investigations.

A small toolbar holds history, settings and new-session controls. The **+** menu offers scope, recipes and tool forms when you need them, keeping the transcript available for results. Proposed plans remain editable and require an explicit run action. **No generative AI is used in AIRK Workbench**; supported intent parsing uses a fixed local classifier, and semantic retrieval uses local embeddings.

![AIRK Workbench 1.8.7 showing cited code and document evidence](https://raw.githubusercontent.com/iokaio/airk/main/docs/workbench/images/workbench-1.8.7-results.png)

*Compiled Workbench 1.8.7 UI with fictional project data. Linked selection controls, source/context actions, Complete response and export are visible. This preview is not installed-editor qualification or a retrieval-quality measurement.*

## AIRK Explorer: search, inspect and manage

Explorer brings search, code navigation, document facts, evidence review and indexing controls into one editor workspace. Check what is indexed, inspect source citations, and manage projects, storage and agent connections.

![AIRK Explorer showing indexed project evidence and navigation](https://raw.githubusercontent.com/iokaio/airk/main/docs/explorer/images/explorer-1.7.1.png)

*Explorer 1.7.1 preview with fictional observations. In 1.7.2, Workbench moves to the toolbar and project selection moves into Overview and Management. Not installed-editor qualification.*

## Get your first result

1. **Install AIRK Service** on your workspace host (see *Requires AIRK Service* above), then install this extension. VS Code 1.140 or newer is required. No Docker, Rust toolchain or separate API key is needed.
2. **Create an index.** Run **AIRK: Open AIRK Admin** and use the Index library to review source, storage and watching choices and create the index. Opening a folder alone does not register or index it. The admin pages show progress, readiness and skipped or failed work.
3. **Try Workbench.** It opens once after successful setup, or run **AIRK: Open Workbench**. Start with `/files` to see indexed paths. Then try `/search retry --mode lexical`, replacing `retry` with a term in your project, and open a cited result.
4. **Connect an agent when you want one.** Run **AIRK: Connect Coding Agent**, review its scoped configuration, and enable or approve AIRK in that client. Ask the agent to discover AIRK’s tools, check `status`, search the relevant scope and verify the cited live files before editing.

Success means a query returns evidence for the intended project and its citations open the expected source. Indexing or connection failures remain visible; an enabled-server badge alone does not prove that your agent can retrieve evidence.

## Three workflows to try

| Your task | Use AIRK to collect |
|---|---|
| Understand a feature | Matching code, named definitions and the design or requirement that describes it. |
| Investigate a bug | Relevant source passages, textual references and test mentions to inspect before changing behavior. |
| Prepare a change | A scoped evidence set with citations, an editable read-only plan, and Markdown or JSON you can share with your agent. |

In Workbench, try `/symbols YourSymbol`, `/outline path/to/file`, or `/read path/to/file:1-20` with names and paths from your project. The **+** menu includes recipes for definitions, document evidence, possible impact, test mentions, components and readiness. These workflows collect evidence; they do not edit source files or execute your tests.

## Code and documents in one index

AIRK combines lexical and local semantic search with bounded reads and structured evidence. Supported formats include source code, Markdown, text, PDF, DOCX, XLSX and PPTX. Document citations retain original locations such as pages, slides, paragraphs and worksheet cells where extraction supports them. See [format support and limits](https://github.com/iokaio/airk/blob/main/docs/document-indexing.md).

Results identify the indexed revision and source hash. Unsaved edits, pending indexing, skipped extraction and parser limitations can affect what is available. Code references and relationships are candidates unless supported by stronger provider evidence; AIRK does not claim complete language understanding or complete impact analysis.

## Use your existing coding agent

One AIRK MCP endpoint gives independently configured clients access to the same project index. Your agent handles reasoning and code changes; AIRK supplies bounded evidence.

| Client | AIRK setup route |
|---|---|
| VS Code / Copilot Chat | Implemented native HTTPS provider/project configuration and VS Code’s MCP controls. |
| Claude Code | Implemented project HTTPS configuration and client approval. |
| Codex | Implemented scoped HTTPS TOML configuration and the client’s MCP controls. |
| Other clients, including Gemini, Antigravity, Cursor, Cline, Roo and Junie | Automatic HTTPS configuration and migration are unsupported. Manual compatibility must be verified against that client's TLS, credential and scope requirements. |

These are implemented adapter routes, not completed installed-client/platform qualification. Use the built-in setup guide and [integration documentation](https://github.com/iokaio/airk/blob/main/docs/agent-integration.md). AIRK’s connection check verifies its authenticated scoped HTTPS path; the external client still needs its own trust, credential provisioning, reconnection and successful scoped request. Local tools can use the [documented HTTPS API](https://github.com/iokaio/airk/blob/main/docs/local-https-api.md); that generic API does not establish compatibility with another client's configuration format.

## Local control and transparent limits

Project indexes stay separate from source files. Native background watching updates saved-file evidence; pause, manual-update and semantic opt-out settings remain available. Explorer’s Management tabs show indexing progress, published readiness, storage, service state, agent setup and logs. Explorer’s **Workbench** toolbar button, immediately left of Help, toggles secondary-sidebar visibility and shows a pressed state while visible. The project selector is available inside Overview and Management; its selection remains shared across Explorer screens.

AIRK does not use a remote inference service. If you give retrieved evidence to an external coding agent, that agent’s provider, privacy and approval policies still apply. Local retrieval does not make the agent itself offline.

Usage reporting can help inspect retrieval activity and supplied measurement scenarios. Context, token or cost savings depend on the task and agent; AIRK does not promise a universal reduction in tokens, time or bills.

Developer references: [MCP tools](https://github.com/iokaio/airk/blob/main/docs/mcp-developer-guide.md) and [CLI commands](https://github.com/iokaio/airk/blob/main/docs/cli-developer-guide.md) explain purpose, examples and implementation ownership. Current CLI `tools list/help/call/shell` and public HTTPS share the 29-tool unified catalog; its 19 native evidence/help/plan/scope operations remain a backend subset.

## Version and setup notes

**Included in 1.8.2:** Supported developer, writer, architect and QA/tester requests retrieve code/document evidence, including simple symbol questions such as “I want to know about struct Owner.” Unsupported requests still ask for clarification, and multiple matches remain cited candidates. Workbench reveals the query bubble for a new pending, completed or failed reply, keeping the question visible above its details and results. Sidebar tab switches retain the reading position. Structured results have linked **Select all N** controls above and below the cards, with **Copy selected** in both rows. It admits supported forward runtime upgrades through the 1.8 line. Explorer publishes verified service health before optional management observations finish and delivers each successful Search result once. Index readiness remains separate, and installed responsiveness and native throughput have separate measurement results. See the [candidate record](https://github.com/iokaio/airk/blob/main/docs/releases/release-1.8.7.md#candidate-1-8-2). Automatic Explorer observations use lightweight status; explicit Refresh obtains separately aged details. Eligible saved selected projects can appear in Explorer and Workbench before other workspace roots finish preparation; native scope and lifecycle checks still apply. First semantic requests can wait within a bounded startup allowance for the embedding-helper connection to become available. Workbench reuses its selected connection across New/Clear, avoids a duplicate session scan when saving, and sends scalar status/save updates while retaining complete export values. Updated indexes publish source-bound optional pages for native facts, relationships, imported assertions and text ranges. Eligible native-owned collections can use selected-page readers with automatic physical integrity maintenance and fresh crash recovery; unsupported storage boundaries and omitted optional representations retain strict reads. Integrated storage and release qualification remains in progress.

**Published version 1.7.6** asks before the first installation what it will change, can run without starting any AIRK process (`airk.runtime.nativeExecution`), removes its terminal integration on uninstall and reports interrupted cleanups, and adds **AIRK: Review and Remove AIRK Data**. Common credential files are skipped by default. [Release notes and validation limits](https://github.com/iokaio/airk/blob/main/docs/releases/release-1.7.6.md) document the changes. Earlier records remain in the [release archive](https://github.com/iokaio/airk/blob/main/docs/releases/README.md).

*Pre-split history (1.7–1.8):* Once the new AIRK loader is active, later detected extension replacements reload AIRK automatically and reopen Explorer if it was open. An older loaded extension still needs VS Code to activate the new loader; use **Restart Extensions** in VS Code’s Extensions pane for that one-time transition. VS Code saves sidebar tab order; drag the AIRK tab to the right if needed. `airk.cli.autoInstall: false` disables automatic runtime and terminal setup; discovery can still run bundled native probes. `airk.runtime.nativeExecution: false` starts no AIRK process at all, and `airk.cli.shellIntegration` limits which shell startup files setup edits.

Before the first installation AIRK reviews runtime/CLI and terminal integration changes; Customize controls the external terminal command. New index source, storage and watching choices use a separate review. Choose this window's search scope among existing index sets (optionally narrowed to some of their indexes) with **Search scope** checkboxes and **Apply selection**; Overview and Search then show **Searching:** with **Clear**. The choice is remembered for the workspace and applied again only after the index library is read at its current revision; if the index library changed, AIRK asks you to reselect. Index sets are created and changed in AIRK Admin. The scope needs an AIRK Service that offers it; native access records are handled internally. Existing accepted jobs or saved watching may continue independently. Hidden files can be indexed and common credential files are skipped by default, but AIRK is not a secret scanner: review sensitive sources before indexing. Indexes and saved Workbench results may retain excerpts; evidence returned to an agent may reach its provider. Private storage permissions, terminal cleanup and explicit data retirement remain separate controls. See [reviewed setup](https://github.com/iokaio/airk/blob/main/docs/service-project-setup.md) and [installation and evidence privacy](https://github.com/iokaio/airk/blob/main/docs/vscode-extension.md#installation-and-evidence-privacy).

AIRK stands for **Artifact Indexing and Retrieval Kernel**. It is an independent local indexing and retrieval engine, licensed under Apache-2.0.

[Source and issues](https://github.com/iokaio/airk) · [Documentation](https://github.com/iokaio/airk/blob/main/docs/README.md) · [Installation and troubleshooting](https://github.com/iokaio/airk/blob/main/docs/service-installation.md)
