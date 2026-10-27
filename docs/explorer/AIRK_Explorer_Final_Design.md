# AIRK Explorer: Final Design

**Status: approved for implementation, 26 October 2026, targeting AIRK 1.5.0.** The maintainer decided every item marked **Decision**; §15 records the outcomes. Phases P0–P5 are in scope, and P6 needs separate approval. Statements about a feature describe the design. A feature is shipped only when its route's availability in the feature manifest says `implemented`.

**Inputs.** This document consolidates the folder's sources, which are summarized in [README.md](README.md):
- **Controlling sources:**
  - [AIRK_Explorer_UI_Prototype.html](AIRK_Explorer_UI_Prototype.html), the primary authority for screens and interaction.
  - [AIRK_Explorer_Local_MCP_Agent_Assistance_Requirements.docx](AIRK_Explorer_Local_MCP_Agent_Assistance_Requirements.docx), the core feature contract. "Requirements" below refers to this document.
- **Commentary:** the Codex plan, its review, the Claude spec, the grok design record and the competitive research. These are adopted only where this document says so.

**Code baseline.** The design is checked against `main` at 1.4.4 (commit `217f226`) and supersedes nothing in that code. Where the sources and the code disagree, this document follows the code and records the change.

**Readers.** Maintainers deciding scope, and the implementing agents who will turn each phase into PRs. Repository rules in `CLAUDE.md`/`AGENTS.md`, [CONTRIBUTING.md](../../CONTRIBUTING.md) and [docs/architecture.md](../architecture.md) apply to all of it.

---

## 1. Summary

AIRK Explorer replaces the extension's current management surface with one workbench. That surface is the Service Console webview plus a static getting-started dashboard. In the workbench a developer can:

1. **See and manage the index honestly:** service, projects, jobs, storage, models, agents, usage, recovery. Every existing control is kept, organized into tabs and states instead of one long page.
2. **Use the index directly.** Search, inspect symbols, references and documents, check citations and jump to source. All of this uses the 16 existing read-only `airk_*` MCP tools, with no new native endpoints.
3. **Prepare work for a coding agent.** Compile bounded, cited evidence into an editable prompt or a saved Markdown plan that Claude Code, Codex, Copilot or another agent can follow.
4. **Let a coding agent help without granting it authority.** A separate extension-owned *Explorer Assistance* MCP server answers how-to questions and drops typed, inert drafts into a Suggestions Inbox. Only the user's own actions apply or run them.

The Explorer adds no inference, agent orchestration, dispatch, cloud calls or new native retrieval endpoints. It composes what AIRK already exposes. Where the controlling prototype predates the 1.4 service, this document changes the prototype and says why (§6.2).

---

## 2. Problem statement (observed on `main`)

| Observation | Evidence | Consequence |
|---|---|---|
| The live management UI is one long webview with about 40 buttons and about 60 fact rows, rendered as `<dl>` lists. | `serviceConsoleView.ts` `consoleContent()`, `projectRow()` | No hierarchy, no task flow, and project rows are walls of cached fields. Hard to scan and hard to act on. |
| The sidebar "Dashboard" is static getting-started text. | `serviceDashboard.ts`; `serviceExtension.ts:399-405` | No live state where users look first. |
| Many palette commands send the user to the same console instead of the specific task: `airk.collections`, `extractionPolicy`, `ingestionLimits`, `storageLimits`, `removeCollection`, `menu` and others. | `serviceExtension.ts:415` | Commands promise features the UI does not deliver. |
| There is no way to *use* the index from the editor except a terminal REPL (`Open AIRK Tools`) or printing `airk_status` to the Output channel. | `serviceExtension.ts:200-210` | The product's main value (cited retrieval) is invisible to the human user. |
| Several 1.3-era modules are no longer wired to activation. | `extension.ts` re-exports `serviceExtension.ts`, and only the `McpVerification` type is imported from `sidebar.ts`. | `sidebar.ts`, `dashboardView.ts` and `diagnosticsView.ts` are legacy. The source documents' file maps point at them and are stale. |

---

## 3. Goals, non-goals and invariants

### 3.1 Goals

These are the requirements' goals G-01 to G-06, adjusted for 1.4.4:

| ID | Outcome | Measure |
|---|---|---|
| G-1 | Any existing management action is at most two clicks from the Explorer, with its state and consequences visible. | Every current command and console action maps to a route (§7.8, Appendix B). |
| G-2 | A developer can search, inspect and open cited evidence without an agent. | Search → result → source at the exact line, with an indexed-vs-live check. |
| G-3 | A developer can produce a bounded, cited prompt or a saved plan for any agent. | Studio/Planner output passes the citation checker. The plan is saved through VS Code and opened. |
| G-4 | An agent can answer "how do I…" for the installed version and propose drafts without silent effects. | Requirements AT-01 to AT-29, restated for 16 tools (§13). |
| G-5 | Users can always tell what is implemented, stale, partial, inferred or proposed. | Shared state vocabulary (§9). No fabricated counts. |

### 3.2 Non-goals

These follow the requirements' non-goals (§2.3) and `CLAUDE.md`:
- No model calls inside the Explorer, including "explain this file". No generative step.
- No agent dispatch, task queue execution, worktree management or companion `airk-explorer` CLI.
- No new native `airk_*` tools or schemas. No retrieval through the control transport. No external database or cloud provider.
- No generic MCP tool for running a command, writing a file, running a shell or setting configuration.
- No claim of resolved references, semantic understanding, coverage or cross-platform qualification beyond the evidence AIRK actually returns.

### 3.3 Invariants

Every phase gate checks these. They consolidate the review's INV list, corrected to the current code.

| ID | Invariant |
|---|---|
| INV-1 | **One VSIX.** The Explorer, its webview and the Assistance MCP server ship in the existing `iokaio.airk` extension. |
| INV-2 | **The native retrieval MCP is unchanged.** It has exactly **16** read-only tools, `airk_help` included. A tool-inventory snapshot test enforces this; `probeMcp` already checks the count. |
| INV-3 | **The extension host is the sole authority.** Only the host talks to the service (control transport) or to retrieval (MCP evidence session), and only the host reads or writes files. The webview has no Node, filesystem or network access. |
| INV-4 | **The Assistance MCP never calls Rust.** It never spawns, proxies or falls back to the native retrieval worker. It serves bundled help or forwards typed drafts to the host over authenticated local IPC. |
| INV-5 | **Drafts are inert.** Applying a draft copies it into a UI draft. Running, saving, indexing, repair and configuration each need the user's own action, plus the existing native confirmations. |
| INV-6 | **Evidence is qualified.** Every displayed result names its producer, evidence class, publication token and limits. |
| INV-7 | **Compound work is pinned.** A Studio run or Planner snapshot pins one collection and one publication token. A token mismatch fails closed. |
| INV-8 | **Counts are never fabricated.** The UI shows "N returned · limit L · more may exist". "Load more" appears only where the tool supports a cursor (§9.3). |
| INV-9 | **Engine states are separate.** Service health, work progress, published readiness, source freshness and semantic coverage are shown separately and never merged into one "OK". |
| INV-10 | **Collection isolation.** One Explorer session binds to one registered project at a time. No query spans collections. |
| INV-11 | **Mutations are preserved, not replayed blindly.** All mutations reuse `ServiceClient` + `IntentJournal`. An unknown outcome is rechecked with the same request key and never regenerated. |

---

## 4. Baseline the Explorer builds on

### 4.1 Two channels already exist

```
                    ┌──────────────── VS Code extension host ────────────────┐
 Explorer webview ◄─┤ ExplorerController (new)                               │
   (UI only)        │   ├─ control plane:  ServiceClient → NativeControlTransport ──► airk service console
                    │   │     projects, workers, jobs, plans, configure_project, lifecycle
                    │   └─ evidence plane: explorerEvidence.ts on mcpStdioSession.ts (new) ──► airk runtime launch … mcp
                    │         the 16 read-only airk_* tools, one bound project       (stdio MCP frontend → worker)
                    └────────────────────────────────────────────────────────┘
```

- **Control plane.** It exists in `serviceClient.ts`, `serviceIntents.ts`, `serviceConsole*.ts` and `serviceBootstrap.ts`. The transport is length-prefixed JSON with a 64 KiB frame cap, up to 8 queued requests and a 20 s deadline. It is documented as never carrying evidence or credentials.
  - **Operations it supports:**
    - project pages and worker observations
    - jobs: `index`, `reconcile`, `vectors`, `model_setup`, `verify`, `inspect`, `recover`, `maintain`
    - retry and cancel
    - repair plans: `auto`, `verify`, `recover`, `reconcile`, `vectors`, `rebuild`
    - pause/resume and watch configuration
    - lifecycle: start, stop, restart, upgrade, rollback, payload repair
    - embedding helper start and stop
    - diagnostic preview
  - **Project configuration operations** (`configure_project`):
    - indexing content: `detail`, `vectors`, `vector_selection`, `install_model`, `extraction_policy`, `scan_limits`, `reset_scan_limits`
    - roots and quotas: `add_root`, `remove_root`, `quota_policy`
    - usage: `usage`, `clear_usage`, `import_usage`, `usage_feedback`, `calibrate`
    - registration and cleanup: `unregister`, `prepare_derived_cleanup`, `delete_derived`
