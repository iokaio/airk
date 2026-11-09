# Build and review the Windows app and VS Code extension

The current local manual-test working version is **1.8.14**. Its [working-build record](releases/release-1.8.14.md) records notice dismissal, automatic extension replacement, current validation and local Windows package evidence as it is collected. Manual installed-editor acceptance and later 1.9.0 promotion remain separate.

The [1.8.7 predecessor record](releases/release-1.8.7.md) consolidates the earlier unpublished 1.8.0-1.8.6 work and retains its dated preparation and publication evidence. At that preparation checkpoint, the stable version was [1.7.6](releases/release-1.7.6.md). These and the prior 1.4.x build records retain their original scope.

Version 1.8.14 uses one native executable with embedded MiniLM and static CPU ONNX Runtime. MCP clients connect to the service's authenticated loopback HTTPS endpoint; there is no persistent launcher or semantic helper process. Native index/source workers still have their existing process isolation. The Windows service manager starts the approved main image directly. See [local HTTPS endpoints](local-https-api.md) for credentials, CA trust, discovery and port fallback.

Recognized existing agent configurations can migrate only when their original saved authority and configuration preimage are verified. Ambiguous configurations require review; migration must not invent a grant or narrow a multi-project connection to the active folder. Restart external clients after their connection changes. Serialization tests are not installed-client qualification; retain actual editor and platform results separately.

For this checkout's **scoped developer Codex setup**, `scripts/setup-local-test.py`
accepts an explicitly approved modern runtime, its existing installation, a
stable connection ID and the complete native-reviewed selection:

```console
python scripts/setup-local-test.py --runtime-dir <verified-runtime> --approved-sha256 <inventory-sha256> --install-dir <owned-installation> --connection-id <stable-native-id> --selection-file <protected-native-selection.json> --report artifacts/local/<run>/agent-setup.json
```

It writes only this checkout's project configuration after a generic HTTPS
smoke; it does not index, invent grants, configure the actual Codex host's trust
or load credentials into existing processes. Owned stable/universal predecessor
entries and pinned predecessor executables require native path/hash lineage from
the approved installation and their exact original scope. An
explicit Project or uniquely resolved root uses `--registration-file` instead;
a frozen catalog/combined entry requires its complete matching selection file.
An ambiguous root-only multi-index connection is preserved and blocked here,
rather than narrowed. Use the installer migration with its original captured
union receipts for that case. Arbitrary old executable paths are not adopted.
Historical split `airk_explorer` declarations block this scoped helper before
enrollment or cleanup because it cannot prove their private descriptor authority;
use the installer migration for complete family consolidation. This is a format
and ownership boundary, not a requirement to update through an intermediate
release. One genuine predecessor regression does not qualify every old format.

This developer helper owns one project Codex file and cannot prove that other
client files migrated. Without explicit cleanup it reports
`configuration_saved_cleanup_pending` and exits unsuccessfully. After checking
all known client declarations, add `--retire-obsolete-executables` (the older
`--retire-legacy-launchers` spelling remains an alias). After the verified save,
the helper requests native `runtime retire-legacy-launchers` once, then only after
its complete receipt requests `runtime retire-previous`, both bound by
`--install-dir <owned-installation> --expected-active <inventory-sha256>`.
The modern retirement contract removes all approved nonselected payload versions,
including orphaned versions beyond the previous-runtime pointer. Both
legacy-to-modern and modern-to-modern upgrades preserve the selected runtime,
sources, indexes and settings. Complete native receipts or blocked/unknown
outcomes remain in the report; cleanup failures are not retried as mutations.
This explicit development-tool option is not a VSIX installation option:
the actual installer must automatically migrate the complete known configuration
set and retire ownership-proven old launchers, with incomplete cleanup surfaced.

Historical October 25 manual-test checkpoint: cleanup, help and live-status changes used 1.4.0. The later 1.4.1 installer candidate is also historical; follow [versioning](versioning.md) and [current candidate notes](releases/release-1.8.14.md). Those earlier Windows checks do not replace installed-editor acceptance or native macOS/Linux qualification. The unified source catalog now advertises 29 tools; see [the MCP catalog](unified-mcp.md).

