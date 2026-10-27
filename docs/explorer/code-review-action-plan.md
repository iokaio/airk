# AIRK Explorer branch code review and action plan

Review date: 26 October 2026. The original review below is historical. The implementation follow-up at the end records the corrective work and its separate validation; original findings and original test receipts are retained for traceability.

## Scope and method

- Branch: `feat/airkexplorer`.
- Base: `217f226b05b5a6ed9ca529a99b290b02dd2fcb5d` (`main`, also the merge base).
- Reviewed tip: `8d0fc802029e98a9f2da7ab27270a4a0acbd1a0e`.
- Commit range: `main..HEAD`, 33 commits, examined in chronological ancestry order with `git log --reverse` and individual Git snapshots/diffs. The combined branch diff contains 194 files, including documentation, fixtures and removed legacy modules.
- Review covered branch changes to native usage attribution and process forwarding, extension integration, host contracts/controllers, webview screens, Assistance, Strategies, Planner, Suggestions, tests, build/packaging and CI selection. Unchanged kernel code was consulted where needed to verify contracts. This is not a review of every pre-existing line inherited from main.
- Read repository guidance, README, CONTRIBUTING, architecture, Explorer design and affected implementations. Followed call paths from webview messages through the host to native adapters and checked relevant tests. Findings were checked against the final implementation so fixes in later commits are not reported as still open.
- AIRK MCP was consulted first. It reported generation 656, revision `airk-r1:19ae18e4a4de52ae6554dac618c4c032:656:7a47ea1d2e4c04390c7248fe63e7beeed9f467f3b916cda39536bbd17aba5b9b`, but freshness was unavailable (`source_watch_root_unavailable`, `live_source_verified: false`). Git snapshots and live source were therefore authoritative for this review. Git commands used a scoped safe-directory override; no global Git settings were changed.
- Existing uncommitted universal-launcher work was preserved and inspected separately. It is not a 34th commit and has not been attributed to the committed branch. Validation ran on the current worktree, including that work, rather than on a clean checkout of the tip.
- Single-reviewer pass, with an explicit final recheck of findings against source and test results. No independent second reviewer or delegated review is claimed.

## Results

There are **11 open findings: three P1, seven P2 and one P3**. P1 means address before release; P2 means a functional correctness issue to fix in this feature; P3 means documentation consistency. Static findings below describe executable paths visible in source; they are distinguished from the three cases additionally reproduced with scripted calls to compiled modules.

The component boundaries are generally clear: native Retrieval remains read-only, Assistance submits inert drafts, Management reuses the existing controller and receipts, and the build restricts bundle inputs. Those boundaries do not eliminate the lifecycle, save and provenance issues below. Passing mock tests does not establish installed-platform qualification.

## Chronological commit review

“No additional finding” means no separate actionable defect was identified in that patch; it is not a certification that the commit is defect-free. References F01–F11 identify findings that remain open at the reviewed tip.

