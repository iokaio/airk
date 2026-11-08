# Historical macOS build guide before the 1.8.14 static-main cutover

Archived 7 November 2026. This preserves predecessor instructions and their dated evidence. It is not current build guidance or qualification of 1.8.14. Use [the current Mac guide](../macos-build.md).

# Building and checking AIRK for macOS

The current unpublished working version is **1.8.12**. Build the native pair and
platform VSIX on the intended Mac architecture. A Windows package, an Apple
Silicon result, or an unsigned archive alone does not qualify installation on
both Mac architectures. The [working-build record](../releases/release-1.8.12.md)
records the current source, artifacts, completed checks and remaining limits.

## Prerequisites and native layout

Use the repository-pinned Rust **1.98.0**, Xcode Command Line Tools, Python
**3.11+**, and Node.js **24.15.0** with the locked extension dependencies.
PowerShell 7 runs the repository hygiene check. Inspect the existing Xcode
selection with `xcrun --find clang` and `xcrun --sdk macosx --show-sdk-path`.
Do not change the machine-wide tool selection merely to make a fixture pass.

The native builder enforces `MACOSX_DEPLOYMENT_TARGET=13.3`, matching the bundled
ONNX Runtime library and `AIRK.app` minimum. That declaration is a build baseline;
oldest-supported-OS acceptance remains a separate observation.

| Host | Rust target | VSIX platform |
| --- | --- | --- |
| Apple Silicon | `aarch64-apple-darwin` | `darwin-arm64` |
| Intel | `x86_64-apple-darwin` | `darwin-x64` |

Both packages contain the lightweight main and inference helper under
`AIRK.app/Contents/MacOS/`. Since 1.8.8, the app also carries a separate
`airk-launcher-image`. The installer copies that independently inventoried image
to the stable launcher location. Copying the app's main image after sealing the
outer app is not the supported standalone-launcher path. The helper and its
library remain in the approved app layout; non-code ONNX notices live under
`Contents/Resources/onnxruntime`. See [Mac login and ownership](../service-macos-login.md).

## Build and exercise a local candidate

Provision or supply the verified offline inputs first. Cargo does not download
model assets or inference libraries during compilation:

```sh
python3 scripts/provision-local-model.py --destination target/local-vector-model --download
python3 scripts/provision-onnx-runtime.py --download
npm ci --ignore-scripts --no-audit --no-fund --prefix packages/vscode
cargo fmt --all --check
python3 scripts/dev-check.py -p airk-cli --all-features --target-dir target
python3 scripts/build-service-runtime.py --profile release --destination artifacts/local/macos-1.8.12/runtime
python3 scripts/check-service-clients.py --runtime-dir artifacts/local/macos-1.8.12/runtime --files 16 --model
python3 scripts/check-service-install.py --runtime artifacts/local/macos-1.8.12/runtime
npm test --prefix packages/vscode
```

