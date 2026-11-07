# AIRK 1.8.9 working build

6 November 2026. Unpublished Windows manual-test candidate on `feat/finetuning`;
no release or Marketplace publication is implied. This patch follows the dated
[1.8.8 working build](release-1.8.8.md).

Compatible installed runtimes upgrade automatically, including 1.8.7 and 1.8.8.
Explorer now follows the current service controller after activation and obtains
native capabilities through its authenticated window-context read. Previously,
the response omitted capabilities, some context updates discarded them, and an
open Explorer could retain methods bound to the disposed controller. These gaps
left **Split an index** disabled despite native support.

The current runtime advertises policy contract 3; split review needs contract 2
or later. Existing supported policy formats 0, 1 and 2 become split-capable
without manual migration, reconnect or reindexing. Capability is separate from
the policy format required by an index's existing rules. Policy changes still
use their reviewed native transactions.

Overview, Indexing and Management show observed upgrade stages, versions,
elapsed time and failures. Split explains an active upgrade or an unconfirmed
capability read. Completion requires the current matched runtime binding;
reconnect failures cannot report success. Capability reads use bounded typed
retries, and identical complete metadata preserves pending management choices.
Changed metadata, trust, runtime or window identity still invalidates stale work.
Source contents, accepted jobs, saved scopes, grants and pause/manual choices
retain their existing owners.

The [MCP QA follow-up](../reviews/mcp-qa-1.8.9.md) evaluates all thirteen supplied
observations. Corrections preserve typed errors and exact safe read retries,
current-source verification and attribution, frozen scope disclosure, open
producer filters, canonical schema bounds and complete full-mode results.
Terse capabilities explicitly summarize Workbench schemas. Search help and
literal snippet focus clarify lexical matching; graph responses disclose their
actual limits. Intentional verification costs and authority limits remain
explicit. The original reported outage's underlying cause remains unproven.

## Validation

The current native test-source identity is
`f200722c2bf21be2c443384f7daf415667b4fafe5c52d5245dd9b69deba2f50a`
(978 inputs). Native, extension and lockfile versions agree on 1.8.9.

| Check | Observed result |
| --- | --- |
| Windows full strict workspace Clippy | Passed, 11.751 s, all targets/all features. |
| Windows final full extension suite | 863 total: 859 passed, 4 skipped, no failures; 178.925 s including compilation. Ownership focused checks: all 124 passed. Final retry target: all 11 passed; scoped CLI strict Clippy passed in 8.346 s. |
| Windows Python contracts | 482 run, 9 skipped, passed; final harness contract rerun 25.776 s in unittest. Editor-helper JavaScript regressions: all 14 passed. |
| Native process preflight | Expanded 209-case run: 207 passed; two external fixture/protocol assertions corrected and both passed in a separate 11.411 s stage. Full MCP unit suite and actual Native/CLI/MCP source verification and busy-read paths passed. |
| Actual original 1.8.8 indexed upgrade | Passed; activation 24.443 s. Complete Index, Project, membership, grant, registration and source values preserved; native policy capability 3 and split baseline available. |
| Real-model service clients | Passed, 39.192 s. |
| Windows full native campaign | 2,720 run: 2,716 passed, four stale schema/documentation checks failed, 51 additional skips; 2,302.209 s in Nextest. All four restored campaigns passed. Assertion-only corrections and generated documentation then passed all seven affected checks in a separate 19.836 s stage, with production inputs unchanged. |
| Windows doctests | Passed across all eight crates; zero doctest examples. |
| Mac Neo full campaign | 2,683 run: 2,679 passed, the same four stale schema/documentation checks failed, 51 additional skips; 1,921.685 s in Nextest. All four restored campaigns passed. Full strict Clippy, lightweight main, workspace build, doctests and real-model clients passed. Python: 482 run, 476 passed, 6 skipped. |
| Mac Neo native correctives | All 21 passed: seven schema/help/reference checks plus all 14 platform retry cases; 27.368 s execution, 28.024 s stage. Scoped strict CLI Clippy passed in 177.496 s with incremental compilation off. |
| Mac Neo final full extension suite | 863 total: 849 passed, 14 skipped, no failures; 1,188.771 s. Files ran serially with each fixture's internal concurrency intact. All 14 editor-helper tests passed. Final Python: 482 total, 476 passed, 6 skipped; 8.058 s. Compile, notices, repository and standard whitespace checks passed. |
| Mac Neo earlier extension runs | Original 856-case run: 841 passed, one diagnostics failure, 14 skipped; 1,169.752 s. The failure exposed the catalog-clock validator mismatch described below. A later 859-case run passed 845 with 14 skips in 1,189.430 s; that source precedes the final pending-observer correction. Both remain separately identified. |
| Optimized Windows runtime and VSIX | Fresh release pair passed; 820.561 s. Real-model clients passed in 25.481 s. Native ZIP and 776-file Windows VSIX sealed and verified. |
| Windows four-core runtime checks | All 15 groups passed across five rounds of MCP smoke, real-model clients and managed installation/upgrade/rollback; 1,659.550 s, four-core affinity, no load workers. Reused the exact optimized runtime; this is not another native build. |
| Actual installed Windows editor | All five selected scenarios passed on the same final VSIX: different-Project preflight 73.257 s, then same-Project, multi-Project/reload, Usage and Restricted Mode in 230.084 s. Canonical cleanup and source-preservation evidence retained. The separately qualified historical legacy-editor fixture was not selected. |

All Windows Cargo work reuses `artifacts/v190/p10-target`. Development checks
enable incremental compilation; optimized builds leave the release profile's
incremental policy unchanged. Commands, scope, elapsed times, results, logs and
Cargo HTML reports are retained under ignored `artifacts/qa-1.8.9-windows/`.