| # | Commit | Change and review result |
|---|---|---|
| 1 | `6915f19` | Design inputs and approved Explorer design. Reviewed as requirements and historical design material, including the prototype rather than treating it as shipped extension code. Preserve those distinctions. Later implementation/status text needs F11. |
| 2 | `8cfa461` | Preact/esbuild toolchain, isolated webview typecheck, packaging allowlist and license banner. Checked bundle/build contracts and corresponding packaging tests. No additional finding. Early scaffolding becomes runnable through later commits. |
| 3 | `0e544ca` | Explorer client family, worker attribution, report labels and native integration test. Exact reserved client label and append-only enum ordering are deliberate. The documented 1.4 ledger rollback incompatibility remains a qualification concern, not a newly discovered defect. |
| 4 | `7727492` | Reusable stdio MCP session and probe extraction. Checked handshake, request IDs, late-reply discard, error/exit handling, budgets and session tests. No additional finding; bounded request admission remains a hardening recommendation below. |
| 5 | `a3d2861` | Route/feature/error registries and strict message contracts. Closed message schemas and host-issued handles are useful safeguards. No additional finding in the initial contract patch. |
| 6 | `12ee113` | Persistent evidence client and shared control facade. Checked tool allowlist, queueing, supersession, retry hints, lazy reconnect and console retention. No additional finding specific to this patch. |
| 7 | `f8011aa` | Panel shell, Overview, sidebar, status and controller integration. F05: a status request is shared across project changes. F06: screen state lacks a project boundary. Later registration publication fixes do not resolve either issue. |
| 8 | `76cb793` | Search, citation handles, expansion and live-checked source opening. F04: expansion loses revision/hash binding and can request lines past EOF. F06 also affects retained Search results after project selection. |
| 9 | `1fbbec3` | Management Projects, cached values and journaled Index now. Checked page membership, cached labels and request acceptance wording. No additional finding; global-project selection usability remains a recommendation. |
| 10 | `9885a94` | Code facts, references, candidate relationships, outlines and cursors. Checked compact/rich parsing, opaque symbol/cursor handles and continuation publication comparisons. F06 extends to these retained lists. |
| 11 | `d005945` | Document fact filters and original-location labels. Checked native arguments and location mapping. No additional finding specific to this patch. |
| 12 | `c70b955` | Inspector, citation Checker and indexed Changes. The unchecked-as-valid issue was subsequently fixed by `9152863`. F07 remains relevant to multi-call publication consistency and classification of lookup failures. Changes pagination is a follow-up improvement. |
| 13 | `bb2851b` | Versioned Markdown help compilation, Help UI and native help cross-references. Checked text-only rendering and catalog validation. No additional finding; drawer behavior differs from the design and is already recorded as a design difference. |
| 14 | `b27597a` | All Management tabs on the shared controller. Checked action/command allowlists, summaries, repair approvals, lifecycle and diagnostics paths against the existing controller. No additional finding. |
| 15 | `d60a289` | Command routing to Explorer Management, temporary console hiding. Checked routing and parity tests. The final removal in `9ba5c23` supersedes the temporary setting; F11 captures remaining current-text drift. |
| 16 | `72e48ee` | Installed-editor Search and project-row checks. Useful smoke coverage, but does not exercise project switching, delayed results, redirected plan saves or concurrent fan-out failure. Later harness adjustment is recorded in commit 17. |
| 17 | `633032e` | Atomic registration publication, friendly names, trust UI and Explorer-driven editor harness. Corrects the transient empty-registration behavior. F05 and F06 survive because they concern in-flight status and retained view state, respectively. |
| 18 | `ecb0f49` | Native descriptor forwarder and shared inherited-stdio process handoff. Checked descriptor schema, bounds, digest/path checks, Unix checks, environment removal, Windows containment and native tests. No additional confirmed finding. Actual Unix and installed-runtime behavior was not executed in this review. |
| 19 | `fd4a543` | Assistance help/tools/resources server and separate bundle. Checked protocol dispatch, strict arguments, output budgets, text-only help and bundle boundary tests. No additional confirmed finding; queue backpressure deserves hardening. |
| 20 | `392b29d` | Trusted-window endpoint, rotating descriptors and launch descriptor integration. Crashed-window ambiguity and failed rotation were subsequently fixed by `9152863`. Native attachment-directory forwarding is present in the final implementation. No additional confirmed finding. |
| 21 | `7d286bf` | Second agent connection and Assistance verification. Checked generated client formats, conflict-preserving JSON merge, Codex creation, provider definitions and probe identity/inventory checks. No additional finding. Real agent interoperability is unqualified. |
| 22 | `5f0f409` | Strategy/run/plan/recipe schemas, template pack and prompt compiler. F10: byte limits are measured with UTF-16 string lengths. The later leading-status validation resolves the original preflight omission. Preview templates remain explicitly labeled. |
| 23 | `d1586cd` | Library/Studio and pinned-publication runner. F02: delayed run completion can attach to edited/replaced drafts, and edits can retain old compiled prompts. F03: a failed fan-out does not settle its sibling workers. F10 also affects evidence clipping. |
| 24 | `9561c2a` | Planner, save and one-task handoff. F01: save containment is lexical and guards are stale after awaited dialogs. F09: Studio prompts are silently cut to 8,000 characters. Revision checks on asynchronous Planner saves are useful but do not protect the host write itself. |
| 25 | `eca7950` | Workspace-state Search recipes. Checked schema revalidation, count limit and inert save/delete behavior. No additional finding specific to recipes; project-scoped display remains F06. |
| 26 | `316c048` | Inert draft contracts, validation, idempotency and volatile Suggestions store. Checked count/byte/rate/TTL limits, explicit statuses and conflict checks. No additional finding specific to store admission. F08 concerns the later UI apply sequence. |
| 27 | `c839c10` | Draft delivery and proposal-status IPC. Checked double validation, credential handling, retry, receipts and self-reported client labeling. No additional finding beyond the later UI application behavior. |
| 28 | `04cd83d` | Suggestions Inbox, draft comparisons and application. F08: local drafts change before host acceptance of the resolution; cached or expired suggestions can still affect local state. Append overflow was subsequently fixed. |
| 29 | `371c237` | Git Changes, Impact and Tests views. F06: changing projects does not reload an already mounted review screen. F07: service failures become “not indexed” and multi-call reports can combine publications. Related deletion/rename and declaration completeness improvements are listed below. |
| 30 | `88201a7` | Guidance-only Review and Management suggestions. Fixed intent routes preserve the inert-draft boundary. No additional confirmed finding; proposed names are explanatory text rather than applied Review filters. |
| 31 | `9ba5c23` | Removes console view/renderer and unreachable legacy modules/tests. Checked remaining imports, packaged runtime closure, controller retention and management-based assertions. No additional functional finding. Current documentation still describing the hidden/available console is F11. |
| 32 | `9152863` | Review fixes. Verified changes for stale parameter filtering, mandatory status preflight, crashed-descriptor filtering, rotation rollback, unchecked citations, Git status enum, append capacity and listeners. These are historical fixes, not open findings. They do not address F01–F10. |
| 33 | `8d0fc80` | Version 1.5.0 and automatic 1.4-to-1.5 upgrade gate. Native/extension/lockfile versions agree. Release record explicitly preserves unqualified platform/upgrade cases. F11: README, architecture and final-design current-status statements still conflict with the retired console and current version. |

## Open findings and concrete corrections

### F01 — P1: Planner save can escape the workspace or overwrite a changed target

Introduced in `9561c2a`. Evidence: [explorerWorkspaceIO.ts](../../packages/vscode/src/explorerWorkspaceIO.ts), `savePlanInWorkspace`, lines 103–120; caller in [explorerController.ts](../../packages/vscode/src/explorerController.ts), `planner.saveInWorkspace`.

The target is checked using URI/path-relative containment only. A workspace directory junction or symlink pointing outside the root passes that check, and `workspace.fs.writeFile` follows it. The saved-file readback hash verifies what was written, not where it was written. In addition, dirty-document state is checked before the awaited replacement confirmation and is not rechecked before writing. A user can edit the target while the confirmation is open. A target created or replaced after inspection can also be overwritten without the reviewed identity being checked again. Workspace trust is checked before entering the dialog sequence, not at the final write.

**Correction:** resolve and verify the actual local root and destination/nearest existing parent, reject redirected or unsupported targets, and use a guarded save that revalidates trust, document version/dirty state and target identity immediately before mutation. Distinguish “does not exist” from other stat failures. For remote filesystem providers, define a supported containment/conditional-write contract or refuse writes that cannot meet it. Keep overwrite confirmation tied to the exact observed file.

