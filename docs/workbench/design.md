# AIRK Workbench implementation plan

**Status:** proposed, not implemented. Written 2026-10-29 against the 1.6.4 tree. Nothing in this plan is part of 1.6.4.

The AIRK Workbench is a second editor surface: a tab titled **AIRK** in the VS Code secondary sidebar. People use it to query an indexed project directly, with no agent. Input is either a slash command or a plain-English request. A fixed parser shipped in the native runtime turns plain English into a visible, editable plan of read-only AIRK tool calls. Results appear as Structured, JSON or Markdown, and each can be copied. Sessions save automatically, and the newest 10 per project can be reopened.

This plan replaces an earlier, uncommitted draft that used MiniLM intent ranking through the `airk-embed` helper and per-project training. That machinery is removed here; §1 says why.

---

## 1. Changes from the earlier draft

| Earlier draft | This plan | Reason |
|---|---|---|
| Three tiers: commands, rules, then MiniLM ranking through a new `rank_text` operation in `airk-embed` | Commands, plus one native `parse` request: rules, an embedded ≈ 0.85 MB linear intent model, and symbol/path lookups against the index | No dependency on vectors, `ort`, `tokenizers` or the embedding helper. Plain English works as soon as a project is indexed, including collections that opted out of vectors. |
| Per-project training and retraining | One fixed model, trained offline from hand-written phrasing templates filled with identifiers harvested from public repositories | The same input gives the same plan on every machine. The product ships no training code, accuracy thresholds or reset UI. |
| A learned slot tagger and a per-project vocabulary file | Rules extract paths, ranges, modes and quoted text; candidate identifiers are checked against the index | The index already is the vocabulary. There is no second artifact to build, version or invalidate. |
| Tool forms generated from JSON schemas in phase 1 | Moved to phase 3 | Commands cover the same ground and ship sooner. |

**Unchanged:** the secondary-sidebar placement; the transcript with editable plan chips; Structured, JSON and Markdown output with copy; auto-saved sessions (newest 10 per project); and reuse of `EvidenceClient`, `samePublication`, `RefRegistry` and the Explorer's webview security setup.

**Size budget:** at most 1 MB added to the `airk` binary and 300 KB to the VSIX, with no new runtime dependencies in either (see §4.3 step 1 on Unicode normalization).

---

## 2. Scope check

[Architecture](../architecture.md) says AIRK "does not reason, generate answers, manage agents, route model calls or require an external database", and that core stays free of parser, inference and transport code. This plan stays inside those limits:

- `parse` is a fixed linear classifier choosing among 13 known intents, plus rules and exact index lookups. It never generates text, summarizes or answers.
- Every plan is shown before or alongside its results, records which rule or lookup produced it, and can be edited.
- Execution uses only the 17 read-only evidence tools. The Workbench never calls management actions or the `draft_*` assistance tools, and it never writes source files.
- The new code goes in the crates that own each concern (§3.1). `airk-core` receives only serializable contracts.

The [architecture document](../architecture.md) must say this in one paragraph when phase 2 lands.

---

## 3. Architecture

```
Webview (Preact)                 Extension host                          Native (airk worker)
┌───────────────────────┐        ┌───────────────────────────┐   stdio   ┌───────────────────────────────┐
│ transcript, plan chips│ ─msgs─▶│ WorkbenchController       │ ─MCP────▶ │ airk_parse (not in tools/list)│
│ output toggle, copy   │        │  ├ CommandParser (tier 0) │           │  kernel: tokenize, rules,     │
│ session bar, project  │ ◀───── │  ├ parse via EvidenceClient│ ◀──────── │          resolve, plan        │
└───────────────────────┘        │  ├ PlanExecutor           │           │  retrieval: intent classifier │
                                 │  ├ Renderers              │           │ 17 read-only evidence tools   │
                                 │  └ SessionStore           │           └───────────────────────────────┘
                                 └───────────────────────────┘
```

### 3.1 Native side