The development runtime retains its original compiled-source identity
`38ba15b4bf72ff08bc2a1db596c430f45bc67d332700a7f3eecf4285764875e7`.
After external CLI test corrections, all 978 inputs were compared and the
complete 675-file runtime seal reverified: only five reviewed external test
files differed; production inputs and payload bytes were unchanged. Its reuse
is recorded separately from the current test-source identity and is not labeled
a fresh build. The optimized installable pair was freshly built from
`73130de0afbf819464d42cd5f576a6743dbe7bb977666358410664124956ea99`;
its complete 675-file seal and all native inputs were then compared after two
external retry-test files changed. Production inputs and payload bytes remained
identical. Its original compiled identity is retained alongside the newer test
identity. Neo's fresh development pair was compiled from
`acc3d1df74d5ee575f59f3f247b5c585bbf81f5ac40b3001df8e02e86f5c6d99`
and has 678 sealed artifacts; corrective reuse requires the same complete input
and payload comparison. Eight Neo Cargo timing reports were collected and
verified before its required Cargo cleanup. Four further corrective Cargo timing
reports were retained and verified; all twelve remain outside cleaned targets.
The final 266 extension/package inputs and two editor-helper scripts matched the
frozen Mac snapshot after testing. The original native seal and all production
inputs were reverified. Cargo cleanup exited zero, the exact target was removed,
and the owned runner/compiler process group was absent. An earlier isolated
snapshot's staged-tree whitespace command lacked historical Git child trees;
that harness failure is retained separately. The final standard whitespace check
and the live Windows changed-line check passed.

Windows extension skips are two POSIX-only cases, the opt-in F09 RuntimeHost
campaign and a separately supplied historical helper pair. Mac skips are twelve
Windows-only cases and the same two optional campaigns. Python's unavailable
historical pre-1.4 helper and platform/signing fixtures remain explicitly skipped.
The 51 native compiled skips are recorded separately from the source ignore
inventory; its source-attribute count is not a compiled test count. No Linux,
container or hosted publication workflow was run for this manual candidate.

The full Mac extension run found that native preparation can advance only the
catalog metadata clock while retaining the exact reviewed authority. The shared
client validator now accepts that monotonic clock change for explicit,
unsubmitted retry receipts, while preserving original arguments and deadlines.
Backward clocks and changed scope or grants remain terminal. The two-index
diagnostics fixture also declares its existing two-resident-project prerequisite.
The observed capacity refusal does not establish a physical-memory root cause,
nor does this finding establish the cause of the original report's outage.

The unpublished Windows VSIX is
`artifacts/qa-1.8.9-windows/vscode-r3/publish/airk-1.8.9-win32-x64.vsix`
(74.06 MB), SHA-256
`8ad68bacf7e5e00e24c10969b7d9e36221715125481cebe0f6a1dd07863b6879`.
It contains the verified native ZIP with SHA-256
`7768aa3cba699050e9d449c19e8ef05b5dc9ccfcc59340b4e2e11d891b4ccba4`.
The first installed-editor harness attempt used an obsolete flat VS Code CLI
path and stopped before loading AIRK. The corrected attempt uses the versioned
CLI path declared by this machine's `code.cmd`, with private profiles only.

That first packaged attempt passed the same-Project editor case, then timed out
in different-Project setup before dispatching Apply. The retained draft had no
window descriptor. Source tracing and delayed-owner regressions showed that the
first successful automatic context read could invalidate an earlier setup
review. Initial Explorer catalog pages now await authenticated window ownership
before admitting a review; direct setup admission also checks that prerequisite.
Later owner changes still invalidate reviews. No fixture workaround or mutation
retry was added. The final VSIX above includes this correction and the pending
observer correction below; all five selected editor scenarios passed. The failed
fixture exited idle naturally, its recorded process was absent, and its missing
UI Stop receipt remains recorded as unqualified separately from those observations.

The next editor attempt reached setup completion but its following page was
cleared. A targeted diagnostic run retained the native view changing from
generation 6 to an empty generation 8 with the same window descriptor. A
deterministic controller regression then reproduced the cause: an automatic
retry remained pending after an explicit context read succeeded, and later read
catalog revision 9 after the current page had established revision 7. A complete
validated observation for the current owner now satisfies and cancels that
pending retry. Malformed, mismatched, missing and retired-owner observations
still cannot satisfy it. No owner-invalidating rule or mutation fence was relaxed.

The editor harness now uses one 120-second action deadline inside its 150-second
driver wait, with bounded independent native/DOM diagnostics. Diagnostic reads
are best effort and cannot replace a valid native result. Earlier failed fixtures
retain their unqualified UI Stop evidence; separate reports verify natural idle
exit and process absence. Their sources and index state were retained.

## Earlier observations retained

The pre-QA source was
`8bcf41751e4cd4281fcc34e0ad649cae8b509ee4b4542488a19e83e2a0cfa462`
(973 inputs). Its original full Windows run had five protected-boundary fixture
failures without the required private base. Neo r2 had nine native and four
extension failures: several fixtures used symlinked `/var` temporary paths,
while others exposed retry receipt/projection gaps corrected here. Those runs,
their complete outputs and timing reports remain identifiable; their passing
parts are not relabeled as final qualification. Both hosts ran all four restored
routine campaigns in those earlier runs. Neo evidence was collected before its
required Cargo cleanup.

Integrated iteration records also retain compiler/lint failures, legacy versus
modern MCP negotiation assumptions, incomplete retained-producer fixtures, and
the automatic-observer selection race. Assertions and protected-read boundaries
were preserved. No 1.8.8 binary or earlier test result is relabeled as 1.8.9.
