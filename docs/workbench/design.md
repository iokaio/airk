# AIRK Workbench implementation plan

**Design baseline:** written 2026-10-29 against the 1.6.4 tree. This is the target design, with its original implementation and qualification gates. It does not describe the published 1.6.4 release.

**Implementation:** the unreleased `feat/workbench` branch implements the sidebar and shared native `parse` prepare/execute path. See the [implementation record](implementation.md) for current commands, contracts, validation and differences from this target. In particular, the candidate classifier is suggestion-only pending independent qualification; do not infer trained-model acceptance or platform qualification from this plan.

The AIRK Workbench is a second editor surface: a tab titled **AIRK** in the VS Code secondary sidebar. People use it to query an indexed project directly, with no agent. Input is a slash command, a reusable evidence recipe or a plain-English request. A fixed parser shipped in the native runtime turns supported requests into visible, editable plans of read-only AIRK tool calls. Results appear as Structured, JSON or Markdown, and each can be copied. Sessions save automatically, and the newest 10 per project can be reopened.

The same proposed `parse` tool is discoverable by agents such as Codex and Claude Code. It can prepare a plan or validate and execute an agent-supplied retrieval plan in one request. The agent decides what evidence the user's task requires; AIRK handles bounded collection, joins, citations and formatting. Neither interface generates an answer or runs an agent. All additions below are proposals, including MCP catalog changes; the current 1.6.4 catalog remains unchanged by this document.

This plan replaces an earlier, uncommitted draft that used MiniLM intent ranking through the `airk-embed` helper and per-project training. That machinery is removed here; §1 says why.

---

## 1. Changes from the earlier draft

| Earlier draft | This plan | Reason |
|---|---|---|
| Three tiers: commands, rules, then MiniLM ranking through a new `rank_text` operation in `airk-embed` | Commands, plus one native `parse` request: rules, an embedded ≈ 0.85 MB linear intent model, and symbol/path lookups against the index | No dependency on vectors, `ort`, `tokenizers` or the embedding helper. Plain English works as soon as a project is indexed, including collections that opted out of vectors. |
| Per-project training and retraining | One fixed model, trained offline from hand-written phrasing templates filled with identifiers harvested from public repositories | Repeatable parsing with pinned inputs, scope, evidence and catalog. The product ships no training code or reset UI; acceptance thresholds are versioned model metadata. |
| A learned slot tagger and a per-project vocabulary file | Rules extract paths, ranges, modes and quoted text; candidate identifiers are checked against the index | The index already is the vocabulary. There is no second artifact to build, version or invalidate. |
| Tool forms generated from JSON schemas in phase 1 | Moved to phase 3 | Commands cover the same ground and ship sooner. |

**Unchanged:** the secondary-sidebar placement; the transcript with editable plan chips; Structured, JSON and Markdown output with copy; auto-saved sessions (newest 10 per project); and reuse of `EvidenceClient`, `samePublication`, `RefRegistry` and the Explorer's webview security setup.

**Size targets:** at most 1 MiB added to the `airk` binary and 300 KiB to the VSIX, measured against the same release profile before and after the change. These are acceptance targets, not measured results; the model, catalog and shared runner must fit together. No new runtime service or inference dependency (see §4.3 step 1 on Unicode normalization).

### 1.1 Prototype review and interaction requirements

The seven [prototype images](images/workbench-overview.png) establish the visual direction, not evidence of implemented behavior. Keep the compact sidebar transcript, anchored composer, project header, editable plan chips and results with an inline format switch. Translate them into the following requirements:

| Prototype | Keep | Clarify or improve |
|---|---|---|
| [Overview](images/workbench-overview.png) | Grouped occurrences, source links, visible resolution and warnings | Say **textual occurrences** in the plan and session title for “who calls”; label total, returned and currently displayed counts separately. Collapsing test files is presentation only. |
| [Commands](images/workbench-commands.png) | Searchable slash menu, examples and keyboard history | Examples insert editable input; selecting a command never runs it. Replace “nothing is sent to a model” with “No generative AI or external network requests; intent parsing runs locally.” Semantic retrieval can use the configured local embedding model. |
| [Ask and fallback](images/workbench-ask-and-fallback.png) | Choice buttons and an explanation when intent is uncertain | No evidence recipe runs until a choice is made. A fallback must show the actual query and mode; lexical AND search must use extracted terms rather than an entire question (§4.3). |
| [JSON](images/workbench-json.png) | Combined envelope and per-step raw payloads | The mockup's child `truncated: true` with outer `truncated: false` is invalid. Aggregate completeness must propagate omissions (§7.1). An abbreviated preview is labeled; copied JSON is complete and parseable. |
| [Markdown and copy](images/workbench-markdown-copy.png) | Preview, publication footer and copy receipt | Receipt reports UTF-8 bytes and the selected format. Preserve citations, coverage warnings and truncation in every format. |
| [Older index](images/workbench-older-index.png) | Per-turn stale badge and explicit Re-run | An unchanged publication does not prove live-file freshness. Show freshness unavailable, source updates pending and saved-file mismatch separately. Re-run appends a result and preserves the original. |
| [Sessions](images/workbench-sessions.png) | Recent ten, turn counts, new session and clear action | Empty sessions are not saved. Add rename, delete one session and Copy All to the session menu. Report save failures instead of claiming “auto-saved”. |

The header shows project name, known published file count and a shortened publication token, with the full identity available in details. Readiness unknown is not a green healthy indicator. Empty, unregistered, disconnected and unindexed states offer specific guidance or a link to Explorer Management; they never initiate indexing or repair from a query.

Use VS Code theme colors, visible focus, accessible names and text alongside status icons. The composer supports Enter to run, Shift+Enter for a newline, Stop while running, and history only when it will not replace edited text. Narrow widths wrap plan chips and scroll code within cards. Keep input and focus during updates; announce completion once through a live region. Advanced scores, full arguments and publication details belong in expandable details.

### 1.2 Common work worth doing without an agent

Prioritize repeated evidence gathering that otherwise spends agent turns issuing searches, opening files and repackaging results. Success means a usable cited bundle, not a generated conclusion.

| Work | Proposed recipe and output | Limit to disclose |
|---|---|---|
| Locate an implementation and understand its file | Definitions → selected file outline → bounded definition excerpt; signatures and links | Multiple definitions need a choice; syntax facts do not establish runtime behavior. |
| Inspect possible impact of a name change | Exact textual occurrences grouped by file; optional candidate relationships from a selected symbol ID | Separate occurrences from candidate edges; neither proves complete impact. |
| Find related tests | Occurrences/code facts within a user-selected test scope, plus cited snippets | Path/name conventions are hints; absence of results does not prove missing tests. Never execute tests. |
| Collect requirements, decisions or configuration evidence | Document facts by kind/name/path → bounded surrounding text, grouped by original document location | Written statements and modality remain evidence candidates, not inferred obligations or an assessment of compliance. |
| Explore a component | Scoped file inventory → selected outlines/imports; a navigable file and declaration map | Bounded inventory may be incomplete; no fabricated architecture narrative. |
| Check evidence after indexing updates | `changes` from an exact baseline or remembered selectors → changed/removed/unknown entries and optional current excerpts | Indexed change observations are not a Git diff or historical source read; coverage floors and expiry remain explicit. |
| Diagnose why retrieval is incomplete | Status/capabilities, then diagnostics only on request; readiness, extraction and policy facts | A successful connection is not proof of available evidence. Reads do not start model setup or repair. |
| Prepare a focused agent handoff | Selected evidence cards → deduplicated excerpts, source hashes, original locations, scope, warnings and the user's objective | Exported source text is untrusted context. The agent verifies relevant live files before editing and supplies its own reasoning. |