Historical October 25 onboarding follow-up: automatic trusted-workspace setup was covered by extension regressions and package closure checks. The current working build uses [explicit index setup and search selection](service-project-setup.md); opening a trusted folder does not register or index it. Earlier installed-editor receipts and the manual-dialog harness below retain their original VSIX scope.

The main executable embeds both pinned INT8 MiniLM exports and links the pinned CPU ONNX Runtime 1.22.1 archives. The historical dynamic `provision-onnx-runtime.py` output cannot satisfy this build. Cargo does not provision the model or ORT libraries.

From the repository root in PowerShell 7, an existing verified input cache can be used without downloading model/ORT inputs:

```powershell
pwsh -NoProfile -File scripts/build-windows.ps1
```

For initial provisioning, explicitly allow downloads and the pinned static native build:

```powershell
pwsh -NoProfile -File scripts/build-windows.ps1 -DownloadInputs
```

`-DownloadInputs` provisions model assets and the pinned native tools/source, then builds static ORT with `--target all re2 --parallel 2`. The default persistent native cache is `target/static-onnx-runtime/x86_64-pc-windows-msvc`; use `-StaticOrtDirectory <absolute-directory>` for another verified cache. Without the switch, missing or invalid inputs fail verification. Cargo/npm dependency restoration can still require network access; this switch controls model/ORT provisioning only.

The provisioner emits `static-onnx-environment.json`. `static-runtime-environment.py` revalidates that report against the actual archives, notices, receipts and link closure, then supplies exactly `LIBONNXRUNTIME_NO_PKG_CONFIG=1`, `ORT_SKIP_DOWNLOAD=1`, `ORT_PREFER_DYNAMIC_LINK=0`, `ORT_LIB_LOCATION=<cache>/b` and `ORT_LIB_PROFILE=Release` to build children. It does not change the caller's environment. A historical cache missing the excluded `re2` archive needs the explicit `provision-static-onnx-runtime.py --destination <cache> --target x86_64-pc-windows-msvc --supplement-link-closure` operation before verification; do not rename another library or fabricate a receipt.

The script builds the Windows x64 release executable and matching VSIX, runs the configured Rust/script/extension checks and native model retrieval gate, and verifies package closure. The runtime builder checks the actual Windows imports for external ORT DLL dependencies and runs `airk.exe --verify-bundled-models` and `airk.exe --verify-model-smoke`. Those checks and static input identities are retained in the runtime build receipt. A failed check stops packaging. No signing or Marketplace credentials are needed; the script does not publish.
The existing npm test step compiles the current extension before Python packaging
contracts inspect its runtime module closure. Stale output from an earlier build
cannot substitute for that compilation, and the wrapper adds no duplicate compile.

Model assets, tokenizers, metadata and licenses are embedded in the main executable. Indexing still requires reviewed setup and accepted work; opening a workspace does not start indexing. Opt-out, Pause and manual-indexing choices remain authoritative.

## Developer prerequisites

- Windows x64 and PowerShell 7.
- Git, rustup and the repository-pinned Rust toolchain.
- Visual Studio C++ tools with MSVC 14.44.35207 and Windows SDK 10.0.26100.0 for the pinned static native build.
- Node.js 24.15.0 (the CI version) and npm; VSCE requires Node 22 or newer.
- Python 3.11+ via `py` or `python`, for notices and archive verification.
- Initial network access for locked Cargo/npm dependencies and missing Rust components.
- VS Code 1.140+ with its `code` launcher on PATH, only for editor testing/review.

The extension runtime needs none of the developer tools. Its VSIX bundles AIRK, and the editor supplies its JavaScript runtime. The executable is unsigned and the extension defaults to the stable channel; oldest-OS and installed upgrade/rollback qualification remain separate work.

## Results and repeat builds

Every run creates a fresh ignored `artifacts/local/<timestamp>-<id>/` directory:

| Path within the run | Contents |
|---|---|
| native/publish/ | Native ZIP, native manifest and SHA256SUMS |
| vscode/publish/ | Matching VSIX, extension manifest and SHA256SUMS |
| build.log | Original local build/check output |
| result.json | Native/VSIX paths and whether checks/editor tests ran |
| vector-check.json | Real-model/DiskANN verification and process measurements |

