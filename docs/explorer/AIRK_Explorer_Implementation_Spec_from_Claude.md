# AIRK Explorer — Implementation Spec for Claude Code

Oct 24, 2026 · @Tyler Jensen

Executable specification for building AIRK Explorer inside the existing `iokaio.airk` VS Code extension against the AIRK 1.4 local service, as revised by the plan review and as prototyped in `AIRK-Explorer-Prototype-v2.html`.

## 1. Scope, inputs and precedence

Build AIRK Explorer as new modules inside the existing `packages/vscode` extension of `github.com/iokaio/airk`, integrating with the AIRK 1.4 local service through one extension-owned client. The prototype `AIRK-Explorer-Prototype-v2.html` is the visual and interaction reference; this spec is the behavioral and contract authority.

**Governing inputs, in precedence order**

1. This spec, including the invariants below.
2. `AIRK_Explorer_Codex_Plan_v1_4.md` for anything this spec does not cover (evidence classes, rule grammar, run manifest, threat matrix).
3. The review document *AIRK Explorer Plan v1.4 — Review and Recommendations* for rationale.
4. The actual 1.4 checkout. When it contradicts items 1–3, **stop, write the discrepancy to `docs/explorer/discrepancies.md`, and do not implement the affected feature** until the owner decides (INV-10).

**Keywords.** MUST / MUST NOT / SHOULD / MAY follow RFC 2119. Requirements carry IDs `REQ-<AREA>-<nn>`; every REQ maps to at least one test ID in §12.

**Invariants (cite by ID; do not restate elsewhere)**

| ID | Invariant |
| --- | --- |
| INV-1 | Exactly one VS Code extension / one VSIX ships Explorer, Strategies, Planner, Management, and the Assistance MCP entrypoint. No companion extension. |
| INV-2 | The 15 `airk_*` Retrieval MCP tools keep their names, input and output semantics. A contract snapshot test fails CI on any drift. |
| INV-3 | The extension host is the sole owner of service discovery, connection, retrieval, management and file writes. The webview and the Assistance MCP never touch the service, the filesystem, or `executeCommand`. |
| INV-4 | The Assistance MCP makes zero calls to `airk`, `airk-embed` or any Retrieval adapter; it serves bundled help and admits typed drafts only. |
| INV-5 | Agent proposals are inert. Apply changes a draft; Run, Save, Index, Repair and Dispatch are separate human actions through native VS Code confirmations. |
| INV-6 | Evidence is qualified: occurrence ≠ resolved reference; syntax candidate ≠ runtime edge; ready index ≠ current file; Exported ≠ Completed. Unknown is a first-class result. |
| INV-7 | Every run pins one publication token plus service instance and collection fingerprint; drift marks the run stale and blocks dependents. |
| INV-8 | Counts are what the backend returned. No fabricated totals, no client-side filtering presented as collection-wide facets, no Load More without a backend cursor. |
| INV-9 | `airk` core service state and `airk-embed` state are reported independently; lexical workflows never depend on embedding. |
| INV-10 | Nothing is guessed about the 1.4 service: no hard-coded ports, binaries, paths or management methods. Mismatch is a build failure, not a fallback. |

**Out of scope for this spec:** K1–K6 backend enhancements, third-party procedural packs, automatic agent dispatch, a standalone CLI, and the forced-1440 desktop preview (standalone HTML only). Leave feature flags for them (§11) but implement nothing.

## 2. Work package 0 — repository discovery and discrepancy log

Do this before writing any Explorer code. Output is one PR containing `docs/explorer/discovery.md`, `docs/explorer/discrepancies.md`, and the contract fixtures in §4. Nothing in this WP changes runtime behavior.

**REQ-DISC-01** Run and record (commands are developer tooling, never exposed to agents):

```bash
git rev-parse HEAD && git describe --tags --always
rg -n '"version"' packages/vscode/package.json
rg -n 'engines|activationEvents|contributes|mcpServerDefinitionProviders' packages/vscode/package.json
rg -n 'airk-embed|airk_embed|ServiceClient|serviceUrl|socket|handshake|protocolVersion' packages crates docs -g '!node_modules'
rg -n 'airk_search|airk_read_file|airk_list_files|airk_status|airk_file_info|airk_outline|airk_symbols|airk_imports|airk_document_facts|airk_references|airk_context|airk_capabilities|airk_code_facts|airk_relationships|airk_changes' --type-add 'src:*.{rs,ts,md,json}' -t src
rg -n 'getServerDefinitions|joinScheduledIndex|showSaveDialog|diagnosticsView|agentSetup|sidebarAction' packages/vscode/src
rg -n 'Content-Security-Policy|acquireVsCodeApi|onDidReceiveMessage' packages/vscode/src
```

**REQ-DISC-02** `discovery.md` MUST answer every row; an unanswered row is written as `UNKNOWN` and blocks the dependent WP.

| Question | Why it matters | Blocks |
| --- | --- | --- |
| How is the 1.4 service discovered (descriptor file, socket path, stdio adapter, HTTP loopback)? | `AirkServiceClient.connect` | WP1+ |
| What authenticates a client (token, OS ACL, none)? | fail-closed policy | WP1+ |
| Who supervises the `airk` process (installer, launchd/systemd, extension)? | whether start/stop is permitted | WP9 |
| Is `airk-embed` a process, library or service, and how does it report readiness? | INV-9 status pill | WP1 |
| Does the service accept concurrent clients from multiple VS Code windows? | multi-window ownership | WP1 |
| Does the 15-tool listing come from the service or an adapter, and where is its schema source? | INV-2 snapshot | WP0 |
| Do `airk_search` / `airk_references` / `airk_relationships` return records in a stable order for identical inputs? | ST-09 determinism | WP7 |
| Which management operations exist (index now, retry, resume, rebuild, verify, storage, policy) and what is their protocol? | Management screens | WP9 |
| Is there an existing service manager in `extension.ts`/`native.ts` to reuse? | one owner (INV-3) | WP1 |
| Does `activate()` still export `getServerDefinitions()`? | public export compatibility | WP1 |
| Current `engines.vscode`, and does it include the MCP server-definition provider API? | Assistance registration path | WP10 |
| Existing test runner, lint and packaging commands | CI | WP0 |

**REQ-DISC-03** Generate `packages/vscode/src/explorer/contracts/fixtures/retrieval-tools.snapshot.json` from the live tool listing (names, input schema, output schema). Commit it. Add a test that regenerates and diffs (§12, `CT-01`).

**REQ-DISC-04** If the checkout is 1.3.x, write the migration gap to `discrepancies.md` and implement only WP0, WP2 (shell) and the offline parts of WP6/WP10 until a 1.4 target exists. Never mock a 1.3.1 Rust subprocess to satisfy a 1.4 gate (INV-10).

## 3. Architecture and module layout

Three runtime planes with one-way dependencies. The webview depends on `contracts/` only; the host depends on everything except `webview/`; the Assistance entrypoint depends on `contracts/` and `help/` only. Enforce with an ESLint `no-restricted-imports` rule per directory (REQ-ARCH-01).

