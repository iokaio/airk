# AIRK 1.8.3 — bounded MCP and Workbench navigation

**Historical local candidate:** the owner requested a distinct [1.8.4 candidate](release-1.8.4.md) on 2026-11-01 after replacement of repeated 1.8.3 builds remained uncertain. This record retains its original implementation, artifact receipts and failed installed-retest observations; current build and qualification evidence belongs to 1.8.4.

Development record, 2026-11-01, on `feat/mcpoptimizations`. This candidate follows the [1.8.2 responsiveness and Workbench record](release-1.8.2.md). Native, extension and lockfile version synchronization is part of the branch validation. Publication, exact artifact acceptance and measured performance are separate gates; this record does not claim a published version or complete platform qualification.

## Choose the right evidence operation

Server instructions and tool descriptions now distinguish compact definition discovery, scoped textual references, exact source expansion and conceptual search. `outline` reads one indexed file's structure in source order. A `suggest_view` route named `code/outline` proposes an editor page and returns no source evidence. Four concurrent bridge handlers remain a cap, not a guarantee that downstream setup/admission accepts a four-call batch.

The [MCP developer guide](../mcp-developer-guide.md) and [CLI developer guide](../cli-developer-guide.md) retain reciprocal navigation, catalog distinctions, examples, implementation ownership and explicit evidence limits. `symbols` and `code_facts` share native fact retrieval; cost depends on scope and representation rather than the tool name alone.

## Absorb brief contention without replaying uncertain work

The bridge reserves one exact status lane alongside three ordinary lanes. Ordinary waiting is capped separately at eight callers and two seconds; simultaneous status callers can immediately borrow idle ordinary capacity to reach the existing coalesced status observation. Status never waits in the ordinary queue before its separate cold-preparation/observation budgets. Evidence-channel serialization and native worker admission have their own bounded waits, which can accumulate rather than forming one global RPC deadline. The worker still executes one native query at a time.

Typed unsubmitted refusals remain distinct from submitted/uncertain work. Scope and authorization are rechecked after waiting; cancellation, peer EOF, supersession and Stop/drain fence admission. Accepted work retains ownership through real completion or drain. Existing status cache age, readiness/freshness distinctions, integrity checks and source read-only behavior remain authoritative.

## Filter and inspect names explicitly

Search and references accept indexed extraction `language`, `evidence_kind` (`code`, `document`, `configuration`, `unknown`) and one literal `exclude_path_prefix`. Saved extraction metadata governs classification; unknown or mixed extraction is not guessed from a filename. Filters constrain candidates before lexical and vector top-k selection under finite roster/work bounds. They do not change saved indexing policy or authorization.

Symbols/code facts keep exact matching by default. Explicit `name_match: suffix|contains` compares simple or qualified names case-sensitively; opaque symbol IDs are not substring targets. Existing imports retain their preceding omitted-mode substring behavior. Count requests report precision and coverage: a partial bounded scan exposes a lower bound/unknown total rather than pretending to count every match. Durable continuity-key lookup remains exact and excludes textual matching/count options.

## Page references and preserve uncertainty

New reference selectors opt into deterministic continuation with the same identifier, publication, filters, representation and ordering. Paged `index` means stable-ID order; `path` and `code_first` are explicit alternatives. Omitting every new selector retains the preceding selection and response contract. File summaries count returned-page records in the native `count_unit` and have an additional metadata allowance of 16 rows/4 KiB beyond the text budget. A complete source line with two identifier occurrences still contributes one matching line.

UTF-8 source results reconstruct complete physical matching lines across retained chunk splits, preserving CRLF and original coordinates. Non-UTF-8 extracted fragments whose original-line completeness cannot be verified remain explicitly qualified, and `count_unit` distinguishes lines, fragments or mixed records. A record that cannot fit the selected text budget can return an empty page, exact required bytes and a resumable cursor; byte/count limits can increase within supported bounds without changing query identity. A record beyond the absolute maximum or an artifact beyond finite scan/deduplication capacity instead reports terminal incomplete coverage. Cooperative scan/deduplication turns have resumable limits too, disclosed by page reason. Source hashes, exact revision identity and explicit truncation remain attached.

**Whole-query reference totals remain unknown.** An identifier itself can straddle index chunk tokenization and therefore be absent from lexical postings. Completing the candidate scan, including finding zero candidates, does not prove exhaustive textual coverage. `page.coverage` declares `query_complete: false`, `query_total_matches: null`, `token_boundary_complete: false` and line-completeness qualification. Exact counts are limited to `verified_lexical_candidates`; incomplete scan counts are `page_scan` lower bounds. Workbench shows this warning alongside counts, and exports retain it. Completeness-sensitive investigations still require live verification or further scoped source reads. This candidate introduces no index-format change, mandatory reindex or unbounded fallback scan to conceal that limitation.

