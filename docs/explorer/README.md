# The AIRK Explorer

Historical design baseline from the pre-Explorer 1.4 planning cycle. The implementation has since shipped; use [current Explorer guidance](../vscode-extension.md), the [unified MCP catalog](../unified-mcp.md) and the [documentation index](../README.md) for current behavior. The baseline observations below retain their original scope.

AIRK Explorer is the proposed replacement for the current AIRK VS Code panel, which is crowded and simplistic. It would be one navigable workbench for evidence search, code and document inspection, review, strategy and plan preparation, and index management.

This folder holds the design inputs for that work. Each document was written in response to earlier ones. This README summarizes them in order of relevance, for an AI agent writing an Explorer design and implementation plan grounded in the code on `main`.

**[AIRK_Explorer_Final_Design.md](AIRK_Explorer_Final_Design.md) is the consolidated design.** It is based on these sources and checked against the code on `main`, and it is the starting point for implementation planning. Its decision register (§15) lists the choices that still need maintainer approval. The summaries below explain where each of its inputs came from.

Nothing in this folder is implemented. Every feature, tool name, schema, file path and milestone below is a proposal, and file paths are relative to the repository root unless stated otherwise.

## Authority

| Role | Documents |
|---|---|
| **Controlling** (design and features) | 1. [AIRK_Explorer_UI_Prototype.html](AIRK_Explorer_UI_Prototype.html)<br>2. [AIRK_Explorer_Local_MCP_Agent_Assistance_Requirements.docx](AIRK_Explorer_Local_MCP_Agent_Assistance_Requirements.docx) |
| **Commentary and review** (proposals to adopt or reject) | 3. [AIRK_Explorer_Codex_Implementation_Plan_v1.4.md](AIRK_Explorer_Codex_Implementation_Plan_v1.4.md)<br>4. [AIRK_Explorer_Plan_v1.4_Review_and_Recommendations.docx](AIRK_Explorer_Plan_v1.4_Review_and_Recommendations.docx)<br>5. [AIRK_Explorer_Implementation_Spec_from_Claude.md](AIRK_Explorer_Implementation_Spec_from_Claude.md)<br>6. [grok_design_review.md](grok_design_review.md)<br>7. [AIRK_1.4_Competitive_Research.md](AIRK_1.4_Competitive_Research.md) |

Several commentary documents claim precedence over the others. The Codex plan calls itself the "single implementation authority", and the Claude spec ranks itself first. Ignore those claims. When documents disagree, the prototype and the requirements document decide, then this repository's own guidance (`CLAUDE.md`/`AGENTS.md`, [docs/architecture.md](../architecture.md)), then the code on `main`.

The documents also cite each other under names that differ from the files here:

| Name used in the documents | File in this folder |
|---|---|
| `AIRK-Explorer-Desktop-Preview-Fixed.html` | `AIRK_Explorer_UI_Prototype.html` (most likely) |
| `AIRK_Explorer_Codex_Plan_v1_4.md` | `AIRK_Explorer_Codex_Implementation_Plan_v1.4.md` |
| `AIRK-Explorer-Prototype-v2.html` | Not in this folder. `grok_design_review.md` describes it. |

## Read this first: the baseline on `main` has moved

All seven documents assume the public AIRK 1.3.1 release (commit `05938a9`, tagged `v1.3.1`) or a 1.4 service target they describe as user-specified. When they were written, nobody had observed that target in code. The repository is now at **1.4.4**. These are the differences found so far, checked against the repository on 26 Oct 2026:

- **The native MCP server has 16 read-only tools, not 15.** `airk_help` is registered in `crates/airk-mcp/src/lib.rs`. It serves a 35-topic usage catalog, generated CLI command help and the current tool schemas, without a workspace or a running service ([docs/mcp.md](../mcp.md)).
  - Every document's invariant of "15 tools, unchanged" and its tool-snapshot test need restating.
  - The proposed `airk_explorer_help_*` catalog would now overlap a help service that already exists in Rust. The requirements document's statement that no help catalog exists in the Rust app is no longer true. Decide whether Explorer help extends, reuses or deliberately separates from `airk_help`.
