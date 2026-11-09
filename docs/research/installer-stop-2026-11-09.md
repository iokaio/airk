# Installer repair checkpoint — 9 November 2026

**Stopped at the user's request. This is unfinished work, not an accepted
installer or release.** The user authorized documenting, committing and pushing
the checkpoint on `feat/finetuning`, with no PR. Repair, packaging and platform
qualification have stopped. Version remains **1.8.16**; 1.9.0 is a future target.
The checkpoint builds on commit `9c71c2b041eaee65c6639019a58c88ad8c85a74c`.

The final Windows VSIX passed package/seal verification but failed the installed
editor journey. It must not be presented as ready to install. The requested
30-minute working-delivery target was not achieved before this stop. No global
installation, release publication or new Mac batch was performed.

## What changed

The [1.8.16 record](../releases/release-1.8.16.md) and
[implementation ledger](one-airk-implementation.md) retain the detailed history,
commands, source identities and passing/failed receipts. The substantive work is:

- Replace migration of pre-1.8.16 indexes with the approved clean break: capture
  all saved source selections and supported preferences, create fresh authority,
  rebuild every index including paused indexes, then restore pause/watch policy.
  Preserve source files and refuse unsupported or unproven cleanup.
- Add the persisted state-contract boundary, interrupted-reset handling,
  predecessor process ownership/exit checks, and bounded cleanup of old state,
  storage, runtimes and launchers. Separate storage ownership from private IPC
  permissions and keep deletion away from all original source roots.
- Retain purpose-bound historical HTTP/client cleanup evidence without granting
  old credentials new authority. Keep native cleanup, host configuration cleanup
  and actual client reconnection as separate outcomes.
- Correct Explorer startup-failure publication and progress wording; retain
  replacement-loader support and complete runtime diagnostic copies. Already
  loaded older extension code still needs one activation/restart to acquire the
  loader. Add standalone Windows PATH ownership bookkeeping; actual Inno,
  registry and signing qualification remains absent.
- Correct host intent persistence and reconciliation of accepted outcomes,
  native parse/read budget ownership, actual delivered-byte accounting, and
  catalog-schema completeness classification. Preserve full CLI/MCP/Workbench
  results and existing scope, authority and size limits.
- Add regression coverage and correct qualification infrastructure: deterministic
  onboarding mocks, CDP connection diagnostics/disposal and action deadlines,
  installed-driver parse budget handling, and package output/seal verification.
  The contributor files now require investigation, batched corrections, combined
  review and frozen inputs before validation; AGENTS.md and CLAUDE.md match.

## Current unresolved failure

`clean-break-r39-installed-editor-r3` failed in **42.121 seconds**. The untrusted
scenario passed. The multi-index scenario stopped at sequence 4, `setupDraft`,
while checking MCP definitions before enrollment. The same-root scenario did
not run. The observed error is `workspace_registration_changed`, through:

`installedEditor.ts: definitions/noSelection/createIndex` →
`ServiceExtension.provideDefinitions` → `hasManualVscodeConnection` →
`vscodeCredentialSources` → its stale-binding check.

Source review of `packages/vscode/src/serviceExtension.ts` identifies a plausible
race: `refreshOnce` assigns a newly constructed binding object even when its
authority is equivalent. `hasManualVscodeConnection` captures the old object
reference and generation, then checks reference equality across awaited
filesystem/inventory reads. `provideDefinitions` does not join an in-flight
refresh when a current binding already exists. An equivalent replacement can
therefore cancel the scan; refresh emits a definitions-change event only when
launch arguments change, so such a cancellation need not produce a retry event.
The sibling credential path also uses reference-based invalidation and requires
review before changing this contract.

**Observed:** the installed run hit the pre-enrollment stale-binding guard.
**Source-supported hypothesis:** equivalent-object replacement can trigger it.
The retained stack does not establish that this fixture's old/new authority was
equivalent; an actual authority change has not been ruled out. No deterministic
regression or production correction for this latest race was completed.

The missing cheap regression should pause the real host credential-source scan,
complete a refresh with unchanged runtime/control/generation, then release the
scan. Pair it with genuine runtime/control/generation changes that must still
refuse, and a provider call entering while refresh is already pending. Joining
only an already pending refresh would miss refreshes starting during later
awaits. Swallowing cancellation as an empty definition list, widening retries or
removing authority checks would not establish a correct fix. This is a handoff
requirement, not authorization to resume repair.

## Why it kept failing

There is no evidence for one remaining installer bug that explains all failures.
Several independently stateful components must agree: installation selection,
native lifecycle/checkpoints, registry and index storage, HTTP authority,
extension discovery and configuration, durable UI intents, and installed-test
cleanup. The clean break removes old index-format migration but does not remove
the need to discover preserved sources, prove old process exit, dispose of owned
artifacts, establish fresh authority and update running clients.

