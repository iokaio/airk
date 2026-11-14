# CLI, MCP, Admin and Workbench read parity

## Current per-surface selections

Source roster reviewed 14 November 2026.

There is no universal four-surface parity runner. Select the affected rows below
and retain their separate results; this roster describes executable checks, not
a claim that they pass on the current tree. A read-contract change also needs
its operation-specific success, refusal, continuation and input-catalog cases.
Management operations outside evidence MCP are explicitly inapplicable there;
do not add an evidence mutation alias to make surfaces appear equivalent.

Run from the repository root. Native commands use pinned Rust/Nextest and the
`ci` profile; add the existing persistent `--target-dir` when applicable. The
`combined_reads` process fixtures require provisioned static ORT/model inputs
for `local-inference`, a supported native host and owner-controlled temporary
storage (`AIRK_PRIVATE_CENSUS_BASE` where needed). Follow CONTRIBUTING.md for
those prerequisites. Keep any supplied immutable runtime's original identity.
Node selections require pinned Node 24.15.0 and each package's locked npm
dependencies. Compile the extension once before its Node selections; its tests
consume `out/test/`, while Admin tests bundle their source through esbuild.

| Surface / boundary | Explicit selection | Evidence and limits |
|---|---|---|
| CLI/session protocol | `cargo nextest run --profile ci -p airkctl --lib --locked --timings -E 'test(cli::session::tests::)'` | Actual session procedures, reviewed frames, budgets and relay applicability with injected service/command observations; not a running-service journey. |
| MCP and CLI full results | Native selection A below | Actual native readers and CLI/MCP transports; single/combined scope, read families, larger plan results, discovery and complete input catalogs. These isolated component fixtures do not qualify installation or browser delivery. |
| CLI/session/HTTPS MCP/API process preflight | `python scripts/check-service-clients.py --runtime-dir <matched-debug-pair-dir> --files 16 --model --report <receipt.json>` | A matched provisioned pair is required. Real client/session and MCP/API observations supplement native tests; this does not exercise Admin or the Workbench host. |
| Admin native handler and I3 contract | `cargo nextest run --profile ci -p airk-https --no-default-features --lib --locked --timings -E 'test(admin_plane::tests::)'`; `cargo nextest run --profile ci -p airk-endpoint --lib --locked --timings -E 'test(admin_contract)'`; `python scripts/dev/contract_types.py i3 --check` | Handler envelopes, operation mappings, native outcome/recovery receipts and generated contract drift. Handler dispatch/peer observations are injected; this is not live browser/OS peer qualification. |
| Admin API, outcomes and selection ownership | `node --test packages/admin/test/r04Outcomes.test.mjs packages/admin/test/r19Serial.test.mjs packages/admin/test/r19Race.test.mjs` | Production API/envelope/workflow callbacks and request-state procedures, shared native fixtures, retained unknown outcomes and stale ownership. Injected hooks/fetch are not DOM, focus, browser export or live-transport qualification. |
| Workbench host, forms, complete details/copies/exports and session consumer | `npm run compile --prefix packages/vscode`, then selection B below | Production host/consumer paths with injected native replies/children, plus render/form/export checks. Combine with selection A's native parser/result evidence; these tests do not establish the new-index default-entry journey against a live session. |
| Workbench automatic/applied window scope and explicit-index native reads | `python scripts/check-workbench-window-scope.py --runtime-dir <matched-debug-pair-dir> --node <Node-24.15.0-executable> --report <fresh-private-receipt.json>` | Compiled production Workbench callbacks, WindowScope, adapter and framed session against a disposable native fixture. Compares complete native results and JSON exports for status/search, plus Workbench/Explorer search/symbol adapters and legacy-binding refusal. The VS Code host and session-history store are injected: this is not installed-editor, DOM or persistence qualification. The runner seals the compiled tree, harness owners and matched pair, preserves source hashes, and writes a pass only after owned native/directory cleanup. This is an executable selection, not a claim that it has passed on the current pair. |

**A — native full-result and input-catalog selection:**