- **The extension runs as a client of the 1.4 native service.**
  - `packages/vscode/src/extension.ts` is a three-line re-export of `serviceExtension.ts`.
  - New modules include `serviceClient.ts`, `serviceBootstrap.ts`, `serviceConsole.ts`, `serviceConsoleController.ts`, `serviceConsoleView.ts`, `serviceDashboard.ts`, `serviceIntents.ts` and `serviceWorkspace.ts`.
  - The plans propose new `AirkServiceClient`, `ServiceManagement` and `CollectionBinding` types. Check them against `serviceClient.ts` before creating parallel abstractions.
- **The current panel layout differs from what the documents describe.**
  - **Views:** a `Dashboard` webview (`airk.dashboard`, built by `dashboardView.ts` and `sidebar.ts`), a `Service Console` webview (`airk.service`) and a `Workspace Projects` tree (`airk.collectionsView`).
  - **Other contributions:** 37 commands, the `airk.gettingStarted` walkthrough and one MCP server definition provider (`airk.collections`).
  - The 1.3.1 file map in the prototype's "Codex implementation map" tab and in requirements §11 (`dashboardView.ts`, `sidebar.ts`, `diagnosticsView.ts` and so on) is only partly current.
- **The service contract the review asked for (issue C1) now exists:** [docs/airkservice-architecture.md](../airkservice-architecture.md). It covers process ownership, the `airk`/`airk-embed` split, publication, transport and discovery, and MCP privilege separation. Read it before any service-facing design.
- **Gaps every document leaves open.** None of them covers DiskANN or the one-container-per-user model, and none has checked:
  - publication-token fields
  - cursors and pagination
  - stable result ordering
  - `airk_changes` selectors
  - which management operations the service exposes

  Verify these in the code. Do not take them from the documents.

---

## 1. AIRK_Explorer_UI_Prototype.html: controlling UI prototype

**What it is.** A standalone, browser-only interactive mock-up (about 225 KB, mostly inline JavaScript) titled "AIRK Explorer · Unified Interactive Prototype". It is labeled as against the AIRK 1.3.1 schema. All data is fictional (the `atlas-service` workspace and a session-renewal bug in `src/auth/session.ts`). It makes no MCP calls and runs no agents. Its stance, stated on every screen, is "NON-AI • LOCAL • READ-ONLY": Explorer gathers evidence and compiles prompts, and never runs a model itself.

**What it defines.**
- **Navigation.** One persistent left navigation of about 21 destinations, grouped by role, plus a pinned Index Management destination:
  - START: Overview
  - DISCOVER: Search evidence, Symbol inspector, References & calls
  - DEVELOPER: Code review, Quality review, Finding details
  - TESTER: Test explorer, Change impact
  - ARCHITECT: Architecture map, Architecture rules
  - WRITER: Document evidence, Docs-to-code drift
  - STRATEGIES: Strategy Library, Strategy Studio, Strategy Planner
  - PACKS: Pack manager, Rule inspector, Report builder
  - SHARED: Evidence inspector, Review reports

  The array is `navs` in the script.
- **Index Management.** A replacement for the current dashboard, Diagnose & Repair, collection tree, usage panel and setup flows, organized as ten tabs (`opsTabs`):
  - Collections & coverage
  - Scheduling & progress
  - Evidence & vectors
  - Diagnose & repair
  - Storage & cleanup
  - Ingestion & extraction
  - Agents, MCP & CLI
  - Usage & savings
  - Logs & reports
  - Codex implementation map

  It ends with an acceptance checklist; search for `Acceptance checklist`.
- **Strategy Library.** 25 blueprints in six categories: Development, Testing, Architecture, Documentation, Quality and Operations. Each is a name, a description and an ordered list of steps; see `const templates=[`.
- **Strategy Studio.**
  - Four tabs: Strategy builder, Prompt editor, Task queue, Automation scripting.
  - **Step catalog** (`steps`): the native `airk_*` tools, plus `VS Code · Git`, `VS Code · Files`, `Explorer · Core Pack` (rules) and `Explorer · Prompt compiler`.
  - **Run settings:** evidence byte budgets (12, 24 or 48 KB), Portable or MCP-aware handoff profiles, and fail-closed publication-token pinning.
  - **Prompt contract:** a five-part structure of Objective, Evidence, Task instructions, Deliverables and Completion gates.
  - **Queue:** a dependency queue with a dispatch policy. Dispatch is manual by default.
  - **Recipe file:** a declarative `strategy.airk.json`.
  - **CLI:** a proposed `airk-explorer` companion CLI, labeled NOT YET IMPLEMENTED.