**Acceptance:** test parent junction/symlink escape, linked target, dirtying during confirmation, concurrent target creation/replacement, denied stat and authorization revocation. Assert that unrelated files retain their bytes. This review established the defective path by static analysis; it did not execute a redirected filesystem write.

### F02 — P1: Studio can attach old evidence and prompts to a new draft

Introduced in `d1586cd`. Evidence: [strategies.tsx](../../packages/vscode/src/explorerWebview/strategies.tsx), lines 126–143 and 163–170; [strategyStore.ts](../../packages/vscode/src/explorerWebview/strategyStore.ts), revision tracking.

Run completion unconditionally stores its result into the current draft. While it is pending, the user can edit steps, parameters or scope, import another strategy, or accept an Inbox replacement. The completion callback uses the old strategy/objective closure while updating the new draft. Even without an in-flight run, changing objective/parameters/scope can leave the existing generated `prompt` and run attached; editing steps clears `run` but leaves `prompt`. A user can therefore copy or add a Planner task whose visible draft and compiled instructions disagree.

**Correction:** bind runs to a captured project, strategy/input digest and draft generation. Discard results when any binding changes. Invalidate run evidence for retrieval-input edits and invalidate/recompile generated prompts when their inputs change. Preserve manually edited prompts only with explicit stale-input labeling and a deliberate rebuild path.

**Acceptance:** delay a run, then change each input, switch project and replace the Studio draft. The old result must not populate the new draft. After a completed run, edit objective/steps/parameters/scope and verify that Copy and Add to Planner cannot silently export the prior generated prompt. Existing runner tests do not exercise this UI lifecycle.

### F03 — P1: Failed fan-out lets a strategy return while sibling work continues

Introduced in `d1586cd`. Evidence: [explorerStrategyRunner.ts](../../packages/vscode/src/explorerStrategyRunner.ts), `pool`, lines 100–103, step error handling and finalization at lines 235–263.

`Promise.all` rejects when the first worker fails, while sibling workers remain active and retain access to shared outputs, counters, reasons and ledger. The outer loop can move to later steps or finalize the run before those workers settle. A sibling can also continue taking additional fan-out items. Consequently step counts, output completeness, live checks and final evidence depend on timing rather than the declared step sequence.

**Correction:** stop admission of new fan-out items after failure, settle every started worker before leaving the step, and collect bounded errors/results deterministically. Use run/step cancellation state for subsequent calls. Finalize the manifest and digest only after no worker can mutate run state.

**Acceptance:** fail one of three concurrent reads immediately and delay the other two; include a queued fourth item and a publication change in a sibling. No later step or completed result may precede settlement of started work. A scripted reproduction against compiled modules confirmed that `runStrategy` returned with a sibling read still pending.

### F04 — P2: Expand silently changes evidence identity and fails near EOF

Introduced in `76cb793`. Evidence: [explorerController.ts](../../packages/vscode/src/explorerController.ts), `evidence.expand`, lines 284–290; [search.tsx](../../packages/vscode/src/explorerWebview/search.tsx), expanded text rendering; [airk-kernel/src/lib.rs](../../crates/airk-kernel/src/lib.rs), `read_snapshot`, line 574.

References retain a publication and content hash, but expansion sends neither `revision` nor `known_hash`. If a file is reindexed between Search and Expand, the host reads the new lines and the UI replaces the old excerpt beneath the original result's publication metadata. Separately, every expansion adds five lines after a hit without clamping to indexed length; native reads explicitly reject an end beyond the source. Expanding a short file or an EOF hit fails routinely. Extracted documents also need page/block/slide boundaries preserved.

**Correction:** use the reference's revision/hash validators and fail explicitly when the citation is stale. Use bounded native context or a pinned file-info lookup to respect EOF and original-document boundaries. Render expansion identity and stale errors explicitly; do not splice new-publication text into an old result card.

**Acceptance:** expand a one-line file, an EOF hit and a PDF/Office boundary; republish between Search and Expand and between any preflight/read calls. Return correctly bounded evidence from the requested identity or an explicit mismatch.

### F05 — P2: Switching projects can suppress the new project's status query

Introduced in `f8011aa`. Evidence: [explorerController.ts](../../packages/vscode/src/explorerController.ts), `select`, `specsChanged`, `observedControl` and `loadStatus`, especially lines 211–224.

`statusLoading` is shared across projects. If A's status is pending, selecting B clears status and disposes A's session, but B's `loadStatus()` returns A's promise. Its completion/failure clears the shared flag without starting B's query. Automatic refresh requires an existing status publication to differ from the cached control row, so an undefined B status need not recover until a manual refresh or reopen. Same-project binding replacement also needs a session generation check, not just equality of project strings.

**Correction:** scope pending status by project plus binding/session generation, and always schedule a load for the current selection after a superseded request settles. Clear or mark prior status stale when its binding changes.

**Acceptance:** delay A, select B and resolve or reject A. B must receive a status call without further user action. Repeat with an A→B→A sequence and runtime/storage rebinding. Scripted reproduction confirmed one call total and no `statusFor` for B after both selection promises settled.

### F06 — P2: Project selection changes the header but retains another project's screen data

Introduced in `f8011aa`, propagated by the evidence and Review screens. Evidence: [workspaces.tsx](../../packages/vscode/src/explorerWebview/workspaces.tsx), line 80; [search.tsx](../../packages/vscode/src/explorerWebview/search.tsx), result state; [review.tsx](../../packages/vscode/src/explorerWebview/review.tsx), `useReview`, lines 15–25.

