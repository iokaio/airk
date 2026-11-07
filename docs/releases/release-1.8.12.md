# AIRK 1.8.12 local working build

**7 November 2026 — unpublished manual-test candidate.** The maintainer requested
a patch increment and a new local Windows installer for persistent Explorer
notice dismissal and automatic AIRK extension refresh. Rust workspace/local
lockfile entries, the extension manifest and npm root lock entries use **1.8.12**.
The [1.8.11 record](release-1.8.11.md) retains its original source, tests and package
identities. Its results do not qualify this new candidate.

## Explorer notices

Update and connection banners, host errors and screen-local action outcomes have
an accessible **Dismiss notice** (**×**) control. Dismissed notices leave no blank
banner space. Dismissals are independent of the active route and are saved in the
workspace before the notice hides, so they survive switching screens, closing and
reopening Explorer, and reloading the VS Code window.

Notice identity follows the update or message content and project where relevant,
not elapsed time or route. The same dismissed notice stays hidden; different
updates and outcomes can still appear. Required validation, evidence
qualifications, empty states and explicit confirmation dialogs retain their
existing roles. Hiding a notice does not approve an action, clear native failures,
change trust/native-execution restrictions or bypass upgrade gating. Service
details and logs remain available.

The host validates exact keys to 1–8192 UTF-8 bytes and serializes preference
writes. Failed persistence leaves the notice visible with an explicit error.
The workspace retains up to 512 unique dismissals without evicting old choices;
capacity exhaustion reports a failure rather than a successful dismissal.

## Extension replacement

Once the new loader is active, later detected installed AIRK payload changes
replace AIRK's own running JavaScript automatically. AIRK disposes the previous
extension instance, loads the installed replacement and reopens its single
Explorer panel when it was open. The whole VS Code window does not need to reload
for that path. Native runtime activation remains separately observed, and native
indexes and accepted work remain with their existing owners.

The first transition from a running 1.8.11 or older extension cannot retrofit
this loader into old code. To activate 1.8.12 for that one-time transition, use
**Restart Extensions** in VS Code’s Extensions pane. This specific GUI action
applies the installed extension replacement; the generic extension-host restart
command does not apply that installed delta. External coding-agent sessions have
their own connection lifecycle. Installed-editor acceptance and platform-specific extension
replacement behavior must be recorded separately from source-level tests.

## Inherited behavior

Indexing retains **Search scope**, **Index library** and **Index sets**, explicit
setup, reviewed index deletion and native scope ownership from 1.8.11. Opening a
folder or evidence view does not create or index it. CLI/MCP/Workbench evidence
reads retain their canonical implementations and complete native response data.

