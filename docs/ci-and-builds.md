# GitHub CI, native artifacts and spending controls

The [CI workflow](../.github/workflows/ci.yml) uses standard GitHub-hosted runners. No self-hosted machine, Docker account, Azure account, Apple account, custom repository secret or repository variable is needed for the current builds. GitHub supplies the job token and artifact-upload authorization automatically. Repository Actions must be enabled and permitted to use the pinned official actions.

Main now requires PRs and the stable `Required checks` / `signed-off` gates. For manual builds and optional Marketplace publication, use **Actions → Extension release** with an explicit version/channel and `publish` false/true. The [release workflow guide](release-workflow.md) describes the live PR policy, artifact verification and external Marketplace trusted-publisher prerequisite. The publishing job is separate from these read-only build jobs.

## Build matrix

| Product | GitHub runner | Rust target |
|---|---|---|
| Linux x64 | ubuntu-24.04 | x86_64-unknown-linux-gnu |
| Windows x64 | windows-2025 | x86_64-pc-windows-msvc |
| macOS Apple Silicon | macos-15 | aarch64-apple-darwin |
| macOS Intel | macos-15-intel | x86_64-apple-darwin |

These are standard labels from [GitHub's runner documentation](https://docs.github.com/en/actions/reference/runners/github-hosted-runners). Each job compiles and runs tests on its native architecture; it does not rely on cross-compilation from Windows. Windows/Linux ARM64 packages are future coverage. Runner OS versions do not establish the oldest OS that AIRK supports.

Jobs install the repository-pinned Rust compiler/components, Python 3.11 and Node.js 24.15.0. They restore the checked-in npm lock with dependency scripts disabled, check formatting and clippy, run workspace and packaging tests, check repository hygiene, and build the release CLI with the locked Cargo graph. They compile/test the extension and its actual native MCP adapter, then verify a matching platform VSIX. A separate Linux job builds and smoke-tests the one-container arrangement. No image is published.

Each native job also runs public CLI checks for DOCX/XLSX table facts, source aliases and quota/relocation rollback, and optional usage imports/calibration/price scenarios. These use original temporary fixtures and the exact release helper that is packaged. The VSIX includes the original listing illustrations as digest-verified assets.

## Downloadable candidates

Successful native jobs upload an artifact named `airk-native-<target>-<commit>-<run-id>-<attempt>` to the workflow run's Artifacts section. It contains:

- `airk-<version>-<target>.zip`: native executable, AIRK legal files and dependency notices.
- `manifest.json`: commit, dirty-checkout flag, compiler versions, lock/toolchain hashes, full payload digests and archive digest.
- `SHA256SUMS`: SHA-256 digests for the ZIP and manifest.

[The packager](../scripts/package-native.py) stages only these allowed inputs. It verifies every archive entry against the inventory, preserves Unix executable permissions, and runs the archived executable's version/help commands. It refuses an existing staging destination. Neither the repository tree nor `temp/`, index state, Cargo build directories or credentials are archived. GitHub retains build and test output in the run logs; failed jobs do not upload a successful candidate.

The manifest is a build record, not a signature or a complete reproducible-build attestation. Current dependency notice gaps remain documented in [third-party notices](../THIRD_PARTY_NOTICES.md). Public release qualification, signing and installed-editor tests are separate from candidate construction. The same run also uploads `airk-vsix-<target>-<commit>-<run-id>-<attempt>` containing the platform VSIX, extension manifest and SHA256SUMS. The VSIX manifest binds the native archive, its receipt, the npm lock and all staged runtime payload hashes. The extension package contains no runtime npm dependencies. See [extension deployment](extension-deployment.md) and [Windows local builds](windows-local-build.md).

To reproduce a candidate locally, substitute the target matching your Rust host:

```console
cargo build --release --locked -p airk-cli --target x86_64-pc-windows-msvc
python scripts/package-native.py --target x86_64-pc-windows-msvc
```

Use Python 3.9+ locally. Output is under ignored `artifacts/native/<target>/`. Retain or deliberately remove only that generated candidate directory before packaging again. Local uncommitted work is recorded as a dirty checkout.

## Controls that reduce unnecessary work

- Push builds run for `main` and `v*` tags; other branch pushes run through their pull requests. Manual dispatch remains available. There is no schedule.
- New commits cancel an older run for the same branch or pull request.
- All four native matrix jobs may run together, plus the container job, so one platform does not wait for another platform's build. Each native compiler still uses two workers to bound memory pressure.
- A pinned official cache action retains Cargo registry/git dependencies and explicitly selected compiler output directories. Keys isolate OS, target, compiler/workflow configuration and dependency manifests; compatible older dependency graphs can supply reusable outputs. Incremental compilation is disabled to keep caches smaller. Every check and build still runs after restoration, and Cargo determines which outputs need rebuilding. Cache misses or eviction cause ordinary fresh builds. Local models, corpora, index state and candidate staging directories are outside the cache paths.
- Native and container jobs have 90-minute limits to accommodate clean compiler, debug-test and optimized ONNX-runtime builds. Timings remain subject to runner capacity.
- Candidate artifacts expire after seven days and exclude compiler outputs and intermediate staging directories. Compiler caches are separate from downloadable candidates. Docker BuildKit keeps local dependency/target caches for repeated builds within a job; generated distributions are copied out of cache mounts and verified normally.
- Jobs have read-only repository permissions and do not receive publishing or signing credentials. Fork contributions use `pull_request`, not a privileged `pull_request_target` build.

Concurrency and time limits bound work per run; they do not impose a monthly spending cap. All four native builds still run for every admitted code change. Documentation-only changes also run CI so repository checks remain required.

## Repository-owner spending setup

GitHub says standard hosted runner use is free for public repositories; larger runners are charged even for public repositories. Storage has its own billing rules. Keep this repository public and retain the standard runner labels above. See [GitHub Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions).

In the owning account or organization's **Billing and licensing → Budgets and alerts**, configure a GitHub Actions budget with **stop usage when the budget is reached**, using a zero paid-spend limit if available and desired. Include the applicable Actions storage scope, keep included-usage alerts enabled, and check whether existing account/organization budgets already apply. Alerts alone do not stop builds. These are owner billing settings, not repository secrets, and this checkout has not changed them. GitHub describes the enforcement and first-cycle limitations in [Budgets and alerts](https://docs.github.com/en/billing/concepts/budgets-and-alerts).

Under repository **Settings → Actions → General**, require approval for workflows from outside collaborators according to the project's contribution policy. Repository workflow edits and runner-label changes should receive review. Neither approval rules nor YAML timeouts replace the billing budget.

## Optional distribution credentials

| Optional operation | Configuration; not required for ordinary CI |
|---|---|
| Windows signing using VCP's Azure OIDC pattern | Variables `AZURE_SIGNING_CLIENT_ID`, `AZURE_SIGNING_TENANT_ID`, `AZURE_SIGNING_SUBSCRIPTION_ID`; AIRK-specific federated identity, signing permissions and protected environment. No Azure client secret is needed with OIDC. |
| macOS Developer ID signing | Manual Signed macOS candidates workflow uses `MACOS_CERTIFICATE_P12` (base64 certificate and private key) and `MACOS_CERTIFICATE_PASSWORD` in the main-only `macos-signing` environment. |
| macOS notarization | The same environment supplies `APPLE_ID` and `APPLE_APP_SPECIFIC_PASSWORD`; the runner creates a temporary validated notarytool profile. |
| VS Code Marketplace publishing | Implemented manual workflow uses VSCE 4.0.0 OIDC trusted publishing and the `vscode-marketplace` GitHub environment restricted to main. Configure the Marketplace publisher's trust policy first; no PAT secret/fallback is consumed. |
| GitHub Release or GHCR publication | Usually GitHub's automatic token with scoped `contents: write` or `packages: write` in a separate publishing job; no personal token needed for same-repository publication. |

The separate [Signed macOS candidates workflow](../.github/workflows/macos-signing.yml) consumes only the four documented macOS environment secrets, on an explicit dispatch from main. [Native signing instructions](native-signing.md) describe provisioning, cleanup and artifact verification. Windows signing configuration remains a suggestion. Ordinary CI and Extension release do not consume signing secrets. Marketplace publication requires successful candidate builds and an explicit publishing dispatch. See [release workflow](release-workflow.md), [extension deployment](extension-deployment.md) and [macOS build instructions](macos-build.md).

The separate [Extended qualification workflow](../.github/workflows/qualification.yml) is manual only. It measures a configurable original corpus on Windows/Linux, runs process-kill recovery, and optionally downloads the pinned public CPython corpus or MiniLM model. It uploads numeric reports only, never source trees, model assets or index state. This longer workflow is separate from required PR checks and is not automatically started by a code change. macOS installed-service qualification remains separate from signing.