- **Strategy Planner.** Combines Studio results into one editable Markdown plan plus a short handoff prompt.
- **Host save contract.** The most concrete host-side contract in the folder is a block comment beginning `CODEX INTEGRATION CONTRACT: STRATEGY PLANNER -> VS CODE EDITOR`. It defines:
  - versioned `postMessage` messages `airk.planner.openDraft`, `airk.planner.saveInWorkspace` and `airk.planner.result`
  - the VS Code APIs to use (`showSaveDialog`, `workspace.fs`, `openTextDocument`, `showTextDocument`)
  - the rule that "opened" is never "saved"
- **Proposed management message.** A shape for the typed host dispatcher (`indexManagement.command`), with a note to preserve the existing `sidebarAction` and diagnostics action allowlists until it replaces them.
- **Presentation.** Responsive layout plus a forced 1440 px desktop preview using CSS zoom.

**Useful to a planner.**
- It is the source of truth for screens, the vocabulary of evidence and readiness states, and interaction rules: never overwrite unsaved work, confirm destructive actions, never fake success.
- Its 1.3.1 file map is stale; see the baseline section above.
- Its role-grouped navigation of about 21 pages is the main structural point the commentary documents later collapse into seven or eight workspaces. Decide that explicitly.
- Search the HTML for `navs`, `opsTabs`, `templates`, `CODEX INTEGRATION CONTRACT` and `opsImplementation` to jump to the definitions.

## 2. AIRK_Explorer_Local_MCP_Agent_Assistance_Requirements.docx: controlling agent-assistance requirements

**What it is.** Requirements specification v0.2, dated 24 Oct 2026, with three diagrams. It specifies how a coding agent can help a user operate Explorer without gaining authority over it. Its stated baseline is 1.3.1 with 15 tools, which is now out of date.

**Core decisions.**
- **Two separate MCP services.**
  - **Service A:** the existing Rust retrieval MCP, which is not to change.
  - **Service B:** a new "AIRK Explorer Assistance" stdio MCP server, written in TypeScript/Node and bundled with the extension. Its tools use the `airk_explorer_*` namespace.
- **Routing invariant:** an Explorer tool never calls, proxies through or falls back to the Rust worker.
- **Agent-originated drafts:** they land in a **Suggestions Inbox**, and the user chooses Apply-to-draft, Edit, Defer or Reject. Applying is not executing. Running, saving, indexing and repairing remain separate user actions, each with its own host-side authorization.
- **No generic powers:** the bridge exposes no `executeCommand`, shell, file-write or set-configuration tool.

**Contents worth extracting.**
- **Goals:** G-01 to G-06. **Non-goals:** §2.3.
- **Session and binding:**
  - Session identity is installation ID + window ID + canonical root + storage/collection fingerprint + schema version.
  - Behavior for multi-root, multi-window, untrusted, WSL, SSH and container workspaces.
- **Help service:** requirements H-01 to H-09, a proposed authored catalog at `docs/explorer-help/`, and a topic taxonomy (`getting-started.*`, `explorer.search.*`, `strategies.planner.*`, `management.recovery.*` and others). Reconcile all of it with the existing Rust `airk_help`.
- **Tool catalog (§5):**
  - **P0:** `help_search`, `help_get`, `ui_status`, `suggest_search`, `suggest_strategy`, `suggest_plan`, `proposal_status`
  - **P1:** `suggest_review`, `suggest_management`
  - **P2:** `suggest_report`, `open_view`
  - JSON request and response examples for each.
