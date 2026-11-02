# Two-tier indexing and reply detail: live AIRK MCP evaluation

**Date:** 2 November 2026. **Scope:** the live AIRK MCP connection bound to this
repository, plus current source inspection. This is research and a proposal;
no source corpus, indexing policy or runtime implementation was changed.

## Recommendation

**Agent Lean supplies useful evidence for many common coding investigations, but
this run does not qualify it as sufficient for every common harness workflow.**
Definitions, complete signatures, imports, textual references, selected
requirements, native DOCX table rows and exact reads worked in this project.
Two important limitations emerged:

1. **A real compact-response consumer failure:** a valid chained plan with compact
   symbol discovery skipped its dependent step because it could not reconstruct
   cited locations from the shared file table. The same plan using rich records
   completed. Increasing index detail would not fix that consumer.
2. **Gaps in retained evidence:** root `Cargo.toml` had zero retained facts,
   including no structured package/workspace/dependency inventory. Standalone
   date facts and separate DOCX cell facts were unavailable even with rich
   replies. Exact text and DOCX row evidence remained readable. A response flag
   cannot create fact families discarded during indexing.

Recommend **two public indexing profiles, Lean and Rich, selected through
file-aware adapter policies**, and **two independent reply choices, Lean and
Rich**, shared by MCP, CLI and Workbench. MCP and CLI should default to Lean
replies; Workbench should default to Rich, with Lean available. Workbench already
explicitly requests rich facts/full metadata through its canonical read handler,
so that part is consolidation rather than a new ability. [S1] [S2] [S3]

Use one collection/publication with a common evidence core and optional rich
enrichment under existing storage, job, lease and quota ownership. Do not create
two independently mutable indexes. A rich read must disclose unavailable detail;
it must not silently index, enable providers, broaden source scope or substitute
changed live files.

## Live test design

### Runtime and revision

| Observation | Value |
| --- | --- |
| Runtime version / platform | AIRK 1.8.7 / Windows x86-64 |
| Published evidence level | `agent_lean` |
| Indexed files / citation chunks | 1,416 / 5,337 |
| Indexed vectors | 34,058 |
| Semantic coverage | 1,414 selected artifacts, 1,414 completed; ready |
| Lexical / vector backends | `sqlite-fts5-current` / Microsoft DiskANN current provider |
| Configured query concurrency | 16 |
| Imported facts / relationships | 0 / 0 |
| Successful evidence-read publication generation | 5887 throughout the battery |

Exact token:

```text
airk-r1:19ae18e4a4de52ae6554dac618c4c032:5887:b631e4203b5150e021bacca1c3acb1702a6ebc87ad18ea8ef129972bb2084bcd
```

Source was reviewed at Git HEAD
`0dbd2f0729d158a73686ce726a424555a9bbb3f8`, preserving existing changes.
Selected live file hashes matched indexed hashes. Matching version numbers alone
do not prove the installed runtime was built from exactly this worktree; the
service diagnostic build source digest was
`84b59dcb4af4ed224b09f72a27ab67e841d05268b125dc09dafd631b8cb802f3`.

Freshness and readiness stayed distinct: a late relationship read reported
`service_freshness_observation_unavailable` while returning the same publication;
a subsequent search reported idle freshness. Indexed replies were not treated
as independent proof that live files were unchanged.

### Battery and receipts

**97 recorded live calls exercised 17 tools:** `status`, `diagnostics`,
`capabilities`, `file_info`, `list_files`, `search`, `symbols`, `code_facts`,
`imports`, `outline`, `references`, `relationships`, `document_facts`, `read_file`,
`context`, `changes` and `parse`. Preflight status/schema discovery was additional.
Draft, UI mutation and management tools were not exercised.

Discovery was followed by selected expansion, source reads and live-file checks.
Independent probes were batched at up to four concurrent calls; dependent reads
and typed retries followed actual results. The published Agent Lean collection
was not intentionally modified or rebuilt. Background reconciliation continued,
but successful reads retained the same publication.

Full requests, returned envelopes and elapsed observations are saved locally in
ignored `target/two-tier-indexing-live.json`; the audit summary is
`target/two-tier-indexing-audit.json`. The receipt SHA-256 is:

```text
b85b003beb86ea98eb8622d26739af69215d18a3bbafd4c45b9342f7d7e36d80
```

These local receipts are not proposed for publication and are not required to
read this report. Test IDs below identify their entries. This is a real-project
probe set, not complete patch-writing sessions across multiple Codex/Claude
models, paired Rich/Lean index builds, or an indexing cost benchmark.

### Errors were retained

- **11 attempts returned typed `publication_busy` receipts.** Each was retried
  once as instructed. Nine retries returned evidence; two reached the intended
  bad-hash/bad-revision rejection. This sample is not a general failure-rate or
  concurrency benchmark.
- **Four guard rejections** were expected: wrong source hash, a syntactically
  valid but mismatched revision generation, a cursor reused with another
  representation, and an escaping source path.
- **Two plan attempts exposed schema friction:** the discovered declaration
  represented `plan` as a string, but the native handler required an object.
  Sending an object worked. The native field is an untyped JSON value; investigate
  the schema/connector boundary before attributing this entirely to the server.
  [S4]
- **Two investigator-authored plans used an unsupported binding destination.**
  `unique_definition_path` can feed `outline`/`file_info`, not a direct
  `read_file` path. Those requests were corrected; they are not product failures.
- The important valid-plan failure returned MCP success with **`status:partial`**
  and a skipped dependent step. Transport success was not counted as task success.

There were 19 MCP error envelopes among 97 attempts. That number mixes retries,
intentional negatives, schema friction and invalid test inputs; it is not a task
pass/fail statistic.

## Results for common coding tasks

| Task / probes | Observed result | Is Agent Lean sufficient here? |
| --- | --- | --- |
| Readiness/formats: D01, C01/C02, Z01 | Published Agent Lean and ready vectors were observable; capabilities were separate from collection readiness. | Yes for orientation. Full diagnostics should be exceptional. |
| Known declaration: S01-S05 | `IndexDetail` enum and impl returned complete signatures, locations and shared source hashes. Compact/rich identities agreed. | Yes. Shared file bindings are essential evidence. |
| Function explanation: K01/K02, E02/E02.r1 | Located `search_current_read_with_filters`, full signature and two call occurrences; context expanded rank fusion. | Yes through targeted expansion; discovery does not replace a body read. |
| Module structure: I01, O01/O02 | Imports and ordered definitions were available. First outline page contained imports; next reached constants. | Yes with pagination; first page is not the whole file. |
| API-change references: R01/R02 | Two pages returned 11 matching core-source lines for `IndexDetail`; live files had the same 11. Response still disclosed `token_boundary_complete:false`. | Useful textual impact evidence, not resolved-reference completeness. |
| Callers/dependencies: G01/G01.r1, G02/G03 | Outgoing calls truncated at 12. Unqualified and qualified incoming spellings each found one caller path; union covered both inspected callsites. | Useful bounded syntax investigation; one spelling was insufficient for both callers. |
| Find tests: Q05/Q08 | Located detail tests and exact `metadata_filtering_precedes_both_branch_candidate_limits` test in `search_filters.rs`. | Yes for discovery, not executed-test coverage. |
| Editor code: T01/T02/T04, O03 | Lexical search found the four-choice picker, with symbols/calls and exact reads. `prepareConfiguration` was a type-property string here; empty call queries were appropriate and references found it. | Yes for this TS surface; text fallback remains important. |
| Packaging scripts: P01/P02/P03 | Python `main` was found, expanded and checked against live source. | Yes for the sampled script, not all Python constructs. |
| Configuration: D04/D07/D12, D08, E05 | Cargo had zero retained facts; rich reads could not add them. JSON package version key survived. Cargo text remained readable. | Partly: exact answers possible; structured manifest inventory is a gap. |
| Requirements: D02/D03, D13/D14/D16 | Markdown/DOCX candidates available; rich expansion added modality/quantities without reindex. Some DOCX title/date/caption text also classified as requirement candidates. | Useful with heuristic labels and source review; not normative truth. |
| Tables: D11, D15/D17, E07 | Markdown table location available. DOCX lean rows retained cells/headers/native coordinates; separate cell facts empty. Exact row read worked. | Often enough for row questions; separate cell inventory needs richer content. |
| Conceptual discovery: Q02-Q04 then T01/E02 | Related evidence returned, but UI-detail query led with help prose rather than picker code. Lexical fallback found implementation. | Mixed ranking/task routing; richer indexing is not a demonstrated cure. |
| Safe evidence reuse: E01/E03/E04, H01/H02, N02/N03 | `not_modified` suppressed repeated hits; Terse/Full validators agreed; citation validated unchanged; bad hash/revision rejected. | Yes for current-publication probes; no edit/rename/reindex experiment. |
| Chained compact discovery: W04/W05, W08/W09 | Compact step retained symbols but aggregation lost evidence and binding failed. Rich equivalent completed. | No for this live consumer; blocks blanket lean-reply qualification. |