The failures expose both implementation coupling and gaps in the workflow:

1. **Different ownership contracts were conflated.** Earlier cleanup applied a
   private IPC permission rule to valid index-storage permissions. Fresh managed
   selection was also interpreted as owned payload before managed policy was
   consulted. Passing isolated lifecycle tests did not cover those compositions.
2. **Concurrency and durable outcome handling were incomplete.** R37's installed
   sequence stopped on `catalog_outcome_unconfirmed_reconcile_first`; the host
   intent correction followed. The present definition-read race exposes another
   state transition whose invalidation/event contract lacks a deterministic test.
3. **Budget and completeness contracts diverged across callers.** R38's Workbench
   failure led to native parse-budget and schema fixes. A guessed 1,024-byte
   receipt reservation then rejected an otherwise fitting scoped capability
   reply. R39 UI r2 found that the external installed driver still applied the
   ordinary 64-KiB read limit to parse results. The earlier caller audit missed
   those two driver sites. These were separate native and test-driver defects.
4. **The harness had its own failures.** R39 UI r1 lost a nested WebSocket error
   as `[object ErrorEvent]`. Deterministic tests exposed missing failure cleanup
   and target/deadline handling. The original connection failure's exact cause
   remains unknown; later passing attachment does not prove that cause.
5. **Packaging was assumed to compile.** It actually stages existing `out/`
   outputs. A host-only TypeScript compilation had replaced the bundled uninstall
   hook with an intermediate file. Verification rejected the first R39 VSIX;
   complete compilation and extension-only repack repaired the artifact. No
   installed journey used that rejected package.
6. **Local green results were too often followed by a broad discovery run.**
   Related callers and cross-component state flows were not fully enumerated
   before correction. Repeated fixes, fixture changes, freezes and installed
   reruns increased cost and made the evidence harder to follow. Later reviews
   and cheap regressions improved individual checks but did not establish an
   end-to-end working installer. The failure was not simply a lack of test count.

A refactor should first make lifecycle ownership, binding identity/invalidation,
durable outcomes and response-budget selection explicit and testable at their
boundaries. Preserve the existing authority/source protections and use retained
failures as acceptance criteria. No wholesale rewrite or refactor was started
at this checkpoint, and no claim is made that a rewrite alone would fix these
problems.

## Evidence at the stop

Receipt stems below are beneath ignored local
`artifacts/local/one-airk-1.8.14/`; the directory name is historical, not the
current version. Full logs/fixtures remain local and are not committed.

| Check | Observed result and scope | Receipt stem |
| --- | --- | --- |
| R39 native regressions | 67/67 passed; 1,225 outside filter | `clean-break-r39-canonical-regressions-r2` |
| Real native/CLI/HTTP-MCP parse parity | 1/1 passed, one- and two-index cases; 42 outside filter | `clean-break-r39-actual-parse-transport-r2` |
| Workspace all-feature/all-target Clippy | Passed, 29.900 s | `clean-break-r39-workspace-clippy-r1` |
| Lightweight main gate | Passed, 24.538 s | `clean-break-r39-lightweight-main-r1` |
| Fresh debug runtime | Passed, 150.817 s, retained Cargo timings | `clean-break-r39-debug-build-r1` |
| Authentic 1.8.11 → 1.8.16 reset | Passed, 113.245 s; both indexes rebuilt, pause/watch and sources preserved, predecessor exits, cleanup and acknowledged teardown | `clean-break-r39-upgrade-gate-r1` |
| Full native-enabled extension suite | **Failed:** 1,292 collected, 1,288 passed, 1 failed, 3 skipped | `clean-break-r39-extension-native-r1` |
| Corrected onboarding module with native runtime | 117/117 passed; test-only mock correction, not a rerun of the full native suite | `clean-break-r39-onboarding-module-green-r1` |
| Full mocked extension suite | 1,291 collected, 1,270 passed, 21 skipped; predates four later installed-helper regressions | `clean-break-r39-extension-mock-r1` |
| Full Python suite | 923 run, 13 skipped, no failures; predates later CDP/driver-only changes | `clean-break-r39-python-full-r1` |
| Changed CDP harness/fixture checks | Node 114/114; scoped Python 7/7 passed | `clean-break-r39-editor-harness-green-r1`, `clean-break-r39-editor-fixture-python-r1` |
| Complete compile and installed-helper checks | Compile passed; affected tests 45/45 passed | `clean-break-r39-installed-budget-full-compile-r1`, `clean-break-r39-installed-budget-green-r1` |
| Package verification | Passed, 925 members, 89 shipped extension outputs, 811 native seals plus manifest | `clean-break-r39-package-proof-r1` |
| Installed UI r1/r2/r3 | **All failed:** connection diagnostic, driver budget, then definition cancellation; retained separately | `clean-break-r39-installed-editor-r1`, `-r2`, `-r3` |
| Current Neo batch | **Not launched.** 17 preparation tests passed; reviewed transfer retained | `clean-break-r39-neo-preparation-t0-r2` |

