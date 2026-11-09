# AIRK 1.8.16 clean-break baseline

**Stopped, 9 November 2026.** This is an unfinished checkpoint, not a release.
The final R39 Windows VSIX passed package verification but failed installed
acceptance. The user requested a documented commit/push and no PR. See the
[stop and handoff](../research/installer-stop-2026-11-09.md) for the remaining
definition-read cancellation, failure analysis, artifact identities and exact scopes.
No further repair, build or platform run is active.

The user requested another patch increment on 8 November 2026 after the installed
1.8.15 r30 candidate refused the 1.8.11 upgrade and Explorer remained on its
initial connecting display. Native workspace, lockfiles, extension and generated
catalog identify 1.8.16 for this correction. Earlier packages and receipts retain
their original versions and hashes.

The retained activation journal is `staged_verified`: accepted-work compatibility
preflight refused the migration before process shutdown. The installation output
reported three unfinished accepted operations, while the progress text had
already announced stopping the predecessor. Remaining predecessor processes do
not establish that the shutdown implementation ran or failed. Read-only follow-up
observations found active reconciliation jobs; they do not reconstruct the exact
three-operation roster at the earlier refusal.

## Current decision: a clean-break baseline

Later on 8 November 2026, the user approved making 1.8.16 the compatibility
baseline. Every earlier installation must create fresh AIRK state and rebuild
its indexes, preserving only reviewed source selections and supported saved
preferences. Original sources remain untouched. Old index IDs, grants,
credentials and pending operations are discarded through the native reset;
ownership-proven predecessor processes, index/state roots, launchers and runtime
artifacts must be retired. An incomplete reset or cleanup stays incomplete.

Ordinary upgrades in the 1.8.16+ line require an approved runtime and the supported
persisted state contract. A version string or copied new executable cannot label
old state as compatible. This line will be stabilized before a future public
1.9.0 release; no 1.9.0 bump, CI dispatch or publication is authorized here.

The [clean-break contract](../upgrade-rebuild-fallback.md) supersedes optional
pre-baseline migration/rebuild choices. Implementation and installed acceptance
of the new policy remain incomplete.

**Later on 9 November 2026 (simplification S2):** the native in-place catalog
upgrade and standalone-index adoption were removed; their CLI commands remain as
typed refusals (`catalog_upgrade_unsupported`, `standalone_adoption_unsupported`).
The extension no longer contributes **AIRK: Review unavailable legacy upgrade
index retirement**, and the obsolete `check-runtime-legacy-retirement` and
`check-standalone-adoption-failure` qualification scripts were deleted. The
automated `legacy` installed-editor scenario (`prepare-native-editor-legacy.py`,
`native-editor-legacy.cjs`, `AIRK_EDITOR_LEGACY_FIXTURE`) was retired as well;
by user decision, upgrades from old installations are tested manually in
installed editors.

The selected control root's `airk-state-contract.json` records schema 1,
`state_contract: "airk-state/1"`, baseline 1.8.16 and exact installation/control/
namespace binding, atomically qualified through native fresh Genesis. Saved
index capture is complete metadata-only catalog/registry capture, bounded to 64
indexes/128 KiB with explicit refusal rather than truncation. The complete
protected predecessor/successor path union keeps its existing 16-path bound,
checked before admission/Stop; 64 logical indexes do not imply 64 physical roots.
Still-existing older retained namespace/private roots without exact immediate
ownership refuse before reset (`clean_break_retained_root_unsupported`). Saved
protection paths alone are not deletion authority; already removed old roots
are not outstanding cleanup.

Historically staged but never-started payloads can lack optional process-role
aliases. When Windows process audit is incomplete, file absence is not exit
proof; this known unproven layout refuses before reset admission/Stop. No alias
files, process proofs or omitted obligations are fabricated to pass retirement.
This is an explicit historical-layout limit, not universal reset qualification.

Source policies
or aliases that would require old-format translation refuse before mutation;
they never become whole-root selections. External user custom models remain
untouched. Each saved index receives one initial build before restoring saved
pause/watch policy. Independent healthy indexes may build while another is
unavailable, but cleanup stays incomplete until all required replacements are
ready. Every deletion is guarded against all primary/additional source roots,
including inactive registrations. External client declarations and old uninstall
receipts use a separate host-owned compare-and-swap plan prepared before reset;
native completion does not claim those external effects succeeded.

After forced-reset admission, predecessor installation-level User Stop/lifecycle
state is discarded so the fresh runtime can start its initial builds. Supported
index pause/watch preferences are carried separately. A new Stop during
reset/rebuild remains authoritative and is not cleared on retry/resume.
Extension native-execution-disabled and workspace-trust gates remain hard gates.