Offer these as **Collect evidence** starters with parameter forms and an editable plan, using the existing Strategies concepts instead of a second workflow language. The classifier's 13 primitive intents remain small; these recipes are named compositions, not new learned intents. Ship definition exploration, document evidence and handoff selection with phase 2; add wider impact/test/component recipes in phase 3 after their budgets and evidence quality are measured.

Result actions include Open source, Expand context, Group by file, filter displayed records, Select for bundle and Copy selected. Keep retrieved records separate from display filters. Context expansion is an explicit bounded read at the same publication; if expired, offer a fresh run. “Show more” distinguishes revealing retained rows from fetching a continuation, shows remaining budgets, and never claims all results when the tool exposes no cursor. Continue from validated cursors rather than increasing limits and repeating whole searches.

Follow-ups use explicit scope chips: **this symbol**, **these files**, **selected results** or **since this baseline**. A turn stores the resolved selection, source turn IDs and publication; there is no hidden conversational memory or free-form reference resolver. “What about its tests?” asks for a selection if none is unique. Save a useful recipe as inputs and parameters, without cached evidence, using the existing guarded Strategies save flow on explicit action. Reuse and handoff are more valuable than a longer chat transcript.

---

## 2. Scope check

[Architecture](../architecture.md) says AIRK "does not reason, generate answers, manage agents, route model calls or require an external database", and that core stays free of parser, inference and transport code. This plan stays inside those limits:

- `parse` uses rules, a fixed linear classifier choosing among 13 known intents, and bounded index lookups. Explicit recipes and agent-supplied steps bypass classification. It never generates text, summarizes or answers.
- Every plan is shown before or alongside its results, records which rule or lookup produced it, and can be edited.
- Execution composes only allowlisted read-only evidence operations and pure bounded transforms. It never calls management actions, `draft_*`, shell commands, external services or itself. The Workbench never writes source files; session persistence and explicitly requested recipe saves are host-owned UI storage actions.
- The new code goes in the crates that own each concern (§3.1). `airk-core` receives only serializable contracts.

The [architecture document](../architecture.md) must say this in one paragraph when phase 2 lands.

---

## 3. Architecture

```
Webview (Preact)                 Extension host                          Native (airk worker)
┌───────────────────────┐        ┌───────────────────────────┐   stdio   ┌───────────────────────────────┐
│ transcript, plan chips│ ─msgs─▶│ WorkbenchController       │ ─MCP────▶ │ parse / native airk_parse     │
│ output toggle, copy   │        │  ├ CommandParser (tier 0) │           │  kernel: tokenize, rules,     │
│ session bar, project  │ ◀───── │  ├ parse via EvidenceClient│ ◀──────── │          resolve, plan, run   │
└───────────────────────┘        │  ├ Execution adapter      │           │  retrieval: intent classifier │
                                 │  ├ Renderers              │           │ 17 read-only evidence tools   │
                                 │  └ SessionStore           │           └───────────────────────────────┘
                                 └───────────────────────────┘
```

### 3.1 Native side