The verified schema-2 main-only runtime is in the run's `runtime/` directory. The script prints its main executable path, the VSIX path and the build directory. It leaves prior run directories intact and never deletes index state or source files. Intermediate staging and compiler outputs are not distributed.

If you deliberately want to omit routine checks during an iterative build:

```powershell
pwsh -NoProfile -File scripts/build-windows.ps1 -SkipChecks
```

Compilation, real-model/DiskANN verification, embedded-model and static-import validation and native/VSIX archive verification still run. `result.json` explicitly records that routine checks were skipped and points to the vector verification report.

For constrained-CPU checks against a newly built runtime, `scripts/ci-local.ps1` uses the same verified static cache and accepts `-DownloadInputs` / `-StaticOrtDirectory`. Reuse an already sealed runtime explicitly with `-SkipBuild -RuntimeDir <runtime-directory>`; this does not qualify it as a fresh build. Fresh builds use a new run directory and preserve previous outputs.

## Connect this checkout for local agent testing

Use **Connect Coding Agent** after explicitly reviewing index setup and search scope. The current setup publishes an HTTPS URL and a protected credential reference, preserves other server settings, and explains local CA trust. Do not create a new stdio launch entry for 1.8.14. The native CLI, Workbench and MCP share the same reviewed authority and complete read contracts. See [service project setup](service-project-setup.md) and [local HTTPS endpoints](local-https-api.md).

## Review in an isolated editor

```powershell
pwsh -NoProfile -File scripts/build-windows.ps1 -OpenReview
```

This installs the generated VSIX in a separate extensions directory and opens a private VS Code profile with a copy of the public project-a fixture, its own native installation and separate index data. It does not install into or alter your normal profile. Grant that fixture workspace trust, then select the AIRK status item. Try Index Now, Reindex, indexing modes, pause/resume, storage selection, collection status and Verify Native MCP.

To install the reviewed package into your normal profile later, use **Extensions: Install from VSIX** and choose the printed VSIX path. Installation is not performed in the normal profile by this script. VS Code skips a VSIX whose version is already installed, so a rebuilt package with an unchanged version looks installed while the old code keeps running. Use `scripts/install-local-vsix.ps1` (or `scripts/install-local-vsix.sh`) instead. It runs `code --install-extension <vsix> --force` and verifies that the installed code matches the VSIX. A forced reinstall is an update, not an uninstall, so AIRK's uninstall cleanup does not run. A running VS Code on Windows locks the installed folder, so close every VS Code window before a same-version reinstall; a higher version installs beside the running one without closing it. The active 1.8.14 loader handles later detected replacements by reloading AIRK and reopening its singleton Explorer automatically. A window still running an older extension needs VS Code to activate the new loader first; use **Restart Extensions** in VS Code’s Extensions pane for that one-time transition.

## Automated packaged-editor smoke

**Current implementation and acceptance (7 November 2026):** the native-enabled
`check-native-editor.cjs` journeys now use **Search scope**, **Index library** and
**Index sets**. The earlier saved-association/explicit-grant UI mismatch is a
retained failed checkpoint, not the current implementation. The first migrated
actual `same` run then failed because a native `SourceChangeBusy` response with
one queued job was classified as `unexpected_service_response`. The host now
validates that exact pre-insert refusal, preserves its complete body and reports
queued/running work through `source_change_jobs_pending`, without replaying the
mutation. Source regressions passed; a new native-enabled Mac GUI run with the
corrected host is pending. Do not report the implemented journeys as passing
current GUI acceptance before that complete run is collected.

Before its first permanent exclusion review, the harness makes a bounded,
read-only observation of the exact index/backing, service and worker generation
until initial indexing is idle, with a **90-second deadline**. This is a dated
qualification precondition, not permission to mutate or a retry of refused work.
Native installer/client/MCP and bootstrap/tab checks retain their separate scope.

```powershell
pwsh -NoProfile -File scripts/build-windows.ps1 -RunEditorTests
```