The runtime builder selects the native host architecture; cross-compilation is
not an execution or installation result for the other architecture. Keep one
persistent Cargo target per checkout and one Cargo owner. Retain the actual
commands, source seal, result logs and Cargo timing reports. Development Clippy
uses incremental compilation off on macOS by default; release builds retain
Cargo release-profile defaults. See [required qualification](../../CONTRIBUTING.md#required-qualification)
for the broader gates and [CI selection](../ci-and-builds.md#checks-selected-by-the-change)
for routine versus opt-in campaigns.

For the space-limited **Mac Neo**, batch its necessary checks over the existing
SSH setup, retain every result and timing report outside the target, then run
`cargo clean` there after the batch, including a failed batch. This Neo-specific
rule does not call for routine cleanup on other development Macs.

Package on the matching host, substituting the target from the table:

```sh
python3 scripts/package-native.py --target aarch64-apple-darwin --runtime-dir artifacts/local/macos-1.8.12/runtime --output artifacts/local/macos-1.8.12/native
python3 scripts/package-extension.py --target aarch64-apple-darwin --native-dir artifacts/local/macos-1.8.12/native/publish --output artifacts/local/macos-1.8.12/vscode
```

These scripts verify the complete native payload, executable permissions,
platform/version identity, source and lock receipts, extension files and loader
inventory. Development signing is ad hoc and permits the development helper to
load its ad-hoc signed library. It establishes neither Developer ID trust nor
notarization. A clean installed-editor run with the resulting platform VSIX is
separate from native client or archive verification.

## Signed distribution and CI installation gates

Current CI builds Linux, Windows and Apple Silicon on pull requests. Main builds
Intel and may promote the other three identical-tree PR candidates; changed
build inputs or failed producer authentication restore all four main builds.
Manual and qualification runs cover all four hosts. Each Mac has native
ownership/retrieval regressions, a fresh release pair, verified same-run payload
transfer, model/direct-client and installed-process checks, cold-client MCP
observations, native packaging and platform VSIX verification. Full Mac workspace
and extended campaigns remain opt-in. See the [CI policy](../ci-and-builds.md).

GitHub release signing restores the authenticated tested native and compiled
VSIX artifacts. On both Mac architectures it signs and notarizes a fresh copy
using the protected `macos-signing` environment, verifies the exact Developer ID
team, hardened runtime and timestamps, seals the outer app after its components,
and verifies the exact standalone launcher bytes after copying them outside the
app. [Native signing](../native-signing.md) describes credential handling, strict
verification and cleanup. No compiler or Docker is required on an end user's Mac.

Signed VSIX promotion replaces the native payload and wrapper without rebuilding
the host or UI. The loader inventory must then change only the native-wrapper
digest: every host/UI/parser digest and the reviewed package manifest remain
unchanged. `verify_vsix` checks that complete inventory for unsigned candidates
and promoted signed packages, so stale wrapper hashes fail the existing routine
package/release gates. Marketplace publication reauthenticates and reuses those
exact signed release packages rather than assembling a separate unsigned set.

VS Code adds its own top-level `__metadata` and may reformat installed
`package.json`. The 1.8.12 loader compares all reviewed product fields while
allowing only that platform metadata rewrite. Archive verification still checks
the exact reviewed source bytes. This is separate from Apple trust, quarantine,
notarization and actual installed-editor acceptance.

The native-enabled `check-native-editor.cjs` journeys now implement the current
**Search scope / Index library / Index sets** flow; the earlier removed-controls
failure remains a dated historical checkpoint. They use a permanent exact
`hidden.ts` source exclusion, real index-rename stale review, index-set deletion
with retained-session fencing and exact saved applied-selector revalidation on
reload. The first migrated actual `same` run failed when a one-queued-job native
`SourceChangeBusy` refusal was classified as `unexpected_service_response`.
The host correction preserves that typed pre-insert refusal and complete body,
shows queued/running work and never replays the mutation. Its source regressions
passed; the actual Mac GUI rerun with the corrected host is pending.

The first exclusion review is preceded by a read-only observation of the exact
index/backing, service and worker generation with a **90-second idle deadline**;
this does not grant native mutation admission. Unix profile paths are short and
canonical, Code runs privately and shell integration is off. Cleanup closes the
fixture editors and requires a qualified exact-owned Stop before removing only
new account uninstall receipts whose exact ownership was captured; unknown
ownership or state is retained. The reviewed old-VSIX upgrade helper remains
unexecuted and is separate from a prequalified native legacy fixture.

Current local notarization reached **Accepted**, with separately recorded native
lifecycle/model checks. Those observations do not establish a passing current
native-enabled GUI journey. Bootstrap/tab handoff and Restricted Mode retain
their narrower scope. No CI workflow invokes the desktop GUI harness; this
follow-up changes local fixture behavior, not hosted workflow selection.

## Prior failures and remaining evidence

The earlier Intel cold-client failures remain identifiable as CI 41380069062,
attempts 1 and 2. The later `c899eca6...` correction preserved setup-deadline
expiry for late waiters instead of turning it into a generic connection failure.
Subsequent main CI 41398425043 and 41551033290 passed both Mac runtime jobs,
including their cold-client repetitions and native/VSIX packaging. Those runs
qualify their own source and artifacts, not the current unpublished working tree.
See the [dated 1.8.7 follow-up](../releases/release-1.8.7.md#cold-setup-deadline-follow-up-november-2-utc).

Earlier Neo Developer ID, copied-launcher and isolated native installation
observations retain their source identities in the [implementation ledger](../research/implementation-v190.md).
A new local Neo pass does not establish Intel execution, fresh hosted CI,
notarization, an oldest-OS result or publication unless those checks are actually
run and recorded. Current evidence belongs in the 1.8.12 working-build record.

The single-executable `target/release/airk --root ... --data-dir ... setup`
examples formerly on this page described pre-1.4 operation. They are historical
and do not describe the current service-owned runtime pair or explicit index
setup. For current user actions, use [reviewed index setup](../service-project-setup.md)
and the [VS Code extension guide](../vscode-extension.md).
