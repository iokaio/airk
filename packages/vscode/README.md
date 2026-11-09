# AIRK: Local Search for Coding Agents

The [fixture compatibility matrix](https://github.com/iokaio/airk/blob/feat/finetuning/docs/testing/fixture-compatibility.md)
keeps imported-legacy migration distinct from reviewed catalog upgrades. The
authentic legacy producer and installed-editor acceptance remain pending; the
1.5.12 schema-8 metadata bridge passed scoped regression. r26 remains preserved.

The approved [automatic rebuild option](https://github.com/iokaio/airk/blob/feat/finetuning/docs/upgrade-rebuild-fallback.md)
will let users choose rebuilding earlier in an upgrade, then automatically build
fresh indexes from eligible raw managed saved sources while preserving previous
state and supported settings. Unsupported configurations are refused before state
changes; client re-enrollment is explicit. Native/editor source is implemented;
installed and frozen qualification are pending.

[Standalone CLI onboarding](https://github.com/iokaio/airk/blob/feat/finetuning/docs/agent-integration.md#standalone-onboarding-without-vs-code)
is implemented on this branch for setup without VS Code: native service
discovery, reviewed index scope and protected client launch. Scoped CLI tests
pass; actual client launch and installed acceptance remain pending. Use the printed installed executable path;
setup does not change `PATH`.
Standalone Windows/macOS/Linux installer source installs only the CLI/native
server; this VS Code extension remains a separate optional distribution.
Actual installer builds, signing and publication remain pending.

**1.8.14 implementation and qualification are in progress; unpublished.** The source versions and generated catalog identify the [One AIRK implementation](https://github.com/iokaio/airk/blob/feat/finetuning/docs/one-airk.md). The single-executable HTTPS runtime is implemented; remaining migration corrections and final native and installed-platform acceptance are tracked in the [1.8.14 working record](https://github.com/iokaio/airk/blob/feat/finetuning/docs/releases/release-1.8.14.md). Earlier binaries keep their original versions and hashes.

Modern installation must migrate recognized owned Codex, Claude Code and
VS Code/Copilot Chat connections to direct native HTTPS, then remove verified
legacy launcher images, their processes and eligible superseded runtime executables.
This applies to legacy-to-modern and later modern-to-modern updates, preserving
the selected runtime, sources, indexes and settings. The explicit rebuild option
separately preserves its predecessor runtime and old state as recovery evidence.
Later ordinary upgrades retire unprotected intermediates and report that
preserved evidence separately. Unexplained obsolete payloads mean unfinished cleanup. Unsupported clients,
unproven scopes and blocked cleanup leave installation explicitly incomplete.
Installed-client and platform acceptance remain pending.

HTTPS uses normal certificate validation with an installation-local CA; clients
do not ignore certificate errors or discard their existing trusted CAs. Native
discovery reports the actual bound port, including an alternate when the preferred
port is occupied. Owned connection configuration can be refreshed, while external
clients may still require reconnection. See the [local HTTPS API](https://github.com/iokaio/airk/blob/feat/finetuning/docs/local-https-api.md)
for trust, credentials, endpoint discovery and local-tool examples.

Credential lifecycle corrections reclaim bounded active-ledger capacity after
revocation and proven Explorer attachment retirement, while retaining immutable
connection ownership and ID-reuse fences. Active manual credentials are preserved.
Validation and remaining client/platform gates are recorded in the working record.

**Explorer notices and updates:** dismiss update, connection and action-outcome notices with **×**. Dismissals are saved for the workspace and stay hidden across screens, Explorer reopen and window reload. Once the new loader is running, later detected AIRK extension replacements reload AIRK automatically and reopen its single Explorer panel when it was open. For the first transition from an older loaded extension, use **Restart Extensions** in VS Code’s Extensions pane to activate the new version. Installed-editor acceptance is recorded separately.

**Inherited 1.8.12 behavior:** Indexing has three tabs: **Search scope** selects local indexes and index sets with checkboxes; **Index library** creates and manages indexes, including **Delete index**; **Index sets** organizes reusable search groups. Choose indexes or sets and apply the selection; access details are handled internally. See the [historical 1.8.12 working-build record](https://github.com/iokaio/airk/blob/feat/finetuning/docs/releases/release-1.8.12.md) for behavior and validation. The earlier 1.8.11, 1.8.10 and 1.8.9 records retain their dated evidence.

**MCP QA follow-up:** the working source preserves structured read failures, discloses unobserved filters and relationship limits, and aligns context inputs across interfaces. Terse capability replies summarize operation schemas; Full retains them. Validation results and remaining limits are recorded in the [QA evaluation](https://github.com/iokaio/airk/blob/main/docs/reviews/mcp-qa-1.8.9.md).

Opening a trusted folder does not register or index it. Use **Indexing > Index library** to create an index with reviewed sources, storage and watching. In **Search scope**, check the indexes or index sets to search and apply the selection. Evidence reads and source verification do not start indexing.

**Setup and uninstall:** Successful setup or reconnection clears obsolete failure warnings. Current setup failures remain visible, and uninstall removes empty shell startup files that AIRK created. See [setup and uninstall guidance](https://github.com/iokaio/airk/blob/main/docs/vscode-extension.md) and the [branch review](https://github.com/iokaio/airk/blob/main/docs/reviews/branch-v190-review-2026-11-06.md).

**Missing sources and storage:** the working source adds [reviewed index
retirement and cleanup](https://github.com/iokaio/airk/blob/main/docs/service-project-retirement.md) for removed files or folders and separate
[unavailable legacy-index upgrade recovery](https://github.com/iokaio/airk/blob/main/docs/service-installation.md#unavailable-legacy-index-during-upgrade). Healthy indexes keep
their independent evidence; old grants are never silently narrowed. Post-P14
recovery has selected Windows native/process and Mac portable validation. Full
workspace and final installer qualification remain separate; see the working-build
record for current results and earlier failures.

**Find the code and documents behind a feature. Give your coding agent cited evidence, or explore it yourself in AIRK Workbench.**

AIRK builds a searchable local index of your project so you and your coding tools can retrieve relevant passages, definitions and document facts without loading entire files into every request. Use it with MCP clients such as Claude Code, Codex and GitHub Copilot, or work directly in VS Code with no coding agent required.

**Configurable concurrent evidence requests.** Management → Service → **Concurrent requests…** selects **4, 8, 16, 32 or 64**, default **16**, with one additional status/control slot. MCP admission, independent native channels and project workers use the same setting. Changes apply to subsequent requests without restarting agents; accepted work drains when capacity decreases. Finite queues, source scope, revision identity and typed retry receipts remain authoritative.

**Complete evidence across interfaces.** CLI and Workbench retain the MCP handlers' full results: provenance, readiness, coverage, continuations and retry details. Workbench exposes filters, exact/suffix/contains matching, count/file summaries and conditional/continuity inputs. **Complete response** and full JSON/Markdown exports retain the result even when cited cards are shown; selected-evidence exports identify their subset. Status and diagnostics report configured capacity, including inside completed plan steps. See the [1.8.7 behavior and validation](https://github.com/iokaio/airk/blob/main/docs/releases/release-1.8.7.md).

- **Find useful context:** search code and documents together, then narrow the scope to a file, folder or named definition.
- **Inspect the evidence:** open cited sources, read surrounding context and keep source locations and indexed revisions with your results.
- **Keep retrieval local:** indexing, lexical and semantic search run on your machine with bundled models. AIRK needs no cloud service or API key.

**Version 1.8.2 improves installed responsiveness.** Normal-editor acceptance testing observed quicker loading and previously slow or timed-out requests returning promptly. Five measured fresh sessions through the installed launcher returned available first statuses with no status timeout; median initialization through confirmed status was 2.29 seconds, with a 7.31-second first-session outlier. The retained 1.7.6 example took about 5.5 seconds including a timeout and a second request; these are unmatched observations, not a guaranteed speedup percentage. Read the [performance improvements and causal analysis](https://github.com/iokaio/airk/blob/main/docs/performance-improvements-1.8.2.md) for complete evidence and remaining native regressions.

**Workbench improvements:** readiness proceeds independently of saved-history loading; New/Clear reuses the selected connection, Save avoids a duplicate session scan, and scalar status/save updates preserve complete exports. Query-first reveal, retained reading position and linked Select all/Copy selected controls make evidence easier to inspect and copy. In the retained 200-turn fixture, scalar status messages are over 95% smaller than full-state messages; this measures transfer work, not an elapsed UI speedup.

**Retained session choices:** each session keeps its selected tool, per-tool inputs and shared Scope/Recipe. Saved history restores those choices. **Reset this tool form** clears the active fields; New, Clear and Fork start fresh. Editing these choices submits no evidence request.

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

1. **Install AIRK** for your workspace host’s OS and architecture. The native runtime and local models are bundled; no Docker, Rust toolchain or separate API key is needed to use the extension. VS Code 1.140 or newer is required.
2. **Create an index and choose what to search.** Open a trusted folder, then use **Indexing > Index library** to review source, storage and watching choices and create the index. In **Search scope**, check the indexes or index sets you want and apply the selection. Opening a folder alone does not register or index it. Management shows progress, readiness and skipped or failed work.
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

These are implemented adapter routes, not completed installed-client/platform qualification. Use the built-in setup guide and [integration documentation](https://github.com/iokaio/airk/blob/feat/finetuning/docs/agent-integration.md). AIRK’s connection check verifies its authenticated scoped HTTPS path; the external client still needs its own trust, credential provisioning, reconnection and successful scoped request. Local tools can use the [documented HTTPS API](https://github.com/iokaio/airk/blob/feat/finetuning/docs/local-https-api.md); that generic API does not establish compatibility with another client's configuration format.

## Local control and transparent limits

Project indexes stay separate from source files. Native background watching updates saved-file evidence; pause, manual-update and semantic opt-out settings remain available. Explorer’s Management tabs show indexing progress, published readiness, storage, service state, agent setup and logs. Explorer’s **Workbench** toolbar button, immediately left of Help, toggles secondary-sidebar visibility and shows a pressed state while visible. The project selector is available inside Overview and Management; its selection remains shared across Explorer screens.

AIRK does not use a remote inference service. If you give retrieved evidence to an external coding agent, that agent’s provider, privacy and approval policies still apply. Local retrieval does not make the agent itself offline.

Usage reporting can help inspect retrieval activity and supplied measurement scenarios. Context, token or cost savings depend on the task and agent; AIRK does not promise a universal reduction in tokens, time or bills.

Developer references: [MCP tools](https://github.com/iokaio/airk/blob/feat/finetuning/docs/mcp-developer-guide.md) and [CLI commands](https://github.com/iokaio/airk/blob/feat/finetuning/docs/cli-developer-guide.md) explain purpose, examples and implementation ownership. Current CLI `tools list/help/call/shell` and public HTTPS share the 29-tool unified catalog; its 19 native evidence/help/plan/scope operations remain a backend subset.

## Version and setup notes

**Included in 1.8.2:** Supported developer, writer, architect and QA/tester requests retrieve code/document evidence, including simple symbol questions such as “I want to know about struct Owner.” Unsupported requests still ask for clarification, and multiple matches remain cited candidates. Workbench reveals the query bubble for a new pending, completed or failed reply, keeping the question visible above its details and results. Sidebar tab switches retain the reading position. Structured results have linked **Select all N** controls above and below the cards, with **Copy selected** in both rows. It admits supported forward runtime upgrades through the 1.8 line. Explorer publishes verified service health before optional management observations finish and delivers each successful Search result once. Index readiness remains separate, and installed responsiveness and native throughput have separate measurement results. See the [candidate record](https://github.com/iokaio/airk/blob/main/docs/releases/release-1.8.7.md#candidate-1-8-2). Automatic Explorer observations use lightweight status; explicit Refresh obtains separately aged details. Eligible saved selected projects can appear in Explorer and Workbench before other workspace roots finish preparation; native scope and lifecycle checks still apply. First semantic requests can wait within a bounded startup allowance for the embedding-helper connection to become available. Workbench reuses its selected connection across New/Clear, avoids a duplicate session scan when saving, and sends scalar status/save updates while retaining complete export values. Updated indexes publish source-bound optional pages for native facts, relationships, imported assertions and text ranges. Eligible native-owned collections can use selected-page readers with automatic physical integrity maintenance and fresh crash recovery; unsupported storage boundaries and omitted optional representations retain strict reads. Integrated storage and release qualification remains in progress.

**Published version 1.7.6** asks before the first installation what it will change, can run without starting any AIRK process (`airk.runtime.nativeExecution`), removes its terminal integration on uninstall and reports interrupted cleanups, and adds **AIRK: Review and Remove AIRK Data**. Common credential files are skipped by default. [Release notes and validation limits](https://github.com/iokaio/airk/blob/main/docs/releases/release-1.7.6.md) document the changes. Earlier records remain in the [release archive](https://github.com/iokaio/airk/blob/main/docs/releases/README.md).

Once the new AIRK loader is active, later detected extension replacements reload AIRK automatically and reopen Explorer if it was open. An older loaded extension still needs VS Code to activate the new loader; use **Restart Extensions** in VS Code’s Extensions pane for that one-time transition. VS Code saves sidebar tab order; drag the AIRK tab to the right if needed. `airk.cli.autoInstall: false` disables automatic runtime and terminal setup; discovery can still run bundled native probes. `airk.runtime.nativeExecution: false` starts no AIRK process at all, and `airk.cli.shellIntegration` limits which shell startup files setup edits.

Before the first installation AIRK reviews runtime/CLI and terminal integration changes; Customize controls the external terminal command. New index source, storage and watching choices use a separate review. Choose local indexes and index sets with **Search scope** checkboxes and **Apply selection**; native access records are handled internally. Existing accepted jobs or saved watching may continue independently. Hidden files can be indexed and common credential files are skipped by default, but AIRK is not a secret scanner: review sensitive sources before indexing. Indexes and saved Workbench results may retain excerpts; evidence returned to an agent may reach its provider. Private storage permissions, terminal cleanup and explicit data retirement remain separate controls. See [reviewed setup](https://github.com/iokaio/airk/blob/main/docs/service-project-setup.md) and [installation and evidence privacy](https://github.com/iokaio/airk/blob/main/docs/vscode-extension.md#installation-and-evidence-privacy).

AIRK stands for **Artifact Indexing and Retrieval Kernel**. It is an independent local indexing and retrieval engine, licensed under Apache-2.0.

[Source and issues](https://github.com/iokaio/airk) · [Documentation](https://github.com/iokaio/airk/blob/main/docs/README.md) · [Installation and troubleshooting](https://github.com/iokaio/airk/blob/main/docs/service-installation.md)