## Find useful follow-up targets

Search can add bounded same-publication definition hints with cited candidate pointers and count precision. Pointer lists are independently bounded; a verified record count does not promise a pointer for every declaration or establish distinct type counts. Search ranking and lexical AND semantics remain unchanged.

For exact supported Rust type/impl IDs, retained same-file owner/scope evidence can expose associated method targets and qualified containment. Duplicate names, missing evidence, cross-file impls and aliases do not become resolved type associations. Incoming call candidates still use written targets such as `Owner::spawn`. No complete call graph, compiler-resolved reference or executed coverage claim follows from these candidates.

## Use the improvements directly in Workbench

Open the composer **+ → Tool forms**. Operation schemas expose optional filters and matching/order choices as selections, booleans as Default/Yes/No, and budgets as validated whole-number inputs. Omitted controls stay omitted, preserving existing defaults and saved-plan compatibility. Catalog input limits are retained, including larger continuation tokens. The same options also work through supported slash flags such as `/code-facts`, `/search --language rust`, `/symbols --name-match suffix` and `/refs --include-files --include-count`.

**Compact results → Yes** or `--compact true` provides compact declaration/fact discovery before a chosen expansion. Omission preserves Workbench's preceding rich-result default; native MCP compact/minimal defaults remain separate.

Each session retains its selected tool and independent per-tool field values, plus shared Scope/Recipe choices. Closing menus and switching tools preserves the draft; reopening saved history restores that session's preferences. The native-family **Reset this tool form** icon resets only the active fields while retaining the operation, other forms and shared context. New, Clear and Fork start with Search and omitted/default preferences. The unchanged omitted defaults include 10 results, 8 KiB, exact symbols/code-facts matching, count/file summaries off and Workbench compact off. Persistence follows the existing session privacy setting. Project/session/epoch/revision fences and scalar acknowledgments prevent stale drafts from overwriting newer choices; editing/resetting never starts a native query.

Structured results show qualified record/matching-line counts, bounded per-file summaries, incomplete reasons, definition pointers and written method targets. **Inspect** and **Find callers** are explicit user actions. Their host-side validation requires an executed, completed source step, the exact retained publication, the original narrower scope and a retained target/pointer; failed or uncertain results cannot authorize a follow-up. Definition inspection uses its exact symbol identity and exact path. A successful step in a partial multi-step response remains usable when those guards hold.

Literal revision-bound syntax/provider IDs stay tied to that immediate investigation. Remember, reusable strategy saving and re-run reject stale literal-ID plans rather than stripping publication identity and silently reusing an offset-based ID. Portable names/paths/durable selectors and freshly bound current IDs remain the supported reusable inputs.

Prepare/Execute requires the original publication even for pasted/raw-text literal IDs; preparing a request cannot bless an old ID with today's token, so stored values are preserved while retained **Inspect** or a fresh name/current binding supplies a valid investigation.

**Fetch next page** understands nested reference cursors and retains original selectors. If a complete record needs a larger byte budget, its required size is shown and an explicit continuation can enlarge the per-step allowance only within the original remaining aggregate budget and supported maximum. Terminal limits stay visible rather than producing an endless empty-page loop.

JSON exports retain full native results. Markdown exports retain discovery counts, summaries, pointers, qualifiers and continuation metadata alongside evidence cards. Selected copies preserve minimal source qualification for selected step IDs in `source_coverage`, labeled `original_source_request`, while complete summaries/pointers/counts remain in full exports. Selection does not rewrite citation/publication identity. Scalar status/save updates keep their preceding ownership and do not replace retained result bodies. Existing sources, normal sessions and indexes are not modified by UI proposal or display changes.

## Status observation follow-up — 2026-11-01

In the 1.8.3 local candidate, a completed status query that overlaps ordinary scalar-cache invalidation returns its original snapshot successfully with `status_observation.superseded: true`, `reason: scalar_invalidation` and an explanatory message. Original publication and observation age are retained. The superseded snapshot does not repopulate the current cache or acknowledge an obsolete status receipt; the query is not replayed. Project/namespace changes, authenticated worker replacement, expired source authorization and shutdown retain their existing rejection behavior. Cached reuse still has its five-second limit, and the bridge retains three general executing slots plus one reserved status slot.

The owner requested this policy after reviewing the [20-scenario investigation](../mcp-improvements-test-results.md). Its historical status error remains recorded as observed; ordinary scalar supersession is now treated as informational. Version 1.8.3 is retained under the owner's explicit instruction because it has not been published. A rebuilt local artifact must be identified by its new digest and receipt, and local installed executables must be replaced and verified. Prior artifact hashes below identify the earlier build.

