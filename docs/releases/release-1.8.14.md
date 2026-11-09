# AIRK 1.8.14 implementation record

**8 November r28 installed-upgrade failure:** upgrading the user's running
1.8.11 installation with `airk-1.8.14-win32-x64.vsix` failed with
`checkpoint_complete_roster_limit: serialized snapshot`. Read-only aggregate
inspection found registry contract 12 with 991 succeeded jobs and 991 request
receipts, and no unfinished jobs. The checkpoint duplicated completed history
already covered by complete table commitments and exceeded its 448 KiB limit.

The correction passed 53 scoped native regressions and shipping-configuration
Store/CLI Clippy. New schema-2 snapshots retain full table
counts and authenticated row digests while expanding pending/running jobs and
their requests; completed records still undergo validation. Limits and retained
history remain unchanged. Schema-1 reobservation preserves existing immutable
checkpoint packets and interrupted journals; maintenance separately checks job,
request and configuration table commitments. Older checkpoint-capable 1.8.14
predecessors still execute their own capture and can retain the original size
refusal. Replacement Windows debug **r29** is built and package-verified:
`artifacts/local/one-airk-1.8.14/windows-debug-vsix-r29/publish/airk-1.8.14-win32-x64.vsix`
(86,427,333 bytes), SHA-256 `a0ed95d3ba0d008c75cd3bfcb89fb00e9063a1cbb150258ccb2a144fe562cc41`.
The runtime and package receipts are `completion-runtime-r29-r1.json`,
`completion-native-package-r29-r1.json` and `completion-vsix-package-r29-r1.json`;
all consumed native/harness inputs remained unchanged. Installed acceptance,
full workspace and cross-platform qualification remain pending for this correction.
The r28 evidence below remains historical.