The current test installs the verified VSIX into isolated profiles, using the
installed editor and matching CLI. It reviews native index setup and a permanent
exact `hidden.ts` exclusion, creates an index set with the current checkboxes,
and explicitly applies a search choice before testing authenticated HTTPS MCP,
citations and native isolation. The actual provider definition is checked against native connection information; its private CA is added only to the owned test client. Revocation requires the exact completed native index-set deletion and one retained-session call. A delivered typed refusal is retained in full; an empty TLS reset counts only when paired with that completed native fence, never by itself. Only generated fixtures and private profiles
are used; the normal user's settings and trust are untouched.

A passing run of a matching fixture is isolated installed-VSIX acceptance. Marketplace, remote-host, upgrade/rollback and clean-machine qualification remain separate. It does not exercise any paid coding model. VS Code startup may show a temporary test window even though the process is launched with its console hidden.

The current production bootstrap and tab-handoff preflight is narrower:

```console
node scripts/check-installed-explorer.cjs "<Code.exe>" "<resources/app/out/cli.js>" "<verified Windows VSIX>"
```

It exercises the actual installed product API/controller with native execution
disabled: platform manifest metadata, one Explorer after repeated opens,
close/reopen and same-version interface replacement. It does not claim index
creation, checkbox selection, native service installation or the broader native-enabled
journeys. The working-build release record retains its actual results.

`-RunEditorTests -OpenReview` runs acceptance first, then opens a separate private profile for review. The review-dialog scenarios explicitly set `airk.cli.autoInstall: false` and select manual native indexing. Runtime onboarding and agent-entry migration are covered separately by `check-agent-install.cjs`. Opening a trusted folder does not create an index; this fixture must perform explicit reviewed setup and apply its search selection.

## Installed extension and two-window qualification

After packaging a verified VSIX and compiling the extension tests, run:

```console
node scripts/check-editor-windows.cjs "<Code.exe>" "<resources/app/out/cli.js>" "<verified Windows VSIX>"
```

Use the installed editor executable and its matching CLI script (the installed `bin/code.cmd` identifies the latter, which can be under a versioned directory). This test installs the VSIX and a tiny local test-driver extension into a fresh directory under ignored `artifacts/editor-qualification/`. Ordinary editor windows use private user profiles and an explicitly private shared-application-data directory. The driver is not shipped in AIRK packages. Only generated fictional workspaces are edited.

The current scenario contracts are:

- **same:** two windows share the existing native index, observe peer edits and
  stage a real index-rename race. The stale reviewed request remains unapplied;
  its exact retained intent and null native receipt are inspected without replay.
- **different:** two windows use distinct indexes in the same private storage
  root, observe edits and check positive/negative index isolation.
- **multi:** two opened folders do not create or select both indexes. After an
  explicit checkbox choice, a real reload must revalidate and restore the
  **exact saved applied selector**, without widening it.
- **usage:** scoped usage stays separate from explicit entire-index
  administration; clearing one index's ledger preserves the other index and its
  evidence scope.
- **untrusted:** Restricted Mode rejects native handlers and leaves Indexing
  controls disabled, with no advertised native evidence definitions or storage
  creation.
- **legacy:** requires the separately validated previous-version native fixture;
  it is not qualified by a current-source mock, bootstrap check or merely
  supplying an old VSIX.

Trusted journeys review the exact source/storage binding, explicitly exclude
`hidden.ts` through the permanent source-policy flow and check its read denial,
path traversal refusal, citation hashes and complete advertised tool catalog.
Deleting the selected **index set** must fence the already retained MCP session;
EOF or a timeout is not native withdrawal proof. These checks replace the removed
partial-grant/saved-association UI; they do not add grant controls back to the UI.

Profiles use an explicitly private Code process and short canonical Unix paths
where applicable, with shell integration disabled. Screenshots use loopback-only
debugging; receipts retain extension-host process IDs and native install identity.
Cleanup first closes fixture editors and obtains a qualified Stop for the exact
owned namespace/runtime. Only newly created, exact-owned account uninstall
receipts captured by the fixture may then be removed, with evidence retained.
Unknown ownership, uncertain Stop or cleanup failures retain the files and
profiles for inspection. This grants no authority to clean unrelated account
receipts, change shell profiles or affect the user's normal installation.
OS-native dialogs, external coding-agent approvals, Marketplace, upgrade/rollback
and full remote/platform qualification remain separate.