Placement follows the [dependency boundaries](../architecture.md#dependency-boundaries):

| Crate | Additions |
|---|---|
| `airk-core` | Serializable `ParseRequest`, `Plan`, `PlanResult`, `IntentId`, `StepBinding`, budgets, version identities and feature-spec version. No tokenizer, inference, execution or index access. Add `ClientFamily::Workbench` with the `airk-workbench` label in `usage.rs` (phase 1). |
| `airk-retrieval` | `intent/`: feature hashing, the quantized linear classifier and model-file validation. The model ships in `assets/intent-v1.bin` via `include_bytes!`. Pure Rust, no feature that pulls in `ort`, so the main `airk` dependency check in `scripts/build-service-runtime.py` still passes. |
| `airk-kernel` | `parse/`: tokenize, rules, resolve, plan validation, intent/recipe catalogs and a bounded runner calling existing collection-scoped read services. Pure selectors/transforms live here; no MCP recursion or generic task scheduler. Reuse existing snapshot admission and cancellation. |
| `airk-mcp` | Strict `parse` schema, public discovery/help, native alias mapping and response serialization. Add one public tool to the current 27-tool unified catalog when phase 2 ships; verify catalog counts and each referenced operation. Execution works without Explorer. |
| `airk-cli` | Worker dispatch and public `airk parse "<text>" --project <id>` (prepare by default), plus `--execute` and structured request input from a file/stdin. The dev-only `dump-vocab` subcommand (§5.2) stays behind a non-default feature. |

**Discoverability is required:** add `parse` to unified `tools/list`, `help(tool: "parse")` and native catalog/dispatch together. Do not rely on calling an unlisted name. Its description states prepare versus execute, bounded read-only composition, uncertainty and partial-result behavior. Existing direct tools remain available for a single precise read.

Public MCP names are unprefixed (`parse`, `search`, `read_file`); native aliases remain `airk_parse`, `airk_search`, `airk_read_file`. Plan contracts use public operation names, with an explicit adapter mapping to aliases. Older mockups show aliases and are not the proposed public schema. Codex, Claude Code and the CLI reach the same native parser/runner without an editor attachment.

### 3.2 Extension side (`packages/vscode/src/`)

| Module | Role |
|---|---|
| `workbenchView.ts` | `WebviewViewProvider` for `airk.workbench`. Copies the Explorer's CSP (`default-src 'none'`, nonce scripts and styles), `localResourceRoots` limited to the bundle, and `enableCommandUris: false`, as in `explorerPanel.ts`. |
| `workbenchContracts.ts` | Versioned, validated message schema, built the same way as `parseWebviewMessage` in `explorerContracts.ts`. |
| `workbenchController.ts` | Project selection, explicit follow-up selections, message routing, superseding and cancellation. Calls prepare for edits/uncertainty and execute for complete requests. |
| `workbenchCommands.ts` | Tier 0 grammar for `/…` input (§4.6). Answers `/help` in-process with `HelpIndex`. |
| `workbenchExecutor.ts` | Phase 1 command execution through `EvidenceClient` and `samePublication`. In phase 2 becomes an adapter to the native shared runner; no second full executor for recipes. |
| `workbenchRender.ts` | Pure JSON-envelope and Markdown renderers, covered by golden tests. |
| `workbenchSessions.ts` | Session store (§8). |
| `src/workbenchWebview/*.tsx` | UI. A new esbuild entry in `scripts/build-explorer.cjs` with its own `checkInputs` allowlist. |

The Workbench opens its **own `EvidenceClient`** lazily and disposes it with the controller. The bridge admits at most 4 in-flight calls and the Explorer client permits 3 outstanding; separate clients avoid competing in one local queue, but still share native worker admission and resource limits. Parameterize the validated client family and allowlist rather than copying the transport. Use `clientName: 'airk-workbench'` for this UI; preserve the actual agent family for agent calls. Keep `WORKBENCH_TOOLS = [...NATIVE_TOOLS, 'airk_parse']` separate from the Explorer allowlist. `parse` is an entrypoint only, never an operation inside a plan.

Reuse the existing Strategy runner's evidence ledger, selector semantics, ordering and completeness fixtures where compatible. The current TypeScript Strategy runner is not a native implementation: phase 2 ports the required read-only subset and adds parity fixtures. Host operations such as Git inspection and saved-file hashing stay in the extension and are excluded from agent plans. Requested UI live verification is separately labeled host evidence and never replaces an indexed payload.

### 3.3 Manifest

The proposed contribution uses `secondarySidebar`, matching the prototypes. Verify registration, focus, restore and resize in the pinned VS Code harness before shipping; mockups do not establish contribution-key compatibility. If unsupported at the declared engine floor, choose a supported location or deliberately revise that floor before implementation acceptance.

```jsonc
"viewsContainers": {
  "activitybar":      [ { "id": "airk", "title": "AIRK", "icon": "images/airk.svg" } ],
  "secondarySidebar": [ { "id": "airk-workbench", "title": "AIRK", "icon": "images/airk.svg" } ]
},
"views": {
  "airk-workbench": [ { "id": "airk.workbench", "name": "AIRK Workbench", "type": "webview" } ]
}
```

Commands: `airk.workbench.focus`, `airk.workbench.newSession` and `airk.workbench.clearSessions`. Phase 3 adds `airk.workbench.searchSelection`.

---

## 4. The parser

### 4.1 Plan shape

```ts
interface Plan {
  schema: 'airk.parse.plan/1';
  tier: 'command' | 'parsed' | 'recipe' | 'explicit';
  project: string; path_prefix: string;       // fixed authorized scope for every step
  publication?: string;                     // required for indexed evidence, not offline help
  catalog: string; digest: string;           // normalized executable fields + scope + versions
  intent?: { id: IntentId; logit?: number; margin?: number; model?: string };
  alternatives?: IntentId[];                 // uncertainty => no executable steps
  requires_input?: Array<{ field: string; reason: string; choices?: string[] }>;
  rule?: string;                             // id of the rule that decided, when a rule decided
  resolution?: { wanted: 'symbol' | 'path'; candidate: string; matched?: string; via?: 'exact' | 'folded' | 'subtokens' | 'prefix';
                 matches?: number; fallback?: 'search' };
  budgets: { calls: number; fanout: number; seconds: number; max_bytes: number };
  output: { view: 'raw' | 'evidence'; max_bytes: number };
  steps: Array<{ id: string; tool: EvidenceOperation; args: Record<string, unknown>;
                 bind?: StepBinding[] }>;
}
```

`logit` and `margin` are integers in the model's fixed-point scale, not probabilities. See §4.4 on determinism.

### 4.2 Intent catalog

The 13 intents correspond one-to-one to the 13 commands in §4.6:

`search` (default), `find_definition`, `references`, `read_file`, `outline`, `list_files`, `file_info`, `imports`, `facts`, `changes`, `status`, `diagnostics`, `help`.

Each intent has a plan template in `crates/airk-kernel/assets/intents.json`. Version the primitive intent catalog separately from recipe/operation schemas. `Plan.catalog` binds the complete executable catalog; classifier metadata binds the 13-label intent catalog only, so adding a recipe does not require retraining:

```jsonc
{ "id": "find_definition", "needs": ["symbol"],
  "steps": [ { "id": "definition", "tool": "symbols", "args": { "name": "$symbol" } },
             { "id": "structure", "tool": "outline", "args": {},
               "bind": [{ "arg": "path", "from": "definition", "select": "unique_definition_path" }] } ] }
```

**Bindings use named selectors, not JSON paths.** Tool payloads aren't uniform. For example, `airk_symbols` returns `items[].file` as an index into `files[]`; there is no `hits[].path`. Each selector is implemented and tested once per tool:

| Selector | Source tool | Meaning |
|---|---|---|
| `unique_definition_path` | `symbols` | Decode the file table and require one unique selected path; several unresolved paths require a choice. |
| `selected_symbol_ids` | `symbols`, `code_facts` | Retain native IDs with project/publication provenance; never substitute a written name for an ID. |
| `hit_locations` | `search`, `references`, `document_facts` | Decode supported terse/full representations into cited path/range/hash/original-location records, stable order, deduplicated and capped. |

`StepBinding` names the destination argument, earlier step ID and a tool-specific selector; location selectors bind an atomic path/range/hash tuple for a bounded read fan-out. They are validated against the consumer's schema. No arbitrary JSONPath, expression evaluation, forward edges, cycles or user code. First-ranked-hit selection is allowed only when the request explicitly asks for top-ranked candidates and the output says so. If a binding discovers ambiguity after earlier evidence steps ran, stop that branch and return `partial` with `requires_input`; do not claim that nothing ran.

An empty source result marks dependent steps skipped with a reason; independent steps may finish. Ambiguity is different from empty evidence and must never silently choose the first definition.

**Two intents need care in wording:**

- **`references` finds text, not callers.** `airk_references` returns exact-case, whole-identifier textual occurrences (definitions, strings and comments included). It does not resolve calls. "who calls X" still maps here, but the chip reads "textual occurrences of X". AIRK must not claim resolved references.
- **`facts` depends on the file.** It maps to `airk_code_facts` or `airk_document_facts`, depending on the resolved path's language/profile from `airk_file_info`. With no path, it asks.

The UI answers `help` in-process with `HelpIndex`, which already includes native engine guidance. The public/CLI path uses native help; neither requires an indexed publication or Explorer attachment.

### 4.3 The `parse` pipeline (native, deterministic)

1. **Normalize.** Trim, collapse whitespace, Unicode-lowercase a copy for features, and keep the original for display. Full NFC normalization would add `unicode-normalization` as a direct dependency; it is only a transitive lock entry today. Skip NFC unless measured inputs need it (open decision 5).
2. **Tokenize.** Split on whitespace and punctuation except inside quoted strings. Keep tokens intact if they contain `/`, `::` or an internal `.`, or end in a known extension.
3. **Extract with rules.**
   - Preserve quoted strings as literal text with an explicit quoted/literal flag. A lexical phrase guarantee is offered only if the underlying query contract supports it; never imply byte-exact or phrase search from quoting alone.
   - Path-like tokens become `path`.
   - `:a-b` or `:a` becomes a line range.
   - Explicit mode options become `lexical|vector|hybrid`; normalize `semantic` to `vector`. `exact` describes literal/name matching, not a fourth search mode. Words inside the query do not become options merely because they match a mode name.
   - An integer after `top|limit|first` becomes `limit`.
   - CamelCase, snake_case, and `::`- or `.`-qualified tokens become `identifiers[]`.
4. **Choose the intent.**
   - A command word typed without the slash (`refs Foo`) is decided by a rule before the model runs.
   - Otherwise the model scores the input (§4.4). The top intent is accepted if `logit ≥ τ` and `logit − second ≥ μ`.
   - If not accepted, the plan returns the top three `alternatives` and no steps.
5. **Resolve.** Lookups use the publication pinned for this request.
   - **Symbols:** check `identifiers[]` in order — exact name, then case-folded, then sub-token set (`evidence client` ↔ `EvidenceClient`).
   - **Paths:** check `path` against the file inventory, exact first, then as a prefix.
   - **Outcome:** one match fills the template. Several return bounded labeled choices and `requires_input`, with no evidence execution. A no-match result can propose search with `fallback: 'search'`; show the rewritten query before execution. Explicit structured plans never silently change tools or arguments.
   - **Search terms:** preserve identifier spelling and quoted content, remove only recognized request grammar (for example “where is … defined”), and expose the actual terms. Lexical search ANDs terms, so submitting every word of a question is a poor fallback. Omitted mode follows saved policy; explicit vector/hybrid on unavailable vectors reports the capability limitation rather than changing mode invisibly.
   - **Native work needed:** exact symbol lookup exists today (the `symbols` path in `airk-mcp`). No case-folded or sub-token lookup was found in kernel or store. Phase 2 must add a bounded kernel query for them (open decision 4).
6. **Emit** the `Plan`, recording every decision: the rule that fired, the resolution step that matched, and the publication. The UI can then explain the plan, and tests can pin it.

Rules and resolution are deterministic for the input, explicit scope/options, catalog/model version and indexed publication. The model only chooses among the 13 fixed intents. Readiness and timing are observations, not deterministic model output. Never claim “the same plan on every machine” without the same scope and indexed evidence.

### 4.4 Intent model

A hashed-feature linear classifier in the style of fastText's linear model, without an embedding layer.

- **Features:**
  - Lowercased word unigrams and bigrams.
  - Character 3- to 5-grams of each token, with boundary markers.
  - Sub-tokens of identifiers.
  - Three shape flags: has-path, has-range, has-quote.
- **Hashing:** FNV-1a 64-bit into 2^16 buckets.
- **Weights:** `int8[65536 × 13]` (851,968 bytes), plus `int16` biases and per-intent scales. The whole file is about 0.85 MB, within the 1 MB binary budget.
- **Scoring:** integer accumulation in a fixed feature order produces fixed-point logits.
  - Acceptance (`τ`, `μ`) and ranking use only these integers, designed for bit-identical results on Windows, macOS and Linux. Pin accumulation widths, overflow behavior and tie-breaking, then verify on the supported hosts.
  - Probabilities are not part of the plan or of any golden output. Rust's `f32::exp` is not guaranteed bit-identical across platforms. If the UI shows a confidence bar, it derives one from `logit`/`margin` for display only.
- **File format:** a versioned, explicitly sized header (magic, schema version, bucket count, intent count, feature-spec version, full catalog digest, `τ`, `μ`), followed by declared weight, bias and scale sections. Specify widths/endianness, overflow bounds, checksum and exact file length before implementing it; a full SHA-256 digest alone occupies 32 bytes, so the previous 32-byte total header was insufficient.
  - The file is validated once per worker.
  - A mismatched catalog digest or feature-spec version is a load error. It is never accepted silently.

### 4.5 Why no slot tagger

Paths, ranges, modes and quotes usually have explicit syntax, but quoted paths, Windows separators and qualified names still require validation. The index can supply candidates, not the user's intended meaning. Keep a deterministic resolver and ask when several candidates remain; evaluation must justify any later slot tagger behind the same contract.

### 4.6 Tier 0 commands

| Command | Intent | Tool calls |
|---|---|---|
| `/search <q> [--mode m] [--path p] [--limit n]` | `search` | `airk_search` |
| `/symbols <name>` | `find_definition` | `airk_symbols`, then `airk_outline` |
| `/refs <name> [--path p]` | `references` | `airk_references` |
| `/read <path>[:a[-b]]` | `read_file` | `airk_read_file` (default lines 1–200, bounded by `max_bytes`) |
| `/outline <path>` | `outline` | `airk_outline` |
| `/files [prefix]` | `list_files` | `airk_list_files` |
| `/info <path>` | `file_info` | `airk_file_info` |
| `/imports [name] [--path p]` | `imports` | `airk_imports` |
| `/facts <path> [name]` | `facts` | `airk_code_facts` or `airk_document_facts` |
| `/changes --since <token> [--path p]` | `changes` | `airk_changes` |
| `/status` | `status` | `airk_status` |
| `/diagnostics` | `diagnostics` | `airk_diagnostics` |
| `/help [topic]` | `help` | none (in-process `HelpIndex`) |

This table uses native aliases; saved/public plans use the unprefixed operations in §3.1. `/symbols` follows a unique selected definition, rather than the first of several. `/changes` requires `--since <token>` or explicit remembered selectors; with neither it asks for a baseline instead of claiming changes “since this morning”. `/read` treats `path:a-b` as convenience syntax; structured arguments remain separate and Windows drive colons are not line delimiters. Extracted-document reads stay within one original page/block/slide.

Add `/context`, `/capabilities` and `/relationships` as explicit commands/forms by phase 2 so useful evidence tools do not depend on classifier training. `/relationships` takes a selected native symbol ID, direction and 1–3 hops, with candidate/coverage warnings. `/help` works offline in the UI; public `parse` may route it to native help without Explorer.

### 4.7 Agent use: prepare and execute through the same `parse` tool

**Purpose:** let an agent turn its own evidence request, derived from the user's prompt, into one validated retrieval operation. For example, before fixing a retry bug, Codex can ask for declarations, textual occurrences, relevant requirements and bounded excerpts under a component scope. AIRK collects the specified evidence; the agent reviews it, verifies live sources and decides the fix. This reduces repetitive tool-selection and result-packaging turns. It does not guarantee sufficient evidence for arbitrary tasks or perform the agent's reasoning.

The proposed tool description is:

> Prepare or execute a bounded read-only AIRK evidence plan. Supply request text, a named recipe with parameters, or explicit steps and bindings. Use prepare to inspect resolved operations and missing inputs; use execute to collect cited evidence in one call. Scope, publication, call/time/byte limits and output selection apply to every step. Uncertainty returns choices without execution. Partial coverage, stale publications, truncation and tool failures remain explicit. Does not edit files, run commands, index, call AI models for reasoning, or apply Explorer drafts.

**Request contract:** strict versioned object with `schema: "airk.parse.request/1"`, `mode: "prepare" | "execute"` (default prepare), one of `text`, `recipe` or `plan`, optional `expected_digest` for a prepared plan, plus explicit `project`, literal `path_prefix`, budgets and output options. A configured single-project connection may supply its default project; multi-root connections require the configured project ID. Neither the active editor nor the Explorer dropdown changes it. Bound request size to 32 KiB and reject unknown fields, unsupported schema versions and out-of-range limits before any evidence work.

- **Text:** deterministic intent parsing for supported requests. Preparation can read bounded symbol/path metadata for resolution; `executed: false` means no evidence steps ran, not zero index reads. Include resolution work in budgets and usage.
- **Recipe:** a versioned catalog ID plus typed parameters, expanded into inspectable steps without model classification. Missing values return `requires_input`.
- **Plan:** explicit steps with stable IDs, operation arguments and named bindings, bypassing classification. This lets an agent express multiple evidence needs precisely instead of relying on the small intent model to understand a compound prompt. `prepare` validates and normalizes it; `execute` validates it again. No hidden steps are added except disclosed scope/budget enforcement and required metadata validation.

**Two usage paths:** a complete explicit plan or unambiguous recipe/text can run immediately with `mode: "execute"`. For review or editing, prepare first, inspect the returned normalized plan and digest, then submit that plan with `expected_digest`. The digest binds the executable fields, scope, publication, budgets, output and catalog versions; it is a change guard, not authorization. The server always revalidates permissions and arguments. Changed catalogs/digests or expired publications require fresh preparation; never execute a stale plan against the latest publication silently. Prepared plans are inert values, not durable jobs or bearer capabilities.

The following illustrative explicit request collects two independent evidence sets and reads bounded context for the second. The final JSON Schema must pin the selector/argument types and tool limits; these are proposed fields, not existing APIs:

```json
{
  "schema": "airk.parse.request/1",
  "mode": "execute",
  "project": "<configured-project-id>",
  "path_prefix": "packages/vscode/src/",
  "budgets": { "calls": 8, "fanout": 3, "seconds": 20, "max_bytes": 48000 },
  "output": { "view": "evidence", "max_bytes": 24000 },
  "plan": {
    "steps": [
      { "id": "declarations", "tool": "symbols", "args": { "name": "EvidenceClient", "limit": 5 } },
      { "id": "occurrences", "tool": "references", "args": { "name": "EvidenceClient", "limit": 12 } },
      { "id": "context", "tool": "read_file", "args": {},
        "bind": [{ "arg": "location", "from": "occurrences", "select": "hit_locations" }] }
    ]
  }
}
```

Here `location` is a typed binding destination, expanded to the underlying tool's path/range/hash arguments; it is not a new `read_file` MCP argument. Fan-out selects at most three unique locations, records skipped locations and stays within the total call budget. The caller can instead provide exact paths/ranges directly. A parent `path_prefix` is enforced even on tools whose native schema has only a path or symbol selector; a child may narrow scope but never widen it.

**Response:** `status` (`prepared`, `needs_input`, `complete`, `partial`, `failed`, `cancelled`, `publication_changed`), `executed`, normalized plan/digest, actual scope/publication, step reports, selected evidence, warnings, completeness and consumed budgets. `executed` becomes true once any evidence step starts; it never implies completion. `needs_input` before execution, or `partial` with missing-input details discovered during execution, returns machine-readable fields/candidates so the agent can refine its request without a forced UI round trip. Evidence is returned directly, not placed in the Suggestions Inbox. No agent session appears in Workbench history unless the user explicitly imports it.

`output.view: "evidence"` returns deduplicated selected records plus step summaries; `raw` includes original decoded tool payloads. Output limits clip only at record boundaries with explicit omission counts and reasons, retaining citation/hash/location bundles together. Selection/ordering are deterministic filters and transforms, never an AI summary. The agent can request an explicit follow-up for omitted evidence; AIRK does not autonomously widen scope or loop until an answer appears.

Describe this as finite composition of retrieval queries, within AIRK's local retrieval boundary. The external agent owns task planning, reasoning, source edits and any further tool requests. `parse` has no model routing, open-ended branching, arbitrary scripting, recursive plans or durable orchestration.

---

## 5. Training the fixed model (offline, in the repository)

The trainer lives in `tools/parser-train/`: Python, with a pinned `requirements.txt`. Committed outputs are `crates/airk-retrieval/assets/intent-v1.bin` and `tools/parser-train/TRAINING.md`.

**Not committed:** cloned repositories, harvested vocabularies and generated datasets. They go under an ignored working directory (for example `target/parser-train/`) and are rebuilt from the manifest.

### 5.1 Training repositories

These were chosen for permissive licenses, language variety, real documentation and recorded design decisions.

- **Licenses:** the script clones each repository at a pinned commit and checks the LICENSE file at that commit before using it. The licenses below are as listed when the plan was written; they were not verified for this document and must be checked when pins are set or changed.
- **Attribution:** the derived weights carry no source text, but repository attributions are recorded in `TRAINING.md`. Whether they also belong in `THIRD_PARTY_NOTICES.md` is open decision 6.

| Repository | Language | License | Why it's useful |
|---|---|---|---|
| `backstage/backstage` | TypeScript | Apache-2.0 | Architecture decision records in `docs/architecture-decisions`; large plugin tree |
| `nats-io/nats-server` + `nats-io/nats-architecture-and-design` | Go, Markdown | Apache-2.0 | Decision-record corpus covering design specs and conventions |
| `dotnet/runtime` | C#, C++ | MIT | `docs/design`, Book of the Runtime, coding and API guidelines |
| `actions/runner` | C# | MIT | Decision records in `docs/adrs`; mid-sized .NET service with tests |
| `simpleiot/simpleiot` | Go, TypeScript | Apache-2.0 | `docs/adr`; smaller, good evaluation candidate |
| `tokio-rs/tokio` | Rust | MIT | Large Rust API surface, module docs, extensive tests |
| `rust-lang/rust-analyzer` | Rust | MIT / Apache-2.0 | `docs/dev/architecture.md`; one crate per concern; rich symbol names |
| `fastapi/fastapi` | Python | MIT | Documentation-heavy; tests mirror the docs |
| `etcd-io/etcd` | Go | Apache-2.0 | Design docs in `Documentation/`; integration tests |
| `kubernetes/enhancements` | Markdown | Apache-2.0 | KEPs: long design documents with consistent headings |
| `open-telemetry/opentelemetry-specification` | Markdown | Apache-2.0 | Specification and OTEPs; terminology shared across languages |
| `protocolbuffers/protobuf` | C++, Java, Python, C# | BSD-3-Clause | Multi-language monorepo; generated and hand-written code side by side |

`simpleiot`, `actions/runner` and `fastapi` are held out entirely for evaluation; the model trains on the rest. Rotate the held-out set between model versions.

### 5.2 Harvesting identifiers

1. Index each repository with AIRK itself.
2. Run `airk dump-vocab --project <id>`, a dev-only subcommand behind a non-default Cargo feature (§3.1). It exports symbol names and kinds, relative paths, extensions, document headings, and location labels where present.

This is the repositories' only use. No source text enters the training data.

### 5.3 Templates

`tools/parser-train/templates.yaml` holds 15–40 phrasings per intent for each role (developer, tester, architect, technical writer), with slots:

```yaml
references:
  developer: ["who calls {symbol}", "usages of {symbol}", "where is {symbol} used"]
  tester:    ["textual mentions of {symbol} in tests", "test files mentioning {symbol}"]
  architect: ["textual dependencies mentioning {symbol}", "occurrences of {symbol}"]
  writer:    ["where is {symbol} referenced in the docs"]
```

Templates are reviewed by hand and committed. A model may propose phrasings; a person merges them.

### 5.4 Expansion and noise

1. Fill the templates with harvested values.
2. Apply seeded, deterministic noise:
   - lowercase identifiers;
   - drop a character;
   - split CamelCase into words;
   - truncate to a fragment;
   - add trailing punctuation;
   - substitute a near-miss name.
3. Aim for about 40,000 examples, balanced per intent.

The generator is seeded, so the dataset can be rebuilt exactly from the manifest.

### 5.5 Training and acceptance

**Training method:**

1. Hash features, then fit L2-regularized multinomial logistic regression (scikit-learn).
2. Quantize to int8 with the same feature hasher the Rust code uses.
3. `tools/parser-train/hasher.py` is a port of the Rust tokenizer and hasher. A golden-hash fixture checked by both the Python and Rust test suites keeps them identical.

**Acceptance floors**, enforced by the training script and recorded in `TRAINING.md`:

| Measure | Floor |
|---|---|
| Intent accuracy on the held-out repositories | ≥ 0.95 |
| Accuracy for each role | ≥ 0.90 |
| Accuracy on 150 hand-labeled utterances (never trained on) | ≥ 0.92 |

**Thresholds:** tune `τ` and `μ` on a separate calibration split, then evaluate once on held-out data. Target an ask rate at or below 10% for supported, unambiguous requests, but never lower confidence thresholds just to meet that rate. Add unsupported tasks, ambiguous names, compound requests, missing scopes and misleading quoted instructions as abstention cases. Report accepted-plan precision, wrong automatic executions, abstention and slot/query correctness separately from intent accuracy. Require zero wrong automatic executions on the curated ambiguity/unsupported safety fixture; generalization still needs measured evidence.

**What CI checks:** CI does not clone repositories or retrain. A workspace test loads the committed `.bin` and enforces:

- the 150-utterance floor;
- the golden inference outputs;
- the model header's catalog digest against `intents.json`.

The held-out repository figures come from training time and are recorded in the manifest; CI does not re-measure them.

### 5.6 Manifest

`TRAINING.md` records:

- repository pins and the licenses verified at those pins;
- the template file hash and the seed;
- dataset size and metrics, per held-out repository and per role;
- the chosen `τ`/`μ` and the model digest.

Retraining is developer work: change templates or pins, re-run, commit the new `.bin`, and bump the model version.

---

## 6. Running a plan

The phase 2 native runner is the authority for Workbench, CLI and MCP execution. It invokes existing kernel read services directly, preserving their contracts; it does not make nested MCP calls. Phase 1's small host command runner uses the same fixtures until replaced.

- **Limits:** at most 6 logical steps, 12 underlying read calls, 12 total fan-out targets, 30 seconds and 512 KiB of combined materialized payload. Defaults may be lower. Resolution, metadata validation, continuations and retry attempts count toward call/time budgets. Reserve capacity before dispatch, and reject an impossible requested budget before execution. Step `limit`/`max_bytes` are capped by remaining run limits; serialized framing and final output have their own checked size cap. No unbounded intermediate arrays.
- **Scope:** fix the configured project and allowed literal prefix at admission. Validate every direct path, derived path, cursor and symbol ID against that scope; a binding cannot escape through aliases, `..`, absolute paths or another collection. Retrieved text is data and cannot add operations or override the request.
- **One publication, not a historical snapshot:** resolve and execute against one exact current token. Where read APIs accept revision/hash validators, pass them; otherwise validate the returned identity before admitting results. Current tokens can expire between steps; this plan does not add a long-lived historical reader or block indexing. Missing or inconsistent identity on an evidence-bearing result fails closed. A change stops dependent work and returns `publication_changed`; retained partial records keep their original identity and are not presented as a complete bundle.
- **Observations:** offline help and status/capability/diagnostic observations can lack an evidence publication. Preserve observation timestamps and readiness separately; do not force them through `samePublication` or treat unknown status as a usable evidence snapshot. Evidence steps require a valid publication before using resolved bindings.
- **Scheduling:** validate the whole acyclic plan first. Run independent reads within existing native admission, with per-run concurrency at most 3; keep stable output order by step ID and original record order. Dependencies wait for validated results. No background jobs or new service scheduler.
- **Cancellation:** per-turn supersede keys cancel queued work and discard late replies. Propagate native cancellation/deadline where supported; Stop does not claim already admitted reads were undone. New turns cannot receive old replies. A project change fences the previous generation before opening the next client.
- **Errors and retries:** preserve typed native codes and per-step outcomes (`complete`, `failed`, `skipped`, `cancelled`). Retry only an explicit safe-to-repeat hint, with unchanged arguments and within the original deadline; transport timeouts or uncertain submission are not retry permission. Whole execute calls are not blindly replayed after partial work. Aggregate partial failures and omitted dependent steps; do not hide a failed step behind successful independent results.
- **Read-only:** validate the closed evidence-operation catalog in native code and again at the host boundary. Exclude `parse` itself, UI drafts, management and host operations. Only fixed dedupe, ordering, projection, citation and clipping transforms are allowed, with no arbitrary code.
- **Usage:** record caller family, underlying work and actual returned bytes without double-counting the outer request and inner reads as independent savings. Keep query/source content out of aggregate usage. Show measured calls, duration and bytes; estimated avoided context must retain its baseline and limitations. Do not claim billed savings or fixed agent-token reductions without a matched-task evaluation.

---

## 7. Output formats and copy

The result header has a **Structured | JSON | Markdown** toggle. **⧉ Copy** copies exactly the format shown. A session menu adds "Copy session as Markdown" and "Copy session as JSON".

### 7.1 JSON envelope

Raw mode retains tool payload values as decoded, without rewriting evidence. Canonical JSON sorts object keys but preserves arrays, strings and numeric values; this is not the transport's original byte stream. Live timings/request IDs prevent byte-identical runs, so golden fixtures normalize those fields. Evidence mode is an explicitly labeled projection and keeps per-record source-step provenance.

```json
{
  "schema": "airk.workbench.result/1",
  "project": "54c6…", "publication": "airk-r1:…",
  "input": "who calls EvidenceClient",
  "status": "complete", "executed": true,
  "plan": { "schema": "airk.parse.plan/1", "tier": "parsed", "intent": { "id": "references" }, "steps": [ "…" ] },
  "steps": [ { "id": "occurrences", "tool": "references", "arguments": { "name": "EvidenceClient" }, "status": "complete", "elapsed_ms": 41, "result": { "…": "decoded payload" } } ],
  "warnings": [ "Textual occurrences, not resolved callers." ],
  "truncated": false
}
```

This is an abbreviated envelope illustration, not a schema-valid plan. The normative result schema includes `completeness` with separate flags/reasons for upstream truncation, unconsumed cursors, fan-out omission, output clipping, budget exhaustion, step failure and publication mismatch. Outer `truncated` is true for any upstream or downstream clipping; `status: complete` requires all requested steps and outputs to be complete. Unsupported extraction coverage remains an independent warning even when retrieval completes. A collapsed UI card is not truncation.

Retain exact publication tokens, content hashes, read validators, artifact/document/symbol identities and original page/slide/block/row labels when supplied. Do not dedupe records across revisions or collapse distinct original document locations into one file/line key. A bundle includes the objective, actual scope and query, provenance, completeness and source read-only warning in every export.

### 7.2 Markdown

Each kind of tool result has a pure renderer:

- **Search and read results:** a `### path:start-end` heading, the location label (page, slide, row or block for extracted documents), then a fenced block.
  - The fence language comes from the file extension.
  - The fence is always longer than the longest run of backticks in the content.
- **Symbols, files and references:** tables, with `|` escaped.
- **Status and diagnostics:** key/value lists.
- **Footer:** the publication token and warnings, reusing the Explorer's wording (for example, "similarity candidates, not proof of relevance").
- **No raw HTML.** Formatting is pinned by golden fixtures, as Strategies does under `src/test/fixtures/explorer/`.

### 7.3 Structured view

The Structured view renders Preact components from the same view model. It needs no Markdown-to-HTML step or sanitizer. Citations are `RefRegistry` handles; the host opens the file at the cited line, and the webview never sends paths.

### 7.4 How copying works

The webview sends `{ sessionId, turnId, format }` (or selected record handles), and the host renders the authorized stored envelope and calls `vscode.env.clipboard.writeText`. Large text need not cross the webview message channel; the Explorer's existing `clipboard.write` message is capped at 512 KiB. Workbench copy is limited to **1 MiB of UTF-8** including framing. If too large, offer Copy selected or an explicit Save export action; never silently slice JSON, a citation or a code fence. Copy All follows the same rules, labels trimmed turns and keeps each turn's publication separate. External links/Markdown content are rendered as inert text; never enable command URIs from evidence.

---

## 8. Sessions

### 8.1 Data model

```ts
interface Session {
  schema: 1; id: string; project: string; title: string;      // title: first input, ≤ 80 characters
  createdAt: number; updatedAt: number; revision: number;
  turns: Array<{ id: string; at: number; input: string; plan?: Plan; format: 'structured' | 'json' | 'markdown';
                 selection?: SelectionProvenance; envelope?: ResultEnvelope; trimmed?: boolean }>;
}
```

### 8.2 Storage

**Location:** `globalStorageUri/workbench/<project-id>/<session-id>.json`.

- Not `workspaceState`: it is a key-value store unsuited to multi-megabyte data, and project ids are per machine, not per workspace.
- Never the source tree, `private/` or index state.

**Saving:**

- Save 750 ms after each turn completes, and again when the view is disposed.
- Write to a temp file, then rename it into place.
- Validate schema, project binding, file size and collection-relative references before loading. Quarantine corrupt files with bounded count/bytes; an unknown future schema is left untouched and reported as unsupported. Neither blocks new sessions or triggers execution. Corrupt artifacts participate in clear/remove cleanup.
- Report saving, saved and save failed honestly. Keep failed saves in memory with Retry and Export; disposal is best effort and cannot guarantee a final asynchronous write. Session IDs are host-generated, and storage uses guarded no-follow paths under the owned directory.

**Retention:**

- Keep the 10 sessions per project with the newest `updatedAt`; delete older files.
- The Recent list is built by scanning the directory. There is no separate index file to drift out of date.
- Each session holds at most **200 turns and 4 MiB of serialized UTF-8**. First remove oldest result envelopes (`trimmed: true`), then evict oldest turns if needed, retaining an omitted-turn count. Inputs/plans also have checked sizes; keeping them forever would defeat the cap. Offer Re-run for retained trimmed turns. Recent ten bounds normal session data to 40 MiB per project, with separate small bounded quarantine/temp allowances.

### 8.3 Behavior

- **Header:** a project picker, **Recent ▾** (titles and relative times) and **＋ New session**.
  - On first opening, suggest the registered project containing the active editor's file, otherwise the last one used in the window. Ask for a choice if no unique binding exists. After selection, editor focus does not switch the project mid-session.
  - The Workbench's selection never changes agent retrieval or the Explorer dropdown.
  - There will be a Copy All button for the session to copy all queries and results.
- **Old results:** every evidence turn keeps its publication. If the index has moved on, show *from an older index* with **Re-run**. If current readiness cannot be observed, say so instead of declaring the saved result current. Re-run reparses/reresolves with current versions and appends a turn, showing changed selections; never overwrite historical evidence.
- **Several windows:** default to window-owned sessions. Opening another window's session is read-only until ownership is acquired through a bounded session write lock and revision check, or it is forked. A plain read-check-rename is not sufficient concurrency control. Retention and clear use the same store lock and never silently delete a session with an active writer.
- **Privacy:** sessions contain source excerpts and stay on this machine.
  - `airk.workbench.clearSessions` deletes them.
  - Removing a project deletes its directory.
  - `extensionUninstall.ts` removes `workbench/`.
  - Setting `airk.workbench.persistResults: false` removes persisted envelopes from existing sessions as well as future saves; inputs/plans can still contain sensitive names and query text. Offer persistence off entirely for sensitive work, with explicit cleanup of saved sessions.
  - Sessions and exports are excluded from extension packaging, diagnostics and telemetry. No automatic import into an external agent or clipboard. Source excerpts remain untrusted content when copied to an agent.

---

## 9. Phases

| Phase | Delivers | Native changes | Size |
|---|---|---|---|
| **1** | Verify view contribution; sidebar, tier 0 commands, editable arguments, three formats, copy, cancellation, project picker and bounded sessions | `ClientFamily::Workbench`; existing reads only | UI/session foundation; commands useful independently |
| **2** | Discoverable `parse` prepare/execute for agents/CLI/UI, explicit multi-step plans, shared native runner, rules/model, safe resolution, definition/document recipes, selected evidence handoff and completeness contracts | Core contracts, retrieval classifier, kernel parser/runner and bounded folded/sub-token lookup; MCP/CLI adapters; dev-only training export | Substantial native contract and execution work; not merely parser polish |
| **3** | Impact/test/component recipes, explicit follow-up chips, recipe reuse, validated remembered inputs, schema-generated forms, selection command and optional Explorer route | Extend versioned recipe/selector catalog where needed; no new crate/service | Incremental workflows, each gated by measured value and bounded evidence quality |

Phase 1 is useful on its own. Plain English and agent batching start in phase 2. Phase 3 adds workflows after real use identifies gaps. Remembered inputs store accepted intent/parameters, keyed by project, scope and parser/catalog versions, then re-resolve against the current publication; never replay old evidence handles or bypass validation. No per-project model training or silent automatic learning.

For each phase, measure common tasks: calls, elapsed time, evidence/output bytes, manual corrections, missing relevant evidence and whether the user can open/copy a useful result. Compare direct tools, Workbench and an agent gathering the same evidence; do not equate fewer MCP round trips with less native work or billed savings. Gate phase 2 on correct ambiguity handling, native/UI parity, supported-platform fixtures and measured size targets, not classifier accuracy alone.

---

## 10. Tests

**Rust:**

- Tokenizer and rule goldens.
- Feature-hash goldens shared with Python.
- An inference golden: fixed input gives fixed integer logits. Exercise the small parser fixture on each routine native host within existing focused gates; broader qualification remains opt-in. Cross-platform equality is a target until those results exist.
- Resolution against a fixture index (exact, folded, sub-token, prefix, no match).
- Bindings for each selector and terse/full payload variant; atomic location/hash binding, no ambiguous-first selection, empty/dependent skip behavior and stable dedupe.
- `parse` discoverable with strict schema/help and correct public/native alias mapping; no editor dependency. Prepare/execute parity across CLI/MCP, explicit plans bypassing classification, edited/digest/version rejection and `needs_input` without evidence execution.
- Whole-plan validation before execution: scope escape via paths, symbol IDs, bindings and cursors; unknown operations/fields, recursion, cycles, oversized requests and exhausted budgets. No hidden widening, management calls, host reads or source writes.
- Native runner: partial failure, cancellation/late replies, all budget counters, retry permission, uncertain transport failure, missing/mixed publications, expired preparations and observation-only requests. Usage counts outer/inner work once.
- Catalog tools present in the evidence catalog.
- The 150-utterance accuracy floor.

**Extension:**

- Command grammar and contract rejection.
- `WORKBENCH_TOOLS` enforcement.
- The executor against a fake `open` (publication change, supersede, partial failure, empty-binding stop).
- Markdown/JSON fixtures: valid complete copy, original locations, upstream truncation propagation, retained-versus-visible counts, selected bundle provenance, escaping/backticks and oversized export refusal without invalid JSON.
- Session store: keep-10, UTF-8 caps including plan-only turns, quarantine limits, unsupported schemas, save failures, lock/revision conflicts, active-writer retention, privacy setting cleanup and removal/uninstall cleanup.
- Prototype interaction acceptance: keyboard/focus, narrow widths, theme/contrast, empty/unknown readiness, unresolved choices, plan edits, Stop, explicit context chips and stale-result Re-run preserving history.
- Container manifest/focus/restore test alongside `explorerParity.test.ts`; UI/native recipe parity and independent Explorer/Workbench sessions under shared worker admission.

**CI:** no new jobs or broad diagnostic matrix. Integrate focused parser/runner checks into existing workspace/extension suites and host gates according to [CI selection](../ci-and-builds.md). Add a routine focused gate only with evidence of its value; do not assume every platform currently runs the full workspace. Training/repository downloads remain offline developer work.

---

## 11. Documentation to update with each phase

- [VS Code extension](../vscode-extension.md): a new Workbench section.
- [Architecture](../architecture.md): parser, bounded native read composition, adapter/host boundaries and crate ownership in §3.1; no generation or agent orchestration.
- [Unified MCP](../unified-mcp.md) and [MCP contract](../mcp.md): discoverable `parse`, catalog count, strict prepare/execute schema, scope, budgets, partial results and agent examples; native alias compatibility.
- [Usage reporting](../usage-reporting.md): the `airk-workbench` client family.
- `tools/parser-train/README.md`: how to retrain.
- [Documentation index](../README.md): link this plan, and later the shipped guide.
- Root README, release notes and Marketplace text; version bump and lockfile sync, per CONTRIBUTING.md. These are implementation-phase obligations; this review changes only this proposed design and does not claim the feature shipped.

---

## 12. Decisions and implementation gates

1. **Usage attribution:** accept `airk-workbench` as a new client family (recommended), or reuse `airk-explorer`. **Decision:** airk-workbench.
2. **CLI exposure:** ship `airk parse` in the public CLI, or keep it dev-only behind a feature. **Decision:** include in public CLI.
3. **Thresholds:** retain the proposed accuracy floors and target ask rate, with calibrated abstention and accepted-plan correctness taking precedence (§5.5). Unsupported requests must remain unsupported rather than forced into a primitive intent.
4. **Name lookup:** how to add case-folded and sub-token symbol lookup. Either a bounded scan through a kernel service, or a stored folded-name column, which changes the storage format and needs a [storage format](../storage-formats.md) identity update. Measure the bounded scan on a large fixture first. **Decision:** do NOT change the storage format.
5. **NFC normalization:** whether to add `unicode-normalization` as a direct dependency (with a license-notice update), or ship with lowercase and whitespace normalization only. **Decision:** do NOT add NFC normalization unless there is compelling reason to do so and it can be done without backward version risk.
6. **Model attribution:** whether attribution for weights derived from the training repositories belongs in `THIRD_PARTY_NOTICES.md` or only in `TRAINING.md`. **Decision:** both.
7. **Agent access:** publish `parse` in the unified catalog, prepare by default, explicit execute mode, no editor attachment required. Retain direct evidence tools and actual caller usage attribution.
8. **Execution ownership:** shared bounded native runner in the kernel, with serializable core contracts and transport-only MCP/CLI adapters. No new crate, runtime service or generic workflow engine.
9. **Gates still requiring evidence:** view contribution compatibility, bounded folded/sub-token lookup latency on a large collection, model/header size, held-out calibration, UI/native recipe parity and cross-platform fixtures. If a lookup budget exhausts, return incomplete resolution rather than scanning without limit or claiming no match.

---

## 13. Risks

- **Held-out accuracy below the floor on first training,** most likely for the technical-writer role. Fix with more templates, not a bigger model.
- **Two containers titled "AIRK."** Tell them apart with the view name and tooltip ("AIRK Workbench").
- **Ambiguous names:** return labeled choices before evidence execution. A high intent score does not resolve which symbol the user meant.
- **"who calls" expectations.** People may read textual occurrences as call resolution. The chip wording and the warning in §4.2 must stay explicit.
- **Templates drifting from real phrasing:** use explicitly volunteered examples and reviewed fixtures; session content is not collected as telemetry or silently used for training.
- **Agent batching amplifying work:** enforce aggregate admission, fan-out and output limits in native code, account for actual inner work and retain partial results honestly. One MCP call is not one underlying read.
- **Publication churn:** long recipes may expire repeatedly while indexing is active. Keep them short, preserve partial-run identities and offer an explicit fresh run; never pause indexing or substitute revisions automatically.
- **Scope and duplication:** Workbench, Strategies and agent plans could become three incompatible languages. Share the closed operation/selector contracts and conformance fixtures, retain guarded host-only actions outside native plans, and expand only for the common workflows in §1.2.
