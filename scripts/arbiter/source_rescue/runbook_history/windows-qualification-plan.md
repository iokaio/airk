# Final Windows candidate qualification plan

Prepared from live script interfaces on 9 November 2026. This file is a proposed
serial execution plan, not a passing receipt. No build or test was run to prepare
it. The primary agent owns Cargo, source freeze, review and all execution.

**Current hold:** the service-runtime process component has reached its two-strike
stop. Its root-cause investigation is open (planned catalog drain and startup
backoff). The commands below are a prospective ladder, not authorization for a
third T2/T3 service run. That component requires the completed root-cause note,
cheap regression evidence, combined review and user go-ahead before resumption.

Target: a locally built **1.8.17 win32-x64 VSIX** suitable for manual testing.
The state-contract baseline remains **1.8.16 / airk-state/1**. No commit, push,
release dispatch, signing, publication, normal-profile installation, or normal
editor update is part of these commands.

## Existing evidence to retain

- `process-preflight.json` / `.log`: 52 selected, 48 passed, one failed and three
  not run. Failure:
  `initialized_completed_reviewed_activation_replay_preserves_stopped_service_and_all_state`.
  This is one failed T2 attempt, not a passing batch. Retain the 48 named passing
  outcomes with their original source identity; do not repeat them merely because
  the remaining fixture is corrected. The stale plan's `expected_selected: 23`
  is superseded by the actual **52** selected in this receipt. A later second
  failure is the reason for the current hold above; retain its receipt too.
- `neo-collected.json`: Store/CLI/MCP scoped Clippy passed; 37 selected tests
  passed; eight timing reports retained; owned Cargo group absent and Neo target
  removed. Its native source hash is
  `3d9e0798580e58105a26a4dc055fa7a18858bf331db46a20f8aceef3a7f83b4d`.
  Later affected native changes need their own scoped Mac evidence; this receipt
  cannot be silently relabeled as the later source.
- `python-contracts-host.json`: 923 tests selected; retain its full successful
  result and documented skips from the log. The earlier sandbox run is a failed
  environment result, not superseded evidence that can be discarded.
- `extension-mocked.json` / `.log`: 1,315 tests, 1,294 passed, 21 skipped, zero
  failed. Preserve source/output hashes; native skips still require native-enabled
  coverage. No blanket npm rerun if its source and compiled output remain exact.
- Scoped Clippy, boundary/archive/repository checks, dependency dispositions,
  notices, component regressions and their failures remain in this directory.
  Parent reconciles their exact identities against the final correction.

## Frozen inputs and child environment

All commands below run from `D:/code/Github/airk`. Keep one persistent Cargo
target, `artifacts/v190/p10-target`, and one serial Cargo owner. Do not run the
large `build-windows.ps1` wrapper: it would duplicate accepted work, rebuild and
couple review-profile creation to packaging. Run the individual steps below.

Pin Node for the shell **and all children**, since `package-extension.py` launches
the bare command `node` internally:

```powershell
$airkNodeDirectory = 'D:/code/Github/airk/artifacts/tools/node-v24.15.0-win-x64'
$env:PATH = $airkNodeDirectory + ';C:/Users/tdotn/.cargo/bin;D:/code/Github/airk/artifacts/tools/nextest;' + $env:PATH
if ((& "$airkNodeDirectory/node.exe" --version) -ne 'v24.15.0') { throw 'Pinned Node differs' }
$env:GIT_CONFIG_COUNT = '1'
$env:GIT_CONFIG_KEY_0 = 'safe.directory'
$env:GIT_CONFIG_VALUE_0 = 'D:/code/Github/airk'
```

Use `run.py` for separate command receipts, timings and before/after group hashes.
It injects the verified static ORT environment with `--static`, the existing P10
target, model directories and owned fixture directories. It removes external
native overrides by default. Use `--runtime <exact-runtime>` only for native
extension tests; omit it from editor commands so their installed package owns
the executable choice. `--release` unsets the local incremental override for
optimized production builds. Development Clippy uses incremental **on**.