The active screen is mounted without a project key. Search and fact components keep their results in local state, and Review reloads only when the connection boolean changes. Selecting B while both projects are ready leaves A's data beneath B's header. The host clears citation handles, so actions on those displayed results then fail as unknown references. Late replies can also repopulate a screen after selection changes. Stored Studio/Planner evidence needs an explicit project association as well.

**Correction:** key evidence views and pending work by selected project/binding generation, clear or visibly retain prior-project evidence with its owner, and discard late results that no longer match. Scope publication pickers and Inspector handles similarly. Keep drafts only with explicit project provenance and stale-state handling.

**Acceptance:** search, inspect, load facts and run all three Review tabs in A; select B without changing connection readiness. A's results must not appear as B's. Resolve an A request after switching and verify it is ignored.

### F07 — P2: Review and Checker need truthful failure and publication contracts

Introduced in `c70b955` and `371c237`. Evidence: [explorerReview.ts](../../packages/vscode/src/explorerReview.ts), lines 81–82, 98–122 and 133–159; [explorerInspect.ts](../../packages/vscode/src/explorerInspect.ts), `checkCitations`.

Review Changes catches every file-info error and reports the file as not indexed. An outage, timeout, admission exhaustion or corrupted index therefore becomes a false absence claim; the UI maps `unchecked` to “not indexed” even when a hash exists but comparison was unavailable. Review declarations/reference calls keep only the first publication or discard publication identity, so a background republish can produce one report containing facts from different generations. Checker similarly keeps the first publication across per-path lookups, and treats most non-connection tool failures as not found. The later unchecked-status correction did not fix these paths.

**Correction:** distinguish explicit indexed absence from unavailable/failed lookup and unavailable live comparison. Pin or compare publication identity on every constituent evidence read, including references; reject a mixed report or label per-result identities and partiality explicitly. Preserve useful failure codes. Do not infer absence from generic `TOOL_ERROR`.

**Acceptance:** inject unavailable, timeout, busy and integrity errors into a middle lookup; assert that none becomes “not indexed/not found.” Change publication between two paths and between declarations and references. The report must fail/restart boundedly or disclose its mixed/partial nature rather than present a single snapshot.

### F08 — P2: Inbox applies locally before confirming the proposal is still applicable

Introduced in `04cd83d`. Evidence: [inbox.tsx](../../packages/vscode/src/explorerWebview/inbox.tsx), lines 40–60 and 99–104; [explorerSuggestions.ts](../../packages/vscode/src/explorerSuggestions.ts), `resolve`.

Search/Studio/Planner application mutates local state before resolving the proposal with the host. `resolve()` catches rejection and returns normally; `finish()` then navigates anyway. A proposal reviewed before expiry can remain cached while a replace/merge dialog is open. Applying after expiry or rejection can still replace the local draft even though the authoritative store refuses it. The agent sees expired/rejected while the user's draft has changed.

**Correction:** obtain a current host-authorized apply result before mutating the target. Bind it to proposal identity, current status, window binding and captured target revision. Propagate resolution failures and keep the existing draft untouched; refresh stale review dialogs instead of applying cached bodies.

**Acceptance:** open a comparison dialog, expire/reject/discard the proposal, then Apply. Search and both strategy drafts must remain byte-for-byte unchanged. Test a failed host response and target edits while the dialog is open.

### F09 — P2: Adding a Studio snapshot silently truncates the agent instructions

Introduced in `9561c2a`. Evidence: [planner.tsx](../../packages/vscode/src/explorerWebview/planner.tsx), `taskFromStudio`, lines 18–23; [explorerStrategies.ts](../../packages/vscode/src/explorerStrategies.ts), `compilePrompt` section order.

The prompt is cut with `.slice(0, 8000)` when added as a Planner task. Normal evidence budgets exceed that size. Because evidence precedes Task instructions, Deliverables and Completion gates, the cut can discard the actual task and safety/completion constraints or leave an unclosed fence. The task still keeps the full run's digest/outcome, with no indication that its instructions were shortened. A Studio draft without a stored prompt falls back to the objective instead of compiling its strategy instructions.

**Correction:** never silently slice a completed prompt. Offer an explicit bounded evidence reduction that preserves the objective, instructions, deliverables and gates, or reject Add to Planner with a clear size message. Bind task evidence metadata to the content actually exported.

**Acceptance:** add a generated prompt over 8,000 characters with distinctive instructions after the evidence; all required sections must survive or the action must be refused explicitly. Cover edited prompts and a strategy with no stored compiled prompt.

### F10 — P2: Declared byte budgets count JavaScript characters instead of UTF-8 bytes

Introduced in `5f0f409`. Evidence: [explorerStrategies.ts](../../packages/vscode/src/explorerStrategies.ts), strategy/plan size checks at lines 112 and 409 and `clipRecords` at lines 334–342.

`record.text.length + record.path.length + 96` undercounts non-ASCII evidence and approximates metadata rather than measuring a defined serialized representation. Strategy and plan validators likewise call `JSON.stringify(...).length` while reporting KiB limits. A scripted reproduction kept an 800-character CJK excerpt containing 2,400 UTF-8 text bytes under a declared 1,024-byte budget. UTF-8 expansion and JSON escaping can exceed documented limits substantially.

**Correction:** define the representation the budget covers and measure it with `TextEncoder` in shared code. Include the actual metadata/framing required by that representation. Use the same accounting in validators, runner clipping and displayed/exported budget explanations.

**Acceptance:** test CJK, emoji, combining text, long paths and JSON escape-heavy strings. The measured output must stay within the configured byte limit and report omitted records accurately.

### F11 — P3: Current documentation still describes the removed console

Exposed by `9ba5c23` and left by `8d0fc80`. Evidence: [README.md](../../README.md), line 49; [architecture.md](../architecture.md), line 13; [AIRK_Explorer_Final_Design.md](AIRK_Explorer_Final_Design.md), Appendix B line 791.

