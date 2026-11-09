# One AIRK 1.8.14 implementation ledger

**Current manual-test candidate (8 November):** Windows debug **r28** is built
and package-verified, VSIX SHA-256 `2312dd70…`, native manifest `5882675c…`,
compiled source `c173380b…`. See the [r28 record below](#corrected-r28-manual-test-candidate)
for exact artifacts, checks and remaining qualification. Earlier sections retain
dated evidence; their old prerequisite/status statements are not current status.

## Fixture compatibility correction and recovery policy — 8 November 2026

The user authorized a complete fixture-contract correction while preserving the
distinct installed imported-legacy and reviewed 1.8.12 catalog cases. AIRK MCP
tools were absent and workspace config disabled the server; local source and
retained receipts were used without configuration changes. The tree contained
substantial existing work, which was preserved. No commit/push/release action was
performed.

First reproduced the actual registration decision cheaply: one selected test
failed in **0.002 s**, schema-1 catalog authority selecting fresh raw registration.
Retained red output: `fixture-compatibility-red.txt` under
`artifacts/local/one-airk-1.8.14/`. The
[compatibility matrix](../testing/fixture-compatibility.md) records exact artifact
pins, source-correspondence limits, the full chain and review evidence.

The shared owner now separates topology/transport, catalog authority, native
setup/input ownership, managed/foreground lifecycle, original scope and migration
eligibility/provenance. Reviewed catalog setup is reused for schema-1 catalog
and schema-2 catalog runtimes; unknown historical contracts refuse before
startup. Paired catalog startup uses managed activation/ensure while retaining
stdio. Unsupported historical fault/source campaigns refuse before activation.
The producer retains the exact original installation-launcher wrapper and
extension-owned argument order; consumer qualification compares the complete
original registration to the native upgrade roster. Real classification,
startup, setup, argument generation, producer/consumer and workload admission
regressions mock process I/O rather than these decisions.

Primary review traced registration, Index, pause, original configuration/read,
upgrade, retirement, editor enrollment/retrieval and Stop with shared callers.
Independent `predecessor_audit` review (no authored correction files) identified
managed-startup and unsupported-sibling defects; both gained cheap regressions
and the final combined review cleared them. `chain_review` authored journey and
workload tests, so is not counted as the independent final reviewer.

Validation, with receipt/log names relative to the artifact directory above:

- `fixture-compatibility-scoped-t0.json`: **156 selected**, **155 passed / 1
  existing skip**, **8.818 s** tests / **9.085 s** wrapper.
- `node --test scripts/tests/*.test.cjs`: **107 passed**, **4.940 s**.
- `fixture-compatibility-python-t1.json`: full Python **865 selected**,
  **5 failure events / 3 error events / 13 skips**, **77.583 s** tests /
  **78.215 s** wrapper. The retained run failed under the sandbox identity.
  An isolated diagnostic found Windows TLS SSPI `No credentials are available
  in the security package` before HTTP dispatch; Git's source-tree test hit
  the sandbox/contributor ownership mismatch.
- `fixture-compatibility-environment-t0.json`: both affected modules rerun
  unchanged under contributor identity and process-scoped `safe.directory`:
  **22/22 passed**, **33.355 s** tests / **33.597 s** wrapper. The original
  full-suite failure is retained; no clean whole-suite rerun is claimed.
- `cargo fmt --all --check` passed using the existing absolute Cargo path.
  Repository and whitespace checks passed. No native compilation, Clippy,
  workspace tests, installed editors or platform batches were run for this
  harness/documentation correction.

Separate corrected harness freeze:
`completion-fixture-compatibility-freeze-r2.json`, **251 harness inputs**,
`8065142aaaaf8bf0e898cc5ddb14df05b791951790d68329677d46a7717a78b1`.
`fixture-compatibility-artifact-proof.json` reverified all **811** original r26
runtime seals and original VSIX `e91f564a…`. Native production inputs remain
unchanged; the same two previously approved external test modules differ from
compiled source `ac2620f0…`; current test source remains `9335cf1d…` / 1,116 inputs.
This was read-only verification, not a rebuilt/relabelled artifact or native run.
The earlier r1 freeze is retained; r2 includes the final documentation changes.

**Authentic producer prerequisite blocked:** published managed 1.5.12 supports raw
registration and original launcher; all 763 inventory hashes and its exact clean
source correspondence verify. Its schema-8 registry has no established supported
path into current schema-11–13 catalog admission. Audited newer raw payloads have
unmapped recorded source identities. None was selected or started. Reviewed
1.8.12 catalog authority remains separate and cannot qualify imported lineage.
No new T2 producer or six-editor attempt was made.

The user then explicitly selected **automatic rebuild and notify**, preserving
previous state, and requested documentation/release-note updates. The
[native recovery contract](../upgrade-rebuild-fallback.md) records that approved
policy, including fresh compatible successor state, exact saved sources/settings,
native resumable jobs, distinct rebuild provenance and complete interface parity.
Native implementation/qualification are **pending**; no generic fallback is
advertised as implemented. Existing explicit retirement/rebuild-repair owners do
not establish this preactivation recovery transaction. Independent docs review
found no blocking issue. README, documentation index, installation guidance,
Marketplace description, completion-plan addendum and release notes retain this
distinction. Original migration acceptance remains required.

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
| C2 client trust | in_progress | Actual Codex r4 passed scoped credential-authenticated reads, crossed-authority refusal, reconnect and wrong-issuer rejection on sealed Windows debug r24. Actual Code r1 failed tool discovery; Claude, final-artifact and other-platform acceptance remain pending. See the recovered results below. |
| C3 contracts | in_progress | Typed endpoint/credential selections, original read-attempt context, inference completion and checkpoint contracts compile and have focused integration coverage. Final full transport/platform qualification remains pending. |
| C4 in-process inference | in_progress | Windows actual model/client checks passed on optimized r2 and the matched current debug r18, including embedded 384-dimensional repeatable inference. Neo r9 linked debug but its model smoke failed private-root protection; the private-root correction is reviewed and Windows-tested. Fresh Mac/model/resource qualification remains pending. |
| C5 canonical HTTP/API/retry | in_progress | Streamable HTTP and raw JSON share canonical backends; typed pre-admission 50 ms retry, bounded sessions/output and native final-write fences authored. Accepted IPC completion ownership is being integrated; no full parity or transport qualification claimed. |
| C6 direct manager/endpoint | in_progress | Modern single-image launcher preparation/repair now returns without copying the sealed main; retained predecessor launchers require explicit migration retirement. Installed signed lifecycle and sealed-image preservation remain qualification gates. |
| C7 checkpoint/activation | in_progress | Typed terminal-receipt classification passed 39 checkpoint tests and five actual split cases. The actual ignored checkpoint owner test passed; sealed Windows debug r8 passed installed upgrade/rollback and genuine 1.8.12 semantic upgrade r13. Remaining predecessor/platform qualification is pending. |
| C8 all consumers | in_progress | Historical installed multi r16 and same-window UI update r3 passed their recorded scopes. Reconciliation extension r1 passed 1,074 tests with three skips on reused debug r24; installed r2 failed same-window retained-session use with HTTP 404 after a forward catalog clock. Other scenarios and final-artifact/platform acceptance remain incomplete. |
| C9 predecessor retirement | in_progress | Genuine 1.8.12 upgrade passed its recorded Windows scope. Sealed r17 positive adoption r20 passed both authentic 1.3.1 and 1.0.0-build.7 cases; the real corrupt-copy/restoration negative campaign also passed. These establish the tested formats/paths, not every predecessor or other platform; final qualification remains pending. |
| C10 distribution/docs | in_progress | Reconciliation workspace r1 completed with 3,235 passed, two failed and 52 skipped; its source receipt reports changed native inputs, so it does not qualify the exact live tree. Frozen Python r3 passed 801 total/12 skips; repository and dependency gates passed their recorded inputs. Final doctests, platform, signed/optimized package and installed acceptance remain pending. |

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

### C8 full extension pass and staged-editor startup correction

The storage correction was committed locally as `f54fb4d`, with the configured
human DCO sign-off; version stays **1.8.14** and nothing was pushed. The eight
authority tests, repository checks, version agreement and whitespace checks
passed again before that commit.

The complete enabled-native **r3** extension gate passed: **1,045 total,
1,042 passed, three skipped, zero failures**, **289.171 s** tests/
**289.240 s** including its subprocess runner. Extension compilation passed in
**12.927 s**. The original r21 executable was reused, not rebuilt: all **811**
sealed files and **1,114** current production-native inputs matched its original
compiled identity. `windows-debug-final-extension-r3-reuse.json` separately
records the extension test sources; its compile/native receipts retain the full
commands. Two POSIX-only cases and one historical-helper case remain skipped.

Native packaging of that debug runtime passed in **19.046 s**, and the
allowlisted development stage passed in **7.657 s**; no VSIX was created.
Actual installed Code then **failed** the staged multi-root journey in
**126.362 s** before the C8 additions were reached: its single Start action never
observed an authenticated connection. Fixture
`artifacts/editor-qualification/native-stage-Rm0nYF` and all UI/cleanup receipts
remain retained. The editor required forced closure and service cleanup was
unqualified because no final Stop receipt existed. Subsequent process inspection
found no process whose executable/command line belonged to that fixture.

The unchanged native runtime separately passed fresh isolated stage and
activation diagnostics in **7.118 s** and **9.133 s**. A read-only reproduction
then found identical stage inventories returned different loader roots through
Windows case aliases despite identical `fs.realpath` results. The staged editor
had logged an automatic interface replacement during Start. The correction
retains the already verified canonical root and recognizes case-equivalent
polling paths, avoiding false context retirement for unchanged bytes. All **18**
loader regressions passed, including the real-disk Windows alias regression.
The actual interrupted-action chain is consistent with this defect; the failed
run did not log its two original root spellings.

The editor driver now adds bounded actual HTTPS Assistance, one-dispatch
in-service model Stop with exact-generation readback, and owned-port endpoint
refresh coverage. Control clients are disposed and compiled helpers are sealed.
These assertions are not yet installed-editor passes. The first compile lacked
the later endpoint helper; `windows-debug-c8-editor-r1-harness-observation.json`
keeps the observed source and compiled identities separate. Post-correction
enabled-native **r4** passed: **1,046 total, 1,043 passed, three skipped, zero
failures**, **286.480 s** tests/**286.545 s** including runner; compilation
passed in **12.580 s**. The runtime reuse receipt revalidated the same original
payload/native source, with current extension source recorded separately.
Subsequent test-only deadline/busy-read/clock assertions are compiled again
before the fresh editor rerun, which remains in progress. No native Cargo
build, final optimized candidate, installed VSIX or platform qualification is
claimed by this checkpoint.

The corrected staged-editor **r2** run passed first Start and reviewed setup,
then **failed in 29.740 s** at the permanent-block draft with the complete native
`{status:"failure",code:"registry_busy"}`. No policy Apply was sent. Fixture
`native-stage-Mye22Y` and its full diagnostics remain retained; cleanup was
unqualified without a UI Stop receipt. A subsequent diagnostic health probe
returned `service_unavailable` before preparing or submitting any Stop.

Queue-idle evidence does not prove absence of a collection writer handoff.
Native source-draft reads explicitly permit that typed contention; shared
extension management reads previously allowed only 25/50 ms backoff. The
central correction retains the same whitelisted read through a bounded
three-second monotonic retry window, at most six additional submissions, preserving the
complete final failure. Epoch acquisition, mutations and unknown outcomes are
excluded. Admission can also report `registry_busy` before an epoch request;
the failed editor receipt did not identify its exact method, so the competing
lock/queue source remains unproven. No generic UI action or key is replayed.
All **29** focused client/loader tests passed on the initial two-second/eight-retry
proposal; enabled-native **r5** retains that identity. The user then selected
three seconds and six retries with larger waits: **100, 200, 400, 600, 700,
800 ms**. Current-source qualification of that adjustment precedes another
fresh editor attempt. Neither failure is relabeled as installed-editor acceptance.

The initial policy's **r5** full suite passed **1,047 total, 1,044 passed,
three skipped, zero failures**, **288.407 s** tests/**288.471 s** including
runner. Current-policy **r6** then passed the same totals in **285.652 s**
tests/**285.716 s** including runner. Its compile passed in **12.245 s** and
all **29** focused tests passed in **4.778 s** tests/**4.837 s** including
runner. The runtime remains the original r21 debug build with separately
revalidated native source and payload seals; no native build was launched.
The current extension/driver's fresh stage passed in **7.878 s**, and actual
editor **r3** is in progress. A separate observation found no executable under
either failed fixture's path; that process observation does not supply the
missing UI Stop receipts or certify full cleanup.

Actual editor **r3** then **failed in 29.537 s** at the same read-only source
review, now with a different complete native refusal: `source_change_busy`,
`source_change_jobs_pending`, the exact index and **one queued/zero running**
jobs. No policy Apply was sent. Fixture `native-stage-XoIcFC` and its failed
cleanup remain retained. The six-retry policy applies only to `registry_busy`;
it does not reinterpret this source-work refusal. The test-only correction
records this known non-admission, waits through actual native job observations,
then permits a new bounded read-only review. Unknown, generic or crossed
refusals and all Apply operations remain outside that path. This explicitly
tests the busy-to-review journey rather than treating a dated idle sample as
future mutation admission. The earlier failure remains a failure.

The next test-only preflights retained their own failures: editor **r4** failed
in **30.128 s** because repeated idle observations reused an evidence filename;
the correction includes window, index and UI-request sequence, uses exclusive
creation and never overwrites earlier evidence. Its **57** contract/UI checks
passed; the filename regression raised the scoped count to **58**. Editor
**r5** then failed in **119.154 s** on an idle-wait deadline. Its **88** retained
samples prove the queue had drained: a succeeded automatic **Verify** job had
replaced the initial Index job in cached progress. Initial index-work proof is
now retained first; only later observations for the exact fixture, index,
backing, service instance and namespace can accept a succeeded canonical
maintenance job as dated idle evidence. All original initial-index, queue,
pause, worker-fencing and failure checks remain enforced. **60** scoped
contract/UI tests passed. No production/native payload changed for these fixes.

Editor **r6** reached first index-set creation and failed in **35.630 s** with
`managed_scope_set_is_not_editable`. This exposed a product guard comparing two
absent IDs as equal. The correction requires an existing ID before matching
the immutable managed set; fresh named sets remain unselected until explicit
scope Apply. A unit regression covers creation with no managed set and protects
existing managed sets; the actual native/CLI test now creates its named set
before ever creating a managed scope. Two compile-only attempts failed on test
typing/import errors and ran no native suite; their receipts remain retained.
After those corrections, **54** focused tests passed, including actual native
fresh-state creation; current enabled-native **r9** is running before any next
editor attempt. The preceding full-suite passes retain their original source
identities.

The complete multi-root driver audit also corrected a test-only DTO assumption:
combined native diagnostics stays `airk.combined-read/1` and has no legacy
`explorer.ui` enrichment. HTTPS help is tested independently, and installed UI
attachment now uses one empty-argument status read through the exact private
standalone Assistance descriptor, with its own catalog and process cleanup
evidence. No combined reply or UI authorization contract was changed. Four
native-read-contract checks passed. Actual Assistance, provider Stop, endpoint
refresh and final editor/package acceptance remain unqualified until observed.

The first-set correction's complete enabled-native **r9** suite passed:
**1,051 total, 1,048 passed, three skipped, zero failures**, **289.218 s** tests/
**289.295 s** including runner; compile **12.241 s**, focused **54** tests
**33.207 s** tests/**33.279 s** including runner. The original r21 native
identity and all payload seals remain unchanged. Test-only Assistance cleanup
then gained a strict zero-exit/no-signal/no-force assertion before packaging.

A **local debug VSIX** was produced and verified in **25.095 s**; it is not the
final optimized Windows candidate. Archive SHA-256 is
`b6a441de0d3471494f9272e45f40b524e983cd749d18f219c25223c72b823ef0`.
`windows-debug-c8-installed-inputs-r1.json` records its actual uncommitted
extension sources separately from packaging's Git context and original native
build identity. The package contains 921 files, approximately 81.99 MB. Actual
installed multi-root acceptance **failed in 40.264 s** after successfully
creating the first named index set: scope selection retained complete
`invalid_registry_request`, with no pending local Apply intent. This generic
failure is not retried or converted into the registry-busy policy. Native
aggregate logs were retained; detailed raw logs were empty. Read-only inspection
found policy/application, source roots, catalog/epoch and receipt capacity
consistent, with all recorded native jobs succeeded. That final state cannot
uniquely identify a transient failed method. Fixture `native-installed-pGrPHQ`
and its unqualified cleanup remain retained; no scope acceptance is claimed.

The independent Explorer update attempt first failed in **310.257 s** before
the test driver activated. VS Code rejected its generated `engines.vscode: "*"`;
AIRK 1.8.12 itself activated. Both generated hot-update and upgrade drivers now
use the supported **^1.140.0** floor, and installed-path containment uses
canonical locations instead of Windows-sensitive prefix spelling. The failed
`hot-explorer-s7nbj9` fixture remains retained. The corrected check passed on
the exact same two archives in **16.640 s**, after its focused script contracts
passed. `hot-explorer-uS3o9Z/result.json` qualifies only actual same-window UI
with native execution disabled: stable extension host/profile, retained route,
one Explorer panel and saved dismissal after update/reopen. This does not
qualify native installation, migration, signed payloads or other platforms.


### C8 policy readiness, atomic mailboxes and shared index-access retry

The installed r3 trace (`native-installed-RYRz2n`, **41.064 s**) identifies the
scope-selection refusal as `catalog_grant_draft`, not epoch acquisition.
Read-only retained timing evidence (`c8-policy-admission-timing-r1.json`) shows
the matching source-policy application arrived **332 ms after** the refusal,
and its configure job succeeded **548 ms after** it. Native policy readiness
correctly fences grants while accepted asynchronous configuration is running.
The private driver now waits for the exact operation/job, policy digest and
application report before preparing a grant once. It does not retry generic
`invalid_registry_request` or weaken native admission. Earlier final-state
inspection alone could not establish this ordering.

Installed r4 (`native-installed-ZAEtr6`, **10.763 s**) then failed before its
first Start dispatch: the controller read partially written request JSON. All
C8 JSON handshakes now use exclusive same-directory staging and atomic rename;
malformed JSON remains an error. Windows transient replacement contention uses
six bounded retries in three seconds with increasing waits, preserves the
existing destination on exhaustion and never deletes it to make progress.
Six focused mailbox tests passed, including concurrent complete reads,
foreign-file preservation and exact retry exhaustion. A zero-delay saturation
reader exhausted the approved bounded policy; the retained regression models
5 ms polling (the actual controller polls 150 ms), rather than requiring
unbounded progress against a continuously open destination.

The user's clarified policy covers AIRK index/catalog access, not the Windows
Registry. The working source moves exact `registry_busy` recovery from the
extension whitelist to the shared native control-request boundary used by
CLI/MCP/Workbench. It preserves the identical method, key and expected service
instance/version through at most six additional requests in one original
three-second monotonic window, waiting **100, 200, 400, 600, 700, 800 ms**.
Native emitters prove either recognized read-only storage contention or queue
rejection before acceptance, so a mutation rejected at admission can also
retry safely. Unknown outcomes and generic/source-job refusals retain their
separate contracts. The extension no longer adds a second retry window.
This production correction requires a fresh sealed debug runtime and fresh
acceptance; r21 evidence remains attributed to its original build.
The final caller audit also removes outer exact-busy replay from CLI epoch
acquisition, Explorer's context observer and the native acceptance helper;
otherwise each could restart the shared window after exhaustion. Separate
unknown-epoch recovery and typed evidence-admission policies remain intact.
The intermediate r22 debug build completed in **128.272 s** (Cargo **28.91 s**),
but the later epoch correction supersedes its source identity. A new r23 debug
candidate is required; no r22 reuse/current qualification is claimed. Scoped
helper-enabled Clippy passed in **44.011 s** with the persistent P10 target,
incremental compilation on and retained Cargo timing HTML. The earlier
local-inference/all-targets attempt exposed existing helper-gated fixture
compilation errors; they remain recorded. Nextest was unavailable, so the
scoped tests use the documented Cargo fallback. The first executable-target
selection ran zero tests and does not qualify any coverage; corrected library
selection is required. Timer-specific retry tests use paused Tokio time to
avoid falsely failing valid deadline exhaustion under a loaded scheduler.

The corrected `cargo test --locked -p airk-cli --no-default-features --features
embedding-worker --lib --timings -- --nocapture service_runtime::
client_requests::tests::epoch_acquisition console_client::tests
service_management::tests` ran **22 tests, all passed**, **13.22 s** execution /
**66.448 s** including compilation. It includes real native IPC through all four
public request variants and deterministic six-wait/deadline checks. Final scoped
`python scripts/dev-check.py -p airk-cli --features embedding-worker --target-dir
artifacts/v190/p10-target --incremental on --skip-fmt` passed in **11.059 s**;
formatting passed first. Cargo `--timings` reports remain under the P10 target's
`cargo-timings` directory. These helper-enabled component checks do not replace
shipping local-inference runtime or final workspace qualification.

The fresh **r23 debug** runtime built and sealed in **59.959 s** (Cargo
**18.92 s**), with static import inventory, embedded-model smoke and legal
inventory checks. Its **1,115** source inputs hash to
`bd4a90f84899bc168e0c813918b1f077c6193d6d1f0d2da815076e493a7ceb69`;
install-manifest SHA-256 is
`3f25bb41342b84c9fdab683510f3ee27c747bf9a2cb65d4d550963f602e07d0d`.
All **811** payload seals and current source inputs were checked before the new
extension campaign. Focused adapter tests passed **142/142**, the Python
service/container mocked contracts **48/48**, and private mailbox tests **6/6**.
An independent combined review found no blocking issue after the obsolete outer
loops and cleanup-error preservation were corrected. The complete enabled-native **r10** suite then passed **1,060 total, 1,057
passed, three skipped, zero failures**, **290.359 s** tests / **290.429 s**
including its runner. Compilation passed in **12.266 s**; 58 focused cases
passed in **31.383 s** / **31.445 s** including the runner. The two POSIX-only
and one historical-helper skips retain their qualifications.

The tested r23 runtime packaged in **17.030 s** and its local debug VSIX in
**20.162 s** (921 files, approximately 82.02 MB). The first packaging invocation
stopped before staging because `rustc` was absent from PATH; the corrected
invocation supplies the explicit existing toolchain location. Current archive
SHA-256 is
`cc72c5dd52c5c2830c4d733fffdcd3aae4a3e9d24d8a22c6f267859d1a9575b6`.
All 278 extension source/test inputs match the completed full-suite receipt;
85 compiled production outputs match the sealed stage. The new installed run
(`native-installed-5XizEw`) failed in **10.590 s** at its first Start evaluation
with **unknown UI dispatch outcome**. It was never replayed. The receipt does
not retain the returned CDP value/type, so it cannot distinguish lost context
from a pre-click refusal. No installed C8 acceptance is claimed.

Read-only source inspection identifies a harness race: readiness and click are
separate evaluations, while automatic Management observation can disable Start
between them. A test-only correction must recognize only an explicit synchronous
pre-click no-effect result as eligible for bounded fresh readiness; exceptions,
malformed/missing values and unknown outcomes remain terminal. The failed
fixture stays retained. Further acceptance uses a fresh fixture and the exact
same sealed debug VSIX; no native rebuild or repackage is needed.

Installed **r7** (`native-installed-lhvxEe`) progressed through Start, setup,
policy application and first named set, then timed out in **162.983 s** while
waiting for active refreshed scope. Retained full snapshots show a successful
`search_scope_selected` result, a real grant at catalog revision **9**, and no
pending mutation; the choices were absent and the renderer reported
`catalog_request_pending`. This supersedes the earlier generic grant-preparation
blocker but does not pass installed acceptance. Read-only process observation
afterward found no native process beneath that exact failed installation;
this is not evidence that the installed UI Stop action passed.

The product correction preserves acknowledgments for explicit scope/set actions
that legitimately change their own evidence selection, matching the existing
grant-use action. Host adapters still enforce native window/runtime/trust
ownership; unrelated late evidence responses remain fenced. The Indexing UI now
holds a local synchronous and rendered working guard through each complete
mutation plus fresh-read chain, so its automatic missing-choice read cannot
race the explicit post-apply read. Old catalog rows are never restored as fresh.
Actual-panel acknowledgment and deferred-renderer regressions passed with the
existing ownership/evidence tests: **80/80**, **2.544 s** tests / **2.606 s**
including the runner. Compilation passed in **12.183 s**. Independent review
found no blocking issue. Native r23 inputs/seals remain unchanged; the new
extension source requires full r11 acceptance and a freshly sealed local VSIX.

The post-correction complete **r11** suite passed **1,063 total, 1,060 passed,
three skipped, zero failures**, **305.757 s** tests / **305.825 s** including
its runner. A new local debug VSIX packaged in **21.198 s**, SHA-256
`ab8d8216e46c01c4a533eb26bd8f3602c495a77d1b2558650faed5373a5049b3`;
all packaged production source inputs match that completed suite. Native r23
was reused with its original compiled/sealed identity.

Installed r8 (`native-installed-a6SAHY`, **49.233 s**) passed active scope but
stopped at a test-only raw path comparison: native Windows `\\?\` paths and
ordinary canonical paths identified the same owned private cache root. The
driver now normalizes namespace spelling after ownership/canonical checks;
original raw native evidence and immutable profile receipts stay unchanged.
The r9 compile-only attempt exposed unknown-value typing in those assertions;
strict type/array checks corrected it before any editor ran.

Installed r10 (`native-installed-VZLIKf`, **47.769 s**) then reached complete
native activation but the helper required optional `index_ids` in a whole-set
selector. Canonical input deliberately omits that restriction, while the native
read's effective selection always includes resolved index IDs. The corrected
helper retains the original selector unchanged and calls the canonical complete
activation reviewer to prove the exact singleton index/project, original backing,
policy/source revisions and full source-root boundaries. It rejects additional
frozen projects/indexes and crossed metadata; the later complete read/member
assertions remain strict. **59 focused contracts passed**. Only three test files
changed after full r11, and all **85** production outputs still match the VSIX;
`windows-debug-c8-harness-sources-r11.json` records the separate test identity.

Installed r11 (`native-installed-jyJBWR`, **58.675 s**) passed that proof and
performed a real reload. Its first definition snapshot was empty after console
Connect. AIRK-only logs show context preparation still pending and no saved-grant
restoration refusal. Startup readiness does not await that asynchronous chain;
definition lookup itself does not start it. A bounded observation of definitions
is required before failing reload: retain zero/one observations, reject multiple
or crossed authorities immediately, preserve the original grant/selector and
fail on deadline. This loop must not Apply, use another grant or alter selection.
The exact same debug VSIX/native seals remain the acceptance candidate.

The complete remaining journey was then audited before another editor run:
Help, combined diagnostics, standalone Assistance, provider controls/ownership,
endpoint relocation, source isolation, selected-set deletion, Stop and source
preservation were matched to current implementations. Two additional harness
gaps were corrected together: retain the original and temporary HTTPS clients'
complete raw cohorts and actual cleanup receipts, and allow only a validated
forward catalog clock when comparing otherwise immutable connection snapshots.
The reload helper's new forward-clock regression also caught reversed
actual/expected authority arguments before any editor launched; r12 stopped at
that unit failure. All **65** consolidated harness tests subsequently passed,
and independent integration review found no blocking issue.

Consolidated installed **r13** (`native-installed-DkurtA`, **90.986 s**) exhausted
all **128** definition observations with no restored definition. This is not
explained by a premature snapshot: AIRK's reload log records
`http_enrollment_refused`. Read-only SQLite inspection confirms the saved
namespace/context/project/grant survived unchanged. Native credentials still
bind the old random Assistance-window descriptor, while the recreated host has
a new descriptor. The extension's stable enrollment key omitted that immutable
attachment, so native enrollment correctly rejected reusing the old ID.
No native equality or permission rule was weakened, and no unknown UI action
was replayed.

A focused actual-controller regression with real Assistance hosts reproduced
the exact refusal in **0.490 s** before production changes. Its compiled test
and original sealed production identity remain in
`c8-attachment-red-source-r1`. The source correction captures the attachment
once for its enrollment key, request and stale-owner guards, including selected,
legacy union/single and fresh standalone paths. Existing headless keys and old
manual-configuration IDs/provenance are retained; bounded source-aware legacy
lookup recognizes the actual credential in an inspected declaration. Existing
grants/index scope are unchanged. Startup now waits for current local Assistance
publication before fresh enrollment and sends definition-change notifications
on completion; an obsolete setup waiter stops before native dispatch. Optional
Assistance failure still permits headless operation.

The corrected attachment/startup and related HTTP/legacy/configuration campaign
passed **143/143** tests in **2.906 s** / **2.966 s** including its runner, after
compilation in **14.908 s**. Intermediate test-only typing/serializer and nested
mock-restoration failures are retained; the latter was confirmed against Node's
restore-in-creation-order implementation and fixed by changing one mock's
implementation rather than stacking mocks. Independent final review found no
blocking issue. Native r23 production inputs remain unchanged; this later
extension production change requires full r12 and a new VSIX before installed
acceptance.

**Explicit remaining capacity gap:** the native HTTP credential ledger allows
128 lifetime retained records, and revocation does not reclaim slots. New
Assistance attachments require new immutable credential identities. Retention/
compaction needs an ownership-safe design during C2/C6/C9 reconciliation before
final qualification; this correction does not delete old manual credentials,
create replacement grants or claim that gap is resolved.







### C8 final notification review and complete extension gate

The first attachment-correction full run, r12, retained **1,070 passed, one
failed, three skipped**: an unchanged headless setup emitted an unnecessary
definition-change event. The corrected full r13 passed **1,071 tests, three
skipped, zero failures** in **305.572 s**. Separate review then identified the
superseding-setup case: a published attachment must still be withdrawn if a
pending replacement is superseded by headless operation. The final source tracks
the last published descriptor, emits only current-generation actual changes,
and has a regression for that precise transition. No source was edited during
the full run. Independent final review found no blocking issue.

Final focused checks passed **146/146** in **3.272 s**, following compilation
in **12.338 s**. Full enabled-native **r14** passed **1,075 total: 1,072 passed,
three skipped, zero failures** in **295.142 s** (**295.209 s** runner).
`windows-debug-final-extension-r14-reuse.json` verifies all **811** native payload
seals and the original r23 production-source identity, and separately captures
the current extension source hashes. This is native reuse, not a fresh build.
The skips are two POSIX-only cases and one historical-helper case.

Fresh local debug VSIX **r4** packaged in **21.422 s**, reusing the already
verified r23 native archive. Its source hashes match full r14. Installed r14
must complete before Assistance, provider Stop, endpoint refresh or the full
installed C8 journey can be qualified; this package is not the final optimized
Windows candidate.

Installed **r14** (`native-installed-uLgnkj`, **181.444 s**) restored the exact
saved grant after actual reload: retained definition observations progressed
from zero to one with the original grant and catalog clock 9. It then failed
at a redundant driver Apply selection. The reload branch discarded its
validated definition and unconditionally invoked the fresh-selection helper.
That helper also incorrectly expected an explicitly applied empty selection
to have `active=false`; the canonical controller retains an active empty
choice with withdrawal. No installed completion or service cleanup is claimed:
the failed fixture has no native-stop receipt and remains retained.

The test driver now keeps the validated restored definition and uses that
original authority directly. Compilation passed in **12.527 s** and all **65**
focused harness tests passed in **1.210 s**. Installed r15 reuses the exact
sealed VSIX r4, SHA-256
`1f0aed1eabf7ec69a0be2ee8d8f3c01d560c3b70ce7f23f3a09348fd0f00efb1`;
`windows-debug-c8-installed-r15-reuse.json` verifies unchanged packaged
production outputs and records the separate current driver/test identities.
This harness-only correction does not relabel full r14 as a new test run.

Installed **r15** (`native-installed-e1Erzt`, **96.805 s**) passed the actual
reload, authenticated scoped reads, standalone Assistance and provider Stop,
including lexical reads afterward. Endpoint relocation reached the fresh
authenticated native `status` response, then the driver rejected its canonical
token-only publication summary as if every tool returned an expanded revision
envelope. The exact token matched the complete publication-vector identity.
This failed attempt and its unqualified final service cleanup remain retained.
The collision listener also observed three TCP contacts; their origin and the
helper's aggregate no-contact assumption require investigation before claiming
that installed endpoint gate passed.

The native `status_summary` contract intentionally returns `{token}` even in
full response mode. The corrected member assertion accepts that exact token-only
shape or the exact full envelope, while validating the complete vector's selected
collection, incarnation, generation, revision and canonical token encoding.
Crossed summary scalars, mixed/partial envelopes and malformed identities remain
refused; ranked origins and complete raw transport evidence are unchanged.
The already-active scope-clear helper also now requires its terminal cleared
outcome and both selected-ID arrays empty, rather than `active=false`.

Read-only review confirmed native port discovery only binds the saved port and
falls back on address-in-use. Other actual editor clients can still close or
reconnect their captured old HTTPS URLs. Retained logs cannot attribute the
three TCP contacts. The installed listener still immediately destroys sockets
without inspecting application data; its evidence now explicitly records
unattributed contacts and uninspected data. Installed qualification requires the
new endpoint/generation, unchanged credential/CA/selector and fresh authenticated
reply, and makes no aggregate zero-contact claim. Isolated native binding
regressions retain their separate no-probe assertion.

All these final harness-only changes passed separate review and **68/68** focused
tests in **1.242 s**, after compilation in **12.064 s**. Installed r16 again uses
the unchanged sealed VSIX r4; its separate production-output and test-source
identity receipt is `windows-debug-c8-installed-r16-reuse.json`.

The actual same-window Explorer update **r3** passed on this same VSIX r4 in
**16.858 s**, retaining route, singleton and saved notice dismissal. Its scope
is UI behavior with native execution disabled, not installed-native lifecycle
or optimized-artifact qualification.

### C8 immediate Windows functional gate completed

Installed **r16** (`native-installed-mPvQJQ`) passed in **104.973 s** on Code
**1.145.2**, using the same sealed debug VSIX r4 and unchanged r23 native inputs.
`summary.json` explicitly selects only **multi**; its selected-scenario success
does not qualify same/different-window, Usage, untrusted or historical-adoption
scenarios. The actual journey passed saved-scope reload without reapplication,
authenticated native tool inventory/reads, exact citations and source exclusion,
storage-preference continuity, live Help, standalone Assistance, provider Stop
with lexical reads afterward, port relocation and fresh authenticated status,
Explorer cited search/current sets/help, and native deletion fencing of the
retained MCP session. Complete raw transport cohorts and cleanup observations
remain in the retained fixture.

The endpoint receipt has one Stop and one Start dispatch, three unattributed
TCP contacts with application data uninspected, a successful authenticated new
endpoint and a closed owned listener. These limits are explicit; no zero-contact
or caller-attribution claim follows. Final native Stop has a **complete** owned
lifecycle receipt, and `multi/native-cleanup.json` records **qualified=true**
with the installation explicitly stopped. All **six** source files retained
their original hashes. The final evidence is
`windows-debug-c8-installed-native-editor-r16.json` and the retained fixture's
`summary.json`, `multi/a-result.json`, `multi/native-stop.json`,
`multi/native-cleanup.json` and `multi/a-c8-endpoint-refresh.json`.

This completes immediate steps 1–2: the earlier storage correction is committed
locally as `f54fb4d048a55f80b0ac2e980260a8f849cfa82c`, and the complete
enabled-native suite plus installed acceptance above passed. C8's wider matrix
remains in progress until existing evidence is reconciled and missing actual
scenario/platform coverage runs. Version remains **1.8.14**. No push, PR,
publication or new optimized build campaign was performed. Final workspace
checks must cover production changes after the earlier 3,224-test pass; that
historical pass does not qualify the current release source.

### C2–C9 reconciliation resumed (8 November 2026)

The all-fifteen [client classification](../agent-integration.md#client-classification-for-1814-8-november-2026)
now distinguishes three implemented HTTPS serializers from twelve blocked
adapters. The retained Codex experiment proves actual app-server TLS/tool
discovery, without a bearer credential or scoped AIRK read. Claude and the
separate Code/Copilot MCP host still require actual connecting-process acceptance.
Existing issuer rotation r2 (105.111 s) and port relocation r2 (33.870 s) passed
on debug r18; these are historical fixture observations, not final-artifact or
external-client qualification.

The missing Windows installed scenario batch `reconcile-c8-installed-r1` failed
in its first, **same** scenario after **61.722 s** on unchanged debug r23 / VSIX
r4. Window A retained catalog revision 9 while window B's selection advanced it
to 10. `airk.index` pinned the old metadata revision and received
`invalid_registry_request`; different, Usage and untrusted did not run. Native
Stop/cleanup remains unqualified for this failed fixture. Three controller
regressions reproduced the refusal before correction. Selected Index management
and Entire Index Usage now read current metadata for the exact chosen Index,
requiring a forward/equal revision, unchanged namespace/backing, live Index and
the original window/selection owner. No selection is reapplied and no mutation
is retried. The full trust/controller module passed **32/32** in **3.082 s**
(**3.142 s** runner), after compilation in **12.487 s**. This extension change
requires a fresh VSIX and installed acceptance.

Credential-capacity source work retains the 128-record/1 MiB active bound.
Durable revocation and gateway delivery fencing precede replacement of an old
token with a bounded installation/ID retirement proof and full-record compaction.
Positive owned attachment closure/expiry can retire an attachment; unavailable
or malformed evidence cannot. Active manual connections remain intact. Old IDs
cannot be reused, orphan tokens remain unknown, and historical proof storage
still grows with issuance. Gateway epochs fence authentication captured before
revocation; a typed epoch refusal is HTTP 503, while invalid credentials remain
401. Independent review identified a cached-context reinsertion race that must
be corrected and tested before this capacity gap is resolved.

The first shipping-feature scoped CLI Clippy attempt failed after **41.419 s**
using `python scripts/dev-check.py -p airk-cli --features local-inference
--target-dir artifacts/v190/p10-target --incremental on --cargo
C:/Users/tdotn/.cargo/bin/cargo.exe`, including formatting and Cargo `--timings`.
The retained report is `reconcile-native-clippy-r1-cargo-timing.html`. It found
legacy feature guards hiding `wait_semantic` and shadowing a reviewed service in
`vector_selection` when only the shipping feature is enabled. Test guards now
select `local-inference` for modern semantic setup/default-search coverage;
genuine helper fixtures retain `embedding-worker`. Validation is pending.
An earlier local formatting wrapper failed before spawning Cargo because its
executable was not on the wrapper's original PATH; the absolute-path rerun passed.

The existing `benchmark-service.py` already supplies the bounded 1/10/64-client
semantic campaign, distinct credentials, complete CLI/MCP/JSON search parity,
native resource/owner/load observations and explicit measurement limits. Its
**14** contract tests passed (**0.923 s**, **1.190 s** runner). No redundant new
campaign was retained. Actual current-source execution remains pending; neither
this harness nor native RSS observations prove OS private memory, idle CPU,
global helper absence or proprietary assistant UI acceptance.

Neo **r11** is now fully collected and remains **failed** on native source
`27981a7de44b31570f83d58a67255db2ece5eb7cecab27bcb16b7c4c0b79ea90`
(1,114 inputs). Its **679.370 s** batch passed Python **790 total / 19 skips**,
formatting (**3.185 s**), workspace Clippy (**150.546 s**), lightweight compile
(**38.750 s**), static debug build/model smoke (**116.838 s**) and checkpoint
regression (**97.139 s**), then failed install preflight (**243.852 s**) because
the predecessor-retirement census could not observe two live foreign executable
paths. Full workspace, optimized, signed and editor stages were never reached.
All completed logs, source/artifact receipts and timings were collected in
`neo-r11-reconciled-20261108`; required Cargo cleanup passed (**0.775 s**) and
compiler descendants were absent. This is collection of an earlier failed run,
not a new Mac qualification.

Read-only native probes reproduced both ordinary and audit-token executable-path
`ESRCH` for a still-live process. The Mac owner now requires a proven dead
retained task-name right before treating that path failure as retirement.
Cross-host and actual owned-child Mac regressions are authored but unrun.
The strict census blocker remains; no weaker memory-map absence proof was added.
Closing Code did not remove its detached crash handler or the detached
bookbuilder dev server. Stopping foreign processes requires the user's separate
authorization; no such processes have been stopped by this reconciliation.

Version remains **1.8.14**. No optimized freeze, final workspace pass, platform
completion, package acceptance, commit, push, hosted dispatch or publication is
claimed by this checkpoint.

### Recovered completed reconciliation results (8 November 2026)

This section supersedes the earlier reconciliation's pending observations without
relabelling its failed attempts or historical artifacts. Receipts are retained
under `artifacts/local/one-airk-1.8.14`; none of the following starts a new run.

The cached-context reinsertion correction serializes cache insertion with
revocation and rechecks the delivery fence. Shipping-feature native preflight
`reconcile-native-tests-r3.json` passed **98/98** tests in **213.365 s** runner
time (**168.571 s** execution), including credential compaction, TLS fencing,
semantic setup/recovery and cross-host Mac exit classification. This supersedes
the earlier source-review race blocker and unvalidated feature guards, not the
remaining actual lifecycle, capacity or platform acceptance.

Windows debug **r24** built in **78.497 s**, with native source
`01814b2357ba556faf4d4f8d3ad21c0183e490f10d3e4bc1b315b665202688fe`
(**1,115 inputs**) and installer manifest
`501c8d85a8d6b836489d4d425f0cb142d3aa965221f70629899dd85de58b7acc`.
Its actual HTTPS/model-client gate passed in **67.062 s** with complete managed
Stop. The native workspace Clippy, lightweight compile and default build passed
in **48.292 s**, **34.841 s** and **41.542 s**, respectively. These receipts
retain their original source identities; they are not proof for subsequent edits.

Workspace `reconcile-workspace-tests-r1.json` completed with **3,235 passed,
two failed and 52 skipped** (**1,974.948 s** runner, **1,867.631 s** execution).
It ran `cargo nextest run --locked --workspace --all-features --profile ci
--ignore-default-filter --no-fail-fast --timings`, using the persistent
`artifacts/v190/p10-target` with incremental **on**. The failures are explicit
source/storage retirement and reference-refresh process coverage; the latter
aborted with Windows stack overflow `0xc00000fd`. Its receipt reports
`native_inputs_unchanged=false`: native inputs changed during the campaign, so
neither its successes nor its failures establish qualification of one exact
current source tree. The Cargo timing report is
`reconcile-workspace-tests-r1-cargo-timing.html`. A frozen-source correction pass
and final doctests remain required.

Enabled-native extension `reconcile-extension-full-r1.json` completed with
**1,074 passed, three skipped, zero failures** in **311.732 s**; it reused r24
with the original compiled identity. That suite does not qualify the installed
journey: `reconcile-debug-installed-r2.json` failed its **same** scenario in
**88.123 s** with HTTP **404**, `Not Found: Session not found`. The read-only
session audit retained a forward catalog clock **9 → 10**, unchanged immutable
selection and an unrevoked generation-1 credential. Completed deliveries already
used catalog 10. This implicates exact cached-context equality replacing the SDK
service/session when only the catalog clock advances; the retained in-process
cache itself was not directly observed. Remaining scenarios did not qualify,
and this failed fixture has no qualified native Stop receipt. Correct session
continuity under unchanged authority before rerunning installed acceptance.

Frozen Python `reconcile-python-frozen-r3.json` passed **801 total / 12 skips**
in **87.782 s** (**87.046 s** tests). Repository checks passed for **2,064 public
files** in **2.928 s**. The live-advisory dependency gate passed in **15.771 s**,
with its documented dispositions for `paste` (`RUSTSEC-2024-0436`), `ttf-parser`
(`RUSTSEC-2026-0192`) and `minimatch` (`GHSA-g3hv-75wg-m249`); it is not a
zero-findings claim. Their receipts are `reconcile-repo-check-r1.json`,
`reconcile-dependency-audit-r1.json` and `reconcile-dependency-audit-result-r1.json`.
Later harness or source edits require their affected checks again.

Actual Codex **0.164.0-alpha.11.1** r4 passed in **62.412 s** on sealed r24:
29 tools, bearer-authenticated scoped status/source reads, original publication,
crossed-authority refusal, fresh-process reconnect, preserved additive CA trust
and wrong-issuer rejection before HTTP/Authorization. All owned clients exited
and native Stop completed; no model turn or OS trust/user-config mutation was
used. Evidence is `windows-debug-r24-actual-codex-r4.json` and its result/reuse
receipts. This supersedes the fictional TLS spike for that exact Windows debug
scope. Actual Code **1.145.2** r1 failed tool discovery in **115.886 s**; its
certificate/trust investigation does not qualify a successful Code/Copilot
connection or cleanup. Claude and final-artifact/other-platform acceptance remain.

The semantic resource campaign `reconcile-debug-client-load-r2.json` completed
the **1**- and **10**-client cohorts, then failed **64** clients with
`Semantic clients loaded additional model copies` (**410.050 s** overall).
The failure remains recorded. The later harness distinguishes an observed idle
uncached model before the first existing warmup read, permits exactly one load
only with complete zero-outstanding evidence, and freezes owner/load count after
that boundary. Its focused cache-boundary checks passed **39** tests; the actual
campaign on that newer harness is still required. Native aggregate RSS and load
counters do not establish private physical memory, idle CPU, global helper
absence or external assistant acceptance.

The original Linux/container r2 receipt stopped at **running**, `passed=false`,
after its **2.168 s** cache-owner regression passed. Read-only Docker Buildx
history recovery now establishes that build `lw5phixc05c3iw4ozvtbl1udy` was
**canceled**, `context canceled`, at **2026-11-08 12:27:54 America/Denver**, after
**2,251.936 s**. Static ORT provisioning was canceled; Clippy/rustfmt tool
provisioning had completed, but no AIRK compilation or qualification followed.
Docker reported no running containers at recovery. The original receipt is
preserved; `linux-container-qualification-r2/recovery-history-record.json`,
`recovery-buildx-inspect.json` and `recovery-buildx.log` retain the completed
observation. Collection launched no build and stopped no process.

Read-only Neo recovery also found previously uncollected **r12**, campaign
`one-airk-1814-final-r12-1794164952960287200`, on native source
`01814b2357ba556faf4d4f8d3ad21c0183e490f10d3e4bc1b315b665202688fe`
(**1,115 inputs**). It ended **failed/interrupted** in **603.141 s**, with
**801 Python tests / 19 skips** passed (**14.939 s** runner), formatting
(**3.245 s**), workspace Clippy (**151.240 s**), lightweight compile
(**39.704 s**), shipping native regressions (**126.311 s**), debug static
runtime/model build (**51.539 s**) and checkpoint regression (**80.729 s**)
passed. Shipping execution ran **46/46 passed, 729 skipped**, in **8.878 s**,
including both the actual owned-child retained task-name proof and missing-path
classification regressions. These Mac regressions are therefore no longer
unrun; their result retains this exact source identity.

`root-directed-interruption.json` records deliberate interruption before broad
workspace/release testing on known pre-correction MCP session-cache source.
The exact owned install preflight received audit-token-bound SIGINT and returned
**-2** after **121.364 s**, while sleeping in its stable-owner interval. Its
`KeyboardInterrupt` is not a Mac assertion failure or a passing installation
gate. The owned failed installation fixture remains retained. Required Cargo
cleanup passed (**0.720 s**) after evidence preservation; the receipt reports
all owned descendants settled and compiler target absent/empty. A recovery SSH
probe observed no active compiler/campaign images. No new workload, process
stop or cleanup ran during recovery.

The complete existing r12 receipts, logs, JUnit and Cargo timings were copied to
`artifacts/local/one-airk-1814/neo-r12-recovered-20261108/evidence`, including
`batch-receipt.json`, `root-directed-interruption.json`,
`shipping-native-corrective-regressions.log` and
`debug-affected-install-https-preflight.log`. R11 remains its separately recorded
census failure. Final-source native install/session continuity, full workspace
and doctests, optimized, signed, Aqua lifecycle and installed-editor/VSIX stages
remain incomplete. Final optimized freeze and distribution remain incomplete. Version stays
**1.8.14**; no commit, push, PR, hosted dispatch or publication is claimed here.

### Resumed correction checkpoint (8 November 2026)

The saved-catalog-clock MCP cache correction now preserves the canonical SDK
service/session for unchanged credential generation and immutable authority when
only the catalog clock advances. An older authentication cannot replace a newer
cached context. The actual TLS/MCP regression passed both retained-TCP and
fresh-TCP use of the same session; this addresses the installed r2 diagnosis but
does not turn that failed installed attempt into a pass. A new matched runtime
and installed journey remain required.

Historical standalone base-ID recovery now depends on positive native proof of
automatic attachment retirement with the original installation, authority,
client and unexpired issuance, followed by a different live Explorer attachment.
The mutation owner rechecks the old ID immediately before issuing its replacement.
Explicit revocation overrides automatic retirement even across compaction-crash
recovery. Schema-2 tombstones are bounded to **2 KiB** and retain the needed
reason/provenance; schema-1 unknown history stays review-required. This is private
CLI/native control metadata, not evidence-MCP management access. Replacement does
not reuse the retired ID or widen the original reviewed grant. Earlier proof
formats and their original validation records retain their historical identity.

The final selected TypeScript modules passed **147 tests, one skipped, zero
failures** (**148 total**) in **8.379 s** runner time (**8.269 s** execution),
after compilation in **12.625 s**. Receipts are
`resume-adoption-provenance-modules-final.json` and
`resume-adoption-provenance-compile-final.json`; the test command selects
`serviceOnboarding`, `agentMigration`, `serviceHttp` and `serviceBootstrap`.
This is focused adoption/provenance coverage, not a replacement full extension
or installed-client gate.

Core/CLI scoped Clippy r4 passed in **18.862 s**, using two Cargo jobs, persistent
`artifacts/v190/p10-target`, incremental **on**, and Cargo timing evidence
`resume-scoped-clippy-r4-cargo-timing.html`. Its exact command was
`python scripts/dev-check.py -p airk-cli -p airk-core --features local-inference
--skip-fmt --target-dir artifacts/v190/p10-target --incremental on --cargo
C:/Users/tdotn/.cargo/bin/cargo.exe`; formatting had its separate recorded pass.
The receipt reports unchanged native inputs for this scoped run. Later native
test/harness edits retain their own identity and need their relevant checks.

Native preflight r2 completed **47 passed, one failed, 770 skipped** in
**56.749 s** test execution. The explicit source/storage retirement case now
passed (**32.440 s**, approximately **33 s**), using explicit fresh TLS while
preserving the same MCP session and without replaying a submitted exchange.
That corrects its earlier transport/idle-socket fixture failure. The remaining
reference-refresh case still aborted with stack overflow. Neither this partial
selection nor scoped Clippy is full workspace qualification. Evidence is
`resume-native-preflight-r2.json`, its JUnit and complete log; earlier failed
preflights remain retained.

Reference-refresh diagnosis measured **1,540,016 bytes** of additional journey
poll stack (approximately **1.47 MiB / 1.54 MB**) against the standard **2 MiB**
thread stack; the heap-allocated future itself was only **41,056 bytes**. The
initial phase-helper attempt failed and remains recorded. The minimal correction
runs native read polling in an independent native task while preserving the
canonical read path and assertions. Its complete bounded reference/control run
passed **11/11** tests in **133.458 s** execution (**144.289 s** runner), with
native/shared fixtures stable. Evidence is `resume-reference-and-control-r1.json`,
its log, JUnit and Cargo timing report. All temporary diagnostic instrumentation
has been removed. The corrected source is ready for freeze and fresh matched
runtime/platform qualification; this focused pass is not full workspace or
installed acceptance.

Actual Claude and Code connecting-process runners are prepared but have not run
in this resumed checkpoint. Neo r13 preparation adds mandatory exact clock/session
regression assertions before the existing debug/model, checkpoint and actual
installation preflight, then retains full workspace with all four restored
campaigns, doctests, optimized, signed and installed-editor gates. R13 is not
transferred or launched; public-source inspection and review precede its frozen
archive. Final optimized/full-workspace/doctest/platform
and release qualification remain incomplete. Version remains **1.8.14**; no
commit, push, PR or publication follows from this checkpoint.

### Completion-plan reconciliation (8 November 2026)

The user approved revision 2 of `docs/one-airk-completion-plan.md` and the
proposed local-completion choices. Windows user-CA enrollment is authorized for
one Code run with owned removal. The load rule permits only documented refusals,
zero failed/uncertain outcomes, and no minimum success count. Raw API clients and
held-inference checks are required. Missing thresholds, additional inference and
fault scenarios, two-user testing and universal/many-client migrations remain
recorded limitations; no Neo GUI Code host or second account has been supplied.
Commit, push, PR, merge and publication are not authorized. Version stays 1.8.14.

This is read-only reconciliation of existing receipts, not a new qualification
run. Unless otherwise stated, paths below are under
`artifacts/local/one-airk-1.8.14/`. Historical runtime r25 retains compiled native
source `9bef1b79cdc7de45dc9abbb2fec0ffde2d6a3fb00181a620a1b40b5c570ac54b`
(1,115 inputs), install-manifest digest
`830bee5ef28e2d7761ced8c8f1aea04ed25e8d5bb76aa696ff105808e51342d9`.
Later edits are not qualified by those results.

| Retained result | Receipt and limits |
| --- | --- |
| Formatting, workspace Clippy, lightweight compile and default build passed | `resume-workspace-format-r25.json`, `resume-workspace-clippy-r25.json` (74.436 s), `resume-lightweight-r25.json` (53.904 s), `resume-workspace-build-r25.json` (47.216 s). Persistent P10 target, incremental on; Cargo timing reports retain each compile identity. |
| Workspace tests interrupted | `resume-workspace-tests-r25.log` ends at 961/3,242; no completed workspace result or doctest pass. |
| Targeted native corrections passed | `resume-reference-and-control-r1.json`: 11/11, including reference refresh and private HTTP controls; earlier partial/failing preflights stay failed. |
| Debug runtime and packages passed their selected gates | `resume-debug-runtime-r25.json` (150.608 s), `resume-debug-clients-r25.json`, `resume-debug-native-package-r25.json`, `resume-debug-vsix-package-r25.json`. Debug model/import, HTTPS clients, archive and VSIX checks only. |
| Enabled-native extension suite passed | `resume-extension-full-r25.json` and log: 1,088 passed, 3 skipped, 1,091 total; before the latest controller correction. |
| Actual Claude 2.1.118 passed | `resume-actual-claude-r25-r1.json` (50.748 s) and `resume-actual-claude-r25-result-r1.json`: scoped reads, reconnect, additive trust, wrong issuer; sources preserved and native stop complete. Windows debug only; Claude projection omits MCP `isError`. |
| Actual Code 1.145.2 failed before tool discovery | `resume-actual-code-r25-collected-r1.json` (106.368 s): extra-CA path only; OS trust was never enrolled. Missing `multi/native-stop.json` leaves cleanup unqualified. |
| Installed journey partial | `resume-installed-editor-r25.json` (249.795 s): `same` passed, `different` timed out after index creation; later scenarios unrun. |
| Client load partial | `resume-client-load-r25-r1.json` (378.797 s), `resume-client-load-r25-result-r1.json`: 1/10 clients passed; 64 stopped on HTTP 503 refusals; later phases unrun. |
| Separate admission diagnostic completed | `resume-http-admission-r25-result-r1.json`: 16 successes, 36 `authentication_busy`, 12 `native_validation_busy`; all sessions closed, native stop complete, sources preserved. No fairness proof or semantic-campaign rerun. |
| Neo r13 failed at uninstall | `artifacts/local/one-airk-1814/neo-r13-complete-20261108/evidence/batch-receipt.json`: 753.436 s overall. Python, formatting, workspace Clippy, lightweight compile, 51/51 native regressions, debug model/runtime and checkpoint passed. Install/HTTPS preflight failed (271.646 s), including ESRCH uninstall; required Cargo clean passed (0.785 s). No later full/platform qualification. |
| Linux r3 canceled during archive preparation | Retained original canceled campaign remains unqualified; no static build or container success claimed. |

AIRK MCP discovery exposes no AIRK tools in this session; `.codex/config.toml`
still disables the server. Local source verification is the fallback. The
session's restricted shell runs as `tyai\\codexsandboxoffline`; protected TLS/ACL
and native fixtures must use the contributor execution context, per the frozen
plan, rather than changing fixture ACLs.

### Completion workstream checks (8 November 2026, in progress)

Primary-owned scoped native Clippy passed in **33.403 s**:
`python scripts/dev-check.py -p airk-cli --features local-inference --skip-fmt
--target-dir artifacts/v190/p10-target --incremental on
--cargo C:/Users/tdotn/.cargo/bin/cargo.exe`. Its underlying command checked all
CLI targets with no default features and `local-inference`, warnings denied,
`--locked --timings`. Native inputs were unchanged; concurrent Python/harness
repairs mean the script inventory changed, so this is a scoped native result,
not an integrated source freeze. Receipt and retained timings:
`artifacts/local/one-airk-1.8.14/completion-scoped-clippy-r1.json` and
`completion-scoped-clippy-r1-cargo-timing.html`.

The first coalesced native T0 selected **63 tests: 62 passed, one failed,
720 excluded** in **11.094 s** execution (**57.246 s** runner). The held-inference
test failed before holding compute because its fixture did not open the model;
the new fixture and its missing complete reply assertion are being reviewed.
Mac retirement pure regressions, actual Windows owned-process/uninstall cases
and gateway cases passed in that selection. Command:
`cargo nextest run --locked -p airk-cli --no-default-features --features
local-inference --profile ci --lib --timings --no-fail-fast --success-output
immediate -E 'test(upgrade_stop) | test(retirement) | test(http_gateway::tests) |
test(held_inference_keeps_native_status_responsive_and_checkpoint_charged)'`.
The persistent target was `artifacts/v190/p10-target`, incremental on. Complete
receipt, log, JUnit and timings retain the failed run under
`artifacts/local/one-airk-1.8.14/completion-native-t0-r1*`. Native inputs were
unchanged; this is T0 failure evidence, not a qualifying combined pass.

Code harness T0 passed **7 Node and 5 mocked Python tests**, no skips, with
receipt `artifacts/local/one-airk-1.8.14/completion-e-t0.json`. It now preserves
the exact one-dispatch native Stop receipt when the injected client fails, and
prepares exact-issuer user trust install/removal in outer cleanup. These tests
performed no actual OS trust enrollment or Code host run. Independent review of
the helper, actual runner transformation, trust ownership and evidence passed;
the platform run still waits for all workstreams and the combined freeze.

Independent review is still open for evolving extension/HTTP corrections and
the held-call fixture. Container effective-context admission is receiving an
additional neighbor-exclusion regression after review of Docker parent
negations. No T2/T3 campaign or new runtime build has started.

The corrected held-call T0 passed **1/1 selected**, 782 excluded, **0.427 s**
execution (**28.294 s** runner), in `completion-held-native-t0-r2.json` with
complete log, JUnit and retained Cargo timings. The real native IPC handshake
consumes the grant before model Open. While compute is held, an actual
CA-validated HTTPS status request observes the same native provider charges;
checkpoint/Stop remain pending until compute and delivery settle. The test also
rejects the old worker grant after a new owner/service generation. This is a
native owner/HTTPS component test using the test engine, not a held real ORT FFI
callback or accepted multi-batch project-worker proof. The latter preflight is
being prepared separately for the matched debug candidate.

The immediately preceding scoped Clippy r2 failed solely on an unnecessary
`mut` introduced by the test's owner wrapper (**11.455 s**); its receipt and
timings remain retained. Removing that binding's mutability produced scoped
Clippy r3 success (**17.994 s**, including formatting check), same CLI
`local-inference` scope, persistent P10 target and incremental on. Native inputs
were unchanged for both checks. Evidence is `completion-scoped-clippy-r2*` and
`completion-scoped-clippy-r3*` under the same local evidence base.

Extension workstream A passed **173/174 selected**, one native-gated skip, and
the mocked T1 suite passed **1,084/1,105**, 21 gated skips, no failures
(**23.166 s** wrapper). `completion-a-t0.json`, `completion-a-t1.json` and
`completion-a-review.json` retain exact commands, source hashes and the full
ownership table. Independent source/evidence review has no A blocker. Tests
cover real scope/set orchestration across forward catalog clocks and the actual
Indexing keyed-child remount path; native/installed acceptance is still pending.

Container workstream C's final effective-context contracts passed **23/24**,
one expected Windows symlink-privilege skip, **1.299 s**, in
`completion-c-container-r3.json`; static ONNX contracts passed **42/42**,
**2.414 s**, in `completion-c-t0-r2.json`. Source review confirms the exact-file
manifest, original owning trees, parent-neighbor exclusions and unchanged
private-state exclusions. No Docker run or extraction speed claim follows.

Cheap Windows installed-journey prerequisite review found a further harness
gap before any candidate build: the historical positive legacy fixture assumes
the old launcher still exists, while the implemented schema-2 migration must
retire it. Its producer/verifier are being adapted to retain original provenance
as non-executable evidence and prove the original executable location absent.
No old fixture is relabeled as a matching r26 fixture.

### Frozen completion validation begins (8 November 2026)

All repair streams and the additional required installed/native harnesses passed
their cheap checks. The independent reviewer `/root/reviewer`, who wrote no
implementation changes, reviewed the complete combined diff, ownership table,
retained failures and final T0/T1 evidence; the primary separately reviewed the
combined source. No blocking findings remain at this gate. Source versions,
lockfiles and catalog remain **1.8.14**, and AGENTS/CLAUDE are byte-identical.

Final integrated formatting/workspace-all-feature Clippy passed in **10.374 s**
with unchanged native, script and shared fixture inputs. Command:
`python scripts/dev-check.py --workspace --all-features --target-dir
artifacts/v190/p10-target --incremental on --cargo
C:/Users/tdotn/.cargo/bin/cargo.exe`; receipt
`completion-integrated-clippy-r3.json` and retained Cargo timing HTML. Earlier
integrated Clippy r1 (**26.248 s**) caught a native-only test-hook variant leaking
into the legacy helper test build; the shared owner-context cfg now excludes it
there without a lint suppression. Integrated r2 passed **17.621 s**; r3 checks
the subsequent MCP-only assertion too. Scoped retirement/held-checkpoint Clippy
r4 had caught an unrelated legacy split-test feature gate (**23.935 s**); that
module retains its original `embedding-worker` gate. R5 stopped at formatting
(**7.155 s**); formatting and scoped r6 then passed (**24.734 s**). These are
cheap compilation corrections, not failed runtime campaigns.

Final native fixture T0 passed **3/3**, 787 excluded, **0.504 s** execution /
**48.607 s** runner, with source unchanged (`completion-checkpoint-native-t0-r3*`).
It covers real held-owner HTTPS/status, canonical semantic multi-batch planning
and canonical modern/legacy image verification. Primary source review then
corrected the verification test's Mac parent/helper topology before platform
runs; final all-feature Clippy covers that test setup. The accepted multi-batch
worker test remains explicitly ignored until supplied the matched r26 image;
it is not counted as executed by the three cheap cases.

Canonical MCP full-capabilities T0 passed **1/1**, 163 excluded, **0.033 s** /
**48.421 s** runner: the complete body is **30,260 bytes**, preserving all
operation schemas within the unchanged **65,536-byte** installed body guard.
`completion-capabilities-t0-r1*` retains log, JUnit and timing evidence. Scoped
MCP Clippy passed **28.481 s**. A pre-index usage-recording warning remains in
the T0 log; retrieval succeeded, and usage history completeness is not claimed.

The final mocked extension suite passed **1,089**, with **21 gated skips**,
**1,110 total**, in **22.654 s** wrapper (`completion-installed-t1-final.json`).
Its selected installed-helper contracts passed **43/43**. Script-side real
component/cross-helper contracts passed **62 Node tests** plus **18 Python
tests**, and the final generated-helper seal rerun passed **5/5**, unchanged
(`completion-editor-final-t0.json`). The full Python T1 suite passed **854**
tests with **13 skips**, **74.871 s** execution / **75.633 s** wrapper
(`completion-integrated-python-r1.json`), source/fixture inventories unchanged.
Whole-diff whitespace passed. None of these mocked checks is installed acceptance.

The authoritative contributor freeze is
`artifacts/local/one-airk-1.8.14/completion-freeze-r26-final.json`:
canonical native source
`ac2620f07a33fa096349197001edaefa5370caacb18310270181b062fbda42ef`
/**1,116 inputs**. Separate content inventories are native **1,104**,
extension **388** (including shipped parser files and compiled output), harness
**249** (including actual-client runners/installed helpers), fixtures **110**.
The independent reviewer rehashed all **1,851 group entries**, zero mismatches.
Native model/static inputs and each generated artifact additionally retain their
own builder/transport seals. The identical sandbox preview is not authoritative.

**Declared first T2:**
`python scripts/build-service-runtime.py --profile debug --destination
artifacts/local/one-airk-1.8.14/windows-debug-r26/runtime`, through the existing
contributor receipt owner as `completion-debug-runtime-r26`. Scope: Windows x64,
single modern main with embedded static MiniLM, persistent P10 target,
incremental on. Expected: verified build/model/import receipts and an original
r26 source seal matching the freeze. Stop on nonzero exit, mismatched frozen
inputs, failed artifact/model/static proof or uncertain process cleanup; do not
package or start later campaigns after failure. No T2 outcome is recorded yet.

**Original r26 debug build passed** in **63.785 s**, source/fixture/script
inputs unchanged and the full freeze verified afterward. Receipt:
`completion-debug-runtime-r26.json`; Cargo timings retained alongside it.
Install-manifest SHA256:
`2069c2d6e3acc48bba07a808fae4b160c46def14b76b03cb12a768357cf2138a`.
The embedded ORT 1.22.1 CPU smoke reports **384 dimensions**, repeat equality,
profile `minilm-l6-v2-u8-v1`, pipeline
`734e4a977c38b719e77143aa25bf053eb6099061ca6a8313bb4bb588dafbc4e5`.
This is a new original debug build, not reused r25 or optimized qualification.

**Declared next T2 component preflight:** matched r26 image and the same frozen
source; set `AIRK_TEST_NATIVE_SOURCE_SHA256` to the above canonical source,
and use `AIRK_TEST_BINARY` from the verified r26 runtime. Command:
`cargo nextest run --locked -p airk-cli --lib --no-default-features --features
local-inference --profile ci --run-ignored only --timings --success-output
immediate -E
'test(accepted_vectors_finish_later_batch_before_native_checkpoint_seals)'`.
Expected: exactly one actual accepted multi-batch worker/provider case passes,
source bytes remain unchanged, and complete held-work/receipt/barrier evidence
is retained. Persistent P10 target, incremental on, contributor fixture base.
Stop at any failure, source mismatch, missing/zero selection or uncertain owned
cleanup; retain the allocated fixture and all evidence before correction.

**First actual checkpoint preflight failed** before worker/fixture allocation:
**1 selected, 0 passed, 789 excluded**, **0.028 s** execution /
**24.085 s** runner. `completion-modern-checkpoint-r26-r1.json`, complete log,
JUnit and Cargo timing report retain the result. All native/script/extension
inputs were unchanged. No later T2 stage ran.

The wrong assumption was in the new test harness: it required
`airk-install.json.schema == 2`. The canonical installer manifest remains schema
**1** for both kinds of runtime; **airk-runtime.json** identifies the modern
single executable with schema **2**, while the build receipt has its own schema.
The earlier cheap input test verified executable-pair kinds but never exercised
this initial receipt parser. The correction is first being reproduced at T0 by
a shared test-only receipt validator using canonical typed manifests, then
audited across source/build/pair/seal checks. This is the first T2 failure for
this component in the completion run; a second failure will stop further runs
under the plan's two-strike rule.

Only the two external `cfg(test)` modules may change for this correction.
The original r26 binary, its complete artifact seals, compiled source
`ac2620f0…` and manifest `2069c2d6…` remain untouched. Reuse must prove every
other original frozen input unchanged and retain the distinct current test
source identity; no rebuilt or relabeled runtime is claimed. The primary
prepares `completion-test-only-gate.py` for this explicit two-file delta, with
independent review before the second actual preflight.

The missing cheap regression reproduced the metadata error without any runtime
process: `completion-checkpoint-schema-red-r1*`, **1 selected / 0 passed**,
**0.045 s** execution / **24.390 s** runner. The shared test-only validator now
treats typed install schema **1**, executable metadata schema **2** and build
schema **1** independently, with pinned manifest/source, complete bounded
inventory/topology/seals and canonical runtime/build equality checks. Its
crossed-input matrix and a read-only invocation against the actual sealed r26
both passed: **2/2**, 790 excluded, **0.673 s** / **33.091 s** in
`completion-checkpoint-schema-t0-r2*`. No worker or service started in these T0
cases. Scoped CLI Clippy passed **17.921 s** with timing evidence in
`completion-checkpoint-schema-clippy-r1*`.

The reuse proof verifies all **811** r26 file seals before/after, every original
frozen entry unchanged except the two approved external cfg(test) modules, and
the unchanged parent test gate. Original compiled source remains `ac2620f0…`
/**1,116**; current test source is
`821c1c969f46815270e99410e4ecdaf7ca958971ba4640d01263f14b813e24f0`
/**1,116**. The original build is reused, never relabeled as freshly built.
The independent reviewer rehashed both correction receipts (1,261 entries
each), zero mismatches, reviewed the complete correction and reuse owner, and
cleared exactly one second component attempt.

**Declared second T2:** same accepted-Vectors Nextest command and immutable r26
manifest/source as the first attempt, through `completion-test-only-gate.py`
and receipt `completion-modern-checkpoint-r26-r2`. Original source and manifest
pins are explicit environment inputs; current test identity is retained
separately. Expected exactly one passing actual worker/provider preflight, full
source preservation and native completion evidence. Stop on failure, changed
inputs/artifacts, zero selection or incomplete ownership/cleanup; no third
attempt without the user. No second-attempt result is claimed yet.

### Mandatory two-strike stop (8 November 2026)

The second actual checkpoint preflight failed: **1 selected / 0 passed**,
791 excluded, **3.177 s** execution / **4.403 s** runner. Evidence:
`completion-modern-checkpoint-r26-r2.json`, complete log, JUnit, timing report
and `completion-modern-checkpoint-r26-r2-reuse-proof.json`. All consumed inputs
and all **811** original r26 file seals remain unchanged before/after. The
fixture `airk-guarded-workers-MNXo1l` remains under the protected fixture base.

Read-only primary/independent audits traced the error to the shared fixture's
direct `Draft::InstallModel.prepare()` call. Current production correctly
requires its native administrative input owner; the inherited fixture assumed
local source-bearing preparation was still allowed. This affects the shared
modern and legacy setup paths. The test failed after worker inspection, before
model configuration or any held-batch/checkpoint assertion. No second-failure
fix, test, build, restart, cleanup or third attempt followed. A read-only exact
fixture process observation found zero matching airk.exe processes; cleanup
remains unqualified, with its observation receipt retained separately.

The [stop/root-cause note](one-airk-completion-stop-2026-11-08.md) records the
wrong assumptions, missing cheap fixture-preparation test, affected callers,
preserved evidence and a bounded native-owner correction proposal. Local and
release completion have **not** been reached. All later Windows, Neo, Docker,
final workspace/optimized and publication gates remain unrun. User authorization
for a new cycle is required by the approved two-strike rule.

## Authorized native fixture preparation repair — 8 November 2026

The user authorized continuing the bounded proposal in the stop note. The shared
modern/historical fixture now captures the original model manifest through its
native administrative capability with `PrepareInputFile` and the existing
16,384-byte bound. Complete native refusals survive decoding and are printed in
failure diagnostics; canonical returned parent/hash, actual inspection settings
and incarnation, and engine identity remain authoritative. No production input
changed. Separate setup and held/checkpoint phases retain failure evidence.

T0 command: `cargo nextest run --locked -p airk-cli --lib --no-default-features
--features local-inference --profile ci --timings --success-output immediate -E
'test(fixture_model_preparation_) | test(qualification_fixtures_require_native_source_preparation) | test(source_bearing_drafts_cannot_bypass_the_native_preparation_owner)'`.
The sandbox attempt (`completion-model-preparation-t0-r1.json`) failed at linking
with LNK1104 on a protected fixture-base linker temporary file; no tests ran.
The contributor-identity run (`completion-model-preparation-t0-r2.json`) passed
**4/4**, 792 excluded, **0.056 s** execution / **17.387 s** wrapper. It checks exact
request/count, original spelling/bound, distinct native pin, complete refusal
body/type/diagnostics, transport errors and malformed pins, plus direct-source
preparation detection and production refusal. No ACL or assertion was weakened.

Final T1 command: `python scripts/dev-check.py -p airk-cli --features
local-inference --target-dir artifacts/v190/p10-target`. Formatting and scoped
all-target Clippy passed in **17.526 s** wrapper / **10.04 s** Cargo, incremental
**on**, persistent P10 target. Receipt `completion-model-preparation-clippy-final.json`
and `completion-model-preparation-clippy-final-cargo-timing.html` retain timing
evidence. Initial scoped Clippy passed **19.917 s** before the final test-data
correction. Repository checks passed **2,083 public files**; whole-diff whitespace
passed. Independent combined review found no blocking issues; primary reviewed
the combined two-module repair and canonical preparation/refusal handlers.

Freeze proof: `completion-model-preparation-freeze-r1-reuse-proof.json` verifies
all **811** original r26 artifact seals and all original frozen entries except
the two external cfg(test) modules, before and after. Original compiled source
remains `ac2620f07a33fa096349197001edaefa5370caacb18310270181b062fbda42ef`
/**1,116**; current test source is
`9335cf1dbff5f128719382049e7db7e146df9d2db98a56bbe2e5d15eae21a735`
/**1,116**. Native manifest remains `2069c2d6…`. Extension, harness and fixtures
retain the original freeze identities. A sandbox read of the protected payload
was denied; contributor-identity verification passed. The original r26 binary
is reused, not rebuilt or relabeled.

**Declared setup-only T2:** through unchanged `completion-test-only-gate.py`,
runtime `windows-debug-r26/runtime`, freeze `completion-freeze-r26-final.json`,
receipt `completion-model-setup-r26-r1`; command `cargo nextest run --locked -p
airk-cli --lib --no-default-features --features local-inference --profile ci
--run-ignored only --timings --success-output immediate -E
'test(modern_fixture_prepares_model_and_publishes_before_checkpoint)'`.
Expected exactly one passing test proving actual worker/model setup, both scoped
semantic reads and unchanged sources. Stop on any failure, changed input/seal,
zero selected tests or uncertain ownership. Do not start the held checkpoint
case until setup succeeds. No actual result is claimed by this declaration.

Setup-only T2 passed **1/1**, 795 excluded, **13.709 s** execution /
**14.907 s** wrapper (`completion-model-setup-r26-r1.json`, log, JUnit, timings
and reuse proof). Both actual workers prepared/configured/published their model
and returned scoped semantic vector hits; source bytes were unchanged. Fixture
`airk-guarded-workers-6I5qP5` is retained. Every input and all **811** seals passed
before/after verification; no native cleanup receipt is inferred from test exit.

**Declared held/checkpoint T2:** same unchanged native/extension/harness/fixture
identities and reuse owner, receipt `completion-modern-checkpoint-r26-r3`;
command `cargo nextest run --locked -p airk-cli --lib --no-default-features
--features local-inference --profile ci --run-ignored only --timings
--success-output immediate -E
'test(accepted_vectors_finish_later_batch_before_native_checkpoint_seals)'`.
Expected exactly one passing actual held semantic-work case, later embedding
batches, exact accepted-job success, native checkpoint/provider completion and
source preservation. Stop on failure, zero selection, changed input/artifact or
incomplete completion evidence. This is the first held/checkpoint attempt in
the newly authorized repair cycle, after the separate setup preflight passed.

Held/checkpoint T2 passed **1/1**, 795 excluded, **17.818 s** execution /
**18.967 s** wrapper (`completion-modern-checkpoint-r26-r3.json`, complete log,
JUnit, timings and reuse proof). The exact accepted Vectors job completed **66
actual embedding calls** before the native checkpoint sealed; pending/held work,
later batches, exact saved-job success, sibling worker seals, provider completion
and unchanged source assertions passed. Fixture `airk-guarded-workers-c2UYkw` is
retained. All consumed inputs and original **811** r26 seals remain unchanged
before/after. Original compiled source/build identity is retained separately from
current test source; no native rebuild occurred. This resolves the fixture
preparation blocker for these selected Windows component cases. It does not
qualify historical runtime execution, native cleanup, installed-editor journeys,
full workspace/doctests, Neo, Docker or release/publication gates.

## Resumed r26 packaging and Windows acceptance — 8 November 2026

The user authorized continuing the local qualification ladder, beginning with
r26 VSIX packaging. Read-only review found that VSIX packaging first needs a
native archive. Protected-payload verification passed through
`completion-packaging-preflight-r26-r1-reuse-proof.json`: all 811 original seals
and frozen inputs outside the two approved external test modules passed before
and after checks. Original compiled and current test identities remain separate.

Declared packaging commands, in order, through the existing
`completion-test-only-gate.py --runtime artifacts/local/one-airk-1.8.14/windows-debug-r26/runtime --freeze artifacts/local/one-airk-1.8.14/completion-freeze-r26-final.json`:

1. `completion-native-package-r26 python artifacts/local/one-airk-1.8.14/completion-package-r26.py`.
   The artifact-local adapter calls canonical native packaging with the proven
   original compiled identity; it does not rebuild or edit the runtime.
   Output: `windows-debug-native-r26`.
2. `completion-vsix-package-r26 python scripts/package-extension.py --target x86_64-pc-windows-msvc --native-dir artifacts/local/one-airk-1.8.14/windows-debug-native-r26/publish --output artifacts/local/one-airk-1.8.14/windows-debug-vsix-r26`.

Expected: canonical archive/model/legal and VSIX inventory checks pass, original
runtime and consumed inputs remain unchanged. Stop at first failure, changed
input/seal or uncertain ownership. No packaging success is claimed yet. These
stages do not compile Cargo; persistent target remains P10, incremental on in
the receipt owner. Separate review precedes execution.

Independent review cleared the wrapper and sequence. Native packaging passed
in **17.471 s**, receipt `completion-native-package-r26.json`, complete log and
reuse proof. Canonical archive/model/legal checks passed; all consumed inputs
and 811 original runtime seals passed before/after verification. No rebuild
occurred. VSIX packaging passed in **20.498 s**, receipt
`completion-vsix-package-r26.json`, complete log and reuse proof; canonical
VSIX checks and unchanged-input/original-seal verification passed.
No installed acceptance is inferred.

Declared Windows acceptance first stage: through the same reviewed reuse owner,
`completion-enabled-extension-r26 npm test --prefix packages/vscode`.
Expected: enabled-native suite passes with nonzero actual native test selection,
and compilation leaves all frozen production outputs and installed helpers
unchanged. Stop at any failure, source/output/seal mismatch or incomplete owned
cleanup. No later Windows campaign starts before this stage passes.

The first extension invocation did not launch npm: Python `CreateProcess`
returned `WinError 2` for bare `npm` on Windows. Zero tests ran; this is not a
suite result. The empty log and `completion-enabled-extension-r26-reuse-proof.json`
retain the launch failure and passing after-verification. Corrected command uses
the Windows executable shim, `completion-enabled-extension-r26-r2 npm.cmd test
--prefix packages/vscode`, under the same reuse owner. No inputs changed.

Enabled-native extension acceptance passed **1,108 / 1,111 tests**, **3 skipped**,
**0 failed**, **150.268 s** execution / **162.923 s** wrapper. Receipt:
`completion-enabled-extension-r26-r2.json`, complete log and reuse proof.
Native cases executed; all frozen extension outputs/helpers, consumed groups
and original 811 runtime seals remained unchanged before/after.

Declared next stage: `completion-editor-fixture-r26 pwsh -NoProfile -File
artifacts/local/one-airk-1.8.14/completion-editor-fixture-r26.ps1` under the same
reuse owner. The independently reviewed wrapper creates only a fresh short
owner-only `artifacts/ef26-<8hex>` directory, verifies its protected ACL and calls
the canonical `prepare-native-editor-legacy.py`. Candidate manifest is
`2069c2d6…`; authentic schema-1 predecessor is
`artifacts/local/1.8.12-explorer-20261107/runtime`, manifest `be73a42f…`.
Expected: actual predecessor evidence, upgrade/retirement/source preservation
and exact native Stop pass before a matched fixture receipt is published.
Stop on any failure, incomplete Stop or frozen-input/seal mismatch; retain every
fixture and do not start installed editor scenarios after a producer failure.
Existing final T0/T1 producer evidence was reviewed; no Cargo build is needed.

**Legacy producer failed**, **30.091 s**, receipt
`completion-editor-fixture-r26.json`, complete log and reuse proof. All frozen
inputs and all 811 original runtime seals remained unchanged. Failure occurred
at predecessor registration, before indexing, candidate activation or editor
launch: `invalid_registry_request`. Preserve
`artifacts/ef26-9af3cdfd/legacy` and its complete command/failure receipts.
The producer's finally path completed its exact request-bound native Stop;
`producer-native-stop.json` records `phase: complete` and matching request,
and `producer-post-stop-health.json` records unavailable health, exit 1.
No uninstall or broader cleanup qualification is inferred.

Primary and independent source review traced the wrong assumption to choosing
the old raw registration path solely from runtime schema 1. Genuine 1.8.12
already requires catalog setup; the proven sibling upgrade script uses its
reviewed catalog fixture instead. The producer mocks replaced registration and
missed this distinction. Merely switching registration is insufficient: the
original launcher scope and consumer also assume raw `--service-project` and
require sealed imported-legacy authority. A catalog-session fixture would
exercise a different migration lineage. No correction, rerun, restart or later
ladder stage has followed. Resolve the required historical migration lineage
before another expensive producer attempt; keep all thresholds intact.

## Early rebuild option — 8 November 2026, implementation in progress

The user retained automatic rebuilding but explicitly reduced its scope: offer an
early rebuild choice rather than enumerate permutations of upgrade failures.
No actual incompatible-installation receipt was supplied. The native entry being implemented is
`runtime activate --rebuild-indexes --expected-active <reviewed-predecessor>`.
After that choice, native owners perform the fresh rebuild automatically. Old
state remains preserved; this is distinct from successful migration and requires
fresh client enrollment. The [recovery contract](../upgrade-rebuild-fallback.md)
records the supported initial lane and pre-mutation refusals. This section does
not claim operational or installed acceptance.

The authentic schema-8 audit corrected an earlier incomplete inference. Candidate
startup runs the ordinary registry migrations before catalog admission. The
new regression uses the original 1.5.12 DDL from clean revision `d3b80ddf…`, with
all fourteen SQL literals independently reviewed, rather than relabeling a modern
registry. Saved reads/checkpoint capture preserve bytes, canonical startup reaches
schema 11 with the original namespace/pause/watch settings, and native catalog
admission/migration/restore succeeds. Original publication, imported authority,
process ownership and installed producer acceptance remain separate.

Scoped checks collected so far under `artifacts/local/one-airk-1.8.14/`:

- `completion-recovery-format-r1.json`: formatting passed, **7.116 s**.
- `completion-recovery-clippy-r1.json`: store/CLI lightweight scoped Clippy
  failed, **32.978 s**, on a test's unsupported SQLite `u64` read and a complex
  settings-capture return type. Both were corrected without changing assertions.
- `completion-recovery-clippy-r2.json`: the same scope failed, **61.159 s**, on
  the existing `live_https_status` test helper being unused without inference
  features. This is not a passing lightweight Clippy result; the documented
  `local-inference` component configuration remains to be checked.
- `completion-recovery-store-t0-r1.json`: **8 selected / 8 passed**, **839
  excluded**, **0.519 s** test execution / **69.049 s** compile/runner. Selection
  covers five settings-capture cases, the typed source-format diagnostic,
  authentic schema-8 bridge and existing INT8 pause regression. Store sources
  were held during this development run; other crate edits continued, so the
  wrapper's global `native_inputs_unchanged=false` is retained. This is T0
  development evidence, not a frozen qualification receipt.

All Cargo invocations used the existing `artifacts/v190/p10-target`, incremental
compilation **on**; exact commands, logs, retained Cargo timings and test JUnit
are beside their receipts. The original r26 runtime/VSIX were not rebuilt or
relabelled. New production work requires a separately identified candidate.
No T2/T3, OS trust mutation, commit, push, PR or publication has occurred in this
follow-up. Later checks must be appended when collected.

Further development checks (same artifact directory and Cargo settings):

- `completion-recovery-clippy-r3.json` stopped at formatting; r4 exposed two
  local lint defects, corrected before `completion-recovery-clippy-r5.json`
  passed the CLI `local-inference` all-target scope in **28.709 s**.
- `completion-recovery-native-t0-r1.json`: **33 selected**, **32 passed / 1
  failed**, **1,966 excluded**, **11.016 s** tests / **128.589 s** wrapper.
  The real Store-backed interruption regression passed all fourteen pause/phase
  subcases. The MCP parity fixture alone requested an unsupported page budget;
  its request was corrected to the existing 48 KiB maximum without weakening
  production limits. `completion-recovery-mcp-t0-r2.json`: **2 selected / 2
  passed**, **164 excluded**, **0.935 s** tests / **55.963 s** wrapper. Actual
  CLI process and external HTTPS recovery acceptance remain pending.
- Extension compilation initially found a missing exact input in the Uninstall
  bundle allowlist. Adding the inert rebuild contract module fixed it;
  `rebuild-extension-compile-r2.json` passed in **12.862 s**.
  `rebuild-extension-t0-r1.json`: **136 selected**, **135 passed / 1 skip**,
  **5.877 s** wrapper. These are mocked host tests, not installed-editor evidence.
- `standalone-packaging-t0-r1.json`: **77/77 passed**, **11.160 s** tests /
  **11.415 s** wrapper. `standalone-release-site-t0-r1.json`: **51/51 passed**,
  **9.322 s** tests / **9.617 s** wrapper. Signing/packaging fixtures use mocked
  platform tools; no actual installer was compiled, signed or installed.
- `standalone-clippy-r1.json` failed on one `int_plus_one` lint in additive CA
  budget arithmetic, **24.100 s**, with timings retained. Correction and final
  guided-CLI validation remain in progress.

The requested standalone distribution is implemented in source: Windows Inno
EXE using the existing Azure signing profile for installer and uninstaller;
macOS DMGs using the existing Developer ID signing/notarization lifecycle; and
an unsigned offline Linux tar installer. All delegate to native CLI setup and
exclude VSIX installation. The existing GitHub Pages generator links only
actually published installer/manifest pairs. The release workflow now requests
GitHub attestations for final Linux installer and GitHub-hosted VSIX bytes;
Marketplace signature handling remains separate. None of these publication or
attestation steps has run. Independent review identified and corrected
pre-execution Mac executable verification, immutable per-attempt enrollment
records that retain explicit same-connection retries, and deferred native
uninstall reporting so the Windows installer preserves its recovery entry.

`standalone-clippy-r2.json` passed the CLI `local-inference` all-target scope in
**24.768 s**. `rebuild-extension-mocked-t1-r1.json` completed the full compiled
mocked extension suite: **1,116 selected**, **1,095 passed / 21 skipped**, **8.979 s**
tests / **9.052 s** wrapper; no native runtime was selected. Native CLI T0
`standalone-native-t0-r1.json` selected **17**, with **15 passed / 2 failed** and
**1,679 excluded**, **0.933 s** tests / **48.612 s** wrapper. One assertion used
raw Windows paths although the prompt deliberately escapes them; it now checks
the complete rendered scope line and exact authorized working directory. The
other fixture wrote terminal job states without their required canonical
outcomes; it now retains valid outcomes for the source-history trigger. Neither
correction changes production behavior or weakens an assertion. The corrected
`standalone-native-t0-r2.json` selection passed **17/17**, **1,679 excluded**,
**0.953 s** tests / **26.469 s** wrapper; consumed native/script inputs stayed
unchanged. The clean whole Python rerun
`completion-python-t1-r1.json` passed: **896 selected**, **883 passed / 13 skipped**,
**77.099 s** tests / **77.768 s** wrapper, with native and script inputs unchanged.
It ran under the contributor identity and command-scoped Git ownership setting;
this supersedes the earlier missing clean whole-suite rerun without deleting its
failed sandbox evidence. `completion-format-final-r1.json` also passed,
**7.117 s**.

GitHub's published Windows 2025 image inventory was checked on 8 November and
lists [Inno Setup 6.7.1](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-Readme.md).
The packager still checks and records the actual compiler version/hash; this
source-level prerequisite check does not substitute for real compilation.

### r27 manual-test candidate freeze — 8 November 2026

The user asked for a fresh VSIX for manual testing. Implementation and independent
component reviews are complete; the primary agent reviewed the combined
CLI/recovery, packaging, release assembly and site changes. The reviewed T0/T1
gates passed, including `completion-workspace-clippy-r27-r1.json` (**38.773 s**)
and `completion-repo-r27-r1.json` (**2.992 s**), version synchronization and
whitespace checks. Full release qualification remains pending.

`completion-freeze-r27.json` records native **4ffad538… / 1,124 inputs**, compiled
native source **498bc344… / 1,136 inputs**, extension **1f8d7ee7… / 391**, harness
**8ab73b20… / 269** and fixtures **bf8399bd… / 110**. The extended freeze includes
the new installer templates, site and release workflow without altering the
retained r26 freeze owner. No consumed inputs may change during the run.

Declared T2 command:
`python scripts/build-service-runtime.py --destination artifacts/local/one-airk-1.8.14/windows-debug-r27/runtime --profile debug`.
Expected: a new sealed schema-2 runtime from exactly the frozen current source,
including successful native embedded-model/import/package checks. Stop on the
first failure or changed consumed identity; preserve failed staging/evidence.
Only after success, package native/VSIX through their canonical owners, verify
the freeze and report the new local manual-test artifact. r26 remains unchanged.

`completion-runtime-r27-r1.json` passed in **95.971 s**, including native embedded
model/import/self-test checks and the release notice inventory (**425 packages**).
Native and script inputs stayed unchanged; Cargo timings are retained. Next
declared packaging commands, on the same freeze:

- `python scripts/package-native.py --target x86_64-pc-windows-msvc --runtime-dir artifacts/local/one-airk-1.8.14/windows-debug-r27/runtime --output artifacts/local/one-airk-1.8.14/windows-debug-native-r27`
- `python scripts/package-extension.py --target x86_64-pc-windows-msvc --native-dir artifacts/local/one-airk-1.8.14/windows-debug-native-r27/publish --output artifacts/local/one-airk-1.8.14/windows-debug-vsix-r27`

Expected: verified native ZIP and platform-specific VSIX with matching sealed
runtime and current compiled extension. Stop on the first failure or changed
consumed input. This unsigned local debug candidate is for manual testing and
does not qualify release signing or installed-platform acceptance.

`completion-native-package-r27-r1.json` passed in **17.473 s**, with unchanged
consumed inputs. Before VSIX packaging, independent final review found that
historical completed rebuild receipts could block a later extension upgrade and
unrelated runtime retirement. The original r27 runtime/package remain preserved;
no r27 VSIX was built. Source freeze r27 is explicitly closed for correction,
not reused as unchanged evidence. The native and editor guards are being scoped
to current/unfinished rebuilds while retaining exact preserved predecessor
payloads and old state. Cheap regressions and review must pass before a new
candidate freeze/build. No installed acceptance failure or repeated T2 attempt
is being hidden by this source-review correction.

The same focused review found two associated receipt issues: a pruned original
Index job must report unavailable historical observation rather than failed
execution, and the editor's reviewed Resume must reach the already accepted
native rebuild even without repeating the initial option flag. The correction
retains complete diagnostics, rejects crossed candidate/predecessor identities
and resumes the same saved operation. No new failure-classification campaign,
job-pinning subsystem or automatic replay of uncertain mutations was added.
`rebuild-later-upgrade-red-r1.json` reproduced the editor blocker with **1
selected / 1 failed**, **0.327 s** wrapper. Following the correction,
`rebuild-history-extension-compile-r1.json` passed (**12.666 s**) and
`rebuild-history-extension-mocked-t1-r1.json` passed **1,117 selected**:
**1,096 passed / 21 skipped**, **9.571 s** tests / **9.632 s** wrapper. Native
regressions and the new source freeze remain pending at this point.

### Corrected r28 manual-test candidate

The final independent correction review cleared the known retirement, history
and exact Resume findings. Formatting was applied after the initial check;
`completion-history-clippy-r1.json` then found a test-only `unwrap_err` Debug
constraint (**41.010 s**), corrected without a production change. The namespace
assertion was also strengthened against a pre-call baseline.
`completion-history-clippy-r2.json` passed workspace all-target/all-feature
Clippy in **12.769 s**. `completion-history-native-t0-r1.json` passed **49/49**,
**1,989 excluded**, **11.674 s** tests / **66.787 s** wrapper. This includes real
canonical job-pressure pruning, native filesystem retirement with exact protected
inventories, reviewed pending-rebuild dispatch, fourteen interrupted recovery
subcases, guided CLI/parser cases and full canonical/MCP unavailable-history
parity. These are component regressions, not installed acceptance.

`completion-freeze-r28.json` separately freezes current native, extension,
harness and fixture inputs. Declared next T2 command:
`python scripts/build-service-runtime.py --destination artifacts/local/one-airk-1.8.14/windows-debug-r28/runtime --profile debug`.
Expected: new sealed native source matching this freeze, successful embedded
model/import/self-test and notice inventory. Stop at first failure or consumed
identity change. Then package with the same canonical native/extension commands
recorded above, replacing all `r27` destination/input components with `r28`.
Expected: verified local Windows debug VSIX; stop at any package failure.
Original r26 and r27 evidence remains unchanged. Release qualification remains
pending; the user will test this local unsigned candidate manually.

`completion-runtime-r28-r1.json` passed in **71.499 s**, including the sealed
embedded-model/import/self-test and **425-package** notice inventory. Native and
script inputs stayed unchanged, and Cargo timings were retained. The compiled
native source is **c173380b… / 1,136 inputs**. Freeze identities are native
**0821bf04… / 1,124**, extension **8c636676… / 391**, harness **8ab73b20… / 269**
and fixtures **bf8399bd… / 110**. Canonical packaging now consumes this successful
runtime; no original r26/r27 artifact is reused as a fresh build.

`completion-native-package-r28-r1.json` passed in **17.464 s**, including native
archive extraction/self-test/model checks. The complete r28 freeze was reverified
unchanged before VSIX packaging.

`completion-vsix-package-r28-r1.json` passed in **20.301 s**. The verified local
artifact is
`artifacts/local/one-airk-1.8.14/windows-debug-vsix-r28/publish/airk-1.8.14-win32-x64.vsix`,
**86,430,214 bytes**, **922 archive entries** (**920 allowlisted extension payload
files**, plus two VSIX metadata files), SHA-256
`2312dd70adc75ca831d3d1b89a26a0bc35ff44536d3c5aa51ed79b62471f8de5`.
Native install manifest:
`5882675cddfb1b57000d5141d4dac0dc17d30ae5e606f8de143e0bac4a106c40`.
All four freeze groups and consumed package inputs remained unchanged. Its
original dirty-build Git base is `f54fb4d048a55f80b0ac2e980260a8f849cfa82c`;
the subsequent source commit must not replace that compiled identity.

The VSIX is an unsigned local debug package for the user's manual testing.
No new enabled-native extension suite, authentic producer/editor campaign,
platform batch, full workspace tests/doctests/excluded campaigns, optimized build,
Docker qualification, fresh advisory audit or `ci-local` pass is claimed for r28.
Standalone installer signing, attestations, GitHub release and Pages publication
remain unrun. A successful package does not close those remaining gates.

README, documentation index, completion plan, working release record and
Marketplace description were reconciled before the authorized commit/push.
`completion-precommit-repo-r28-r1.json` passed (**2.894 s**); native/extension
versions and lockfiles agree at **1.8.14**. The existing authorized version bump
was retained. The configured human contributor is Tyler Jensen
`<tyler@tsjensen.com>`; the source commit must use `git commit -s` and its trailer
must be verified. The unrelated untracked feature-review draft is excluded from
this commit. No PR, release, signing or Pages publication is part of this push.

The staged whitespace check found one extra blank line at the end of
`scripts/tests/native-editor-failure.test.cjs`; only that blank line was removed.
The original consumed r28 freeze remains retained. Post-package snapshot
`completion-freeze-r28-postpackage.json` keeps native, compiled source, extension
and fixture identities identical; only the harness identity changes to
**aafe264a…**, with the same 269 inputs. No runtime or VSIX rebuild is warranted
for this test-file whitespace correction. The staged whitespace check then passed.

The user subsequently authorized documentation updates, a DCO-signed commit and
push after the new manual-test VSIX is built; **no PR** is authorized. Publication,
signing and release workflow dispatch remain outside that authorization.

## Initial observations (historical implementation baseline)

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