**Agent Lean is a viable common-task baseline when the harness understands shared
bindings, finishes pages, reads source, observes coverage and can request
expansion.** It is not a promise that every structured query, consumer or file
family works without fallback.

## Index detail versus reply size

All pairs below used the **same Agent Lean index**. Bytes measure UTF-8
serialization of returned tool envelopes observed by this connector, excluding
request/schema overhead. These are not tokenizer, provider-context or billing
measurements. Include-status choices and budgets matched within the fact pairs;
timestamps/request identities were not stripped from envelope counts.

| Matched pair | Compact/Terse bytes | Rich/Full bytes | Reduction | Preserved evidence |
| --- | ---: | ---: | ---: | --- |
| Two symbols, Terse metadata (S01/S02) | 2,134 | 3,441 | 38.0% | Names, kinds, complete signatures, locations and source identity. |
| Two symbols, Full metadata (S03/S04) | 2,689 | 4,123 | 34.8% | Same declarations; Full did not cancel compact selection. |
| Two Markdown requirements, Terse (D02/D03) | 3,589 | 4,323 | 17.0% | Statements, headings/citations; rich adds modality/quantities. |
| Three DOCX requirements, Terse (D13/D14) | 3,510 | 4,182 | 16.1% | Statements/original locations; rich adds document metadata. |
| Same exact read, Terse/Full metadata (E01/E04) | 3,485 | 4,328 | 19.5% | Exact text, citation and validator; envelope savings only. |

The two-symbol pair charged **593 compact versus 1,951 rich record/binding bytes**.
At `max_bytes:1`, no records were admitted and `required_record_bytes:494` plus
truncation explained why; 600 bytes fit both records. The complete envelope was
larger because framing/status are additional. A 64-byte search text budget
produced a 1,911-byte envelope while respecting the 64-byte hit-text allowance.
These are distinct budget domains, not a violated text budget.

W04/W05 would be a misleading savings comparison: single-step compact plan output
was 7,944 bytes versus 16,362 rich, but aggregated evidence fell from two records
to zero. That 51.4% reduction is **not functionally equivalent output**.

Also, `full_chunks:true` returned one hit where focused snippets returned two
under the same 4,096-byte text budget (Q06/Q07). A Rich reply should not blindly
turn search into full chunks; more text can reduce breadth.

## Why the four levels feel complicated

There are three independent decisions in current contracts. [S1] [S3] [S5] [S6]

| Decision | Current control | Effect |
| --- | --- | --- |
| Retained evidence | `essential`, `agent_lean`, `balanced`, `detailed` | Changes extraction/retention policy; needs successful indexing publication. |
| Record projection | `compact`, minimal navigation, focused/full chunks | Returns less/more of stored evidence; no reindex needed. |
| Metadata encoding/repetition | `response_mode:terse/full`, `include_status` | Shares provenance, abbreviates envelopes, suppresses repeated observations; cannot restore omitted facts. |

`symbols`/`outline`/`imports` normally return minimal records; `code_facts` and
`document_facts` derive their default compact choice from applied index detail.
This couples storage policy to reply defaults unnecessarily. Workbench already
overrides it with rich/full reads.

The four levels are not relevance settings. Essential removes native structured
facts but retains text and eligible semantics. Agent Lean retains an allowlist
and omits continuity fingerprints. Balanced adds broader facts and declaration
continuity. Detailed also supports document-unit continuity where implemented.
Neither richer indexing nor richer output creates compiler resolution or OCR.

