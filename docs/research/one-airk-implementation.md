# One AIRK 1.8.14 implementation ledger

Started 7 November 2026 against the [implementation plan](../one-airk.md).
Implementation, a new Windows VSIX and final tests of the completed changes on
Mac Neo over SSH are authorized. No PR or publication is requested.

## Baseline and ownership

The starting source version was 1.8.12; the configured live MCP service reported
1.8.11. Those identities must not be relabeled as new builds. Indexed source is
navigation evidence only; affected source is verified on disk before editing.

The checkout already contains uncommitted installed-editor, container-check and
test-helper work, plus the plan and documentation-index entry. Preserve those
changes and record the full final build inputs rather than attributing them all
to this implementation. There is no newly qualified native artifact yet.

The primary agent owns integration, contract approval, this ledger and Windows
Cargo execution. Independent agents investigate client trust, static inference
and Mac readiness, and implement version synchronization and the bounded safe-read
retry. No parallel Cargo runners are authorized in this checkout.

## Work status

| Card | State | Evidence / next gate |
| --- | --- | --- |
| C0 baseline/version | implemented | 1.8.14 manifests/locks agree. Windows debug r8 built and sealed; its actual install/HTTPS/upgrade/rollback/retirement/uninstall gate passed. No accepted final VSIX exists. |
| C1 static inference | in_progress | Windows debug r8 links the verified ORT 1.22.1 static inputs and passed builder model/import checks and actual strict-TLS installation. Signed Mac and final optimized artifacts remain pending. Original static-library reuse retains its recorded build and runner-cleanup limitations. |
| C2 client trust | in_progress | Actual Codex MCP accepted the local trusted CA and rejected an untrusted issuer before HTTP. User approved normal CA validation with existing trust preserved after the additive-bundle experiment; installation pinning is not required. Remaining client/credential/rotation checks are pending. |
| C3 contracts | in_progress | Typed endpoint/credential selections, original read-attempt context, inference completion and checkpoint contracts compile and have focused integration coverage. Final full transport/platform qualification remains pending. |
| C4 in-process inference | in_progress | Windows actual model/client checks passed on optimized r2 and the matched current debug r18, including embedded 384-dimensional repeatable inference. Neo r9 linked debug but its model smoke failed private-root protection; the private-root correction is reviewed and Windows-tested. Fresh Mac/model/resource qualification remains pending. |
| C5 canonical HTTP/API/retry | in_progress | Streamable HTTP and raw JSON share canonical backends; typed pre-admission 50 ms retry, bounded sessions/output and native final-write fences authored. Accepted IPC completion ownership is being integrated; no full parity or transport qualification claimed. |
| C6 direct manager/endpoint | in_progress | Modern single-image launcher preparation/repair now returns without copying the sealed main; retained predecessor launchers require explicit migration retirement. Installed signed lifecycle and sealed-image preservation remain qualification gates. |
| C7 checkpoint/activation | in_progress | Typed terminal-receipt classification passed 39 checkpoint tests and five actual split cases. The actual ignored checkpoint owner test passed; sealed Windows debug r8 passed installed upgrade/rollback and genuine 1.8.12 semantic upgrade r13. Remaining predecessor/platform qualification is pending. |
| C8 all consumers | in_progress | Latest enabled-native suite ran 1,044: 1,039 passed, two failed, three skipped. Reviewed harness corrections passed their actual focused Assistance/provider-stop cases; the complete enabled suite is not yet a pass. Actual enrollment passed three cases. Editor acceptance remains pending. |
| C9 predecessor retirement | in_progress | Genuine 1.8.12 upgrade passed its recorded Windows scope. Sealed r17 positive adoption r20 passed both authentic 1.3.1 and 1.0.0-build.7 cases; the real corrupt-copy/restoration negative campaign also passed. These establish the tested formats/paths, not every predecessor or other platform; final qualification remains pending. |
| C10 distribution/docs | in_progress | Windows r20 workspace campaign passed 3,224 tests and doctests; retrieval r6 passed two workloads/nine cases. The later schema-13 correction passed 14 focused tests; the corrected storage lifecycle gate passed on reused, sealed r21. Neo r10, current-source editor/signed acceptance and final VSIX remain pending. |

## Required final evidence

Record exact source/build seals, commands, target directories, incremental setting,
elapsed time and result for builds and tests. Retain Cargo timings. Distinguish
source implementation, tests, installed qualification and publication; none imply
the next. Keep private credentials, machine configuration and index state out of
this public ledger.

The final Mac batch must test the completed changes and actual signed native
lifecycle, not just an earlier feasibility snapshot. Retain logs and needed
artifacts before the required Neo Cargo cleanup, including after failed batches.
Windows packaging must use the new verified 1.8.14 native artifact, not a renamed
predecessor VSIX or reused binary with a new version label.

## Integration checks to date

Scoped Windows Clippy uses `python scripts/dev-check.py -p airk-core -p
airk-process -p airk-mcp --target-dir artifacts/v190/p10-target --incremental on
--skip-fmt --cargo C:/Users/tdotn/.cargo/bin/cargo.exe`, which adds `--timings`,
`--locked`, all targets and denied warnings. The first run failed in 34.528 s on
a large selection enum; boxing its imported payload preserves the wire contract.
The second failed in 36.631 s on an unchecked SQLite `u64` read in checkpoint
capture; the source now uses checked conversion from SQLite's signed integer.
A third attempt stopped before compilation because the static ORT feature change
required a lockfile refresh; concurrent source formatting also observed a test
module before its file had been written. Neither attempt is counted as a pass.
Timing reports and complete logs are retained in ignored local artifacts.

After correcting SDK construction, optional verification allocation handling,
test factory setup and feature-specific helper ownership, scoped Core/Process/MCP
Clippy passed (Cargo reported **35.08 s**). Their library test run passed **326
tests**: Core 160, MCP 154 and Process 12; zero failures or filtered tests. The
wrapper took **128.248 s** including compilation. This includes real SDK
Streamable HTTP sessions, raw JSON contract preservation, concurrent response
preferences, session bounds, accepted-work lifetime and canonical selection
retry tests. It excludes the native CLI gateway and final installed runtime.
A subsequent tightening of the original settings-retry deadline needs its own
targeted rerun before final qualification. That rerun subsequently passed
**329 tests** (Core 162, MCP 155, Process 12), with no failures, skips or
filtered tests, in **116.546 s** including compilation. It covers the updated
retry deadline and exact imported-union contract; native CLI qualification
remains separate.

The first CLI static-inference Clippy pass stopped in **27.311 s** on an obsolete
model-verification helper, before checking CLI source. Its removal preserves
tests through the canonical verification helper. This is not static executable
linkage or model qualification. The second CLI pass failed in **63.445 s** on
seven integration errors in routing, error conversion and updated test
constructors. Those were corrected. The third failed in **61.500 s** because
the source-watcher checkpoint reply handling was not yet integrated. That barrier
was subsequently completed in source. No CLI success is claimed from these runs.

After checkpoint integration, another attempt stopped before compilation on the
new test HTTP client's lockfile requirement (**0.368 s**); an offline refresh
added its cached `want` and `try-lock` dependencies. The next pass failed on
seven storage-version range-pattern lints (**31.161 s**), now corrected. The
following pass reached CLI and found two type/call-site errors (**48.127 s**):
an endpoint needed unboxing and an existing activation test needed to await the
new asynchronous completion barrier. Both were corrected; integrated success
and actual native tests remain pending. These scoped runs use
`artifacts/v190/p10-target`, incremental compilation **on**, and retained Cargo
`--timings` reports.

Scoped CLI Clippy subsequently passed for **all targets** with
`--features local-inference`, denied warnings, the same persistent target and
incremental setting (**9.922 s** including formatting; Cargo **3.74 s**).
Earlier complete logs retain the retired-helper cfg, API grouping and test-client
corrections that preceded this pass. Further inference completion changes still
need rechecking.

The first selected native Nextest attempt failed during compilation in
**6.957 s**, before any test ran: `ort-sys` requires a static `re2` library,
while the C++ producer built only the `onnxruntime_session` dependency closure.
The correction must build the real pinned `re2` target and verify the complete
Rust adapter link inputs on Windows and Neo. It must preserve the original
library/build receipts and record any supplemental build separately. Static
executable linkage and native test success are not established by Clippy.

The Windows static-library adoption verifier passed in **32.144 s** after
completing the pinned license inventory. This verifies the existing library
build and preserves its original postflight failure; it is not a fresh build.
On Neo, the ARM64 static ORT prerequisite build passed: configure **6.738 s**,
compile **645.473 s**, total **654.312 s**, producing 79 ARM64 archives. The
initial Python-interpreter configuration failure is retained separately.
Neither prerequisite result establishes AIRK linkage, inference, signing or
installation success on that platform.

The complete extension suite passed **930 tests**, with **21 optional skips**
and zero failures (951 total). Host, webview and assistance compilation and
bundle construction passed. The standalone Python HTTPS example passed two
real-TLS tests, including rejection of crossed CA/token inputs before sending
credentials. Repository checks passed for **1,920 public files**. These checks
precede the remaining native integration and final source freeze.

After adding the protected VS Code credential input command, the complete
extension suite passed **934 tests**, with zero failures and **21 optional
skips** (955 total). Native command resolution and actual external VS Code host
trust still require installed qualification. Further caller corrections require
the next complete suite, rather than adopting this result for changed source.

Focused extension HTTPS/bootstrap/migration checks passed **49 tests**, with one
platform-specific skip. Focused packaging/signing-selection/release-format tests
passed **95 tests**, with three expected macOS platform skips, in **20.111 s**.
These packaging tests use fixtures and mock publication; no release was
published, signed or installed by those tests.

Long-lived TLS leaf renewal is wired into service maintenance. The local CA's
eventual replacement still requires explicit client trust re-enrollment; current
code fails before issuer expiry instead of silently replacing a trusted issuer.
Real VS Code MCP-host trust provisioning remains an acceptance gate separate
from the extension's own HTTPS client and the successful Codex CA experiment.

## Additional integration evidence, 7 November 2026

The additive Windows static-library correction passed in **13.643 s**. The
upstream default target excludes `re2`, while the pinned Rust adapter requires
it. Building that explicit target produced the real static archive; verification
confirmed all 82 original archives were unchanged and all 52 consumer-required
archive paths were present. The supplemental receipt is separate from the
original build receipt. This establishes library availability, not successful
final symbol resolution or model execution.