```text
packages/vscode/src/
  extension.ts                     # existing; add lazy Explorer registration, keep exports
  explorer/
    contracts/                     # pure types + JSON Schemas; NO vscode import
      manifest.ts                  # WORKSPACES, ROUTES, FEATURES (generated from manifest.json)
      manifest.json                # single source for routes, help topics, feature flags
      runtime.ts                   # AirkRuntimeState, ConnectionState, EmbedState
      evidence.ts                  # EvidenceRef, EvidenceClass, ToolEnvelope
      messages.ts                  # HostToWebview / WebviewToHost discriminated unions, v:1
      errors.ts                    # ExplorerErrorCode registry (single)
      schemas/                     # strategy-1.json, run-1.json, plan-1.json, review-rule-1.json,
                                   # search-recipe-1.json, proposal-1.json (additionalProperties:false)
      fixtures/retrieval-tools.snapshot.json
    host/
      ExplorerController.ts        # route state, command router, view lifetimes
      AirkServiceClient.ts         # ONE owner: discover, connect, handshake, reconnect, status
      EvidenceClient.ts            # typed 15-tool facade over AirkServiceClient
      ServiceManagement.ts         # user-confirmed index/repair/storage adapter
      CollectionBinding.ts         # root, fingerprint, publication, trust, rebind
      FileNavigation.ts            # safe URI resolve + live hash + reveal
      GitProvider.ts               # read-only Git scope via vscode.git API
      IdeEvidenceProvider.ts       # E4 provider assertions (allowlisted)
      Lifecycle.ts                 # cancel/rebind/dispose
    review/  RuleEngine.ts  RuleCompiler.ts  FindingFingerprint.ts  BaselineStore.ts  ReviewReporter.ts
    strategies/  TemplateRegistry.ts  templates.json  StrategyValidator.ts  StrategyRunner.ts
                 EvidencePacker.ts  PromptCompiler.ts  StrategyStore.ts
    planner/  PlannerModel.ts  PlannerMarkdown.ts  PlannerController.ts  WorkspaceSavePolicy.ts
              CompletionReport.ts  CitationChecker.ts
    suggestions/  SuggestionStore.ts  SuggestionPolicy.ts  SuggestionDiff.ts
    management/  ManagementController.ts
    webview/                       # bundled separately (esbuild), CSP nonce, no node APIs
      index.ts  shell.ts  router.ts  bridge.ts  state.ts
      views/   search/ code/ review/ evidence/ strategies/ reports/ management/
      components/  StatusPill.ts  Inbox.ts  SourceDrawer.ts  Dialog.ts  Badge.ts  DataTable.ts
      styles/  tokens.css  shell.css  components.css
    help/
      catalog.build.ts             # docs/explorer-help/*.md → resources/explorer-help/catalog.json
  explorerBridge/                  # Assistance MCP entrypoint (Node; separate esbuild bundle)
    server.ts  toolRegistry.ts  validators.ts  ipcClient.ts  helpCatalog.ts  errors.ts
    launcher.ts                    # tiny shim copied to globalStorage on activation
  explorerIpc/
    ipcHost.ts                     # host-side socket/pipe server + credential rotation
docs/explorer-help/                # authored help topics (front matter + markdown)
test/explorer/                     # unit, contract, integration, e2e; fixtures/ repo
```

**Ownership rules**

- **REQ-ARCH-02** `AirkServiceClient` is instantiated once per extension host and injected; `EvidenceClient`, `ServiceManagement` and `StrategyRunner` receive it. If `discovery.md` names an existing service manager, wrap it rather than duplicate it (INV-3).
- **REQ-ARCH-03** `contracts/manifest.json` generates `manifest.ts`, the help catalog's route validation, the webview nav, and the Assistance `ui_status` feature list. A build step fails on an orphan route or help topic.
- **REQ-ARCH-04** All state visible to the webview is a serializable `ExplorerViewModel` produced by the host; the webview holds UI-only state (open drawer, focused tab, unsaved text).
- **REQ-ARCH-05** Activation budget: Explorer adds ≤ 20 ms to `activate()` (register commands and the view provider only); modules load on first route visit. Webview bundle ≤ 1.5 MB gzipped; Assistance bundle cold start ≤ 300 ms. Measure in CI (`PERF-01..03`).
- **REQ-ARCH-06** Route UI strings go through `vscode.l10n.t()` from the first commit.

## 4. Shared contracts

Freeze these in WP1 before any screen. Types below are normative for shape; field names from the 1.4 service are filled in from `discovery.md`.

### 4.1 Route manifest (`contracts/manifest.json`)

```json
{
  "workspaces": [
    {"id":"overview","label":"Overview","tabs":[]},
    {"id":"search","label":"Search","tabs":["query","recipes"]},
    {"id":"code","label":"Code","tabs":["symbols","references","architecture"]},
    {"id":"review","label":"Review","tabs":["findings","rules","trace"]},
    {"id":"evidence","label":"Evidence","tabs":["inspector","citation-check","watches"]},
    {"id":"strategies","label":"Strategies","tabs":["library","studio","planner"]},
    {"id":"reports","label":"Reports & Packs","tabs":["builder","packs"]},
    {"id":"management","label":"Management","pinned":true,
     "tabs":["collections","connections","scheduling","embedding","recovery","storage","agents","usage","logs"]}
  ],
  "routes": {
    "search/query": {"featureId":"search.query","availability":"implemented","requires":["retrieval"],"helpTopic":"explorer.search.query","flag":"airk.explorer.feature.search"}
  }
}
```

`availability` ∈ `implemented | prototype_only | unsupported | disabled`. **REQ-CT-01** A route with `prototype_only` renders its help text and a disabled state, never a live action.

### 4.2 Runtime state (`contracts/runtime.ts`)

```ts
export type ConnectionState = 'not_configured'|'connecting'|'ready'|'degraded'|'disconnected'|'incompatible'|'permission_denied';
export type EmbedState = 'ready'|'disabled'|'unavailable'|'unknown'|'error';
export interface AirkRuntimeState {
  connection: ConnectionState;
  serviceVersion?: string; protocolVersion?: string; serviceInstanceId?: string;
  endpointLabel?: string;              // redacted; never credentials
  capabilities: readonly string[];     // as advertised, e.g. 'lexical.search','syntax.calls','cursor.search'
  embed: { state: EmbedState; componentVersion?: string; modes: readonly ('vector'|'hybrid')[] };
  collection?: { id: string; fingerprint: string; publicationToken?: string; mode: 'lexical'|'lexical+vector'; files?: number; current?: number; stale?: number; missing?: number };
  observedAt: string;
}
export const canRetrieve = (s: AirkRuntimeState) => s.connection === 'ready' || s.connection === 'degraded';
export const canVector   = (s: AirkRuntimeState) => canRetrieve(s) && s.embed.state === 'ready' && !!s.collection?.mode.includes('vector');
```

**REQ-CT-02** `canRetrieve` gates every retrieval and management action; `canVector` gates only vector/hybrid modes (INV-9).

### 4.3 Service and evidence clients (`host/`)

```ts
export interface AirkServiceClient {
  readonly state: AirkRuntimeState;
  onDidChangeState: vscode.Event<AirkRuntimeState>;
  connect(binding: TrustedWorkspaceBinding): Promise<void>;   // handshake + version negotiation; fail closed
  bindCollection(id: string): Promise<void>;                   // cancels in-flight work, drops stale results
  readTool<T>(name: NativeAirkTool, args: Readonly<Record<string, unknown>>, o: { signal?: AbortSignal; timeoutMs: number }): Promise<ToolEnvelope<T>>;
  management(): ServiceManagement;                             // separate allowlisted, user-only API
  dispose(): void;
}
export interface ToolEnvelope<T> { data: T; publicationToken: string; serviceInstanceId?: string; truncated: boolean; cursor?: string; warnings: string[]; observedAt: string }
export interface EvidenceClient {
  readonly binding: CollectionBinding;
  call<T>(name: NativeAirkTool, args: Readonly<Record<string, unknown>>, o: { signal?: AbortSignal; timeoutMs: number }): Promise<ToolEnvelope<T>>;
  pin(): Promise<PinnedPublication>;                           // status + capabilities → {token, instanceId, fingerprint}
}
```