Existing research explicitly separates index costs from reply costs and leaves
paired real-agent qualification outstanding. Its old corpus cost numbers remain
historical, not measurements from this run. [S7]

## File-aware policies: use profiles, not extensions alone

**Use the effective adapter/profile and evidence family, with reviewed path or
basename overrides, rather than one global fact-kind allowlist for every file.**
The live Cargo/JSON difference illustrates why.

Root Cargo is classified `project-manifest`, not generic TOML. That producer emits
`workspace_configuration`, `package_dependency` and related kinds. Agent Lean's
allowlist retains `key` but not those kinds; this file consequently had zero
facts. The JSON package file's generic key evidence survived. Saying that Lean
retains configuration keys does not adequately explain this to a harness. [S1]
[S8]

Two other observations refine the proposal:

- The sampled Markdown adapter emits a table-location fact, not DOCX-style row
  records. Empty Markdown `table_row` queries did not establish discarded row
  evidence. DOCX did retain rows with structured cells, while separate cell-fact
  expansion was omitted. Distinguish **not produced by this adapter** from **not
  retained by policy**. [S9] [S10]
- Release notes contain explicit dates readable through `read_file`, but date-fact
  queries over releases were empty. Source review explains that Agent Lean omits
  standalone dates. A small date/version family for release/review tasks may be
  useful; decide using tasks/costs, not a universal prohibition. [S1]

Markdown can contain instructions, designs, release records or fenced code. JSON
can be a manifest, schema or dataset. Composite hosts contain mapped languages.
Extensions help select a qualified adapter; they do not establish understanding
or source authority.

### Proposed evidence by family

This is a **new proposal**, not today's Lean behavior. Keep two public profiles
with versioned adapter-owned selection behind them.

| Effective family | Lean common core | Rich additions | Boundaries |
| --- | --- | --- | --- |
| Source code/tests | Definitions, complete signatures, imports/ownership, supported calls, original locations, parse quality. | Broader framework/type facts, expensive documentation/attributes, declaration continuity/fingerprints. | Syntax candidates remain candidates; no agent reasoning or implied resolution. |
| Build/manifests/config | Written package identity, dependencies, scripts, workspace membership, relevant paths/keys and values. | Wider schema/framework inventories and structure tracking. | Fix manifest-family omissions; bound large inventories and preserve text. Never execute config. |
| Markdown/text/instructions/releases | Headings, useful requirement/decision/acceptance and applicable date/version cues, source text; modality/bounds when needed for interpretation. | Wider section/entity/date inventories and supported unit continuity. | Historical/proposed prose stays identifiable; heuristics are candidates. |
| DOCX/PDF/PPTX prose | Native page/block/slide locations, extraction quality, useful prose facts and text. | Broader inventories/document-unit continuity. | Conversion cost remains; no automatic OCR or layout completeness promise. |
| XLSX/CSV/TSV/DOCX tables | Native coordinates, headers, typed values and formula/cached/hidden/merge qualifications needed to interpret a row. | Separate cell inventories, wider traversal and supported continuity. | Lean must retain interpretation-critical metadata; capabilities remain adapter-specific. |
| Composite hosts/notebooks | Host/native coordinates and qualified child evidence selected by child profile. | Broader host/child structure and continuity. | Preserve mapping/region boundaries; do not flatten unrelated coordinates. |
| Lockfiles/minified/generated material | Eligible exact text/citations and explicit eligibility reasons. | Useful supported structure when selected. | Vectors are separate; do not automatically discard tests/vendor/generated paths. |

PDF, PPTX, XLSX, CSV/TSV and composite-host recommendations are source-informed,
not live qualification from this battery. Rust, TypeScript, Python, Markdown,
TOML/project-manifest, JSON and DOCX were exercised directly.

## Recommended two-tier contract

### Indexing: common core plus optional rich enrichment

Expose **Lean** and **Rich** as the new everyday indexing profiles. Lean retains
file-aware common-task evidence; Rich retains the same core plus supported optional
families. Rich must not remove baseline evidence or alter source authority.

Keep one collection. Source/profile changes publish through current admission;
enrichment binds the same captured source, extraction profile, model/producer
identity and storage ownership. It must not become a separate mutable database.