## Current host, bootstrap and standalone PATH corrections

**Additional HTTP disposal correction implemented in source; qualification in
progress:** purpose-bound predecessor
metadata proof is separate from ordinary live-grant lookup. Complete bounded
original HTTP and host client metadata survive retirement durably. Native HTTP
capture occurs after verified old exit and before the state fork, with a
128-ID/64-KiB bound and original control/namespace/predecessor/operation binding;
fresh-namespace collisions or unknown ownership block disposal. Only the owning
fresh HTTP service initializes its first empty ledger before endpoint
publication under the service lease; reads never repair missing authority.
Missing/corrupt/orphan metadata is unavailable, and compacted retired markers
without a token digest cannot authorize environment-token removal. The r34 results below predate
this correction and are historical, not qualification of the changed inputs.

The successor bootstrap uses an opaque permit for only the exact clean-break
`Starting` operation, matching activation intent, fresh state marker and verified
candidate. Its fixed request-epoch transport revalidates candidate
instance/namespace; ordinary reads, enrollment and checkpoint callers keep their
existing gates. A real local framing/transport regression exercises this boundary.

Host cleanup captures only predecessor-native-proven HTTP environment and saved
connection metadata. Public trust copies require native CA and live creator
provenance plus matching bytes and full-width file identity. The creator handle
stays open through the awaited publication observer, and the caller checks the
original file identity; post-return adoption is rejected. Changed, unproven,
shared or new-namespace copies remain preserved with visible blockers. Plans do
not persist token plaintext and do not remove the existing PATH mutator. Only
this window's live Assistance creator can retire and positively refresh its
endpoint; failures remain visible/retryable, other windows are untouched and no
new evidence connection is automatically enrolled. These host results do not
become native cleanup or client-reconnection proof.

The standalone Windows Inno EXE now adds the literal existing `{app}\runtime`
directory to user HKCU PATH only after native setup succeeds. It preserves raw
other entries, manual paths and registry kind; comparison no longer assumes all
kinds expand environment variables. The durable receipt moves pending to owned
only after verification. Uninstall confirmation and native exit 0 precede removal
of the exact owned literal; failures abort before packaged bootstrap deletion.
This is separate from the VSIX's existing reviewed terminal integration. See
[standalone installer ownership](../../installers/windows/README.md).

Historical r34 checks: 72/72 selected native tests passed (17.610 s wrapper,
15.664 s tests), including the 1/1 actual initialized-candidate transport
regression. Workspace Clippy with all features and all targets passed (48.236 s).
The full extension compile/suite collected 1,154: 1,133 passed, 21 skipped,
0 failures (24.349 s total, 11.079 s tests); the 13 focused host/lifetime
regressions across two files also passed (1.801 s). Full Python ran 917 tests with
13 skipped and no failures (86.601 s wrapper, 85.855 s tests). The 13 static/mocked Windows
PATH/package tests passed (0.248 s). These source/component checks are not
installed acceptance.

Historical r35 component checks: the full extension suite collected 1,170 tests:
1,149 passed, 21 skipped, 0 failures (24.741 s total, 11.368 s tests).
Workspace Clippy with all features and all targets passed (46.553 s).
Full Python ran 919 tests, 13 skipped, 0 failures (82.300 s total, 81.414 s tests).
The native batch selected 363 tests: 362 passed and one stale guided test failed;
after its test-only correction, the named regression passed (32.587 s total,
24.627 s tests). This is not a passing rerun of the full 363-test batch.
The actual Stop-owner T0 passed. Stop, pause and root handling retain bounded
metadata proof; completion/PATH bookkeeping does not ensure or start a runtime.
Python's mocked signing output does not qualify actual Inno or registry work.
These checks predate the r36 storage-disposal correction below.

ISCC was unavailable for the current local check: actual Inno compilation,
registry/type/Unicode round-trips and real signing remain unqualified.

The r35 fresh native build passed (170.689 s), producing main SHA-256
`4739cec2582bbc609bd7fe030d5bed2ae9d38c32cfde506225e90e87000db404`
and manifest `81e1d40605c6e23f7cf7089863814c13098e1bad5e0201d1295ed4a44ac27afe`.
The fourth two-index journey failed (73.208 s) with consumed inputs unchanged:
activation took 23.217 s, the held old client terminated, and both fresh
publications and restored pause/watch policies completed (2/2), with sources
preserved. Cleanup incorrectly applied an IPC privacy guard to valid index
storage ACLs. Canonical Stop completed; overall cleanup failed. Fixture
`q31au-i64mt0b9` remains preserved. These partial observations do not qualify
installed cleanup.