**REQ-CT-03** `readTool` validates `args` against the committed snapshot schema before sending and rejects unknown keys with `SCHEMA_UNSUPPORTED`. **REQ-CT-04** Concurrency cap 4, bounded backoff on reconnect (250 ms → 8 s, max 6), per-call timeout default 15 s. **REQ-CT-05** On `serviceInstanceId` or `publicationToken` change, emit `evidenceInvalidated` so runs, findings and watches mark stale (INV-7).

### 4.4 Evidence record (`contracts/evidence.ts`)

Use `EvidenceRef` from plan §4.4 unchanged, plus `liveState: 'current'|'changed'|'notfound'|'unchecked'`. Evidence classes E1–E4 and their labels are a single enum used by webview badges, reports and prompts.

### 4.5 Error registry (`contracts/errors.ts`) — the only list

| Code | Plane | Emitted when |
| --- | --- | --- |
| SERVICE\_UNAVAILABLE | host | no endpoint / connect failed |
| SERVICE\_INCOMPATIBLE | host | protocol or version rejected |
| PERMISSION\_DENIED | host | workspace untrusted, service auth refused |
| EMBEDDING\_UNAVAILABLE | host | vector/hybrid requested, embed not ready |
| CAPABILITY\_DISABLED | host, mcp | feature flag off or capability absent |
| SCHEMA\_UNSUPPORTED | host, mcp | unknown tool arg, unknown op, bad version |
| INVALID\_SCHEMA | mcp | proposal fails JSON Schema |
| STALE\_BINDING / AMBIGUOUS\_TARGET | host, mcp | window/collection mismatch |
| EVIDENCE\_MISMATCH | host | publication drift, hash mismatch |
| PROPOSAL\_CONFLICT / PROPOSAL\_EXPIRED | mcp | idempotency key reuse, TTL |
| RATE\_LIMITED | mcp | per-client limits |
| POLICY\_DENIED | host, mcp | save:true, absolute path, disallowed op |
| UI\_NOT\_ATTACHED / HOST\_UNAVAILABLE / WORKSPACE\_UNTRUSTED | mcp | IPC state |
| REJECTED\_BY\_USER | host | inbox reject, dialog cancel |

### 4.6 Webview message envelopes (`contracts/messages.ts`)

All messages are `{ v: 1, type, requestId }` discriminated unions; the host validates with the same schemas the webview uses. Host → webview: `state.update` (full `ExplorerViewModel`), `result` (correlated), `toast`. Webview → host: `nav.go`, `search.run`, `code.inspect`, `review.run`, `review.disposition`, `evidence.open`, `evidence.checkCitations`, `library.load`, `studio.update`, `studio.run`, `studio.rebuild`, `planner.update`, `planner.openDraft`, `planner.saveInWorkspace`, `planner.report`, `inbox.apply|defer|reject|discardAll`, `management.*` (one per confirmed action). **REQ-CT-06** Unknown `type` or `v` → `SCHEMA_UNSUPPORTED` and a visible error; never silent success.

## 5. Webview shell

One `WebviewViewProvider` in the activity bar (`airk.explorer`), plus a command `AIRK: Open Explorer` that reveals it. Command titles are always prefixed `AIRK:` to avoid collision with VS Code's Explorer (REQ-UI-01). Layout, spacing and component vocabulary follow `AIRK-Explorer-Prototype-v2.html`; copy its CSS tokens into `styles/tokens.css`, mapping them to `--vscode-*` variables with the prototype values as fallbacks.

**Shell elements**

| Element | Behavior | REQ |
| --- | --- | --- |
| Primary nav (224 px rail) | Seven workspaces; Management pinned in the rail footer; `aria-current="page"` on the active workspace; collapses to a drawer below 860 px with a `☰` button, `aria-expanded`, Escape to close, focus returned. | REQ-UI-02 |
| Header | breadcrumb, collection `<select>`, **status pill**, **Suggestions** button with pending count. Nothing else; results limits live on screens. | REQ-UI-03 |
| Status pill | Text `airk <conn> · embed <state> · <freshness>`; colour class plus text (never colour alone). Click opens a popover (role=dialog) with the full `AirkRuntimeState`, one-sentence guidance per state, and a link to `management/connections`. | REQ-UI-04 |
| Tabs | Generated from `manifest.json`; `role="tablist"`, `aria-selected`; the Planner tab shows the included-task count. | REQ-UI-05 |
| Route outlet | `<main tabindex=-1 aria-live=polite>`; focus moves here on navigation; scroll resets to top. Unsaved drafts (Studio prompt, Planner Markdown, Search query) persist in host `workspaceState` across route changes. | REQ-UI-06 |
| Suggestions inbox | Right drawer (role=dialog); cards with client label (self-reported), type, destination route, target draft revision, fields, intent, warnings; actions Review & apply / Defer / Reject; Discard all. | REQ-UI-07 |
| Source drawer | Opens on any file reference: indexed hash vs live hash, excerpt with the exact line highlighted **only when hashes match**, "Open in VS Code", "Index this file now…" (user-confirmed) when stale. | REQ-UI-08 |
| Empty / blocked states | Every screen renders a `state-empty` block with a bold title and a one-line next action for: no data, disconnected, incompatible, no collection, embedding unavailable (vector views only). | REQ-UI-09 |

**Rendering and security**

- **REQ-UI-10** CSP: `default-src 'none'; img-src ${cspSource} data:; style-src ${cspSource} 'nonce-…'; script-src 'nonce-…'; font-src ${cspSource}`. `enableCommandUris: false`. No CDN.
- **REQ-UI-11** Data from the host (paths, excerpts, proposal fields, Markdown) is inserted via `textContent` or a DOM builder; the only sanctioned HTML renderer is a Markdown renderer configured with `html: false`, links limited to `https:` and `file:`-free workspace-relative references handled by the host, images disabled.
- **REQ-UI-12** Badges use the state vocabulary from the prototype: `● Ready · current`, `◐ Indexed · live unverified`, `◔ Partial / truncated`, `△ Stale`, `∅ Unsupported`, `✕ Disconnected`, `⊘ Permission denied`, `… Loading`, plus `E1 indexed text`, `E2 syntax fact`, `E3 rule observation`, `E4 provider-asserted`.
- **REQ-UI-13** Breakpoints: ≥ 1024 full grid; 860–1024 two-column panels collapse to one; < 860 drawer nav, header wraps, no root horizontal scroll. Test at 320, 500, 700, 1024, 1440 px (`UI-01..06`).
- **REQ-UI-14** Dialogs: `role=dialog aria-modal`, focus trap, Escape closes, focus returns to the opener. Every destructive or write action uses a native VS Code dialog on the host side, not a webview-only confirm (INV-5).
- **REQ-UI-15** Theme: light, dark and high-contrast via `--vscode-*` tokens; `prefers-reduced-motion` disables drawer transitions.
- **REQ-UI-16** No forced-desktop mode in the VSIX. The standalone prototype keeps it; the webview does not implement `force-desktop` CSS.

## 6. Search, Code and Evidence workspaces

### 6.1 Search (`search/query`, `search/recipes`)