**8 November local manual-test candidate:** Windows debug **r28** and its VSIX
are built and package-verified. The local file is
`artifacts/local/one-airk-1.8.14/windows-debug-vsix-r28/publish/airk-1.8.14-win32-x64.vsix`
(86,430,214 bytes), SHA-256
`2312dd70adc75ca831d3d1b89a26a0bc35ff44536d3c5aa51ed79b62471f8de5`.
Native manifest `5882675c…` seals compiled source `c173380b…` / 1,136 inputs;
the [ledger](../research/one-airk-implementation.md#corrected-r28-manual-test-candidate)
retains exact identities and receipts. Workspace Clippy, 49 selected native
regressions and the full mocked extension suite passed. The clean Python suite
passed 883 tests with 13 skips. This is an unsigned local debug package, with
installed/platform and final release qualification pending. Original r26 and
the intervening r27 runtime/package remain preserved; no r27 VSIX was built.

**8 November rebuild follow-up:** explicitly preserved predecessor payloads and
old state are recovery evidence, distinct from incomplete obsolete-payload
cleanup. A later ordinary upgrade retires unprotected intermediates and reports
the protected set separately. Historical rebuild receipts remain readable after
their candidate is superseded. Missing or expired original job receipts report
`Unavailable`; they prove neither execution success nor failure, and current
native readiness remains separate. These corrections require a new candidate
after r27; final artifact/installed qualification remains pending.
The clean full contributor-identity Python run selected 896 tests: 883 passed,
13 skipped, 77.768 s wrapper. Earlier sandbox failures and targeted recovery
results retain their original evidence scope.

**8 November standalone installer implementation:** Windows and macOS installer
support is implemented in source for only the CLI and native MCP/HTTPS service. They must not
install or invoke the VS Code extension/VSIX or Code actions; that distribution
remains optional and separate. Installers must use the existing platform signing
methods and verified GitHub release-asset pipeline before a signed download is
claimed. Actual native installer builds, signing and installed qualification are
pending. No publication action is authorized or claimed by this work.
Linux x64 standalone packaging and GitHub workflow attestations for its installer
and GitHub-hosted VSIX files are also implemented in source. Attestations authenticate
workflow provenance; Linux still has no native OS installer signature.
Marketplace retains Microsoft's automatic extension signing. The
[canonical standalone guide](../agent-integration.md#standalone-onboarding-without-vs-code)
records asset names and verification; website links and the version asset catalog
remain pending until exact artifacts are published and verified.

**8 November standalone onboarding:** guided `airk setup` without VS Code is
implemented in the working tree to discover/start native service, select or create indexes
with explicit source review, and obtain native grant/credential references plus
client trust/provisioning instructions. Existing-selector setup alone does not
provide this first-use flow. The [agent guide](../agent-integration.md#standalone-onboarding-without-vs-code)
and [HTTPS API](../local-https-api.md#obtain-a-connection) distinguish standalone
client environment provisioning from the VS Code resolver; bearer values remain
in native owner-protected files and never project JSON. Headless use requires
the native service, not a running VS Code proxy. Rebuilding into a successor
namespace requires explicit re-enrollment. Setup prints the approved installed
CLI path; it adds no proxy launcher and does not modify `PATH` or shell startup
files. `--launch` provisions only the child client process; top-level `uninstall`
delegates to native ownership. Validation and installed qualification are pending;
retained actual Codex/Claude acceptance keeps its older debug identity.

**8 November automatic rebuild scope refined:** offer rebuilding earlier in an
upgrade through native `runtime activate --rebuild-indexes --expected-active`
and the corresponding editor choice. After explicit admission, automatically
preserve previous state and supported settings, create fresh compatible indexes from
authenticated saved source definitions, and notify the user of progress/outcome.
Native and editor source is **implemented; installed/frozen qualification is pending**.
This replaces the earlier incompatible-failure trigger; no actual incompatible
failure receipt is retained, and exhaustive failure permutations are outside
this bounded implementation. The authentic 1.5.12 schema-8 metadata bridge passed
its scoped regression using original DDL independently of this rebuild path. The
[recovery contract](../upgrade-rebuild-fallback.md) records ownership, scope,
pause/watch policy, durable resume and interface parity requirements. Rebuilding
will remain separate from successful migration coverage. Versions remain 1.8.14;
production changes require a new candidate identity, preserving r26 evidence.
Initial support is limited to authenticated raw managed pre-catalog definitions
with exact saved source aliases, pause/watch preferences, vector opt-out, simple
detail settings and default bundled model policy. Modern catalog memberships or
grants and unsupported custom settings are refused before state changes; see
the recovery contract's complete limits. Old client configuration and credentials
remain preserved under their original namespace; explicit re-enrollment is
required for the successor. Selecting the editor option still requires native
authentication and eligibility checks before mutation. No complete-settings or
automatic grant-migration claim is made.
Selected recovery checks passed 8/8 Store tests, then 32/33 native tests with one
MCP fixture budget failure retained; corrected MCP tests passed 2/2. Fourteen
Store-backed interruption subcases passed. `Ready` records native Index-job
success; semantic readiness is separate. Standalone packaging contracts passed
77/77 and release/site contracts 51/51 with mocked platform tools. The latest
mocked extension suite passed 1,095 of 1,116 selected tests, 21 skipped, no
failures, in 9.052 s wrapper time. These are scoped development/component results,
not installed recovery, actual installer/signing or frozen qualification.

**8 November fixture compatibility repair:** a retained failing T0 regression
reproduced schema-1 catalog authority selecting raw registration. The
[compatibility matrix](../testing/fixture-compatibility.md) separates topology,
lifecycle, authority, input preparation, original scope and migration provenance.
The imported-legacy producer now requires a verified compatible managed raw
predecessor before installation and preserves exact launcher forwarding and scope.
At that repair checkpoint the prerequisite remained blocked: published 1.5.12 had schema-8 migration
eligibility unverified; newer raw artifacts lack matching reviewed source
correspondence. Reviewed 1.8.12 catalog migration remains separate. No new T2
producer/editor attempt, rebuild, release or publication is claimed by that repair.
The later authentic schema-8 bridge result above resolves the metadata question;
publication/imported-authority and installed producer acceptance remain separate.

**8 November resumed packaging:** the r26 native archive and debug VSIX passed
canonical packaging checks in **17.471 s** and **20.498 s**, respectively.
The original native build identity and all 811 runtime seals are preserved;
the two approved test-only corrections retain a separate source identity.
VSIX SHA-256: `e91f564a2db03a3e97fa1832aeb9f6075bb55be5b189c5f96720fe2bca8a8272`.
Enabled-native extension acceptance passed **1,108 tests / 3 skips / 0 failures**
in **162.923 s**. The authentic legacy-editor fixture producer then failed at
1.8.12 registration (`invalid_registry_request`), before editor launch. Its
failed fixture is retained; exact native Stop and subsequent unavailable health
were recorded. The raw-registration/catalog-authority fixture mismatch needs
resolution before another producer run. Installed-editor, load, raw clients/trust,
Neo, Docker and final Windows qualification remain pending.
No release action occurred; see the [ledger](../research/one-airk-implementation.md).

**8 November authorized fixture repair:** the shared modern/historical fixture
now requests native bounded model input preparation and preserves the returned
pin and complete refusals. Four T0 regressions and scoped Clippy passed; separate
actual r26 setup and held/checkpoint preflights passed one test each, including
66 embedding calls before checkpoint completion. All 811 original r26 seals and
production inputs remain unchanged; the runtime was reused, not rebuilt.
Historical runtime execution, cleanup, installed/platform and full final gates
remain unqualified. Versions remain 1.8.14; no release action occurred.

**8 November completion-plan stop:** the planned repairs and T0/T1 checks passed
combined review, and the original r26 debug runtime built with verified embedded
model/import receipts. The new actual checkpoint preflight then failed twice in
fixture setup. The mandatory two-strike rule stopped further validation;
local completion and release completion remain unmet. No r26 VSIX, OS trust
operation or later platform campaign ran. See the
[root-cause/evidence note](../research/one-airk-completion-stop-2026-11-08.md)
and [implementation ledger](../research/one-airk-implementation.md). Earlier
dated results below keep their original source/artifact identities.

**7 November 2026 — implementation in progress; unpublished and unqualified.**

**8 November commit checkpoint:** saved Windows receipts now show 3,224 workspace
tests passing (52 skipped), passing workspace doctests, and retrieval acceptance
of two workloads/nine cases on r20. The later schema-13 retirement correction
passed 14 focused tests. Debug r21 storage lifecycle still fails at full removal
after successful quota recovery; its preserved fixture requires diagnosis.
These results retain their individual source identities. Final extension,
Mac, signed installation and VSIX acceptance remain outstanding; see the
[ledger checkpoint](../research/one-airk-implementation.md#8-november-commit-checkpoint-and-remaining-storage-blocker).

**Subsequent storage acceptance:** the r21 timeout was a harness expectation
error: completed native cleanup intentionally retains the inactive retirement
identity. The corrected gate still requires collection deletion, worker exit
and committed cleanup metadata; it passed the complete lifecycle and replacement
campaign on the unchanged, seal-verified r21 executable in **321.351 s**. Eight
focused harness tests passed and a separate review found no blocking issues.
Native cleanup behavior is unchanged. This supersedes that storage blockage,
not the remaining platform/editor/package qualification requirements.

**C8 extension checkpoint:** the complete enabled-native r3 suite passed
**1,042 tests, three skipped, zero failures** on the verified reused r21 debug
runtime. Actual staged-editor acceptance then failed at its first Start action.
The working source corrects false interface replacement for Windows path aliases;
18 focused loader regressions passed. A subsequent staged-editor failure retained
`registry_busy`; the then-current shared client retained exact whitelisted reads through
six bounded retries in three seconds, with increasing waits. The historical full
extension r6 passed **1,044 tests, three skipped, zero failures**. Actual editor acceptance is still
pending. No final VSIX or optimized qualification
is implied; see the [C8 evidence](../research/one-airk-implementation.md#c8-full-extension-pass-and-staged-editor-startup-correction).

**First index-set correction:** actual editor acceptance subsequently exposed
creation being mistaken for editing an immutable managed set when both IDs were
absent. The working guard now requires an existing ID before that comparison;
new named sets stay unselected until explicit Apply. Fresh-state native/CLI
and focused regressions passed; the post-correction complete suite passed
**1,048 tests, three skipped, zero failures**. Actual installed debug VSIX
creation passed, then scope selection failed with retained generic
`invalid_registry_request`; a later trace proved the driver requested a grant
before asynchronous source-policy application completed. The driver now waits
for the exact application receipt. Atomic private-editor mailboxes also correct
a separate partial-JSON handoff race. The shared native index/catalog boundary
now owns the six-retry/three-second busy policy for all management frontends;
the extension no longer duplicates that window. The corrected r23 debug build,
22 scoped native regressions and Clippy passed; the new complete enabled-native
suite passed **1,057 tests, three skipped, zero failures**. The matching local
debug VSIX is packaged and sealed. Its installed-editor check failed at the
first UI dispatch with an unknown result; that action was not replayed, and
a later run progressed to a successful native grant but exposed lost scope
acknowledgment and competing choice refreshes. The working extension corrects
that sequence; 80 focused UI/controller tests passed. A new complete suite and
installed VSIX check remain required after this production change. The actual Explorer
update check passed with native execution disabled, preserving its route,
singleton and saved dismissal in the same extension host. These partial
results do not complete installed-native or final optimized-package acceptance.

**Reload attachment correction:** the later complete suite passed **1,060 tests,
three skipped, zero failures**, but installed reload then exposed an immutable
HTTP attachment mismatch. The saved grant persisted; a new Assistance host must
not reuse a credential tied to its old descriptor. The working extension now
binds automatic enrollment to the captured attachment and waits for its local
publication, preserving headless and old manual identities. The exact failure
was reproduced before the fix. Final notification handling also preserves a
published attachment withdrawal when a newer setup supersedes a pending one;
**146 focused tests passed**. Complete enabled-native r14 then passed **1,072
tests, three skipped, zero failures** in **295.142 s** on the unchanged sealed
r23 debug runtime. Fresh local debug VSIX r4 packaged in **21.422 s** from that
tested extension source; installed-editor acceptance remains pending. The existing 128-record
credential ledger has no reclamation; safe retention/compaction remains an
explicit matrix gap before final release qualification.

**Installed-editor checkpoint:** the current debug candidate passed actual
reload restoration, scoped HTTPS reads, standalone Assistance and provider Stop
with lexical reads afterward. Its r15 journey then failed a test-only assertion
that confused canonical status publication tokens with expanded envelopes;
68 final harness checks passed after the correction. Installed **multi r16**
then passed the complete selected journey in **104.973 s**, including endpoint
relocation, Explorer, revocation, final owned Stop/qualified cleanup and unchanged
hashes for all six source files. Its old-port listener retained three unattributed
TCP contacts; application data was not inspected. This result does not assert
zero contacts or attribute them to discovery. The same
VSIX passed actual same-window Explorer update in **16.858 s**, with native
execution disabled. These debug installed results do not qualify the final
optimized artifact or the other scenario/platform matrix entries.

The user authorized implementation of [One AIRK](../one-airk.md), including its
section 12.2 version adjustment to **1.8.14**. This record begins with C0 source
metadata preparation. It does not claim that the planned architecture or its
user-visible changes have shipped. Subsequent source integration and scoped
checks are tracked in the [implementation ledger](../research/one-airk-implementation.md);
the required final Windows package and Mac Neo qualification remain pending.

The [1.8.12 record](release-1.8.12.md) and earlier release records retain their
original versions, source identities, artifact hashes, successes and failures.
No previously built executable or installer is relabeled as 1.8.14.

## C0: synchronized source metadata

The version task began at base commit
`3c6e81cf5911751aac8dbe6767ea77abab11d206` with preexisting uncommitted
documentation, fixture and harness changes. Those changes are preserved; this
is not a clean-checkout or compiled-source claim. The existing version helper's
`changes()` logic synchronized:

- The Rust workspace package version in `Cargo.toml` and all eight local
  `airk-*` entries in `Cargo.lock`.
- The extension version in `packages/vscode/package.json` and both root package
  version entries in its npm lockfile.
- The generated offline Explorer template catalog's `extension_version`.

Unrelated dependency versions were not changed. The documented TypeScript-first
sequence compiled host modules before regenerating the catalog. Review of the
generated catalog diff found only the version field change; the unified MCP
schema catalog did not change.

Completed C0 checks:

| Command | Result and scope |
| --- | --- |
| `node packages/vscode/node_modules/typescript/bin/tsc -p packages/vscode` | Passed; compiled the host modules required by catalog generation. |
| `node packages/vscode/scripts/build-explorer.cjs --write-mcp-catalog` | Passed; regenerated and reviewed the versioned catalog. |
| `npm run compile --prefix packages/vscode` | Passed; normal host, webview and Assistance compilation plus bundle/catalog drift checks. |
| `python scripts/release-version.py check --expected 1.8.14` | Passed; native source, extension and lockfile versions agree. |
| `pwsh -NoProfile -File scripts/check-repo.ps1` | Passed for **1,847 public files** with the explicit current-workspace Git ownership exception scoped to that process. |
| Scoped `git diff --check` | Passed for the C0 version, generated-catalog and documentation files. |

The first repository-check invocation stopped at Git's current-user/workspace
ownership mismatch before enumerating files. The successful invocation used a
process-only `safe.directory` entry for this explicitly selected workspace;
global Git configuration and contributor identities were not changed. Git also
reported an inaccessible user ignore file. Those environment limitations do not
establish a source defect or native/platform qualification.

These are metadata and compilation checks. No native build, static inference
qualification, platform package, installed upgrade, security/client campaign or
release workflow has been run as part of this C0 version task. Later work must
record its exact commands, source/artifact seals, results and limits separately.

## Subsequent integration evidence

An earlier complete extension suite passed **938 tests**, with zero failures and
**21 optional skips**. Host, webview and Assistance compilation and bundle
construction passed. Scoped Core/MCP/process library tests passed **329 tests**,
including real Streamable HTTP sessions and retry/lifetime contracts. Subsequent
native integration changes still require their final regression runs.

Static ONNX Runtime prerequisite libraries built on Windows and Mac Neo. An
additional required `re2` archive was verified on Windows before successfully
linking the native CLI test executable. The full native CLI library run passed
600 tests and failed seven, with 19 skips; fixture and lifecycle corrections
remain under verification. The Mac supplemental build completed its compiler
steps but failed its process-runner postflight; that failed attempt is retained
and must not be called a qualified build. These results do not qualify the
final packaged executable or installed model behavior.
There is no newly qualified Windows VSIX or completed-source Mac installer test
yet. The [implementation ledger](../research/one-airk-implementation.md) retains
the exact scopes, failed attempts and timings.

## Historical integration gate checkpoint

Scoped CLI all-target/all-features Clippy passed in **35.463 s**, with the
persistent P10 target, incremental compilation on and retained Cargo timings.
The preceding native follow-ups recorded **three passes/two failures** across
five cases, then **one pass/one failure** across two cases. Their fixture source
was corrected; those failed runs remain failures. The full CLI all-features
**r2 campaign failed: 947 passed and 22 failed out of 969 run, with 30 skipped**.
It took **1044.904 s** of tests (**1053.874 s** including its runner).
The complete results are retained; the combined correction gate is pending.

The later Core/Store/Kernel/MCP all-features Clippy gate passed in **47.887 s**
observed wall time, retaining P10, incremental compilation and Cargo timings.
Its selected campaign ran **193 tests: 192 passed, one failed, 1,571 excluded**
in **27.144 s** of tests (**112.830 s** observed wall time). Physical adoption
layouts and the four previously failing Store adoption cases passed. The sole
MCP context test omitted a required `query_scope` view; its fixture input is now
corrected, with a rerun pending. The failed campaign remains recorded as failed.

The complete extension build/test suite passed **1,000 tests**, with zero
failures and **21 native-gated skips** (**1,021 total**), in
`npm-launcher-finalization-r4.log`. This covers host orchestration with mocked
native transport; it is not installed native/editor qualification. Retained
logs are `legacy-authority-mcp-{clippy,tests}-r1` and the extension log above.

Successful installation must migrate recognized MCP configuration to HTTPS
and remove the old AIRK launcher, rather than retaining it as a fallback.
The native cleanup and setup completion gates are being integrated; no removal
or installer acceptance is claimed yet.

An earlier full Python campaign ran **698 tests**, with **one failure and ten
skips**. After adding the required explanation for a deliberately single-shot
offline Assistance read, **30 targeted tests passed**. This does not replace a
successful full Python rerun.

The next full Python run executed **716 tests** in **68.570 s** (**69.108 s**
including its runner), with **five failure reports, two errors and ten skips**
(`python-full-final-r2.log`). The observed failures/errors were confined to
sandboxed PowerShell local HTTPS example cases. The unchanged complete affected
module passed **five tests** as the normal user with the existing protected
fixture base: **29.744 s** tests, **29.969 s** including its runner
(`python-https-examples-normal-user-r1.{log,json}`). This records scoped
normal-user acceptance and an environment-dependent failure; it does not prove
an exact OS cause or turn the failed full campaign into a clean full-suite pass.
No source or protected-file guards were changed for the follow-up.

Independent review found a release blocker: modern activation's
`Phase::Launcher` still copied the sealed main outside its bundle. The correction
is in progress; source intent alone does not prove the original
[#66 installation problem](https://github.com/iokaio/airk/issues/66#issuecomment-6710990197)
resolved. The Mac R1 source archive was transferred and restored only; its build
is held for that correction and source freeze. No Mac build, signing, lifecycle,
installed-editor or final Windows VSIX success follows from that transfer.
Subsequent source review confirms modern single-image launcher preparation and
repair now skip copying the sealed main. The old transferred Mac source is stale;
a fresh final source export and signed installed verification are still required.
The later five-package native r6 gate passed Clippy and CLI reference validation.
Its selected integration run completed **269 tests: 267 passed, two failed**
(**2,581 excluded**, **228.892 s** test time). All four previously failing split
transport cases and cache reprepare cases passed. Remaining failures are a TLS
test's chunked-body decoder and an MCP integration timeout. Complete commands,
timings and limitations are recorded in the
[implementation ledger](../research/one-airk-implementation.md).
The subsequent normal-user full Python run passed **730 tests with 11 skips**
and no failures/errors in **73.391 s**, including the local HTTPS examples.
The later diagnostic run identified a native help stack overflow: first-use CLI
catalog construction now uses one joined temporary thread with the existing CLI
stack allowance, then caches the complete catalog. Its small-worker-stack
regression and actual MCP terse/full-response integration both passed.

Both Mac signing workflows now share a post-sign execution gate: after removing
temporary signing credentials, run embedded-model and private installation
checks against the exact signed app, then verify its payload is unchanged.
This supplements signature/notarization checks for the original
[#66 installer failure](https://github.com/iokaio/airk/issues/66#issuecomment-6710990197).
The gate's 13 focused CI contracts and eight signing contracts passed; actual
signed Neo/hosted execution remains pending.
The later extension r5 suite passed **1,011 tests**, with zero failures and
21 native-gated skips. Python r4 passed **736 tests**, with zero failures/errors
and 11 skips. The sealed Windows debug r4 build passed payload and embedded-model
checks, but actual installation failed strict HTTPS verification because the
server certificate lacked an Authority Key Identifier. Native issuance and
same-CA leaf repair now pass focused regressions and actual strict HTTPS checks
in rebuilt Windows debug runtimes. The subsequent upgrade test exposed a
checkpoint bug: completed `catalog_index_setup/ready` receipts were counted as
unfinished work. Typed terminal-receipt classification subsequently passed
39 checkpoint tests and five actual split transport cases; the sealed Windows
r7 installer then passed the complete installed lifecycle gate described below. The latest full extension suite passed 1,013 tests with 21
native-gated skips. No completed Mac qualification or Windows VSIX follows from
these scoped results. The implementation remains in progress and unpublished.


## Windows debug r7 acceptance and remaining failures

The fresh debug runtime built and sealed in **162.591 s including its runner**
(**162.580 s** in its subprocess receipt). Actual installed acceptance passed in
**165.079 s**, recorded in
`artifacts/local/one-airk-1.8.14/windows-debug-install-r7.json` and `.log`. The
owned private fixture passed strict normal-CA HTTPS validation, independent
sessions, instance-fenced Stop/Start, exact-payload upgrade, rollback, previous
runtime retirement and resumable uninstall. This supersedes the earlier
checkpoint blockage; it is Windows debug evidence, not final optimized, VSIX or
Mac qualification.

Authentic pre-1.4 adoption r7 remains failed (**21.701 s**): the fixture failed the
native owner-only directory precondition before adoption. Schema-1 upgrade r7
failed its baseline ACL precondition (**0.162 s**); r8 failed during the original
old runtime's Stage with Windows path-length `os error 206` (**3.964 s**). These
setup/validation failures remain recorded and establish no successful
historical migration.

The later adoption r8 failed in **31.500 s** on an actual native defect:
checkpointing disabled serving admission, then the drain path requested
serving-only health and was refused before Shutdown; restoration found the
instance still busy. Its reviewed fix now uses exact
health identity during drain after admission closes; actual acceptance against
the corrected fresh runtime remains pending. This supersedes the earlier fixture
ownership diagnosis for that later attempt, not the failure record.

Schema-1 r9 passed old Stage using a short owned path but its wrong explicit
legacy registration mode was refused (**22.521 s** including its runner). R10
completed reviewed setup and model/index/vector jobs before a harness assertion
required a pipeline field absent from scoped status (**33.005 s**). A strict
semantic-query proof correction let r12 pass actual old semantic vector and
hybrid reads, but that attempt then failed the native engine contract
(**52.514 s**). The reviewed correction uses the explicit approved payload
version for engine contracts. These failed attempts are retained; the r13 pass
against sealed r8 is recorded below.

Full-workspace Clippy passed (**41.347 s**). The lightweight-main r1 check failed
(**69.065 s**, 231 unresolved names) because a shared standard-library import
was accidentally conditional on `local-inference`. After removing that
test-only condition, CLI Clippy r2 passed (**4.154 s**), lightweight-main r2
passed (**3.951 s**) and the default-feature workspace build passed
(**54.289 s**). The full Nextest campaign completed **3,187 tests: 3,178 passed
and nine failed**, with **51 skipped** (**1,766.015 s** test time;
**1,872.430 s** including its runner). All four restored long campaigns passed;
doctests did not run after the failed campaign.

**8 November - combined correction:** reviewed changes cover health-identity
drain, engine contracts from `Approval.version`, repair of five logical entries
from one proved backup with complete escrow inventory, the shared model
compatibility fence's existing-only two-second admission wait, and fixture
CAS/setup settlement, unified CLI catalog and ignore handling. Native policy and
ownership checks remain enforced, without replaying accepted mutations.
Formatting passed (**6.953 s**), full-workspace Clippy passed (**33.251 s**),
lightweight-main validation passed (**19.692 s**) and all **45 selected tests
passed** (**73.980 s** tests; **145.872 s** including the runner). Full Python
passed **755 tests with 12 skips**, zero failures/errors (**91.790 s** tests;
**92.782 s** including the runner). This is scoped correction evidence, not a
passing replacement full-workspace campaign.

**8 November - sealed r8 actual acceptance:** the debug runtime built and
sealed in **68.324 s** in its build receipt (Cargo **25.98 s**), using the persistent P10 target, incremental
compilation on and retained Cargo timings. Actual installed acceptance passed
in **169.275 s** (`windows-debug-install-r8.json`): strict normal-CA HTTPS,
instance-fenced Stop/Start, payload upgrade/rollback, obsolete retirement and
resumable uninstall. The ignored actual checkpoint owner gate passed **one
test** in **18.323 s** (**50.607 s** including compilation/runner; **756 skipped**
by the exact selection).

Genuine **1.8.12** schema-1 indexed semantic upgrade r13 passed in **83.045 s**
in its subprocess receipt. It completed
explicit old model/index/vector work and actual old vector/hybrid reads,
preserved registration/source/reviewed scope, reconnected over HTTPS and proved
obsolete-image cleanup and complete teardown. The full native observations are
retained in `windows-debug-schema1-upgrade-result-r13.json`. Command/result
identity binds the test to sealed **r8**, manifest
`3de8a00951667fb8bfd097991f795c0f2aba0c4238e7e0581abc789867da52fc`
and executable
`f9cdb76fab4c5d58b9033d101645bf09a592cb830e946279b97480aba03249d8`;
its runner's free-text r7 label is stale and is not the artifact seal.

Genuine **1.3.1** saved-index adoption r9 **failed** in **34.814 s** in its
subprocess receipt. At `adopt_saved_index_batch`,
native conversion refused the historical Current publication with `original
group has no saved publication`. This is an unsupported conversion, not proof
that the saved original index was empty. Source and pending native state remain
preserved after cleanup also refused. The old runtime was not executed, agent
UI was not exercised, and the requested 1.0.0-build.7 case was not reached.
The complete failed result/log remain retained.

The latest complete extension gate (`npm-final-r7.log`) passed **1,013 tests**,
zero failed, with **21 native-gated skips** (**1,034 total**; **23.437 s**
including compilation/bundling and runner). This does not replace the failed
full Windows campaign or turn its 45 selected correction tests into a full
pass. The final full rerun, doctests, Neo/signed qualification and optimized
Windows VSIX remain pending. The implementation is unpublished.

**8 November - historical Current and destination corrections:** the saved
Current reader now preserves original publication tokens, configuration and
resources without writing original indexes. Its final selection passed
**28/28 tests**, **1.495 s** tests and **19.244 s** including compilation/runner
(`windows-current-adoption-tests-r3`). The two initially failing fixtures were
corrected for actual lexical data and Balanced binding, without weakening the
reader's production checks. A subsequent scoped Clippy pass completed in
**12.046 s** (`windows-current-adoption-clippy-r5`). Canonical CLI reference/help
checks passed **three tests**, **40.252 s** including compilation/runner
(`windows-cli-reference-r1`).

Sealed debug r9 built in **139.406 s** (Cargo **38.64 s**), but actual 1.3.1
adoption r10 **failed** in **39.448 s** on a read-only destination registry.
The independently reviewed fix permits a narrow existing-writable registry
closure only while native maintenance is held; it adds no CREATE authority.
Its expanded final selection passed **40/40 tests**, **2.040 s** tests and
**71.390 s** including compilation/runner (`windows-current-adoption-tests-r4`),
with scoped Clippy **26.935 s** (`windows-current-adoption-clippy-r6`).

R10's secondary cleanup failure was a stale fixture instance after a legitimate
maintenance restart, rather than a RequestEpoch defect. The exact
selected/original/withheld-image census observed **zero matching live
processes**, without claiming pending native state had been removed.

Sealed debug r10 built in **68.280 s** (Cargo **25.26 s**), but actual 1.3.1
adoption r11 **failed** in **39.806 s** at the enforced source-isolation guard:
the coordinator had put converted data under protected service state.
The sibling-data/private-parent destination and all-batch source-preflight fix
is **in progress, not qualified**. Full failed results remain retained in
`windows-debug-pre14-adoption-result-r10.json` and `-r11.json` with their logs.
These times use subprocess receipts. The full Windows rerun, doctests,
current-source Neo/signed acceptance and final optimized VSIX remain pending.

**8 November - destination correction and partial conversion evidence:** the
sibling-data/private-parent and all-batch source-preflight fix subsequently
passed **50/50 selected tests**, **2.179 s** tests and **73.135 s** including
compilation/runner (`windows-current-adoption-tests-r5`). Sealed Windows debug
r11 built in **67.894 s**, with Cargo **26.08 s** and retained timings.

Authentic 1.3.1 campaigns r12/r13 reached **ready native conversion**, retaining
full original saved Current publication, configuration and resource provenance
for one group and two original configuration claims. Their complete campaigns
still **failed**: r12 in **53.931 s**, r13 in **55.508 s**, at `read_file`
CLI/API comparison. Review diagnosed validated observation clocks as the only
differences; the harness now validates these clocks before excluding their
transport-time values. This does not discard saved content, scope, authority or
provenance from comparisons, and it does not turn either failed run into a pass.

R14 then **failed** in **59.021 s** on unsupported `response_mode` in
`query_scope` (`INVALID_SCHEMA`). Investigation additionally uncovered a real
CLI shortcut that bypassed native scope authority. That shortcut has been
removed in favor of canonical native dispatch. Scope-r3 then passed **9/9
selected tests**, including actual installed query-scope calls/shell authority,
in **19.711 s** (**27.950 s** including compilation/runner), with **2,692 skipped**
by that selection; scoped Clippy passed in **2.007 s**. Full receipts/logs are
retained under `windows-query-scope-*-r3` and
`windows-debug-pre14-adoption-result-r12.json` through `-r14.json`.

Times above use subprocess receipts. No complete pre-1.4 campaign or earliest
requested tag is qualified. The full Windows rerun, doctests, current-source
Neo/signed acceptance and final optimized VSIX remain pending.

**8 November - complete 1.3.1 case, campaign still failed:** full Python r8
passed **756 tests: 744 passed, 12 skipped**, zero failures/errors, in
**76.956 s** (**77.612 s** including its runner). Windows debug r12 built and
sealed in **50.205 s** in its subprocess receipt (Cargo **7.23 s**), retaining
P10 timings with incremental compilation on.

Adoption campaign r15 passed its **complete genuine 1.3.1 case**: saved Current
conversion/provenance, two distinct Codex/Claude HTTPS credentials, both guarded
configuration migrations, full canonical-read parity, pre-migration cleanup
refusal, post-migration cache retirement and unchanged original fixture bytes.
Agent UI was not exercised. Its raw process observations were incomplete:
**434 scanned, 174 unobserved**, `complete=false`. No matching owned PIDs plus
the separate exact-image exclusion proof supported the scoped retirement
receipt; this is not a complete machine-wide census.

The **overall campaign remained failed**, **79.492 s** in its subprocess
receipt, because the subsequent **1.0.0-build.7** case hit Windows `os error 206`
during modern Stage before activation. Raw `CreateDirectoryW` has a
**248-character directory limit**; the staged notice directories reached
**249/251 characters**. A shared extended-path correction at the three native
path call sites is implemented, but scoped validation is still **running and
not yet qualified**. The complete r15 result retains the successful 1.3.1 case
and earliest-tag failure separately.

Full Windows rerun, doctests, current-source Neo/signed acceptance, earliest-tag
adoption and the final optimized VSIX remain pending. No overall release or
all-predecessor acceptance follows from this single passing historical case.

**8 November - earliest saved journal and checkpoint continuation:** sealed
Windows debug r16 built in **78.724 s** (Cargo **36.18 s**). Campaign r19 passed
its complete 1.3.1 case but **failed overall** in **114.007 s**: the
1.0.0-build.7 case reached native batch adoption and refused the old semantic
format during Current journal preparation; restoration also refused its
changed continuation snapshot. The retained delta contains zero jobs/requests
and one native setup receipt at `prepared`. This does not prove unexpected job
execution, and it is distinct from the earlier Stage path-length failure.

Implemented source corrections add a legacy saved-payload reader for lexical
journal construction/replay and validate the exact planned native Prepared
adoption checkpoint transition. They do not declare historical semantic
projection compatible or admit arbitrary changed continuations. Ordinary,
crossed, corrupted or unproven storage/receipt cases remain refusals. A new
negative campaign source corrupts only an authenticated fixture copy, expects
post-preparation integrity failure and retains its pending journal; it never
executes the historical helper. Corrected positive and negative actual
campaigns have **not passed**.

The preceding selected correction run (`windows-current-adoption-tests-r11`,
runner r15) **failed: 90 passed, one failed out of 91**, with **2,469 skipped**,
**4.606 s** tests and **23.461 s** including compilation/runner. Its new lexical
commit/replay/search case returned zero hits instead of one; investigation is
ongoing, without waiving the assertion. Full Python r9 passed **762 tests:
750 passed, 12 skipped**, zero failures/errors, **88.033 s** tests and
**88.720 s** including runner. These timings use subprocess receipts.

Full Windows workspace rerun, doctests, actual corrected earliest-tag and
corrupt-copy campaigns, current-source Neo/signed acceptance and final
optimized VSIX remain pending. No overall implementation or release completion
is claimed.

The subsequent correction runner r16 passed formatting (**7.038 s**) and
scoped Clippy (**12.659 s**). Selected test run r12 passed **91/91**, **2,469
selected out**, **4.824 s** tests and **22.056 s** including compilation/runner.
Its corrected test uses actual `prepare_artifacts_with_fts`, durable replay and
lexical search; the earlier map-only fixture helper had omitted FTS. The failed
r11 run remains recorded. R17 is building, and actual corrected positive and
corrupt-copy campaigns remain pending. Times use subprocess receipts; no final
qualification follows from this selected pass.

**8 November - actual r17 migration acceptance:** sealed Windows debug r17
built in **69.062 s** in its subprocess receipt (Cargo **27.18 s**). Positive
campaign r20 **passed both authentic 1.3.1 and 1.0.0-build.7 cases** in
**140.499 s**. Both retained full saved Current provenance, distinct
Codex/Claude HTTPS credentials, both guarded configuration migrations, full
canonical-read parity, premature-cleanup refusal, exact scoped cache retirement
and unchanged original fixture bytes. Full results are retained in
`windows-debug-pre14-adoption-result-r20.json`.

The actual corrupt-copy negative campaign **passed** in **37.682 s**. Adoption
of the deliberately damaged authenticated copy failed as expected; real native
maintenance restoration completed, originals stayed unchanged, diagnostic
fixture/pending cache state remained preserved and canonical final Stop reached
`complete`. Its full result explicitly records `adoption_succeeded=false`;
expected safe refusal/restoration is not adoption success.

Both campaigns bind runtime manifest
`98972d76393d86e09cbc3c8ae4a8bca33bd190635aa73692d3650f98e481a7db`.
These receipt timings and actual Windows debug results supersede the earlier
blockers for the tested formats while retaining all failed attempts. Neither
agent UI, every historical version, another platform nor a complete global
process census is qualified. The full Windows workspace rerun, doctests,
current-source Neo/signed acceptance, installed-editor hot update and final
optimized VSIX/release workflows remain pending. No publication is claimed.

**8 November - Neo r6 portability failure, r7 pending:** Neo r6 failed before
Rust compilation: Python ran **762 tests with two failures, 14 errors and 19
skips**, **12.826 s** tests/**13.442 s** owned stage; the batch took **26.842 s**.
Offline preparation, locked dependency fetch, npm installation and extension
compilation passed, but no Rust target or native/model/signing/UI acceptance
ran. This is a failed batch, not native or cross-platform qualification.

The independently reviewed correction changes only three Python test files.
LibreSSL fixtures now provide explicit SKI/AKI for Python 3.13 strict TLS;
same-subject issuer ambiguity in the Node extra-CA test is resolved while its
inherited root remains preserved; bootstrap's platform probe is mocked narrowly.
Diagnostics observed zero bearer requests before the TLS refusal. Production
native TLS and normal CA validation remain unchanged.

Normal-user Windows Python r10 passed **764 tests: 752 passed, 12 skipped**,
zero failures/errors, **71.567 s** tests and **72.339 s** in its subprocess
receipt. Neo r7 is pending. Windows workspace Clippy r3 (**26.666 s**) and
lightweight-main r3 (**35.036 s**) passed, but its **3,221-test full campaign is
still running**. Full workspace results, doctests, Neo/signed and installed
editor acceptance, and the final optimized VSIX/release workflows remain
pending.

**8 November - Windows full pass; Neo r7 correction:** Windows final runner r3
passed full-workspace Clippy (**26.666 s**), lightweight all-target validation
(**35.036 s**) and default workspace build (**40.058 s**). Full Nextest passed
**3,221/3,221**, with **52 skipped**, **three slow**, **1,772.412 s** tests and
**1,851.053 s** including compilation/runner. All four restored routine long
campaigns were included. Workspace doctest commands passed in **21.555 s**;
the eight crates contain **zero executable doctest examples**. Full logs use
the `windows-final-workspace-tests-r2` and `windows-final-doctests-r2` receipt
names within runner r3.

These complete Windows results and native r17 retain source identity
`ab43f7c8890c9cbdd5ef5f3aead2af7020cdde98928adf7826a7781d431be839`
(**1,114 files**). Subsequent source changes and artifacts must retain separate
identities rather than calling this a fresh complete rerun.

Neo r7 passed Python **764 tests: 745 passed, 19 skipped**, plus formatting,
then **failed Clippy** in **91.441 s** on unused
`macho_identity::payload_digest`. Cleanup removed **3,276 files / 807.8 MiB**
after preserving evidence. The reviewed correction gates that private helper
module with `#[cfg(test)]`; it has no production callers and Windows compilation
behavior is unchanged. Targeted post-fix formatting passed (**6.716 s**),
retrieval Clippy passed (**25.328 s**) and **one identity test passed**, with
**46 selected out**, **0.024 s** tests/**31.655 s** including compilation/runner.
The Neo batch remains failed until the next actual run; these scoped Windows
results do not substitute for it.

Neo r8 and its fresh source/artifact hash, signed lifecycle, installed-editor
hot-update acceptance and the final optimized VSIX/release workflows remain
pending. All Windows timings above use retained subprocess receipts.

**Pre-r9 checkpoint:** optimized Windows r1 built in **794.269 s** in its
subprocess receipt, with incremental compilation off and retained P10 timings.
Its native source identity
`6723387a63f6daff54e74d40a78992b59ff7648e22467686904d5d016fe36b06`
(**1,114 files**) matches Neo r8 and is
separate from the earlier full Windows `ab43...` test identity.

Neo r8 passed Python/formatting, then **failed Clippy** in **122.393 s** on
**eight Unix-only errors** involving a `Duration` import, legacy producer dead
code and test containment. Cleanup removed **4,020 files / 975.8 MiB** in
**0.346 s** after preserving evidence. Two reviewed source files were corrected;
an initial Windows dead-code failure was then resolved by gating the containment
method to actual legacy or Unix-test callers. Final scoped Clippy passed
(**19.418 s**), and **56/56 selected tests passed**, **1,046 selected out**,
**8.764 s** tests/**68.432 s** including compilation/runner. This does not
qualify the failed Neo batch; fresh optimized r2 is building on the frozen fix.

Reviewed VS Code alias/user-profile credential lookup and disabled duplicate/
manual policy handling corrections passed compilation and **41 scoped host
tests** (**2.048 s**) for the exact four-file freeze. The older 45-test
credential log is not validation of this correction. Private-home/shell-off
agent-install harness checks passed **30
Node tests**; modern 12/13 and legacy 12 readiness guards passed **11 Python
harness tests**. These are not actual editor/client acceptance campaigns.
Full Windows Python r11 passed **764 tests: 752 passed, 12 skipped**, zero
failures/errors, **75.096 s** tests/**75.761 s** including runner.

Neo r9, optimized r2's new identity, current-source signed/installed-editor
acceptance, post-review extension gates and final VSIX/release workflows remain
pending. Earlier full Windows and failed Neo results are preserved under their
original identities; no completed release is claimed.

**Final r9 review addendum:** future modern-upgrade cleanup now invokes the
exact verified predecessor for self-test, three metadata RPCs and endpoint
discovery, after the new bundle still verifies the selected installation with
`runtime inspect`. This fixes the compiled version/image guard mismatch without
weakening payload, health, namespace, instance or image checks. Ordinary clients
retain strict version gating; the older-owner adapter is metadata-only, with no
source/job/grant mutation or unscoped fallback.

Independent review approved the correction; TypeScript compilation passed
(**7.717 s**) and **44 service-bootstrap tests passed** (**1.176 s**). The
frozen production/test hashes are recorded in the implementation ledger. The
Explorer Evidence change is only a stale HTTPS/queue comment correction.
Python r11 remains applicable because no Python inputs changed afterward.
Optimized Windows r2 is still building; Neo r9, current-source editor/signed
acceptance and final VSIX/release gates remain pending.

**Pre-r10 checkpoint - private model root and matched debug evidence:** Neo r9
passed workspace Clippy (**150.781 s**) and lightweight validation (**39.228 s**),
then linked static debug successfully, but model smoke **failed** because its
temporary parent was mode **0755**. No runtime was published. Its **23-file**
evidence/timings set was retained, followed by cleanup of **7,001 files / 3.3
GiB** in **0.645 s**.

The reviewed fix creates the Unix root mode **0700** at creation, with a
canonical private child and lease. A child-only umask regression covers
**022/002/000** without changing the parent. Windows Clippy r1 passed
(**10.653 s**), **11 bundled-model tests passed** (**26.978 s** tests/**45.201 s**
including compilation/runner), and final formatting/Clippy passed
(**6.989 s / 10.735 s**). Actual Mac private-root/model qualification remains
pending.

Fresh debug r18 built in **66.994 s** (Cargo **25.62 s**), source
`b41b3611f47182d9d1b5e27c561b4704f6c5c26fbb4f28cfa91e3ad26406a13d`
(**1,114 files**), manifest
`3cd96b4f23bac3af274e435230d71730224154018d013b234541a209ec0c4005`.
Its embedded model produced **384 dimensions** and equal repeat results; the
matched current-source actual model/client preflight **passed in 65.468 s**,
retaining scoped full read parity, source bytes and completed managed Stop.

Optimized r2 remains on the older `bbba...` pre-private-root-fix source. Its
model/client attempts r1/r2 failed incorrect discovery-grant and impossible
scoped freshness-idle harness expectations (**31.166 s / 169.983 s**). Reviewed
harness fixes preserve exact scope/privacy/full parity; r3 on that original
optimized artifact passed (**52.597 s**). Neither that pass nor its older
manifest is relabeled as current optimized qualification. Failed-fixture
canonical Stop/health/image audits observed no matching live processes but
retain **177/192 inaccessible-process** limitations, not global census claims.

Full Python r12 passed **767 tests: 755 passed, 12 skipped**, zero failures/errors,
**74.977 s** tests/**75.582 s** including runner. Windows timings use subprocess
receipts. A fresh Neo model preflight is required before another optimized build;
other current-source native, extension, editor, signed and final VSIX/release
gates remain pending.

**Working r18 preflight results:** actual installed acceptance passed
(**167.102 s**), but the enabled-native extension suite **failed: 1,020 passed,
18 failed, three skipped out of 1,041**, **153.760 s** tests/**153.841 s** in its
subprocess receipt. Owner-private input ACL mismatches affect production
callers as well as fixtures; fixes are underway without weakening privacy.
Earlier passing suites with native cases skipped are separate evidence.

Managed read-only acceptance **failed** (**15.492 s**) on a missing supported
modern catalog-detach transition; a native correction is underway. Issuer
acceptance **failed** (**99.455 s**) on a stale 15-second header transport after
a 60-second overlap wait. Native finalization and canonical Stop were proven
separately, without claiming that transport campaign passed.

Port relocation r1 failed a missing fixture data-parent precondition
(**21.062 s**). Five harness unit tests then passed, followed by actual r2
**PASS** (**33.870 s**): preferred/saved-port collisions, stale TLS refusal and
scoped-authority refresh on the unchanged r18 image. Retrieval r2 **failed**
(**3.783 s**, zero completed workloads) on a reserved fixture-base/state clash;
its and sibling harness corrections remain underway.

These are subprocess-receipt timings, not a completed acceptance batch. The
current r18 native identity is unchanged until its pending managed-transition
fix. Native/extension/editor, Neo r10, signing and final VSIX/release gates
remain pending. This working update does not establish a final freeze.

**Working follow-ups - managed/issuer passes, native authority still blocked:**
Python r13 passed **776 tests: 764 passed, 12 skipped**, zero failures/errors,
**77.372 s** tests/**77.965 s** including runner. Managed-detach Clippy first
failed three test-only `unwrap_err`/missing-Debug usages; corrected Clippy r2
passed (**10.565 s**). An incorrect filter selected zero tests and failed;
the corrected selection then passed **31/31**, **1,073 selected out**,
**1.674 s** tests/**9.639 s** including compilation/runner.

Debug r19 built (**61.781 s**), source
`a5990432f53f0307d574a6912654d0a89b5441d1f44b88fad3f529791cb2b247`
(**1,114 files**), manifest
`fc91e578d53ba0448b9845ea996566e4c0df306974881d723a2a9dcf6783d2bc`.
Actual managed read-only r2 passed (**28.489 s**), covering exact settled
user-on-demand detach, preserved reservation/state/provider and untouched
external ownership. Same-approved reattachment remains an explicit configuration
action. Actual issuer r2 on debug r18 passed (**105.111 s**) for native/TLS
rotation; OS trust installation and external hosts are not qualified.

Production/fixture native-input corrections passed compilation and **61 scoped
tests**, **one skip** (**62 total**, **0.978 s** tests). The actual debug-r18
universal slice still failed **one of three tests** after the former ACL
refusals passed: schema-12-to-13 committed authority revalidation rejected the
stored identity. Its native correction is underway; no authority check is waived
and no passing full enabled-native suite is claimed. All times above use
subprocess receipts. Other current-source platform/editor/signed and final
optimized VSIX/release gates remain in progress, without a final freeze.

**Working r20 checkpoint:** schema activation passed **47/47 selected tests**,
**5.636 s** tests/**65.909 s** including compilation/runner; scoped Clippy
passed (**24.372 s**). Debug r20 built (**61.365 s**), source
`34f68b3bfa6ad0d1abfafd5d5a9130c02e3bfdf376ad3f924c0ef599eb69c74f`
(**1,114 files**), manifest
`75a3349b95cbdb81ab47edfa1d4f02aa60ea1575d50ae1a63143f6ef3c94ed78`.
Actual enrollment passed **three cases** (**16.994 s**).

The full enabled-native extension run still **failed: 1,039 passed, two failed,
three skipped out of 1,044**, **291.016 s** tests/**291.085 s** in its receipt.
Reviewed asynchronous-completion harness corrections passed actual focused
Assistance **1/1** (**41.638 s** tests) and provider-stop **2/2** (**42.582 s**
tests/**42.668 s** including runner), using single-Stop/read-only observation
without production changes. A complete corrected-suite pass is still required.

Publication faults passed **three attempts** (**64.895 s**) in an owned
foreground fixture, not on-demand qualification. Code benchmark **r19** passed
(**97.741 s**); transformer **r20** (**65.935 s**), document tables (**57.406 s**)
and usage (**74.230 s**) passed their actual preflights. Storage r1 failed a
quota-related scoped read (**62.582 s**); r2 remains running. Retrieval r3
**failed** (**39.989 s**, zero completed workloads) on obsolete configure/
add-root handling. The reviewed modern `nativeSourceReview` helper correction
preserves original aliases and explicitly granted new roots; failed-context
retention is fixed. The 20-test and six-test unit selections passed; actual
retrieval rerun remains pending.

Python r14 passed **785 tests: 773 passed, 12 skipped**, zero failures/errors,
**82.342 s** tests/**82.964 s** including runner, before the latest retrieval
helper correction. The current r20 **3,224-test**
Windows workspace campaign is running; formatting (**7.161 s**), Clippy
(**18.487 s**), lightweight validation (**19.309 s**) and default build
(**24.953 s**) passed, but no complete pass is yet claimed. Times use subprocess
receipts. Earlier full results retain their own source identity; fresh Neo r10,
extension/editor, signing and final VSIX/release gates remain pending.

**Working final gaps before retrieval r5:** storage r2 **failed late**
(**155.672 s** in its subprocess receipt), after durable quota wait and
same-job resumption passed. Schema-13 purge deleted immutable retired backing,
so same-root replacement removed the original retirement evidence. A reviewed
three-file native fix retains backing for schemas **12/13** and applies the
same compatibility guard to relocation's early read-only check. Tests are
authored; Cargo validation remains pending behind the ongoing full r20 run.

Retrieval r4 **failed** (**38.969 s**, zero completed workloads) on proven
typed `registry_busy`: its harness omitted read-only `sourceMutationDraft`
from `REGISTRY_READS`. Production already supports two bounded retries for
that read. The harness-only correction/regressions preserve request identity
and the original deadline, with no generic refusal retry, mutation replay or
retrieval native-production change. Actual r5 and storage-fix validation remain
pending; earlier failures are not relabeled as passes. Neo r10, current-source
editor/platform and final VSIX/release gates are still incomplete.

**8 November — C2–C9 reconciliation resumed:** all fifteen client entries are
now [classified](../agent-integration.md); three have implemented HTTPS adapters
and twelve stay explicitly blocked. Actual scoped Claude and Code/Copilot MCP
host acceptance remains pending. The retained Codex TLS experiment did not use
a bearer credential or execute a scoped AIRK read.

Missing installed Windows scenario preflight failed in **same** after
**61.722 s** on sealed debug r23 / VSIX r4; the remaining selected scenarios did
not run and native cleanup is unqualified. A peer window advanced the catalog
clock, causing selected Index management to pin stale metadata. The correction
revalidates current exact Index/backing metadata under unchanged window and
selection ownership, including Entire Index Usage. Its full controller/trust
module passed **32/32** (**3.142 s** runner); fresh packaged acceptance is pending.

Credential lifecycle source corrections durably revoke before gateway fencing
and compaction. The original token path retains a bounded secret-free ownership
proof and permanently blocks old-ID reuse; active manual credentials remain.
Positive owned attachment retirement can reclaim the 128-record active ledger;
historical proof storage still grows with issuance. Typed authentication-epoch
fences return 503, invalid credentials remain 401, and cache insertion rechecks
delivery while serialized with revocation. Independent source review found no
remaining blocker; formatting, native regressions and actual lifecycle evidence
remain separately required. Shipping-feature Clippy exposed legacy test guards
that are now corrected; its failed attempt is retained.

Neo r11's earlier **679.370 s** batch is collected and remains failed. Python,
formatting, workspace Clippy, lightweight compile, debug model build and
checkpoint checks passed on its original source. Installation preflight failed
because two live foreign process image paths were unobservable. Later workspace,
optimized, signed and installed-editor stages never ran; required Cargo cleanup
passed and all completed logs/timings were retained. A related safety correction
requires a proven dead retained task-name right before a Mac audit-token path
`ESRCH` can mean process retirement; its native Mac regression is pending.
See the [implementation ledger](../research/one-airk-implementation.md) for exact
source identities, commands, timings, retained failures and subsequent gates.
No optimized freeze or final package/platform completion is claimed here.

**Reconciliation validation checkpoint:** scoped shipping-feature preflight
passed **98/98** tests (**213.365 s** runner, **168.571 s** execution), including
actual semantic setup, MCP/CLI reads and recovery, credential compaction, TLS
fencing and cross-host Mac exit classification. Two earlier Assistance test
failures retained their fixture/architecture evidence; the correction preserves
the existing owner/SYSTEM/admin ACL and pure forwarder boundary. Fresh sealed
Windows debug **r24** built in **78.497 s**, native source
`01814b2357ba556faf4d4f8d3ad21c0183e490f10d3e4bc1b315b665202688fe`
(**1,115 inputs**), installer manifest
`501c8d85a8d6b836489d4d425f0cb142d3aa965221f70629899dd85de58b7acc`.
Its actual HTTPS/model-client gate passed (**67.062 s**) with complete managed
Stop. Current native workspace Clippy (**48.292 s**), lightweight compile
(**34.841 s**) and default build (**41.542 s**) passed; full workspace execution
is still running. The previous **791-test / 12-skip** Python pass predates the
new container-seal parser and is not final-source script qualification.

Actual Codex **0.164.0-alpha.11.1** passed scoped native reads, crossed-authority
refusal, fresh-process reconnect, retained additive CA trust and wrong-issuer
rejection on r24 (**62.412 s**). No model turn, OS trust or user configuration
change was used; all client processes exited and native Stop completed. Earlier
fixture-access and one-line-range harness failures remain separately recorded.
Claude and the separate Code MCP host still need their actual acceptance.

The extension's separate lifetime-history limit was also found and corrected:
legacy reconnection now uses stable original provenance and direct/native exact
connection IDs, retaining old manual metadata without scanning every attachment.
The new asynchronous owner-fence and >128-history regressions require the full
current enabled-native suite, which is running. Native r24 is reused with its
original compiled identity; this extension change still needs a fresh VSIX.

The first actual 1/10/64-client benchmark failed before a completed cohort
(**86.629 s**). Its held HTTP connection outlived the native 15-second header
idle timeout while a semantic job took **21.745 s**; teardown then masked the
primary transport failure. A corrected harness must use an explicit fresh TCP
connection before a new request and preserve uncertain failures without replay.
This is not an accepted resource campaign yet. Linux/container qualification
first failed before compilation because the pinned base image lacked Clippy.
That failed attempt and historical cache exports remain identified; the isolated
component-prerequisite correction is running. Its source-seal parser correction
passed **15** focused tests. No final platform or package pass follows yet.

**Recovered completed results (8 November):** the earlier “running” Windows
campaigns have completed. Workspace reconciliation r1 ran **3,237 tests:
3,235 passed, two failed, 52 skipped** in **1,974.948 s** runner time
(**1,867.631 s** execution). The failures cover explicit source/storage
retirement and reference refresh; reference refresh aborted with Windows stack
overflow `0xc00000fd`. Its receipt reports `native_inputs_unchanged=false`, so
this result does not qualify one exact current source tree. The unfiltered
workspace command restored the four routine exclusions, retained Cargo timings
and used persistent `artifacts/v190/p10-target` with incremental on. A corrected,
frozen-source workspace result and final doctests remain required.

Enabled-native extension reconciliation r1 passed **1,074 tests, three skipped,
zero failures** in **311.732 s**, reusing sealed r24 with its original compiled
identity. The packaged installed r2 attempt then failed **same** in **88.123 s**
with HTTP **404**, `Not Found: Session not found`. Its read-only audit retained
catalog revision **9 → 10**, unchanged immutable selection and an unrevoked
credential; completed delivery receipts already used revision 10. This supports
the cached-context/session replacement diagnosis when only the catalog clock
advances, without directly observing the retained in-process cache. Remaining
scenarios and this failed fixture's native cleanup are unqualified. The earlier
multi r16 success retains its separate artifact/scenario scope.

Frozen Python r3 passed **801 total / 12 skips** in **87.782 s**. Repository
checks passed **2,064 public files** in **2.928 s**. The dependency gate passed
in **15.771 s**, including live advisories, with documented dispositions for
`paste` (`RUSTSEC-2024-0436`), `ttf-parser` (`RUSTSEC-2026-0192`) and `minimatch`
(`GHSA-g3hv-75wg-m249`); this is not a zero-findings claim. Later harness edits
require their affected checks again.

Actual Code r1 failed tool discovery (**115.886 s**) and has no qualified native
cleanup receipt. Actual Codex r4's **62.412 s** scoped authenticated acceptance
above remains passed on sealed r24; Code/Copilot, Claude, final-artifact and
other-platform acceptance remain incomplete.

Resource campaign r2 completed **1**- and **10**-client cohorts, then failed
**64** clients with an additional-model-load assertion (**410.050 s** overall).
The later harness permits exactly one first-warmup reload only after observed
uncached/no-outstanding state, then freezes owner and load count. Its **39**
focused checks passed; actual execution of the newer campaign remains required.
This does not establish private memory, idle CPU or global helper absence.
Credential compaction is implemented and scoped-tested; installed capacity and
resource acceptance remain separate gates.

Read-only Docker history recovery resolves the stale Linux/container r2
“running” receipt: build `lw5phixc05c3iw4ozvtbl1udy` was **canceled**,
`context canceled`, after **2,251.936 s** at **2026-11-08 12:27:54 America/Denver**.
Its cache-owner regression passed (**2.168 s**) and Clippy/rustfmt provisioning
completed; static ORT provisioning was canceled before AIRK qualification.
The original receipt and recovered Buildx record/log remain separate evidence.

Neo **r12** was also recovered on native source `01814b2357ba556faf4d4f8d3ad21c0183e490f10d3e4bc1b315b665202688fe`
(**1,115 inputs**). Python (**801 total / 19 skips**), formatting, workspace
Clippy, lightweight compile, debug static runtime/model and checkpoint passed.
Shipping native regressions passed **46/46** (**8.878 s** execution,
**126.311 s** runner), including actual owned-child Mac task-name and missing-path
classification checks; these regressions are no longer unrun on that source.
The batch was deliberately interrupted before broad workspace/release tests on
known pre-correction MCP session-cache source. Installer preflight returned **-2**
after **121.364 s**, with `KeyboardInterrupt` during its stable-owner interval;
this is neither a Mac assertion failure nor passing installed acceptance.
The overall r12 result remains **failed/interrupted**, **603.141 s**. Required
Cargo cleanup passed (**0.720 s**) after preserving evidence, and owned
descendants settled. Complete existing receipts/logs/JUnit/timings are retained
in `artifacts/local/one-airk-1814/neo-r12-recovered-20261108/evidence`.
R11 remains the earlier separate census failure; recovery launched no workloads
and stopped no processes. Final-source, optimized, signed, installed-platform
and VSIX qualification remain incomplete. No final package acceptance
follows from these results. The [recovered ledger](../research/one-airk-implementation.md#recovered-completed-reconciliation-results-8-november-2026)
names the exact retained receipts and source limitations. Version stays **1.8.14**.

**Resumed correction checkpoint (8 November):** the native MCP context cache now
retains its canonical SDK service/session across a forward catalog clock under
unchanged credential generation and immutable authority. Delayed older
authentication cannot replace a newer context. Actual TLS/MCP regressions passed
same-session reads on retained and fresh TCP; a matched installed rerun is still
required before claiming the earlier installed r2 session failure is resolved.

Historical standalone base-ID recovery now requires native installation,
authority/client/expiry proof and a different live Explorer attachment. Native
issuance rechecks the old ID under its mutation owner, so explicit revocation
overrides automatic attachment retirement even across compaction-crash recovery.
Schema-2 tombstones retain the reason/provenance within **2 KiB**; unknown
schema-1 history stays review-required. This metadata remains private CLI/native
control authority; evidence MCP gains no management capability. The original
reviewed scope and retired-ID reuse fence remain enforced.

Selected adoption/provenance TypeScript modules passed **147 tests, one skipped,
zero failures** (**148 total**) in **8.379 s** after compilation in **12.625 s**.
Core/CLI shipping-feature scoped Clippy r4 passed in **18.862 s**, with two jobs,
incremental on, persistent P10 target and retained Cargo timings. These focused
checks do not replace the full extension/native/platform gates.

Native preflight r2 completed **47 passed, one failed, 770 skipped**. Explicit
source/storage retirement now passed in approximately **33 s**, preserving the
same MCP session through explicit fresh TLS and replaying no exchange. Reference
refresh still aborted with stack overflow in that retained attempt. Diagnosis
measured **1,540,016 bytes** of additional journey poll stack (approximately
**1.47 MiB / 1.54 MB**) against a **2 MiB** standard thread stack; the heap future
itself was **41,056 bytes**. The initial phase-helper attempt failed and remains
recorded. The minimal independent native-task correction preserves canonical
handlers and assertions; `resume-reference-and-control-r1` completed **11/11
passed**, **133.458 s** execution / **144.289 s** runner, with stable native/shared
fixtures. Temporary diagnostics are all removed and source is ready for freeze.
This focused result does not qualify full workspace or installed acceptance.

Actual Claude/Code runners and Neo r13 are prepared, not run. R13 retains the
shipping corrective → debug/model → checkpoint → actual installed preflight
order before full workspace, restored long campaigns, doctests, optimized,
signed and editor acceptance. No final optimized, full-workspace, doctest,
platform or release qualification is established. Exact receipts and remaining
limits are in the [resumed ledger](../research/one-airk-implementation.md#resumed-correction-checkpoint-8-november-2026).

## Planned changes and acceptance evidence still required

The following are requirements from the implementation plan. Source slices are
being integrated; each contract still needs its relevant final acceptance before
it can be described as available in an installer:

| Area | Required evidence |
| --- | --- |
| One native executable and embedded MiniLM | Static model/runtime linkage, original model/tokenizer hashes and inference outputs, executable/import inventories and actual same-image worker counts. No helper-distribution removal claim follows from C0. |
| Authenticated local HTTPS MCP | Endpoint discovery, automatic alternate-port behavior, certificate trust enrollment before credentials, scoped authentication and real direct-client compatibility. Existing transport/configuration behavior remains the baseline until explicitly migrated. |
| Developer JSON API | Implemented endpoint/schema contracts, generated reference/OpenAPI or JSON schemas and runnable authenticated local-tool examples. |
| CLI, Workbench and MCP parity | Equivalent supported inputs and complete results, provenance, coverage, continuation, retry and failure observations through actual native/transport paths. |
| Checkpointed shutdown and upgrades | Durable state packets, accepted-job compatibility, interruption/recovery, exact predecessor/process inventory and guarded older-installation migration. Supported predecessor versions remain to be qualified. |
| Direct operating-system supervision | Actual lifecycle and ownership checks for each supported OS. The plan's Mac baseline is the current-user GUI login domain; broader login/session coverage is not implied. |
| Packaging, signing and publication | Fresh source-bound 1.8.14 artifacts, complete inventories, platform validation and authorized release workflows. The sealed Mac main must remain inside its signed bundle; embedding inference does not waive Developer ID or notarization. |

The implementation responds to the original
[issue 66 comment](https://github.com/iokaio/airk/issues/66#issuecomment-6710990197)
and [issue 67](https://github.com/iokaio/airk/issues/67). Those issue references
and the plan establish requirements, not proof of a fix. Record failed attempts
and unknown coverage separately from successful evidence as implementation
progresses. Future tested client versions, platform limits and predecessor
support must be explicit.

## Distribution status

The [local HTTPS API guide](../local-https-api.md) documents the unpublished
developer contract, including canonical tool discovery, full JSON results,
scoped credentials, local CA validation, asynchronous admission and reconnect
behavior. Native CLI setup returns a setup bundle with configuration and
protected-file references; it does not print bearer secrets. Serializer support
and actual client/platform qualification remain separate observations.

The macOS packaging changes continue to address the original installer report
[#66](https://github.com/iokaio/airk/issues/66), including the
[reported installation failure](https://github.com/iokaio/airk/issues/66#issuecomment-6710990197).
The modern single main executable stays inside its signed app bundle. Native
service startup selects that sealed image directly; it must not copy the sealed
main executable out as a launcher. Historical launcher payloads retain their
version-specific verification and retirement paths. These source and fixture
checks do not substitute for final Developer ID and installed Neo evidence.

Replacement is being extended to direct upgrades from any earlier official AIRK
installation format, including standalone extension helper caches and skipped
versions. A completed replacement must remove owned obsolete launchers, role
copies, helper caches and runtime payloads after preserving saved evidence and
migrating client configurations. Future HTTPS upgrades and ordinary restarts
must rediscover changed ports through native management and refresh owned client
entries without changing their credentials or selected indexes. External clients
may require their own configuration reload or reconnect.

An earlier combined CLI correction campaign finished **failed: 29 passed and
nine failed out of 38 tests**, with 982 excluded. A separate MCP/Store gate passed
161 tests; neither result qualifies the final installer. A targeted Neo process
identity probe passed for the native audit-token termination API, using only an
owned test child. Subsequent scoped/native and Windows debug results are recorded above; signed
Mac installation, historical migration and Windows VSIX acceptance remain pending.

There is no 1.8.14 native installer or accepted release artifact from the C0 task.
Current source version metadata does not certify an old runtime, a new process
layout or an endpoint migration. Full repository/platform gates and independent
combined-diff review remain required before an installable candidate or release
claim. No commit, push, hosted CI dispatch, notarization or publication was
performed by this version task.