Placement follows the [dependency boundaries](../architecture.md#dependency-boundaries):

| Crate | Additions |
|---|---|
| `airk-core` | `parse` contracts only: `ParseRequest`, `Plan`, `IntentId`, `StepBinding`, the feature-spec version. No tokenizer, inference or index access. Add `ClientFamily::Workbench` with the `airk-workbench` label in `usage.rs` (phase 1). |
| `airk-retrieval` | `intent/`: feature hashing, the quantized linear classifier and model-file validation. The model ships in `assets/intent-v1.bin` via `include_bytes!`. Pure Rust, no feature that pulls in `ort`, so the main `airk` dependency check in `scripts/build-service-runtime.py` still passes. |
| `airk-kernel` | `parse/`: `tokenize.rs`, `rules.rs`, `resolve.rs`, `plan.rs`, and the intent catalog (`intents.json`, hashed into `Plan.intent.catalog`). Resolution calls existing collection-scoped kernel services, pinned to one publication. |
| `airk-mcp` | `airk_parse` handling: the worker accepts it by name, but it is excluded from `catalog()` and `tools/list`. A test asserts it is absent from the 27-tool unified listing. Another test checks every tool named in `intents.json` against the evidence tool catalog. |
| `airk-cli` | Worker dispatch for `airk_parse`; the `airk parse "<text>" --project <id>` subcommand (exposure is open decision 2); the dev-only `dump-vocab` subcommand (§5.2), behind a non-default feature. |

**How `airk_parse` is reached:** the stdio bridge already forwards `call_tool` by name to the backend and handles `airk_help` specially by name (`crates/airk-mcp/src/remote.rs`). The worker can therefore accept a name that `tools/list` does not advertise. Agents can still call an unlisted name, so `airk_parse` must stay read-only, bounded and harmless to call. It is documented in [unified MCP](../unified-mcp.md) as "not an agent tool", not as a secret.

### 3.2 Extension side (`packages/vscode/src/`)

| Module | Role |
|---|---|
| `workbenchView.ts` | `WebviewViewProvider` for `airk.workbench`. Copies the Explorer's CSP (`default-src 'none'`, nonce scripts and styles), `localResourceRoots` limited to the bundle, and `enableCommandUris: false`, as in `explorerPanel.ts`. |
| `workbenchContracts.ts` | Versioned, validated message schema, built the same way as `parseWebviewMessage` in `explorerContracts.ts`. |
| `workbenchController.ts` | Project selection, message routing, superseding and cancelling requests. For plain English, calls `airk_parse`, then the executor. |
| `workbenchCommands.ts` | Tier 0 grammar for `/…` input (§4.6). Answers `/help` in-process with `HelpIndex`. |
| `workbenchExecutor.ts` | Runs a `Plan` within limits (§6), pinned to one publication via `samePublication`. |
| `workbenchRender.ts` | Pure JSON-envelope and Markdown renderers, covered by golden tests. |
| `workbenchSessions.ts` | Session store (§8). |
| `src/workbenchWebview/*.tsx` | UI. A new esbuild entry in `scripts/build-explorer.cjs` with its own `checkInputs` allowlist. |

The Workbench opens its **own `EvidenceClient`**. The native bridge allows 4 in-flight calls per connection, and the Explorer's client already keeps 3 outstanding, so sharing one connection would starve both. The client reports `clientName: 'airk-workbench'`. `EvidenceClient` restricts calls to `NATIVE_TOOLS`; add a separate `WORKBENCH_TOOLS = [...NATIVE_TOOLS, 'airk_parse']` allowlist rather than widening the Explorer's list.

### 3.3 Manifest

The installed Claude Code and Codex extensions use the `secondarySidebar` contribution key. AIRK's engine floor (`^1.140.0`) needs no fallback container.

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
  tier: 'command' | 'parsed';
  publication: string;                       // index snapshot used for resolution
  intent: { id: IntentId; logit: number; margin: number; model: string; catalog: string };
  alternatives?: IntentId[];                 // present only when the model was not confident; then steps is empty
  rule?: string;                             // id of the rule that decided, when a rule decided
  resolution?: { wanted: 'symbol' | 'path'; candidate: string; matched?: string; via?: 'exact' | 'folded' | 'subtokens' | 'prefix';
                 matches?: number; fallback?: 'search' };
  steps: Array<{ tool: WorkbenchTool; args: Record<string, unknown>; bind?: StepBinding }>;
}
```

`logit` and `margin` are integers in the model's fixed-point scale, not probabilities. See §4.4 on determinism.

### 4.2 Intent catalog

The 13 intents correspond one-to-one to the 13 commands in §4.6:

`search` (default), `find_definition`, `references`, `read_file`, `outline`, `list_files`, `file_info`, `imports`, `facts`, `changes`, `status`, `diagnostics`, `help`.

Each intent has a plan template in `crates/airk-kernel/assets/intents.json`. The file is shipped, versioned and hashed into `Plan.intent.catalog`:

```jsonc
{ "id": "find_definition", "needs": ["symbol"],
  "steps": [ { "tool": "airk_symbols", "args": { "name": "$symbol" } },
             { "tool": "airk_outline", "args": { "path": { "from": 1, "select": "first_definition_path" } } } ] }