Report configured choice, effective published families, unsupported/omitted/failed
families, digests and pending work for a selected file/profile. A single global
`indexing.level` is insufficient once files have different effective policies.
Use bounded aggregates in status, not a full catalog repeated on every hit.

Two storage layouts deserve evaluation:

1. **One selected payload per file/profile:** simplest reuse of current extraction
   and publication. Only affected effective-profile changes require reprocessing.
2. **Common payload plus separately addressable rich objects under one publication:**
   may improve upgrades/reads but adds ownership, integrity, quota and reclamation
   work. Use existing object/page mechanisms, not another state authority.

Start with the first. Keep dependency direction: Core owns contracts; Ingest owns
adapter policy/early construction; Store owns retained objects/publication; Kernel
owns scoped selection/budgets; interface adapters own presentation. [S11] [S12]

### Replies: independent choices on the same tools

A proposed shared input such as `reply_detail: lean | rich` selects an adapter-aware
projection of **available** evidence. This is illustrative, not a current argument.
A proposed CLI spelling could be `--reply-detail`; persisted indexing remains a
separate native management choice.

| Surface | Default | Alternative |
| --- | --- | --- |
| MCP reads for coding harnesses | Lean | Rich, with selected expansion. |
| CLI convenience reads, `tools call` and shell | Lean | Same Rich input semantics and canonical result. |
| Workbench forms/transcript | Rich | Lean as a session/tool choice, retained in copies/exports. |

Defaults must not depend on the collection's indexing profile. Two clients can
choose different replies from the same publication without modifying it. Keep
`response_mode` for metadata/audit compatibility; avoid another everyday selector.
Preserve legacy `compact`, `full_chunks` and response flags during migration;
reject conflicting new/old arguments rather than guessing precedence.

Both replies retain scope, hashes, original locations, exact token, validators,
basis/provenance, omissions/coverage, continuation, retries and truncation needed
to interpret evidence. Shared file/provenance tables are valid only when every
consumer reconstructs equivalent meaning. A small definition can omit repeated
scopes/modifiers but must still support the next exact read or relationship
investigation using a revision-bound locator or explicit expansion.

Rich means more available fields, not more hits, larger budgets, automatic full
chunks, live-source reads, external analysis or indexing work. Focused snippets
remain useful in Rich search; full chunks should stay an explicit choice.

### Missing detail must be actionable

Distinguish these states for a requested family:

- available and returned;
- indexed but omitted by this reply projection;
- not indexed under current policy;
- unsupported/not applicable to this adapter;
- pending, failed, stale or budget-truncated.

A Rich request for an omitted DOCX cell inventory should identify unavailable
families and return usable requested row/text evidence, rather than only empty
`items`. A follow-up may suggest a costed native policy/enrichment operation, but
execution uses the existing explicit management path. Empty output never proves
no cells, dependencies, dates or callers exist.

## Implementation options

| Option | Benefits | Costs/limits | Assessment |
| --- | --- | --- | --- |
| A. Two visible presets over current policies; unify replies first | Smallest change; keeps legacy values and fixes consumer compatibility before indexing changes. | Does not repair the global allowlist or add effective family coverage. | Useful first delivery if limits are explicit. |
| B. Two versioned file-aware profiles plus independent replies | Fits common-task needs, native format differences and requested defaults. | Requires adapter inventory, effective coverage, migration/cost qualification. | Recommended target, after A's correctness work. |
| C. Lean core plus explicitly requested native enrichment jobs | Avoids rich cost for rarely examined files. | More job/receipt/captured-source lifecycle work; first rich evidence may wait. Reads still cannot secretly mutate. | Defer until measured costs justify it. |

Do not replace four names with Lean, Rich, Advanced and Detailed. Prefer **Rich**
for the second everyday tier; “advanced” describes management controls. Rich
alone must not imply language-server resolution, imported analysis or OCR.

### Compatibility and migration

A label-only step can group Balanced/Detailed under Rich for navigation, but
cannot equate their stored semantics. Keep `essential`, `agent_lean`, `balanced`
and `detailed` readable with their original behavior until explicit migration:

- Search Only/Essential must not silently acquire structure or trigger indexing
  because a UI now offers two names. Identify the legacy policy honestly.