Core/Store all-target Clippy, including formatting, passed in **35.368 s** with
`python scripts/dev-check.py -p airk-core -p airk-store --target-dir
artifacts/v190/p10-target --incremental on --cargo
C:/Users/tdotn/.cargo/bin/cargo.exe`. Cargo timing evidence is retained. The
preceding run failed on a test-only SQLite `usize` read; the assertion now reads
the supported signed integer type. The latest full extension run passed
**938 tests**, with **21 optional skips**, zero failures, and successful
compilation/bundling. Native linking, installed tests and final Mac qualification
remain separate pending gates.

The corrected archive set linked the native CLI test executable successfully.
The selected `local-inference` Nextest pass ran **78 tests: 76 passed, two
failed**, in **104.889 s** including compilation. Both failures rejected a
generic temporary directory's ownership permissions. The subsequent full CLI
library pass ran **607 tests: 600 passed, seven failed, 19 skipped**, in
**89.833 s** including compilation (**67.356 s** tests). The original two fixture
failures passed after using newly created private control directories. Remaining
failures comprise four split/discovery fixtures rejecting inaccessible ignore
metadata, two obsolete launcher/schema expectations, and another private-owner
fixture. These are retained failures, not qualified passes. Both runs used
`cargo nextest run --locked -p airk-cli --no-default-features --features
local-inference --lib --profile ci --timings --target-dir
artifacts/v190/p10-target`, incremental compilation on; the first additionally
selected HTTP, inference and checkpoint cases. This is test-executable linkage,
not the final packaged executable or completed Mac qualification.

After the cumulative native completion ledger and TLS output-fence changes,
Core/Store all-target Clippy passed in **37.331 s**. CLI all-target Clippy with
`local-inference` passed in **15.450 s**, including formatting, using the same
persistent target, incremental setting and retained timings. Intermediate runs
retain type-inference, test-only visibility and lint failures; none are counted
as passing tests. The next native test run includes the full CLI library and
actual HTTPS/CLI parity fixtures.

Read-only inspection of the configured Ninja dependency graph found that
`onnxruntime_session re2` omitted nine archives required by the Rust consumer.
Fresh provisioning now builds `all re2`; its graph covers all 52 required
Windows archive paths. This correction does not rerun or relabel the original
prerequisite campaigns. Neo's exact supplemental archive was recovered with a
separate receipt after verifying unchanged original inputs; its failed command
exit remains unknown. Recovery and subsequent verification took **5.442 s** and
**2.495 s**. Final AIRK model, signing and installation tests remain pending.

The next integrated native run completed **615 tests: 599 passed, 16 failed,
32 skipped**, in **172.891 s** including compilation (**79.651 s** tests).
Actual buffered-TLS cancellation/deadline and keepalive/pipelining cases passed,
as did 130 native inference-owner restarts, persistence/tamper refusal and the
four isolated split fixtures. Modern fixture coverage exposed two installation
defects: managed genesis still assumed a helper image, and first activation
required a control directory before its owning initialization. Remaining
retirement/snapshot failures used obsolete helper/alias assumptions. The two
actual HTTPS integration cases stopped at fresh activation; they did not prove
end-to-end parity. Corrections remain subject to rerun. Subsequent CLI/Store
all-target Clippy passed in **36.295 s**, including formatting, with the same
static inference configuration and persistent target.

The targeted installation rerun completed **31 tests: 29 passed, 2 failed,
620 skipped**, in **58.750 s** including compilation (**23.596 s** tests).
All selected managed-genesis, fresh-activation, retirement and snapshot
regressions passed. Both actual HTTPS integrations advanced beyond installation
but failed decoding a response (`expected value at line 1 column 1`); transport
parity remains unqualified until that shared failure is resolved. The command
selected the CLI library plus `mcp` and `usage_native` integration binaries with
`local-inference`, the pinned Nextest CI profile and `--timings`, using
`artifacts/v190/p10-target` and incremental compilation on. The retained
`native-integration-r2.log` and JSON receipt record the complete filter and result.

The Core/Store library campaign ran **950 tests: 948 passed, 2 failed,
4 skipped**, in **299.577 s** including compilation (**235.096 s** tests).
Both failures required a private native creation boundary and correctly refused
the ordinary temporary directory's foreign write permissions. A new isolated
fixture directory was created with inheritable access for its current owner
only; existing directories and production checks were unchanged. Selecting that
directory through `AIRK_PRIVATE_CENSUS_BASE`, both exact failed tests passed in
**1.195 s** including Cargo startup (**0.380 s** tests). This qualifies the
controlled private fixture, not the original shared temporary namespace.
Both runs used `cargo nextest run --locked`, the CI profile, `--timings`, the
persistent `artifacts/v190/p10-target` and incremental compilation on. Initial
failure evidence remains in `core-store-final-r1.log`; the bounded rerun and
fixture descriptor are retained separately.

The next full CLI-library/selected integration run completed **628 tests:
626 passed, 2 failed, 32 skipped**, in **147.908 s** including compilation
(**118.630 s** tests). All selected library tests passed, including concurrent
port fallback without contacting the occupant, TLS delivery fences and 130
inference-owner restarts. Rust and Python HTTPS fixtures now decode the finite
SSE response used by stateful MCP initialization; the Python fixture's seven
focused tests include a real private TLS server and passed. The remaining native
integration failures exposed fixture assumptions: process-wide usage opt-out
overrode explicit persisted accounting policy, and a scoped lightweight status
was incorrectly treated as an owner-management progress report. These failures
remain recorded until their corrected actual-client tests pass.

The first final workspace/all-feature Clippy pass additionally exposed legacy
helper feature compilation gaps that shipping-feature checks had not exercised.
It did not pass; explicit legacy coverage is being repaired, not excluded. A
subsequent shipping-feature lint failure in the new SSE fixture was corrected.
The canonical static environment handoff was also exercised against the real
verified Windows prerequisite receipt, including its supplemental archive.

All **four required routine-excluded workload campaigns passed** in
**374.749 s** including compilation (**303.067 s** tests). The coalesced
Core-dependent Kernel/Store run selected the 200-edit retention plateau,
large-catalog compact usage measurements, sparse catalog edits and multi-page
manual retention backlog, with `--ignore-default-filter`, all features, the
pinned CI profile and retained `--timings`. It reused `artifacts/v190/p10-target`,
incremental compilation on, and the explicitly owner-private fixture base.
These results restore the four named campaigns; they do not replace the full
workspace suite or doctests.

Repository checks subsequently passed for **1,963 public files**. The first
complete Python campaign ran **646 tests**, reporting **5 failures, 3 errors
and 10 skips**. Seven failure/error observations were Windows PowerShell TLS
subcases blocked by sandbox Schannel credentials; the same focused seven-test
TLS suite passed outside that sandbox in **29.350 s**. The remaining error
identified 54 new dependency identities missing source-policy review. Review
and separately disclosed cargo-vet coverage remain required; the failed full
Python campaign is not reported as passing.

The completion audit also identified required issuer rotation as unfinished:
automatic leaf renewal was implemented, but the local CA could only fail closed
near expiry. Explicit native receipt-backed preparation, trust overlap,
checkpointed replacement, interruption recovery and owned cleanup are now being
implemented before final platform qualification. No final runtime/VSIX claim
follows from the component results above.

The later combined TypeScript build/test pass completed with **968 tests:
947 passed, 21 optional skips, no failures**, including issuer rotation and
distinct Explorer/Workbench client attribution. The installed-editor journey
still required migration from its old stdio-provider assumptions; this pass
does not establish installed-editor acceptance.

The 54 new native HTTPS dependency identities now have targeted source review
and explicit limited exact-version exceptions, recorded in
[the dependency review](../../supply-chain/reviews/native-https-2026-11-07.md).
The source guard passed; imported audits, local audits and Cargo.lock were not
changed by admission. Locked cargo-vet and fresh advisories remain separate
gates, and the exceptions are not full security audits.

The first out-of-sandbox full Python rerun was interrupted after an environment
mistake: it inherited the sandbox-owned private fixture base, which the normal
user process could not write. A direct empty-directory creation probe confirmed
access denial; the identified Python runner was stopped and its partial log
retained. The next rerun uses the normal user's temporary directory and Windows
TLS context, without changing existing ACLs or user configuration.
That complete rerun passed **669 tests with 10 documented skips** in
**70.473 s** (**71.144 s** including the runner), including the updated
local HTTPS setup and stale-extension build-order regression. This is Python
contract/fixture evidence, not a new installed AIRK runtime acceptance result.

The non-CLI workspace campaign completed **2,079 tests: 2,066 passed,
13 failed, 25 skipped**, in **1,041.213 s** including compilation
(**970.652 s** tests). All failures were Kernel source-policy/refresh fixtures
whose inherited global Git-ignore directory was inaccessible in the sandbox.
Read-only native path probes confirmed access denial. All **13 exact failed
tests passed** with the original user configuration outside the sandbox in
**36.984 s** including compilation (**27.138 s** tests). Neither production
ignore behavior nor existing ACLs were changed. Both runs retain timings and
their distinct environment/failure evidence.

Fresh advisory checks found four rustls 0.23.45 advisories, all patched by
0.23.46. The exact pin and lock entry were updated without changing other
dependencies; the complete patch delta received a separate targeted review.
The first combined Clippy pass then failed only a collapsible conditional in
issuer recovery. After that correction, formatting and **workspace/all-targets/
all-features Clippy passed** in **32.176 s**, with warnings denied, incremental
compilation on, the persistent P10 target and retained Cargo timings. Final
advisory revalidation and installed runtime acceptance remain separate gates.
The subsequent fresh dependency gate passed, including locked cargo-vet,
Rust/npm advisories and license/source policy under the existing dated exception
policy. Cargo-vet reports **46 fully covered, one partially covered and 372
covered by exceptions**; these counts do not mean all dependencies were audited.

After the installed-editor HTTPS harness and certificate-bundle publication
corrections, the full TypeScript build/test run passed **977 tests: 956 passed,
21 optional skips, zero failures**, in **21.093 s** including compilation.
The harness has not yet run against a new installed candidate.

The full CLI all-features campaign completed **898 tests: 826 passed,
72 failed (including parser-test stack aborts), 30 skipped**, in **634.039 s**
including compilation (**557.917 s** tests). The complete run is retained.
Correction work covers fresh fixtures lacking reviewed embedded-model authority,
HTTP protocol negotiation headers, old helper-process assertions, native
quiescence for full-result parity, command-coverage records, and parser tests
using the existing production CLI stack contract. It also found a real setup
validation defect when a global installation argument preceded its subcommand.
These failures are not represented as passing or waived by earlier scoped tests.