The README still directs users to AIRK Service Console, architecture calls 1.4.4 “Current,” and Appendix B's explicit “Status (1.5.0)” says the console is hidden behind `airk.explorer.showServiceConsole`. The view and setting have been removed. These are current-status statements, not merely dated historical descriptions.

**Correction:** reconcile current instructions with Explorer Management and 1.5.0. Label the temporary hidden-console stage as historical. Retain historical release evidence without rewriting it as newly executed validation. Include docs index, release notes and Marketplace README in the same consistency pass.

**Acceptance:** search current guidance for the retired view/setting and stale “current” version claims; verify that referenced commands/settings actually exist in the extension manifest. The synchronized version check already passes, but it does not validate these prose claims.

## Additional improvements and qualification work

These recommendations are separate from the confirmed findings above.

1. Extend tests at the asynchronous boundaries rather than adding more source-string parity assertions. Use deferred responses, a controllable clock and real temporary files where justified. Add a small component/editor test lane for project changes, Studio replacement and Inbox expiry; the current smoke harness mainly establishes happy-path Search/Management wiring.
2. Make Changes continuation usable. `changesSince` drops the native cursor and shows only “more may exist”; the UI cannot consume the remaining pages. Add project/revision-scoped continuation handles before workflows can treat a comparison as complete or advance a baseline.
3. Review deletion/rename impact explicitly. `changedSource` excludes deleted files, and the Git adapter retains only one path per entry. Use still-indexed old paths where available, disclose when old evidence has already disappeared, and preserve staged and working-tree states when both exist. Deduplicate declarations by path/location as well as name; two changed files defining the same name currently collapse to one declaration. Disclose the final 100-candidate cut in Tests.
4. Bound transport admission and output queues, not just individual message sizes. `EvidenceClient` queues requests and Assistance `serve` serializes messages without a queue ceiling/input pause mechanism. Consider finite queue counts/bytes and response backpressure with explicit overload reporting. Keep these controls in the adapters; do not add orchestration to the kernel.
5. Align the global project picker with what can actually be selected. Control pages can contain projects outside this window, while `project.select` accepts only workspace launch specs. Disable selection with an explanation or offer a separately reviewed binding flow instead of silently swallowing `NO_PROJECT`.
6. Qualify native forwarder/runtime paths on Linux and both Mac architectures, Windows default/custom/managed installations, and actual editor updates. Check process ownership, Unix permissions, socket teardown, descriptor rotation, payload verification and no stdout pollution. Real agent and remote editor tests remain separate evidence.
7. Exercise the installed 1.4.4→1.5.0 upgrade and explicitly refused rollback after recording Explorer usage. Confirm preserved project/storage/watching bindings and useful diagnostics when older runtimes encounter the new enum key. The release record already calls this unqualified.
8. Perform accessibility checks for focus restoration/trapping, keyboard tab navigation, active tab/panel relationships, screen-reader announcements, contrast and narrow windows. The text-only rendering/CSP choices are useful but are not accessibility qualification.

## Recommended implementation sequence

| Order | Work package | Required completion evidence |
|---|---|---|
| 1 | F01 guarded Planner filesystem save | Redirect/concurrent-edit/authorization regression cases; source and unrelated files unchanged. |
| 2 | F02, F03, F05, F06 request and view lifetimes | Deferred-response tests prove project/draft ownership, worker settlement and no stale result export. |
| 3 | F04, F07 evidence identity and error contracts | Native boundary reads, republish tests and explicit absence/unavailability distinctions. |
| 4 | F08, F09, F10 proposal application and bounded export | Expired proposal leaves drafts untouched; complete task instructions; UTF-8 byte accounting. |
| 5 | F11 and selected improvements | Documentation consistency, focused editor regressions and clearly separated platform receipts. |

Keep changes in their owning extension/CLI components and preserve the existing native authority, source-read-only, collection isolation, receipt and publication contracts. Before any corrective commit/push cycle, review the complete combined correction and validation evidence. Before a feature/fix PR, follow the contributor's patch/build-number reminder and synchronization rule; **1.5.0 is already bumped on this branch**, so do not bump it again merely to implement this review record. No commit, push, PR, publication or hosted CI run was performed by this review.

## Validation observed during this review

| Check | Observed result |
|---|---|
| `npm test --prefix packages/vscode` | Passed compilation of host, webview and Assistance plus bundle build. 248 tests: 237 passed, 11 skipped, 0 failed. This includes the existing uncommitted universal-launcher tests. |
| `python -m unittest discover -s scripts/tests -q` | Passed: 172 tests, 4 skipped. Mock output mentioning published releases is fixture output, not evidence of an actual publication. |
| `pwsh -NoProfile -File scripts/check-repo.ps1` | First attempt blocked by Git ownership trust. Rerun with a process-scoped safe-directory configuration passed; final run including this report passed for 1,140 public files. |
| `python scripts/release-version.py check --expected 1.5.0` | Passed: native, extension and lockfiles agree. |
| `git diff --check` | Passed, including the final verification pass. The new untracked report was separately checked for trailing whitespace, all 33 chronological entries, 11 finding sections and valid local links. `AGENTS.md` and `CLAUDE.md` remain byte-identical. |
| Scripted compiled-module probes | Confirmed F03 pending fan-out on return, F05 suppressed project-B status query and F10 UTF-8 budget overflow. No source/test files were added for these probes. |
| `cargo fmt --all --check` | Could not start: `cargo` not found on PATH. |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | Could not start: `cargo` not found on PATH. |
| `cargo test --workspace --all-features --locked --offline` | Could not start: `cargo` not found on PATH. |
| `python scripts/license-notices.py --output target/review-notices` | Failed to start its Cargo metadata subprocess: executable unavailable. No successful notice regeneration claimed. |
| Native build, service-client harness, native-enabled extension tests, VSIX packaging, installed-editor harness, Unix/remote qualification | Not run in this review. No provisioned current native test pair was selected; `AIRK_TEST_BINARY` was unset. Existing release-note receipts are historical evidence only. |

