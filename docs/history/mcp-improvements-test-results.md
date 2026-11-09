# AIRK scoped MCP scenarios — 2026-11-01

Raw ledgers and validation JSON under `artifacts/` are local-only evidence and are not included in the repository or release packages. The summaries below retain their observed results and limitations.

## Installed 1.8.7 acceptance — November 1, 2026

The owner confirmed a campaign of **78 live MCP checks**, distinct from the Rust library suite. These manually selected checks exercised all **28 public tools** through the current Windows x64 installation. **All 78 reached their expected outcome after two harness input corrections and one explicitly permitted retry; this was not an error-free first pass.** On valid initial inputs, 77 met their expectation and one concurrent reference read returned `publication_busy`, `retry_after_ms: 25`, `action: repeat_same_request`. Its single later retry succeeded. Negative controls passed by refusing invalid input; no submitted or uncertain request was blindly replayed.

The run retained publication `airk-r1:19ae18e4a4de52ae6554dac618c4c032:5772:98e751039f8f7b460bdaf2fd06460acf1f5da55cacfcf92c8bfaa822fe2e1383`. Live-file comparisons describe the files at that observation, before later documentation edits; they do not establish that today's source still matches that publication.

Two initial parse requests followed the client-rendered string signature for an unconstrained JSON field; native `plan` requires an object. Both corrected object requests passed. Preparation stayed inert, and execution disclosed its deliberately bounded file-list continuation. Three four-request batches combined status, search, references and file reads; all three status calls succeeded. One file read retained evidence while disclosing `discovery_observation: unavailable_after_read`.

Live verification matched all **22 distinct cited-file SHA-256 hashes**. Each of three complete reference responses matched all **11 whole-identifier `Session` lines** in the selected CLI test file. Bounded README reads matched live text, six adjacent-page checks found no duplicate records, conditional reads returned `not_modified`, and path escape, unconfigured project, malformed cursor, expired revision, invalid handoff token and unknown draft-field controls refused as expected. Four test proposals remained `pending_review` in Explorer; none was applied or executed.

The extension, frontend, service and worker reported **1.8.7**, with configured `query_concurrency: 16`. The observed service source digest was `84b59dcb4af4ed224b09f72a27ab67e841d05268b125dc09dafd631b8cb802f3` (749 inputs); its payload was `b185f0980631b319968dbd209c14970020f11048c47a46412a57d6f1e7d1fb61`, matching the local candidate receipt. Frontend/worker hash observations remained pending, so full process-payload agreement was not established. Final status remained available with discovery and semantic readiness reported ready.

This was one warm connection with selected scopes and pages, not the original 20-scenario replay, a throughput benchmark, cold-start/overload/cancellation qualification, CLI/Workbench parity proof or cross-platform acceptance. Successful user-shared handoff and applying drafts were not tested. The known incomplete `Session` struct navigation remains: its bounded relationship query returned no edges; written `Session::new` returned four syntax candidates. The candidate's native/transport parity tests and package evidence are recorded separately in the [1.8.7 release record](releases/release-1.8.7.md).

## Historical 1.8.3 campaign

This run executed 10 documentation scenarios and 10 source scenarios derived from [mcp-recommendations.md](mcp-recommendations.md), especially sections 5.1–5.8 and the implementation phases. Each scenario used at least two distinct AIRK MCP tools.