- **REQ-SRCH-01** Form fields map 1:1 to `airk_search` arguments: `query`, `mode` (default and always explicit `lexical`), `path_prefix`, `limit` (10/25/100), `max_bytes` (8192 default, 65536 max). Vector/hybrid options render disabled with the reason when `!canVector`.
- **REQ-SRCH-02** Results list each hit with `fileLink`, evidence class badge, live-state badge (from `FileNavigation.liveState`, computed lazily per visible row), excerpt, and actions *Inspect citation* and *Make Studio task*.
- **REQ-SRCH-03** Footer text is exactly `${n} returned · limit ${limit} · more may exist`; if `envelope.truncated`, append `· truncated: ${reason}`. Load More renders only when `envelope.cursor` is present **and** `capabilities` includes `cursor.search` (INV-8).
- **REQ-SRCH-04** Language/origin facets render disabled with `pending K1` until the capability exists; never filter the returned page client-side as if collection-wide.
- **REQ-SRCH-05** *Copy query JSON* emits only validated native args. *Import JSON* validates against the snapshot schema and creates a **draft** (does not run).
- **REQ-SRCH-06** Recipes are `airk.explorer.search-recipe/1` records in `workspaceState`; `boundCollection` is the host fingerprint, never user-supplied. A recipe may become a Watch (§6.3).

### 6.2 Code (`code/symbols`, `code/references`, `code/architecture`)

- **REQ-CODE-01** Symbol inspector composes `airk_symbols`, `airk_outline`, `airk_imports`, `airk_code_facts`, `airk_references` (bounded), optional `airk_relationships` (depth ≤ 2, ≤ 100 symbols, ≤ 256 files). Every record keeps `producer` and `evidenceClass`.
- **REQ-CODE-02** A written call candidate shows the exact written expression and containing path with badge `unresolved`; cross-language name coincidence renders `not linked — different language`. No target is inferred.
- **REQ-CODE-03** E4 provider assertions come from `vscode.executeDefinitionProvider` / `executeReferenceProvider` / `prepareCallHierarchy` only when the setting `airk.explorer.ideProviders.allowlist` contains the providing extension ID; label `E4 provider-asserted · live buffer vN`; shown beside E2, never merged.
- **REQ-CODE-04** Architecture map renders dashed edges for candidates, dotted for text-only, no solid edges; an accessible edge table (From, To, Type, Status) is always present and is the keyboard-navigable primary; the SVG has `role=img` and an `aria-label` summary. Reference-count footers follow REQ-SRCH-03.

### 6.3 Evidence (`evidence/inspector`, `evidence/citation-check`, `evidence/watches`)

- **REQ-EV-01** Inspector shows one citation record: collection fingerprint, full publication token (abbreviated only in display, with tooltip), indexed hash, live hash and match state, location in lines and UTF-8 bytes, producer, basis, class, qualifiers; a publication ledger lists each call's token agreement.
- **REQ-EV-02** Byte offsets convert to UTF-16 editor offsets only after the live bytes match the indexed hash; otherwise reveal the line without a precise range.
- **REQ-EV-03** `CitationChecker.parse(text)` accepts `path:line`, `path:line-line`, `path#Lx`, `path#Lx-Ly`; anything else is `unsupported`. Each citation resolves under the current collection via `airk_file_info` (+ `airk_changes` selectors when the service supports them) and live hash → `current | moved | changed | not-found | unsupported`. The result footer states how many citations were parsed and that uncited claims were not checked.
- **REQ-EV-04** Watches store `{recipeId, lastPublicationToken, lastFingerprints[]}`. They re-run only on `evidenceInvalidated` with a new token and report `added / removed / changed` fingerprint counts. No timers, no file watchers.
- **REQ-EV-05** `FileNavigation.open(ref)` resolves a workspace URI inside the bound root (canonical containment), checks live hash, then `showTextDocument` + `revealRange`. Unknown or outside-root paths show the source drawer's *not in collection* state and do not open.

## 7. Review workspace

### 7.1 Engine (`review/RuleEngine.ts`)

Pipeline, in order: pin publication → group enabled rules by equivalent fact query → issue one bounded fact query per group → dedupe matched paths → batch `airk_file_info` + live hash for matched files only → apply predicates → fingerprint → baseline compare → view model.

- **REQ-REV-01** Rule records are `airk.explorer.review-rule/1` (plan §5.3), validated by JSON Schema with `additionalProperties:false`. `RuleCompiler` maps `factQuery` to native arguments through the snapshot schema and rejects unsupported fields with `SCHEMA_UNSUPPORTED`.
- **REQ-REV-02** Predicate grammar: field selectors over the fact record; `eq | neq | prefix | contains | lt | gt | in | matches`; Boolean `all | any | not` to depth 4; `matches` accepts an RE2-compatible subset validated at load (reject backreferences, lookaround, nested quantifiers). No scripts.
- **REQ-REV-03** Saved-text rules (e.g., conflict markers) read saved files through `host.readSavedText` within the selected Git scope only; they never join to the index.
- **REQ-REV-04** A finding whose live hash disagrees with the indexed hash carries `verification: 'cannot_confirm'` and cannot be marked reviewed-as-verified; the UI shows `cannot confirm — saved file differs from index`.
- **REQ-REV-05** Ship built-in TypeScript rules in WP4 (`CORE-JS-01 js-dynamic-eval`, `CORE-JS-04 req-body-query`, `CORE-TXT-01 conflict-marker`, `CORE-TEST-02 test-name-only` opt-in). The declarative grammar loader lands in WP8; built-ins are expressed in the same record shape so they migrate without change.
- **REQ-REV-06** AIRK's own Problems diagnostics (source `airk`) are excluded from E3 imports.

### 7.2 Fingerprints (`review/FindingFingerprint.ts`)

```ts
findingId = `${ruleId}:${sha256(normVersion + '\0' + ruleId + '\0' + relPath + '\0' + enclosingDecl + '\0' + normalizeSpan(excerpt)).slice(0,12)}`
// normalizeSpan: CRLF→LF, trim trailing whitespace, collapse runs of spaces/tabs, strip line numbers
// enclosingDecl: nearest E2 declaration name; for unparsed files the nearest Markdown heading; else ''
```

- **REQ-REV-07** Duplicate identical spans in one declaration get an ordinal suffix `#2` derived from span order; if order is ambiguous the finding is flagged `collision` and never merged.
- **REQ-REV-08** Property tests: moving a span changes location only; editing the span changes the ID (`PT-01`, `PT-02`).

### 7.3 Baselines and dispositions (`review/BaselineStore.ts`)

- **REQ-REV-09** Default store: `workspaceState['airk.explorer.review.dispositions']` keyed by `findingId` with `{disposition, reason, ruleVersion, at}`.
- **REQ-REV-10** Opt-in team baseline `.airk/review-baseline.json` (`schema: airk.explorer.review-baseline/1`): writes go through `WorkspaceSavePolicy` with a diff preview and `showSaveDialog`-equivalent confirmation. Reading an unknown schema version shows a migration notice; `normVersion` change triggers a warning that fingerprints may not match.
- **REQ-REV-11** Baseline states: `new | existing | resolved | uncertain | suppressed`; SARIF export maps to `baselineState` and `partialFingerprints.primaryLocationLineHash`-style keys using the fingerprint, not the line.

### 7.4 Git scope, reindex, trace

