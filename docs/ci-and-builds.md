# GitHub CI, native artifacts and spending controls

The [CI workflow](../.github/workflows/ci.yml) uses standard GitHub-hosted runners. No self-hosted machine, Docker account, Azure account, Apple account, custom repository secret or repository variable is needed for the current builds. GitHub supplies the job token and artifact-upload authorization automatically. Repository Actions must be enabled and permitted to use the SHA-pinned actions, including rust-cache, install-action and the Docker cache-mount adapter.

Main requires PRs and the stable `Required checks` / `signed-off` gates. Use **Actions → CI** for manual builds and **Extension release** to promote a successful main candidate into the chosen Marketplace channel. The [release workflow guide](release-workflow.md) describes artifact verification and external Entra identity/publisher membership prerequisites. Publishing jobs are separate from read-only builds.

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

## Storage concurrency and fixture capacity

Windows concurrent publication exposed `SQLITE_PROTOCOL` with the bundled SQLite 3.53.2: its VFS classified canonical extended local paths (`\\?\C:\...`) as UNC network paths and selected shared-handle WAL locking. The exact upstream SQLite 3.53.3 bundle in [Cargo.toml](../Cargo.toml) includes the [path classification fix](https://github.com/sqlite/sqlite/commit/2429af2e0ce2e4c643786f8d2533f79d974e3270). The [dependency notice](../THIRD_PARTY_NOTICES.md) records the source pin and package versions. The store's `sqlite_wal` integration test exercises concurrent read transactions and connection churn on a canonical path longer than 260 characters, without AIRK retries. The kernel's `publication_v2` test additionally checks publication, pinned metadata and search consistency.

Retention lifecycle fixtures configure a 512 MiB root and 128 MiB default collection allowance **before initialization**. SQLite precharges its maximum page allowance, so leaving a tiny fixture at the production 4 GiB default can instead fail on changing hosted-runner physical capacity. All fixture collections share the explicit policy; pressure tests then set their deliberate tighter limits. The 200-edit plateau and exact retained-object assertions remain in place. Physical admission and production default quotas are unchanged.

Real index/provider pressure tests live in the kernel's unit harness so successful single-retry reclamation can use a private, instance-scoped test budget (30 seconds) without exposing a production setting. Their first publication contains real data, and manual mode preserves a known obsolete backlog; no preparatory timed cleanup determines whether reclaimable payloads exist. The index test first proves that the same publication fails with typed quota pressure in manual mode and releases admission without advancing the head. Separate tests exhaust the unchanged production one-second budget before retirement authorization: both index and provider publication must then fail with typed quota pressure, preserve every generation and the head, report pending maintenance, and release reservations. Explicit bounded maintenance must recover before a new request succeeds. Provider invocation counts remain exactly one per request, including the failed automatic retry. The test budget field is absent from production builds.

Recovery and idle-worker tests inspect maintenance receipts and continue explicitly reported pending work within a finite pass limit. A single default one-second pass does not promise complete cleanup. Fault tests drain setup backlog and, if a pass expires before the injected stage, continue pending maintenance without publishing until that stage is reached. The recovery test deliberately exhausts the deadline after retirement authorization, reopens again, and requires complete cleanup with zero pending intents, the exact protected generations/objects, unchanged publication identity and no leaked reservations. Failed or quota-blocked maintenance is rejected instead of retried. Reader-release tests likewise require eventual removal while preserving the active and predecessor revisions.

Usage accounting has a separate best-effort contract: successful and failed jobs can both lose optional observations when nonblocking locks or the 10 ms quota-admission budget prevent recording. Scheduler integration tests run in isolated processes and require exactly one persisted observation or one visible drop for each job, correct collection attribution, exact recorded work, and no extra observation for idle or disabled workers. Held usage locks deterministically exercise loss for successful and failed jobs. Exact payload, reuse, failure, revision-baseline, feedback and privacy assertions use the existing strict store path in unit fixtures; production admission budgets and behavior are unchanged.

## Downloadable candidates

Ordinary CI and local extension packaging default to the stable channel for 1.0.0. A stable candidate is still an unpublished build artifact; release workflows verify and publish it separately. Future previews require an explicit channel and a distinct version.

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

## Checks selected by the change

Every PR starts the complete-diff classifier and quick checks. The classifier uses the PR merge base and both paths of moves/deletions. Unknown paths, workflow/build inputs and Rust changes select full validation. Main pushes and manual CI always build complete candidates.

| PR change | Required work |
|---|---|
| Markdown documentation only | Formatting, workflow validation, repository/link checks, classifier tests and whitespace |
| Extension README/media or listing illustrations | Quick checks plus extension compilation/tests and packaging contracts; illustrations also select platform script contracts |
| Extension TypeScript/package configuration | Real native adapter and candidate checks on all four platforms, plus extension checks |
| Script contract tests only | Quick checks and Python contracts on all four platforms |
| Rust, dependency, Docker, workflow, executable script or unknown input | Complete four-platform native checks, Python/extension contracts and container validation |

`Required checks` always runs. It requires success for every selected job and an intentional skip for every unselected job. A failed plan, cancellation, unexpected skip or missing result cannot pass. `signed-off` remains independently required; branch protection is unchanged.

## Test execution and measurements

Nextest **0.9.148** runs each test in its own process on all four native targets and in Docker. [The checked-in profile](../.config/nextest.toml) disables retries, bounds storage-heavy concurrency, starts the 200-publication retention test early and produces a JUnit report without captured test payloads. Doctests run separately with Cargo. Existing ignored tests retain their explicit fixture requirements. The 200 real edits, exact retained-object checks, quota/lease exclusion and crash-recovery assertions remain required.

The kernel's integration modules share one executable instead of 45. Self-spawning test selectors include the module name; the usage child must report that it executed one test. Fixture includes and documentation links follow the new layout. The other crates retain their existing targets pending measurements of this pilot. Ordinary `cargo test` defaults to two libtest threads through the Cargo configuration, preventing high-core developer machines from starting dozens of storage fixtures together; explicit environment/command-line overrides remain available. Nextest independently controls its process scheduler.

The relocation regression models an inherited Unix open file description with a duplicate descriptor. Closing the original alone left the exclusive lifecycle lock held. The relocation guard now explicitly unlocks after subordinate locks are dropped; shared entrants still fail immediately during an active relocation. The regression failed before the change and passed afterward in Linux, followed by 20 clean parallel relocation repetitions.

Native job summaries and `airk-ci-timings-*` artifacts separate clippy, test compilation, execution, doctests and optimized compilation, and list the slowest tests. Docker prints the same phases and exports reports after a successful build. Failed container builds retain their phase/test diagnostics in the build log. Cache actions report transfer sizes and durations separately. Compare cold and warm runs on the same platform; local timings and cache hits are not reproducibility attestations or cross-platform qualification.

### First hosted pilot: 2026-10-16

[PR #13 CI run 38967989160](https://github.com/iokaio/airk/actions/runs/38967989160) passed all four native platforms, container smoke, extension and script contracts, and the required gate. Its critical path was approximately 54 minutes. The following phase durations come from its numeric timing artifacts, rounded to the nearest second; job totals also include tool installation, packaging and artifact transfer.

| Platform | Clippy | Test compilation | Test execution | Optimized compilation | Whole job |
|---|---:|---:|---:|---:|---:|
| Linux x64 | 2:10 | 2:23 | 10:50 | 8:29 | 25:02 |
| Windows x64 | 3:46 | 5:33 | 21:39 | 17:23 | 50:58 |
| macOS Apple Silicon | 3:31 | 4:09 | 15:34 | 13:46 | 38:20 |
| macOS Intel | 5:45 | 4:54 | 23:39 | 16:20 | 53:18 |
| Linux container | — | 3:17 | 6:36 | 7:31 | 19:00 |

The new native cache namespace had not yet been populated by a main push; the Windows and Intel logs explicitly report no cache found. This is a first-run measurement, not a warm-cache result. The [previous PR run 38956054308](https://github.com/iokaio/airk/actions/runs/38956054308) took 51:45 on Windows and 53:43 on Intel; its container job failed after 25:39. Those runs had different cache conditions, so their totals do not isolate the effect of consolidation or scheduling. The pilot establishes a passing full matrix and working measurements, not a 25-minute full-CI target.

The unchanged 200-edit retention test remains the slowest test: 7:12 on Linux, 11:22 on Windows, 9:55 on Apple Silicon and 15:06 on Intel. It performs real publications and verifies the retained-object plateau. These observations do not justify reducing its edit count, weakening assertions or removing Intel's timeout headroom.

After merging, main CI must produce its own exact-commit candidate and populate trusted dependency caches. Subsequent measurements should establish cache save/restore sizes and time, a warm run on the same platform, and successful candidate authentication before release promotion. A documentation-only PR can independently verify that the classifier skips native/container jobs while the required gate and DCO still pass. Preserve these distinctions when reporting pipeline improvements; release promotion and publication need their own evidence.

## Controls that reduce unnecessary work

- Branch pushes outside main use PR validation; newer commits cancel superseded runs for that PR. There is no schedule.
- Main produces the complete authoritative candidate once. GitHub and Marketplace release workflows authenticate and promote its exact artifacts instead of invoking the Rust matrix again. A separate tag workflow verifies published release provenance; unverified tags fail with an instruction to validate a main candidate.
- The native matrix runs together. Compiler concurrency is four on x64 and remains two on Apple Silicon for memory headroom. Test scheduling is independently bounded. Platform clippy checks remain; formatting runs once. Intel's existing 120-minute job ceiling remains until hosted measurements justify reducing it; other full jobs retain 90 minutes.
- SHA-pinned `rust-cache` keeps dependency outputs, excludes workspace crates and writes only from main push CI. PRs may restore main caches but cannot update them. Incremental compilation remains disabled; runtime/release optimization settings are unchanged.
- Docker tests precede optimized compilation. BuildKit layer caching is supplemented by explicit registry/git/target cache-mount injection and extraction. Workspace outputs are pruned before exporting dependency caches. PR builds do not write shared Docker caches. A cache miss performs the same required checks.
- Dispatch **CI** with `fresh_build` enabled to disable both native and Docker restores for cold qualification. For a release, supply that successful run's ID as `candidate_run_id`. Missing/expired candidates fail early with this rebuild instruction; releases never silently start another long build.
- Candidates, producer records and numeric timing reports expire after seven days. Public release assets have independent retention. Cache paths exclude corpora, models, private state, credentials and release staging directories.
- Builds have read-only repository permissions and no signing or publishing credentials. Forks use `pull_request`, never privileged `pull_request_target` execution.

Concurrency and timeouts bound each run; they do not enforce monthly spending. Measure several cold/warm runs before promising a duration. The documentation-only target is under five minutes excluding queue time; full-matrix speedups remain subject to hosted evidence.

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
| VS Code Marketplace publishing | Pinned VSCE 4.0.1-3 uses Microsoft Entra authentication after GitHub OIDC login in the main-only `vscode-marketplace` environment. Configure `AZURE_MARKETPLACE_CLIENT_ID`, `AZURE_MARKETPLACE_TENANT_ID`, federation and publisher Contributor membership. The explicit `verify_identity` mode reports the Marketplace profile ID without building or publishing. No client secret or PAT fallback is consumed. |
| GitHub Release publication | Manual numbered GitHub release workflow; automatic token with `contents: write` only in the final `github-releases` publishing job. No personal token needed. |
| Downloads website | GitHub Pages for `airk.ioka.io`, main-only `github-pages` environment; automatic token with `pages: write` and `id-token: write` only for deployment. Refreshes after release publication, website changes on main, or a manual dispatch. |

The separate [Signed macOS candidates workflow](../.github/workflows/macos-signing.yml) consumes only the four documented macOS environment secrets, on an explicit dispatch from main. The [GitHub release workflow](../.github/workflows/github-release.yml) promotes authenticated main CI artifacts, signs Windows through Azure OIDC and both Mac architectures through the Apple environment, then optionally publishes the complete verified download set under an automatically numbered release tag. [Native signing instructions](native-signing.md) describe provisioning, cleanup and artifact verification. Ordinary CI and Extension release do not consume native-signing secrets. Marketplace publication remains separate. See [release workflow](release-workflow.md), [extension deployment](extension-deployment.md) and [macOS build instructions](macos-build.md).

The separate [Extended qualification workflow](../.github/workflows/qualification.yml) is manual only. It measures a configurable original corpus on Windows/Linux, runs process-kill recovery, and optionally downloads the pinned public CPython corpus or MiniLM model. It uploads numeric reports only, never source trees, model assets or index state. This longer workflow is separate from required PR checks and is not automatically started by a code change. macOS installed-service qualification remains separate from signing.