After the first combined correction, workspace/all-targets/all-features Clippy
passed again in **15.767 s**, using the persistent P10 target, incremental
compilation and retained Cargo timings. The actual parser regenerated the CLI
reference and its reference test passed. The follow-up HTTP-fixture and macOS
installation Python contracts passed **33 tests** in **1.619 s**. The targeted
native correction campaign completed **75 tests: 66 passed, nine failed** in
**147.310 s** including compilation (**120.979 s** tests). The failures expose
remaining model-directory assertions, command coverage, setup-parent and roster
fixtures, required-verification response parity, protocol metadata and the old
headless fixture. Corrections remain pending; these checks do not supersede
the failed full CLI campaign or qualify a new installer. Workspace default-feature
doctests also completed successfully in **62.680 s**, with zero runnable doctests.

The next combined correction passed full workspace Clippy in **8.821 s** after
removing two test-only lint errors. The following native/MCP campaign completed
**198 tests: 191 passed, seven failed**, in **155.646 s** including compilation
(**113.974 s** tests). Required-verification error parity and actual HTTPS protocol
negotiation passed. Remaining failures identified initial setup observation order,
a configuration-preview race, a legacy unregister request correctly requiring
reviewed retirement, and an overly narrow budget applied to an already-finalized
combined response. These remain recorded failures pending the next complete check.
The full extension suite after HTTPS smoke migration again passed **956 tests**
with **21 optional skips**, in **19.819 s** including compilation; native-enabled
extension acceptance remains pending. The transformer gate's verified custom-input
copy also passed against the actual compiled main's selected manifest; this is
input verification, not installed semantic qualification.

## Current gate checkpoint

The five-case native follow-up ran **five tests: three passed, two failed** in
**50.079 s** including compilation (**39.545 s** tests). The next exact two-case
follow-up ran **two tests: one passed, one failed** in **34.184 s** including
compilation (**24.905 s** tests). The remaining supervised-job fixture needed
its actual bound service runtime for embedded-model policy. Source corrections
were made; neither failed campaign is relabeled as passing. Their original logs
and receipts remain retained.

Scoped CLI all-target Clippy with all features subsequently passed in
**35.463 s** using `python scripts/dev-check.py -p airk-cli --all-features
--target-dir artifacts/v190/p10-target --incremental on --cargo
C:/Users/tdotn/.cargo/bin/cargo.exe`. This retains the persistent target and
Cargo timings. The complete CLI all-features **r2 campaign failed**:
**969 run, 947 passed, 22 failed, 30 skipped**, in **1044.904 s** of tests
(**1053.874 s** including its runner), with the same persistent target and
incremental setting. The retained log and receipt are
`artifacts/local/one-airk-1.8.14/cli-full-final-r2.{log,json}`.
Corrections address public-envelope limits, reviewed fixture authority,
retained background work, explicit post-catalog connection fences and
in-process provider identities. The combined correction campaign ran **38 tests:
29 passed, nine failed, 982 excluded**, in **433.298 s** of tests
(**486.214 s** including compilation and runner). Its selector included the CLI
binary tests, not the library cleanup regressions; those still require execution.
Preceding all-features CLI Clippy passed in **27.238 s**, retaining Cargo timings
in the same persistent target with incremental compilation enabled. Generated
CLI-reference checks passed (three tests). These results are retained as
`cli-combined-{clippy,correction-tests}-r1` and `cli-cleanup-reference-r1`.

The separate MCP/Store correction gate passed **161 tests** with **884 excluded**
in **25.286 s** of tests (**65.449 s** including compilation and runner).
Its scoped Clippy passed in **28.674 s**. This is component evidence, not installed
upgrade or cross-platform qualification. The remaining CLI failures exposed
concurrent authentication lock contention and public CLI routing/context gaps;
catalog-fenced connection expectations also need correction without weakening
delivery fences.

The user clarified that successful installation must remove the old launcher,
not retain it as a fallback. Post-configuration migration cleanup is being
integrated into native management, extension setup and local setup. Success
must require verified launcher absence; incomplete cleanup cannot be reported
as a completed update.

Replacement must work directly from any prior official installation format,
including standalone extension helper caches predating managed installation,
and from future HTTPS installations without requiring intervening upgrades.
Native origin capture and saved-evidence adoption are being added for those
older formats. Existing index bytes and provenance must be preserved; migration
must not silently reindex sources or fabricate historical extraction metadata.
The review also found missing automatic macOS legacy-process termination and
future HTTPS configuration refresh on endpoint relocation. Both remain required
before declaring the replacement path complete.

The TOML migration parser's six exact locked dependencies received targeted
source review and independent primary review, recorded as limited exceptions
rather than full audits. The fresh combined source-policy, RustSec, npm,
cargo-deny and cargo-vet gate passed (`dependency-toml-r3.json`). The first pass
correctly blocked unreviewed identities; the second exposed required cargo-vet
entry ordering, corrected with its formatter. Neither failed pass is relabeled.
The upgrade-script suite passed **19 tests** after adding obsolete-payload
absence, strict cleanup receipts, unchanged selected-service identity and
modern registry-contract checks. Actual native upgrade execution remains pending.

The first combined Core/Store/Kernel all-features Clippy attempt failed on three
new Kernel test-fixture API mismatches (**54.812 s**). After correcting those
fixtures, the same scoped all-target Clippy passed in **8.568 s**. Commands used
`scripts/dev-check.py -p airk-core -p airk-store -p airk-kernel --all-features
--skip-fmt --target-dir artifacts/v190/p10-target --incremental on`, following
scoped formatting and retaining Cargo timing reports. The ensuing selected
adoption/source-isolation campaign ran **27 tests: 23 passed, four failed, 1571
excluded**, in **1.054 s** of tests (**80.127 s** including compilation). All four
failures report a registry-layout/authority-marker mismatch in new catalog
adoption cases; that transition is being corrected, not bypassed.

The complete extension suite subsequently passed **991 tests**, with **21 native
gated skips** and no failures (`npm-launcher-finalization-r3.log`). This includes
closed-UI endpoint-change hints and retry behavior; the transport is mocked in
those focused orchestration cases. Actual rebuilt native/editor acceptance is
still pending. Authentic Mac 1.0.0-build.7 and 1.3.1 fixtures have independently
created saved indexes and served actual stdio MCP reads on Neo; their original
publication identities, configuration and payload hashes are retained. That is
predecessor-fixture evidence, not successful adoption by 1.8.14.

The next Core/Store/Kernel/MCP all-features all-target Clippy gate passed
(`legacy-authority-mcp-clippy-r1`, **47.887 s** observed runner wall time), using
the persistent P10 target, incremental compilation on and retained Cargo
timings. Its selected Nextest CI-profile campaign ran **193 tests: 192 passed,
one failed, 1,571 excluded**, in **27.144 s** of tests (**112.830 s** observed
runner wall time). All physical adoption layouts and the four previously failing
Store adoption fixtures passed. The sole failure was
`selected_public_routes_preserve_original_attempt_and_lifetime`: its `query_scope`
input omitted the required view. That fixture input has been corrected; the
replacement run is pending. This complete campaign remains failed.

The full TypeScript build/test gate then passed **1,000 tests**, with zero
failures and **21 native-gated skips** (**1,021 total**), retained as
`npm-launcher-finalization-r4.log`. The endpoint watcher uses real filesystem
events in focused tests but mocked native transport. Source now watches endpoint
publication even while Explorer is closed, treats filesystem hints as untrusted,
and performs bounded authenticated Health reads before guarded URL refresh.
Native-derived cache receipts distinguish `native-defaults` from `context-only`
installations; neither an editor path nor a test flag creates machine-wide
cleanup authority. These results do not qualify the rebuilt runtime, actual
installed editor, Windows VSIX or completed-source Mac Neo migration.

The full Python campaign ran **698 tests**, with **one failure and ten skips**,
in **71.571 s** including its runner (**70.889 s** tests). The failure required
an explicit one-shot reason for the offline Node Assistance request that checks
UI/help while native evidence is stopped. After documenting that reason,
**30 targeted tests passed**. The complete failed Python campaign remains a
failure; its replacement full-suite result is pending.

The later full Python command `python -m unittest discover -s scripts/tests -v`
ran **716 tests** in **68.570 s** (**69.108 s** including its runner), with
**five failure reports, two errors and ten skips**
(`python-full-final-r2.log`). All observed failures/errors were in the sandboxed
PowerShell branches of the local HTTPS examples module. With no source changes,
the entire affected module was rerun as the normal user using the existing
protected fixture base: **five tests passed** in **29.744 s** (**29.969 s**
including its runner), retained in
`python-https-examples-normal-user-r1.{log,json}`. This is observed
environment-dependent failure followed by scoped normal-user acceptance; the
generic example diagnostics do not prove one exact OS cause. File guards were
not weakened. The original full campaign remains failed and is not replaced by
a claim that all 716 tests passed.