- **REQ-REV-12** `GitProvider` uses the `vscode.git` extension API (read-only) for working-tree vs HEAD changed saved files; unsaved buffers are listed separately and excluded from rule input.
- **REQ-REV-13** *Index changed saved files now…* is a host command that confirms with a native modal, calls the verified 1.4 management operation (from `discovery.md`), waits for a new publication token, then re-pins and re-runs.
- **REQ-REV-14** Trace tab: requirement → code → test candidates from `airk_document_facts` + bounded `airk_search`/`airk_references`; labels `possible link | not examined | parser unavailable | no link observed`; a button *Open Testing view* runs `workbench.view.testing.focus`; no test results are read.
- **REQ-REV-15** Docs-drift rows compare exact strings only and say so.

## 8. Strategies: Library, Studio, Planner

### 8.1 Library (`strategies/library`)

- **REQ-LIB-01** `strategies/templates.json` holds all 25 templates from plan §6.1 with `templateId`, `version`, `name`, `category`, `description`, `inputSchema`, `requiredTools`, `requiredCapabilities`, `defaultSteps`, `defaultBudgetBytes`, `promptTemplateVersion`, `runnable` (true only for `airk.core/bug`, `airk.core/review`, `airk.core/test-unit` until a fixture qualifies another), `aliases`.
- **REQ-LIB-02** Cards show category, `runnable`/`preview` badge, name, description, `id · version · steps · budget`. Preview panel shows the step list with operator descriptions, required capabilities and the prompt template ID. Non-runnable templates offer *Fork* and *Export prompt skeleton*, never *Run*.
- **REQ-LIB-03** *Load in Studio* opens a dialog (Keep / Create a copy / Replace) when Studio has an edited prompt or a completed run.

### 8.2 Studio (`strategies/studio`) — tabs Builder, Prompt, Script

**Operators.** The DSL op catalog is exactly plan §7.2 (`airk.*`, `host.gitDiff|readSavedText|hashSavedFiles|diagnostics|gitHead`, `core.*`). **REQ-STU-01** `StrategyValidator` rejects any other op, any `${…}` binding outside `params.<key>`, `scope.pathPrefix`, `steps.<id>.output`, cycles, unknown predecessors, duplicate IDs, budgets above advertised limits.

**Runner (`StrategyRunner.run(recipe)`), in order (REQ-STU-02):**

1. Validate recipe; refuse if `!canRetrieve` or template not `runnable`.
2. `EvidenceClient.pin()` → `{publicationToken, serviceInstanceId, collectionFingerprint}`; freeze normalized params.
3. Topological schedule, ≤ 4 concurrent; each call records `{stepId, nativeTool, argsDigest, records, bytes, truncated, cursor, status, errorCode}`.
4. Stop on required-step failure, publication drift (`token` differs in any envelope), capability denial, cursor expiry → `status: failed | stale`. Optional-step gaps → omissions.
5. If `guards.verifyLiveSource`, `host.hashSavedFiles` on cited files; mismatches → omission + `partial` unless `allowPartial` is false, then `stale`.
6. Dedupe key: `fingerprint + token + sourceHash + path + locator + class + producer`. Sort key: exact-name match, scope, class (E2 < E1 < E3 < E4), shorter span, path, line, producer. **If `discovery.md` reports unstable backend ordering, sort by this total key before packing** (ST-09).
7. Clip to `budgets.maxEvidenceBytes` without dropping citations/qualifiers; record truncation reasons in the coverage ledger.
8. `PromptCompiler.render(template, run)` → `compiledPrompt`; `compiledPromptDigest = sha256(compiledPrompt)` excluding `observedAt`.
9. Persist `StrategyRunManifestV1` (plan §7.5, with `serviceVersion`, `protocolVersion`, `serviceInstanceId`, `embedComponentVersion?` replacing `retrievalHelperVersion`) in `globalStorageUri/runs/<runId>.json`.

- **REQ-STU-03** Prompt order: Objective → Repository and evidence scope → Evidence (untrusted, in fences chosen so source cannot close them) → Tasks → Guardrails → Expected outputs → Completion gates. Portable profile embeds excerpts; MCP-aware profile adds conditional `airk_*` instructions phrased as *if actually connected*.
- **REQ-STU-04** `compiledPrompt` and `editedPrompt` are separate; *Rebuild* shows a line diff and asks before replacing; *Copy* copies the visible text exactly; the size footer reads `N chars · ~T estimated tokens (not billing)`.
- **REQ-STU-05** Script tab round-trips `airk.explorer.strategy/1` JSON (plan §7.3); *Validate* uses the same validator; *Apply to Builder* replaces steps and params and clears the run.

### 8.3 Planner (`strategies/planner`) — Queue is merged here

**Model (`PlannerModel.ts`)**

```ts
export type TaskStatus = 'Draft'|'Ready'|'Exported'|'Review'|'NeedsWork'|'Completed'|'Blocked'|'Stale';
export interface PlanTask { taskId: string; title: string; objective: string; fromSnapshotId?: string; source: string;
  dependsOn: string[]; scope: string[]; acceptance: string[]; instructions: string;
  evidenceState: 'verified_snapshot'|'partial'|'stale'|'unverified'; status: TaskStatus; included: boolean; }
export interface PlannerDraft { schema: 'airk.explorer.plan/1'; draftId: string; revision: number; workspaceBinding: string;
  title: string; objective: string; tasks: PlanTask[]; suggestedRelativePath: string;
  compiledMarkdown: string; editedMarkdown?: string; lastSaved?: { uri: string; relativePath: string; sha256: string; revision: number }; }
```

- **REQ-PLN-01** `recomputeBlocked()` after every change: an included task whose `dependsOn` contains a task that is excluded, missing, or not `Completed` becomes `Blocked`; a `Blocked` task whose dependencies are all `Completed` becomes `Ready`. Reordering warns when a task now precedes its dependency.
- **REQ-PLN-02** Status transitions are human actions. Copying the compact prompt sets the first `Ready` task to `Exported`. Setting `Completed` requires a native modal confirming independent review. `evidenceInvalidated` sets tasks with `fromSnapshotId` to `Stale`.
- **REQ-PLN-03** `PlannerMarkdown.build(draft)` emits YAML front matter then the body of plan §8.4:

```yaml
---
schema: airk.explorer.plan/1
plan_id: <slug>
workspace: <folder name>
collection_fingerprint: sha256:<host-owned>
publication: <token or unknown>
extension_version: <from package.json>
tasks:
  - id: T1
    title: "…"
    depends_on: []
    evidence: verified_snapshot
    status: ready
---
```

Output is deterministic for the same tasks, order and versions; timestamps live only in the manifest. Headings and fences inside task instructions are escaped.

- **REQ-PLN-04** Save As implements plan §8.5 host steps 2–10 exactly, with these specifics: `WorkspaceSavePolicy.resolve(relativePathHint)` → `showQuickPick` on multi-root → `showSaveDialog({defaultUri, filters:{Markdown:['md']}})` → canonical containment check (`fs.realpath` for `file:` scheme; parent-URI containment + scheme allowlist `file, vscode-remote` for others; refuse otherwise) → existing-file hash and dirty-buffer check with diff/overwrite/new-name dialog → `createDirectory` → `writeFile` → readback compare → `openTextDocument` + `showTextDocument({preview:false})` → `result.status:'saved'` with `{uri, workspaceFolder, relativePath, hash}`. Any failure returns a truthful status; a receipt whose `expectedDraftRevision` ≠ current revision is discarded.
- **REQ-PLN-05** *Copy saved-plan prompt* is enabled only while `lastSaved.revision === revision`; the text names `lastSaved.relativePath` and the next non-completed task. *Copy anyway* prefixes a one-line unsaved warning.
- **REQ-PLN-06** Completion report: paste box + task selector. `CompletionReport.check(text, taskId)` runs `CitationChecker` (REQ-EV-03), stores verdicts on the task, and moves `Ready|Exported` → `Review`. The UI repeats: an agent claim is not evidence.
- **REQ-PLN-07** Drafts persist in `workspaceState['airk.explorer.planner.draft']` with `revision`; reopening a saved plan file whose front matter parses rehydrates tasks and marks `Stale` where the publication differs.