Every receipt name below is fresh. If a name/output directory already exists,
use a new attempt suffix and preserve the earlier attempt. Do not overwrite a
receipt, failed fixture, runtime or package output.

Freeze and identify separately:

1. Native inputs: complete builder `source_identity()` roster, Cargo.lock,
   rust-toolchain, local model manifests/assets, exact static ORT build receipt,
   link closure and five child environment values. The builder's native source
   roster includes test sources, selected scripts and legal files. Record changed
   test sources separately if reusing a compiled production artifact; do not edit
   its original build/source receipt to claim a fresh build.
2. Extension: source, package manifests/lock, generated MCP catalog, `out/`,
   jsonc-parser and pinned bundler/packager dependencies. Preserve compilation
   receipts. Do not use `npm ci` speculatively or rebuild unchanged `out/`.
3. Harness/fixtures: `scripts/`, nextest config, fixture/public example assets,
   runner bytes, selected test identities and exact test outcomes.
4. Final artifacts: full runtime inventory, install manifest hash, runtime/build
   JSON hashes, executable hash, native archive/manifest hashes and final VSIX
   archive/manifest hashes. Store source build identity separately from test-source
   identity and qualification receipt identity.
5. Editor: the official unmodified 1.145.0 archive receipt and complete stock-file
   seal under `artifacts/tools/vscode-1.145.0-a717-r2`. Its editor is
   `runtime/Code.exe`; its matching CLI is
   `runtime/a717861c07/resources/app/out/cli.js`. Do not flatten this versioned
   layout or use the normal installed editor's updater-managed paths.

## Read-only editor seal result

`editor-stock-verification.json` retains the first extraction's complete local
verification and the newly prepared **r2** selection. The r2 preparation receipt
is `artifacts/tools/vscode-1.145.0-a717-r2/receipt.json`; it references the existing
read-only ZIP and metadata under r1. No new archive was downloaded. The source
archive SHA-256 `79cf4cc5b17357cc64abdd22a2d7d4884d05f07b3f0defbf5fc0d67fe03d9dc4`
agrees with both the existing download receipt and saved official metadata.
The stock seal SHA-256 is
`4689e94350310a4f7adb366145e41b6f85b1c8a6268207baddda83d41a86c729`.
All **3,280 stock files** match both the official ZIP contents and fresh r2
extracted hashes. The exact r2 roster passed before its first launch: none is
missing, changed or additional, and no portable `data` directory exists.
The earlier r1 exact-roster failure remains recorded: its extra generated
`runtime/debug.log` was preserved with size/hash. r1 was not changed or deleted.
Fresh extraction verified workspace containment, nonredirected ancestors, safe
ZIP paths, no links, exact stock membership, bounded counts/sizes and all hashes.
No editor launch, trust change, native build or test occurred.
The recorded origin is the saved official Microsoft update metadata and archive
URL, not a newly fetched network assertion. After a future launch, reverify every
stock code/resource hash and report generated logs separately; do not silently
allow additional resources, scripts, libraries or executable files.

The exact read-only tree/archive comparison can be repeated, when needed, with:

```powershell
@'
import hashlib,json,zipfile
from pathlib import Path
base=Path('artifacts/tools/vscode-1.145.0-a717-r2')
def digest(path):
    with path.open('rb') as source:return hashlib.file_digest(source,'sha256').hexdigest()
receipt=json.loads((base/'receipt.json').read_text())
metadata=json.loads(Path(receipt['source']).read_text())
seal=json.loads((base/'stock-file-seal.json').read_text())
assert digest(Path(receipt['archive_path']))==receipt['archive_sha256']==metadata['sha256hash']
assert digest(base/'stock-file-seal.json')==receipt['stock_file_seal_sha256']
actual={path.relative_to(base/'runtime').as_posix():digest(path) for path in (base/'runtime').rglob('*') if path.is_file()}
with zipfile.ZipFile(Path(receipt['archive_path'])) as zipped:
    archive={}
    for item in zipped.infolist():
        if not item.is_dir():
            assert item.filename not in archive
            with zipped.open(item) as source:archive[item.filename]=hashlib.file_digest(source,'sha256').hexdigest()
print(json.dumps({'stock_files':len(seal),'archive_matches_seal':archive==seal,'runtime_matches_seal':actual==seal,'missing':sorted(seal.keys()-actual.keys()),'extra':sorted(actual.keys()-seal.keys()),'changed':sorted(name for name in actual.keys()&seal.keys() if actual[name]!=seal[name])},indent=2))
assert archive==seal and actual==seal
'@ | python -
```

## Ordered ladder

Before each T2/T3 stage, the primary agent reviews the entire affected diff and
available evidence. No source/harness/fixtures consumed by that stage may change
during execution. Freeze receipt and exact expected result precede the command.

### 1. Finish the currently open correction

Complete the analyzed activation-restoration correction, its cheap observer
regressions and affected scoped Clippy in one reviewed batch. This plan does not
prescribe that correction's implementation. Retained failed fixture state must
not be repaired just to manufacture a successful cleanup receipt.

The next process check selects only the failed test and three unrun tests from
the earlier sorted selection. Confirm the retained 48 identities still have
unchanged production dependencies; if the correction changes a dependency of one,
rerun that named affected identity and record why, rather than restarting all 52.

```powershell
python artifacts/readiness-1817/run.py --static --groups native,harness,fixtures --tier T2 process-preflight-remainder-r1 cargo nextest run -p airk-cli --test service_runtime --no-default-features --features local-inference --locked --timings --profile ci --test-threads 1 --fail-fast -E 'test(initialized_completed_reviewed_activation_replay_preserves_stopped_service_and_all_state) | test(installed_control_loss_rejects_native_startup_and_restoration_retains_registry_namespace) | test(installed_log_binding_places_the_ring_outside_control_and_missing_ownership_never_falls_back) | test(reviewed_https_discovery_and_evidence_preserve_native_contracts)'
```

Expected selection: **4**, each named identity passes. Stop on first failure,
zero/unexpected selection, changed inputs or unresolved cleanup. This selection
can run only after the current two-strike hold is resolved; the already collected
second failure must not be omitted from its attempt count.

### 2. Required broad Windows source gates, once

Reuse exact-source passed formatting/Clippy/repository results where still valid.
Run missing or invalidated required broad gates serially. Full nextest explicitly
restores the four routine excluded campaigns; doctests remain a separate gate.
Do not infer complete qualification from the focused process selection.

```powershell
python artifacts/readiness-1817/run.py --groups native,harness --tier T0 final-format cargo fmt --all --check
python artifacts/readiness-1817/run.py --static --groups native,harness --tier T1 final-lightweight-main cargo check -p airk-cli --no-default-features --all-targets --locked --timings
python artifacts/readiness-1817/run.py --static --groups native,harness --tier T1 final-workspace-clippy cargo clippy --workspace --all-targets --all-features --locked --timings -- -D warnings
python artifacts/readiness-1817/run.py --static --groups native,harness,fixtures --tier T3 final-workspace-tests cargo nextest run --workspace --all-features --locked --timings --profile ci --ignore-default-filter --test-threads 2 --fail-fast
python artifacts/readiness-1817/run.py --static --groups native,harness,fixtures --tier T3 final-workspace-docs cargo test --doc --workspace --all-features --locked --timings -- --test-threads=1
python artifacts/readiness-1817/run.py --static --groups native,harness --tier T3 final-workspace-build cargo build --workspace --locked --timings
```