- **Evidence plane.** It exists only in fragments. `probeMcp` in `native.ts` speaks MCP JSON-RPC over stdio to `launchArgs(spec,'mcp')`, and `showTool` spawns `tools call` once per request. The Explorer needs a persistent, bounded client (§5.3).

### 4.2 Retrieval capabilities (from `docs/mcp.md`)

| Need | Tool(s) | Notes that drive the UI |
|---|---|---|
| Full-text and semantic search | `airk_search` | `mode`: lexical, vector or hybrid. Omitting it uses the collection policy. Also `path_prefix`, `limit` (1–100), `max_bytes` (≤65,536) and `full_chunks`. Snippets are ≤768 B per hit. **No cursor.** |
| Read and expand | `airk_read_file`, `airk_context` | Conditional reads via `if_evidence`/`known_hash`/`artifact_id`. The response includes `read.status`, which can be `not_modified`. |
| File facts | `airk_file_info`, `airk_outline` | Language, parse status, hash and line basis. The outline supports a cursor and `compact`. |
| Code structure | `airk_symbols`, `airk_code_facts`, `airk_imports` | Cursors. Exact names; SCIP facts are producer assertions. |
| Occurrences and edges | `airk_references`, `airk_relationships` | References are textual, with **no resolution and no cursor**. Relationships are 1–3 hops, 100 symbols, 256 files, **no cursor**. |
| Documents | `airk_document_facts` | `kind` (requirement, decision, date, table_row…) and original locations. Cursor. |
| Change and validation | `airk_changes` | Remembered-citation validation and freshness. Cursor. |
| Readiness | `airk_status`, `airk_capabilities` | Publication token, `index_state`, semantic readiness, `runtime` bundle, `work` sample. |
| Help | `airk_help` | 35 topics (`quickstart`, `search`, `jobs`, `freshness`, `troubleshooting`, `agent-setup`…), CLI command help and tool schemas. Needs no index or service. |

### 4.3 Extension facts that constrain the design

- Build is `tsc -p .`, tests are `node --test`, and the only runtime dependency is `jsonc-parser`. There is no bundler today.
- Existing webviews use a nonce CSP and server-rendered HTML strings. Webview messages are fixed action names validated by allowlist (`consoleAction`), and the webview may never send paths, commands or RPC bodies. Keep this pattern.
- Settings today: `airk.mcp.responseMode`, `airk.mcp.registrationEnabled`, `airk.cli.autoInstall`, `airk.storage.path`, and `airk.service.*`. All are machine-scoped.
- The Activity Bar container `airk` holds three views: `airk.dashboard` (webview), `airk.service` (webview) and `airk.collectionsView` (tree). There is also the `airk.gettingStarted` walkthrough and the `airk.collections` MCP server definition provider.
- Native ingest and the watcher exclude any `.airk` directory (`airk-ingest/src/lib.rs`, `airk-kernel/src/watcher.rs`).
- Usage accounting attributes calls by the `AIRK_CLIENT` label (`airk-core/src/usage.rs` `ClientFamily::from_label`). `AIRK_USAGE_MODE=disabled` opts a process out (`airk-kernel/src/usage.rs:76`).

---

## 5. Architecture

### 5.1 VS Code surfaces

**Decision D-01: where the Explorer lives.** Recommendation: use VS Code's surfaces for what each does best. Do not force everything into the narrow sidebar the prototype assumes.

| Surface | Content | Rationale |
|---|---|---|
| **Editor-area webview panel** "AIRK Explorer" (`airk.explorer.open`) | The full workbench: navigation rail, workspaces, source drawer, Inbox. One panel per window, revealed rather than duplicated. | The prototype's layout needs editor width. Developers already keep side-by-side editor tabs. |
| **Sidebar webview view** (replaces the static `airk.dashboard`) | A compact **status card** and a **quick search box**, followed by: <ul><li>*Open Explorer*</li><li>pending-suggestion count</li><li>first-run steps while setup is incomplete</li></ul> | This is where users look first. It must show live state, not static text. |
| **Status bar item** (existing, `airk.menu`) | One status pill: `AIRK ● ready · embed ● · fresh`, or the worst current state. Click opens Explorer › Overview. | The review's "one status pill" recommendation, using the existing item. |
| **Workspace Projects tree** (existing) | Kept. Context menus deep-link to Explorer routes. | Native multi-project navigation. |
| **Walkthrough** (existing) | Kept for first run. Steps link to Explorer routes. | Native onboarding. |
| **Problems panel / CodeLens** | Not in the initial phases. Considered only with Review rules (P6). Opt-in. | The competitive research's warning about intrusive CodeLens. |

The existing `airk.service` Service Console stays available as a hidden **Advanced: raw service observations** view until Management reaches parity (§14 P2). Then it is removed and its commands are redirected.

### 5.2 Process and trust planes

```
 ┌─────────────── window extension host (authority) ───────────────┐
 │ ExplorerController ── RouteManifest / FeatureManifest           │
 │   ├─ ControlFacade (wraps ServiceClient + IntentJournal)        │──► native control transport
 │   ├─ EvidenceClient (explorerEvidence.ts)                       │──► airk … mcp  (bound project)
 │   ├─ WorkspaceIO (save plan, open file, hash saved file, Git)   │──► vscode.workspace.fs / vscode.git API
 │   ├─ SuggestionStore (inbox, TTL, idempotency)                  │
 │   └─ AssistanceIpcHost (per-window pipe/socket, session creds)  │◄── Explorer Assistance MCP (Node stdio)
 └──────────────────────────────────────────────────────────────────┘
        ▲ typed {v:1,type,requestId} messages only                    ▲ launched by Copilot/Claude Code/Codex
 Explorer webview (CSP nonce; no Node/fs/network; no command URIs)    serves bundled help directly;
                                                                      drafts → host IPC → Inbox
```

### 5.3 Evidence session

**Decision D-04 (decided): how the Explorer calls retrieval.** Add a long-lived evidence session per window, bound to the currently selected registered project.
- **Launch.** Start the same verified launcher with the same arguments as agents: `spec.command`, `launchArgs(spec,'mcp')`. The JSON-RPC framing moves out of `probeMcp` into a shared `mcpStdioSession.ts`. The session keeps the 1 MiB response ceiling and identifies itself with `clientInfo.name: "airk-explorer"`.
- **Engine limits on one connection** (checked in code):
  - The stdio bridge accepts 4 in-flight calls and rejects a 5th (`airk-mcp/src/remote.rs`).
  - The worker connection serializes calls (`airk-cli/src/project_client.rs`).
  - Replies time out after 120 s.
  - `notifications/cancelled` drops the reply but does not stop the work.
  - So the session runs its own queue with at most 3 calls in flight. A newer Search supersedes an older one and its result is discarded, and late replies after the UI deadline are ignored.
- **Lifetime.** Restart lazily. Dispose on project switch, registration change, trust loss or runtime upgrade. The disposal triggers are the same as `ServiceExtension.reset()` and the `isCurrent(spec)` checks.
- **Bounds:**
  - at most 4 concurrent calls and 15 s per call
  - cancellation via MCP `notifications/cancelled`
  - responses are validated as one JSON text block before reaching the webview
- **Response mode.** Terse by default, since it keeps the publication token, citations, hashes and truncation. Use `response_mode:"full"` per call only on Evidence › Inspector and Overview diagnostics.
- **Rejected alternatives:**
  - `tools call` per request: one process spawn per call means too much latency for interactive search.
  - New native query RPCs: these would violate INV-2 and the control plane's "no evidence" contract.

**Decision D-05 (decided): usage attribution.** Searches the user runs in the Explorer would otherwise mix into agent usage and distort *Usage & Savings*.
- **Why environment variables can't be used.** In 1.4 service mode the project worker records usage, not the stdio frontend. `AIRK_USAGE_MODE` and `AIRK_CLIENT` set on the frontend never reach the worker. The worker takes the client family only from the MCP `initialize` `clientInfo.name`, through `ClientFamily::from_label` (`airk-mcp/src/remote.rs`, `airk-cli/src/project_client.rs`).
- **Decision.** Add `ClientFamily::Explorer` with the label `airk-explorer`. Usage reports then show Explorer queries as their own client.
- **Rollback limitation.** A 1.4.x runtime cannot read a usage ledger that contains the new `explorer` key. The 1.5.0 release notes record this.

### 5.4 Webview implementation