## 9. Reports & Packs and Management

### 9.1 Reports & Packs (`reports/builder`, `reports/packs`)

- **REQ-RPT-01** Report types: Core review summary; Strategy run bundle (`prompt.md`, `manifest.json`, `evidence.jsonl`, optional `strategy.lock.json`); Architecture boundary evidence. Formats: Markdown, JSON (strict schema `airk.explorer.report/1`), SARIF 2.1.0 (review only). Every export records collection fingerprint, full publication token, service/protocol versions, rule and pack versions, sorted findings, suppressed count, omissions and per-file live state.
- **REQ-RPT-02** Export is a host action through `WorkspaceSavePolicy` (same dialog path as Planner Save As). A best-effort secret scan (high-entropy strings, common key prefixes) warns before export and is labelled best-effort.
- **REQ-RPT-03** Exported `vscode://` links are generated only for the current workspace; on reopen they are validated for trust and containment before navigation.
- **REQ-PCK-01** Pack registry entries: `{id (namespaced), publisher, version, kind: 'review'|'strategy', engineRange, schemaRange, capabilities[], signature?: {keyId, status}, lock}`. Unsigned declarative packs load with a persistent warning; procedural packs are rejected with `POLICY_DENIED` and a reason, regardless of origin.
- **REQ-PCK-02** Built-in `airk.core` is signed with the extension's embedded public key; the key ID appears in the pack list.

### 9.2 Management (all tabs)

Every Management action is a host command that (1) checks workspace trust, (2) checks `canRetrieve` where relevant, (3) shows a native modal naming the exact collection and effect, (4) calls the verified 1.4 management operation, (5) reports observed state, never assumed success (REQ-MGT-01). Preserve existing `sidebar.ts` / `diagnosticsView.ts` handlers by routing the new UI to them rather than reimplementing.

| Tab | Shows | Actions (confirmed) | REQ |
| --- | --- | --- | --- |
| Collections | per collection: root, publication, current/stale/missing counts, semantic %, mode, `no value returned` when unmeasured | Index now; Full re-extraction | REQ-MGT-02 |
| Connections | `airk` connection, service/protocol version, redacted endpoint, instance, supervisor, workspace-host note; `airk-embed` state, version, modes; lexical availability independent of embed | Reconnect; Copy diagnostics | REQ-MGT-03 |
| Scheduling | mode, quiet/idle/max-delay, discovery and enrichment progress as separate channels | Apply; Pause/Resume | REQ-MGT-04 |
| Embedding & vectors | configured vs published detail level; vector toggle disabled when embed not ready | Save (pending until next index) | REQ-MGT-05 |
| Diagnose & repair | four distinct conditions; probes; retained-data verification progress, cancellable | Refresh; Verify; Retry; Resume; Rebuild (modal) | REQ-MGT-06 |
| Storage | root, quotas, usage, eligible cleanup estimate | Choose folder; Save quotas; Reclaim…; Remove inactive… | REQ-MGT-07 |
| Agents & MCP | two cards: Retrieval (15 tools, inventory check vs snapshot, client discovery check) and Assistance (launcher path, runtime, IPC, zero-backend badge); config preview per client with merge diff | Check inventory; Preview config; Check discovery | REQ-MGT-08 |
| Usage | calls, bytes, estimated reduction labelled hypothetical, dollar savings `unknown`; per-plan evidence bytes | Export CSV/MD; Clear… | REQ-MGT-09 |
| Logs | redacted output with level filter | Open Output panel | REQ-MGT-10 |

- **REQ-MGT-11** The status pill, Connections tab and Agents tab read the same `AirkRuntimeState`; there is one source of truth for service state.
- **REQ-MGT-12** Client config writes: generate from the verified discovery result, show a diff against the existing file, and write only on explicit confirmation; never guess an executable or port (INV-10).

## 10. Explorer Assistance MCP

A Node stdio MCP server bundled in the VSIX (`explorerBridge/`), launched by external agents, that serves offline help and admits typed drafts to the host over local IPC. It imports `contracts/` and `help/` only (INV-4).

### 10.1 Launcher and runtime

- **REQ-MCP-01** On activation the extension copies `launcher.js` (≤ 4 KB, no dependencies) to `<globalStorageUri>/assistance/launcher.js` if the content hash differs. The launcher reads `<globalStorageUri>/assistance/current.json` (`{bundlePath, extensionVersion}`) and `require`s the current `server.bundle.js` from the installed extension directory. Agent configs therefore survive extension updates.
- **REQ-MCP-02** Generated client configs run the launcher with VS Code's own Electron as Node: `command: <process.execPath>`, `env: { ELECTRON_RUN_AS_NODE: "1" }`, `args: [launcherPath]`. The Agents tab also offers a plain `node` variant with a note that Node ≥ 20 must be on PATH.
- **REQ-MCP-03** Where `engines.vscode` supports the MCP server-definition provider API, register the same server for in-editor agents (Copilot) through that API; keep `getServerDefinitions()` returning both Retrieval and Assistance definitions without breaking existing consumers.

### 10.2 IPC rendezvous and credentials (`explorerIpc/ipcHost.ts`, `explorerBridge/ipcClient.ts`)

- **REQ-MCP-04** Per window, the host creates an OS-user-restricted endpoint: Unix domain socket `<globalStorageUri>/assistance/ipc/<windowId>.sock` (mode 0600) or Windows named pipe `\\.\pipe\airk-explorer-<userSid>-<windowId>` with a DACL for the current user only.
- **REQ-MCP-05** The host writes a descriptor `<globalStorageUri>/assistance/ipc/<windowId>.json` (mode 0600): `{ endpoint, workspaceRoots[], collectionFingerprint, sessionCredential, expiresAt, extensionVersion, schemaVersions }`. Credentials rotate on reload, trust change, collection rebind and service reconnect; stale descriptors are deleted on dispose.
- **REQ-MCP-06** Client resolution: the server lists descriptors, keeps those whose `workspaceRoots` contain the agent's `cwd` (or an explicit `workspace` argument), and (a) one match → connect, (b) zero → `UI_NOT_ATTACHED`, (c) several → `AMBIGUOUS_TARGET` listing root labels only. It never picks the first.
- **REQ-MCP-07** Every IPC request carries `sessionCredential`; the host validates credential, window, root and collection on each message and answers `STALE_BINDING` on mismatch. Credentials never appear in agent config files, process args or logs.

### 10.3 Tools (P0) and proposal admission