Earlier R37 Neo component checks passed on their own frozen source: 398 native
tests, Python 923 with 19 skipped, mocked extension 1,170 collected/1,137
passed/33 skipped. Its evidence was collected and its Cargo target cleaned.
Those results do not qualify R39's native changes or the later extension changes.
Current full workspace tests/doctests, installed Mac acceptance, Docker,
signing, hosted qualification and publication are incomplete or unrun. No new
expensive validation was started to make this stop checkpoint look green.

Retained final package:
`windows-debug-r39-vsix-final/publish/airk-1.8.16-win32-x64.vsix`
(86,985,679 bytes), SHA-256
`b879926d43d4d816fb48eff32afc2ec134c22bd36da98a2d4e744269875eedb6`.
Its native compiled-source identity is
`f187c34556fe74ee72ef7ab645100a0e57c564da77160c59219ca0dddb97e9da`
(1,154 inputs); main executable SHA-256 is
`5a6a8780561b86cdc01edaaf5afe6c528fd0194f1fbec96a714ca770e26158e9`.
These identify the failed candidate, not the subsequent documentation-only
checkpoint or a newly rebuilt package.

## Stopped processes and preserved work

All delegated repair workers finished; no new build, installed run, transfer or
Mac execution was started after the stop request. The last UI fixture is
`artifacts/editor-qualification/native-installed-pid1qa`; its failure disposal
is complete but does not qualify normal acceptance cleanup. The preceding
`native-installed-KKmCTx` and `native-installed-gSWrVc` fixtures also remain.

A stop-time process census found four older task-owned processes under the exact
`artifacts/q31/au-ifvpz0ha/` and `artifacts/q31/au-msqi029h/` fixture roots.
Executable path and creation identity were rechecked before forced termination;
all four exited and no files were removed. Receipt:
`clean-break-stop-2026-11-09-processes.json`. This is forced failure-fixture
disposal, **not** successful native uninstall or cleanup acceptance. The normal
user AIRK installation and its processes were left untouched.

Source, tests and documentation from this repair are checkpointed. Generated
packages, private state/indexes, logs, immutable transfer archives and failed
fixtures remain ignored and local. The separately edited user documents
`docs/simplify-plan.md` and `docs/airk-feature-review.md` are preserved outside
this repair commit. The checkpoint does not approve their proposals or erase
unrelated work. No PR is to be created.

## Follow-up: source correction (9 November 2026, after the S1 review)

The user authorized the repair (decision D10 in the
[S1 review record](../simplify/s1-review.md#10-review-decisions-user-9-november-2026)).
This section records a source correction only. **No installed-editor journey,
VSIX, runtime build or Neo batch has run since this stop.** The
`clean-break-r39-installed-editor-r3` gate is still unqualified.

**Root cause, confirmed by a deterministic regression:**

- `refreshOnce` publishes a new binding object on every refresh.
- Fifteen staleness checks in `serviceExtension.ts` compared that object by
  reference. These include `hasManualVscodeConnection`, `httpCredential` and
  the HTTP owner-health watch.
- So any equivalent refresh during their awaits cancelled them with
  `workspace_registration_changed`.
- During a clean-break rebuild, every refresh carries a newer `rebuild`
  progress observation. That makes equivalent replacement routine in exactly
  the R39 scenario.

**Correction:**

- All of those checks now use `sameRuntimeAuthority`, which compares
  `runtimeBindingIdentity` and ignores only the progress-only `rebuild`
  observation.
- Genuine runtime, control-directory and generation changes still refuse.
- A genuine authority change between two published bindings now fires the
  definitions-change event, so a refused provider is asked again.
- No cancellation is swallowed, no retry is widened, and no authority check
  is removed.

**Evidence:** `packages/vscode/src/test/serviceBindingIdentity.test.ts`. Its two
former `todo` reproductions now pass as ordinary tests. It adds a
rebuild-progress-only refresh case, an event assertion for a genuine runtime
change, and unit cases for `sameRuntimeAuthority`. The installed fixture's exact
old and new authority remains unobserved, so whether this was R39's only cause
is established only by a future installed run.