- **Inbox UX:** UX-01 to UX-07, plus screen behavior for Search, Studio, Planner (with the VS Code save contract) and Management.
- **Scenarios:** SC-01 to SC-22, covering setup, debugging, review, recovery, isolation, prompt injection and concurrent proposals.
- **Data contracts and limits:**
  - a proposal receipt and state machine (`RECEIVED → … → APPLIED | REJECTED | DEFERRED | EXPIRED | CONFLICTED`)
  - a fail-closed error registry (`UI_NOT_ATTACHED`, `STALE_BINDING`, `PROPOSAL_CONFLICT` and others)
  - retention limits: 50 proposals, 24 h TTL, 256 KiB
- **Security:** a threat and control table (§10).
- **Implementation map (§11):** existing and new modules (`explorerBridge/`, `explorerSuggestions/`, `explorerPlanner/`) and VS Code MCP provider registration pseudocode.
- **Tests:** acceptance tests AT-01 to AT-29 and fixture guidance.
- **Phases:** P0 foundation, P1 advanced assistance, P2 orchestration, P3 ecosystem, with a recommended build order and open decisions.

**Useful to a planner.** This document defines the agent-facing contract and the trust boundary. Keep its invariants: drafts are inert, the user decides, and there are no generic commands. The §11 module map and AT-23/AT-26 ("exactly 15 tools") have to be restated against 16 tools and the 1.4 service modules.

## 3. AIRK_Explorer_Codex_Implementation_Plan_v1.4.md: consolidated implementation plan (commentary)

**What it is.** "AIRK Explorer: Unified Implementation Plan for Codex", about 2,050 lines. It was written on 24 Oct and later retargeted to AIRK 1.4 (`airk` service plus `airk-embed`). It consolidates the earlier prototypes, a design review, a strategies design document and the requirements document into one specification for Codex. It is the most detailed document here, and also the most repetitive.

**Proposals.**
- **Packaging and ownership:**
  - one VSIX
  - the extension host owns all service calls through `AirkServiceClient`, with an `EvidenceClient` read-only facade and a separate confirmed management facade
  - the webview never touches the service or the filesystem
- **Navigation:** seven workspaces (Search, Code, Review, Evidence, Strategies, Reports & Packs, Management) driven by an `EXPLORER_ROUTES` manifest, plus a global Agent Suggestions inbox.
- **Contracts:**
  - types: `ToolEnvelope`, `EvidenceRef`, `StrategyRunManifestV1`, `PlanTask`, `PlannerDraft`
  - schema IDs: `airk.explorer.{search-recipe,review-rule,strategy,run,plan}/1`
  - evidence classes E1 (indexed text), E2 (syntax fact), E3 (rule observation) and E4 (IDE/provider-asserted)
  - truthful counts, in the form "4 returned · limit 10 · more may exist"
- **Studio and Library:**
  - a Studio DSL whose operators are `airk.*`, `host.gitDiff`, `host.readSavedText`, `host.hashSavedFiles`, `host.diagnostics` and `core.*`
  - default budgets: 24,000 bytes, 60 calls, fan-out 24, 120 s, concurrency 4
  - 25 `airk.core/*` templates, three of them runnable (`bug`, `review`, `test-unit`)
- **Review engine:** query-driven rules, fingerprints that survive line moves, and baselines in `.airk/review-baseline.json`. Plans are saved to `.airk/plans/`.
- **Repository layout (§12):** `packages/vscode/src/explorer/{contracts,host,review,strategies,planner,suggestions,ui}/` and `explorerBridge/*`.
- **Tests (§14):** matrices EX, ST, MCP, UI and SVC.
- **Roadmap (§15):** R0 contract freeze → R1 Search/Code/Evidence → R2 review beta → R3 trace/packs/reports → R4 Strategies + Planner + Assistance MCP → R5 Management convergence → R6 optional dispatch and CLI. It also lays out a 14-PR sequence and an R4 Definition of Done.