**Decision D-03 (decided): webview stack.** An eight-workspace interactive app is not maintainable as server-rendered HTML strings.
- **esbuild** (exact-pinned dev dependency) builds one webview bundle and one Assistance-server bundle.
- The webview uses **Preact** (TSX), also a dev dependency. It is bundled into `out/explorerWebview.js` with its MIT license text embedded in the bundle header.
- `@vscode/webview-ui-toolkit` is deprecated and not used.
- Styling uses VS Code CSS variables. Codicons are deferred because the font's CC-BY-4.0 license would need a new asset class and attribution; icons are text or inline SVG.
- Keep the existing CSP pattern: `default-src 'none'`, nonce for scripts and styles, `localResourceRoots` restricted to the bundle directory. Set `enableCommandUris:false` on the Explorer panel.

Budgets (adopted from the review, measured in CI where cheap):
- ≤20 ms added to activation; the Explorer code loads lazily on first open.
- Webview bundle ≤1.5 MB gzipped.
- Assistance server cold start ≤300 ms.

**Decision D-13: visual system.** Follow the prototype's *layout and density*: rail, panels, evidence rows, badges and source drawer. Replace its custom palette and IBM Plex fonts with VS Code theme tokens (`--vscode-*`) and the user's editor font for code, paths and hashes. Status always pairs a word with a hue (prototype rule). There is no forced 1440 px desktop mode in the VSIX; that was a standalone-HTML preview device.

### 5.5 Module layout

The extension's packaging only stages an explicit list of **flat** `out/*.js` modules (`scripts/package-extension.py` `MODULES`), and its tests run the non-recursive glob `out/test/*.test.js`. So the Explorer's host code stays flat, with an `explorer` prefix. Only the two bundle sources live in subdirectories, and those are compiled by esbuild, not packaged module by module. Existing modules are reused, not duplicated.

```
packages/vscode/
  tsconfig.json              host + tests (tsc → out/); excludes the two bundle-source trees
  tsconfig.webview.json      noEmit typecheck: Preact TSX, DOM lib, no Node or vscode types
  tsconfig.assistance.json   noEmit typecheck: Node types, no vscode types
  scripts/build-explorer.cjs esbuild driver: input allowlist, Preact license banner, help catalog
  src/
    explorerContracts.ts, explorerRoutes.ts, explorerFeatures.ts, explorerErrors.ts   pure, shared by all three
    mcpStdioSession.ts       JSON-RPC stdio framing extracted from native.ts probeMcp
    explorer*.ts             flat host modules: controller, panel, sidebar, evidence, control,
                             workspace IO, strategies, planner, suggestions, assistance host/launch, review
    explorerWebview/         Preact app → out/explorerWebview.js + out/explorerWebview.css
    explorerAssistance/      Node stdio MCP server → out/explorerAssistance.js
    explorerHelp/*.md        Explorer help topics; embedded into both bundles at build time
    test/explorer*.test.ts
```

Boundary rules. There is no ESLint, so `explorerBoundaries.test.ts` and the separate typecheck configurations enforce them:
- `explorerWebview/` imports only Preact and the pure contract modules. Its typecheck config has no Node or `vscode` types.
- `explorerAssistance/` imports only `node:` built-ins and the pure contract modules. It never imports `serviceClient`, `native`, `model`, `mcpStdioSession` or `vscode`.
- Only host modules import `vscode`, the control plane or the MCP session.
- The esbuild driver fails the build if anything outside the allowlisted inputs enters a bundle. Packaging also rejects a bundle that contains host-class markers. Together these keep `ServiceClient`, `IntentJournal` and `probeMcp` from shipping a second copy.

---

## 6. Information architecture

### 6.1 Navigation

The prototype has about 21 role-grouped pages plus a 10-tab Index Management area. That is too many top-level destinations for an editor tab rail. Commentary documents 3–6 collapse the screens into seven or eight workspaces.

**Decision D-02: navigation structure.** Recommendation: keep **every prototype screen** that the current code can support, but organize them as **8 workspaces with tabs**, with Management pinned at the bottom. Keep the prototype's *roles* as Overview "lenses": Developer, Tester, Architect, Writer. A lens pre-filters journeys and defaults and never hides a workspace.

| Workspace (route) | Tabs (route suffix) | Prototype source screens | Phase |
|---|---|---|---|
| **Overview** `#overview` | — | home, index (freshness and limitations) | P0 |
| **Search** `#search/*` | Query, Recipes | search | P0 (Recipes P3) |
| **Code** `#code/*` | Symbols, References, Outline, Map | symbol, references, architecture (map) | P1 (Map P6) |
| **Documents** `#docs/*` | Facts, Drift | docs, doc-drift | P1 (Drift P6) |
| **Evidence** `#evidence/*` | Inspector, Checker, Changes | evidence, exports (citations) | P1 |
| **Review** `#review/*` | Changes, Impact, Tests, Quality | code-review, change-impact, tests, quality, quality-detail, architecture-rules | P5 (Quality P6) |
| **Strategies** `#strategies/*` | Library, Studio, Planner | strategy-library, strategy-studio, strategy-planner | P3 |
| **Management** `#manage/*` (pinned) | Projects, Activity, Repair, Indexing, Storage, Service, Agents, Usage, Logs | index-manager (all opsTabs) + current Service Console | P0 (Projects), P2 (all) |
| *Help* `#help/*` (drawer) | — | (new; requirements §4) | P1 |
| *Agent Suggestions* (header inbox) | — | (requirements §6.1) | P4 |
| *Reports & Packs* | Builder, Packs, Rules | pack-center, rule-inspector, report-builder | **Deferred** (§14 P6) |

Global chrome, simplified per the review:
- project selector
- **one status pill** that expands into a popover showing service, worker, publication, freshness, semantic coverage, embed helper and response mode
- Inbox badge
- Ctrl/⌘K command palette over the route manifest
- Help button

Results limits live on each screen, not in the header.

Every route has an `availability` value of `implemented`, `prototype_only`, `unsupported` or `disabled`, held in one `features.ts` manifest. This manifest drives the rail, the help catalog's applicability field (requirements H-03) and `airk_explorer_ui_status`.

### 6.2 Changes to the prototype

The prototype stays primary for layout, vocabulary and interaction rules. These changes come from the code baseline or the commentary.

| # | Prototype element | Change | Why |
|---|---|---|---|
| 1 | "AIRK 1.3.1 schema"; 15 tools; the "Codex implementation map" tab listing `sidebar.ts`, `dashboardView.ts`, `diagnosticsView.ts` | Label 1.4.4, use 16 tools, drop the implementation-map tab from the product (it moves to Appendix B of this document). | Stale baseline (§2). |
| 2 | Scheduling tab with debounced, idle, on-save and manual modes | Replace with native **Automatic watching (continues after windows close)** or **Manual**, plus Pause/Resume, pending source changes and sampled wait reasons. | 1.4 retired editor timers (`package.json` walkthrough text; `serviceExtension.watching()`). |
| 3 | Index Management has no service or runtime area | Add a **Service** tab: start, stop, restart, runtime upgrade, rollback and payload repair; legacy startup review; resource preset; embedding helper; response mode. | These are live operations in `serviceConsoleView.ts` that the prototype never designed. |
| 4 | Role-grouped ~21-page navigation | Use 8 workspaces with tabs plus role lenses on Overview (§6.1). | Width and scannability in an editor tab. All screens are kept. |
| 5 | Studio's fourth tab "Task queue" with dispatch policy and concurrency | Remove. The Planner owns ordered tasks, dependencies and handoff. Dispatch mode is fixed to "manual copy / file handoff". | Commentary consensus. `CLAUDE.md` excludes orchestration. |
| 6 | Studio "Automation scripting" tab and `airk-explorer` CLI commands | Keep **import/export of `strategy.airk.json`** inside Studio › Builder (Advanced). Remove the CLI panel. | The CLI is out of scope. The recipe file is useful and inert. |
| 7 | Strategy Library: 25 blueprints, all loadable and runnable | Keep all 25. Mark 3 **Runnable** (`bug`, `review`, `test-unit`) and 22 **Preview: export prompt skeleton** until each has a qualified fixture. | Honest availability (review and grok record). |
| 8 | Search mode default "Lexical, deterministic" | Interactive Search defaults to the **collection's saved policy** and shows the applied mode. Studio steps pin `mode:"lexical"` unless the template says otherwise. | Native default is partial hybrid (`docs/mcp.md`). Developers expect relevance, while strategies need repeatability. **Decision D-06.** |
| 9 | Custom fonts and palette; forced 1440 px desktop preview | Use VS Code tokens and codicons; drop forced desktop. | §5.4. |
| 10 | Evidence & vectors offers detail levels Search Only, Agent Lean, Rich Structure, Detailed | Keep, mapped to native `detail` values `essential`, `agent_lean`, `balanced` and `detailed`. Show "requires reindex" before applying. | Matches `validateConfigurationRequest`. |
| 11 | Agents tab lists 6 clients | Drive it from `model.ts` `clients` (15 labels) and existing `agentSetup`/`agentDiagnostics`. Show **two connection types**: AIRK Retrieval and Explorer Assistance (§8.5). | Code reality, plus the requirements' two-server rule. |
| 12 | Pack manager: third-party analysis packs through a VS Code extension API | Defer (P6+) with no public pack API in this program. | Scope and the security surface (unsigned third-party code). |