## Developer-directory permission follow-up — 2026-11-01

At the owner's request, normal read/execute sharing no longer disqualifies the Windows native write boundary. Standard creator-owner inheritance templates and partial SYSTEM/Administrators grants are accepted while retaining current-user ownership and its full-control propagating grant. Foreign write/delete/ACL/ownership permissions still select strict verified reads; this is not a promise of confidentiality for read-shared directories. Source read-only behavior, configured write roots, path redirection checks, corruption detection and project isolation are unchanged. Positive and negative ACL tests distinguish the native write boundary from stricter per-object privacy. Three Windows/Linux-only test helpers now carry matching platform guards so macOS Clippy does not compile unused fixtures. Initial failing results are retained and final reruns are recorded separately.

## Validation and remaining qualification

**2026-11-01 replacement installed-build retest: release remains on hold.** Repeating the earlier D10/S10 mixed workloads three times each produced 3 typed `status_observation_superseded` errors in 24 calls; all 18 sibling evidence calls succeeded. The informational supersession note was not observed. Current-checkout status regressions passed 21 tests and Windows write-boundary regressions passed 5 with 1 explicit remote-fixture skip, but the installed source digest differs from the current checkout. These local passes do not qualify that installed artifact. Struct-ID navigation remained empty/truncated in the unchanged S5 control. See the [retest evidence and exact provenance](../mcp-improvements-test-results.md#replacement-installed-build-retest--2026-11-01). Diagnose the installed failures and reconcile source/artifact identity before another candidate; no new version or release was created by this retest.

The pre-installer review on 2026-11-01 compared the implemented contracts, adapters, tests and documentation with the recommendation plan. Local evidence includes:

| Check | Observed result |
| --- | --- |
| Extension compile and complete unit suite | 534 passed, 18 skipped |
| Native Workbench tests, Windows and WSL Ubuntu | 12 passed, 1 explicitly ignored on each host |
| Reference regression tests, Windows | 10 integration and 4 unit tests passed |
| Rust type navigation, Windows | 2 integration and 6 helper tests passed |
| Python repository/CI/packaging contracts | 285 passed, 6 skipped |
| Generated CLI reference | Both checks passed |
| Workspace formatting and Windows all-feature/all-target Clippy | Passed |
| WSL Ubuntu all-feature workspace Clippy, tests and doctests | Passed; final publication-guard change additionally passed the focused Workbench rerun |
| Version synchronization and public repository checks | Passed; native/extension/lockfiles agree on 1.8.3 |
| Local Windows x64 release pair, real-model retrieval and VSIX packaging | Passed; package inventories, hashes, versions and runtime self-test verified |

Independent host tests and actual compiled-browser interactions cover session restoration, tool switching, project isolation, fresh-session defaults, keyboard reset, stale acknowledgments and absence of query side effects. The final symbol-ID regression deliberately replaces a declaration with an equal-length name that reuses its syntax ID: unfenced preparation/execution stays inert, retained Inspect works under its original publication, and changed publications reject stale evidence. The full WSL workspace run completed successfully; its compiled snapshot preceded the final publication-guard addition, which passed a separate focused Workbench rerun. Local results do not substitute for hosted architecture checks.

The measurement harness and frozen fictional fixture are prepared, but the matched 1.8.2/1.8.3 launcher campaign has not run. Cold/unprimed and reused-connection cohorts, concurrency 1/4/6/8, task completion, errors/retries, serialized bytes, coverage and latency distributions remain to be measured against an authenticated local 1.8.3 runtime. Actual-agent routing trials and installed-editor responsiveness are separate outstanding measurements. No speedup percentage, model-turn reduction or universal token/time saving is established by these correctness tests.

The local Windows x64 installer was built on 2026-11-01 from the uncommitted development checkout: `airk-1.8.3-win32-x64.vsix`, 70,767,875 bytes, SHA-256 `48addf677b6cd104ebb880f4069acbfce3e0cbd9f70b77dba53522a8e0341587`. The build reused the completed routine checks instead of repeating them; its receipt records `checksSkipped: true` and `editorTests: false`. Fresh release compilation, real helper inference/default-vector-hybrid retrieval, native archive self-test and native/VSIX payload verification still ran and passed. The 763-entry VSIX excludes the private recommendation plan and private/index state. This is a local test candidate, not a published or installed-editor-qualified release.

Before accepting the installer, the candidate still needs four-processor repeated installed-process/client smokes and a manual installed Workbench check. The matched performance campaign and required hosted native/container gates remain outstanding before merge/publication. Whole-query reference completeness across lexical token seams remains a disclosed limitation, search ranking remains unchanged, and connection-only evidence compression remains deferred. Response-local status suppression retains exact tokens and original locations. Publication and release workflows remain on hold.