The optional older `check-native-editor.cjs` trusted selection journeys still
target saved-association and explicit-grant controls removed by the inherited
1.8.11 UI redesign. This is a known fixture mismatch, not an observed new native
installation failure. Current bootstrap/tab-handoff and Restricted Mode passes
do not qualify full index-creation, checkbox-selection or multi-window GUI
journeys. The native installer/client/process/MCP gates remain separate; see the
[CI scope and bounded follow-up](../ci-and-builds.md#native-build-reuse-and-darwin-failure-diagnostics).

## Validation and package evidence

The current Windows evidence directory is
`artifacts/local/1.8.12-explorer-20261107/`. Stage receipts retain the exact command,
exit code, elapsed time, target/incremental settings and before/after native
source seals. Existing 1.8.11 artifacts retain their original identity and are
not relabeled as 1.8.12.

### Completed source and native checks

| Command and scope | Observed result |
| --- | --- |
| `cargo fmt --all --check` | Passed in **5.700 s**. |
| `python scripts/dev-check.py --workspace --all-features --skip-fmt --target-dir artifacts/v190/p10-target` | Passed in **66.633 s**; invokes locked workspace/all-targets/all-features Clippy with `--timings` and warnings denied. |
| `cargo check -p airk-cli --no-default-features --all-targets --locked --timings` | Passed in **63.863 s**. |
| `cargo build --workspace --locked --timings` | Passed in **78.206 s**. |
| `python scripts/build-service-runtime.py --profile release --destination artifacts/local/1.8.12-explorer-20261107/runtime` | Fresh optimized main/helper pair passed in **828.151 s**, including release notices for **366 third-party packages**. |
| `python scripts/check-transformer-model.py --binary artifacts/local/1.8.12-explorer-20261107/runtime/airk.exe --report artifacts/local/1.8.12-explorer-20261107/vector-check.json` | Passed in **23.255 s**: real helper inference, persistent DiskANN, default/vector/hybrid reads, opt-out/re-enable, invalid inputs and source preservation. |
| `python scripts/check-service-clients.py --runtime-dir artifacts/local/1.8.12-explorer-20261107/runtime --files 16 --model --report artifacts/local/1.8.12-explorer-20261107/service-clients.json` | Passed in **28.569 s**: two projects, six MCP connections, reconnect/same-project reads, model use, retained failure diagnostics and source preservation. |

The persistent Cargo target is `artifacts/v190/p10-target`. Development checks and
the workspace build used `CARGO_INCREMENTAL=1`; the optimized producer removed
that development override and used Cargo release-profile defaults. Cargo
`--timings` HTML reports were copied into the evidence directory's
`cargo-timings/` subdirectory. These are observed local timings, not a
cross-platform performance claim.

Every completed native receipt above retains the same **978-input** source seal:
`51536a705cf9cbabe4947cacddf43a8cb29b0af8bad4e23ca7f293aa6dd347fa`.
The fresh runtime's `airk-build.json` binds its release profile, that source seal
and the following actual images:

| Native image | SHA-256 |
| --- | --- |
| Main `airk.exe` | `2f15a6c2ceb033d551ef98741a8a5999fccea341ce906f2cc8099ce4c7cad37d` |
| Inference helper `airk-embed.exe` | `5816f6a48c93287a3ffb5c579f8c9c173dde3eb9b28081541f80e4ff40df3d34` |

`npm test --prefix packages/vscode` completed host, webview and Assistance
compilation, bundle/catalog checks and **913 passed / 0 failed / 21 optional
cases skipped** (**934 selected cases**, test-run duration **7.048 s**).
This is the retained pre-metadata-correction checkpoint; the later completed
corrected-loader results appear below.
The complete Python contract run completed **485 tests in 28.664 s**, with
**9 skips** and no failures. That Python result precedes the final installer
guidance regressions. Their focused run, `python -m unittest
scripts.tests.test_install_local_vsix -v`, subsequently passed **4 tests in
2.968 s**, including modern/historical archive guidance and byte-mismatch
refusal. Repository checks passed for **1,835 public
files**, and whitespace/version-agreement checks passed at their recorded
checkpoints. These observations do not claim unavailable online advisory or
complete release qualification.

The completed routine Windows workspace run used
`cargo nextest run --workspace --all-features --locked --offline --timings
--profile ci --no-fail-fast --retries 0 --test-threads 4`. It passed
**2,717 tests / 0 failed**, with **55 platform/default-profile skips** and no
retries. Test execution took **1,335.366 s**; the wrapper completed in
**1,433.561 s**. `workspace-tests-result.json` retains the command, the unchanged
978-input native seal and Cargo timings. The checked-in default filter remained
active: the four long campaigns were not restored for this extension correction.
This is a completed routine selection, not the unfiltered qualification suite.

`cargo test --doc --workspace --locked --timings -- --test-threads=1` passed in
**34.466 s** wrapper time across **8 crate targets**, with **0 eligible examples**.
That is eight successful doctest targets, not eight executed examples. Its
`doctests-result.json` retains the matching native source seal and timing report.
Both commands reused `artifacts/v190/p10-target` and `CARGO_INCREMENTAL=1`.
The later repository check passed for **1,838 public files**, recorded in
`repository-final.log`; the earlier 1,835-file result above retains its original
checkpoint scope.

### Actual VS Code loader preflight

`scripts/check-extension-update-editor.cjs` exercised the actual extension loader
and Explorer panel in an isolated VS Code profile. The successful receipt is
`artifacts/editor-update-preflight/run-aY9GCw/result.json`: a fictional
**1.8.12 → 1.8.13** extension entry and a same-version replacement preserved the
selected route, forwarded the stable API, kept one Explorer panel and left a
closed panel closed. The receipt explicitly reports
`native_service_exercised: false` and `window_reloaded: false`. The native service
and packaged AIRK application were not exercised by this fixture; it is separate
from full installed-VSIX/native acceptance.

Earlier attempts remain failures rather than being relabeled: the editor
preflight retained an invalid-update-inventory rejection, a closed-panel
disposition timeout and a later tab-close assertion failure. The final fixture
uses the editor's actual webview tab identity and explicitly closes the intended
Explorer tab before checking that it stays closed. The product panel owner also
tracks exact tab/group closure so delayed webview disposal cannot reopen a
user-closed Explorer. An extension-suite attempt
also failed a successor-activation timing assumption; the final full-suite
result above follows its correction. The initial Python run retained two
packaging failures involving the new loader module and parser-closure fixture;
the final compile/package closure and corrected fixture run passed. The initial
Electron launch failed its installed-editor directory ACL check; its diagnostic
remains in `artifacts/editor-update-preflight/run-k5Ecrc/editor.log`. This was
an editor sandbox launch failure, not a failed native private-census check.
These corrections do
not turn the fictional editor fixture into native service or cross-platform
qualification.

### First Windows package candidate: packaging passed, activation failed

`python scripts/package-native.py --target x86_64-pc-windows-msvc --runtime-dir
artifacts/local/1.8.12-explorer-20261107/runtime --output
artifacts/local/1.8.12-explorer-20261107/native` passed in **15.558 s**.
`python scripts/package-extension.py --target x86_64-pc-windows-msvc --native-dir
artifacts/local/1.8.12-explorer-20261107/native/publish --output
artifacts/local/1.8.12-explorer-20261107/vscode` passed in **19.711 s**, including
the final packaging compile and platform-VSIX verification. This first candidate
then failed actual installed-editor activation and is **not an accepted
installer**.

The native candidate records **`dirty_checkout: true`**. This is an authorized
local build from uncommitted source, not a clean committed-tree candidate. The
manifests retain base commit `50beedd5c57978b52ac174fe1089d36ac4620a5d` and base tree
`6ca5bc5224e49cfd25f89ef31dedad7a4a614de9`; those Git identities do not identify
the uncommitted implementation by themselves. The retained native source seal,
build receipt and exact archive/payload hashes identify what was built and
packaged. No source commit was created for this request.

| Artifact | SHA-256 |
| --- | --- |
| `native/publish/airk-1.8.12-x86_64-pc-windows-msvc.zip` | `9e1b0db7cd30a191f1bea6f8d8e388260257b129d54e1bb95c2d8b3c83514865` |
| `vscode/publish/airk-1.8.12-win32-x64.vsix` | `e50765659b3b53c3df6d461a9fcb6273afd46d483d659319b7a529ec23f53151` |

Artifact paths are relative to the evidence directory above. This first VSIX is
**77,674,698 bytes**, contains **779 archive entries**, and includes the loader's
`airk-update.json` inventory. The local Node version was **24.21.0**; this does
not assert testing under CI's separately pinned Node version.

Actual installed-editor evidence in
`artifacts/editor-qualification/native-installed-KBbwrh/` and
`installed-editor.log` retained the activation failure: the loader rejected the
installed payload after VS Code added its own `__metadata` property to
`package.json`. Inspection found that manifest mutation as the only inventory
mismatch. The corrective scope is to normalize only the editor-owned `__metadata`
property for manifest verification while retaining the rest of the update
inventory and payload checks. The corrected regression, second package and
selected actual installed-editor results follow below; the failed first VSIX
hash above is not the accepted candidate. The fresh native runtime is
unchanged and retains its original producer/source identity when reused by that
extension-only repackaging.

### Corrected Windows candidate and selected installed acceptance

The metadata correction's complete extension suite passed **915 tests / 0 failed /
21 optional cases skipped**, with **936 total selected cases** and test-run
duration **7.147 s**, in `extension-tests-metadata-fix.log`. The complete Python
run passed **487 tests in 29.992 s**, with **9 skips**, in
`python-tests-metadata-fix.log`. A focused package contract run additionally
passed **15 tests in 3.584 s**, in `package-contract-final-authorized.log`.
Its first focused attempt retained a per-process Git `safe.directory` ownership
configuration error; the authorized process configuration resolved that access
limitation without a source-code correction or global Git setting change.

The corrected package command reused the exact unchanged native producer and
archive above:
`python scripts/package-extension.py --target x86_64-pc-windows-msvc --native-dir
artifacts/local/1.8.12-explorer-20261107/native/publish --output
artifacts/local/1.8.12-explorer-20261107/vscode-candidate2`. It passed in
**18.350 s**, including the final compile and VSIX checks. The receipt's before
and after native source seals still match the **978-input** seal above; this was
extension repackaging, not another native build.

The accepted metadata-correction checkpoint VSIX was
`artifacts/local/1.8.12-explorer-20261107/vscode-candidate2/publish/airk-1.8.12-win32-x64.vsix`,
**77,681,973 bytes**, SHA-256:
`129958a2b7cc044e420596be0a9bb5c3a8b4e21335574b3bfc82208bc4e9efd1`.
Its manifest binds the same native archive
`9e1b0db7cd30a191f1bea6f8d8e388260257b129d54e1bb95c2d8b3c83514865`.
The uncommitted-source/dirty-checkout disclosure above still applies; the base
Git commit is not a claim that these changes were committed.

The actual installed production Explorer preflight passed in
`artifacts/installed-explorer-preflight/run-ZYigdc/result.json`, against that
exact VSIX hash. It installed the real package, activated the production
controller graph, accepted VS Code's installed-manifest metadata, exercised a
same-version CSS replacement through the loader, forwarded the original API,
kept a singleton panel, kept a closed panel closed and reopened it explicitly.
The receipt records the extension-host PID and reports `window_reloaded: false`.
Native execution was disabled: this proves production
activation, tab ownership and UI asset handoff, not DOM interaction, native
runtime lifecycle or a user-profile install. The initial attempt in
`run-r2V7Mc/result.json` failed a harness assertion that expected installed
metadata through the extension API; the corrected harness inspects the actual
installed manifest on disk. That failed attempt remains separately recorded.

The corrected fictional-entry actual-VS-Code fixture also passed in
`artifacts/editor-update-preflight/run-JbTRyQ/result.json`, now including an
installed metadata rewrite. It remains a loader/panel fixture with
`native_service_exercised: false`, separate from the production-package
checkpoint above. The selected actual installed **untrusted** scenario passed
in `artifacts/editor-qualification/native-installed-07pAnQ/summary.json` and
`installed-untrusted-editor.log`: VS Code **1.145.2**, installed AIRK **1.8.12**,
Restricted Mode native handlers and actual Indexing forms blocked, with fixture
source hashes preserved. This selected scenario is not the full trusted native
installation/upgrade/editor campaign.

`python scripts/dependency-audit.py --offline --output
artifacts/local/1.8.12-explorer-20261107/dependency-audit-offline-authorized.json`
completed in **2.656 s** with all requested cached Rust checks passing. The
receipt explicitly remains **limited**: strict offline mode did not check npm,
fresh/yanked coverage or fresh online advisories. An initial sandbox attempt
could not read the required cached checkout; the authorized read-only run
completed against the existing cache. Neither that access correction nor this
limited result is a fresh security audit.

### CI and historical Mac review

Review of the complete package/promotion path found that signing rewrites the
native wrapper while the extension update inventory still named its previous
hash. The correction updates only that `native/manifest.json` entry during signed
promotion and retains the other inventory entries and complete payload checks.
The existing VSIX verification gates now validate the complete update graph
automatically. **54 scoped contracts passed**, and the already corrected Windows
VSIX passed the stronger complete-inventory verification without repackaging;
its accepted SHA-256 remains unchanged. The primary independently reviewed the
combined packaging/promotion diff and validation evidence.

The read-only hosted review is retained in
`hosted-mac-review/summary.json`. It preserves the earlier Intel failures,
the correction in [PR 56](https://github.com/iokaio/airk/pull/56), and later
successful ARM and Intel build/validation runs on their actual source identities:
[run 41398425043](https://github.com/iokaio/airk/actions/runs/41398425043) and
[run 41551033290](https://github.com/iokaio/airk/actions/runs/41551033290).
The primary confirmed that corrected main commit
`0dbd2f0729d158a73686ce726a424555a9bbb3f8` is an ancestor of the current HEAD.
The historical 1.8.7 signing/package evidence in
[run 41405017894](https://github.com/iokaio/airk/actions/runs/41405017894)
retains its own version and source. None of these historical runs qualifies the
dirty 1.8.12 working source; no hosted workflow was dispatched or rerun for this
review, and no current hosted Mac qualification is claimed.

### Final Windows UI package

The final help/UI correction was packaged into
`artifacts/local/1.8.12-explorer-20261107/vscode-final/publish/airk-1.8.12-win32-x64.vsix`.
The package command used the unchanged native archive above and passed in
**18.398 s**. This final Windows installer is **77,682,291 bytes**, SHA-256
`64609d630765a161fd84d887daa4253ea0ea387c33fcf0805e1e9cb7f1c8d54b`.
Earlier candidate hashes remain historical checkpoints rather than being
relabeled as this artifact.

The final complete extension suite passed **915 tests / 0 failed / 21 optional
skips** (**936 total selected cases**; test-run duration **7.051 s**) in
`extension-tests-help-final.log`. Actual production installed-Explorer preflight
passed against this final hash in
`artifacts/installed-explorer-preflight/run-B81lfr/result.json`, with production
activation, installed metadata rewrite, same-version CSS handoff, API forwarding,
singleton/close/reopen behavior and no window reload. Native execution remained
disabled, retaining the earlier preflight's limited scope. The final actual
installed Restricted Mode scenario also passed in
`artifacts/editor-qualification/native-installed-35op5w/` and
`installed-untrusted-final.log`.

### Current Mac Neo ARM build and selected acceptance

The Neo batch used the same **978-input native source seal** recorded above and
the existing isolated checkout target
`/Users/tylerjensen/airk-builds/explorer-1812-20261107-1794068515/target`.
Its completed receipts and preserved Cargo timing reports are collected under
`mac-neo/` in the evidence directory. The original batch's qualification remains
`incomplete_or_failed` because its optional signing/UI attempts failed; later
completed receipts provide the separate corrected evidence below.

| Neo command/scope | Completed result |
| --- | --- |
| `cargo fmt --all --check` | Passed in **2.731 s**. |
| `python scripts/dev-check.py -p airk-cli --all-features --skip-fmt --target-dir <existing-target> --incremental off --offline` | Scoped CLI/all-targets Clippy passed in **175.773 s**; `CARGO_INCREMENTAL=0`, timings retained. |
| `python scripts/build-service-runtime.py --profile release --destination artifacts/neo/runtime` | Fresh ARM main/helper producer passed in **1,099.454 s**, with release-profile incremental defaults and two Cargo timing reports. |
| Initial model and service-client smokes | Passed in **24.562 s** and **93.285 s**, respectively. |
| Initial installed-runtime lifecycle | Passed in **231.497 s**. |
| Final complete Mac extension suite | **905 passed / 0 failed / 31 optional or platform skips**, **936 selected cases**, in `extension-tests-help-corrected.log`; wrapper **14.350 s**, tests **8.659 s**. |
| GUI Developer ID signing of the app and launcher | Passed in **10.181 s** using the unchanged original native producer. |
| Signed runtime lifecycle and model/service-client smoke | Passed in **230.804 s** and **93.112 s**. |
| Signed native archive and signed ARM VSIX packaging | Passed in **15.559 s** and **11.385 s**. |
| Actual installed production Explorer with the signed VSIX | Passed in **8.809 s**, with native execution in the editor disabled. |

The initial SSH signing attempt and explicitly authorized SSH retry remain
failures (**1.231 s** and **1.605 s**). The later successful attempt ran in the
user's existing GUI/Aqua session; its receipt reports no keychain settings
changes and no notarization or publication. It reused the original native
producer rather than rebuilding it. Strict signature and copied-launcher
verification passed for **Developer ID Application: Ioka LLC (5PX7D99PXA)**.
The signed runtime lifecycle and model checks are separate from the editor
preflight, which exercises the installed production controller/UI with native
execution disabled.

Initial editor attempts retained the wrong Mac executable-path failure,
bootstrap timeout and later long-profile/harness failures. The corrected
short-profile actual production and separate fictional-bootstrap runs passed;
`ui2-result.json` identifies those as harness-only changes without a native
rebuild or production VSIX repackaging. The later signed production installed
result is `mac-neo/developer-id-final/evidence/signed-ui-result.json`. These
completed checks retain their own exact artifact scopes; earlier failed
receipts were not rewritten.

The final signed ARM artifacts are identified in
`mac-neo/developer-id-final/packages/`:

| Artifact | SHA-256 |
| --- | --- |
| `native-signed-gui1/airk-1.8.12-aarch64-apple-darwin.zip` | `ffbc21c9b0ff4efdb37e7201dc087681d81a9cdc7e65d1672df52f5449211e4e` |
| `vscode-signed-gui1/airk-1.8.12-darwin-arm64.vsix` | `d0401fc233f263340c3e80fc8b749bb87f1c1a8979885e5887778231c9c3be9b` |

The Mac native manifest also records **`dirty_checkout: true`** and the original
source seal. Evidence and packages were retained outside the Cargo target.
Cleanup receipts record no compiler descendants before cleanup, successful
`cargo clean`, target absence and preserved source; GUI job cleanup also
completed. No routine Windows target cleanup was performed.

### Exact public signing-certificate identity and local guard

Public-certificate inspection found the same leaf DER SHA-256 on Neo's app and
copied launcher:
`4260bd90b8623e45da6d67144a2bff4fd8fa0a8186c7281c0125e39877c3f5c1`.
`mac-neo/public-certificate-identity/certificate-identity.json` retains the
strict-signature observations. The independent historical archive comparison in
`hosted-mac-review/signing-identity/provenance.json` matches that exact leaf and
public key to the signed **1.8.7** ARM archive from its original CI signing run.
The comparison used public certificates from signed images; it did not read or
export private keys. A shared Team ID or display name alone was not treated as
an exact certificate match.

The local CI certificate guard pins that public leaf identity, and the primary
independently reviewed the guard diff. **52 scoped guard/signing/promotion tests
passed in 8.970 s** in `mac-certificate-guard-final-tests.log`. The complete
Python rerun after the actual output-certificate probe and syntax correction passed
**494 tests: 485 passed / 9 expected
skips / 0 failed**, with **28.406 s** unittest time and **28.911 s** wrapper time,
in `python-tests-certificate-final.log`. Repository checks passed for **1,838
public files**. The new guard has not run in a hosted signing workflow for this
dirty 1.8.12 source; historical certificate agreement does not substitute for
that future check.

The actual public-only helper probe on Neo passed its imported-identity check
against the exact certificate. Its output-certificate step then exposed a
`codesign` argument syntax error: certificate extraction requires
`--extract-certificates=<prefix>`. That failed first probe remains retained.
The corrected actual probe passed in `mac-neo/ci-guard-proof2/proof.json` using
helper SHA-256 `c0c3d23e5da7c5ebe1c497ac5b16b05d20ff58f286a869c33b158e32bf208ad3`.
It confirmed the exact imported-identity SHA-1 and verified the public leaf on
the app, main, helper, copied-launcher image and inference library. The receipt
reports no signing, import, unlock, password or keychain setting changes, no
compiler processes and an absent Cargo target. The exact new `codesign -R`
requirement, including Apple anchor, Team ID, Developer ID OID and SHA-1 leaf,
also passed on all five signed paths; public command results are retained in
`mac-neo/exact-leaf-requirement-proof.json`. The complete Python rerun after
the syntax correction passed as recorded above. Neither probe changed the already signed
images or their separate signature/install/artifact observations above.

### Issue 66 installer review (7 November 2026)

The [specific issue comment](https://github.com/iokaio/airk/issues/66#issuecomment-6710990197)
was reviewed against live source and the retained signed Neo artifacts. Its
reported defect is loss of bundle signature context after copying the sealed
main executable; matching certificates alone cannot fix it.

**Behavior and platform impact.** This launcher fix changes Mac executable
handling; Windows and Linux keep their existing launcher and process behavior.
Before app sealing, the builder creates `airk-launcher-image` and signing gives
it its own Developer ID signature. The installer copies that approved image to
the stable `airk-launcher` path, checking its recorded digest. Services and
workers execute inside `AIRK.app`, preserving their bundle signature context.
User-facing commands and argument forwarding stay the same. Upgrade and repair
replace a recognized old launcher; unknown or modified files remain preserved
and reported. This implements the remedy described in
[issue #66](https://github.com/iokaio/airk/issues/66), including the
[original review comment](https://github.com/iokaio/airk/issues/66#issuecomment-6710990197).

**Release verification across platforms.** The related
[issue #67](https://github.com/iokaio/airk/issues/67) concerns GitHub and
Marketplace serving different native payloads. Normal Marketplace publication
uses the authenticated signed GitHub packages. The new post-upload gate downloads
all four served platform packages and compares their complete extension payloads
with those verified inventories. Marketplace signing metadata outside the
extension payload may differ. Missing packages receive bounded propagation
waits; a payload or channel mismatch fails verification. This changes CI for
Windows, Linux, Mac ARM and Mac Intel, without changing their installed runtime
behavior. Verification cannot roll back an upload that already succeeded.
This local workflow correction has not been exercised by a new hosted publication.

| Required behavior | Current finding |
|---|---|
| Service and worker executables stay in the sealed app | Current Mac process-role selection returns the bundled main; service, indexing and source workers use that selector. Inference uses the bundled helper. |
| Launcher has its own signature before the enclosing app is sealed | The builder prepares a distinct image before sealing. Signing signs the image and other components before the app, and checks a post-seal copy. |
| Installation and repair use approved image bytes | The installer selects the inventoried standalone image for current Mac layouts, rechecks its digest and retains regular-file, ownership and durable repair checks. Unknown or modified files remain protected. |
| Actual Developer ID negative and positive copy controls | Passed on Neo: the byte-identical sealed-main copy failed strict verification with `invalid Info.plist` and was not executed. The approved standalone copy passed strict verification and `--version`. Receipt: `mac-neo/sealed-main-negative-copy-proof.json`; temporary files removed and source bytes unchanged. |
| Failing historical installation upgrade | A separate recovery preflight passed with the authentic signed 1.8.7 payload and its real stage-produced launcher receipt. Its broken copied launcher failed strict verification without execution. Current-only activation repaired it to the approved signed standalone image; two client preparations, version/signature/digest checks, service health and MCP lifetime/reconnect checks passed. This is recovery from a staged historical payload, not a full affected-editor upgrade. |
| Full distributed editor acceptance | Still open: the installed production Explorer check disabled native execution. Native Developer ID lifecycle and indexing/search/MCP checks passed separately. These do not establish the complete signed/notarized editor setup scenario on a clean standard-user account. |
| Useful failure reporting | Early exit records `exited_before_health`; native error text retains termination status, and the extension preserves process signal/stderr rather than inferring a signature failure from SIGKILL. Persisted bootstrap JSON retains only exit code, so a Unix signal is not independently recorded in that field. Affected-editor failure reproduction remains open. |
| GitHub/Marketplace native payload parity | Current normal publication stages the authenticated signed GitHub VSIX without rebuilding an ad-hoc runtime. The audit found and corrected a missing readback after fresh Marketplace upload; 27 scoped version/promotion tests passed in 2.645 s (`marketplace-readback-tests.log`). The full Python rerun passed: **498 tests, 489 passed, 9 expected skips, 0 failed**, in **26.841 s** (27.350 s wrapper), retained in `python-tests-marketplace-final.log`. No current published-channel comparison has occurred. |

Historical recovery evidence is retained in
`mac-neo/historical-recovery2/historical-recovery2-result.json`. The authentic
old launcher matched the published main hash
`d28828af6eb8275c55748bc5202e9a7e642e8568ff915eaa7284e343bb56a066`;
the repaired launcher matched the approved standalone image
`d72ea573c4816b35c964a8dfa93ed74fbd8d5a5ab87a2102cf1390ce486b57c2`.
Old staging took **13.863 s**, current staging **14.794 s**, current-only
activation **17.954 s**, two client preparations **2.482 s / 2.512 s** and health
**0.870 s**. The MCP check covered abrupt client loss, concurrent-session
preservation and normal EOF. The old service was never activated. Owned
shutdown/uninstall succeeded, the sentinel was preserved, the private fixture
was removed and the Cargo target remained absent. The initial noncanonical
Mac `/var` temporary-path fixture failure and its diagnostic remain retained
under `mac-neo/historical-recovery1/`; the corrected run canonicalized its own
temporary root without weakening runtime path checks. No new indexing/search
campaign, signing, notarization or keychain change occurred in this preflight.

No additional production launcher-source or process-role defect was found in
this review. Current Intel, notarization/staple, downloaded-artifact Gatekeeper,
historical affected-editor upgrade and published-channel acceptance remain
separate gates. The [release verification procedure](../release-verification.md#mac-installed-artifact-and-channel-acceptance)
now names these checks explicitly. Neither issue 66 nor issue 67 is established
as resolved in a public release by this local review.

### Local MCP contention review (7 November 2026)

The maintainer requested investigation after the currently running **1.8.11**
MCP status returned typed `selection_busy`, `retryable: true`,
`action: repeat_same_request` and `retry_after_ms: 25`. Live code maps either
collection-selection lock contention (after its 100 ms admission wait) or typed
shared-storage accounting contention to this existing safe-read retry category.
It does not establish exhausted configured request capacity or index corruption.

Subsequent status and diagnostics succeeded without service intervention on the
same configured project and collection. Publication advanced through generations
9828 and 9829 as semantic readiness changed from 1763/1764 to 1764/1764 artifacts.
This is consistent with temporary contention, but does not prove which lock or
owner caused the earlier rejection. The reply merges the two typed causes and
retained logs have no matching event, so exact retrospective attribution remains
unavailable. No persistent malfunction or additional production defect was
established; this investigation changes no retry, scope or mutation policy.

### Remaining checks

The routine Windows Nextest and serial doctest runs above are complete. The
broader native/editor campaign and four long workload campaigns remain outside
that selected scope. The complete Python rerun after the promotion correction
completed **489 tests: 480 passed, 9 expected skips, 0 failed** in
**27.079 seconds**; its complete
log is `python-tests-promotion-final.log`. This supersedes the earlier Python
checkpoints for the final package/promotion source.
That 489-case checkpoint predates the certificate-guard tests. The 494-case
rerun after the actual-probe syntax correction superseded it for that source.
The latest full rerun after Marketplace readback changes completed **498 tests:
489 passed, 9 expected skips, 0 failed** in **26.841 s** (27.350 s wrapper),
retained in `python-tests-marketplace-final.log`.
The selected current Neo ARM build, signing, runtime, package, editor and cleanup
checks above are complete. They do not establish current Intel, oldest-supported
macOS, notarization, transfer/Gatekeeper, Linux, fresh online advisory or the full
release/publication matrix. The corrected public-only guard probe passed locally;
the hosted signing run remains separate. No notarization was submitted and
nothing was published.

The earlier validation checkpoints authorized local Windows and Mac Neo builds
and CI review, with no commit or publication at that time. On 7 November 2026,
the maintainer subsequently authorized committing and pushing this work on
`feat/finetuning`, explicitly without a PR, then creating a fresh local-test
Windows VSIX. Version 1.8.12 remains the already requested working-version bump;
it is not incremented again for this commit. Native runtime reuse requires the
unchanged 978-input source seal and complete artifact verification, retains the
original compiled build identity, and is not described as a fresh Rust build.
No merge, Marketplace publication or GitHub release is part of this local-test
packaging request. Installed-editor, other-platform and publication qualification
retain their separately recorded evidence and limits.
