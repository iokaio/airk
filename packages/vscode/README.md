# AIRK: Local Search for Coding Agents

**Find the code and documents behind a feature. Give your coding agent cited evidence, or explore it yourself in AIRK Workbench.**

AIRK builds a searchable local index of your project so you and your coding tools can retrieve relevant passages, definitions and document facts without loading entire files into every request. Use it with MCP clients such as Claude Code, Codex and GitHub Copilot, or work directly in VS Code with no coding agent required.

- **Find useful context:** search code and documents together, then narrow the scope to a file, folder or named definition.
- **Inspect the evidence:** open cited sources, read surrounding context and keep source locations and indexed revisions with your results.
- **Keep retrieval local:** indexing, lexical and semantic search run on your machine with bundled models. AIRK needs no cloud service or API key.

## Meet AIRK Workbench

Workbench is a compact evidence workspace in VS Code’s secondary sidebar. Type a slash command, prepare an evidence recipe, inspect results and open the original source. Copy cited results as Markdown or JSON, export a session, and return to recent investigations.

A small toolbar holds history, settings and new-session controls. The **+** menu offers scope, recipes and tool forms when you need them, keeping the transcript available for results. Proposed plans remain editable and require an explicit run action. **No generative AI is used in AIRK Workbench**; supported intent parsing uses a fixed local classifier, and semantic retrieval uses local embeddings.

![AIRK Workbench showing cited source evidence](https://raw.githubusercontent.com/iokaio/airk/main/docs/workbench/images/workbench-1.7.1-results.png)

*Preview from the compiled Workbench UI with fictional project data. It illustrates the interaction, not a measured retrieval-quality result.*

## AIRK Explorer: search, inspect and manage

Explorer brings search, code navigation, document facts, evidence review and indexing controls into one editor workspace. Check what is indexed, inspect source citations, and manage projects, storage and agent connections.

![AIRK Explorer showing indexed project evidence and navigation](https://raw.githubusercontent.com/iokaio/airk/main/docs/explorer/images/explorer-1.7.1.png)

*Explorer 1.7.1 preview with fictional observations. In 1.7.2, Workbench moves to the toolbar and project selection moves into Overview and Management. Not installed-editor qualification.*

## Get your first result

1. **Install AIRK** for your workspace host’s OS and architecture. The native runtime and local models are bundled; no Docker, Rust toolchain or separate API key is needed to use the extension. VS Code 1.140 or newer is required.
2. **Open a trusted project.** With automatic setup enabled, AIRK registers the folder and starts indexing saved files. Open **AIRK Explorer → Management** to inspect progress, readiness and any skipped or failed files.
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
| VS Code / GitHub Copilot | Workspace MCP registration and VS Code’s MCP controls. |
| Claude Code | Project MCP configuration and client approval. |
| Codex | Scoped TOML configuration and the client’s MCP controls. |
| Gemini CLI / Antigravity | Project configuration and client-specific enablement. |
| Cursor, Cline, Roo, Junie and others | Configuration previews or connection guidance for the selected client. |

These are configuration routes, not a claim that every client/version has passed end-to-end testing. Use the built-in setup guide and [integration documentation](https://github.com/iokaio/airk/blob/main/docs/agent-integration.md). AIRK’s connection check verifies its helper; a successful request in your agent verifies that client’s access.

## Local control and transparent limits

Project indexes stay separate from source files. Native background watching updates saved-file evidence; pause, manual-update and semantic opt-out settings remain available. Explorer’s Management tabs show indexing progress, published readiness, storage, service state, agent setup and logs. Explorer’s **Workbench** toolbar button, immediately left of Help, toggles secondary-sidebar visibility and shows a pressed state while visible. The project selector is available inside Overview and Management; its selection remains shared across Explorer screens.

AIRK does not use a remote inference service. If you give retrieved evidence to an external coding agent, that agent’s provider, privacy and approval policies still apply. Local retrieval does not make the agent itself offline.

Usage reporting can help inspect retrieval activity and supplied measurement scenarios. Context, token or cost savings depend on the task and agent; AIRK does not promise a universal reduction in tokens, time or bills.

## Version and setup notes

**Version 1.7.4** asks before the first installation what it will change, can run without starting any AIRK process (`airk.runtime.nativeExecution`), removes its terminal integration on uninstall and reports interrupted cleanups, and adds **AIRK: Review and Remove AIRK Data**. Common credential files are skipped by default. [Release notes and validation limits](https://github.com/iokaio/airk/blob/main/docs/releases/release-1.7.4.md) document the changes. Earlier records remain in the [release archive](https://github.com/iokaio/airk/blob/main/docs/releases/README.md).

VS Code may require **Developer: Reload Window** to replace an already running extension after an upgrade. AIRK keeps a **Reload to finish update** action when it detects a pending replacement. VS Code saves sidebar tab order; drag the AIRK tab to the right if needed. `airk.cli.autoInstall: false` disables automatic installation and registration; discovery can still run bundled native probes. `airk.runtime.nativeExecution: false` starts no AIRK process at all, and `airk.cli.shellIntegration` limits which shell startup files setup edits.

Before the first installation AIRK shows what setup changes and lets you set up, customize, defer or decline it. Setup installs the runtime/CLI, updates user PATH or marked shell-profile blocks and starts background indexing of registered folders; home folders and drive roots need an explicit choice. Hidden files can be included; common credential files are skipped by default, but AIRK is not a secret scanner, so ignore other sensitive sources before indexing. Index objects and saved Workbench results can retain excerpts, and evidence returned to an agent may reach its provider. On macOS/Linux an existing storage folder that other accounts can reach is refused. Uninstall removes AIRK's terminal integration but preserves native indexes; use **AIRK: Review and Remove AIRK Data** first if you want them gone. See [installation and evidence privacy](https://github.com/iokaio/airk/blob/main/docs/vscode-extension.md#installation-and-evidence-privacy) for controls and cleanup limits.

AIRK stands for **Artifact Indexing and Retrieval Kernel**. It is an independent local indexing and retrieval engine, licensed under Apache-2.0.

[Source and issues](https://github.com/iokaio/airk) · [Documentation](https://github.com/iokaio/airk/blob/main/docs/README.md) · [Installation and troubleshooting](https://github.com/iokaio/airk/blob/main/docs/service-installation.md)