Installed runs explicitly launch the separate private driver through its own development path. VSIX installation creates an `extensions.json` registry, so copying the driver directory afterward does not register it. AIRK resolves from the verified installed VSIX directory; the driver is separate and is never shipped in the package. The receipt must report `staged: false` and every target window must report `installed: true`.

Before the final packaging gate, the same harness accepts an allowlisted development stage. `package-extension.py --stage-only` writes its hashed stage receipt without creating a VSIX. Pass its `stage/` directory as the third argument to `check-native-editor.cjs`. This tests the staged runtime, while installed-VSIX acceptance remains a later distinct gate.

On October 17, 2026, AIRK 1.1.1 passed all four scenarios in VS Code 1.142.0 on Windows, across six installed-editor windows. The verified VSIX SHA-256 was `4265b3f4a90a8e15a2357253204a68fd903f147a9c5448f411b98e7efbe77dee`. Both same-workspace windows reported the same collection ID; different-workspace windows reported distinct IDs and never returned the other collection's evidence. The multi-root storage change preserved the original index. The untrusted window launched no index and exposed no MCP definitions. Each window used a distinct private profile while sharing AIRK storage with its scenario peers; shared-profile settings synchronization was not tested.

Repeating the earlier package exposed intermittent concurrent-start failures: a live collection writer incorrectly blocked the scheduled request, and quota accounting could fail when a managed temporary path disappeared during enumeration. The qualified package defers live-lock contention without losing the queued revision and tolerates disappearing managed entries while preserving growth reservations. Native regression tests cover both cases. Startup screenshot timing was also corrected in the harness so an untrusted window has time to create its webview content.

The 1.1.3 harness also includes a `usage` scenario. It clicks the installed report's date selector, exports CSV and Markdown through VS Code's built-in simple file dialogs, imports a fictional measurement through preview/confirmation, enters model prices, toggles recording, opens two collection reports, cancels and confirms clearing, and rejects an old panel after a storage change. Native readback verifies the other collection's imported measurement survives and the source files/indexes remain intact. The untrusted scenario also rejects the reporting command. These test profiles select VS Code's simple dialogs; OS-native dialogs and a live trust-revocation/reload remain separate from these checks.

To run just the reporting and untrusted scenarios, set `AIRK_EDITOR_SCENARIOS=usage,untrusted` in the harness process environment. The default names `same,different,multi,usage,untrusted`; the former `legacy` scenario was retired on 9 November 2026 (see below) and is now rejected as an unknown scenario. The test driver, fixtures, screenshots and receipts stay in the private qualification directory; none are packaged with the extension.