**Useful to a planner.**
- Treat it as a parts catalog: contracts, test IDs and the authorization matrix.
- Its claims about 1.4 (service discovery, the management API, the supervisor) are assumptions.
- Where it goes beyond bounded retrieval, check it against the project scope rule. Watch especially the dependency queue, dispatch adapters, the `airk-explorer queue` CLI, persona presets and remote service binding.
- The forced-desktop requirements (§11.4–11.5, UI-01–04) were later descoped.
- Jump-to line ranges:
  - L18–39: precedence
  - L78–101: invariants and shipped vs proposed
  - L117–184: routes
  - L301–362: service contract
  - L366–469: tools and evidence classes
  - L606–781: templates, DSL and compiler
  - L851–997: Planner and save
  - L1081–1270: Assistance MCP
  - L1304–1328: Management
  - L1525–1611: layout
  - L1826–1947: roadmap

## 4. AIRK_Explorer_Plan_v1.4_Review_and_Recommendations.docx: review of the Codex plan

**What it is.** A short critique of document 3, dated 24 Oct 2026. Its verdict: the plan is "strong on safety and evidence truthfulness, but not yet executable as a Codex brief."

**Keep, it says:**
- the three-plane trust split
- evidence classes E1–E4 and truthful pagination
- publication pinning per run
- query-driven review and line-move-resilient fingerprints
- Planner save through the real VS Code host with readback
- portable prompts
- keeping the K1–K6 backend enhancements out of the baseline

**Critical issues C1–C8:**
- **C1:** no 1.4 service contract. This is now largely answered by [docs/airkservice-architecture.md](../airkservice-architecture.md).
- **C2:** no tie-break when the checkout contradicts the target.
- **C3:** the Assistance MCP launch path is unstable. It proposes a stable launcher shim in `globalStorageUri` and `ELECTRON_RUN_AS_NODE`.
- **C4:** the host cannot prove a physical click, so the native save dialog or a modal should be the consent gate.
- **C5:** two error registries that disagree.
- **C6:** 1.3.1 residue such as `retrievalHelperVersion`.
- **C7:** determinism depends on stable service ordering.
- **C8:** a pointless compatibility field.

**Other recommendations.**
- **Plan hygiene:** numbered invariants INV-1 to INV-10, RFC 2119 wording, and requirement IDs traced to tests.
- **Configuration and lifecycle:**
  - a location-agnostic `CollectionBinding`
  - a `StorageMigrator`
  - named `contributes.configuration` keys and `airk.explorer.feature.*` flags
  - activation and bundle budgets
- **MCP and localization:** help as MCP resources, templates as MCP prompts, and `vscode.l10n`.
- **Security:**
  - IPC rendezvous and credential handoff
  - path checks for non-file schemes
  - a named Markdown sanitizer, and no `command:` URIs
  - an SBOM
  - a telemetry default and a pack-signing decision
- **Planner:**
  - merge the Queue into the Planner until dispatch exists
  - YAML front matter in saved plans
  - a completion-report loop with a citation checker
  - label the 22 templates that cannot run as Preview
- **UI:**
  - collapse the 12-element header into one status pill
  - use native surfaces (Problems, TreeView, status bar, walkthroughs)
  - qualify the name "AIRK Explorer" to avoid confusion with VS Code's built-in Explorer
  - design the empty and disconnected states
  - drop forced desktop from the VSIX gate
- **Testing:** tag every test with gate, requirement and level; run a real-service CI lane; snapshot the tool contract; commit fixtures.
- **Resequenced roadmap:**
  - R0 contract
  - R0.5 walking skeleton (Search end to end)
  - R1 retrieval workbench
  - R2 review beta
  - R3 Strategies + Planner
  - R4 Assistance MCP + Inbox
  - R5 trace, packs, reports
  - R6 Management convergence
  - R7 adapters, CLI, Queue

**Useful to a planner.** This is the best short checklist of what to fix in any plan derived from document 3, and its consolidated P0/P1/P2 change table is ready to adopt. Its recommendation for real-service CI and nightly Windows runs must be reconciled with the lean-CI rules in [CONTRIBUTING.md](../../CONTRIBUTING.md) and [docs/ci-and-builds.md](../ci-and-builds.md).

## 5. AIRK_Explorer_Implementation_Spec_from_Claude.md: executable spec applying the review