---

## 7. Workspace specifications

Every screen uses the shared state vocabulary (§9) and empty, disconnected and incompatible states with a one-line next action. For example, *Service disconnected → Open Management › Service*. Tool names are the existing native tools.

### 7.1 Overview (P0)

- **Status block.** One row each, never merged (INV-9):
  - service health (control `health`)
  - project registration and worker (`workers`)
  - published readiness and publication token (`airk_status`)
  - source freshness (`airk_status` freshness and watcher)
  - semantic coverage (`semantic_readiness`)
  - current work (`airk_status.work`, or the console page progress)
- **Journeys** (prototype "One index. Four jobs."): Explore, Investigate, Review, Prepare, Hand off, Maintain. These are links into workspaces, not a workflow engine.
- **Lens selector:** Developer, Tester, Architect, Writer. It changes the journey order and default filters only.
- **First-run states:** untrusted workspace, not registered, indexing in progress, ready. These reuse `serviceDashboard.ts` copy.

### 7.2 Search (P0)

- **Form.** Fields map one to one to `airk_search`:
  - query
  - mode selector (the disabled reason comes from `airk_capabilities` and semantic readiness)
  - path prefix, with folder-picker assist restricted to the bound root
  - limit, max bytes and full chunks
- **Result order.** Lexical results come back in index identity order (`stable_id`), not relevance order (`airk-store/src/current/lexical.rs`). Vector and hybrid results are ranked by reciprocal-rank fusion with a total tie-break. The screen states the ordering basis for the applied mode.
- **Result rows:**
  - path:line range, snippet and evidence class (E1 text or E2 syntax where the producer says so)
  - truncation flag and source hash
  - footer: `N returned · limit L · more may exist`
- **Row actions:**
  - *Open source* (§7.5 live check)
  - *Expand*: `airk_context` or `airk_read_file`
  - *Inspect symbol* (→ Code)
  - *Inspect citation* (→ Evidence)
  - *Add to Studio evidence*
- **Recipes (P3).** Saved argument sets (`airk.explorer.search-recipe/1`) in workspace state.
- **Sidebar quick search** runs the same request and opens the Explorer on its result.

### 7.3 Code (P1)

- **Symbols:**
  - `airk_symbols` (name, language, path prefix; cursor)
  - definition detail via `airk_code_facts` (rich on demand) and `airk_file_info`
  - imports via `airk_imports`
  - Producer badges: syntax and SCIP.
- **References:** `airk_references` textual occurrences, labeled "textual occurrence, not resolved". Results are capped with no cursor.
- **Relationships:** `airk_relationships` with hops 1–3 and direction/kind filters, rendered as a list first. Unresolved targets are dotted or labeled "candidate", and the screen states that absence of an edge proves nothing.
- **Outline:** `airk_outline` for the active editor's file. Compact by default; cursor.
- **Map (P6):** a bounded expandable subgraph that explains each edge, as the prototype's architecture screen does. Deferred until list views are proven.

### 7.4 Documents (P1)

- **Facts:** `airk_document_facts` with kind chips (requirement, decision, design, architecture, feature, acceptance, date, table_row, table_cell), name substring, language and path prefix.
  - Each record shows heading context and its original location: page, sheet or slide for PDF, DOCX, XLSX and PPTX where returned.
  - *Open original* opens the file. Office and PDF formats open in the default external app or a VS Code viewer when available.
- **Drift (P6):** compares documented names and paths with code facts and shows only literal mismatches. Deferred.

### 7.5 Evidence (P1)

- **Inspector:**
  - one citation's path, line basis, indexed hash, publication, producer, evidence class and limitations
  - `airk_file_info` and `airk_read_file` with `known_hash`
  - **Live check:** AIRK's `content_hash` is SHA-256 hex of the raw saved bytes (`airk-ingest/src/lib.rs`). The host hashes the file at `source.root` + `source.relative_path` the same way and compares. It never hashes `TextDocument.getText()`, which decodes the file and drops a BOM. The result is *matches index*, *changed since index* or *not saved / dirty buffer*. *Open in editor* reveals the line, and warns rather than pretending when the file differs.
- **Checker:**
  - paste text (an agent report, a plan, a PR description) and extract `path:line` and `path#Lx-Ly` citations
  - each citation resolves to: valid, not found, changed since index, outside the collection, or unsupported syntax
  - prose without citations is *unsupported*, never a pass
- **Changes:** `airk_changes` with `since` or selectors to validate remembered citations and show bounded observed changes. Cursor.

### 7.6 Review (P5, later than the prototype implies)

- **Changes:**
  - the VS Code Git extension API (`vscode.git` `getAPI(1)`) supplies the changed saved files against HEAD
  - AIRK supplies `airk_file_info`, `airk_outline` and `airk_code_facts` for those paths, plus the `airk_changes` freshness note
  - Git is the editor's; AIRK history is not used (no historical reads)
- **Impact:** for changed declarations, `airk_references` and `airk_relationships` candidates produce a **checklist**, not a verdict.
- **Tests:** test-path declarations from `airk_symbols` and `airk_code_facts`, filtered by configurable path and name patterns. Labeled "declared tests (candidates); not executed; not coverage".
- **Quality (P6):** a deterministic built-in rule set (query-driven, line-move-resilient fingerprints, suppression with reason) only after the above ships. No third-party rules in this program.

### 7.7 Strategies (P3)

The prototype's Library, Studio and Planner are kept as three tabs.

**Library.** The 25 templates in 6 categories are versioned as `airk.core/<id>@1.0.0`, with honest Runnable or Preview badges (§6.2 #7). Browsing never replaces Studio work; loading asks first when Studio has edits or a run.

**Studio.**
- **Builder.** An ordered step DAG from a **closed operator catalog**:
  - `airk.*`: the 16 tools, excluding `airk_help`, which is not evidence
  - `host.gitDiff`, `host.hashSavedFiles`, `host.readSavedText`
  - `core.dedupe`, `core.sort`, `core.clip`, `core.cite`, `core.renderPrompt`

  It has no shell, write, network or recursion operators. Unknown operators fail validation before any call.
- **Bindings:** only `${params.*}` and `${scope.*}`.
- **Default budgets:** 24 KB evidence, 60 calls, fan-out 24, 120 s, concurrency 4. Templates may set per-template budgets, shown before Run.
- **Run.** Pins the project and publication token (INV-7) and fails closed on drift.
  - Outcomes: `verified_snapshot`, or `partial` with reasons such as live hash mismatch, truncation or step error.
  - Records `airk.explorer.run/1`: service version, protocol, service instance, project, publication, template version, step list and evidence digest.
  - Adding, removing or reordering a step clears the run.
- **Prompt.** The fixed contract is Objective, Evidence (quoted as data), Task instructions, Deliverables and Completion gates.
  - **Profiles:** *Portable* (excerpts included; assumes no AIRK) or *MCP-aware* (asks the agent to re-check with AIRK only if it has the tools).
  - **Editing:** the prompt is editable. *Rebuild* asks before discarding edits.
  - **Token count:** labeled as an estimate.
- **Determinism.** The compiler sorts all records by a total key (path, line, producer, hash) before packing, so output does not depend on service result order (review C7). Golden-file tests cover this.

**Planner.** It combines Studio snapshots and manual tasks into one Markdown plan.
- **Plan file:**
  - YAML front matter: `schema: airk.explorer.plan/1`, workspace, project, publication, extension version, and each task's id, dependencies, evidence state and status
  - the body repeats preflight rules: read the whole plan, verify current files, treat snippets as candidates, stop on blocked dependencies or stale evidence
- **Task states:** Draft, Ready, Exported, Review, NeedsWork, Completed, Blocked and Stale.
  - Blocked is derived from incomplete predecessors and is never chosen by hand.
  - Completed requires confirmation.
  - Copying a prompt marks Exported, never Completed.
- **Save contract:**
  - messages: `airk.planner.openDraft` → untitled Markdown (`opened`, never `saved`); `airk.planner.saveInWorkspace` → host flow; `airk.planner.result`
  - the host flow resolves a real workspace folder, using a quick pick for multi-root
  - it opens `showSaveDialog` with the path hint, which serves as the consent gate (review C4)
  - it checks containment for the `file` and `vscode-remote` schemes and refuses other schemes
  - it guards against overwrites and dirty buffers, writes through `workspace.fs`, reads back, compares the hash and opens the editor
  - a late result for an older draft revision is ignored
- **Short handoff prompt.** Generated only after a confirmed save, for example: "Read `<path>` in this workspace. Do T1 only. Verify current files. Report changed files and test results, then ask before T2."
- **Completion loop.** The user pastes or points at an agent's report. The Checker (§7.5) validates its citations, then the user moves the task to Review, NeedsWork or Completed.
- **Decision D-14 (decided): default plan location.** The workspace setting `airk.explorer.planner.directory` defaults to `.airk/plans/`. That folder is excluded from native indexing (§4.3) and usually git-ignored, so plans don't pollute evidence or commits unless the team chooses otherwise. The save dialog always lets the user pick another location, such as `docs/airk-plans/` (the requirements' suggestion).
- **Optional P3+ export.** A template or plan can be exported as a Claude Code skill or slash command (`SKILL.md`) or a Codex prompt file. These are plain files the user saves, with no agent invocation.