**Latest follow-up:** the [contention and navigation investigation](#184-contention-and-navigation-follow-up--2026-11-01) exercised larger shared-session bursts. One eight-request burst returned a submitted status supersession error; all 16 unsubmitted refusals recovered on one permitted retry. The earlier 20-scenario replay passed its mixed status batches, but does not qualify larger harnesses. Struct navigation works on a complete positive control and remains blocked for all six checked Session structs. Earlier results remain historical evidence below.

**Result:** 73 retained scenario calls: 72 successful tool responses and one typed status error. Scoped references matched the live oracle in the selected scopes. Struct-level navigation remained inconclusive, and a mixed batch superseded one status observation. Successful calls and bounded results are not equivalent to universal recall or performance qualification.

## Executive summary

The 1.8.3 tools supported useful, verifiable investigations across documentation and source in this run. All 20 scenarios used multiple AIRK tools; 72 of 73 retained scenario calls succeeded. The strongest evidence is retrieval correctness in the checked scopes: paged references matched all 42 documentation and 37 source matching lines found by live `rg`, with no missing, extra or duplicated lines. All 28 returned evidence files matched their indexed SHA-256 hashes when checked externally.

- **Scoping and tool choice matter.** Language/evidence filters, exclusions and explicit name matching behaved as expected. For a known identifier, compact `symbols` followed by scoped `references` and a bounded read gave direct evidence. Conceptual search benefited from narrowing and context reads; its highest-ranked result did not always address the intended meaning.
- **Paging and count precision make bounded results easier to assess.** Continuations recovered the checked reference sets without boundary duplicates, and file summaries identified where matches occurred. Small-budget responses exposed lower bounds and unknown totals. Completing a candidate scan still did not establish universal query completeness.
- **Compact responses and validators reduced returned bytes in the measured examples.** The same excerpt occupied 1,837 serialized response bytes in terse mode versus 3,691 in full mode; an unchanged conditional read returned 703 bytes. These observations establish response-size savings for those calls, not end-to-end speed, wire-byte or cost savings.
- **Two gaps remain priorities for diagnosis.** Struct-ID relationship queries returned no edges or suggested targets and reported truncation; written-target lookup found four syntax call candidates for a separate pooled Session type. A four-call mixed batch also returned one `status_observation_superseded` error while its searches succeeded. Later successful status observations do not explain that failure.
- **Broader matching requires care.** Suffix matching found seven definition records, while contains matching expanded to 88 records through qualified names, including nested variables. Returned records are not necessarily distinct types, and textual or syntax candidates do not establish resolved references.

The next work should reproduce the mixed-load status failure and explain the struct-navigation limit, then run a controlled baseline/replay campaign before claiming performance improvement. This was one Windows session on a dirty working tree with an existing connection. It does not qualify cold startup, other platforms, overload/cancellation behavior or universal recall. The historical recommendations also retain inconsistent call-count and resolved-caller wording that should be reconciled separately.

## Follow-up decision — 2026-11-01

The owner accepted ordinary status supersession as informational and requested a rebuilt local 1.8.3 installer. The status path now preserves a completed same-authority snapshot with `status_observation.superseded: true` rather than returning an error solely because scalar state was invalidated. Scope/worker replacement, source authorization expiry and shutdown remain guarded. The four-call bridge cap is unchanged. D10 and the original call/error totals below remain historical observations from the earlier build; they are not rewritten as passing results. Subsequent validation and installer receipts are recorded in the [1.8.3 release record](releases/release-1.8.7.md#candidate-1-8-3). Other improvements remain follow-up work.

A further owner-requested 1.8.3 follow-up accepts ordinary read/execute-only sharing in Windows native write-boundary eligibility and keeps strict fallback for foreign-writable or unsupported layouts. This changes the optional fast-path preconditions, not source write permissions or corruption checks. The initial scenario ledger predates this change; final platform validation belongs to the release record.

## Replacement installed-build retest — 2026-11-01

**Release assessment: hold.** Three fresh repetitions each of D10 and S10 produced 3 status errors in 24 mixed-batch calls. All 18 sibling evidence calls succeeded. The new informational `status_observation.superseded` note was not observed. This does not confirm the installed status repair; the exact invalidation/authority cause remains undiagnosed. The historical 73-call run above and below is unchanged.

| Retest | Observed result | Assessment |
|---|---|---|
| D10: status plus lexical/vector/hybrid search, three four-call batches | Status succeeded in rounds 1 and 3; round 2 returned `status_observation_superseded`, `query_submitted=true`, `fresh_brokerage_required=true`. All 9 searches succeeded with 3 scoped hits each. | Installed repair not confirmed |
| S10: status plus symbols/references/read_file, three four-call batches | Status succeeded in round 2; rounds 1 and 3 returned the same typed error. All 9 evidence calls succeeded; every reference response matched the 11 live matching lines. | Installed repair not confirmed |
| S5 control, not a claimed follow-up repair | Fresh struct lookup followed by incoming/outgoing relationships still returned no items or suggested targets, with `truncated=true`. Written `Session::new` returned the same 4 syntax candidates at 219/229/239/242, matching live source. | Navigation gap unchanged |
| Current-checkout status ownership regressions | 9 passed, including same-head scalar invalidation preserving publication/time without cache/receipt acknowledgment and retaining authority/expiry guards. | Local pass |
| Current-checkout actual-IPC brokerage regressions | 12 passed, including submitted scalar invalidation without replay, obsolete status rejection, private-channel reuse and namespace/worker/expiry fences. | Local pass |
| Current-checkout Windows storage-boundary regressions | 5 passed: read sharing, creator-owner/partial privileged grants, foreign-write rejection, inherited private files and fixed-volume classification. 1 actual remote-share fixture explicitly ignored. | Local pass; remote layout unqualified |

The installed service and MCP frontend report version 1.8.3, executable SHA-256 `80b5a3e51456e55965da6dcaf49ee2f439d0d21486173ecafefeef1f5af47b53`, payload `4eb86172e159da428033cf4e71b9951e5c6ced5a6fa4849a1d17edfe91a4f8ad` and native source digest `c6616f37f97b79fdd3dcbf66bd55e69784471b461935c2ce90ecaab589fc37ec` (747 source inputs). The installed extension manifest has the same payload identity. Service PID 330564 remained responsive. The worker observed before retesting was PID 162244/fencing generation 4159; final diagnostics observed PID 222656/generation 4160, started at 13:23:35 MST, after the six batches. No per-failure authority trace was collected, so the errors cannot yet be attributed specifically to ordinary scalar invalidation rather than a retained hard guard.

The runtime builder's `source_identity()` on the live checkout returned `45190cf815b46824d25653c1701d0ef5bdb06e1ca19e1f5302fe5676aab988ec`, also 747 inputs. **The locally compiled regression passes do not qualify the different installed build.** Reconcile source/artifact identity and diagnose the installed failures before accepting another candidate; a separately identified 1.8.4 is recommended for the next correction. No version was changed in this retest.

The replacement ledger (local-only `artifacts/mcp-scenarios-2026-11-01/replacement-1.8.3-retest.json`) retains exact arguments, raw tool results, timings and final diagnostics. It contains 28 scenario calls (25 successful responses, 3 errors) plus one final diagnostic call. Setup/navigation calls are outside this ledger. Each repeated batch was a separate planned observation; submitted failed requests were not automatically replayed. All scenario evidence used publication `airk-r1:19ae18e4a4de52ae6554dac618c4c032:5566:f5714cb7c74c2e91fa8f4ddaecb0d010f6ace63fdb78c6d68618ef3560116c85`. Live validation (local-only `artifacts/mcp-scenarios-2026-11-01/replacement-1.8.3-validation.json`) matched all 4 distinct returned-file hashes and the checked reference/constructor line sets. Bounded search truncation and reference coverage limits remain authoritative.

Focused commands used the installed Cargo executable by absolute path (Cargo was initially absent from PATH):

```powershell
& "$env:USERPROFILE\.cargo\bin\cargo.exe" test -p airk-cli --lib --locked --offline project_client::status_observation::ownership_tests:: -- --test-threads=2
& "$env:USERPROFILE\.cargo\bin\cargo.exe" test -p airk-cli --lib --locked --offline project_client::source_brokerage_ipc_tests:: -- --test-threads=2
& "$env:USERPROFILE\.cargo\bin\cargo.exe" test -p airk-store --lib --locked --offline private_storage::windows::tests:: -- --test-threads=2
```

Initial status test selectors matched zero tests; they are not counted as passes. The corrected module selectors above executed 21 status tests. No implementation changed, and no commit, push, PR, installation or publication ran. Full workspace formatting/Clippy/tests, repository-wide checks, performance/cold-start campaigns, hosted gates, macOS/Linux and installed-editor UI qualification were not rerun. This is a focused Windows retest, not release qualification.

## Distinct 1.8.4 candidate validation — 2026-11-01

The owner requested a new version because a same-version 1.8.3 replacement could not be tied to the tested checkout. The freshly compiled Windows x64 1.8.4 pair has native source digest `b356747fff3c1382071c50d61578506228cbd208b9030c42281e41df739b0694` and payload `954972f90b9680fbb3a7ef48478524cb83182069c57a9344d896dc702db8b0c4`. This is separate from the earlier installed 1.8.3 observations; the normal installation was not replaced by this task.

An isolated, real-model D10-style retest used status plus lexical/vector/hybrid search in three batches for each of source-bound and pinned MCP connections. **All 24 single-shot calls succeeded**, including six status observations; **three returned the new informational supersession note**. All 18 search citations matched the exact original fictional Markdown source hash. Before/after worker identity remained stable, and final runtime diagnostics reported version/build/payload agreement. The raw candidate ledger (local-only `artifacts/mcp-optimizations-1.8.4/vector-mixed-batches.json`) preserves exact arguments, replies, publication tokens and runtime observations. This is a controlled fictional scope, not a replay of the entire repository corpus or a ranking/performance comparison.

The final three four-processor managed-smoke rounds passed, including a mixed documentation/source regression in both connection modes: **36 batches / 144 measured reads**, with zero unsubmitted refusals or retries required in those final rounds. Earlier four-processor fixture runs encountered permitted unsubmitted source-setup refusals, including after warm-up; those are retained as failed test-assumption observations. The corrected test follows the shared typed-read contract and observes every refusal. Unknown or submitted errors remain fatal; a separate unit test proves a submitted status error with `retryable=true` executes once and is never replayed. No native assertion or admission policy was weakened to obtain these passes.

The [1.8.4 release/build record](releases/release-1.8.7.md#candidate-1-8-4) documents the version reason, issues addressed, initial failures, exact VSIX digest and remaining limits. Its full Windows workspace run passed **1,821 tests with 31 explicit ignores**; extension units passed **535 with 19 skips**. The S5 conservative incomplete-declaration limit remains disclosed. The user's Mac was not accessed; no 1.8.4 macOS or Linux testing occurred in this run. Neither the full original 20-scenario campaign nor installed-editor/manual acceptance was rerun.

## 1.8.4 contention and navigation follow-up — 2026-11-01

The owner requested deeper testing of brief admission refusals and incomplete type navigation, then asked how a harness with more than four independently operating agents would behave on one session. This follow-up kept the original repository binding and installed 1.8.4 build. No implementation or concurrency settings changed.

**Release assessment: hold the many-caller claim pending diagnosis.** Four-request batches remain useful, but larger shared-session bursts exposed one submitted status supersession error. Its receipt does not distinguish the guarded scope/authority/expiry causes, and no per-failure worker trace was captured. This is not proof that the scalar-only repair regressed; neither is it evidence that a larger harness is qualified.

The raw follow-up ledger (local-only `artifacts/mcp-scenarios-2026-11-01/limitations-1.8.4-ledger.json`) retains **224 calls: 207 successful replies and 17 error replies**. Sixteen errors explicitly said unsubmitted/retryable; all sixteen succeeded on one retry after the indicated delay. The remaining error was a submitted status observation and was not replayed. Counts include the retries. Setup/navigation discovery calls and final diagnostics are outside those 224 measured calls.

| Campaign | Initial calls | Initial errors | Recovery/findings |
|---|---:|---:|---|
| Twelve serial samples | 12 | 0 | Alternating status, references, contains definitions and hybrid search |
| Twelve two-request batches | 24 | 0 | Same rotating workload |
| Twelve four-request batches | 48 | 2 | Both `direct_query_setup`, explicitly unsubmitted; both single retries succeeded |
| Three sets of four independent paged workflows | 48 | 4 | All four workflows completed each time: 25 headings, 42 reference lines, 88 contains-definition records, 50 outline records; all four single retries succeeded |
| Three eight-request shared-session bursts | 24 | 5 | Four unsubmitted setup refusals recovered; one submitted `status_observation_superseded` remained unretried |
| Three twelve-request shared-session bursts | 36 | 6 | Four unsubmitted `direct_query_admission` and two setup refusals; all single retries succeeded |
| Navigation, written-target and scope controls | 16 | 0 | Positive navigation works; checked Session structs remain incomplete |

Eight/twelve-request bursts contained one status call and repeated reference, definition and hybrid reads. Round 2 placed status last; rounds 1 and 3 placed it first. The eight-request status failure occurred in round 1 after 1,773 ms. All five other burst status calls succeeded in 169–398 ms, two with informational scalar supersession. All 21 status calls in the 1/2/4-request campaign succeeded; four included informational scalar supersession. These are warm observations on this Windows machine, not a latency or capacity guarantee. Local Cargo compilations occurred during parts of the follow-up, so timings are not controlled performance measurements.

### What the four-request guidance actually means

Live `crates/airk-mcp/src/remote.rs` reserves **three generic executing permits, one status permit and eight waiting permits**. Evidence reads wait in the bounded bridge queue; status uses the reserved lane, or an idle generic lane if its status lane is occupied. Four is therefore execution guidance, not an agent-count setting or immediate rejection of every fifth request. The larger bursts demonstrate that more than four requests can complete through one session. They also demonstrate finite queue/deadline refusals.

Native `Backend` in `crates/airk-cli/src/project_client.rs` additionally serializes each connection's evidence-channel access through `state: Mutex<ClientState>`, bounds its waiters to eight, and bounds the state wait to two seconds or the earlier source authorization expiry. Its source/authority checks can emit the observed setup receipt at several stages. Worker-side admission has its own bounded FIFO turn. Increasing the bridge permits alone does not create equivalent parallel evidence execution; the existing per-connection transport is a separate bottleneck. The observed setup errors do not identify one exact guard, so their root cause must not be labeled simply bridge saturation.

**Recommended next implementation, not shipped behavior:** replace the one fixed evidence-execution choice with a measured, resource-derived automatic default and an explicit user override, retaining finite ceilings, byte/memory budgets, deadlines and reserved status capacity. Absorb ordinary bursts in a bounded queue and report explicit backpressure when it fills. If parallel evidence work is desired, use a bounded pool of authenticated native channels within the current CLI adapter, preserving project/publication/worker fencing and receipt ownership per channel; merely enlarging the bridge queue or request count is insufficient. Enforce a shared project budget across clients so separate frontends cannot multiply resource allowances. Any fairness or adaptive admission belongs in the existing native owners, not an agent orchestrator. Compare 1/2/4/8/16 callers on multiple machine classes before selecting defaults; CPU count alone does not measure storage, inference or memory headroom. No particular 8/16 default is justified by these tests.

An eight-independent-frontend probe attempted the configured launcher using read-only initialization/catalog commands. It could not initialize in this shell: a separate launcher `--help` check reported that its selected installation was inaccessible, `Access is denied (os error 5)`. The connected MCP session remained usable. Thus separate-process/same-project throughput was **not tested**; the shared-session bursts are not a substitute for that campaign. No launcher configuration, permissions, installation or service process was changed.

### Navigation depth and cause

All six returned Rust `Session` struct IDs were tested with scoped incoming lookup and outgoing contains traversal at two hops: `embed_pool.rs`, `embedding_client.rs`, `embedding_pool_client.rs`, CLI `tests/mcp.rs`, `providers_lsp.rs` and MCP `response.rs`. All twelve responses succeeded with **zero items/suggestions and `truncated=true`** despite a 100-record/48,000-byte response allowance. Expanding the response budget and traversal did not restore navigation.

The positive control `QueryAdmission` in `project_worker_admission.rs` returned five exact same-file candidate targets: `default`, `close`, `completed`, `while_open`, `admit`. Two-hop outgoing containment returned seven edges (two impl blocks plus five methods), without truncation. A scope excluding that file returned zero results without truncation. Written `Session::spawn` in the direct embedding client returned three syntax candidates at lines 30/33/164, matching live source. These are navigation and textual call candidates, not resolved callers.

Live retained facts and source explain a broader limit than the original S5 example: `navigation::derive` abandons the **entire artifact's type navigation** if any native struct/enum/impl definition has `details_truncated=true`. Thus complete target declarations can be blocked by another declaration in the file. For example, the MCP response `Session` and its impl are complete, but `ResponseArgs` at line 85 is flagged truncated. The flag in `syntax::code_evidence` combines several omissions, including a predecessor-text length check performed before classifying that predecessor as a comment/attribute. This suggests a potential unnecessarily broad truncation path; this follow-up did not isolate it with a new fixture and does not claim a proven extractor defect. The file-level refusal is directly observed and materially limits usability. A fix should establish which identity/scope evidence is actually complete before narrowing that refusal; it must retain ambiguity, generic-shadowing and source-binding protections.

### Validation and limits

Live validation (local-only `artifacts/mcp-scenarios-2026-11-01/limitations-1.8.4-validation.json`) checked **1,899 hash observations across 15 evidence files**, with zero mismatches. Each of the three paged reference workflows matched all 42 live documentation matching lines without missing/extra/duplicated lines. All retry attempts preserved their original arguments and had explicit unsubmitted/retryable receipts.

Focused current-checkout tests passed **24 tests, zero failures/ignores**: six store navigation tests; two kernel type-navigation integrations (positive same-file/generic/trait scope and negative alias/duplicate/generic-shadowing/budget/refresh behavior); five worker-admission tests (capacity release, bounded waits, cancel/expiry/EOF and revalidation); eleven MCP remote transport/admission tests (reserved status, queue bounds, cancellation, closure, EOF and forwarding). Tests used locked offline Cargo inputs, two test threads, and no default features for CLI/kernel/MCP. These local test builds are distinct from the installed runtime used for tool calls; they do not independently qualify its binary behavior.

Final tool diagnostics retained publication generation 5597, token `airk-r1:19ae18e4a4de52ae6554dac618c4c032:5597:3ad5179625f849cfe0c7a130b2443ec6bc8d7fc283ff7dfcf46518e9792775ef`. Frontend/service identities matched the previous replay's 1.8.4 payload and source digest. Worker metadata was initially pending in this final diagnostic; it observed PID 243004/fencing generation 4178, rather than the previous replay's PID 159368/generation 4175. That change occurred across the broader observation interval, not necessarily during the failed burst. Without a before/after worker observation for the failed request, its cause remains unknown. No status error was hidden by retrying it, changing permissions, increasing limits or weakening a check.

This follow-up did not rerun full workspace formatting/Clippy/tests, cross-platform qualification, installed-editor acceptance, fresh installed startup or resource-controlled load/throughput benchmarks. There was no implementation, version, commit, push, PR, installation or publication change. The results support configurable, layered capacity work; they do not establish an automatic policy or qualify unlimited callers.

## Installed 1.8.4 full replay — 2026-11-01

**Assessment: concerns partly resolved.** All 10 documentation and 10 source scenarios were repeated against the configured repository connection. The status repair is confirmed in this Windows session, including its actual scalar-invalidation path. Reference paging, filters, name matching, counts, definition pointers and conditional reads behaved as checked. Brief admission refusals still occur, and the original embedding-client struct investigation still supplies no method navigation.

The retained replay ledger (local-only `artifacts/mcp-scenarios-2026-11-01/replay-1.8.4-ledger.json`) contains **91 calls: 89 successful responses and 2 typed unsubmitted busy refusals**, including the successful single retries and two additional repetitions each of D10/S10. This is 73 calls for the original workflows plus 2 retry attempts and 16 additional mixed-batch calls. Dependent continuations used fresh returned cursors; the conditional read used its freshly returned validator, and relationships used the freshly discovered struct ID. The original query filters and budgets were preserved. Calls ran in groups of at most four. Submitted errors were not replayed.

| Scenario | Replay finding | Assessment |
|---|---|---|
| D1: inventory and outline | 12 scoped documentation paths; 8 index outline records with continuation | Observed within bounds |
| D2: historical record and plan | 25 heading facts over 2 pages; matches 25 live headings; plan excerpt remains explicitly proposed work | Pass |
| D3: conceptual admission guidance | Broad top hits still need interpretation; narrowed search and context expose current guidance | Observed; ranking judgment remains |
| D4: extraction filters | Markdown/document returns 3 hits; Rust/document returns 0; metadata identifies Markdown extraction | Pass |
| D5: paged documentation references | 42 unique lines over 5 successful pages match live `rg`; one intervening unsubmitted setup refusal, then successful retry | Correct result; admission friction remains |
| D6: exclusion | 4 search hits outside the excluded file; references restricted to that same excluded file return 0 | Pass |
| D7: outline continuation | 50 facts over 4 pages; final delivery excerpt retrieved | Pass |
| D8: conditional excerpt | Same validator returns `not_modified`; 703 serialized bytes versus 1,837 for content; hash lookup succeeds | Pass; indexed validation only |
| D9: response modes | Terse/full/include-status return the same excerpt in 1,837/3,691/2,848 serialized bytes; serial status healthy/idle | Pass; no speedup claim |
| D10: status and three search modes | Three four-call batches: all 12 calls succeed; round 3 status returns informational scalar supersession; each search returns 3 scoped hits | Status repair observed |
| S1: known type | 2 MCP Session definition records, 11 textual reference lines and bounded source excerpt; references match live `rg` | Pass |
| S2: name variants | Exact 0, suffix 7, contains 88 definition records over 5 successful pages; one unsubmitted setup refusal recovers on retry | Matching passes; breadth/admission limits remain |
| S3: source extraction filters | Rust/code admission search returns 4 hits; Rust/document returns 0; metadata reports syntax extraction | Pass |
| S4: source reference paging | 26 unique lines over 4 pages match live `rg`; 9 definition records | Pass within checked scope |
| S5: type and written constructor | Embedding-client struct incoming/outgoing both return 0 items, no suggested targets, `truncated=true`; pooled `Session::new` returns the same 4 syntax candidates at 219/229/239/242 | Navigation gap persists |
| S6: trait and imports | Scoped RemoteSession trait and grouped import retrieved | Pass |
| S7: outline versus named lookup | First 12 outline records require continuation; direct lookup returns 2 Session records | Pass |
| S8: identifier pointers | 2 search snippets with 4 definition pointers; bounded declaration read succeeds | Pass |
| S9: small budgets | 2-record pages disclose lower bounds/unknown totals; 5 symbol pages recover 9 records | Pass |
| S10: status and three source reads | Three four-call batches: all 12 calls succeed; each reference response has the expected 11 lines | Pass for these batches |

All **24 mixed-batch calls succeeded**, including six status observations. The observed supersession note has `reason=scalar_invalidation` and `superseded=true`; it is a successful snapshot rather than the prior submitted error. Batch wall times were D10: 1,280/1,039/1,034 ms and S10: 720/740/735 ms. These are warm single-session observations, not matched performance comparisons. Two other continuation calls returned `direct_query_setup`, `query_submitted=false`, `retryable=true`, `retry_after_ms=100`, `fresh_brokerage_required=true`; each was retried once after more than the indicated delay and succeeded. Thus zero-refusal admission is **not** established.

The live validation (local-only `artifacts/mcp-scenarios-2026-11-01/replay-1.8.4-validation.json`) checked 329 hash observations across **28 distinct evidence files with zero mismatches**. All 42 documentation, 26 CLI-source and 11 MCP-source matching lines agree with live `rg -w`, with no missing, extra or duplicated lines. Completed candidate scans still disclose incomplete universal query coverage; the live oracle supports only these selected scopes. AIRK itself continues to report `live_source_verified=false`.

Final diagnostics report 1.8.4 for frontend, worker and service, with `versions_match`, `builds_match` and `payloads_match` all true. Executable SHA-256 is `5bef886bff21969c1a00c64dc4da5ccc9bc3e441d83a08e57b9cc68777aaddae`, payload is `954972f90b9680fbb3a7ef48478524cb83182069c57a9344d896dc702db8b0c4`, and compiled native source digest is `b356747fff3c1382071c50d61578506228cbd208b9030c42281e41df739b0694` (747 inputs). Initial diagnostic metadata was pending; the final observation resolved it. The serving worker remained PID 159368/fencing generation 4175 across the two diagnostic observations. Collection publication remained generation 5595, exact token `airk-r1:19ae18e4a4de52ae6554dac618c4c032:5595:9ac87e47b5c5670c2ab0df6272e63ef00268a917f44ad9ff63f698e80d8e9284`; status reported 1,410 files, 5,292 chunks and 33,615 vectors, with all 1,408 selected semantic artifacts ready. This identifies the running build, not every byte of the dirty checkout.

S5's conservative limit has a concrete source explanation: the retained `impl RemoteSession for Session` reports `details_truncated=true`. Live `crates/airk-store/src/current/relationships/navigation.rs` rejects navigation for an artifact with any truncated native struct/enum/impl definition. This explains the absence without resolving the original user-facing navigation concern. The four written constructor calls concern the separate pooled type and do not establish callers of the embedding-client struct. Historical recommendation wording describing them as resolved callers remains overstrong.

Harness disclosure: 20 preliminary read-only calls returned before a logging helper failed on an unavailable `TextEncoder`; their replies were not retained, so their outcomes are not counted. The helper was corrected before the retained replay. An initial setup read exceeded the tool's 200-line range and was corrected to 180 lines. Discovery/setup/navigation and the two diagnostics are outside the 91-call scenario ledger. No source implementation, installation, version, commit, push, PR or publication changed. This is a Windows warm-connection retrieval audit; full workspace formatting/Clippy/tests, release gates, cold-start/overload/cancellation campaigns, installed-editor UI, macOS and Linux qualification were not rerun.

## Run identity and method

- Date: 2026-11-01, Windows/PowerShell, existing MCP connection; no cold-connection benchmark.
- Installed AIRK status version: **1.8.3**. Exact installed binary hash/build provenance was not collected.
- Configured source binding: `\\?\D:\code\Github\airk`; selector `54c6ddfbd9f5ea27159c390186159e9628fc5951682a7015e13744e6965315c1`.
- Collection: `project-2cb86554e9ba4fd24218d0af`.
- Publication for every retained evidence response: `airk-r1:19ae18e4a4de52ae6554dac618c4c032:5544:df6d14b77e00bfffde8d9eb41d42bc1171ece4da1f17b1d54adf973131ccacdd`.
- Status corpus: 1,408 files, 5,282 chunks, 33,541 vectors; 1,406/1,406 selected semantic artifacts completed. Fact queries reported Agent Lean.
- Source HEAD: `44cce94dc296d47c4d9e2e7009f19500b0bf7f6f`, with substantial pre-existing modified/untracked work. HEAD does not identify this dirty tree. The file-hash verification below identifies checked live bytes.
- There is no top-level `src/`. Source scenarios use `crates/*/src/` and `packages/vscode/src/`; every request has an explicit project and literal path scope.
- Independent calls ran in batches of at most four. Continuations and conditional reads followed their predecessors. No UI proposals, indexing, repairs, source edits, commits, pushes, or publication actions ran.
- The saved ledger (local-only `artifacts/mcp-scenarios-2026-11-01/ledger.json`) retains tool names, exact arguments, response content including citations/validators, request IDs, elapsed milliseconds, and serialized response sizes. Entries are recorded in completion order; they are not a cold-start replay schedule.
- 7 additional setup/probe MCP calls are outside the scenario ledger: status, projects, file_info, and four exploratory calls. The four exploratory responses were lost to a harness logging error after the calls returned; their outcomes are unretained. These four were subsequently run as ledgered scenario calls. Thus 80 MCP invocations occurred in this task, not only the 73 retained scenario calls. A later display-only helper error left its already-recorded results intact and caused no replay.
- Sizes count UTF-8 JSON serialization of the returned tool-result object. They exclude discovery, request bytes, transport framing and model tokens, so **they are not wire-byte or cost measurements**. `max_bytes` bounds records under existing contracts, not necessarily the complete envelope.
- Timing is a single observation per call, including concurrent waiting and tool wrapper overhead. Sum-of-call times below overlap; they are not whole-task wall time or latency distributions. No historical speedup, median/p95, or cold/warm comparison is claimed.

## Twenty scenarios

| ID | Scoped use case and recommendation | Scope | AIRK tools/workflow | Observed finding | Assessment |
|---|---|---|---|---|---|
| D1 | Find documentation before opening it (§5.7) | `docs/` | list_files → outline | 12 inventory paths; the index outline returned 8 facts and a cursor. Both respected scope. Partial inventory/outline was deliberate; no corpus-completeness claim. | Observed |
| D2 | Separate the historical investigation from the plan (Phase 0/1) | `docs/mcp-recommendations.md` | document_facts → continuation → read_file | 25 heading records over 2 pages match 25 live Markdown headings. Reading lines 185–217 confirms the implementation section is a proposal, not a measured delivery claim. | Pass |
| D3 | Find admission guidance with conceptual search (§5.1/5.7) | `docs/ → docs/mcp-developer-guide.md` | search → narrower search → context | Broad lexical top 4 included historical release/service material. Narrowing reached current batching guidance at line 24, although its first hit discussed proposal admission. Context recovered the complete rules. Scope helps; ranking still needs reader judgment. | Observed |
| D4 | Use extraction metadata instead of filename guesses (§5.2) | `docs/mcp-recommendations.md` | file_info + search | file_info reports markdown/source/structured-markdown. Session with markdown + document returned 3 bounded hits; rust + document returned 0. Positive and negative filters behaved as expected. | Pass |
| D5 | Audit textual mentions with paging and file summaries (§5.3) | `docs/mcp-recommendations.md` | references → 4 continuation pages → context | 42 unique matching lines over 5 pages, exactly matching live rg -w. Page file counts are 10/10/10/10/2. No duplicated or missing boundary lines. Final query_complete remains false; scan completion is not a universal recall guarantee. | Pass with coverage limit |
| D6 | Exclude a noisy document explicitly (§5.2/Phase 3) | `docs/ excluding docs/mcp-recommendations.md` | search + references | Search returned 4 hits elsewhere under docs/. References with the selected file both included and excluded returned 0, with an empty file summary. Exclusion affected selection as expected. | Pass |
| D7 | Read document structure through continuation (§5.7) | `docs/mcp-recommendations.md` | outline → continuation → read_file | 50 facts over 4 pages (15/15/15/5), including 25 headings and other document cues. Read the final delivery section at 272–278. Outline is evidence, and these fact records are not 50 sections. | Pass |
| D8 | Reuse an unchanged excerpt safely (§5.8) | `docs/mcp-recommendations.md:144–152` | read_file → conditional read_file + file_info | Initial excerpt was 1,837 serialized response bytes; identical if_evidence returned not_modified with the same validator in 703 bytes. known_hash file_info still returned content metadata. These checks validate indexed evidence, not live freshness. | Pass |
| D9 | Compare terse/full/explicit-status evidence (§5.6) | `docs/mcp-recommendations.md:144–152` | read_file in 3 modes + status | Same excerpt/validator: terse 1,837 bytes, full 3,691 bytes, include_status 2,848 bytes. Later serial status reported health=ok and freshness=idle, still live_source_verified=false. Counts are serialized tool-result bytes, not transport wire bytes. | Pass |
| D10 | Check a four-call documentation batch and retrieval modes (§5.1/5.2) | `docs/mcp-recommendations.md` | status + lexical/vector/hybrid search concurrently | All three searches returned 3 scoped hits with different ranking. Status failed with status_observation_superseded (query_submitted=true; fresh_brokerage_required=true). Batch wall time 1357 ms. Submitted status was not replayed. | Failure observed |
| S1 | Investigate one known type using the recommended route (§5.7/5.8) | `crates/airk-mcp/src/response.rs` | symbols → references → read_file | 2 definition records (struct + impl), 11 textual matching lines, and a hashed excerpt. All 11 lines match live rg. The struct is currently at line 94; the historical record says line 90. | Pass |
| S2 | Discover Session variants deliberately (§5.4) | `packages/vscode/src/` | symbols exact/suffix + code_facts contains with continuation | Exact Session: 0 records. Suffix: 7 records in 5 files, including functions as well as types. Contains: 88 unique definition records over 5 pages; qualified-name matches include nested variables such as useSearchSession::listener. Broad matching needs a tighter scope for a type-only question. | Pass with breadth limit |
| S3 | Restrict source concepts by language and evidence kind (§5.2) | `crates/airk-mcp/src/` | file_info + search | remote.rs reports rust/source/syntax. admission with rust + code returned 4 scoped hits, including test/help text inside Rust. rust + document returned 0. The code category does not exclude source comments or embedded prose. | Pass |
| S4 | Compare paged source references with a live oracle (§5.3) | `crates/airk-cli/src/` | references → continuation + symbols | 26 unique matching lines in 5 files over 4 pages (8/8/8/2); exact match to live rg -w. Includes comments and string literals. symbols found 9 definition records across 3 files. These are different count units. | Pass with coverage limit |
| S5 | Navigate a type and its written constructor target (§5.5) | `crates/airk-cli/src/embedding_client.rs; embedding_pool_client.rs` | rich code_facts → relationships incoming/outgoing → written-target relationships | Exact embedding-client struct ID produced 0 edges, no suggested_targets, truncated=true in both directions. Separately, written Session::new in embedding_pool_client.rs returned 4 syntax_candidate calls at 219/229/239/242, matching live source. Those calls belong to the separate pooled Session type; they do not resolve to the embedding-client struct. | Navigation gap |
| S6 | Distinguish a trait from similarly named wrappers (§5.4/5.8) | `crates/airk-retrieval/src/embedding_worker.rs; crates/airk-cli/src/embedding_client.rs` | code_facts + imports | RemoteSession trait at 99–101; one grouped use declaration imports it at 7–9. Scoping distinguishes the trait from unrelated wrappers, without claiming transitive or resolved dependencies. | Pass |
| S7 | Compare file-order outline with direct definition lookup (§5.7) | `crates/airk-mcp/src/response.rs` | outline + symbols | First 12 outline records contain imports/early calls and no Session; it returns a cursor. Direct symbols returns the struct and impl immediately. Partial outline was deliberate; no absence claim was made. | Pass |
| S8 | Check identifier definition pointers without changing search semantics (§5.2/Phase 5) | `crates/airk-cli/src/embedding_client.rs` | search → read_file | Session lexical search returned 2 bounded snippets plus definitions metadata pointing to 4 exact native definition records with suggested_tool=symbols. Read the struct at 60–72. Search snippets alone did not provide the complete declaration. | Pass |
| S9 | Preserve honest counts under small budgets (§5.4/Phase 4) | `crates/airk-cli/src/` | symbols → continuation + code_facts | limit=2/max_bytes=2000 returns 2 records, a cursor, and lower_bound=3 with unknown total in both tools. All 5 symbols pages yield 9 unique records, agreeing with S4's complete single-page lookup. Final paged metadata remains page-scoped rather than inventing a cumulative total. | Pass |
| S10 | Observe status alongside three source reads (§5.1) | `crates/airk-mcp/src/ and response.rs` | new status observation + symbols/references/read_file concurrently | All 4 calls succeeded in 948 ms wall time. Evidence agrees with S1; status is a later observation, not a replay of D10's submitted request. One successful batch does not explain or erase D10's failure. | Pass for this batch |

“Pass” means the stated check held in this run. “Observed” records a deliberately bounded investigation. Coverage limits remain authoritative even when a live oracle happens to match.

## Live verification

At `2026-11-01T19:07:06.1901302Z`, SHA-256 values of all **28 distinct returned evidence files** matched indexed content hashes (285 checked hash occurrences, zero mismatches). This is an external live-byte check of those files at that time; AIRK continued to report `live_source_verified=false`. Initial lightweight status had health unknown/freshness unavailable; later detailed evidence and serial status reported idle freshness and health ok. Snapshot availability and service health are separate observations.

Live comparison commands included:

```powershell
rg -n -w Session docs/mcp-recommendations.md
rg --json -w Session crates/airk-cli/src/
rg -n -w Session crates/airk-mcp/src/response.rs
rg -n 'Session::new' crates/airk-cli/src/embedding_pool_client.rs
rg '^#{1,6} ' docs/mcp-recommendations.md
Get-FileHash -Algorithm SHA256 -LiteralPath <each returned evidence file>
```

Exact set comparisons established:

| Check | AIRK | Live oracle | Missing / extra / duplicates |
|---|---:|---:|---|
| Recommendations Session matching lines, all reference pages | 42 | 42 | 0 / 0 / 0 |
| CLI src Session matching lines, all reference pages | 26 in 5 files | 26 in 5 files | 0 / 0 / 0 |
| response.rs Session matching lines | 11 | 11 | 0 / 0 / 0 |
| Recommendations Markdown headings | 25 | 25 | 0 / 0 / 0 |
| Pooled Session::new written calls | 4 | 4 | 0 / 0 / 0 |

These are matching **lines**, not identifier occurrences. Reference page summaries describe their own page. The final `candidate_scan_complete=true`/`more=false` responses still report `query_complete=false`, `token_boundary_complete=false`, and unknown query-wide totals. This run verifies these scopes, not arbitrary textual completeness.

## Findings that need attention

1. **Mixed-load status is not uniformly successful.** D10's exact error was:
   `{"code":"status_observation_superseded","fresh_brokerage_required":true,"query_submitted":true,"retryable":true}`.
   All three sibling searches succeeded. There were no retained busy refusals. S10 and the later serial observation succeeded. No worker-side logs, cancellation trace or runtime provenance were collected, so the cause and reproducibility remain unknown. Do not treat this as a missing index or automatically replay submitted/uncertain work.
2. **The original struct-navigation case is still not answered by struct ID here.** Both directions for the embedding-client Session return empty, truncated evidence and no suggested targets. Written-target calls work, but the pooled Session is a separate type. Current live relationship source contains bounded same-file navigation logic; this run does not establish that it is unimplemented, nor distinguish retained-fact limits, admission/work bounds, or a runtime implementation issue.
3. **Broad name matching can become expensive to consume.** S2's contains lookup includes qualified-name descendants, giving 88 definition records and 61,468 response bytes for its five code_facts pages. The suffix result is 7 records, not 7 types; exact name is 0. Tighten the file/type scope and inspect kinds/signatures before interpreting counts.
4. **Conceptual search still benefits from narrowing.** D3 broad admission results included release history, and the narrow top hit concerned UI proposal admission. Exact file context was needed for runtime batching guidance. Definition pointers in S8 are useful supplements, not evidence that search snippets alone answered a type question.
5. **The dated recommendation record is not current implementation guidance.** Its summary says “resolved callers,” while the detailed log and current tool evidence say `syntax_candidate`. Phase 0 still describes the old 22-call/five-refusal summary although the summary now says 28/eight. The status banner correctly calls the appended plan proposed work. These internal historical inconsistencies were observed and left unchanged.

## Per-scenario measurements

| Scenario | Calls | Tool errors | Serialized response bytes | Sum of call elapsed ms (overlapping) |
|---|---:|---:|---:|---:|
| D1 | 2 | 0 | 7,410 | 1485 |
| D2 | 3 | 0 | 22,177 | 1945 |
| D3 | 3 | 0 | 10,325 | 1231 |
| D4 | 3 | 0 | 5,167 | 1636 |
| D5 | 6 | 0 | 30,607 | 4093 |
| D6 | 2 | 0 | 5,436 | 1481 |
| D7 | 5 | 0 | 23,313 | 3946 |
| D8 | 3 | 0 | 3,911 | 903 |
| D9 | 4 | 0 | 9,868 | 2183 |
| D10 | 4 | 1 | 8,416 | 3138 |
| S1 | 3 | 0 | 7,872 | 2518 |
| S2 | 7 | 0 | 64,914 | 5492 |
| S3 | 3 | 0 | 6,869 | 1346 |
| S4 | 5 | 0 | 16,700 | 4749 |
| S5 | 4 | 0 | 7,811 | 2307 |
| S6 | 2 | 0 | 2,584 | 819 |
| S7 | 2 | 0 | 5,112 | 1074 |
| S8 | 2 | 0 | 5,041 | 1067 |
| S9 | 6 | 0 | 9,902 | 2360 |
| S10 | 4 | 0 | 8,912 | 2703 |
| Total | 73 | 1 | 262,347 | 46476 |

D10 four-call wall time: 1357 ms; S10: 948 ms. They are different workloads and do not constitute a controlled comparison. The record budget and output envelope are separate; for example S9 code_facts reported 2 records within its 2,000-byte record budget while the serialized result was 2,116 bytes.

## Validation and limits

Artifact assertions passed: 20 scenarios use at least two distinct tools, 73 unique call IDs, the recorded error count, exact live reference sets, 25 unique headings, 50 outline facts, unique S2/S9 paging, consistent publication and matching hashes. Artifact whitespace and `git diff --check` also passed; Git emitted existing CRLF-normalization warnings.

The audit checks every scenario has at least two distinct tools, retained publication identity is consistent, paging contains no duplicate boundary records in checked scenarios, the selected live reference/heading sets agree, and all returned-file hashes agree. Exact arguments and observed errors are preserved in the linked JSON ledger.

No Rust/TypeScript implementation was changed. Cargo formatting/Clippy/workspace tests, extension tests, repository-wide checks, Linux/macOS qualification, concurrency 6/8 overload, cold launches, cancellation/EOF, project-isolation adversarial fixtures, Unicode/overload fixtures, expired-publication rejection, and UI integration were **not run**. They remain separate qualification work. No commit/PR/version change was made; pre-existing user changes were preserved.