- Balanced declaration continuity must not silently become Detailed document
  continuity. Show effective families for legacy Rich collections.
- New Lean retaining additional manifest facts changes extraction identity; it is
  not merely a label rename.
- Preserve adapter overrides, source roots, pause/manual preferences, quotas,
  provider bindings and explicit vector policies.
- Distinguish configured/published policy and pending work. Failure leaves current
  evidence readable under its original contract.

A versioned extraction policy must handle mixed profiles deterministically.
Current selection applies collection detail; per-adapter family selection needs
intentional changes to profile identity, reuse, inventory and reporting. [S6]
[S11]

## Phased implementation plan and acceptance evidence

### Phase 0: repair consumers and establish the baseline

1. Reproduce W08/W09 through actual MCP `parse`. Teach the canonical location/
   evidence reader to reconstruct minimal `items` from `files` and original
   locations, preserving ambiguity, truncation and exact revision checks. The
   current `locations` walker accepts objects with `citation`, but does not
   reconstruct minimal rows. [S2]
2. Cover `unique_definition_path`, `hit_locations`, `selected_symbol_ids`,
   aggregation, cards and exports. Rows without IDs need a supported expansion/
   locator path, not invented IDs. Renderer mocks alone are insufficient.
3. Tighten nested-plan schema exposure so native schemas and actual connector
   declarations agree on object/string inputs.
4. Convert appropriate audit invariants into regressions and add explicit task
   assertions for the compact-plan failure. Reproducing a failure is not passing
   qualification of the affected workflow.

**Gate:** compact discovery feeds the same useful dependent reads as rich, with
equivalent cited evidence surviving aggregation. No semantic omissions are hidden
just to obtain smaller payloads.

### Phase 1: unify Lean/Rich replies across surfaces

Add one canonical projection policy, explicit per-call selection and legacy
argument compatibility. Audit CLI convenience reads, `tools call`, shell, MCP
aliases, parse/recipes, Workbench forms/defaults, structured details, JSON/Markdown
copies, exports and session restoration. Workbench remains Rich by default;
MCP/CLI default Lean. [S3] [S5] [S13]

**Gate:** native handlers and actual CLI/MCP transports compare complete results
and input catalogs for the same explicit choice. Differences are limited to
documented transport identity/observation timing. Preserve scope, omissions,
partial coverage, retries and source bindings.

### Phase 2: implement adapter-owned common evidence

Inventory produced kinds and costs, starting with Rust/TS/Python, project
manifests, Markdown and DOCX. Replace the general allowlist with a small versioned
family map. Retain useful package/workspace facts in Lean; judge date/cell policies
against actual tasks. Qualify before extending to PDF/XLSX/PPTX/composite hosts.

Omit optional construction early where qualified. Dropping completed facts may
save retention without reducing parsing, native conversion or fingerprint work.
Current documentation distinguishes qualified early paths from specialized
fallbacks; claim no universal ingestion savings. [S6] [S8] [S9] [S10]

**Gate:** common evidence survives every selected family, Rich is a superset,
and exact text, semantic projection, locations and provider caveats are preserved.
Cases must cover large manifests/tables, long signatures, UTF-8, malformed sources
and explicit partial/unknown outcomes.

### Phase 3: publish availability and migrate safely

Add bounded family availability to existing profiles/publications and management
observations. Route rebuild/enrichment through current service jobs, reserved
resources and receipt recovery; keep MCP reads read-only. Update native flags,
schemas, capabilities, configuration review, help and migration diagnostics
together. Avoid another state store or scheduler.

**Gate:** interruption, source changes, stale requests, worker replacement,
pause/manual mode and quota failure preserve the last valid publication and
disclose pending work. No source/provider scope expansion.

### Phase 4: measure completed tasks before broad default changes

Run alternating Lean/Rich **index** builds on identical captured corpora/models,
independently crossed with Lean/Rich **replies**. Include code, manifests, prose,
tables and composite hosts. Measure cold/no-op/edit indexing time, CPU, peak
memory, retained/physical bytes, query work and expansion calls per task.