Before executing full nextest, list its exact same selection with `cargo nextest
list` and record all identities/counts. It must be nonzero and include the four
named excluded campaigns from `.config/nextest.toml`. Compare selected identities
and outcomes, not only totals. For doctests, record per-crate zero-test results
honestly; a zero total cannot establish doctest execution. `cargo build --workspace --locked` is explicitly required by
`CONTRIBUTING.md` under Required qualification; it is retained as that distinct
workspace default-feature build gate. The lightweight
configuration is distinct from the all-features Clippy configuration, so its
separate compile gate is intentional.

Refresh only invalidated Python/extension/static checks after the coherent source
batch. A static or documentation-only correction consumes no native runtime and
does not justify rebuilding it. A changed native dependency requires native seal
reassessment before proceeding.

### 3. Build the final optimized runtime once

```powershell
python artifacts/readiness-1817/run.py --static --release --groups native,harness,fixtures --tier T3 final-runtime-build python scripts/build-service-runtime.py --profile release --destination artifacts/readiness-1817/final-runtime
```

Expected: fresh output, version 1.8.17, schema-2 single `airk.exe`, matching Windows
x86_64 metadata; static imports, embedded model inventory and actual model smoke
pass; complete legal/static ORT notice inventory; before/after native source and
static input identities unchanged. Cargo `--timings` is built into this builder.
Record target, environment-unset incremental setting, elapsed time and receipt.

Before using/reusing this runtime, call the canonical read-only
`scripts/package-native.py::validate_runtime(runtime, '1.8.17',
'x86_64-pc-windows-msvc', builder.source_identity())`. It verifies the entire
bounded file inventory, exact install inventory, runtime/build metadata, source
identity, executable digest and allowlist including notices. Checking only files
named by a manifest is insufficient: it would miss unexpected extra files.
Retain its returned full inventory and receipt hashes. The later native packaging
step calls this validator again automatically.

### 4. Qualify this exact sealed runtime

The commands use the optimized artifact; they do not rebuild Cargo binaries.

```powershell
python artifacts/readiness-1817/run.py --groups native,extension,harness,fixtures --runtime artifacts/readiness-1817/final-runtime --tier T2 final-native-extension node --test packages/vscode/out/test/*.test.js
python artifacts/readiness-1817/run.py --groups native,harness,fixtures --tier T2 final-service-clients python scripts/check-service-clients.py --runtime-dir artifacts/readiness-1817/final-runtime --files 16 --model --report artifacts/readiness-1817/final-service-clients-report.json
python artifacts/readiness-1817/run.py --groups native,harness,fixtures --tier T2 final-model python scripts/check-transformer-model.py --binary artifacts/readiness-1817/final-runtime/airk.exe --report artifacts/readiness-1817/final-model-report.json
python artifacts/readiness-1817/run.py --groups native,harness,fixtures --tier T2 final-clean-install python scripts/check-service-install.py --runtime artifacts/readiness-1817/final-runtime
```

The native-enabled Node suite must select nonzero tests and exercise the named
native smoke/lifecycle cases previously skipped. Check its final skip roster;
do not call it a complete native pass if those cases remain skipped. Service
clients must report two projects, 16 original files per project, model checked,
isolation, real MCP lifecycles/reconnect, source bytes preserved and acknowledged
managed stop. Install fixture must reach its positive final message including
resumable uninstall while retaining independent state/source sentinel evidence.
Any failed or uncertain native lifecycle retains exact owned fixture state.

The process preflight already covers actual CLI/MCP complete-result parity and
HTTPS protocol/body contracts on its identified development inputs. Retain that
evidence without rerunning its 48 passing cases; service clients and native Node
cases now add observations about the optimized artifact.

### 5. Preflight and qualify predecessor paths