### 7.8 Management (P0 Projects, P2 all tabs)

Management replaces the Service Console. Every action calls the **existing** control-plane operation through `ControlFacade`. That keeps:
- `IntentJournal` persistence and exact-key retry for unknown outcomes
- the `consoleAction`-style allowlist (actions are names plus project IDs, never paths or RPC bodies)
- native modal confirmations for destructive work

| Tab | Content | Existing operations |
|---|---|---|
| **Projects** | A project table: collection, root, state, watch, paused, publication, files, semantic. Row selection; register a workspace; inactive projects. | `console` / `inactive_console` pages, `workers`, `registerWorkspace`, `select` |
| **Activity** | Current job per project; phase, counters, sampled wait reason and its age; queued jobs; recent transitions. **No ETA**, only real advancement ages. | Console page `progress`/`pending_job`, `events`, `retryJob`, `cancelJob` |
| **Repair** | The current Diagnose & Repair. Operation chooser (auto, verify, recover, reconcile, vectors, rebuild) for one project, the selected projects or all. A paged plan preview that must be read in full before *Approve exact preview*. Pause, resume and cancel plan; pending intents. | `plan_create`, `approvePlan` (with `allowFallback`), `pausePlan`, `resumePlan`, `cancelPlan`, `retryIntent`, `forgetIntent`, `forceIntent` |
| **Indexing** | Index now, inspect, pause/resume, automatic or manual watching, detail level, vectors on/off, vector selection rules, bundled or installed model, extraction policy, scan limits. | Jobs `index` and `inspect`; `pause`; `configure_watch`; `configure_project`: `detail`, `vectors`, `vector_selection`, `install_model`, `extraction_policy`, `scan_limits`, `reset_scan_limits`; job `model_setup` |
| **Storage** | Storage folder (`airk.storage.path`), per-project index location, quota policy, roots (add or remove), unregister, derived-state cleanup with exact preview, legacy 1.3 cleanup guidance. | `chooseStorage`; `configure_project`: `quota_policy`, `add_root`, `remove_root`, `unregister`, `prepare_derived_cleanup`, `delete_derived` |
| **Service** | Health and instance; lifecycle (start, stop, restart); runtime upgrade, rollback and payload repair; resources preset; embedding helper (start, stop, force); legacy startup review; external-manager mode notice. | Console simple actions: `start`, `stop`, `restart`, `upgradeRuntime`, `rollbackRuntime`, `repairRuntime`, `resumeActivation`, `resources`, `startEmbedding`, `stopEmbedding`, `forceEmbedding`, `reviewLegacyStartup`, `recoverLifecycle`, `abandonLifecycle` |
| **Agents** | Two connection types (§8.5); per-client setup and diagnostics; VS Code/Copilot MCP controls; CLI install or repair; MCP response mode; registration enabled. | `setupAgent`, `agentDiagnostics`, `verifyMcp` (`probeMcp`), `manageVscodeMcp`, `installCli`, `airk.mcp.*` settings |
| **Usage** | The existing Usage & Savings report (7/30 days), recording mode, feedback, import, clear and calibrate, always labeled as an estimate and not billing. | `usagePanel.ts`; `configure_project`: `usage`, `clear_usage`, `usage_feedback`, `import_usage`, `calibrate` |
| **Logs** | Service logs, Output channel, diagnostic preview with path redaction and copy. | `logs`, `previewReport`, `copyReport` |

