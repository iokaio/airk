# Building and checking AIRK for macOS

**1.8.14 source implementation; unpublished and not yet Mac-qualified.** Build
and exercise the single-executable runtime on the intended architecture. Apple
Silicon preparation, portable tests or signing alone do not qualify Intel,
installed Code, model execution, upgrades or publication. Actual evidence and
remaining gates belong in the [working record](releases/release-1.8.14.md).
The [dated predecessor guide](reviews/macos-build-before-static-main-2026-11-07.md)
preserves the former pair/dynamic-library/copied-launcher instructions and results.

## Prerequisites and native layout

Use the repository-pinned Rust 1.98.0, Xcode Command Line Tools, Python 3.11+,
Node.js 24.15.0 and locked extension dependencies. PowerShell 7 runs repository
hygiene checks. Inspect the existing SDK/compiler with `xcrun --find clang` and
`xcrun --sdk macosx --show-sdk-path`; do not change global Xcode selection to make
a fixture pass. The builder requires `MACOSX_DEPLOYMENT_TARGET=13.3`. The actual
build SDK is separately recorded; using a newer SDK is not oldest-OS acceptance.

| Native host | Rust target | VSIX platform |
| --- | --- | --- |
| Apple Silicon | `aarch64-apple-darwin` | `darwin-arm64` |
| Intel | `x86_64-apple-darwin` | `darwin-x64` |

The modern payload contains `AIRK.app/Contents/MacOS/airk`, with embedded pinned
MiniLM/tokenizer assets and statically linked CPU ORT 1.22.1. There is no separate
`airk-embed`, copied `airk-launcher-image`, dynamic ORT library or external default
model payload. The main remains inside its signed app. Static channel plists,
legal notices and signed resources remain inventoried. Runtime schema 2 binds
one executable; the complete installer inventory independently binds all files.
Native project workers may use the same image: one distributed executable does
not promise a single OS process for every workload.

## Verified model and static ORT inputs

Provision verified inputs before Cargo; compilation cannot substitute an ambient
ORT installation or download model assets. For Apple Silicon (substitute the
Intel target on that native host):

```sh
python3 scripts/provision-local-model.py --destination target/local-vector-model --download
python3 scripts/provision-static-onnx-runtime.py --target aarch64-apple-darwin --destination target/static-onnx-runtime/aarch64-apple-darwin --download --download-tools --build --parallel 2 --environment-file artifacts/local/static-ort-environment.json
```

The environment JSON contains a receipt and an `environment` object. A reviewed
runner must supply only its five verified child-environment values:
`LIBONNXRUNTIME_NO_PKG_CONFIG`, `ORT_SKIP_DOWNLOAD`, `ORT_PREFER_DYNAMIC_LINK`,
`ORT_LIB_LOCATION` and `ORT_LIB_PROFILE`. Do not source/evaluate the JSON as shell
code or export the entire wrapper. Hosted runners use `--github-env`; local
scripts must not forge a GitHub command-file context. The runtime builder
revalidates the receipt/archive identities and all five values. The historical
`provision-onnx-runtime.py` dynamic NuGet libraries cannot satisfy this build.
See [static provisioning](../scripts/provision-static-onnx-runtime.py) and
[contributor prerequisites](../CONTRIBUTING.md).

After setting that verified child environment, one coordinated Cargo owner runs:

```sh
npm ci --ignore-scripts --no-audit --no-fund --prefix packages/vscode
cargo fmt --all --check
python3 scripts/dev-check.py -p airk-cli --features local-inference --target-dir target
python3 scripts/build-service-runtime.py --profile release --destination artifacts/local/macos-1.8.14/runtime
python3 scripts/check-service-clients.py --runtime-dir artifacts/local/macos-1.8.14/runtime --files 16 --model
python3 scripts/check-service-install.py --runtime artifacts/local/macos-1.8.14/runtime
npm test --prefix packages/vscode
```