Independent source review identified a production blocker: modern activation's
`Phase::Launcher` still copied the sealed main into a launcher path. That violates
the single-image/direct-manager contract and the original
[#66 installation report](https://github.com/iokaio/airk/issues/66#issuecomment-6710990197).
The source correction is in progress and still needs regression, rebuilt
artifact inventory and actual installed evidence. Mac R1 archive transfer and
restoration completed, but its build is held; these steps establish neither
Mac compilation nor model, signing, lifecycle or editor acceptance. This working
tree remains unpublished, with no accepted final Windows VSIX or completed Mac
qualification.

## Subsequent integrated migration gate

The Windows `legacy-cli-integration` r6 gate retained the persistent
`artifacts/v190/p10-target`, incremental compilation on and Cargo `--timings`.
Formatting passed in **6.838 s**; scoped all-feature/all-target Clippy for
`airk-cli`, `airk-mcp`, `airk-kernel`, `airk-store` and `airk-core` passed in
**23.713 s**. CLI reference generation/validation passed in **77.946 s**.
The same five-package selected Nextest CI-profile run completed **269 tests:
267 passed, two failed, 2,581 excluded**, in **228.892 s** of tests
(**313.699 s** including compilation and its runner).

All four previously failing split transport cases, cache reprepare cases,
shared adoption claims and corrected MCP context input passed. The two remaining
failures are the new TLS refusal test parsing chunked wire bytes as JSON and
the MCP terse/full-result integration timing out. The former received HTTP 403
before its decoder failed; that is not yet a passing body/fencing regression.
The timeout remains under investigation. This run is failed, not full workspace
or installed-platform acceptance. Logs and exact commands are retained in
`legacy-cli-integration-{format,clippy,reference,tests}-r6.{log,json}`.

Source review now confirms modern launcher preparation and repair skip copying
single-image payloads. Actual signed installation remains pending. The two-client
historical migration harness is authored and reviewed: both clients must share
one imported index/grant, receive distinct credentials, and migrate before old
cache retirement. Its focused Python tests passed four cases with one host
symlink-permission skip; genuine native execution has not occurred yet.

The subsequent complete Python suite ran as the normal user using the protected
fixture base: **730 tests, zero failures/errors, 11 skips**, in **73.391 s**.
The exact command was `python -m unittest discover -s scripts/tests -v`;
`python-full-final-r3.{log,json}` retains output, environment scope and wall time.
This includes the previously failing PowerShell HTTPS examples and the new
historical adoption harness contracts. It does not execute a packaged native
migration or replace final Mac and installed-editor acceptance.

The subsequent focused r7 run passed 27 of 29 cases, including all new native
origin classification/cache cases; its two failures remained the TLS fixture
shutdown and native help. Worker stderr identified `0xc00000fd` and a Tokio
worker stack overflow while first constructing the complete Clap help catalog.
The corrected factory uses one joined temporary thread with the existing CLI
stack allowance and caches the unchanged catalog. R8 Clippy passed in
**10.606 s**. Its three-test run passed the new small-stack catalog regression
and actual MCP terse/full integration; only the TLS fixture shutdown failed
(**18.368 s** tests, **41.534 s** including compilation/runner).

The TLS fixture retained a second response-byte owner after copying into rustls,
correctly preventing admission drain. It now releases that duplicate owner and
uses production flush/close tracking for the buffered stream. The exact focused
library regression passed in `tls-lifetime-test-r9.{log,json}` (**53.555 s**
including compilation/runner), retaining Cargo timings, the P10 target and
incremental compilation on. Both flush and read-driven cases retain exact HTTP
403/error-body, credential, stale-byte fence and bounded shutdown assertions.
The failed runs remain failed; these scoped passes do not replace final suites.

The failed r6 fixture's two surviving processes were subsequently stopped through
held Windows handles after revalidating their original creation times, approved
image hash, private installation and pending native Stop identity. Both exits
were observed, then that original native Stop completed. Original fixture files
and failure evidence were retained; no user service was targeted.

### Subsequent installed-runtime preflight

The complete normal-user Python r4 run passed **736 tests, zero failures/errors,
11 skips** (70.199 s test time, 70.823 s including its runner). Subsequent native
enrollment fixture changes passed scoped tests; a final complete run is still
required. The extension r5 run passed **1,011 tests, zero failures, 21 native-gated
skips**, including mixed standalone/managed configurations and interrupted
two-file migration sharing one native index. These are not installed-editor
qualification claims.

The subsequent full Python r5 run, including native enrollment driver changes
and explicit strict TLS validation, passed **744 tests, zero failures/errors,
11 skips**, in **71.594 s** test time (**72.275 s** including its runner).

The sealed Windows debug r4 runtime built successfully in **42.707 s**, including
static model/import and complete payload checks. Its installation preflight
passed staging and native HTTP enrollment but failed strict Python 3.13 TLS
validation: the generated server certificate lacked an Authority Key Identifier.
The shared Python transport now explicitly enables strict validation on older
Python versions too. Native issuance and same-CA leaf repair are being corrected;
this artifact is not a qualified installer. The final historical adoption and
schema-1 semantic upgrade stages have not run because installation failed first.

Earlier preflight failures identified a too-deep static license payload path,
use of forbidden console HTTP enrollment in test drivers, and Windows extended
path identity comparison in the shared driver. Packaging now preserves original
license bytes and provenance under bounded flat names; drivers use native
`runtime http-connection` and filesystem identity checks. All failed receipts
remain retained. Current source requires a new sealed build after the TLS fix.

The certificate correction authenticates the saved issuer key and original leaf
before renewing an older leaf under the unchanged CA. Tests cover fresh,
renewed and staged certificates, immutable old stages, and refusal to repair
tampered certificates or deficient issuers. The scoped r10 Clippy run failed
after **48.403 s** on a new external benchmark test comparing a typed digest to
a string; its test stage did not execute. The comparison now parses the native
health digest explicitly. Review also added admission guards preventing public
manager registration/policy changes during pending issuer or saved-index
maintenance. Ordinary Stop and authenticated internal recovery retain their
existing behavior. Combined r11 Clippy passed in **13.186 s**; **52 selected
native tests passed**, with 1,037 excluded, in **7.841 s** test time (**79.218 s**
including compilation/runner). The command used `cargo nextest run --locked
-p airk-cli --all-features --profile ci --no-fail-fast --timings` with the retained
P10 target, incremental compilation on, and TLS/maintenance/login selectors.
It does not replace the final full workspace or installed-runtime gates.

The fresh dependency policy run after adding the already locked X.509 parser
as a direct dependency passed in **14.195 s** (`dependency-tls-r4.json`). Existing
dated advisory dispositions remain visible; this is policy compliance, not a
claim that the dependency graph has no advisories.

Windows debug r5 rebuilt and sealed in **85.145 s** (Cargo compilation
**42.08 s**) using the same target and verified static dependencies. Actual
installation passed strict HTTPS, independent sessions, full MCP/JSON read
parity, no-launcher preparation and instance-fenced Stop/Start. Its subsequent
same-executable/different-payload activation timed out at the harness's outer
180-second limit while the native checkpoint remained in drain/quiescing.
The complete install gate failed after **286.375 s**. Historical adoption and
schema-1 upgrade stages did not run. The original fixture and journals remain;
an exact post-cleanup process census found no surviving fixture processes.
That failed attempt is retained as historical evidence; the typed receipt
correction and passing r7 installer run below supersede its pending diagnosis.

The full extension r6 suite passed **1,012 tests, zero failures, 21 skips**
in **24.078 s** including compilation/runner. A later bounded-reprepare caller
correction adds fresh native-origin locators while retaining accepted claims;
its focused controller suite passed **112 tests with one native-gated skip**.
The subsequent full extension r7 run passed **1,013 tests, zero failures,
21 native-gated skips**, in **23.437 s** including compilation/runner.

Checkpoint diagnostics now retain the last validated operation/instance/image
observation, with separate active-request, metadata-idle and source-admission
fields at the first barrier. Optional private diagnostic files are bounded and
do not control admission. The native deadline remains 180 seconds. The test
driver gives activation/rollback a separate outer allowance so native refusal
details survive; other calls retain their ordinary deadline and no operation is
retried. Its 17 focused Python contracts passed. R12 scoped native Clippy passed
in **11.226 s**, and **25 checkpoint library tests passed** (729 excluded,
**3.266 s** tests, **27.671 s** including compilation/runner). Diagnostic runtime
r6 built and sealed in **60.846 s**, including **18.70 s** Cargo compilation.
The matched installed reproduction failed after **288.559 s**, this time with
the native `native_checkpoint_pending` response and its complete validated
observation retained rather than an outer timeout. It reported
`accepted_native_work_pending` with exactly one pending operation. Read-only
inspection of that fixture confirmed five succeeded jobs, no plans, a complete
lifecycle request and one completed `catalog_index_setup` receipt at `ready`.
The generic checkpoint stage filter incorrectly classified that terminal receipt
as unfinished. The correction must use each receipt owner's typed terminal
contract, including retained split-finalization proofs, while preserving unknown
work and all original row digests. R15 and the passing r7 installer run below
subsequently validated that correction. The exact post-cleanup census found no surviving diagnostic-fixture
processes; original evidence remains retained.

The typed receipt correction passed scoped Clippy in r13 (**22.487 s**), but
two newly added whole-snapshot assertions were incompatible with existing Store
component fixtures: one intentionally retained a multi-megabyte pending split
reservation, and the other lacked complete physical publications. That run
finished **35 passed, two failed** (37 selected, 1,530 excluded). Those fixtures
now exercise the same production receipt classifier directly; the snapshot
size and publication checks remain unchanged. Actual full checkpoint capture
was added to the existing native split transport cases instead.

R14 Clippy passed (**26.188 s**); **38 of 39 selected tests passed**. Its new
history test incorrectly attempted 4,097 rows despite the canonical catalog's
existing 1,024-row receipt limit. The test now fills the actual 1,024-row limit,
requires rejection of row 1,025 and verifies that complete history remains
readable. Canonical capacity triggers were not changed. Split parent lookup
uses one bounded identity map per snapshot; it still authenticates each native
parent before recognizing terminal evidence.

R15 Clippy passed in **11.869 s** and **all 39 selected checkpoint tests passed**
(1,530 excluded; **3.830 s** test time, **18.493 s** including compilation/runner).
The five actual split transport cases with full checkpoint assertions all
passed as one coalesced matched-development-binary preflight: **189.047 s**
test time (**216.605 s** including compilation/runner; 37 cases excluded).
Coverage includes shared-root retirement, interrupted funding retirement,
cancellation, provider cutover and semantic-ready destination reads. Every
completed split retained complete checkpoint evidence and its digest, including
native publications. This is component/process evidence; the sealed installer
subsequently passed its separate r7 upgrade run below.
The latest full Python r6 run passed **746 tests, zero failures/errors, 11 skips**
in **69.978 s** test time (**70.568 s** including its runner).


## Windows debug r7 installed acceptance

The fresh Windows debug r7 runtime built and sealed successfully in **162.591 s
including its runner** (**162.580 s** in `windows-debug-runtime-r7.json`). It used
`python scripts/build-service-runtime.py --profile debug`, the persistent P10
target and incremental compilation on, retaining Cargo timings. The actual
`check-service-install.py --runtime .../windows-debug-r7/runtime` gate passed in
**165.079 s** (`windows-debug-install-r7.json` and `.log`). It exercised offline
staging/election, exact installed images, service-owned HTTPS with strict normal
CA validation, independent sessions and instance-fenced Stop/Start, exact-payload
upgrade and rollback, previous-runtime retirement and resumable owned uninstall.
This supersedes the earlier installed checkpoint failure; it does not qualify
an optimized build, historical migration, a VSIX or another platform.

The separate authentic pre-1.4 adoption r7 attempt failed after **21.701 s** at
`prepare-legacy-caches`: its fixture directory did not satisfy native owner-only
protection. Schema-1 upgrade r7 failed its baseline ACL precondition after
**0.162 s**; r8 then failed while the authentic old runtime staged its payload
with Windows `os error 206` (path length), after **3.964 s**. These are retained
failed validation/setup attempts under investigation, not successful adoption or
upgrade evidence. No native ownership guard or historical binary is weakened to
make these cases pass.

The final full Windows workspace Clippy gate passed in **41.347 s**. Its first
lightweight-main check failed in **69.065 s**, reporting 231 unresolved names:
a shared standard-library import had accidentally been gated on
`local-inference`. Removing that test-only conditional fixed the configuration;
CLI Clippy r2 passed in **4.154 s**, lightweight-main r2 passed in **3.951 s**,
and the default-feature workspace build passed in **54.289 s**. The full
Nextest campaign then completed **3,187 tests: 3,178 passed and nine failed**,
with **51 skipped** (**1,766.015 s** test time; **1,872.430 s** including its
runner). All four restored long campaigns passed. Doctests did not run after
that failure. This remains a failed full-workspace test run.

Authentic pre-1.4 adoption r8 failed in **31.500 s** and exposed a native defect:
checkpointing closed serving admission (`accepting=false`), then
`drain_identity` requested serving-only health and was refused before Shutdown;
restoration encountered the still-busy instance. Cleanup also observed the
retained upgrade fence. The subsequently reviewed correction uses exact health
identity for drain after admission closes; fresh installed acceptance remains
pending. This was an actual native failure, not the earlier fixture ownership
precondition.

Schema-1 r9 used the short owned path and passed old-runtime Stage, but its
registration used the wrong explicit legacy mode and was refused
(`invalid_registry_request`; **22.521 s** including its runner). R10 completed
reviewed setup and actual model/index/vector jobs, then the harness rejected
scoped status because it required a pipeline field that this status observation
does not supply (**33.005 s**). R12 subsequently passed actual old-runtime
semantic vector and hybrid reads, then failed the native engine contract
(**52.514 s**). The reviewed source correction now uses the approved payload
version for engine compatibility; no historical upgrade pass follows from
these attempts.


### 8 November: combined correction checkpoint

The complete reviewed correction includes health-identity drain after admission
closes, engine contracts derived from explicit `Approval.version`, repair of five
logical entries from one authenticated backup with complete escrow inventory,
the existing-only two-second shared model compatibility-fence wait, and fixture
initial-setup/CAS settlement, unified CLI catalog and source-ignore corrections.
It retains native ownership, receipt, schema and configuration checks; accepted
jobs and mutations are not blindly replayed.

Combined formatting passed (**6.953 s**), full-workspace Clippy passed
(**33.251 s**) and lightweight-main validation passed (**19.692 s**). All **45
selected tests passed** (**73.980 s** test time; **145.872 s** including the
runner). Full Python validation passed **755 tests, zero failures/errors,
12 skips** (**91.790 s** tests; **92.782 s** including its runner). These scoped
Rust results do not turn the preceding 3,187-test campaign into a passing run.

### Sealed r8 actual acceptance and remaining historical-format blocker

Windows debug runtime r8 built and sealed in **68.324 s** in
`windows-debug-runtime-r8.json` (Cargo **25.98 s**). The builder
used the persistent P10 target with incremental compilation on and retained
Cargo timings. Actual installed acceptance passed in **169.275 s**
(`windows-debug-install-r8.json`), exercising strict normal-CA HTTPS,
instance-fenced Stop/Start, payload upgrade/rollback, retirement and uninstall.
The separately selected ignored checkpoint owner test passed **one test** in
**18.323 s** (**50.607 s** including compilation and its runner), with **756
tests skipped** by that exact selection (`windows-checkpoint-owner-r8.log`).

Genuine **1.8.12** schema-1 indexed semantic upgrade r13 passed in **83.045 s**
in its subprocess receipt. The actual
fixture completed explicit old model/index/vector jobs and old vector/hybrid
reads, preserved its exact registration, source and reviewed scope through
upgrade, reconnected over HTTPS, removed obsolete owned images and completed
teardown without a retained fixture. Full observations are in
`windows-debug-schema1-upgrade-result-r13.json`. The command arguments and
result bind this run to **r8**, manifest
`3de8a00951667fb8bfd097991f795c0f2aba0c4238e7e0581abc789867da52fc`
and executable
`f9cdb76fab4c5d58b9033d101645bf09a592cb830e946279b97480aba03249d8`;
the runner's free-text execution label still says r7 and is not used as
artifact identity.

The genuine **1.3.1** saved-index adoption r9 remained **failed** after
**34.814 s** in its subprocess receipt, at
`adopt_saved_index_batch`: native conversion refused the historical Current
publication with `original group has no saved publication`. The source snapshot
and pending native fixture were retained after cleanup also refused; this is
an unsupported saved-publication conversion, not evidence that the original
index had no saved data. This attempt did not execute the old runtime, did not
exercise agent UI and did not reach the requested 1.0.0-build.7 case. See
`windows-debug-pre14-adoption-result-r9.json` and its complete failure log.

The latest extension gate passed **1,013 tests**, zero failures, with **21
native-gated skips** (**1,034 total**; `npm-final-r7.log`, **23.437 s** including
compilation/bundling and runner). These results preserve the preceding failed
full-workspace baseline and the separate 45-test correction pass. The final
full Windows rerun, doctests, optimized Windows VSIX and current-source
Neo/signed qualification remain pending. No accepted final release artifact or
publication is established.

### Historical Current reader correction and destination preflight failures

The historical Current reader correction preserves the original saved
publication token, configuration and resources without modifying original
indexes. Two initially failing Current unit fixtures were corrected to use
their actual lexical saved data and Balanced pipeline binding; production
validation was not weakened. The final Current correction selection passed
**28/28 tests** in **1.495 s** (**19.244 s** including compilation/runner;
`windows-current-adoption-tests-r3.json` and `.log`). A subsequent scoped
Store/Kernel/CLI all-feature Clippy pass completed in **12.046 s**
(`windows-current-adoption-clippy-r5.json`). Canonical CLI reference/help checks
passed **three tests**, **40.252 s** including compilation/runner
(`windows-cli-reference-r1.json` and `.log`).

Windows debug r9 built and sealed in **139.406 s** (Cargo **38.64 s**;
`windows-debug-runtime-r9.json` and `.log`). Authentic 1.3.1 adoption r10 then
failed in **39.448 s**, at `adopt_saved_index_batch`, with `attempt to write a
readonly database`. The destination registry needed a narrow existing-writable
closure while native maintenance was held. Its independently reviewed
correction opens no new registry and does not add SQLite CREATE authority.
The expanded correction selection passed **40/40 tests** in **2.040 s**
(**71.390 s** including compilation/runner;
`windows-current-adoption-tests-r4.json` and `.log`); scoped Store/Kernel/CLI
all-feature Clippy passed in **26.935 s**
(`windows-current-adoption-clippy-r6.json`). These commands reused the P10
target with incremental compilation on and retained Cargo timings.

R10's secondary fixture-cleanup failure was traced to its stale stored service
instance after legitimate maintenance restoration restarted the owner, rather
than a RequestEpoch defect. The exact selected/original/withheld-image census
found **zero matching live processes**
(`preserved-adoption-r10-readonly-census.json`); this is scoped process evidence,
not proof of completed adoption or removal of pending fixture state.

Windows debug r10 built and sealed in **68.280 s** (Cargo **25.26 s**;
`windows-debug-runtime-r10.json` and `.log`). Authentic 1.3.1 adoption r11 still
**failed** in **39.806 s**: `source roots must be disjoint from service control
and registered project data`. The coordinator had placed converted data under
protected service state. The sibling-data destination, private parent and
all-batch source-preflight correction is **in progress and not yet qualified**.
The isolation guard remains enforced; the failure is not relabeled as a
successful conversion. See the complete r10/r11 failed result JSON and logs.

Times in this section are taken from subprocess receipts, rather than outer
runner observations. The complete Windows workspace rerun, doctests,
current-source Neo/signed acceptance and final optimized Windows VSIX remain
pending. No new native acceptance pass or publication is claimed.

### Destination correction, native conversion proof and remaining parity failure

The sibling-data/private-parent and all-batch source-preflight correction
subsequently passed **50/50 selected tests** in **2.179 s** (**73.135 s**
including compilation/runner; `windows-current-adoption-tests-r5`). The sealed
Windows debug r11 runtime built in **67.894 s**, with Cargo **26.08 s**
(`windows-debug-runtime-r11.json` and `.log`), using the same persistent P10
target, incremental compilation on and retained Cargo timings.

Authentic 1.3.1 adoption campaigns r12 and r13 reached **ready native batch
conversion**: one physical group, two original configuration claims, and full
saved Current publication, configuration and resource provenance were retained.
This is a positive conversion observation, not a passing campaign. R12 failed
in **53.931 s** and r13 in **55.508 s**, both at `compare_canonical_reads` with
`Complete CLI/API parity differs: read_file`. Review traced the differences to
validated observation clocks rather than changed saved content or authority;
the harness comparison was corrected to validate those clocks before excluding
their transport-time values. Complete results remain in
`windows-debug-pre14-adoption-result-r12.json` and `-r13.json`; the failed
campaigns are not relabeled as passes.

R14 failed in **59.021 s** when `query_scope` was sent an unsupported
`response_mode`, returning canonical `INVALID_SCHEMA`. Investigation also found
a real CLI convenience shortcut that synthesized scope discovery instead of
going through native authority validation. That shortcut was removed; the CLI
now uses the canonical native path rather than relying on a reduced parallel
answer. The subsequent scope-r3 selection completed **9/9 tests**, including
actual installed query-scope calls/shell authority retention, in **19.711 s**
(**27.950 s** including compilation/runner; `windows-query-scope-tests-r3`), with
**2,692 skipped** by the exact selection. Scoped Clippy passed in **2.007 s**
(`windows-query-scope-clippy-r3.json`).

These are subprocess-receipt timings. No complete pre-1.4 campaign or
1.0.0-build.7 qualification has passed. The final Windows workspace rerun,
doctests, current-source Neo/signed acceptance and final optimized VSIX remain
pending; neither native conversion readiness nor these selected tests establish
release completion.

### First complete 1.3.1 case and Windows directory-length blocker

Full Python validation r8 passed **756 tests: 744 passed, 12 skipped**, with no
failures/errors, in **76.956 s** (**77.612 s** including its runner;
`python-full-final-r8.json` and `.log`). This complements the nine passing
query-scope tests above and does not replace full Rust workspace qualification.
Windows debug r12 built and sealed in **50.205 s** in its build receipt, with
Cargo **7.23 s**, retained timings, P10 target reuse and incremental compilation
on (`windows-debug-runtime-r12.json` and `.log`).

Authentic adoption campaign r15 records the first **complete 1.3.1 case pass**:
ready native saved-index conversion with full Current provenance, independent
Codex and Claude HTTPS credentials, two configuration migrations, complete
canonical-read parity, refusal of cleanup before migration, exact cache
retirement after migration and unchanged original fixture bytes. This positive
case is retained in `windows-debug-pre14-adoption-result-r15.json`; agent UI was
not exercised.

The process evidence is deliberately scoped. Its raw observations scanned
**434 processes**, left **174 unobserved**, and reported `complete=false`.
No matching owned PIDs were observed; the additional exact-image exclusion
proof supported the `verified_legacy_cache_images` receipt with `remaining=0`.
This does not establish a complete global process census or the absence of
unrelated AIRK installations.

The **overall r15 campaign failed** in **79.492 s** in its subprocess receipt.
The subsequent **1.0.0-build.7** case failed during modern runtime Stage,
before activation, with Windows `os error 206`. Investigation identified the
raw Win32 `CreateDirectoryW` directory limit of **248 characters**, reached by
notice directories of **249/251 characters**. A shared extended-path correction
has been implemented at the three raw path call sites in
`local_ipc_windows.rs`, preserving ownership/security checks. Its scoped
validation is **running; no qualifying result is yet recorded here**. The
original failure and full successful 1.3.1 case remain separately identifiable.

The complete Windows workspace rerun, doctests, current-source Neo/signed
acceptance, earliest-tag adoption and final optimized VSIX remain pending.

### Earliest-format journal and exact Prepared checkpoint correction

Sealed Windows debug r16 built in **78.724 s** (Cargo **36.18 s**;
`windows-debug-runtime-r16.json` and `.log`). Adoption campaign r19 again
passed its complete 1.3.1 case, but the **overall campaign failed** in
**114.007 s**. The 1.0.0-build.7 case reached `adopt_saved_index_batch` and
refused the old semantic extraction format while preparing its Current journal.
Maintenance restoration also refused the changed continuation snapshot. The
retained delta shows zero jobs/requests and one native setup receipt at
`prepared`, not accepted job replay (`preserved-adoption-r19-continuation-delta.json`).
This later failure is distinct from the earlier Stage path-length failure.

The source correction now supplies a legacy saved-payload reader for lexical
journal construction, commit and replay without pretending the historical
payload satisfies the modern semantic projection contract. Ordinary semantic
reader refusals remain enforced. Maintenance checkpoint validation also
recognizes only the exact planned native adoption reservation/Prepared
transition with its bound storage proof and capsule. Ordinary preparations,
crossed storage, changed capsules, unknown slot files and missing Current files
after Ready retain their refusal paths; arbitrary continuation changes do not
become acceptable.

The new `check-standalone-adoption-failure.py` campaign source deliberately
corrupts an authenticated fixture **copy**, expects an integrity failure after
native preparation, never executes the historical helper and retains the
damaged copy/pending native journal for diagnosis. Its implementation and
Python coverage do not establish a passing actual negative campaign. Neither
the corrected positive earliest-tag campaign nor this negative campaign has
passed actual runtime qualification.

Correction runner r15's selected test run r11 **failed**: **91 run,
90 passed, one failed, 2,469 skipped**, **4.606 s** tests and **23.461 s**
including compilation/runner (`windows-current-adoption-tests-r11`). The new
`legacy_adoption_saved_chunks_commit_replay_and_search_without_semantic_projection`
test observed zero lexical hits where one was expected. That result remains
under investigation; the assertion is not waived and the run is not counted
as a pass.

Full Python validation r9 passed **762 tests: 750 passed, 12 skipped**, with no
failures/errors, **88.033 s** tests and **88.720 s** including its runner
(`python-full-final-r9.json` and `.log`). All times use retained subprocess
receipts. The full Windows workspace rerun, doctests, actual corrected
earliest-tag and corrupt-copy campaigns, current-source Neo/signed acceptance
and final optimized VSIX remain pending. No release completion is established.

The subsequent correction runner r16 passed formatting (**7.038 s**) and
scoped Clippy (**12.659 s**). Test selection r12 passed **91/91**, with **2,469
selected out**, **4.824 s** test time and **22.056 s** including
compilation/runner (`windows-current-adoption-tests-r12.json` and `.log`).
The preceding failure was the test's map-only preparation helper omitting FTS;
the corrected test calls actual `prepare_artifacts_with_fts` and exercises
durable commit, replay and lexical search. The r11 failed result remains
historical evidence, not a passing run. Windows debug r17 is building; corrected
positive and corrupt-copy actual campaigns remain pending.

### Sealed r17 positive and negative actual migration acceptance

Windows debug r17 built and sealed in **69.062 s** in its subprocess receipt
(Cargo **27.18 s**), retaining P10 timings with incremental compilation on
(`windows-debug-runtime-r17.json` and `.log`). Both following campaigns bind
the exact runtime manifest
`98972d76393d86e09cbc3c8ae4a8bca33bd190635aa73692d3650f98e481a7db`.

Positive adoption campaign r20 **passed both authentic 1.3.1 and
1.0.0-build.7 cases** in **140.499 s** in its subprocess receipt. Each completed
native saved-index conversion with retained Current provenance, two distinct
Codex/Claude HTTPS credentials, both guarded configuration migrations, full
canonical-read parity, refusal of premature cleanup, scoped old-cache
retirement with `remaining=0`, and unchanged original fixture bytes. Full proof
is retained in `windows-debug-pre14-adoption-result-r20.json`. This supersedes
the earlier blockers for these two tested Windows formats; it does not turn
the earlier failed attempts into passing runs or qualify all historical
versions. Agent UI was not exercised. Retirement proves exact owned images,
not a complete global process census.

The actual negative campaign **passed** in **37.682 s**
(`windows-debug-adoption-failure-r1.json` and its full result). A deliberately
corrupted authenticated fixture copy failed native adoption after preparation,
as expected; real maintenance restoration completed, original fixture bytes
remained unchanged, the diagnostic fixture and pending cache journal remained
preserved, and canonical final Stop reached `complete`. The result explicitly
records `adoption_succeeded=false`: this is successful failure/restoration
qualification, not successful adoption of damaged input. No historical helper
or agent UI was executed by that campaign.

These subprocess-receipt timings and full results establish Windows debug
positive/negative migration evidence for the tested formats. The final full
Windows workspace rerun, doctests, current-source Neo/signed qualification,
actual installed-editor hot-update acceptance and final optimized Windows
VSIX/release workflows remain pending. The implementation remains unpublished.

### Neo r6 fixture failure and pre-r7 correction checkpoint

The retained Neo r6 batch **failed before Rust compilation**. Full Python ran
**762 tests: two failures, 14 errors, 19 skips**, **12.826 s** tests and
**13.442 s** for its owned stage; the batch took **26.842 s**. Offline-model
preparation, locked dependency fetch, npm installation and extension compilation
had passed, but no Rust target, native/model acceptance, signing or installed UI
gate was executed. The complete receipt/logs are retained under
`artifacts/local/one-airk-1814/neo-batch-r6-complete-1794134044342333100/`.

Investigation identified **test-fixture portability defects**, not a production
TLS change: LibreSSL-generated CA/leaf fixtures lacked explicit SKI/AKI under
Python 3.13 strict verification; the Node extra-CA case preserved the inherited
root but encountered same-subject issuer ambiguity; bootstrap's broad process
mock intercepted a platform probe with incompatible binary/text behavior.
Node diagnostics showed the extra root present, certificate-signature failure
and **zero bearer requests**. The three corrected test files are
`test_http_fixture.py`, `test_local_https_examples.py` and `test_bootstrap.py`.
They now give fixtures explicit key identities and scope the platform mock;
normal CA verification, inherited trust and production native TLS remain
unchanged. The correction received independent review.

Strict TLS/Node checks and the normal-user Windows full Python r10 run passed
**764 tests: 752 passed, 12 skipped**, zero failures/errors, **71.567 s** tests
and **72.339 s** in its subprocess receipt (`python-full-final-r10.json` and
`.log`). This is Windows evidence for the corrected tests; **Neo r7 remains
pending**. The current Windows 3,221-test full workspace campaign is still in
progress. Workspace Clippy r3 passed in **26.666 s** and lightweight-main r3 in
**35.036 s**, but neither substitutes for that complete campaign, doctests,
Neo/signed acceptance, installed-editor qualification or a final optimized VSIX.

### Complete Windows campaign and Neo r7 test-helper correction

Windows final runner r3 completed successfully. Full-workspace all-feature
Clippy passed in **26.666 s**, lightweight all-target/no-default-feature
validation in **35.036 s**, and default-feature workspace build in **40.058 s**.
The full Nextest campaign, including all four restored routine long campaigns,
passed **3,221/3,221 tests**, **52 skipped**, **three slow**, in **1,772.412 s**
test time and **1,851.053 s** including compilation/runner
(`windows-final-workspace-tests-r2.json` and `.log`; the runner reuses this
receipt suffix). Workspace all-feature doctest commands passed in **21.555 s**
(`windows-final-doctests-r2`); all eight crates reported **zero executable
doctest examples**. A successful command is not fabricated example coverage.

That complete Windows run and sealed r17 native build retain their original
compiled-source identity
`ab43f7c8890c9cbdd5ef5f3aead2af7020cdde98928adf7826a7781d431be839`
(**1,114 files**). It is not relabeled as the subsequent source or fresh Mac
artifact identity. Persistent P10 targets, incremental compilation on and
Cargo timing evidence were retained.

Neo r7 passed corrected Python validation, **764 tests: 745 passed, 19 skipped**,
and formatting, then **failed Clippy** after **91.441 s** on one `dead_code`
diagnostic for `macho_identity::payload_digest`. Required cleanup completed,
removing **3,276 target files / 807.8 MiB** after retaining evidence. This remains
a failed Neo batch, not full native/platform acceptance.
Its workspace/all-feature/all-target Clippy used **two jobs**, incremental
compilation **off** and the campaign's `cargo-target`; the HTML timing report
was retained before cleanup. Debug runtime, checkpoint, installer, model,
signing and UI gates were not reached.

The independently reviewed correction changes only the private module gate in
`airk-retrieval/src/lib.rs` from the former test-or-Mac-inference condition to
`#[cfg(test)]`. There are no production callers; Windows compilation behavior
is identical, while the unused production Mac helper is no longer compiled.
Post-correction formatting passed in **6.716 s**, scoped retrieval Clippy in
**25.328 s**, and the exact Mach-O identity test passed **1/1** (**46 selected
out**, **0.024 s** test time, **31.655 s** including compilation/runner;
`macho-test-gate-*-r1`). This targeted result is separate from the preceding
complete Windows source identity.

Neo r8 validation and its new native source/artifact hash remain pending. The
full Windows results do not establish Mac/signed lifecycle acceptance,
installed-editor hot-update qualification or an accepted optimized VSIX/release.

### Optimized Windows r1, Neo r8 failure and pre-r9 review corrections

The first optimized Windows runtime built successfully in **794.269 s** in
its subprocess receipt (`windows-release-runtime-r1.json`), using the persistent
P10 target with incremental compilation **off** and retained Cargo timings.
Its native source identity is
`6723387a63f6daff54e74d40a78992b59ff7648e22467686904d5d016fe36b06`,
covers **1,114 files**, and is the
same source identity exported for Neo r8. This is separate from the earlier
`ab43...` full Windows campaign and is not relabeled as a new full run.

Neo r8 passed Python and formatting, then **failed Clippy** in **122.393 s**
with **eight Unix-only compile/lint errors** involving `Duration`, unused
legacy-producer paths and a test containment method. Cleanup completed in
**0.346 s**, removing **4,020 files / 975.8 MiB** after retaining evidence.
These failures prevented later native/installed/signed/editor qualification.

Two reviewed native source files were corrected. The first Windows scoped
Clippy attempt then failed because the containment method was unused in the
modern Windows test configuration. Its final gate is
`any(not(feature = "local-inference"), all(test, unix))`, preserving the actual
legacy and Unix test callers without silencing dead code globally. Final scoped
CLI Clippy passed in **19.418 s**; the inference/pool/Unix-guardian selection
passed **56/56 tests**, **1,046 selected out**, **8.764 s** tests and **68.432 s**
including compilation/runner (`unix-inference-gate-*-r2`). These Windows scoped
checks do not turn Neo r8 into a passing batch. Native source is frozen and the
fresh optimized Windows r2 build is running; its artifact identity is pending.

Final extension review corrected actual VS Code alias/user-profile credential
lookup and disabled duplicate/manual configuration handling. The final reviewed
four-file freeze passed compilation and **41/41 scoped tests** in **2.048 s**
(`vscode-credential-alias-global-freeze.json` binds the source files). The older
45-test credential log predates this correction and is not its validation
identity. No actual editor qualification is inferred from these host tests.
The agent-install harness now isolates client homes and disables shell changes;
its **30 Node contract tests passed**. Native-editor harness readiness checks
retain modern registry formats **12/13** and legacy **12** guards; **11 Python
harness tests passed**, rather than eleven actual native/editor campaigns.

Full normal-user Windows Python r11 passed **764 tests: 752 passed, 12 skipped**,
zero failures/errors, **75.096 s** tests and **75.761 s** including its runner
(`python-full-final-r11.json` and `.log`). Neo r9, fresh optimized r2 identity,
current-source installed-editor/signed acceptance, post-review extension gates
and final VSIX/release workflows remain pending. Prior full Windows results and
all failed Mac attempts retain their original source/artifact identities.

### Final r9 source inspection: verified predecessor metadata caller

Final review found a future modern-to-modern cleanup defect: invoking the new
bundle for an older selected owner's metadata trips the native compiled
version/image guard. The narrow `migrationClient` correction keeps the new
bundle's complete `runtime inspect` verification, then uses the exact verified
predecessor executable for self-test, its three metadata RPCs and
`http-endpoint`. Selected payload, manifest version, control path, health,
namespace, service instance and executable-image checks remain enforced.
The ordinary client retains its strict running-version gate; this metadata-only
facade exposes no source, job or grant mutation and adds no unscoped fallback.

The independently reviewed correction passed TypeScript compilation
(**7.717 s**) and **44 service-bootstrap tests** (**1.176 s**). Frozen source
identities are `serviceBootstrap.ts`
`01392dfa063fa9bc57b6956ee34cc4641d5ff002b18d36680b52fb6a2f13f901`
and its test
`8fcb2f9c48bd3fbaa38d79d6361e28ffbf419c6f959ffe461b7ff8aad0607abd`.
A stale Explorer Evidence comment was also updated for HTTPS/default queue
behavior; that comment changes no execution. No Python inputs changed after
the passing r11 suite. Native source remains frozen while optimized Windows r2
builds; Neo r9 and current-source editor/signed/final VSIX qualification remain
pending. These source checks are not a newly completed platform campaign.

### Neo r9 private model root failure and matched debug r18 preflight

Neo r9 passed full-workspace Clippy (**150.781 s**) and lightweight validation
(**39.228 s**), then successfully linked the static debug executable. Its model
smoke **failed** the owner-only root check because the temporary parent was
created mode **0755**. No runtime was published. The complete **23-file**
evidence set and Cargo timings were retained; required cleanup removed **7,001
files / 3.3 GiB** in **0.645 s**. Successful linking does not qualify that
failed smoke or later Mac installation/signing/UI gates.

The reviewed bundled-assets correction creates the Unix temporary root at
mode **0700**, then uses a canonical private child and its lease. The Unix
regression changes umask only in owned child processes (**022/002/000**),
verifying that the parent's umask stays unchanged. Windows scoped Clippy r1
passed in **10.653 s**; **11 bundled-model tests passed**, **26.978 s** tests
and **45.201 s** including compilation/runner. Final formatting passed in
**6.989 s** and scoped Clippy r2 in **10.735 s**
(`model-smoke-private-root-*-r1/r2`). These Windows results do not replace the
pending actual Unix regression and Mac smoke rerun.

Fresh Windows debug r18 built and sealed in **66.994 s** (Cargo **25.62 s**),
with native source identity
`b41b3611f47182d9d1b5e27c561b4704f6c5c26fbb4f28cfa91e3ad26406a13d`
(**1,114 files**) and runtime manifest
`3cd96b4f23bac3af274e435230d71730224154018d013b234541a209ec0c4005`.
Its actual embedded smoke returned **384 dimensions** with equal repeat
results. The **matched current-source model/client preflight passed** in
**65.468 s** (`windows-debug-model-clients-r1.json` and its full result), with
model checking enabled, complete scoped read parity, preserved fixture source
bytes and completed managed Stop. This is current Windows debug model evidence,
not a passing Mac gate or complete installer acceptance.

Optimized Windows r2 remains an **older pre-private-root-fix artifact**
(native source prefix `bbba`; manifest
`122eee02b218c73ce07ff810167844c94c2e38049257d08224c862f8ab6ae442`).
Its model/client r1 attempt failed in **31.166 s** on an incorrect harness
expectation that the frozen single-index grant was a discovery grant; r2
failed in **169.983 s** waiting for a scoped freshness-idle observation that
the connection cannot provide. Reviewed harness corrections retain exact scope,
privacy and full result parity. The subsequent r3 actual model/client check on
that optimized artifact **passed in 52.597 s**. These older-artifact observations
retain their original identities and are not presented as current-source
optimized qualification.

Both failed model fixtures received canonical Stop, health and exact-image
process audits with zero observed matching live processes. Their **177/192
inaccessible-process** limits remain explicit: this is not a complete global
census or blanket proof about unrelated processes. Full Python r12 passed
**767 tests: 755 passed, 12 skipped**, zero failures/errors, **74.977 s** tests
and **75.582 s** including its runner. Windows timings use exact subprocess
receipts. Fresh Neo r10/model preflight is required before another optimized
build; other current-source native, post-review extension, installed-editor,
signed and final VSIX/release gates remain pending.

### Debug r18 installed preflight batch: mixed results, corrections underway

The subsequent actual installer gate **passed in 167.102 s**
(`windows-debug-final-install-r1.json`) on the unchanged current debug r18
`b41...` source/manifest recorded above. This does not qualify the rest of the
batch. The enabled-native extension suite **failed**: **1,041 total, 1,020
passed, 18 failed, three skipped**, **153.760 s** test time and **153.841 s**
in its subprocess receipt (`windows-debug-final-extension-native-r1`). Native
requests rejected owner-private input ACL mismatches. Investigation includes
**production callers as well as test fixtures**; the correction is underway
and must preserve the native privacy checks. The earlier passing extension
suite, with native cases skipped, is not relabeled as this enabled-native run.

Managed read-only acceptance **failed in 15.492 s** on
`managed_catalog_detach_requires_supported_transition`. A supported modern
catalog-detach transition is missing and a native correction is underway.
Issuer rotation acceptance **failed in 99.455 s** on a stale **15-second**
header transport after a **60-second** overlap wait. Native finalization and
canonical Stop were separately proven, but they do not turn the failed
end-to-end transport observation into a passing issuer campaign.

Port relocation r1 **failed in 21.062 s** because its fixture data parent was
missing. Its corrected harness passed **five unit tests**, then actual r2
**passed in 33.870 s** (`windows-debug-final-port-relocation-r2.json` and full
result): preferred-port and saved-port collisions, stale TLS endpoint refusal
and exact scoped-authority refresh were exercised. The result binds the same
unchanged debug r18 image and explicitly records `production_native_changed=false`.

Retrieval r2 **failed in 3.783 s**, with no completed workloads, on a reserved
fixture-base/state layout collision (`windows-debug-final-retrieval-result-r2.json`).
Retrieval and sibling harness corrections remain underway. Windows timings here
use exact subprocess receipts. Native r18 remains unchanged until the pending
managed-transition correction; current-source native/extension/editor, Neo r10,
signed and final VSIX/release acceptance remain pending. This is working
evidence, not a final source freeze or completion claim.

### Managed detach and issuer follow-ups; enabled-native authority blocker

Full Python r13 passed **776 tests: 764 passed, 12 skipped**, zero
failures/errors, **77.372 s** tests and **77.965 s** including its runner.
The managed-detach correction's first Clippy attempt failed on three new test
`unwrap_err` calls requiring `ServiceHealth: Debug`; that test-only usage was
corrected rather than changing the production health contract. Scoped Clippy
r2 then passed in **10.565 s**. The initial test filter selected **zero tests**
and exited with failure: it is not coverage. The corrected `test(managed)` r3
selection passed **31/31**, **1,073 selected out**, **1.674 s** tests and
**9.639 s** including compilation/runner (`managed-detach-tests-r3`).

Debug r19 built and sealed in **61.781 s**, native source
`a5990432f53f0307d574a6912654d0a89b5441d1f44b88fad3f529791cb2b247`
(**1,114 files**), manifest
`fc91e578d53ba0448b9845ea996566e4c0df306974881d723a2a9dcf6783d2bc`.
Actual managed read-only r2 **passed in 28.489 s**: user-on-demand detach and
external-owner preservation were exercised without writing the provider
payload. Detach applies only to a settled exact managed origin, preserves its
immutable reservation/state/provider and leaves the external owner untouched;
reattachment requires explicit configuration of the same approved identity.
This is supported detach, not permission to execute an arbitrary replacement.

Actual issuer rotation r2 on the original debug r18 **passed in 105.111 s**
(`windows-debug-final-issuer-rotation-r2.json`). This supersedes that harness's
earlier stale-stream failure for the tested native/TLS path, while preserving
the failed r1 record. OS trust installation and external hosts remain
unqualified by this private-fixture campaign.

Owner-private native-input corrections now cover production extension callers
and fixtures; TypeScript compilation and **61 scoped tests passed**, **one
skipped** (**62 total**, **0.978 s** tests; `native-input-scoped-final.log`).
The actual debug-r18 universal slice ran **three tests: two passed, one
failed**. The original ACL refusals passed, exposing the next native defect:
committed authority revalidation across catalog **schema 12 to 13** refused
with `authority-1 committed activation identity changed`. A native correction
is underway. This is neither a passing enabled-native full suite nor evidence
that the committed authority check should be bypassed.

Windows times above use exact subprocess receipts. Current native, extension,
retrieval/editor, fresh Neo/signed and final optimized VSIX/release gates remain
in progress. No final source freeze or release completion is claimed.

### Debug r20 authority correction and completed sibling preflights

The schema-activation correction passed **47/47 selected tests**, **1,990
selected out**, **5.636 s** tests and **65.909 s** including compilation/runner;
scoped CLI/Store Clippy passed in **24.372 s** (`activation-promotion-*-r1`).
Debug r20 built in **61.365 s**, native source
`34f68b3bfa6ad0d1abfafd5d5a9130c02e3bfdf376ad3f924c0ef599eb69c74f`
(**1,114 files**), manifest
`75a3349b95cbdb81ab47edfa1d4f02aa60ea1575d50ae1a63143f6ef3c94ed78`.
Actual enrollment then passed **three cases** in **16.994 s**.

The complete enabled-native extension suite on this correction still
**failed**: **1,044 total, 1,039 passed, two failed, three skipped**,
**291.016 s** tests/**291.085 s** in its subprocess receipt. Reviewed harness
corrections subsequently passed actual focused **Assistance 1/1**
(**41.638 s** tests) and **provider-stop 2/2** (**42.582 s** tests/**42.668 s**
including runner). These correctly observe asynchronous completion after a
single Stop using read-only observations; they introduce no production change
or mutation replay. The focused passes do not relabel the failed full suite.

Three publication-fault attempts passed in **64.895 s** using an explicitly
owned foreground fixture, **not** on-demand service qualification. Additional
completed native preflights passed: code benchmark on **r19**, **97.741 s**;
transformer on **r20**, **65.935 s**; document tables, **57.406 s**; and usage,
**74.230 s**. Each retains its own recorded artifact and supported scope.

Storage lifecycle r1 failed in **62.582 s** on the post-quota scoped-read
refusal; r2 remains running after observing the accepted job's durable quota
wait. Retrieval r3 still **failed**, **39.989 s**, with no completed workloads:
its configure/add-root path used obsolete legacy configuration. The reviewed
source-helper correction now uses `nativeSourceReview` for modern sources rather
than legacy add-root handling, preserving the original alias and explicitly
granted new root. Failed-context retention is corrected; the 20-test and
six-test unit selections passed. Actual retrieval rerun remains pending. Source
binding and failed read-contract frames remain retained; no retrieval
acceptance is inferred from the passing code benchmark.

Full Python r14 passed **785 tests: 773 passed, 12 skipped**, zero
failures/errors, **82.342 s** tests/**82.964 s** including runner. The current
r20 full Windows **3,224-test** workspace campaign remains in progress. The
Python result predates the latest retrieval-helper correction. Windows
formatting (**7.161 s**), full-workspace Clippy (**18.487 s**), lightweight
validation (**19.309 s**) and default build (**24.953 s**) passed. All Windows
times here use subprocess receipts. The earlier complete Windows pass retains
its earlier source identity; there is no new full-suite pass yet. Fresh Neo r10,
current-source extension/editor, signed and final VSIX/release acceptance
remain pending. This working checkpoint establishes no completed release.

### Late storage retirement failure and exact registry-read retry gap

Storage lifecycle r2 **failed in 155.672 s** in its subprocess receipt, late
in the campaign. Durable quota wait and resumption of the **same accepted job**
had passed; failure followed same-root replacement, when the original canonical
retirement evidence could no longer be read. Investigation found schema-13
purging deleted the immutable old backing, allowing replacement to erase the
evidence needed to inspect its retirement. The independently reviewed three-file
native correction retains that backing for catalog schemas **12 and 13** and
recognizes both in the relocation path's early read-only check. New regression
tests are authored, but Cargo validation is **pending** while the current r20
full workspace campaign finishes. This implemented fix is not yet qualified.

Retrieval r4 **failed in 38.969 s**, **zero completed workloads**
(`windows-debug-final-retrieval-result-r4.json`). Captured outgoing request and
reply prove the same exact `registry_busy` condition, not an inferred generic
busy message. The harness `REGISTRY_READS` set omitted the read-only
`sourceMutationDraft` method, although production `ServiceClient` already
permits its two bounded registry-read retries. A narrow harness correction and
regressions are underway: preserve the exact request key/arguments and original
deadline, retry only this typed registry-read admission failure, and retain the
complete final response. No generic refusal retry, mutation replay or retrieval
production-native change is introduced.

Actual retrieval r5, storage-fix validation and fresh Neo r10 remain pending.
The original r20 native/full-suite identity and all failed receipts remain
separate from the pending corrected source. No current full-suite, release or
all-platform completion is claimed.


### 8 November: commit checkpoint and remaining storage blocker

Saved receipts supersede the preceding in-progress descriptions: Windows r3
workspace Nextest completed **3,224 passed, 52 skipped** in **1,871.920 s**
including its runner; workspace all-features doctests passed in **43.024 s**.
These retain the r20 source identity and do not certify later native edits.
Retrieval r6 on debug r20 passed **two workloads, nine cases** in **454.855 s**.
The schema-13 retirement correction passed **14 focused tests** in **47.939 s**
including compilation/runner, plus its scoped Clippy and lightweight checks.

Debug r21 built successfully. Its receipt correction records the actual Cargo
target as `target/`, rather than the declared persistent P10 target; no rebuild
or cleanup is implied. Storage lifecycle r3 on r21 **failed in 178.194 s**:
quota pressure and resumption of the same accepted job passed, then the full
removal observation timed out. Its owned fixture is preserved. The next work is
to distinguish collection deletion, inactive registration cleanup and worker
exit before changing native behavior or the acceptance test. Current-source
complete extension, Mac, signed installation and final VSIX qualification remain
outstanding. This is an unpublished implementation checkpoint, not release
acceptance.

### r21 removal diagnosis and acceptance correction

The preserved storage-r3 fixture was inspected read-only under its existing
owner protection. Its schema-13 registry retained the original registration
with `registered=0`, `cleanup_guard=NULL` and its exact
`service_removed_projects` marker; its collection directory was absent. The
fixture's final instance-fenced Stop receipt was complete. That final Stop means
current process absence alone cannot prove the worker's state at the timeout.

The timeout came from an obsolete acceptance requirement: `inactive_console`
includes retained retirement identities, whereas the storage gate required it
to become empty. The scoped harness correction preserves the exact binding and
no-continuation checks, collection deletion and actual native worker absence.
For a retained modern identity it additionally reads only the owner-contained
fixture registry to prove the final cleanup transaction committed; legacy
removal still requires no inactive row. Native behavior and read DTOs are
unchanged. Eight focused authority regressions passed, and a separate agent
review found no blocking issues. The actual storage-r4 rerun uses the original
r21 debug executable, not a fresh build: all **811** sealed payload files and
all **1,114** production source inputs were revalidated. Compiled source remains
`27981a7de44b31570f83d58a67255db2ece5eb7cecab27bcb16b7c4c0b79ea90`;
the separate corrected test identities are in
`windows-debug-storage-r4-reuse.json`.

The complete actual storage-r4 gate **passed in 321.351 s** in its subprocess
receipt (**321.364 s** including its runner). This includes quota wait/resumption,
maintenance, source preservation, full physical removal, exact permanent
retirement evidence, denied stale authority and fresh reviewed replacement
registration/indexing. Receipt and output are
`windows-debug-storage-lifecycle-r4.{json,log}`. The successful fixture cleaned
up normally; the failed r3 fixture remains preserved. No new Cargo run was
needed for this test-only correction. Repository and whitespace checks passed.
The checkpoint commit is `7037cae`, with the configured human DCO trailer; it
was not pushed. The storage correction and this subsequent evidence update are
separate working-tree changes.

## Initial observations

- Read-only Windows process observation found 20 launchers, 11 `airk.exe`
  processes, one service, one index worker, one source worker and one embedding
  helper. Process names alone do not prove which clients or scopes own them.
- Neo is reachable at the updated `.61` address. ARM64 tools and signing identity
  are present; static ORT, CMake and Ninja provisioning remain necessary. This is
  readiness evidence, not final testing of the new implementation.
- Current verified ORT packaging supplies dynamic libraries/import libraries,
  not a static inference engine. A pinned 1.22.1 CPU source build is being prepared;
  automatic download of the dependency's different 1.22.0 distribution is excluded.
- Existing native read execution lacks a propagated original deadline and consumes
  one-shot handlers. The 50 ms retry requires a canonical read-attempt context;
  an outer replay or snapshot-only workaround would not satisfy the plan.