Use authentic retained runtimes only as read-only source payloads. The known
pre-baseline source is
`C:/Users/tdotn/.vscode/extensions/iokaio.airk-1.8.11/native/runtime`; this script
creates its own install/state/source roots and does not mutate that installation.
The fixture parent must be a newly created short owner-only directory,
`artifacts/r17`, prepared by the primary agent before input preflight; do not
reuse or change the old `artifacts/q31` permissions. The accessible supported-line
predecessor is
`artifacts/ladder-2026-11-10/service-runtime` (metadata version 1.8.16).
Retain both exact inventories and original build identities; do not relabel them.

```powershell
python artifacts/readiness-1817/run.py --groups native,harness,fixtures --tier T0 final-clean-break-inputs python scripts/check-runtime-upgrade.py --runtime artifacts/readiness-1817/final-runtime --previous-runtime C:/Users/tdotn/.vscode/extensions/iokaio.airk-1.8.11/native/runtime --fixture-setup reviewed --fixture-base artifacts/r17 --clean-break --check-inputs --output artifacts/readiness-1817/final-clean-break-inputs-report.json
python artifacts/readiness-1817/run.py --groups native,harness,fixtures --tier T0 final-compatible-inputs python scripts/check-runtime-upgrade.py --runtime artifacts/readiness-1817/final-runtime --previous-runtime artifacts/ladder-2026-11-10/service-runtime --fixture-setup reviewed --fixture-base artifacts/r17 --check-inputs --output artifacts/readiness-1817/final-compatible-inputs-report.json
python artifacts/readiness-1817/run.py --groups native,harness,fixtures --tier T2 final-clean-break python scripts/check-runtime-upgrade.py --runtime artifacts/readiness-1817/final-runtime --previous-runtime C:/Users/tdotn/.vscode/extensions/iokaio.airk-1.8.11/native/runtime --fixture-setup reviewed --fixture-base artifacts/r17 --clean-break --output artifacts/readiness-1817/final-clean-break-report.json
python artifacts/readiness-1817/run.py --groups native,harness,fixtures --tier T2 final-compatible-upgrade python scripts/check-runtime-upgrade.py --runtime artifacts/readiness-1817/final-runtime --previous-runtime artifacts/ladder-2026-11-10/service-runtime --fixture-setup reviewed --fixture-base artifacts/r17 --output artifacts/readiness-1817/final-compatible-upgrade-report.json
```

Both input checks must say `passed: true`, `runtime_executed: false` and disclose
the complete sealed census and path budget. The live runs must say `passed: true`
with exact source/index/registration preservation or clean-break rebuild proofs,
namespace/authority dispositions, predecessor exit, reconnect and completed owned
teardown. A successful health read or shutdown acknowledgement alone is not exit
or cleanup proof. `--semantic-old` is deliberately not added to the clean-break
path: that combination is rejected by the script's contract.

### 6. Package the exact native payload and VSIX

Only after extension compiled output/catalog identity is current and all consuming
inputs are frozen:

```powershell
python artifacts/readiness-1817/run.py --groups native,harness,fixtures --tier T2 final-native-package python scripts/package-native.py --target x86_64-pc-windows-msvc --runtime-dir artifacts/readiness-1817/final-runtime --output artifacts/readiness-1817/final-native
python artifacts/readiness-1817/run.py --groups native,extension,harness,fixtures --tier T2 final-extension-package python scripts/package-extension.py --target x86_64-pc-windows-msvc --native-dir artifacts/readiness-1817/final-native/publish --output artifacts/readiness-1817/final-extension --channel stable
```

Expected native candidate schema `airk-native-candidate/2`; exact runtime/source,
lock/toolchain, archive seal and extracted model/self-test pass. Expected VSIX:
`artifacts/readiness-1817/final-extension/publish/airk-1.8.17-win32-x64.vsix`.
`stable` here is package channel metadata, not publication. Local unsigned/dirty
source candidates remain explicitly identified as local; no signing or release
claim follows. Native packaging performs extracted executable identity,
runtime self-test, model-catalog, embedded inventory and actual model smoke.
Extension packaging verifies every packaged byte, update inventory, legal/module
allowlists and native manifest/archive/payload hashes. Do not patch an archive or
receipt after testing to change its version or source identity.