**What it is.** An "executable specification" for Claude Code, dated 24 Oct 2026 (596 lines), to build Explorer inside the existing `iokaio.airk` extension against the 1.4 service. It adopts document 4's recommendations on top of document 3. It cites a prototype v2 that is not in this folder. It never cites the requirements .docx, and it ranks itself above the other documents, which inverts the authority set out above.

**Contents.**
- **Invariants:** INV-1 to INV-10. INV-2 freezes "the 15 tools", which is now wrong because `airk_help` makes 16.
- **WP0 repository discovery:** search commands and a question table, with conflicts logged to `docs/explorer/discrepancies.md`.
- **Module layout** under `packages/vscode/src/explorer/`:
  - `contracts/`, `host/`, `review/`, `strategies/`, `planner/`, `suggestions/`, `management/`, `webview/` (built with esbuild, with a CSP nonce)
  - `help/` (`catalog.build.ts`)
  - `explorerBridge/` and `explorerIpc/`
  - import boundaries enforced with ESLint `no-restricted-imports`
- **Contracts:**
  - route availability values `implemented|prototype_only|unsupported|disabled`
  - runtime gates `canRetrieve` and `canVector`
  - `ToolEnvelope`, an `ExplorerErrorCode` registry, and a message envelope `{v:1,type,requestId}` with its message catalog
- **Shell:** view `airk.explorer`, the command "AIRK: Open Explorer", a 224 px nav rail, a status pill, a fixed CSP and breakpoint targets.
- **Screens:** Search, Code, Evidence, Review (built-in rules first, then SARIF), Strategies (a nine-step `StrategyRunner` and a deterministic prompt order), and a Planner with front matter, Save As and completion reports.
- **Management:** nine tabs, which reuse the existing `sidebar.ts` and `diagnosticsView.ts` handlers.
- **Assistance MCP:** launcher, per-window IPC descriptor, seven P0 tools and rate limits.
- **Settings:** 13 `airk.explorer.*` settings plus feature flags, persistence and migrations.
- **Testing:** fixture generator, six CI lanes and a REQ→test map.
- **Delivery:** work packages WP0–WP10 with day estimates. The §13 delivery table did not survive export. The Definition of Done falls at the end of WP7, and there is a per-PR report template.

**Useful to a planner.**
- It is the closest of these documents to a buildable work breakdown, with module boundaries, message catalogs and settings.
- Its proposed `AirkServiceClient` and management facade must be reconciled with the existing `serviceClient.ts` and `serviceConsole*.ts`.
- Its CI lanes and performance budgets are speculative.
- Jump-to line ranges:
  - L20–33: invariants
  - L53–72: discovery
  - L78–131: layout
  - L137–227: contracts
  - L331–387: runner and Planner
  - L399–416: Management
  - L420–452: Assistance MCP
  - L456–494: settings
  - L515–543: test map
  - L562–593: Definition of Done

## 6. grok_design_review.md: design record for prototype v2 (commentary)

**What it is.** A design record for a later revised prototype ("Prototype v2"), which is **not** the HTML in this folder. It explains which recommendations that prototype followed and why. That prototype labels its service as 1.4.0, protocol 2, and refuses protocol 1.

**Decisions it records.**
- **Navigation:**
  - The earlier role-based IA and the four-tab Studio are superseded.
  - It uses eight workspaces: Overview, Search, Code, Review, Evidence, Strategies, Reports, and Management (pinned, nine tabs).
  - There are hash routes from one manifest and a Ctrl/⌘K command palette.
  - Overview offers six journeys: Explore, Investigate, Review, Prepare, Hand off, Maintain.
- **Planner:** the queue is folded into the Planner. Task states are Draft, Ready, Exported, Review, NeedsWork, Completed, Blocked and Stale, and Blocked is derived from dependencies.
- **Evidence:**
  - Evidence classes E1–E4 with fail-closed connection states: `ready`/`degraded` allow retrieval, while `disconnected`/`incompatible` do not.
  - Vector controls stay visible but disabled, with a reason.
  - Citation checking accepts `path:line` or `path#Lx-Ly`.
  - Watches are triggered by a publication-token change, not by polling.
