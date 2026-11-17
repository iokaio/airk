# Root-owned final native Help command chain

Prepared only; no Cargo or generation command below has run in this task.

The complete root-only implementation of this chain is now run_final_help.py.
Default invocation prints a plan and runs no Cargo/native/Node generation.
After the coherent source is held, root reviews that plan and executes the same
source seal. The runner uses existing receipt-owner source observation and test
counting, permits only each emitter's named output, preserves the Usage SHA,
captures exact commands/counts/timings and stops on the first failure:

```powershell
python artifacts/advanced-split/local-delivery-preparation-r1/run_final_help.py --out artifacts/advanced-split/receipts/final-help-r1 > artifacts/advanced-split/local-delivery-preparation-r1/help-runner-plan.json
$helpPlan = Get-Content artifacts/advanced-split/local-delivery-preparation-r1/help-runner-plan.json -Raw | ConvertFrom-Json
python artifacts/advanced-split/local-delivery-preparation-r1/run_final_help.py --execute-reviewed --expected-source-sha256 $helpPlan.source_sha256 --out $helpPlan.out
```

Do not execute the second command until root has independently reviewed the
script, actual plan and current source. Its six emitter tests each require
exactly one selected; every other test step requires a nonzero actual count.
Nine pure refusal/recovery controls pass without a real command: stale source,
zero/wrong counts, original failure preservation, unexpected changes, Usage
identity, declared output scope, child-only writer variables and between-command
source changes. Every changed generated output is retained under its step with
its before/after SHA; the final receipt retains the full final source map.
For a newly approved embedded MiniLM catalog, supply its real provisioned
directory with --bundled-model-dir in both plan and execution commands; the
default is the currently matching bundled-model-v2 directory. Generation does
not itself qualify or promote a model. Native/runtime final gates remain real.

The helper was frozen after npm isolation was added; review evidence and the
9/9 pure control log are in help-runner-freeze-r2/receipt.json and controls.log.
Run the original helper path after checking its frozen SHA, not the deeper
review copy (its workspace-root calculation is source-location-relative).
Every child receives NPM_CONFIG_CACHE inside artifacts/advanced-split/npm-cache,
NPM_CONFIG_LOGS_DIR inside that cache, and disabled update notifications. No
human-owned npm cache is copied, read or written by this preparation.
The workspace cache is currently absent. Both node_modules trees and installed
lock markers exist, but locked npm ci reinstalls from tarballs. All173 distinct
locked URLs (29Admin entries are a subset) use registry.npmjs.org with integrity
pins. The first run therefore requires registry network access for the selected
platform packages; cache population/connectivity has not been tested here.
See npm-prerequisites.json. Do not claim offline npm readiness from node_modules.
Uses live owners, not the unavailable historical arbiter_help selector.
Read together with arbiter-help-preparation-r1/GENERATION.md. StageService
confirmed the current parser in crates/airkctl/src/cli/arbiter.rs: initialize,
status, configure --file, provision/remove --expected-revision --profile,
cancel/recover --expected-revision --stage focus|context, and I1-only
import --expected-revision --profile --directory ORIGINAL_ABSOLUTE_NATIVE_PATH.
The authored operator guide now documents that exact source behavior and its
qualification limits. No generated JSON/TS or Rust Help source was hand-edited.

Root is the only Cargo owner. Reuse an already passed equivalent current-input
three-crate Clippy result if available; otherwise run the scoped check first.
Use the same native dependencies and target as the integrated candidate.
These are intentional generation writes before qualification input freeze;
retain before/after identities and review only their expected output roster.

## Windows environment and check

```powershell
Set-Location D:/code/Github/airk
$env:PATH = "D:\code\Github\airk\artifacts\tools\node-v24.15.0-win-x64;D:\code\Github\airk\artifacts\tools\nextest;C:\Users\tdotn\.cargo\bin;" + $env:PATH
$env:CARGO_TARGET_DIR = 'D:/code/Github/airk/artifacts/v190/p10-target'
$env:CARGO_INCREMENTAL = '1'
$env:CARGO_BUILD_JOBS = '8'
$env:CARGO_NET_OFFLINE = 'true'
$env:AIRK_BUNDLED_MODEL_DIR = 'D:/code/Github/airk/artifacts/advanced-split/bundled-model-v2'
$staticEnvironmentFile = 'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json'
if ((Get-FileHash -LiteralPath $staticEnvironmentFile -Algorithm SHA256).Hash.ToLowerInvariant() -ne '0edd26b2f85e2ca84b24f2bfd83855ffebb593697e247532684c10e749aef870') { throw 'Previously verified static handoff identity changed' }
$verifiedStatic = (Get-Content -LiteralPath $staticEnvironmentFile -Raw | ConvertFrom-Json).environment
$verifiedStatic.PSObject.Properties | ForEach-Object { [Environment]::SetEnvironmentVariable($_.Name, $_.Value, 'Process') }
python scripts/dev-check.py -p airkctl -p airk-mcp -p airk-https --all-features --offline --incremental on --target-dir artifacts/v190/p10-target --cargo C:/Users/tdotn/.cargo/bin/cargo.exe
if ($LASTEXITCODE -ne 0) { throw 'Scoped Clippy failed' }
```

This reuses root's already verified current r5 static owner, without repeating
its full source/tool/archive rehash during Help preparation. The final canonical
runtime builder still performs its own checks. The earlier observation of
ort130-x64 was a different historical owner; its timeout is not evidence about
the current r5 closure.