### 7. Installed-package acceptance on private editor profiles

First run the narrower installed bootstrap/metadata/controller gate. Then run
the native journey that includes the earlier R39 failure, with the complete
scenario selection explicitly recorded. The official editor paths are pinned.
Both harnesses create private profile/extension/workspace/shared-data roots.

```powershell
python artifacts/readiness-1817/run.py --groups native,extension,harness,fixtures --tier T2 final-installed-explorer node scripts/check-installed-explorer.cjs artifacts/tools/vscode-1.145.0-a717-r2/runtime/Code.exe artifacts/tools/vscode-1.145.0-a717-r2/runtime/a717861c07/resources/app/out/cli.js artifacts/readiness-1817/final-extension/publish/airk-1.8.17-win32-x64.vsix
$env:AIRK_EDITOR_SCENARIOS = 'untrusted,multi,same,different,usage'
python artifacts/readiness-1817/run.py --groups native,extension,harness,fixtures --tier T2 final-installed-native node scripts/check-native-editor.cjs artifacts/tools/vscode-1.145.0-a717-r2/runtime/Code.exe artifacts/tools/vscode-1.145.0-a717-r2/runtime/a717861c07/resources/app/out/cli.js artifacts/readiness-1817/final-extension/publish/airk-1.8.17-win32-x64.vsix
Remove-Item Env:AIRK_EDITOR_SCENARIOS
```

The narrower gate must retain its actual installed VSIX digest/version and
positive result; it intentionally exercises no native service. The native gate
must report `status: passed`, `qualifiesFullScenarioSelection: true`, the exact
five selected scenarios and **seven** role results (same/different each have two
roles). Empty or partial scenario results and exit 2 are failures for this gate.
Retain screenshots, UI dispatch receipts, source preservation, owned editor exit,
native cleanup and profile receipts. Failed fixtures remain identifiable.

Do not substitute `check-agent-install.cjs` for this journey. Its optional previous
VSIX mode explicitly requires an authentic pre-1.8.14 stdio predecessor, and its
current assertions retain imported-legacy migration expectations. It cannot
qualify a 1.8.16-to-1.8.17 compatible upgrade or the current fresh catalog
enrollment without a separately reviewed harness correction. The native upgrade
script above owns current predecessor/reset evidence.

## Stop, delivery and remaining qualification

Stop each ordered ladder at the first nonzero command, unexpected/zero selection,
input/seal drift, missing positive receipt or incomplete cleanup. Collect the
whole result before correcting source. Two T2/T3 failures for one component stop
further runs until the required root-cause note and user review. A Windows pass
does not establish macOS/Linux/container qualification.

Before presenting the package for manual testing, verify its final SHA-256 and
manifest again and link the actual artifact, current ledger and honest limits.
Do not install it in the user's normal profile. If a manual private review profile
is requested, prepare it from this existing VSIX with explicit private
`--user-data-dir`, `--extensions-dir`, `--shared-data-dir` and private native/data
settings; do not invoke a build wrapper that would create another candidate.

The full accepted release-readiness ladder also includes container qualification,
the remaining platform gates and the documented CI-local four-core five-pass
campaign. Those are separate qualification receipts, not prerequisites to merely
locating a package, and must not be described as passed when unavailable. The
exact CI-local command reuses this sealed optimized runtime:

```powershell
python artifacts/readiness-1817/run.py --groups native,extension,harness,fixtures --tier T3 final-ci-local pwsh -NoProfile -File scripts/ci-local.ps1 -SkipBuild -RuntimeDir artifacts/readiness-1817/final-runtime -Cores 4 -Repeat 5
```

Do not turn incomplete platform/container qualification into a release-ready
claim. The handoff can accurately identify a Windows manually testable candidate
once the applicable Windows gates pass, retaining all explicit exclusions and
the exact frozen artifact identity.