| Tool | Input (schema `proposal-1.json` family) | Output |
| --- | --- | --- |
| `airk_explorer_help_search` | `{query, area?, limit≤20}` | ranked topics `{topic_id, title, summary, route, availability}` |
| `airk_explorer_help_get` | `{topic_id}` | full topic incl. steps, warnings, `supported_since`, fallback |
| `airk_explorer_ui_status` | `{}` | `{attached, trusted, bindingFingerprint, features: [{id, availability}], schemaVersions}` — no paths, no draft content |
| `airk_explorer_suggest_search` | `{schema_version, idempotency_key, title, draft:{query, mode:'lexical', path_prefix?, limit≤100, max_bytes≤65536}, intent}` | receipt |
| `airk_explorer_suggest_strategy` | `{…, template_id, objective, parameters, steps[] (allowlisted ops), evidence_budget_bytes≤65536}` | receipt |
| `airk_explorer_suggest_plan` | `{…, title, tasks[≤20] {task_id, title, depends_on, acceptance?, from_snapshot_id?}, path_hint, short_prompt?}` — **no `save` field** | receipt |
| `airk_explorer_proposal_status` | `{proposal_id}` | `{status}` only |

- **REQ-MCP-08** Receipt is plan §9.4 exactly; `status` is always `pending_review` on admission; `accepted/applied/executed` are `false`.
- **REQ-MCP-09** Limits: payload ≤ 16 KiB, JSON depth ≤ 8, ≤ 50 outstanding proposals per window, TTL 24 h, 30 requests/min per client. `idempotency_key` + canonical body digest dedupes; same key with a different body → `PROPOSAL_CONFLICT`.
- **REQ-MCP-10** The same JSON Schemas validate in the server and again in `SuggestionPolicy` on the host; the host additionally checks trust, binding, `from_snapshot_id` existence and target draft revision.
- **REQ-MCP-11** Help catalog: `docs/explorer-help/<area>/<topic>.md` with front matter `{topic_id, feature_id, route, aliases[], supported_since, prerequisites[], warnings[]}`; `catalog.build.ts` compiles to `resources/explorer-help/catalog.json` and fails on duplicate IDs/aliases, routes absent from `manifest.json`, or an `availability` that contradicts the manifest. Search is bounded BM25-style lexical ranking over title/aliases/summary/body.
- **REQ-MCP-12** Additionally expose help topics as MCP `resources` (`airk-help://<topic_id>`) and the 25 templates as MCP `prompts`; both are read-only views of bundled data.
- **REQ-MCP-13** Instrumented test `MCP-18`: with `airk` and `airk-embed` running and a network/socket spy attached, exercising every tool produces zero connections to anything but the host IPC endpoint.

## 11. Settings, feature flags, persistence and migrations

### 11.1 `contributes.configuration` (REQ-CFG-01)

| Key | Type / default | Purpose |
| --- | --- | --- |
| `airk.explorer.enabled` | boolean / `true` | master switch for the Explorer view |
| `airk.explorer.retrieval.defaultMode` | `"lexical"` (enum lexical only in 1.4) | explicit mode on every call |
| `airk.explorer.retrieval.defaultLimit` | 10 (10–100) | search/references page size |
| `airk.explorer.retrieval.maxBytes` | 8192 (≤ 65536) | per-call payload cap |
| `airk.explorer.evidence.defaultBudgetBytes` | 24000 | Studio default when a template has none |
| `airk.explorer.ideProviders.allowlist` | string\[\] / `["vscode.typescript-language-features"]` | E4 providers permitted |
| `airk.explorer.ideProviders.strictModelFree` | boolean / `true` | disable E4 from non-allowlisted providers |
| `airk.explorer.review.teamBaseline` | boolean / `false` | opt in to `.airk/review-baseline.json` |
| `airk.explorer.planner.directoryHint` | `".airk/plans"` | Save As default folder hint |
| `airk.explorer.assistance.enabled` | boolean / `true` | write launcher + descriptors |
| `airk.explorer.assistance.ttlHours` | 24 | proposal TTL |
| `airk.explorer.assistance.maxPending` | 50 | inbox cap |
| `airk.explorer.usage.recording` | enum `aggregates-only` \| `off` | local usage counters |
| `airk.explorer.feature.*` | boolean per feature ID | see 11.2 |

Telemetry: Explorer adds no telemetry; any future addition honors `telemetry.telemetryLevel` (REQ-CFG-02).

### 11.2 Feature flags (REQ-CFG-03)

Flags mirror `manifest.json` feature IDs: `search`, `code`, `review`, `evidence`, `strategies.library`, `strategies.studio`, `strategies.planner`, `reports`, `packs`, `management`, `assistance`, and reserved-off `review.declarativeRules`, `packs.thirdParty`, `dispatch.adapters`, `cli`. A flag can only set a route to `disabled`; it never promotes `prototype_only` to `implemented` — that promotion is a manifest change in the PR that ships the feature.

### 11.3 Persistence (REQ-CFG-04)

| Store | Location | Content | Schema |
| --- | --- | --- | --- |
| Drafts | `workspaceState` | search draft, studio draft, planner draft (with `revision`) | `airk.explorer.drafts/1` |
| Dispositions | `workspaceState` | finding dispositions | `airk.explorer.dispositions/1` |
| Recipes & watches | `workspaceState` | user recipes, watches | `search-recipe/1`, `watch/1` |
| Runs | `globalStorageUri/runs/` | run manifests + evidence | `airk.explorer.run/1` |
| Snapshots | `globalStorageUri/snapshots/` | Studio result snapshots | `airk.explorer.snapshot/1` |
| Proposals | memory only (default) | inbox | `proposal/1` |
| Assistance | `globalStorageUri/assistance/` | launcher, `current.json`, descriptors | — |

- **REQ-CFG-05** `StorageMigrator` runs on activation: each store has `{schema, version}`; migrations are forward-only functions `vN → vN+1` with a test that loads every historical fixture (`MIG-01`). Unknown future versions are left untouched and reported, not overwritten.
- **REQ-CFG-06** Retention: runs and snapshots older than 30 days or beyond 200 entries are pruned on activation with a log line; drafts and dispositions are never auto-pruned.

## 12. Testing and CI

Use the repository's existing runner for unit tests and `@vscode/test-electron` for integration; add Playwright for webview E2E. Every test carries a tag `[REQ-…]` and a level; the CI summary lists untested REQs and fails if any `MUST` REQ has none (REQ-TEST-01).

### 12.1 Fixture repository (`test/explorer/fixtures/repo/`)

Committed and generated by `scripts/make-fixtures.ts` (REQ-TEST-02): a TS+Rust+Python workspace with real imports, an unresolved call candidate, identifiers inside comments/strings, UTF-8 multibyte offsets, mixed line endings, a Markdown requirements doc and one DOCX, tests, a `.git` with working-tree changes, one file edited after publication (hash mismatch), two identical spans in one declaration, a comment that instructs an agent to delete data, and recipes for stale/interrupted/vector-off collection states.

### 12.2 CI lanes (REQ-TEST-03)

| Lane | Runs | Platform |
| --- | --- | --- |
| unit | contracts, validators, fingerprints, compilers, markdown builders, property tests | ubuntu |
| contract | `CT-01` regenerate tool snapshot from a running `airk` 1.4 and diff; schema round-trips | ubuntu with service binary |
| integration | extension host against the real service on the fixture repo, with and without `airk-embed`; management operations in dry mode | ubuntu (every PR), windows + macos (nightly) |
| e2e | Playwright against the webview at 320/500/700/1024/1440 px; axe-core accessibility | ubuntu |
| security | MCP-18 zero-backend-calls, path traversal, IPC credential rotation, replay | ubuntu, windows |
| perf | PERF-01 activation ≤ 20 ms, PERF-02 bundle size, PERF-03 Assistance cold start, MCP-25 proposal p50 < 150 ms | ubuntu |