Then run paired Codex/Claude tasks with identical prompts, revisions, models and
permissions: declaration explanation, bug location, API change, test selection,
manifest update and document-backed implementation. Judge patch correctness and
missing-evidence recovery, not the first reply's size. Include negative questions,
ambiguous references and independently reviewed outcomes.

**Suggested gate:** no lost mandatory evidence or safety regression; no material
completed-task degradation; measured resource benefits justify omitted families.
Choose numerical thresholds before running the campaign. Keep small regressions
routine and broad qualification explicit; do not repeatedly push expensive CI
matrices as the debugging loop.

## Reproduction and validation

Representative current arguments follow. `parse.plan` must be an object despite
the exposed string declaration. The proposed `reply_detail` control is not used
in these examples because it does not exist yet.

```jsonl
{"tool":"symbols","args":{"name":"IndexDetail","path_prefix":"crates/airk-core/src/","compact":true,"limit":5,"max_bytes":8192,"response_mode":"terse","include_status":true}}
{"tool":"symbols","args":{"name":"IndexDetail","path_prefix":"crates/airk-core/src/","compact":false,"limit":5,"max_bytes":8192,"response_mode":"terse","include_status":true}}
{"tool":"code_facts","args":{"path":"Cargo.toml","compact":false,"limit":8,"max_bytes":8192,"response_mode":"full","include_status":true}}
{"tool":"document_facts","args":{"path_prefix":"docs/releases/","kind":"date","compact":false,"limit":5,"max_bytes":4096,"response_mode":"full","include_status":true}}
```

Valid compact-plan reproducer W08:

```json
{
  "schema": "airk.parse.request/1",
  "mode": "execute",
  "response_mode": "full",
  "plan": {
    "steps": [
      {
        "id": "find", "tool": "symbols",
        "args": {
          "name": "search_current_read_with_filters",
          "path": "crates/airk-kernel/src/current/search.rs",
          "compact": true, "limit": 5, "max_bytes": 8192
        }
      },
      {
        "id": "structure", "tool": "file_info", "args": {},
        "bind": [{"arg":"path","from":"find","select":"unique_definition_path"}]
      }
    ]
  }
}
```

Changing only `compact` to false produces W09. Here W08 skipped `structure` with
`dependency has no cited locations`; W09 completed both steps. The local receipt
retains their complete returned plans and step reports. Plan envelopes did not
expose request IDs in this run, so the receipt's test IDs are the correlators.

**35 post-run checks passed** against captured results and original files:
selected hash/text equality; compact/rich declaration identity; paged references
versus all 11 live matching source lines; conditional-validator preservation;
expected source/revision/cursor/path rejections; budgets/scope; unchanged remembered
evidence; DOCX row values/native coordinates; and targeted Python/config/test
discovery. These invariants deliberately coexist with the compact-plan failure.

No Rich-index counterfactual, indexing-cost savings, complete caller graph,
concurrent edit/reindex behavior, installed Workbench UI interaction, actual CLI
transport parity or non-Windows qualification was established. The MCP plan
probes exercised native Workbench read handlers, not the rendered UI. Full
Rust/extension suites were not run as a substitute for these live probes;
implementation must run relevant native/transport regressions and required
repository checks.

For this documentation change, repository checks, Rust formatting, source-link
validation, receipt-integrity checks and whitespace checks passed. An explicit
final review checked the results and proposal against the receipt and source;
this was not an independent review. No implementation fixes were applied.

## Source references

Links refer to the reviewed worktree. Existing dated research remains historical.

[S1]: ../../crates/airk-core/src/extraction.rs
[S2]: ../../crates/airk-kernel/src/workbench.rs
[S3]: ../../crates/airk-mcp/src/workbench.rs
[S4]: ../../crates/airk-mcp/src/lib.rs
[S5]: ../../crates/airk-mcp/src/response.rs
[S6]: ../extraction-policy.md
[S7]: ../mcp-prioritization.md
[S8]: ../../crates/airk-ingest/src/project_schemas/manifests.rs
[S9]: ../../crates/airk-ingest/src/prose.rs
[S10]: ../../crates/airk-ingest/src/tables.rs
[S11]: ../../crates/airk-ingest/src/adapters/profile.rs
[S12]: ../architecture.md
[S13]: ../../crates/airk-cli/src/tools.rs