```console
cargo nextest run --profile ci -p airk-cli --no-default-features --features local-inference --test combined_reads --locked --timings -E 'test(actual_native_cli_and_mcp_preserve_shared_index_scope_and_complete_results) | test(all_native_read_families_keep_complete_results_through_cli_and_mcp) | test(capabilities_plans_keep_complete_native_cli_and_mcp_results_above_read_frame) | test(actual_query_scope_discovery_keeps_three_views_and_continuations_in_parity) | test(opted_file_family_report_keeps_full_native_cli_console_mcp_and_input_catalog_parity) | test(installed_single_index_scope_disclosure_matches_native_cli_mcp_and_workbench) | test(airkctl_combined_reads_print_what_airk_printed) | test(open_reads_keep_cli_and_pipe_results_and_follow_connection_defaults) | test(open_https_connection_follows_choice_and_defaults_on_the_same_session)'
```

**B — compiled frontend/session consumer selection:**

```console
node --test packages/vscode/out/test/airkctlSession.test.js packages/vscode/out/test/workbenchOpenSelection.test.js packages/vscode/out/test/workbenchMcpOptimizations.test.js packages/vscode/out/test/profileFamilies.test.js packages/vscode/out/test/installedCompletion.test.js
```

The selected native scope-disclosure test compares the native parser path used
by Workbench; its name does not imply an installed VS Code host was exercised.
Admin management receipts and public evidence replies have distinct authority
and applicability. Retain every applicable result field and supported input,
including unknown fields in consumer copies/exports. Generated-schema checks
alone do not establish native/transport or frontend result preservation.

For changed Admin bundle bytes, run `npm run build --prefix packages/admin`
and `npm run check --prefix packages/admin`; affected embedded-asset checks and
candidate packaging remain separate. Report nonzero selected counts and exact
identities for every filtered batch. Missing live Admin/Workbench journeys,
platform prerequisites or affected operation cases remain explicit gaps until
their own source-bound observations are collected. No passing mock selection
closes those gaps.

## Contract notes and retained historical evidence

The dated phase/build statements below retain their original scope and do not
describe current candidate qualification. Current progress is in the
[execution ledger](simplify/one-process-ledger.md).

MCP's protocol envelope follows the negotiated SDK version. For sessions older
than `2026-07-28`, the SDK omits the top-level `resultType: "complete"`
discriminator; a direct native/CLI result may retain it. The `2026-07-28` inline
protocol uses per-request metadata and retains the discriminator. Requesting that
version through the legacy `initialize` handshake negotiates the supported
`2025-11-25` fallback. Parity tests project only that explicitly checked complete
discriminator for the negotiated legacy transport and compare the complete
remaining result, including error receipts, scope, provenance and retry details.

An explicit, unsubmitted scoped-read retry may report a catalog metadata clock
newer than the caller's original selector: the native resolver refreshes that
clock before admission. Clients accept only a monotonic clock with the same
validated context, namespace, index, Project selection, grant and grant revision.
They repeat the original arguments within the original deadline and retain the
complete receipt. Backward clocks, changed authority, unknown errors and
submitted or uncertain work do not authorize a retry.

Answer's validated nested deadline is anchored at public canonical admission,
before routing, scope setup or selected-member work. MCP projects the payload
using the actual native, Unified and combined tool schemas; selectors remain in
the unchanged original call for their owning authority and error checks. A
longer parent never refreshes the nested Answer ceiling after setup. Generic
reads retain their supported original allowance, including queries over30s.
Malformed arguments retain the canonical handler's existing error envelope.

P9's [permanent exclusions](source-policy.md) apply to canonical native handlers
and delivery fences across CLI tool calls/shell/convenience commands, MCP aliases
and Workbench plans, details and copies/exports. Full status and refusal responses
retain the policy observation, dated application, cleanup uncertainty and exact
job data. Narrow scope omits index-wide application details explicitly. Native
policy status/resume management responses distinguish the exact accepted retry
job from current policy status; evidence MCP remains read-only. Actual native,
CLI and MCP regressions compare complete results, including status after the last
Unblock; the [ledger](research/implementation-v190.md) records source-bound evidence.

The native management exact resume-receipt read returns the complete accepted
job, configuration and result, independently of current policy status. CLI and
Workbench preserve these fields and validate index/operation/caller/key identity.
This management response is outside the evidence MCP catalog; no evidence alias
acquires mutation authority. Missing/expired receipts remain explicit unknowns.

P7 adds explicit reviewed combined sessions, unique-index execution, shared
budgets and complete member-qualified results. [Combined-read contracts](combined-reads.md)
record selectors, discovery, publication vectors, plan verification and the
current qualification boundary. Legacy source bindings retain their one-index
meaning. Use the actual connection catalog for supported inputs.