The current bundled-model-v2 bytes match the live two-profile catalog; that is
an input identity check, not a new MiniLM quality or promotion result. If root
promotes a new approved catalog/model identity before this run, replace the
directory with its genuinely provisioned closure and record that identity.
Do not point the existing catalog at the new parent-bag graphs without its own
approved catalog change and actual qualification.

## Parser first, then native content and projections

Each assignment applies only to its named owner; finally clears it even on
failure. Stop at the first failure. Do not start all six write modes together.

```powershell
$env:AIRK_WRITE_CLI_HELP = '1'
try { cargo test -p airkctl --lib help_catalog_matches_the_embedded_airk_mcp_catalog --all-features --locked --timings; if ($LASTEXITCODE -ne 0) { throw 'CLI catalog emission failed' } }
finally { Remove-Item Env:AIRK_WRITE_CLI_HELP }

$env:AIRK_WRITE_CLI_REFERENCE = '1'
try { cargo test -p airkctl --lib cli_reference_documents_every_public_command --all-features --locked --timings; if ($LASTEXITCODE -ne 0) { throw 'CLI reference emission failed' } }
finally { Remove-Item Env:AIRK_WRITE_CLI_REFERENCE }

$env:AIRK_WRITE_HELP_CONTENT = '1'
try { cargo test -p airk-mcp --lib help_content_manifest_matches_native_owners --all-features --locked --timings; if ($LASTEXITCODE -ne 0) { throw 'Native Help emission failed' } }
finally { Remove-Item Env:AIRK_WRITE_HELP_CONTENT }

$env:AIRK_WRITE_UI_PILOT_HELP = '1'
try { cargo test -p airk-mcp --lib ui_pilot_help_native_cases --all-features --locked --timings; if ($LASTEXITCODE -ne 0) { throw 'Native Help semantic emission failed' } }
finally { Remove-Item Env:AIRK_WRITE_UI_PILOT_HELP }

$env:AIRK_WRITE_UI_PILOT_CLI = '1'
try { cargo test -p airkctl --lib ui_pilot_cli_semantic_cases --all-features --locked --timings; if ($LASTEXITCODE -ne 0) { throw 'I2 semantic emission failed' } }
finally { Remove-Item Env:AIRK_WRITE_UI_PILOT_CLI }

$env:AIRK_WRITE_UI_PILOT_I3 = '1'
try { cargo test -p airk-https --lib ui_pilot_i3_semantic_cases --all-features --locked --timings; if ($LASTEXITCODE -ne 0) { throw 'I3 semantic emission failed' } }
finally { Remove-Item Env:AIRK_WRITE_UI_PILOT_I3 }
```

Each generator must select one actual test. Expected changed owners are
crates/airk-mcp/src/help/cli_commands.json, docs/cli-reference.md,
crates/airk-mcp/src/help/content.json and the existing Help/I2/I3 pilot JSON
fixtures. Retain the unchanged usage_native.json SHA before/after. No Usage
generation or handwritten mirror is required. I2/I3 cases still include Usage;
those unchanged values must remain byte/semantic equivalent under their
original identities.

## Drift, native semantics and consumers

All six write variables must be absent during this regression command. Record
the real selected test count; zero selected is a failure. The filter covers
native Help content/tasks, selectors, serialized budgets, full continuation,
request-local schemas, public Help conflict/refusal behavior and the three
native pilot projections. It is not a substitute for full transport qualification.

```powershell
cargo nextest run --profile ci -p airk-mcp -p airkctl -p airk-https --all-features --locked --timings -E '(package(airk-mcp) & (test(help::) | test(ui_pilot_help_native_cases) | test(public_help_) | test(tool_help_) | test(native_help_detail_))) | (package(airkctl) & (test(help_catalog_matches_the_embedded_airk_mcp_catalog) | test(cli_reference_documents_every_public_command) | test(ui_pilot_cli_semantic_cases))) | (package(airk-https) & test(ui_pilot_i3_semantic_cases))'
```

After reviewing generated native outputs, use existing locked npm dependencies
or run the necessary npm ci for changed locks. The Help writer must precede a
normal extension compile, whose drift check expects the new authored/native
manifest to exist already. No Cargo runs alongside this source/asset mutation.

```powershell
node packages/vscode/scripts/build-explorer.cjs --write-help-manifest
npm run compile --prefix packages/vscode
node --test packages/vscode/out/test/explorerHelp.test.js
node --test packages/ui/test/helpManifest.test.mjs packages/ui/test/helpSearch.test.mjs packages/ui/test/helpRoute.test.mjs packages/ui/test/helpPresentation.test.mjs packages/ui/test/help.dom.test.mjs packages/ui/test/generatedHelp.dom.test.mjs packages/admin/test/helpSemantics.test.mjs
python -m unittest scripts.tests.test_help_site -v
npm run build --prefix packages/admin
npm run check --prefix packages/admin
python scripts/build-admin-assets.py --output artifacts/advanced-split/local-delivery-candidate-r1/admin-assets.json
```

Stop and retain diagnostics on failure. The final output directory must be
fresh, and the Admin owner must write that inventory before the fresh native
pair embeds those bytes. Freeze the resulting shared Help, Admin assets,
contract fixtures and source identities before expensive pair/package/journey
qualification. This produces neither a published Help site nor a VSIX release
publication capture; those separate owners and gates remain required.