The r36 source correction implements the following: Store separates typed storage-metadata
ownership from IPC privacy, inventories canonical writer/quota/lifecycle/usage
locks and the complete known nonlive pending-credit roster, and journals exact
root/file identities and hashes under private control for bounded crash replay.
Read preflight neither creates nor migrates state. Existing Store data-root
permissions remain unchanged; control/log roots retain strict privacy. A
readable Unix child beneath a private root requires the same owner and no links.
Bounds remain 1,024 retained registrations, 4,096 entries and a 512-KiB marker,
with at most 64 files per root turn and one root per metadata tick. Four boundary
cases remained under self-review before the earlier focused r36 run selected
28 tests: 24 passed and four failed. That failure remains historical. The final
r36 native selection passed 389/389, with 2,149 tests outside the filter
(164.091 s total, 101.863 s tests); all consumed inputs remained unchanged.
Workspace Clippy with all features and all targets passed (42.936 s). The
selection includes the corrected filesystem guards, all 18 Store regressions,
active accepted work, funded closed-stage and foreign-capsule handling,
zero-index behavior and actual CLI/MCP transport coverage. These component
results alone did not qualify installed cleanup. The ledger owns the full
commands, identities, root-cause notes and qualification ladder; earlier failed
fixtures retain their original identities.

## Historical r37 correction and scoped r36 qualification

The sixth owned native r36 journey passed (106.995 s): both indexes rebuilt,
pause/watch restored and sources preserved (2/2); all seven held predecessor
PIDs exited; cleanup completed, fresh HTTPS worked, old authority was gone and
uninstall was acknowledged. Earlier failed journeys remain historical evidence.

The enabled-extension r36 run failed (149.132 s): 1,171 collected, 1,151 passed,
17 failed and three skipped. Its native fresh-managed baseline resolver treated
the published selection as an owned payload before consulting managed policy.
The central r37 guard checks managed policy first and cleared independent
source review. Five actual native T0 tests
passed (27.201 s total, 1.892 s tests), scoped CLI Clippy passed (23.086 s), and
workspace Clippy with all features passed (19.666 s). The broader native
selection passed 394/394, with 2,149 outside the filter (135.647 s total,
100.612 s tests), with consumed inputs unchanged. The lightweight main gate
with no default features and all targets passed (15.369 s total, 15.17 s Cargo);
its existing test-only unused `live_https_status` warning remains.

Mac r36 checks used frozen Rust identity prefix `cf81`: scoped Clippy passed (151.4 s)
and 393 native tests passed (508.713 s total, 215.487 s tests). The first Python
run failed on ordering/cold-artifact setup; the corrected run completed 919 tests
with 19 skipped and no failures. The original Node run had 24 failures from a
171-byte Unix socket fixture path. After cheap guards and a short private
0700 fixture, the separate third Node run collected 1,170: 1,137 passed and
33 skipped (14.203 s). Both original failed Mac runs remain preserved. This
component evidence does not qualify the newer r37 Rust delta.

The fresh Windows r37 debug runtime build passed (62.465 s total, 20.13 s
Cargo). The actual native-enabled extension suite collected 1,171 tests:
1,168 passed, three original skips and no failures (319.368 s); all 211
generated inputs remained unchanged. This supersedes the failed r36
enabled-extension result for this tested scope.

The authentic 1.8.11 to 1.8.16 two-index clean-break journey passed (105.505 s,
activation 22.736 s). Both indexes rebuilt, paused/watch policies were restored
and both sources remained preserved. All seven held predecessor identities
exited with zero extra signals; cleanup completed. Both fresh HTTPS connections
retained full parity, uninstall was acknowledged and teardown completed with
zero errors. Earlier failed fixtures and identities remain preserved.

The fresh r37 Mac ARM batch passed from its pinned immutable public package
(704.004 s, 21 reports collected). Scoped Clippy passed (151.173 s); native tests
passed 398/398, with 2,105 outside the filter (507.440 s stage, 218.715 s tests).
Python ran 923 tests with 19 skipped and no failures (13.927 s stage, 13.114 s
tests). The full mocked extension suite collected 1,170: 1,137 passed, 33 skipped,
0 failures (14.747 s stage); npm installation took 1.501 s and compilation
7.934 s. All 209 generated outputs, including 104 test files, remained unchanged.
All owned descendants settled, evidence was retained before cleanup, and Cargo
cleanup passed (0.905 s) with an empty target; retained fixtures remain preserved.

This is ARM component qualification, not Mac native-enabled extension,
installed Mac/GUI, Intel, signing or public-release qualification. Earlier r36
Mac runs and their original failures retain their separate historical identities.