### 12.3 Requirement-to-test map (abridged; the full map lives in `test/explorer/REQ-MAP.md`)

| Test ID | Level | REQs | Assertion |
| --- | --- | --- | --- |
| CT-01 | contract | INV-2, REQ-DISC-03 | live listing equals committed snapshot (names, schemas) |
| CT-02 | unit | REQ-CT-03, REQ-CT-06 | unknown tool arg / message type → `SCHEMA_UNSUPPORTED` |
| SVC-05 | integration | INV-3, REQ-ARCH-02 | all reads share one `AirkServiceClient` instance (spy count) |
| SVC-06 | integration | REQ-CT-05, INV-7 | service restart mid-run → `stale`, dependents blocked, rebind required |
| SVC-08 | integration | INV-9, REQ-CT-02 | embed down → lexical search succeeds, vector option disabled with reason |
| EX-07 | e2e | REQ-SRCH-03 | no Load More without cursor; footer wording exact |
| EX-09 | unit | REQ-CODE-02 | TS→Rust name coincidence renders `not linked` |
| EX-13 | integration | REQ-REV-01 | 4 rules → ≤ 2 fact queries; hash checks only on matched files |
| PT-01 / PT-02 | property | REQ-REV-07/08 | fingerprint stable across line moves; changes on edit |
| ST-09 | unit | REQ-STU-02 | shuffled fixture order → byte-identical compiled prompt and digest |
| ST-10 | unit | REQ-STU-03 | injected instruction in source excerpt stays inside the fence as data |
| ST-27..31 | integration | REQ-PLN-04 | cancel → no write; save → file exists, readback hash, editor open; traversal/absolute/symlink denied; dirty buffer → dialog; late receipt discarded |
| ST-32 | e2e | REQ-PLN-05 | compact prompt disabled until saved; names saved path |
| PLN-06 | unit | REQ-PLN-06 | report citations → verdicts; task moves to Review |
| MCP-07 | unit | REQ-MCP-08 | `save: true` in plan proposal → `POLICY_DENIED` (unknown field) |
| MCP-09 / MCP-10 | unit | REQ-MCP-09 | idempotent retry returns same receipt; different body → conflict |
| MCP-15 / MCP-16 | integration | REQ-MCP-06/07 | two windows → `AMBIGUOUS_TARGET`; reload rotates credential, old one rejected |
| MCP-18 | security | INV-4, REQ-MCP-13 | zero connections except host IPC |
| MCP-20 | integration | REQ-MCP-02/03 | Codex CLI, Claude Code and VS Code MCP each list exactly the 7 P0 tools |
| UI-01..06 | e2e | REQ-UI-13 | layouts at each breakpoint; no root horizontal scroll; drawers hidden by default |
| UI-07 / UI-08 | e2e | REQ-UI-14/15 | axe-core clean; dialog focus trap and return |
| MIG-01 | unit | REQ-CFG-05 | every historical store fixture migrates to current |
| GOLD-01..03 | unit | REQ-STU-03, REQ-PLN-03, REQ-RPT-01 | golden files for prompt, plan Markdown and reports |

**REQ-TEST-04** Never claim a passing test without running it; the PR description must paste the CI summary line for each lane.

## 13. Delivery sequence

&#91;embedded content: delivery sequence · 11 work packages, one gate each\]

Each work package is one or more PRs; a package is merged only when its gate tests pass in CI and the discrepancy log has no open item that it depends on. The accent marks the two packages that deliver the product's agent-facing value and should not slip behind WP8.

**PR rules (REQ-DLV-01)**

- One PR per cohesive change; a package may span several PRs but never one giant patch.
- Every PR includes: changed contracts/schemas, feature-flag and manifest `availability` updates, unit tests, integration result where the gate requires one, at least one negative security path, help topic updates, and the pasted CI summary per lane.
- PRs that touch `contracts/` or `schemas/` bump the schema version and add a migration fixture.
- A PR may not add `vscode`, `fs`, `net` or `child_process` imports under `webview/` or `explorerBridge/` (lint-enforced).

**Sizing guidance** (engineer-days, for tracking only): WP0 3 · WP1 6 · WP2 6 · WP3 8 · WP4 8 · WP5 10 · WP6 8 · WP7 8 · WP8 10 · WP9 6. Re-scope at the end of any package that exceeds its estimate by half.

## 14. Definition of Done and increment reporting

### 14.1 Definition of Done for the unified milestone (end of WP7)

- [ ] One VSIX installs Explorer, Strategies, Planner, Management and the Assistance entrypoint; `SVC-01`, `SVC-02` pass (INV-1).
- [ ] `CT-01` green against the live 1.4 service: exactly 15 `airk_*` tools, no `airk_explorer_*` in the backend (INV-2).
- [ ] One `AirkServiceClient` instance serves every read and management call (`SVC-05`); restart and rebind invalidate evidence (`SVC-06`); embed outage leaves lexical working (`SVC-08`).
- [ ] Seven workspaces, Management pinned, Library/Studio/Planner as three routes; all breakpoints and axe checks pass (`UI-01..08`).
- [ ] Search, Code, Evidence show publication token, evidence class and live state on every record; no Load More without a cursor (`EX-07`), no resolved cross-language edges (`EX-09`, `EX-10`).
- [ ] Review runs query-driven rules, fingerprints survive line moves (`PT-01`), stale files cannot be marked verified (`EX-04`).
- [ ] Library lists 25 versioned templates with three runnable; Studio produces byte-identical prompts for identical inputs (`ST-09`) and never silently discards edits (`ST-13`).
- [ ] Planner combines snapshots with dependency gates, writes front matter, saves only through the host Save dialog with readback and editor open (`ST-28`), rejects traversal (`ST-29`), discards late receipts (`ST-31`), and enables the compact prompt only after a confirmed save (`ST-32`).
- [ ] Completion reports are citation-checked and move tasks to Review; Completed requires a confirmation dialog (`PLN-06`).
- [ ] Assistance MCP serves help with `airk` and `airk-embed` stopped (`MCP-01`), admits drafts to a session-bound inbox, and makes zero backend calls (`MCP-18`); `save:true` is rejected (`MCP-07`); two windows resolve or fail with `AMBIGUOUS_TARGET` (`MCP-15`).
- [ ] Settings, flags and stores exist as specified in §11; `MIG-01` passes.
- [ ] Existing indexing, diagnostics, usage and agent-setup workflows still work (`SVC-14`, regression suite from the current repo).
- [ ] `docs/explorer/discrepancies.md` has no open item marked blocking.

### 14.2 Increment report (paste into every PR description)

```markdown
## AIRK Explorer increment — WP<n> · <short title>
**Scope:** <what this PR implements; REQ IDs covered>
**Discrepancies:** <new or resolved entries in docs/explorer/discrepancies.md, or "none">
**Contracts changed:** <schemas / manifest / errors, with version bumps>
**Feature flags / availability:** <routes moved between prototype_only / implemented / disabled>
**Tests actually run:**
- unit: <command> → <pass/fail counts>
- contract: <command> → <result>
- integration: <command> → <result, platform>
- e2e / security / perf: <result or "not in scope for this WP">
**Known gaps:** <what is not done, and the gate it blocks>
**Next:** <the next PR in the sequence>
```

**REQ-RPT-DONE-01** A PR without this block, or with a test line that was not executed, is not mergeable. If a required test cannot run (service binary missing, platform unavailable), say so explicitly and mark the gate blocked rather than skipped.