```

**Bindings use named selectors, not JSON paths.** Tool payloads aren't uniform. For example, `airk_symbols` returns `items[].file` as an index into `files[]`; there is no `hits[].path`. Each selector is implemented and tested once per tool:

| Selector | Source tool | Meaning |
|---|---|---|
| `first_definition_path` | `airk_symbols` | `files[items[0].file].path` |
| `first_hit_path` | `airk_search` | `hits[0].citation.path` |
| `first_hit_range` | `airk_search` | `hits[0].citation.start_line..end_line` |

An empty source result stops the plan at that step and reports why.

**Two intents need care in wording:**

- **`references` finds text, not callers.** `airk_references` returns exact-case, whole-identifier textual occurrences (definitions, strings and comments included). It does not resolve calls. "who calls X" still maps here, but the chip reads "textual occurrences of X". AIRK must not claim resolved references.
- **`facts` depends on the file.** It maps to `airk_code_facts` or `airk_document_facts`, depending on the resolved path's language/profile from `airk_file_info`. With no path, it asks.

`help` has no native steps. The extension answers it with `HelpIndex`, which already includes the native engine guidance.

### 4.3 The `parse` pipeline (native, deterministic)

1. **Normalize.** Trim, collapse whitespace, Unicode-lowercase a copy for features, and keep the original for display. Full NFC normalization would add `unicode-normalization` as a direct dependency; it is only a transitive lock entry today. Skip NFC unless measured inputs need it (open decision 5).
2. **Tokenize.** Split on whitespace and punctuation except inside quoted strings. Keep tokens intact if they contain `/`, `::` or an internal `.`, or end in a known extension.
3. **Extract with rules.**
   - Quoted strings become exact terms.
   - Path-like tokens become `path`.
   - `:a-b` or `:a` becomes a line range.
   - `lexical|semantic|vector|hybrid|exact` becomes `mode`.
   - An integer after `top|limit|first` becomes `limit`.
   - CamelCase, snake_case, and `::`- or `.`-qualified tokens become `identifiers[]`.
4. **Choose the intent.**
   - A command word typed without the slash (`refs Foo`) is decided by a rule before the model runs.
   - Otherwise the model scores the input (§4.4). The top intent is accepted if `logit ≥ τ` and `logit − second ≥ μ`.
   - If not accepted, the plan returns the top three `alternatives` and no steps.
5. **Resolve.** Lookups use the publication pinned for this request.
   - **Symbols:** check `identifiers[]` in order — exact name, then case-folded, then sub-token set (`evidence client` ↔ `EvidenceClient`).
   - **Paths:** check `path` against the file inventory, exact first, then as a prefix.
   - **Outcome:** one match fills the template. Several fill it with the first and record `matches`. None downgrades the plan to `search` with the original text and records `fallback: 'search'`.
   - **Native work needed:** exact symbol lookup exists today (the `symbols` path in `airk-mcp`). No case-folded or sub-token lookup was found in kernel or store. Phase 2 must add a bounded kernel query for them (open decision 4).
6. **Emit** the `Plan`, recording every decision: the rule that fired, the resolution step that matched, and the publication. The UI can then explain the plan, and tests can pin it.

Rules and resolution are pure functions of the token stream and the pinned index snapshot. The model only chooses among the 13 fixed intents.

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
  - Acceptance (`τ`, `μ`) and ranking use only these integers, so they are bit-identical on Windows, macOS and Linux.
  - Probabilities are not part of the plan or of any golden output. Rust's `f32::exp` is not guaranteed bit-identical across platforms. If the UI shows a confidence bar, it derives one from `logit`/`margin` for display only.
- **File format:** a 32-byte header (magic, schema version, bucket count, intent count, feature-spec version, catalog digest, `τ`, `μ`) followed by the weights.
  - The file is validated once per worker.
  - A mismatched catalog digest or feature-spec version is a load error. It is never accepted silently.

### 4.5 Why no slot tagger

Paths, ranges, modes and quotes are unambiguous from their syntax. The only ambiguous question is whether a span is a symbol or just words, and the index answers that exactly for this project. If evaluation later finds inputs the rules miss, a tagger can be added behind the same `parse` contract.

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
| `/changes` | `changes` | `airk_changes` |
| `/status` | `status` | `airk_status` |
| `/diagnostics` | `diagnostics` | `airk_diagnostics` |
| `/help [topic]` | `help` | none (in-process `HelpIndex`) |

`airk_context`, `airk_capabilities`, `airk_relationships` and native `airk_help` have no command. They become reachable through the phase 3 schema-generated forms.

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
  tester:    ["which tests touch {symbol}", "what exercises {symbol}"]
  architect: ["what depends on {symbol}", "fan-in of {symbol}"]
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

**Thresholds:** choose `τ` and `μ` to maximize accuracy among accepted plans while keeping the "ask" rate at or below 10%.

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

- **Limits:** at most 6 steps, 12 tool calls, 20 seconds and 256 KiB of combined payload. Each step passes through the person's `limit`/`max_bytes`.
- **One snapshot:** the publication from `parse` (or from the first step, for commands) is pinned. Every later step goes through `samePublication`. If the index changes mid-plan, the result is marked *publication changed* and offers **Re-run** instead of combining snapshots.
- **Cancellation:** each session has its own supersede key, so a new submission cancels the queued one.
- **Errors and retries:** reuse the `explorerError` codes and `readRetryDelay` handling. A failed step keeps its error in the result and is never dropped.
- **Read-only:** only `WORKBENCH_TOOLS` can run. Plans arriving from `parse` are validated against it in the extension, too.

---

## 7. Output formats and copy

The result header has a **Structured | JSON | Markdown** toggle. **⧉ Copy** copies exactly the format shown. A session menu adds "Copy session as Markdown" and "Copy session as JSON".

### 7.1 JSON envelope

Tool payloads appear exactly as returned, never rewritten. Keys are sorted so output bytes are stable.

```json
{
  "schema": "airk.workbench.result/1",
  "project": "54c6…", "publication": "airk-r1:…",
  "input": "who calls EvidenceClient",
  "plan": { "tier": "parsed", "intent": { "id": "references", "…": "…" }, "steps": [ "…" ] },
  "steps": [ { "tool": "airk_references", "arguments": { "name": "EvidenceClient" }, "elapsed_ms": 41, "result": { "…": "verbatim" } } ],
  "warnings": [ "Textual occurrences, not resolved callers." ],
  "truncated": false
}
```

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

The webview sends `{ sessionId, turnId, format }`, and the host renders from the stored envelope and calls `vscode.env.clipboard.writeText`. Large text therefore never crosses the webview message channel; the Explorer's `clipboard.write` message is capped at 256 KiB. The Workbench copies up to **1 MiB**. Beyond that it copies the first 1 MiB and shows a notice.

---

## 8. Sessions

### 8.1 Data model

```ts
interface Session {
  schema: 1; id: string; project: string; title: string;      // title: first input, ≤ 80 characters
  createdAt: number; updatedAt: number; revision: number;
  turns: Array<{ id: string; at: number; input: string; plan: Plan; format: 'structured' | 'json' | 'markdown';
                 envelope?: ResultEnvelope; trimmed?: boolean }>;
}
```

### 8.2 Storage

**Location:** `globalStorageUri/workbench/<project-id>/<session-id>.json`.

- Not `workspaceState`: it is a key-value store unsuited to multi-megabyte data, and project ids are per machine, not per workspace.
- Never the source tree, `private/` or index state.

**Saving:**

- Save 750 ms after each turn completes, and again when the view is disposed.
- Write to a temp file, then rename it into place.
- A file with unknown schema or bad content is renamed to `*.corrupt` and skipped; it never blocks the view.

**Retention:**

- Keep the 10 sessions per project with the newest `updatedAt`; delete older files.
- The Recent list is built by scanning the directory. There is no separate index file to drift out of date.
- Each session holds at most **200 turns or 4 MiB**. Beyond that, the oldest turns keep only their input and plan (`trimmed: true`) and offer **Re-run**.

### 8.3 Behavior

- **Header:** a project picker, **Recent ▾** (titles and relative times) and **＋ New session**.
  - The project defaults to the registered project containing the active editor's file, otherwise the last one used in the window.
  - The Workbench's selection never changes agent retrieval or the Explorer dropdown.
- **Old results:** every turn keeps its publication. If the index has moved on, the turn shows a *from an older index* badge with **Re-run**. Old evidence is never shown as current.
- **Several windows:** each window writes only sessions it created or opened. A changed `revision` on disk means reload, or fork into a new session. A window never overwrites another window's newer revision.
- **Privacy:** sessions contain source excerpts and stay on this machine.
  - `airk.workbench.clearSessions` deletes them.
  - Removing a project deletes its directory.
  - `extensionUninstall.ts` removes `workbench/`.
  - Setting `airk.workbench.persistResults: false` stores only inputs and plans.

---

## 9. Phases

| Phase | Delivers | Native changes | Size |
|---|---|---|---|
| **1** | Secondary-sidebar view, tier 0 commands, Structured/JSON/Markdown and copy, sessions (auto-save, recent 10, new), project picker | `ClientFamily::Workbench` | Largest (UI and session store) |
| **2** | `airk_parse` and CLI subcommand, rules, embedded intent model, index resolution, editable plan chips, intent buttons when the model isn't sure, training pipeline and CI check | Contracts in `airk-core`; classifier in `airk-retrieval`; parse and resolution in `airk-kernel`, including the new folded/sub-token name lookup; `airk-mcp`/`airk-cli` adapters; dev-only `dump-vocab` | Medium |
| **3** | Remembered inputs per project (normalized input → accepted plan, checked before `parse`), schema-generated tool forms, "Search in AIRK Workbench" editor command, optional Explorer route | None | Small |

Phase 1 is useful on its own. Plain English starts working in phase 2. Phase 3 is polish.

---

## 10. Tests

**Rust:**

- Tokenizer and rule goldens.
- Feature-hash goldens shared with Python.
- An inference golden: fixed input gives fixed integer logits, byte-equal on all three operating systems in routine CI.
- Resolution against a fixture index (exact, folded, sub-token, prefix, no match).
- Bindings for each selector.
- `airk_parse` absent from `tools/list`.
- Catalog tools present in the evidence catalog.
- The 150-utterance accuracy floor.

**Extension:**

- Command grammar and contract rejection.
- `WORKBENCH_TOOLS` enforcement.
- The executor against a fake `open` (publication change, supersede, partial failure, empty-binding stop).
- Markdown and JSON golden fixtures.
- Session store: keep-10, atomic write, corrupt-file recovery, revision conflict, trimming.
- A manifest check for the new container, alongside `explorerParity.test.ts`.

**CI:** no new jobs. Everything runs in the existing workspace and extension suites.

---

## 11. Documentation to update with each phase

- [VS Code extension](../vscode-extension.md): a new Workbench section.
- [Architecture](../architecture.md): one paragraph on `parse` (rules, a fixed classifier over a versioned catalog, and index resolution, with no generation) and the crate ownership in §3.1.
- [Unified MCP](../unified-mcp.md): `airk_parse` is unlisted and not an agent tool.
- [Usage reporting](../usage-reporting.md): the `airk-workbench` client family.
- `tools/parser-train/README.md`: how to retrain.
- [Documentation index](../README.md): link this plan, and later the shipped guide.
- Root README, release notes and Marketplace text; version bump and lockfile sync, per CONTRIBUTING.md.

---

## 12. Open decisions

1. **Usage attribution:** accept `airk-workbench` as a new client family (recommended), or reuse `airk-explorer`.
2. **CLI exposure:** ship `airk parse` in the public CLI, or keep it dev-only behind a feature.
3. **Thresholds:** the "ask" rate (10% proposed) and the accuracy floors in §5.5.
4. **Name lookup:** how to add case-folded and sub-token symbol lookup. Either a bounded scan through a kernel service, or a stored folded-name column, which changes the storage format and needs a [storage format](../storage-formats.md) identity update. Measure the bounded scan on a large fixture first.
5. **NFC normalization:** whether to add `unicode-normalization` as a direct dependency (with a license-notice update), or ship with lowercase and whitespace normalization only.
6. **Model attribution:** whether attribution for weights derived from the training repositories belongs in `THIRD_PARTY_NOTICES.md` or only in `TRAINING.md`.

---

## 13. Risks

- **Held-out accuracy below the floor on first training,** most likely for the technical-writer role. Fix with more templates, not a bigger model.
- **Two containers titled "AIRK."** Tell them apart with the view name and tooltip ("AIRK Workbench").
- **Ambiguous names** (several symbols with the same name). The first match is used, the count is shown, and the chip lets the person pick another.
- **"who calls" expectations.** People may read textual occurrences as call resolution. The chip wording and the warning in §4.2 must stay explicit.
- **Templates drifting from real phrasing.** Remembered inputs in phase 3 give a cheap signal for what to add next.
- **Agents calling the unlisted `airk_parse`.** Harmless by design (read-only and bounded), but its usage is counted under the caller's own client family.