Presentation rules (prototype acceptance checklist plus the console's honesty copy):
- Every cached value shows "cached · observed <age>".
- Unknown stays "Not observed", never zero.
- Current failures are kept separate from historical ones.
- "Ready" is never shown as integrity. Verify is a separate, cancellable job.

**Decisions D-11 and D-12 (decided): retiring the Service Console and legacy modules.**
- At P2, once Management passes the parity checklist (Appendix B), hide the `airk.service` view and point every `consoleAction()` command at an Explorer route. For example, `airk.extractionPolicy` → `#manage/indexing?focus=extraction`.
- In P5, remove the console view and its rendering. The controller logic stays, used headlessly by the Explorer's control facade.
- Also in P5, after moving the `McpVerification` type, delete the legacy modules that are no longer reachable from activation:
  - `sidebar.ts`, `dashboardView.ts`, `diagnosticsView.ts`, `diagnostics.ts`, `agentDiagnostics.ts`, `collectionTree.ts`
  - `scheduler.ts`, `history.ts`, `coverage.ts`, `retention.ts`, `indexProgress.ts`
  - `extractionPolicy.ts`, `ingestionLimits.ts`, `vectorModel.ts`

  Their tests go too, once a reachability check confirms each one.

### 7.9 Help (P1)

**Decision D-07 (decided): how help is split.**

| Topic family | Owner | Served by |
|---|---|---|
| Engine, CLI, MCP, indexing, jobs, freshness, models, storage, troubleshooting (35 topics) | Native `airk_help`, version-matched to the runtime | Explorer Help calls `airk_help` through the evidence session. Agents call it on AIRK Retrieval. |
| Explorer UI: screens, routes, Strategies, Planner, Inbox, Management task flows (`explorer.*`, `strategies.*`, `management.*`) | Extension help catalog `src/explorerHelp/*.md`, version-matched to the VSIX | Explorer Help renders it. Agents call `airk_explorer_help_*` on Explorer Assistance. |

- Explorer topics carry `related_airk_help` topic IDs, so an agent can follow up on the other server. This is a cross-reference, not a proxy, which preserves INV-4.
- The catalog is validated at build time and embedded into both bundles, so there is no separate JSON asset. Validation checks:
  - every topic's route exists in the route manifest
  - each topic has an `applicability` value from the feature manifest
  - no topic duplicates an engine topic
- Topic files reference routes in front matter, not as Markdown links, because the repository link check resolves every relative Markdown link.
- This changes requirements §4.2. They place *all* help in the TypeScript catalog, written when `airk_help` did not exist. Duplicating 35 engine topics in TypeScript would drift from the runtime.

---

## 8. Explorer Assistance MCP and Suggestions Inbox (the requirements' core)

This section adopts requirements §3–§10. The changes are listed in §8.7.

### 8.1 Two servers

| | AIRK Retrieval (existing) | Explorer Assistance (new) |
|---|---|---|
| Implementation | Native Rust, launched through `airk runtime launch … mcp` | TypeScript/Node, bundled in the VSIX |
| Tools | 16 read-only `airk_*` | `airk_explorer_*` (§8.2) |
| Scope | One collection fixed at launch | One trusted window's Explorer, bound by session |
| Reaches | Indexed evidence | Bundled help, and drafts delivered to the host's Inbox |
| Never | Changes the index, runs commands | Calls Rust, reads source, writes files, runs commands or settings |

### 8.2 Tool catalog and phasing

| Tool | Purpose | Phase |
|---|---|---|
| `airk_explorer_help_search(query, area?, limit?, max_bytes?)` | Deterministic lexical ranking over Explorer topics; bounded. | **P2a** |
| `airk_explorer_help_get(topic_id, detail?)` | Steps, route, prerequisites, warnings, `applicability`, `related_airk_help`, `catalog_version`, `topic_revision`. | **P2a** |
| `airk_explorer_ui_status()` | Whether the UI is attached; schema versions; available routes with availability; opaque binding fingerprint; no paths. | **P2a** |
| `airk_explorer_suggest_search(draft)` | Search draft → Inbox → Search form. | P4 |
| `airk_explorer_suggest_strategy(draft)` | Template, parameters and allowlisted steps → Studio. | P4 |
| `airk_explorer_suggest_plan(draft)` | Tasks, dependencies and a path hint → Planner. No `save` field (review C8). | P4 |
| `airk_explorer_proposal_status(proposal_id)` | `pending_review`, `applied`, `rejected`, `deferred`, `expired` or `conflicted`; no body. | P4 |
| `airk_explorer_suggest_management(intent)` | Known guidance intents only: open a route, preview safe settings. Never executes. | P5 |
| `airk_explorer_suggest_review(draft)` | Review scope → Review. | P5 |
| `airk_explorer_suggest_report`, `airk_explorer_open_view` | Report draft; opt-in focus navigation. | Deferred |

Help tools ship in **P2a**, before the draft tools, because they need no Inbox and give immediate value. An agent can answer "how do I…" correctly as soon as the Explorer exists.

**Help as MCP resources and prompts.** Expose help topics as MCP **resources** (`airk-explorer-help://<topic_id>`) and the Runnable templates as MCP **prompts**, in addition to the tools. Tools remain the portable baseline (requirements H-07; the review's recommendation).

### 8.3 Contracts

Adopted as written from requirements §5.1 and §9:
- Shared draft envelope: `schema_version`, `idempotency_key`, `title`, `expected_target_revision?`, unknown fields rejected.
- Receipt: `proposal_id`, `status`, `destination`, `workspace_binding`, `applied`, `executed:false`, `expires_at`, `warnings`.
- State machine: `RECEIVED → VALIDATED → PENDING_REVIEW → APPLIED | REJECTED | DEFERRED | EXPIRED | CONFLICTED`.
- Limits: 16 KiB per draft, 256 KiB total, 50 pending, 24 h TTL, 30 requests/min.

There is **one error registry**, `contracts/errors.ts`, which closes the review's C5. Each code is tagged with the plane that emits it:
- **Assistance:** `UI_NOT_ATTACHED`, `HOST_UNAVAILABLE`, `WORKSPACE_UNTRUSTED`, `AMBIGUOUS_TARGET`, `STALE_BINDING`, `SCHEMA_UNSUPPORTED`, `INVALID_SCHEMA`, `CAPABILITY_DISABLED`, `EVIDENCE_MISMATCH`, `PROPOSAL_CONFLICT`, `PROPOSAL_EXPIRED`, `RATE_LIMITED`, `POLICY_DENIED`, `REJECTED_BY_USER`
- **Host/service:** `SERVICE_UNAVAILABLE`, `SERVICE_INCOMPATIBLE`, `EMBEDDING_UNAVAILABLE`, plus the native `ServiceError` codes, passed through unchanged

### 8.4 Inbox UX

Requirements UX-01 to UX-07, with additions from the review:
- **Placement:** a header badge plus a sidebar count.
- **Cards** show: the client label (self-reported, never treated as authenticated), received time, destination, status, a safety note, and evidence qualifiers.
- **Actions:** Review, Apply to draft (with a field diff and merge when the target has unsaved edits), Edit, Defer, Reject, and Discard all. Clicking the card body never approves it.
- **Flood control:** cap visible cards and collapse duplicates by idempotency key and by target, so a looping agent cannot bury real proposals.
- **Retention:** in memory by default. Opt-in session retention comes after a security review (requirements §9.4).

### 8.5 Launch, binding and IPC

Requirements §3.3–3.5 left these open; the review's C3 and its rendezvous and credential gaps are resolved here.

- **Provider for in-editor agents.** Add a second VS Code provider with ID `airk.explorerAssistance`, separate from `airk.collections`. It returns an `McpStdioServerDefinition` whose command is `process.execPath` with `ELECTRON_RUN_AS_NODE=1`, and whose argument is the bundled `out/explorerAssistance.js`.
- **Decision D-08 (decided): external agents use a native forwarder.** Claude Code, Codex and Gemini CLI launch their own configured stdio command. Requiring Node on PATH could force users to install Node, so the agent entry uses the existing stable AIRK launcher instead:
  - **Agent entry.** Key `airk_explorer`, which can never collide with an `airk-<collection>` retrieval key. Command `binding.launcher`. Args `runtime launch --install-dir <dir> -- explorer-assistance --descriptor <globalStorage>/explorer-assistance/launch-1.json`.
  - **Forwarding.** The launcher already forwards subcommands to the selected runtime `airk`. A new hidden runtime subcommand, `airk explorer-assistance`, reads the descriptor. The descriptor names VS Code's runtime executable (`process.execPath`, run with `ELECTRON_RUN_AS_NODE=1`, or the remote server's `node`) and the bundled `out/explorerAssistance.js` with its SHA-256.
  - **Validation.** The subcommand checks the descriptor's location, size and fields, the `iokaio.airk` package identity, the server hash and the runtime file.
  - **Hand-off.** It spawns the server with inherited stdio and working directory. Unix uses `exec`; Windows waits on the child. `NODE_OPTIONS` and `NODE_PATH` are removed.
  - **What it never does.** It never opens project stores, contacts the service or calls retrieval, so no MCP bytes pass through Rust code. Requirements AT-26 is read as "no retrieval or tool invocation by Rust". The forwarder process exists only to start Node.
  - **Descriptor freshness.** The extension rewrites the descriptor atomically on every trusted activation, so extension or VS Code updates take effect the next time VS Code runs. In remote windows the server's `node` path changes on each VS Code update, so a stale descriptor fails with an explicit "open VS Code once" error.
  - **Platform note.** On Windows an external Assistance session keeps the launcher and runtime executables open, as retrieval sessions already do. A runtime upgrade therefore waits for those sessions to close.
- **Rendezvous.** Each trusted window's host opens a per-window endpoint: a Windows named pipe with a user-only ACL, or a Unix socket in a `0700` directory under `globalStorageUri`. It writes a descriptor file next to it containing the endpoint, window session ID, canonical workspace roots, binding fingerprint, expiry and a rotating credential, with user-only permissions.
  - *As implemented (P2a):* Node cannot set a named pipe's ACL, so on Windows the pipe has a random name and the credential in the user-only descriptor authenticates callers; a wrong credential gets no reply. Unix sockets live in a `0700` directory under the temporary directory, because socket paths under `globalStorageUri` can exceed the platform length limit. The descriptor stays under `globalStorageUri/explorer-assistance/windows/`. Credentials rotate every 10 minutes and descriptors expire after 30.
- **Connecting.** The sidecar reads descriptors at connect time and chooses the one whose roots contain the agent's working directory. If several match it returns `AMBIGUOUS_TARGET`. Credentials never appear in agent configuration, process arguments or the environment.
- **Rotation.** Window reload or close deletes the descriptor and rotates credentials, so stale drafts fail with `STALE_BINDING`.
- **Help without a UI.** Help tools work without any attached window. Draft tools return `UI_NOT_ATTACHED`.
- **Untrusted workspaces.** No descriptor is written, so only static help is served.
- **Agent setup** offers two separately labeled entries: *AIRK Retrieval (evidence)* and *AIRK Explorer Assistance (help and UI drafts)*. Each is verified independently. `agentConfig.ts` and `agentMigration.ts` are extended rather than replaced.

### 8.6 Developer workflow this enables

This is how developers actually use AIRK with Claude Code and similar agents.
1. In the terminal or the Claude Code panel, the developer asks: "Find where session renewal fails and set up a plan."
2. The agent uses **AIRK Retrieval** (`airk_search`, `airk_code_facts`…) for evidence. It then calls `airk_explorer_suggest_strategy` or `airk_explorer_suggest_plan` with that evidence and publication token.
3. The Explorer Inbox shows the cards. The developer applies the plan draft, adjusts the dependencies, and **Saves As** to `.airk/plans/session-renewal.md`. The file opens in the editor.
4. The developer copies the short handoff prompt into the same or another agent session. The agent reads the plan file directly, so AIRK MCP is optional for the executing agent.
5. When the agent reports back, the developer pastes the report into Evidence › Checker or Planner › Completion, which validates its citations. The developer marks the task.

At no point does the Explorer run an agent, and the agent never presses a management button.

### 8.7 Changes to the requirements document

| Requirement | Change | Reason |
|---|---|---|
| Baseline "1.3.1, 15 tools"; AT-23/AT-26 "exactly 15" | 16 tools, including `airk_help`. | Code (§4.2). |
| §4.2 all help in the TypeScript catalog, H-05 offline | Explorer topics in TypeScript; engine topics remain in `airk_help` and are cross-referenced (§7.9). | Avoid two drifting engine catalogs. |
| §11 map: `dashboardView.ts`, `sidebar.ts`, `diagnosticsView.ts` | Target `serviceExtension.ts`, `serviceConsole*.ts`, `serviceClient.ts` and the new flat `src/explorer*.ts` modules (§5.5). | Those files are legacy in 1.4 (§2). |
| §13 phases P0–P3 | Re-sequenced to follow the UI destinations they target: help in P2a, drafts in P4 (§14). | Drafts need real destinations. |
| SC-20 "queue tasks" | Planner tasks with dependency gates; no queue, no dispatch. | Scope (§3.2). |
| §3.3 launch details | Concrete provider, stable launcher and rendezvous (§8.5). | Open in the source. |

---

## 9. Evidence and state model

### 9.1 Evidence classes

From the Codex plan, as kept in the grok record:

| Class | Meaning | Typical source |
|---|---|---|
| E1 indexed text | A textual hit or occurrence. Not a binding and not coverage. | `airk_search` lexical, `airk_references` |
| E2 syntax fact | A declaration, signature or written call/import the indexer saw. | `airk_symbols`, `airk_code_facts`, `airk_imports`, `airk_outline` |
| E3 rule observation | A deterministic finding from a built-in rule. | Review › Quality (P6) |
| E4 provider-asserted | An external producer's claim (SCIP import, VS Code language provider), not promoted to resolved. | `airk_code_facts` SCIP producer; optional VS Code providers (P6) |

Vector and hybrid hits are labeled **similarity** (E1 with a `similarity` qualifier), never relevance or understanding.

### 9.2 Shared state vocabulary

These words are used identically across screens, help and Assistance responses:

| Group | Words |
|---|---|
| Connection | `ready`, `degraded`, `disconnected`, `incompatible` |
| Index | `not_registered`, `not_indexed`, `indexing`, `available`, `unavailable` |
| Freshness | `fresh`, `pending changes`, `stale`, `unknown` |
| Semantic | `pending`, `partial`, `ready`, `failed`, `disabled` |
| Results | `found`, `not found`, `not indexed`, `truncated`, `ambiguous`, `candidate`, `verified` (live hash matches), `changed since index` |
| Planner and Studio | `verified_snapshot`, `partial`, `Exported ≠ Completed` |

Retrieval UI is enabled only when the connection is `ready` or `degraded` and `index_state` is `available` with a nonempty publication token. This follows the consumer rule in `docs/mcp.md`.

### 9.3 Counts and pagination

| Tools | Pagination in the UI |
|---|---|
| `list_files`, `outline`, `symbols`, `imports`, `document_facts`, `code_facts`, `changes` | "Load more" with the returned cursor. The cursor is discarded when the publication changes. |
| `search`, `references`, `relationships` | Raise the limit or narrow the query, with the hint "Refine: no cursor for this tool". |

When the response does not state a total, the UI never shows one.

---

## 10. Security

These rules adopt requirements §10 plus the review's additions.

**Webview**
- Strict CSP with a nonce. `enableCommandUris:false` on the Explorer panel. No remote resources. `localResourceRoots` limited to the bundle.
- All evidence is rendered as text. Markdown previews in Planner use a named sanitizer: no raw HTML, no `javascript:` or `command:` links, no image loading. Exported reports never contain `command:` links.
- Webview → host messages are an allowlist of `{v:1,type,requestId,...}` with typed fields. The host never accepts a path, command ID, RPC body or tool name from the webview. Paths come from host-side pickers, or are resolved from IDs the host issued (citation IDs, project IDs).

**Trust and destructive actions**
- An untrusted workspace allows no service connection, evidence session, Assistance descriptor or file writes, matching existing `trusted()` checks.
- Destructive operations keep their native preview → modal confirm → exact-preview approval flow. The Explorer adds no shortcut.

**Prompt injection and leakage**
- Repository text, filenames, agent reports and drafts are untrusted data. They are never interpreted as directives. Studio quotes evidence inside fenced, length-prefixed blocks.
- Every copy and export action warns before sharing excerpts that may contain secrets. Studio additionally flags any cited path that matches secret-like patterns (`.env*`, `*.pem`, `*.key`, `.ssh/`). Check the native ingest exclusions for these patterns before relying on them being unindexed.

**Supply chain and telemetry**
- Pin the webview and Assistance dependencies. Add an SBOM line for new runtime dependencies to the extension's notices.
- The Explorer adds **no telemetry**. Local usage remains AIRK's existing ledger (§5.3 D-05).

---

## 11. Settings and commands

### 11.1 New settings

All new settings are machine-scoped unless marked as a workspace setting.

| Key | Default | Purpose |
|---|---|---|
| `airk.explorer.openOnStartup` | `false` | Open the Explorer panel after activation when a project is registered. |
| `airk.explorer.search.defaultLimit` | `20` | Search form default (1–100). |
| `airk.explorer.evidence.budgetBytes` | `24000` | Studio default evidence budget. |
| `airk.explorer.planner.directory` | `.airk/plans` | Save dialog default (D-14). This is a **workspace** setting. |
| `airk.explorer.assistance.enabled` | `true` | Register the Explorer Assistance provider and write IPC descriptors. |
| `airk.explorer.suggestions.ttlHours` | `24` | Inbox TTL (1–168). |
| `airk.explorer.feature.<route>` | per manifest | Feature flags for routes still `prototype_only`. A flag moves to `implemented` only when the phase gate passes. |

UI strings go through `vscode.l10n` from P0. English only at launch.

### 11.2 Commands

- **New:** `airk.explorer.open`, `airk.explorer.search`, `airk.explorer.openRoute` (internal, typed route argument), `airk.explorer.showSuggestions`.
- **Command titles** are always qualified as "AIRK: Open Explorer", to avoid confusion with VS Code's built-in Explorer (review).
- **Existing command IDs are kept.** After P2 they deep-link to Management tabs instead of the console (Appendix B).

---

## 12. Performance targets

| Path | Target | Notes |
|---|---|---|
| Activation overhead | ≤20 ms | Explorer code loads lazily. |
| Panel first paint (cached state) | ≤300 ms | Render the cached console snapshot first, then refresh. |
| Interactive search round trip, excluding engine time | ≤50 ms host overhead | Engine latency is shown separately, never hidden. |
| Evidence session start | Shown as progress | Cold start includes the service and worker. Show "connecting", never block the UI. |
| Inbox admission (Assistance → host) | p50 ≤150 ms for drafts ≤16 KiB | Requirements AT-22. |

These are targets, measured locally and recorded honestly. They are not release claims until qualified.

---

## 13. Testing and qualification

These rules fit the lean-CI requirements in [CONTRIBUTING.md](../../CONTRIBUTING.md) and [docs/ci-and-builds.md](../ci-and-builds.md).

**Routine `node --test` (fast, no service)**
- contract validators
- route and feature manifest
- strategy validator (cycles, unknown operators, bindings)
- compiler golden files with shuffled input order
- Planner Markdown golden files and the state machine
- citation parser
- Inbox admission: idempotency, conflict, TTL, flood
- error registry
- path containment
- webview message allowlist
- fuzz and property tests for validators

**Existing service fixture lane** (`nativeServiceFixture.ts`, `serviceTestHost.ts`)
- the evidence session against the real launcher
- the tool-inventory snapshot (**16**, INV-2)
- publication pinning and the fail-closed drift check
- the Management control facade reusing existing intents

Extend the existing suites; add no new CI matrix.

**Assistance**
- the help catalog works offline with the Rust runtime stopped (AT-01, AT-27)
- zero Rust invocations from Assistance tools (AT-26), checked with an instrumented launcher
- two-server isolation (AT-28)
- descriptor and credential permissions on Windows named pipes and Unix sockets (AT-20), locally on the developer's platform; other platforms are recorded as unqualified until run

**Manual and recorded**
- accessibility: keyboard-only, screen reader labels, focus ring, reduced motion (AT-19)
- remote windows (SSH, WSL, dev container) for binding (AT-21)
- installed-VSIX smoke, using the existing `installedEditor.ts` pattern

**Traceability.** Each requirement ID (G-, H-, UX-, AT-) maps to a test or to a recorded manual check in a table under `packages/vscode/src/test/explorer/TRACE.md`.

**Fixture repository.** Add one small committed fixture workspace (code, tests, docs, a PDF/DOCX, a drifted file) with a generator for the stale and interrupted states. No real customer source.

---

## 14. Delivery phases

Each phase is a coherent set of PRs. It ends when its exit gate passes locally, with relevant checks recorded per `CONTRIBUTING.md`. Remind the user about the patch/build version bump before each feature PR, per `CLAUDE.md`.

| Phase | Scope | Exit gate |
|---|---|---|
| **P0 Walking skeleton** | <ul><li>`explorerContracts`/`explorerRoutes`/`explorerFeatures` contracts and the controller</li><li>esbuild webview shell with rail, status pill and command palette</li><li>evidence session (`explorerEvidence.ts`)</li><li>Overview</li><li>Search end to end with Open source and live hash check</li><li>Management › Projects (read-only table and Index now)</li><li>sidebar status card and quick search</li></ul> | Search against a real registered project returns pinned, cited results and opens the exact line. The 16-tool snapshot passes. No regression in existing extension tests. |
| **P1 Retrieval workbench** | Code (Symbols, References, Outline), Documents › Facts, Evidence (Inspector, Checker, Changes), Help (`airk_help` plus Explorer catalog). | Every tool except `airk_help` is reachable from the UI with honest pagination and states. Help routes validate at build time. |
| **P2 Management convergence** | All nine Management tabs at parity with the Service Console (Appendix B checklist). Commands redirected. Service Console hidden. **P2a:** Explorer Assistance server with `help_search`, `help_get` and `ui_status`, the provider, stable launcher, descriptors and two-connection agent setup. | Parity checklist complete. AT-01, AT-02, AT-04, AT-23–AT-29 pass or are recorded. |
| **P3 Strategies and Planner** | Library (3 Runnable, 22 Preview), Studio (Builder, Prompt, import/export of recipes), Planner (front matter, Save As contract, handoff prompt, completion loop), Search Recipes. | Golden files are stable. Planner save, cancel, overwrite and dirty cases pass (AT-09, AT-10). Exported is never Completed. |
| **P4 Suggestions** | `suggest_search`, `suggest_strategy`, `suggest_plan`, `proposal_status`; Inbox with diff and merge; flood control. | AT-05–AT-08, AT-11–AT-18, AT-22, AT-24, AT-25 pass. The agent workflow in §8.6 is demonstrated with at least Claude Code and Copilot. |
| **P5 Review and management assistance** | Review › Changes, Impact, Tests; `suggest_management` and `suggest_review`; remove the legacy console and legacy modules. | No agent path can reach Verify, Retry, Rebuild or Remove (AT-12). Review output is labeled as candidates. |
| **P6 Later, each needing separate approval** | Code › Map, Documents › Drift, Review › Quality (built-in rules, fingerprints, SARIF export), Reports, optional VS Code-provider E4 evidence, Problems panel integration. | Decided individually. |
| **Out of this program** | Task dispatch adapters, `airk-explorer` CLI, third-party packs API, team/remote service binding, new native tools. | Requires a new design. |

**Sequencing rationale.**
- Search and Management come first because they are the user's stated pain (the crowded, low-value panel) and they prove the two channels.
- Assistance help ships with Management because it is independent of the Inbox.
- Drafts follow the destinations they target.
- Review and rules come last because they are the largest new logic with the most risk of overclaiming.

---

## 15. Decision register

The maintainer decided every item on 26 October 2026. Implementation is phases P0–P5, delivered as one PR with the version bumped to **1.5.0**.

| ID | Decision | Outcome | Phase |
|---|---|---|---|
| D-01 | Host surfaces | Editor panel workbench, plus sidebar status and quick search, plus the status bar pill (§5.1). | P0 |
| D-02 | Information architecture | 8 workspaces with tabs; prototype roles become Overview lenses (§6.1). | P0 |
| D-03 | Webview stack | esbuild + Preact, VS Code theme tokens; codicons deferred (§5.4). | P0 |
| D-04 | Retrieval transport | Persistent MCP stdio session through the existing launcher, at most 3 calls in flight (§5.3). | P0 |
| D-05 | Explorer usage attribution | New `explorer` client family via `clientInfo.name: "airk-explorer"`. Environment opt-outs do not reach the worker (§5.3). | P0 |
| D-06 | Search default mode | Interactive search uses the collection policy, shown explicitly; Strategies pin lexical (§6.2 #8). | P0 |
| D-07 | Help ownership | Explorer topics in TypeScript, embedded in the bundles; engine topics in `airk_help`, cross-referenced (§7.9). | P1 |
| D-08 | Assistance runtime for external agents | Native `airk explorer-assistance` forwarder starts the bundled server with VS Code's runtime; no system Node (§8.5). | P2a |
| D-09 | Queue and dispatch | Merged into Planner; no dispatch or CLI (§6.2 #5–6). | P3 |
| D-10 | Review rules and packs | Built-in rules only, in P6; no third-party packs (§7.6, §6.2 #12). | P5 |
| D-11 | Legacy modules | Unreachable legacy modules are deleted after the type move and a reachability check (§7.8). | P5 |
| D-12 | Service Console retirement | Hidden at P2 parity, removed in P5 (§7.8). | P2, P5 |
| D-13 | Visual system | Prototype layout with VS Code theme tokens (§5.4). | P0 |
| D-14 | Default plan directory | `.airk/plans` (not indexed, typically ignored); workspace setting (§7.7). | P3 |
| D-15 | Version | 1.5.0. The native `service-1.4` namespace and state-layout names are unchanged, so there is no reindex. Automatic upgrade accepts 1.4.x → 1.5.x. Runtime rollback across major.minor stays refused. | P5 |

**Answered questions** (checked in code, 26 October 2026):
1. **Usage opt-out.** `AIRK_USAGE_MODE` and `AIRK_CLIENT` on the stdio frontend have no effect in service mode; the worker records usage using the `clientInfo` family. Hence D-05.
2. **Live hash.** `content_hash` is SHA-256 hex of the raw file bytes. Lines are 1-based and split on `\n`, and `line_basis` is `"source"` for UTF-8 text. The host can reproduce it (§7.5).
3. **Search order.** It is deterministic: lexical by `stable_id`, vector and hybrid by reciprocal-rank fusion with a `stable_id` tie-break. Lexical order is not relevance order (§7.2).
4. **Console page.** It carries publication, files, semantic, discovery, storage and watch. The inspection snapshot refreshes only after inspect or configure jobs, so Projects labels these values as cached. Overview uses `airk_status` for the bound project.
5. **VS Code API.** `@types/vscode` 1.140.0 provides `McpStdioServerDefinition` (with env, version and cwd), the provider `resolveMcpServerDefinition` callback, `enableCommandUris`, `vscode.l10n` and `registerWebviewPanelSerializer`.

---

## Appendix A. What was adopted from each source

| Source | Adopted | Modified | Not adopted |
|---|---|---|---|
| UI prototype (controlling) | Screen set; Strategy Library, Studio and Planner; the Planner save contract; Index Management acceptance checklist; honesty copy; evidence ledger; prompt contract; recipe validator rules | Navigation grouping; scheduling modes; service tab added; theme; Runnable vs Preview labels (§6.2) | Forced desktop; Task queue and dispatch; companion CLI; third-party packs (deferred) |
| Requirements (core) | Two-server architecture; routing invariant; Inbox; tool names; envelopes; receipts; state machine; error registry; limits; security table; scenarios; acceptance tests | Help split with `airk_help`; 16 tools; module map; phasing; launch details (§8.7) | Queue dispatch scenario (SC-20 reduced to Planner gates) |
| Codex plan | Evidence classes; truthful counts; run manifest; DSL operators; budgets; schema IDs; 25 template IDs | Plan path default; Queue removed; 1.3.1 residue removed | R6 adapters and CLI; persona presets as separate presets (now lenses); remote binding |
| Plan review | INV list; C1–C8 fixes; settings and feature flags; status pill; native surfaces; walking skeleton; front matter; completion loop; sanitizer; `command:` rule; MCP resources and prompts | Real-service CI → existing fixture lane only | Nightly hosted matrices |
| Claude spec | Module boundaries; message envelope; settings list shape; per-PR increment discipline | Paths fitted to the actual `src/` layout; 15 → 16 tools | Self-declared precedence |
| grok design record | 8-workspace IA; Overview journeys; Planner task states; citation syntax; fail-closed connection states | Tabs per workspace adjusted to code (§6.1) | — |
| Competitive research | Service-health UX; honest-incompleteness vocabulary; Planner states; narrow Assistance scope; "don't grow native tools" | — | Graph/agent rebranding; orchestration patterns |

## Appendix B. Service Console → Management parity checklist

**Status (1.5.0):** implemented. `packages/vscode/src/explorerCommands.ts` maps each former console command to its tab, `MANAGE_ACTIONS` equals the console's own action allowlist, and `explorerParity.test.ts` checks both. The legacy view is hidden behind `airk.explorer.showServiceConsole`.

Management is at parity when every row below works from its tab, with the same confirmations and receipts as today. Only then can `airk.service` be hidden and the commands redirected.

| Current console action or command | Management route |
|---|---|
| `refresh`, `connect` | Global status pill and Refresh |
| `registerWorkspace`, `airk.index` (unregistered) | `#manage/projects` |
| `registeredProjects`, `inactiveProjects`, `firstPage`, `nextPage`, `select` | `#manage/projects` |
| `inspect`, `index`, `pause`/`resume`, `watch`, `restartProject`, `modelSetup`, `configureProject`, `copyConfiguration` | `#manage/indexing` (per project) |
| `retryJob`, `cancelJob`, events | `#manage/activity` |
| `repair` (one, selected, all; 6 operations), `morePlan`, `refreshPlan`, `approvePlan`, `pausePlan`, `resumePlan`, `cancelPlan`, `retryIntent`, `forgetIntent`, `forceIntent` | `#manage/repair` |
| `previewCleanup`, `deleteDerived`, `airk.chooseStorage`, `airk.storageLimits`, `airk.removeCollection`, `airk.treeRemove` | `#manage/storage` |
| `start`, `stop`, `restart`, `resources`, `repairRuntime`, `upgradeRuntime`, `rollbackRuntime`, `resumeActivation`, `recoverLifecycle`, `abandonLifecycle`, `startEmbedding`, `stopEmbedding`, `forceEmbedding`, legacy startup actions | `#manage/service` |
| `responseMode`, `airk.setupAgent`, `airk.manageMcp`, `airk.verifyMcp`, `airk.manageVscodeMcp`, `airk.installCli`, `airk.openTools` | `#manage/agents` |
| `airk.usage` and its actions | `#manage/usage` |
| `logs`, `previewReport`, `copyReport`, `airk.logs`, `airk.serviceLogs`, `airk.serviceDiagnostics` | `#manage/logs` |
| `airk.extractionPolicy`, `airk.configureExtractionPolicy`, `airk.ingestionLimits`, `airk.configureIngestionLimits` | `#manage/indexing?focus=extraction` or `?focus=limits` |
| `airk.status` | `#overview` (plus Evidence › Inspector › full status) |
| `airk.reindex`, `airk.diagnose`, `airk.currentMaintenance` | `#manage/repair` with the operation preselected |