On October 18, 2026, the final 1.1.3 candidate passed all five scenarios across seven installed VS Code 1.142.0 windows. See [1.1.3 validation](releases/release-1.1.3.md#installed-vs-code-acceptance) for the package digest, reporting checks and qualification limits.

## Troubleshooting

The pinned quick-status native smoke emits content-free attempt timing/outcome observations separately from confirmed recovery, without a diagnostic warm-up. It retains the first 64 attempts, a separate final confirmation and a dropped count; errors and unrecognized states never become confirmed availability. Native stage/hash counts remain unmeasured. These test observations do not establish a production latency improvement or cold filesystem-cache conditions.

- The script fails at the first unsuccessful tool/check; retain its build.log.
- Missing C/C++ compiler: install the Visual Studio C++ build tools/Windows SDK.
- Locked dependency restoration fails: check network/registry access; do not regenerate locks or enable dependency scripts just to suppress a failure.
- Missing native executable or platform mismatch: use the produced win32-x64 VSIX, not the raw TypeScript package directory. Native binaries are staged only during packaging.
- Writer lock after an interrupted native job: follow [operations and recovery](operations.md); do not delete a live writer's lock.
- Index budget reached: manage unused collections or raise the machine-scoped soft budget, then Resume. Manual indexing intentionally overrides the soft limit.

Hosted equivalents and credentials are described in [CI and builds](ci-and-builds.md).

## Targeted local agent installer check

Run `node scripts/check-agent-install.cjs <Code.exe> <new-vsix> <previous-vsix>` to test a running upgrade, or omit the previous VSIX to install the VSIX into an isolated editor profile with existing Codex, Claude Code and Copilot workspace entries. The check verifies automatic connection migration, preserved settings, repeat repair and a real MCP status handshake.

In a fresh profile it first drives the actual **Set up AIRK on this computer?** review before waiting for native/MCP readiness: readiness intentionally waits for that decision. It captures the real dialog text and screenshot, checks the displayed version/source/storage and persistent effects, and verifies that no native installation, source index or agent-file migration exists before clicking **Set up**. The retained `setup-review.json`, `setup-review.txt` and `setup-review.png` record that boundary. Do not preseed an accepted decision or bypass the review merely to make the readiness wait pass. A previously accepted installation may instead report previous-version readiness, which is recorded separately. Installing the VSIX alone does not qualify this first-use boundary.

It shuts down its private service and retains results under ignored `artifacts/editor-qualification/`. On Windows, compare the observed control directory and owned installation root using the same namespaced-path normalization before the containment check; ordinary `D:\...` and `\\?\D:\...` spellings must not falsely reject the same fixture. Keep the ownership check and shutdown-result assertion: a product assertion receipt alone does not prove successful harness cleanup. It does not change the normal editor profile or qualify replacement of already connected external agent processes.

### Native previous-version upgrade fixture

For a predecessor using reviewed catalog setup, including the 1.8.8 working
build, pass `--fixture-setup reviewed`. This creates one fictional Index,
Project membership and reviewed grant, explicitly indexes it with the old
runtime, and verifies that the new runtime exposes split capability while
preserving the complete original catalog values. The legacy default remains
available for predecessors using historical registration. Existing eligible
indexes do not need a manual policy-contract update just to expose splitting.

Run `python scripts/check-runtime-upgrade.py --runtime <new-native-payload> --previous-runtime <old-native-payload> --output target/upgrade-receipt.json` on Windows or Linux. An optional `--interrupted-runtime <superseded-native-payload>` also checks guarded recovery of an exact older Drain intent. Use independently built payloads with their original manifests; never relabel a binary. The fixture creates isolated source, data, installation and state directories under `target`, starts an old launcher/MCP client, upgrades with that client connected, checks registry migration and source/registration preservation, then uninstalls its runtime. It writes a passing receipt only after teardown succeeds. This is a native process regression, separate from installed-editor qualification and the routine CI matrix.

The fixture creates both roots required by `register`, lets the stable launcher supply installation context, completes the MCP handshake and explicitly closes read-only SQLite connections before teardown. Bounded client startup diagnostics distinguish setup errors from upgrade failures.

### Installed-editor legacy scenario (retired)

**9 November 2026:** the native in-place catalog upgrade (P5) was removed, and
every pre-1.8.16 installation now takes the
[clean break](upgrade-rebuild-fallback.md). The automated `legacy` installed-editor
scenario depended on that upgrade's native roster (`index-catalog-upgrade.json`)
and imported legacy scope, so it could no longer produce or qualify a fixture.
By user decision it was retired: `scripts/prepare-native-editor-legacy.py`,
`scripts/native-editor-legacy.cjs`, the `AIRK_EDITOR_LEGACY_FIXTURE` input, the
`legacyUpgradeCoverage` summary field and their synthetic tests were removed.
Upgrades from old installations are tested manually in installed editors. The
earlier design and evidence remain in the
[fixture compatibility record](testing/fixture-compatibility.md) and Git history.

### Reviewed old-VSIX upgrade helper

`scripts/check-vsix-upgrade-editor.cjs` implements a separate reviewed real
old-VSIX/new-VSIX upgrade journey with a private driver. It has not been executed
for this checkpoint, and its contract tests are not upgrade acceptance.
**9 November 2026:** its imported-legacy HTTPS migration check (which read the
removed upgrade roster) was removed. When the old 1.8.7 setup indexed the
fixture, the journey now goes straight to the current Index library and reviewed
selection; it does not yet qualify the clean-break rebuild.