## Existing uncommitted work

At review start the worktree already contained changes in CLI `lib.rs`, `main.rs`, `runtime_install_commands.rs`, `user_paths.rs`; extension `agentMigration.ts`, `model.ts`, `serviceBootstrap.ts`, `serviceExtension.ts`, `serviceWorkspace.ts`; three installation/release/agent guides; and new `universal_launcher.rs` and `universalLauncher.test.ts` files. Those files were not edited by this review.

The supplemental review traced pointer publication/resolution, early launcher dispatch, launch argument serialization, default versus custom/managed installation selection and recognized agent-entry migration. The extension tests exercise serialization/migration, but the real universal-launcher test skipped without `AIRK_TEST_BINARY`, and native compilation was unavailable. Do not treat the supplemental inspection as native qualification. Before committing that separate work, run the actual installed launcher path, repeat concurrent publication/retargeting and redirected-file checks, and verify migration preserves foreign agent entries and installation identity.

Only this action-plan document was added by the review. All actions above remain recommendations.

## Implementation follow-up — 26 October 2026

The current branch implements F01–F11 while retaining version 1.5.0 as requested for local installation. The original review above describes the pre-correction checkpoint.

| Finding | Implemented behavior | Regression evidence |
|---|---|---|
| F01 | Local Planner saves reject redirected directories, symlink/hardlink targets and oversized existing targets. Exact target identity/content, authorization and dirty editors are checked after confirmation, with no asynchronous gap before the guarded descriptor write. New files use exclusive creation. Remote providers use Open as draft and the editor Save command. | Real temporary-directory tests for junction escape, hard links, concurrent target edits, dirtying during confirmation and authorization revocation. |
| F02 | Studio input changes invalidate runs/generated prompts; edited prompts are marked stale and blocked from export until rebuilt. Delayed run admission checks draft revision and project/binding identity. | Bundled UI-state tests edit inputs, replace generations, switch projects and attempt old-run admission. |
| F03 | Parallel steps stop admitting new items on failure and settle every started worker before finalizing or advancing. | Deferred sibling-read test proves no early return and no fourth-item admission. |
| F04 | Expand uses revision/hash-bound native context and clamps EOF. Extracted documents conservatively retain one cited line because the host cannot prove page boundaries. | Host request-contract test and native EOF/stale-token regression. |
| F05 | Status completions carry a binding epoch and reschedule the current selection after an older request settles. | Deferred A-to-B status test; old results cannot replace B. |
| F06 | Evidence screens remount per route/project/binding epoch; old host replies are superseded. Inspector/publication/query state is cleared at project transitions. Studio evidence is invalidated and Planner ownership is explicit. | Bundled state and host generation tests; native/editor acceptance remains a separate lane. |
| F07 | Review/Checker compare every constituent publication, preserve tool/service failures, and recognize only native explicit absence as not indexed. Missing line counts remain unchecked. | Mixed-publication and integrity-failure regressions plus real native missing-path checks. |
| F08 | Host proposal resolution succeeds before local mutation. Reviewed target revisions and binding generations are checked before and after admission; conflicts preserve local work and report conflicted status. | Bundled apply-order/expired/racing-revision tests and existing store expiration/conflict tests. |
| F09 | Planner accepts complete Studio prompts only. Oversized/stale snapshots are refused with guidance, never sliced; unstored prompts are compiled before adding. | Bundled over-8,000-character rejection and intact-instructions tests. |
| F10 | Evidence budgets cover the complete UTF-8 JSON record array; strategy/plan schema limits also count UTF-8 bytes. | CJK and JSON-escaping boundary tests. |
| F11 | Current README, architecture, design status, docs index, Windows guide, release notes, Marketplace README and bundled help describe Explorer Management and the current save/evidence contracts. Historical review/release receipts retain their dates and scope. | Version, documentation/build and repository checks. |

Additional code improvements from the plan are included: Changes cursor paging with project/publication-bound handles; retained old rename/deletion path review and disclosure when old declarations are no longer indexed; path/location-aware declaration deduplication; explicit test-candidate clipping; finite evidence/stdio/Assistance request queues and output backpressure; disabled selection of projects outside the window; keyboard tab navigation, dialog focus cycling/restoration and combobox/tab/panel ARIA relationships.

This is a local Windows implementation and qualification effort. Linux, both Mac architectures, remote editor providers, real external coding-agent sessions, installed 1.4.4 upgrade/rollback and a full screen-reader/contrast audit require their respective environments. No hosted matrix, deployment or publication is implied. Remote guarded saving deliberately refuses providers without a proven containment/conditional-write contract. Local save checks defend against observed redirection and concurrent review changes; they do not claim protection against a malicious same-user process continuously racing filesystem namespace operations.

The complete diff is reviewed explicitly by the implementing reviewer after the corrections; no independent review is claimed. Current validation results and the local installer receipt are recorded below when their runs finish.

### Current validation receipts

Implementation began at `09f1c9d` with a clean worktree: the previously uncommitted universal launcher had been committed as `c9baace`. This corrective pass changes extension code/tests and documentation; it preserves the native launcher implementation. All native checks include that committed launcher work.

- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked --offline`: passed, including doctests. Full local Windows workspace coverage was requested for this implementation; no hosted CI was invoked.
- Extension build and mock lane: 264 tests, 253 passed, 11 platform/native skips, zero failures. Native-enabled results are recorded separately below.
- Python contracts: 172 tests, four skipped, zero failures.
- Repository checks: passed for 1,142 public files, including new lifecycle/backpressure tests. Whitespace, documentation UTF-8 and byte-identical AGENTS.md/CLAUDE.md checks passed.
- Version synchronization: native, extension and lockfiles remain 1.5.0.
- Notice generation: succeeded for 411 third-party packages. The generator repeats the existing missing-standalone-license-text review notices documented in THIRD_PARTY_NOTICES.md; this run does not constitute a new license approval.
- Explicit complete-diff review checked save authorization/root/target guards, post-confirmation editor state, worker settlement, late result admission, suggestion lookup/application races, project binding, publication checks and packaged-module closure. Two additional related cases were corrected during that pass: root replacement during a save dialog, and proposal binding changes after admission. Their regressions pass in the extension lane.
- The first native-enabled extension run stalled during universal-launcher cleanup: a Windows executable-removal race could skip disposal of its owned service fixture. Cleanup now retries removal and always disposes the fixture. The focused real-launcher test passes; the full native lane is rerun after this correction. The interrupted run is retained as diagnostic evidence, not counted as passing.

### Local installer receipt

- Release runtime pair: built and verified with `scripts/build-service-runtime.py --profile release`; includes the committed universal launcher.
- Final native-enabled extension suite: 264 tests, 262 passed, two skips (POSIX startup and the old-helper replacement fixture), zero failures (85.7 seconds). This rerun includes the cleanup correction and the final BigInt filesystem-identity checks.
- Exact-pair service-client/model qualification: passed with two isolated projects, 16 retrieval tools, real local-model queries and preserved source bytes.
- Native archive and platform VSIX: verified by `scripts/package-native.py` and `scripts/package-extension.py`, including payload inventories, runtime identity, digests and packaged module closure.
- Installer: [`airk-1.5.0-win32-x64.vsix`](../../artifacts/local/explorer-review-150-20261026/vscode/publish/airk-1.5.0-win32-x64.vsix), Windows x64, version 1.5.0. SHA-256: `fdd88d363f91fcb7084ff5a6aa393ce9f5cfab92c26dc331fbb714d943594822`.
- Evidence directory: `artifacts/local/explorer-review-150-20261026/`, including package manifests, build/test logs and `service-client-check.json`. This is a local candidate built from the uncommitted corrective work on baseline `09f1c9d`; it is not a published release.
- Private-profile VSIX installation succeeded. The initial installed-editor attempt failed before extension loading because the command sandbox denied Electron access to the VS Code installation; no editor ACLs were changed. Outside-sandbox acceptance used isolated profiles/extensions and made no change to the normal editor profile.
- All five installed-VSIX scenarios passed: same-project windows, different-project windows, multi-root Search/Management, usage reporting and untrusted workspace. The first three receipts are in `artifacts/editor-qualification/native-installed-OU8Sf0/summary.json`; usage/untrusted are in `artifacts/editor-qualification/native-installed-c51Xtf/summary.json`.
- The first usage attempt reached its final stale-binding check but the harness timed out because storage review left the source editor active; the hidden report could not receive its Refresh click. The harness now explicitly activates the original report before clicking, retaining the stale-refusal assertion. Only usage and untrusted scenarios were rerun; both passed against the same verified VSIX. The already passed scenarios were not repeated.

## Subsequent requested Assistance, Help and UI work — 26 October 2026

This follow-up includes the original F01–F11 corrections above and the later requested features. Version 1.5.0 remains synchronized as requested. Earlier installer receipts identify earlier snapshots; the installer below supersedes them for local testing.

| Requested work | Implemented behavior and evidence |
|---|---|
| Five new Assistance tools | `airk_explorer_templates_list`, `airk_explorer_template_get`, `airk_explorer_validate_draft`, `airk_explorer_suggest_navigation`, `airk_explorer_handoff_get`; fourteen tools total. Offline template/paging/budget tests, real bundled MCP/IPC tests and installed native-forwarder tests exercise the contracts. |
| Existing status tools | UI status reports route, binding generation, blockers and revisions. Proposal status reports fixed reasons and admitted/current/acknowledged application revisions. Applying to a draft is distinct from execution. Binding changes cannot retarget retained proposals or snapshots. |
| Project setup | New configurations include Retrieval and Assistance. Installation/repair backfills a missing companion only beside a recognized project Retrieval entry, preserving disabled settings, unrelated entries and custom conflicts. Codex supports an explicit guarded update of an existing recognized project config. Trust, unsaved-edit, foreign-entry, idempotency and disable-setting tests pass. |
| Required UI | Navigation cards preview supported route/form fields; users submit queries themselves. Studio and Planner offer explicit shared snapshots with confirmation, ten-minute tokens, complete content/hash/revision metadata, bounded storage and revocation. Read-only MCP access cannot enumerate or inspect other drafts. |
| Unified Help | One searchable, ranked, area-filtered offline catalog contains Explorer articles and every static native engine topic. Both the UI and Assistance help tools use it. Live CLI command help/tool schemas retain runtime identity and bounded page navigation. Old reference replies cannot replace another topic. |
| Four primary roles | Developer, Tester, Architect and Writer guides cover purpose, code/document evidence, verification, strategies and handoff. Indexing and format guides distinguish Search Only/Agent Lean/Rich Structure/Detailed, independent vectors, watching, supported syntax, original PDF/Office locations, spreadsheet caches and explicit local OCR. Roles never silently change settings. |
| About and community | Header About opens Help > About AIRK with version, Ioka LLC credit, Munarium/VCP inspiration and independent implementation history. Fixed user-clicked links include ioka.io/airk, the repository, Discord invitation, GitHub Issues, contribution guide and license notices. Public history was checked against Ioka's page and the repository's inspiration record; no private reference material was copied. |
| Local files | Planner Load plan starts in .airk/plans and validates JSON or AIRK-generated Markdown; replacement confirms and rechecks draft/project identity. JSON retains complete states/provenance; abbreviated Markdown imports as unverified draft tasks with an explicit notice. Export plan JSON uses the guarded save path. Strategy import starts in .airk/strategies and cannot overwrite intervening Studio edits. Recipes remain editor workspace state. |
| UI controls and style | Static TypeScript JSX audit checked 120 native buttons, 28 inputs, 17 selects, six text areas and eight forms: no inert buttons or controlled editable fields without handlers were found. Custom action controls delegate to shared typed handlers. Dialog focus/restoration, hover feedback, long text/token wrapping, Help layout and friendly detail labels were refined. This is not a claim of exhaustive manual accessibility qualification. |
| Recoverable errors | Error codes remain available for diagnostics; common failures explain the next step. Webview admission is bounded to 64 awaited requests; revisions/navigation notifications reserve no pending slots. Read deadlines release controls, late replies are ignored and lost action acknowledgments report unknown outcome. Saves/configuration/proposal/repair operations are never automatically replayed. Recovery Help works offline. |

### Validation for the updated installer

- Full Rust workspace/all-features tests and doctests: passed. Final formatting and workspace/all-targets/all-features Clippy with warnings denied: passed. Four focused native help tests also passed after the final help-topic changes.
- Extension native-enabled suite: 283 tests, 281 passed, two skips, zero failures. Final extension suite after the additional local-import regression: 284 tests, 273 passed, eleven native/platform skips, zero failures. Tests run the actual bundled server and deferred/lifecycle/temporary-file boundaries; no source-string-only claim substitutes for execution.
- Python repository/packaging contracts: 172 tests, four skipped, zero failures. Repository checks pass for 1,157 public files; whitespace and version synchronization checks pass. AGENTS.md/CLAUDE.md remain byte-identical.
- Fresh release runtime pair built and verified. Exact runtime service-client/model check passed for two isolated projects and sixteen Retrieval tools, preserving source bytes. The packaged native executable also answers its current About/Discord guidance directly through airk_help.
- Native archive and extension package inventory/digest/module closure checks passed. License notices were generated for the same 411 packages; the existing three missing-standalone-text notices remain review notices rather than newly approved licenses.
- Installer: [airk-1.5.0-win32-x64.vsix](../../artifacts/local/explorer-assistance-150-20261026/vscode-final/publish/airk-1.5.0-win32-x64.vsix). SHA-256: `2083df201b489aaeb2a587a617f1afdeaceb871beb3489a56b9575e1ce3a8dac`.
- The full five-scenario installed-editor run passed against the first expanded candidate: same project, different projects, multiple roots with Help search/role/purpose/offline engine/live schema/About checks, usage reporting and untrusted workspace. Receipt: `artifacts/editor-qualification/native-installed-yQ2XWA/summary.json`.
- Final review then reproduced a Windows path comparison issue in the new Codex update flow: native extended-path registrations were compared with an ordinary canonical directory spelling. The update now follows the same canonical-root path as configuration creation. The actual setup/save regression now supplies a namespaced native registration and passes; the complete final 284-test extension suite passes after this fix.
- The final repack changes only `extension/out/agentSetup.js`; comparison of every VSIX entry proves the native runtime, Help/Assistance/UI bundles and all other payload bytes match the five-scenario candidate. Receipt: `artifacts/local/explorer-assistance-150-20261026/final-payload-comparison.json`. The exact final VSIX passed the focused installed multi-root/Help/About check; final receipt: `artifacts/editor-qualification/native-installed-1z6Ku9/summary.json`.
- Evidence directory: `artifacts/local/explorer-assistance-150-20261026/`, with manifests, runtime/service-model checks, native help output, test logs and package inventories.

The implementing reviewer explicitly reviewed the combined changes and validation evidence; no independent review is claimed. No commit, publication, normal-profile installation or change to editor ACLs was performed. Linux/macOS, remote providers, real external agents, installed-version upgrade/rollback and a full accessibility audit remain environment-specific qualification work. The authorized work here produces a Windows local installer and preserves the earlier review corrections.

## Local commit preparation — 27 October 2026

The user requested a local commit without a push. The pre-commit review checked the combined changes, save guards, project/publication identity, proposal admission, shared snapshot revocation, transport bounds, packaging modules and documentation agreement. This was an explicit review pass, not an independent review. Native, extension and lockfile versions remain synchronized at the previously requested 1.5.0.

- Extension compilation and tests: 284 tests, 273 passed, eleven native/platform skips, zero failures.
- Python repository/packaging contracts: 172 tests, four skipped, zero failures.
- Rust formatting and workspace/all-targets/all-features Clippy with warnings denied: passed using the installed Cargo toolchain and offline dependency resolution.
- The Rust workspace/all-features test rerun was stopped at the user's request before completion; no complete passing result is claimed for this rerun.
- Repository checks: passed for 1,157 public files; version synchronization and whitespace checks passed. AGENTS.md and CLAUDE.md remain byte-identical.
- Notice generation: succeeded for 411 third-party packages after adding the installed Cargo directory to the process PATH. The existing adobe-cmap-parser, pdf-extract and type1-encoding-parser standalone-text review notices remain unresolved.
- Native-enabled extension, service-client/model, packaging and installed-editor qualification were not repeated during commit preparation; the dated receipts above retain their original scope. No cross-platform qualification or publication is claimed.

Commit-preparation logs are retained locally under target/commit-*.log and are excluded from the commit along with build artifacts and private/index state.