Keep one warm Cargo target per checkout; retain the exact command, source seal,
feature/target identity, logs and Cargo timings. Development Clippy defaults to
incremental off on Mac; release profiles are unchanged. These scoped commands
do not replace [required qualification](../CONTRIBUTING.md#required-qualification).
The builder verifies embedded inventory, executes the real model smoke and
checks actual native imports. Link/build success alone is not model correctness
or installed-client readiness.

For space-limited Mac Neo, batch necessary checks through the existing SSH setup,
retain all evidence outside the target, then run `cargo clean` there after the
batch, including failures. This exception does not authorize routine cleanup on
other Macs. Preparation recorded for 1.8.14 recovered and reverified 51 consumer
archives, including real re2, while retaining the original 79-archive producer.
The earlier supplemental runner lost its exit observation and remains a failed
or unknown attempt; the explicit no-build recovery does not turn it into a fresh
build. SDK 26.5 and minimum deployment target 13.3 are distinct. None of those
static-library observations qualifies an AIRK link, model, installer or GUI run.
See the [implementation ledger](research/one-airk-implementation.md).

## Package and sign the exact tested runtime

Package the matching native architecture:

```sh
python3 scripts/package-native.py --target aarch64-apple-darwin --runtime-dir artifacts/local/macos-1.8.14/runtime --output artifacts/local/macos-1.8.14/native
python3 scripts/package-extension.py --target aarch64-apple-darwin --native-dir artifacts/local/macos-1.8.14/native/publish --output artifacts/local/macos-1.8.14/vscode
```

These commands verify complete payloads, executable modes, source/lock/build
receipts, actual embedded inference and the extension loader inventory. Ad-hoc
development signatures do not establish Developer ID trust, notarization or
Gatekeeper acceptance. Never copy the app-bound main out as an executable client.
MCP clients use the saved HTTPS endpoint; optional terminal integration forwards
to the approved main, and manager definitions target it directly in the bundle.

Release signing works on a fresh copy of this tested runtime with
`sign-native.py --runtime-dir <runtime>` and the exact approved identity. It
signs the main and enclosing app, verifies Developer ID/team/hardened runtime/
timestamp and the pinned public leaf, obtains Apple's explicit Accepted result,
then staples and validates the app ticket. No modern helper/dylib/launcher copy
is manufactured. Full legal/resource/runtime/VSIX inventories remain intact.
See [native signing](native-signing.md) for explicit local keychain selection and
CI's ephemeral keychain. Certificate/keychain preparation and publication require
their own authorization; no credentials belong in logs or repository files.

Signed VSIX promotion replaces the native subtree/wrapper without recompiling
host/UI/parser files. Loader inventory changes only the native-wrapper digest;
reviewed product manifest fields and other module digests remain unchanged.
Installed Code's top-level `__metadata` and JSON formatting are the only allowed
manifest rewrite. Apple trust, payload integrity and actual Code acceptance are
separate checks.

## CI, upgrades and installed-client acceptance

Routine CI builds Linux, Windows and Apple Silicon on PRs; main builds Intel and
may promote authenticated identical-tree candidates. Each Mac's selected CLI
regressions use shipping `local-inference` features, with a separate no-feature
component compile. Static input provisioning, release-main build, managed HTTPS
client/model/Stop checks and native/VSIX verification remain required. Full
workspace and extended campaigns retain their documented opt-in scope. See
[CI selection](ci-and-builds.md#checks-selected-by-the-change).

Hosted signing imports the protected `macos-signing` identity into a temporary
keychain, validates its exact public leaf and matching identity, preserves the
original search list and removes temporary credentials after success or failure.
Signed release/promotion verifies the complete tested native and compiled VSIX
graph. A local keychain path is not proof of which certificate CI imported.

Actual installed-client acceptance must cover normal CA validation, protected
credentials, saved-port conflicts, exact native scope and complete canonical
read results. Internal AIRK HTTPS success does not qualify Code's trust cache or
an external client. Current Code credential command inputs require no environment
reload; actual cold/running-host transport observations remain separate gates.
Do not remove quarantine, disable TLS/signature checks or spoof HOME to pass them.
An isolated Code profile is not a new OS user.

Checkpointed upgrades preserve canonical state, original requests and source
bytes. Old multi-process adapters disclose committed-store/verified-exit
boundaries rather than pretending old runtimes implemented the new protocol.
The explicit `check-runtime-upgrade.py --semantic-old` branch additionally
requests real predecessor model/vector work and identity preservation; default
coverage remains lexical. Mac native graceful exit does not provide Windows
HANDLE/Linux pidfd census or global all-process proof. For automatic removal of
authenticated obsolete process images, the installer retains a read-only Mach
task-name right, obtains its native audit token and uses the kernel's atomic
PID/idversion signal API. It never requests debugger/task-control rights or
signals a numeric PID after a path check. This later-added API is discovered at
runtime: macOS 13.3 remains the deployment minimum, but older systems without
the API must release predecessors through authenticated graceful shutdown or
Service Management. A stalled owner produces
`macos_atomic_process_stop_unavailable`; it is not silently declared stopped.
The first failed graceful attempt remains bounded diagnostic evidence in the
upgrade journal even if a later exact-image hard stop succeeds. No unexecuted
branch is recorded as a successful semantic or Mac upgrade.

Use reviewed index setup and the current Search scope / Index library / Index
sets flows. One Explorer per window, automatic compatible AIRK host handoff,
dismissed notices, typed source-change busy refusals without mutation replay,
Stop/uninstall ownership and exact-owned fixture cleanup remain installed tests.
Mac login consent/registration, signed upgrade/rollback, offline extracted-ticket
behavior, source preservation, semantic inference and Intel execution require
their own receipts. See [installation](service-installation.md),
[extension behavior](vscode-extension.md) and [current release gates](releases/release-1.8.14.md).

## Native direct-manager acceptance

On the selected standard user's Mac GUI session, current schema-2 acceptance is:

```sh
python scripts/check-macos-direct-runtime.py --runtime <signed-runtime> --approved-sha256 <independently-selected-inventory-digest> --evidence-parent <existing-private-directory> --allow-preview-registration
```

This explicitly authorizes the otherwise unused preview registration for the
owned fixture. The script verifies the actual payload and native manager
identity, exercises direct main execution and HTTPS sessions, and retains
uncertain ownership evidence. Existing foreign registrations remain refused.
The old `check-macos-login.py` fixture only accepts schema-1 1.4 payloads and
copied-launcher behavior; it cannot qualify the current runtime. Record actual
consent, signing/notarization, upgrade/rollback and installed-editor results
separately when those paths are exercised.

## Prior failures and remaining evidence

The dated predecessor guide retains CI 41380069062 attempts 1/2, later cold-client
fixes and authenticated historical Mac successes. Their exact source/artifacts
do not qualify the current 1.8.14 tree. Earlier Neo Developer ID/copied-launcher
observations are historical schema-1 evidence. Current Windows installer, Mac
Neo build/model/sign/notarize/installed-editor and independent Intel qualification
must be recorded as actually run, failed, skipped or unavailable. No compiler
or Docker is needed on an end user's Mac, but that packaging goal does not replace
those release gates.