P6f keeps single-index scoped reads on the canonical native handlers. CLI tool
calls/shell and convenience commands, MCP aliases and public names, and Workbench
complete or explicitly selected JSON/Markdown exports retain scope provenance,
publication, coverage and complete refusals. Native administrative reads remain
separate from evidence grants. Partial-subtree status/diagnostics refuse corpus
aggregates rather than returning guessed counts. Input-catalog comparisons retain
raw schemas and explicitly account for documented transport selectors and UI-only
tools. Actual transport and installed-editor evidence have separate receipts in
the [ledger](research/implementation-v190.md#p6f--catalog-management-and-scoped-native-activation).

P6c's [search execution contract](search-execution.md) discloses policy-qualified lexical results when the embedding helper is unavailable, preserving strict explicit semantic requests and independent source-verification receipts. Local Windows/Mac ARM gates and all 84 unchanged-bound latency comparisons passed; the ledger records exact coverage and remaining release limits.

The historical manual-test working version was **1.8.8**, implementing the planned
1.9.0 work. Earlier 1.9.0 phase headings below identify original development
checkpoints; their implemented interfaces remain in this branch. That build's
validation is in the [working-build record](releases/release-1.8.8.md) and ledger.

**1.8.8 working implementation:** [Saved-source verification](source-verification.md) defaults to Report for evidence reads, with bounded native checks and explicit Off/Require choices. CLI, MCP, Workbench and Inspector/Checker retain dated outcomes and uncertainty. Phase acceptance and performance qualification remain in the [ledger](research/implementation-v190.md).

**1.9.0 P2b:** optional [evidence guidance](evidence-guidance.md) is derived by one
Core projection used in native reads and Kernel Workbench fallback. CLI aliases,
tools-call/shell, MCP, parse steps, structured Workbench details and full/selected
exports preserve applicable reasons. There are no new query inputs. Suggestions
are inert; no source, coverage, model or registry work is added to explain replies.


**1.9.0 P1 update (2 November 2026):** compact discovery now feeds dependent
plans and cited evidence through the shared consumer. The earlier audit below
remains the inherited contract; current outcomes are in the
[candidate record](releases/release-1.9.0.md).

**1.9.0 P2a update:** optional `reply_detail: lean|rich` uses one Core policy for
native handlers, all CLI read aliases, tools-call/shell and Workbench plans/forms.
Absent arguments keep legacy defaults; explicit choices do not depend on retained
detail. Matching compatibility flags work; contradictions reject. See
[reply detail](reply-detail.md) for exact mappings. Plan defaults and per-step
overrides survive typed frontend observation, structured views and full/selected
exports. New input catalogs/tool-help schemas grow intentionally; ordinary default
evidence payloads gain no placeholder field or source/model/index work.

Audit dated November 1, 2026, for the 1.8.7 candidate. Equivalent read requests must retain complete native data through each interface. Workbench wraps reads in bounded plans; `airkctl tools call airk_<operation>` returns the native MCP result envelope. The tools shell uses that same call path and pretty-prints its text content. Dedicated CLI aliases below are convenience argument builders; their documented defaults can differ. Use `tools help` and `tools call --args-file` for the complete native input schema.

Since split Phase 4 the CLI is `airkctl`; the aliases below keep their names and arguments.

| Native operation | Workbench command | Dedicated CLI alias, where present |
|---|---|---|
| search | `/search` | `airkctl search` |
| symbols | `/symbols` | `airkctl symbol` |
| references | `/refs` | `airkctl refs` |
| read_file | `/read` | `airkctl read` |
| outline | `/outline` | `airkctl outline` |
| list_files | `/files` | `airkctl files` |
| file_info | `/info` | `airkctl info` |
| imports | `/imports` | `airkctl imports` |
| code_facts | `/code-facts` | `airkctl code-facts` |
| document_facts | `/facts` | `airkctl facts` |
| changes | `/changes` | `airkctl changes` |
| context | `/context` | `airkctl context` |
| relationships | `/relationships` | `airkctl relationships` |
| status | `/status` | `airkctl status` |
| diagnostics | `/diagnostics` | `airkctl tools call airk_diagnostics` |
| capabilities | `/capabilities` | `airkctl tools call airk_capabilities` |
| help | `/help` | `airkctl tools call airk_help` |

`airkctl parse --execute '/status'` and Workbench execute the same native `airk_parse` handler. `airkctl tools call airk_parse --args-file request.json` accepts an explicit bounded plan. This is a read-only composition layer, not an agent or management runner.

## Shared implementation and data

The kernel validates plan scope, revision, bindings, fan-out and budgets. The MCP adapter supplies a callback that invokes its canonical handlers inside the already admitted blocking lifetime. Inner reads use the refreshed outer kernel, one outer usage record and its execution permit. They cannot recursively invoke parse, management or transport operations. All 17 operations retain the MCP presentation, including `items` fact records, provenance/readiness and complete capabilities/help. Failed reads retain their complete native receipt under the same byte budgets.

The combined native/CLI/MCP parity fixture authenticates each complete raw
delivery digest and materializes its canonical member data before comparison.
Separate requests may report different remaining verification windows after
metadata I/O and sub-millisecond clock truncation. Only each known member
`source_verification.limits.deadline_ms` and `effective_deadline_ms` are treated
as elapsed timing observations after validating `effective <= deadline <=` that
member's admitted `allocation.verification_deadline_ms`, its member identity and
source/byte bounds. The admitted verification allocation, source outcomes,
counts, hashes, limits hit, fixed budgets, quota observations and raw receipts
remain literal; no production deadline or source-check budget is increased.

The frontend and one-shot CLI share response observation code. Direct status/diagnostics and completed plan steps report configured `query_concurrency`, reusing the policy observation made for admission. Unified project selection and diagnostics editor observations are also retained inside plan steps. Fresh inner status reads identify their own observation time, zero cache age and `cached: false`; separately cached direct status calls report their actual captured time and age. These values need not match between separate requests.

Scope selection remains interface-specific: unified MCP's `project` is a connection-configured selector, while CLI root/data/collection options select a native registration. The unified endpoint adds its configured project identity and optional Explorer diagnostic observation to both direct and nested results. Headless native CLI calls do not fabricate an attached editor or a connection allowlist. Compare the same underlying collection and native arguments, retain those additional observations when present, and distinguish them from indexed evidence. Native usage accounting measures the worker response before frontend-only metadata is added.

This configured-binding selector also appears on public `query_scope`. Its
closed public input schema includes optional `project`; the native prefixed
handler and combined-session catalog omit it. Tests compare each complete
schema with its owning canonical catalog and independently check the closed,
read-only contract. Selecting a binding does not widen its installed grant.

Workbench requests full responses with explicit rich facts unless compact is chosen. Its forms and slash flags include every native read input apart from transport display controls. Conditional validators, artifact/continuity selectors, change selector arrays, extraction filters, paging/count options and detailed capabilities/help options remain available. Exact fact paths use the same native handler as prefix reads and remain scoped by the plan.

Structured views offer **Complete response** even when cited cards are present. Full JSON and Markdown copies/exports preserve the complete response. Selected-evidence exports deliberately remain selected subsets and identify source uncertainty. Output/payload limits still apply; truncation, omitted records and failures are explicit. Previously saved turns keep their original response; rerun a command to collect new fields.

## Regression evidence

Compact `items` resolve against their own page's `files` and `provenance` tables.
Aggregated evidence carries an ordinary citation with the exact source hash and
original document location; its `fact.file` and `fact.provenance` contain resolved
descriptors, not page-local indexes. Original minimal step results are retained
unchanged. Invalid bindings or out-of-scope evidence reject the entire step;
continuations still prevent a unique-definition claim. Output clipping remains
explicit. This decoding performs no extra query, source read or indexing work.

`unique_definition_path` can target `outline`, `file_info`, `symbols` or
`code_facts`. A compact row without a native symbol ID cannot supply
`selected_symbol_ids`: explicitly read `symbols`/`code_facts` with `compact:false`,
the same exact name and the uniquely selected path, then bind the returned IDs.
That lookup is a visible, budgeted plan step; there is no hidden expansion.
`hit_locations` retains the cited path, inclusive lines and known source hash.

The native `parse` catalog advertises `plan`, `recipe`, `budgets` and `output`
as optional JSON objects. Encoded strings, arrays and booleans are rejected;
nested contents still use the canonical kernel contract.

- The native 17-operation table compares full MCP and executed Workbench results, checks the complete native input inventory, and verifies failed-read receipts. It excludes per-request IDs and cache observation timing from value equality.
- Bridge tests cover aliases, modes, all five capacity choices, live policy updates, identical JSON text/structured content, failed/unrelated results and payload/output bounds.
- The CLI session regression checks all 18 tool names through both response modes and all capacity choices, retaining arbitrary native fields, complete arguments and unchanged error receipts. Existing dedicated-alias tests deserialize their arguments against actual MCP schemas and check filter/matching/paging flags. The actual-process CLI/stdio regression also compares direct MCP status field presence with the nested Workbench result and requires capacity in dedicated CLI status and CLI-executed status plans.
- Extension regressions cover typed selector arrays and complete Markdown exports with evidence cards, alongside existing state, continuation, selection and host-boundary tests.
- P1 regressions exercise compact/rich chained reads through actual handlers and
  CLI/stdio transports, page-local default/override provenance, document/mapped
  locations, malformed bindings, scope/revision rejection, ambiguity, clipping,
  explicit rich symbol-ID expansion and full/selected exports.

These local regressions do not establish installed-editor acceptance or cross-platform qualification. Current run outcomes are in the [1.8.8 working-build record](releases/release-1.8.8.md); original 1.9.0 checkpoints and inherited 1.8.7 qualification remain in their dated records.


Independent optional readiness probes can differ in availability between requests:
`discovery_observation: unavailable_after_read` retains uncertainty rather than
inventing a state. Actual full replies and exports preserve that observation.
The native parity fixture compares available readiness exactly; for an explicit
available/unavailable timing difference it validates the typed fixture state and
each original projection's calculated receipt headroom separately. Only those
dated observations and derived headroom are normalized for equality. Requested
verification budgets, evidence, counts, hashes, publication and origin identities
remain exact. A native verifier refusal never acknowledges undelivered observations
in the direct client's status receipt.

Single-index catalog-grant and in-memory Window/Admin backends expose their
selector source through the same scoped routing hook. Its public descriptor
contains selector projections, never a nonce, lease or service instance; live
native admission still owns authority. Unified routing, per-call wrappers and
HTTP capacity decoration preserve both sources. Component regressions retain
one complete canonical handler observation through those adapters and the SDK
bridge, including help/status/diagnostics/parse success, refusal, setup and
observation failures. They do not substitute for the required running-service,
CLI/session, HTTPS MCP or installed Workbench parity campaigns.


### Strict advanced-split transport observations

New transport comparisons use `tests/common/parity/strict.rs` and raw
`airk.split.parity/2` snapshots tied to the observed matched pair’s runtime
manifest, service-image and CLI-image SHA-256 values. Constructing a snapshot is
not qualification. A new baseline must come from the identified actual pair;
legacy canonical placeholders cannot reconstruct stable digest bytes.

Normalization defaults to none. Only explicit reviewed outer JSON pointers for
transport request/service/window/connection identities and observation timing
may be registered; each identity must match its captured context. Nested
document fields, content/model hashes, publications, revisions, continuations,
counts and unregistered timing remain exact. Independent negative cases cover
stable digest changes and misleading nested identity/timing names.

The original U95 `airk.split.parity/1` captures and their approved recorded
limitations remain historical replay evidence. Their explicitly named legacy
hex-token comparison does not establish strict v2 transport parity.

### Current /2 Answer comparison boundary

Fresh Answer transport tests validate each complete raw ContextResponseV2 with
its original Core budget before comparison. For unassessed native fallback,
the existing pure Kernel Gathering owner first replays the complete retained
fallback dispatch prefix and regenerates every canonical record digest. Omitted
or learned/non-prefix packets are refused by this proof. Only then may tests
normalize canonical-record primitive transport request IDs and typed clocks, then rebind that record's derived identifier by a
validated positional bijection at records, grounded paragraph references and
assessment basis source-unit references. Original records, stable source/model/
publication identities, native arguments, counters, allocations and unknown
nested evidence fields remain retained. Raw replies are saved on differences.
Learned selected/abstained assessments require their own decision/prompt parity
proof; this fallback comparator refuses them rather than hiding a changed
model input or decision. Tamper cases must detect changed evidence, authority,
coverage and orphaned record references.

The optional `check-service-clients.py --answer-validator <prebuilt-combined_reads-test-binary>`
selection collects actual current CLI/session/MCP/API packets and executes only
`retained_current_answer_pair_validates_full_transport_results --exact --ignored`
on that existing Rust integration binary. It never invokes Cargo. The validator
checks the full raw packets and emits its executable, input and compiled owner/
fixture hashes; the harness refuses stale source/fixture identities. It requires
the Rust-emitted current /2 public request case. This selection proves only the
exercised unassessed fallback; learned prompt/decision transport parity remains
a separate required qualification. Without this option the preflight report
explicitly records Answer transport coverage as unexecuted.