The r37 VSIX was created and all seals verified, but installed Windows editor
gate `clean-break-r37-installed-editor-r1` failed (47.833 s). The untrusted case
passed. In the multi-index case, after index creation and trim-policy Apply, the
next set Create failed with `catalog_outcome_unconfirmed_reconcile_first`; the
same-root case did not run. The editor was forcibly closed. No
`native-stop.json` receipt exists, so cleanup is unqualified; this is not an
observed native Stop failure. Trust cleanup is not claimed. The failed fixture,
profile and receipts remain at
`artifacts/editor-qualification/native-installed-Ujos45`.

All 211 generated outputs and the full package freeze remained unchanged. The
earlier Windows native-enabled suite and owned two-index journey, and the Mac
component results above, retain their passing scopes. At the r37 installed-editor
stop, that VSIX was not accepted for user installation; full workspace/T3
qualification stopped. Investigation and cheap regressions were planned before
the subsequent r38/r39 corrections described below. Versions
remain 1.8.16, and no publication is claimed.

## R38/R39 evidence at the stop

R38 extension checks collected 1,291 mocked tests: 1,270 passed and 21
skipped. Native-enabled checks collected 1,292: 1,289 passed and three
skipped. These retain their exact component scope. The passing authentic
r37 two-index reset separately proved both rebuilds/restored pause-watch/source
preservation, all seven predecessor exits, cleanup, fresh HTTPS and uninstall.

The R38 installed UI run failed on the sequence-16 response budget. R39's
portable parse-budget and catalog-schema corrections passed 67 selected native
regressions and scoped CLI/MCP/Kernel Clippy. Full Python passed 923 tests with
13 skips. Its first actual transport preflight failed on a direct scoped
capabilities size refusal, before the parse assertion. The strengthened scoped
fixture proved that a guessed 1,024-byte receipt reservation rejected a complete
reply that fit the actual limit. The correction measures complete actual bytes,
including the typed delivery receipt, with unchanged read/plan/transport bounds.
All 67 scoped regressions then passed, followed by actual one-/two-index native,
CLI and HTTP-MCP parity: one selected process test passed in 33.959 seconds total
(11.854 tests), retaining full data, scope and genuine delivery proofs. The real
two-index delivered body is 64,925 bytes within 65,536. The fresh r39 runtime,
workspace Clippy and authentic two-index clean-break upgrade passed. The full
native-enabled extension suite failed with 1,288 passed, one failed and three
skipped; after a test-only onboarding mock correction, the affected 117-test
module passed. The full mocked suite passed (1,270 passed, 21 skipped), before
the later installed-driver test additions. Final package verification passed;
all three R39 installed-editor attempts failed. The latest stopped before
enrollment with `workspace_registration_changed`; its cause remains
unresolved. Current Neo execution was not launched. R37 Neo native/component results
retain their frozen source scope; they do not qualify the later TS journal or
r39 native delta. The ledger retains exact receipts and pending gates.

## Earlier correction work — superseded before installed qualification

The unpublished `legacy-reconcile-continuation/1` adapter had been implemented
and tested for a narrow 1.8.11 pair to 1.8.16 single-image transition. It is
superseded by the clean break and is not part of the current compatibility
policy. Its test/package identities remain in the ledger. Passing those scoped
checks did not qualify the new reset path or prove the user's upgrade succeeded.

Explorer also retained its initial startup message after the installation output
recorded failure because startup waited for an early connection read before
publishing failure. Failure is now published immediately even if that read is
still pending, and the header stops reporting connecting for terminal startup or
upgrade failures. Progress text describes preparation without claiming shutdown
before native preflight passes. Loading replacement extension code is not itself
proof that a visible panel received state; regression coverage observes the open
panel's failure publication.

Management → Service → Runtime → **Complete runtime diagnostics** now opens,
copies and exports the complete cached native bootstrap and diagnostic JSON with
its capture timestamp. Host-owned actions trigger no new native reads. Summary
copies identify their selected subset and point to this full route; cached
observations are not current health, cleanup or successful-upgrade proof.

Earlier correction checks include the full Python suite (896 tests, 13 skips)
and 14 versioning tests; their exact identities and later correction checks are
retained in the ledger. They precede this policy switch and do not qualify the
clean-break implementation. New native/extension checks, installed reset and
cleanup acceptance, complete workspace checks, platform qualification and
publication remain separate gates.
The [1.8.15 record](release-1.8.15.md) retains the earlier candidate evidence; the
[1.8.14 working record](release-1.8.14.md) retains the broader implementation and
qualification history. No publication is authorized by this patch.