- **Library:** three runnable templates; the other 22 are skeleton-only.
- **Operators:** a closed operator catalog with `airk.*`, `host.*` and `core.*` producers.
- **Plan format:** `airk.explorer.plan/1` with front matter, and the Planner save contract with path rules.
- **Wording:** a list of honest limitations, such as "Ready is not integrity" and "Exported is not Completed".
- **Visual system:** IBM Plex Sans and Mono, slate with a teal accent, 44 px targets, a navigation drawer below 960 px.

**Useful to a planner.** This is the clearest single statement of the revised information architecture and its UX rules. It is the bridge between the role-grouped navigation of the controlling prototype and the seven- or eight-workspace layout of documents 3–5. Adopt it only by an explicit decision; it does not override the controlling prototype.

## 7. AIRK_1.4_Competitive_Research.md: market and pattern research (background)

**What it is.** A report dated 25 Oct 2026 (about 1,180 lines) comparing 17 VS Code and MCP products from their Marketplace listings and READMEs. Nothing was installed or benchmarked. Products include CodeExplorer, CodeGraph, SocratiCode, Gortex, Redcon, RepoForge, Blueprint, Ralphdex, SonarQube for IDE, Continue and Cline. It explicitly says it is not an approved specification.

**Findings relevant to Explorer.**
- **Opportunity register:** E01–E24, plus holds H01–H05.
- **Service health (E01):** one diagnostic surface for service version, collection, publication and errors, with `airk-embed` readiness kept separate and named failure states.
- **Honest-incompleteness vocabulary (Wedge C):** found, not found, not indexed, truncated, stale, ambiguous, syntax candidate, provider-asserted, verified, agent-claimed.
- **Review output:** claim, rule version, fingerprint, evidence class and uncertainty. An empty match is not a pass.
- **Code views:** small expandable subgraphs that explain each edge, instead of a busy MCP console.
- **Task shortcuts:** composed in the extension host from existing tools, such as "locate related tests".
- **Planner states:** prepared, reviewed, saved, handed off, agent-reported done, verified.
- **Budget ledger:** keeps source bytes, response bytes, estimated tokens and billed tokens apart.
- **Assistance MCP:** keep it narrower than VS Code's own MCP surface.
- **Its own advice:**
  - Do not grow the native tool count.
  - Do not rebrand as a graph database or an agent.
  - Lead with mixed-artifact evidence-to-plan workflows.
- **Out of scope:** its holds (autonomous loops, arbitrary query languages and plugins, new native schemas, historical search, agent-originated save/repair/dispatch) and orchestration-style competitors.

**Useful to a planner.** Use it for differentiation and UX patterns, not for requirements. Jump-to line ranges:
- L62–117: baseline and planned Explorer
- L250–311: service health and graph UX
- L331–402: Planner and handoff
- L549–563: wedges C and D
- L583–640: opportunity register and P0 program
- L786–899: prototype specs

---

## Decisions a plan must make explicitly

1. **Information architecture.** Choose between the controlling prototype's role-grouped navigation (about 21 pages plus Index Management) and the consolidated seven or eight workspaces (documents 3–6).
2. **Help.** Choose whether to extend the existing Rust `airk_help` or add an extension-side `airk_explorer_help_*` catalog as the requirements document specifies. Then restate the invariant on the native tool count as 16 tools.
3. **Service client.** Decide whether to reuse or extend `serviceClient.ts` and the Service Console, or introduce the proposed `AirkServiceClient`/`EvidenceClient` layering.
4. **Panel migration.** Decide how the current Dashboard, Service Console and Workspace Projects views and 37 commands map into Explorer Management without losing any operation.
5. **Scope.** Decide which of these stay within "local file discovery, native extraction, indexes and bounded retrieval":
   - Studio prompt compilation
   - Planner dependency gating
   - the review rule engine and SARIF
   - packs
   - the Assistance MCP and Inbox
   - dispatch adapters and the `airk-explorer` CLI

   Most commentary defers dispatch and the CLI; keep that.
6. **Sequencing.** Choose between document 3's R0–R6 and document 4's R0–R7, which adds a walking skeleton and moves Strategies before Assistance.
