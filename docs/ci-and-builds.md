# GitHub CI, native artifacts and spending controls

A one-release exception covers the [pinned 1.4.4 tooling repair](release-workflow.md#pinned-144-release-tool-repair). Its complete cumulative diff must stay within an explicit release-tool/test/document allowlist, retain version 1.4.4 and match the original 664-file runtime identity. Only then do PR/main CI run quick checks and Python contracts on all four hosts without repeating native/container builds. Other changes, missing history, manual CI dispatches and version 1.4.5 restore the ordinary plan. The original full main CI remains the authenticated producer.

The current development package version is **1.6.4**; see [its release record](releases/release-1.6.4.md). Prior release and qualification records retain their original scope.

Container compilation and ownership tests run as UID/GID 10001. Every cache-using
build step first repairs only its three mounted Cargo/target cache directories,
then drops privileges before running Cargo or tests. This covers fresh,
recreated and restored root-owned mounts; an earlier cached `chown` layer cannot
establish ownership for a later mount. Cache mounts are locked while in use. The
small `python scripts/check-container-build-cache.py` regression reproduces the
old permission failure and verifies cold, replaced, restored and warm caches,
the non-root execution identity and unchanged source permissions. It compiles no
Rust code and runs before the normal container build.

Native smoke jobs, the container adapter, service-pool campaign and installed-console fixture share one metadata retry policy. It honors only the native actor's explicit `registry_busy` contract within finite attempt/time budgets; an accepted index/configuration job is polled by the same receipt, never resubmitted. Deliberate negative tests require an exact expected failure code and do not retry it. The document-table smoke retains its original source, coordinate, extraction, budget and usage assertions. See [the protocol](service-protocol.md) for retry exclusions.

Native smoke and qualification tools distinguish a raw diagnostic status from an available publication. They share scoped, bounded observation helpers and preserve explicit failed readiness, typed read-retry limits and exact usage accounting; see the [MCP consumer contract](mcp.md). The mandatory 16-file/model service-client and document-table smokes remain routine on every native host. Local verification uses a freshly constructed matching release pair before a corrective push; hosted CI is not the primary debugging loop.

MCP smoke checks require the exact unified 27-tool catalog, including rejection of missing, duplicate and retired advertised names. Container and native client checks share that expected public catalog, while standalone extension frontend checks use the current 27-tool probe. The 17 prefixed native `airk tools` aliases remain a separate CLI compatibility contract.

The existing model-enabled native-client check also starts an owned inference worker, verifies real inference replies and requires a zero exit status after input EOF. Successful replies followed by a native shutdown abort fail qualification. This runs inside the existing native smoke jobs without another build matrix.

Mac contract jobs compile a tiny helper/library fixture to verify that development bundle signatures permit loading their ad-hoc library. Development candidates clear runtime flags because they have no Team ID; [Developer ID release signing](native-signing.md) applies hardened runtime and verifies the exact team on every signing component.

The full extension suite's two simultaneous-project scenarios declare their required resident capacity on the disposable service through `configure_resources`, verify admission and stay within detected host memory. An 8 GiB host's default 2 GiB service budget intentionally has one resident slot; waiting longer cannot satisfy a two-live-project test under that budget. Production defaults and low-budget admission checks remain unchanged. Mock onboarding/trust fixtures use canonical temporary roots so the real uninstall-receipt directory guard stays enabled on macOS.

Native builds embed the two pinned INT8 MiniLM exports. Before Cargo checks/builds, run `python scripts/provision-local-model.py --destination target/local-vector-model --download`, or supply verified offline assets there (or through `AIRK_BUNDLED_MODEL_DIR`). Also prepare native CPU libraries with `python scripts/provision-onnx-runtime.py --download`. Cargo never downloads model assets or runtime libraries. See [model provisioning](onnx-models.md).

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

Jobs install the repository-pinned Rust compiler/components, Python 3.11 and Node.js 24.15.0. They restore the checked-in npm lock with dependency scripts disabled and build with the locked Cargo graph. Routine checks are deliberately small on standard hosted runners:

- Linux runs workspace Clippy, all-feature workspace tests with the `ci` default filter, and doctests once. The separate extension job runs its mocked contracts; Python packaging/protocol contracts run on each host.
- Every native host runs the short `airk-process` ownership regressions. Windows and both Macs compile only the CLI library plus `native_read_retry`, `legacy_native`, `pdf_worker` and `unix_guardian` integration binaries. These use the default lightweight-main features and at most two test processes. The selected targets do not require inference/model features; on Windows the default and all-feature inventories contain the same 157 test identities (156 runnable, one ignored). These cover installer behavior, typed contention, source preservation and the native process regressions without compiling the entire workspace test matrix.
- All four hosts build the release executable pair, exercise short direct-client/shared offline-model and DOCX/XLSX smoke, verify managed installation and registered MCP before first publication, and verify native archives and platform VSIX packages. The VSIX includes digest-verified listing illustrations. The managed-install/MCP smoke runs three independent passes per host with no retries, so cold-verification and admission races surface before merge.
- PR CI runs the Linux x64, Windows x64 and Apple Silicon native jobs. macOS Intel runs on main after merge. The [plan script](../scripts/ci-plan.py) emits the native matrix; manual and qualification dispatches always cover all four hosts.
- The Linux container job builds the runtime image and runs the one-container collection smoke. It does not repeat workspace compilation/tests in routine builds. Container timing reports explicitly record this scope and contain no synthetic test results.

Dispatch **Actions / CI** with `qualification` enabled to restore workspace Clippy/tests/doctests on every native host, the full real native extension lifecycle and multi-project suite, container workspace tests/doctests, and the long workload, performance and fault campaigns. Those campaigns include the 200-edit retention plateau, large-catalog measurements, multi-page maintenance backlog, sparse-catalog measurements, ignored real-model performance regressions, model retrieval, code benchmark, ranking/recall, quota/relocation lifecycle, usage calibration and process-termination checks. Full qualification uses `--ignore-default-filter`; its assertions and workloads remain intact. Local `cargo test` is still unfiltered. A local container qualification build uses `docker build --build-arg AIRK_QUALIFICATION=true .`; default container builds omit the duplicate workspace suite.

On October 29, 2026, at the repository owner's request, main stopped rebuilding the targets that the merged PR had already tested. Measured runs for one change had built the full matrix twice: PR CI 40824252870 took 41 minutes and main CI 40828289966 took 38 minutes on the same tree. The releases that followed took 7 and 2 minutes. In the 16 failed CI runs from October 25 to 29, Windows failed most often (10 runs), so it stays in the PR gate. Every CI failure on macOS Intel coincided with an Apple Silicon failure, so Intel moved after merge, where it is also the slowest host. Main CI 40985769764 then failed on Windows in the pinned quick-status smoke, while PR CI 40980983335 passed on the identical tree. That fixture treated a documented typed read refusal as fatal. Shared retry helpers, repeated smoke passes and a local four-processor runner now target that class of failure; see [focused status regressions](#focused-status-and-mac-loader-regressions).

This policy changed on October 26, 2026 at the repository owner's request after per-platform workspace execution alone took 16-25 minutes on the Windows/macOS runners, in addition to test compilation and release builds. Dated pilot measurements below describe earlier policies. Exact-main candidate authentication, all four native artifact sets, version agreement and publication provenance remain mandatory. Routine CI does not claim the optional qualification campaigns or manual multi-project/multi-editor acceptance ran.

## Storage concurrency and fixture capacity

The `native_read_retry`, `legacy_native`, `pdf_worker` and `unix_guardian` binaries contain complete owned service/process fixtures. CI assigns them a nextest group with one slot, preserving concurrency exercised inside each test and leaving unrelated library cases parallel. The one-slot group also applies to these four binaries in Linux and full qualification; only the focused Windows/macOS step has the two-process global cap. Existing timeouts, exact registration/read assertions and zero automatic retries remain in force. The focused step enables existing process/fixture phase diagnostics for failure output. All-feature coverage remains on Linux and in explicit qualification; every native release still builds and exercises the real inference helper.

The October 26 Windows run at `59887cd` passed 154 focused tests but timed out the PDF fixture and failed another fixture during registration with repeated unknown registry responses. Both Macs passed that selection; all three other native hosts passed release/model/install/package gates. A local two-core/four-process reproduction passed the overlapping startup cluster, so CPU contention and the exact hosted actor stall remain unproven. The CI correction removes unnecessary debug inference payloads and bounds independent fixture concurrency as resource policy; it does not label that unknown cause as diagnosed. A local default-feature run with the new scheduling passes all 156 runnable tests in 25.5 seconds. See the [dated release record](releases/release-1.4.4.md).

Windows concurrent publication exposed `SQLITE_PROTOCOL` with the bundled SQLite 3.53.2: its VFS classified canonical extended local paths (`\\?\C:\...`) as UNC network paths and selected shared-handle WAL locking. The exact upstream SQLite 3.53.3 bundle in [Cargo.toml](../Cargo.toml) includes the [path classification fix](https://github.com/sqlite/sqlite/commit/2429af2e0ce2e4c643786f8d2533f79d974e3270). The [dependency notice](../THIRD_PARTY_NOTICES.md) records the source pin and package versions. The store's `sqlite_wal` integration test exercises concurrent read transactions and connection churn on a canonical path longer than 260 characters, without AIRK retries. The kernel's `publication_v2` test additionally checks publication, pinned metadata and search consistency.

Retention lifecycle fixtures configure a 512 MiB root and 128 MiB default collection allowance **before initialization**. SQLite precharges its maximum page allowance, so leaving a tiny fixture at the production 256 GiB default can instead fail on changing hosted-runner physical capacity. All fixture collections share the explicit policy; pressure tests then set their deliberate tighter limits. The 200-edit plateau and exact retained-object assertions remain in place. Physical admission remains enforced.

Real index/provider pressure tests live in the kernel's unit harness so successful single-retry reclamation can use a private, instance-scoped test budget (30 seconds) without exposing a production setting. Their first publication contains real data, and manual mode preserves a known obsolete backlog; no preparatory timed cleanup determines whether reclaimable payloads exist. The index test first proves that the same publication fails with typed quota pressure in manual mode and releases admission without advancing the head. Separate tests exhaust the explicit one-second test budget before retirement authorization: both index and provider publication must then fail with typed quota pressure, preserve every generation and the head, report pending maintenance, and release reservations. Explicit bounded maintenance must recover before a new request succeeds. Provider invocation counts remain exactly one per request, including the failed automatic retry. The test budget field is absent from production builds.

Recovery and idle-worker tests inspect maintenance receipts and continue explicitly reported pending work within a finite pass limit. A single bounded maintenance pass does not promise complete cleanup. Fault tests drain setup backlog and, if a pass expires before the injected stage, continue pending maintenance without publishing until that stage is reached. The recovery test deliberately exhausts the deadline after retirement authorization, reopens again, and requires complete cleanup with zero pending intents, the exact protected generations/objects, unchanged publication identity and no leaked reservations. Failed or quota-blocked maintenance is rejected instead of retried. Reader-release tests likewise require eventual removal while preserving the active and predecessor revisions.

Usage accounting has a separate best-effort contract: successful and failed jobs can both lose optional observations when nonblocking locks or the 10 ms quota-admission budget prevent recording. Scheduler integration tests run in isolated processes and require exactly one persisted observation or one visible drop for each job, correct collection attribution, exact recorded work, and no extra observation for idle or disabled workers. Held usage locks deterministically exercise loss for successful and failed jobs. Exact payload, reuse, failure, revision-baseline, feedback and privacy assertions use the existing strict store path in unit fixtures; production admission budgets and behavior are unchanged.

## Downloadable candidates

Ordinary CI and local extension packaging default to the stable channel for the checked-in version. A stable candidate is still an unpublished build artifact; release workflows verify and publish it separately. The [1.6.4 release notes](releases/release-1.6.4.md) describe current behavior and qualification limits, while the [October 17 release summary](releases/releases-2026-10-17.md) records earlier published versions. The container smoke verifies the exact unified 27-tool MCP catalog; the unsupported history endpoint remains absent. Future previews require an explicit channel and a distinct version.

Successful native jobs upload an artifact named `airk-native-<target>-<commit>-<run-id>-<attempt>` to the workflow run's Artifacts section. It contains:

- `airk-<version>-<target>.zip`: native executable, AIRK legal files and dependency notices.
- `manifest.json`: commit, Git tree, dirty-checkout flag, compiler versions, lock/toolchain hashes, full payload digests and archive digest. PR runs build GitHub's merge ref, so their artifact names and receipts carry that merge-ref commit; releases accept such a receipt only when its tree equals the released main commit's tree.
- `SHA256SUMS`: SHA-256 digests for the ZIP and manifest.

[The packager](../scripts/package-native.py) stages only these allowed inputs. It verifies every archive entry against the inventory, preserves Unix executable permissions, and runs the archived executable's version/help commands. It refuses an existing staging destination. Neither the repository tree nor `temp/`, index state, Cargo build directories or credentials are archived. GitHub retains build and test output in the run logs; failed jobs do not upload a successful candidate.

The manifest is a build record, not a signature or a complete reproducible-build attestation. Current dependency notice gaps remain documented in [third-party notices](../THIRD_PARTY_NOTICES.md). Public release qualification, signing and installed-editor tests are separate from candidate construction. The same run also uploads `airk-vsix-<target>-<commit>-<run-id>-<attempt>` containing the platform VSIX, extension manifest and SHA256SUMS. The VSIX manifest binds the native archive, its receipt, the npm lock and all staged runtime payload hashes. The extension package includes the pinned MIT-licensed `jsonc-parser` runtime and its original license for configuration editing; build dependencies remain excluded. See [extension deployment](extension-deployment.md) and [Windows local builds](windows-local-build.md).

To reproduce a candidate locally, substitute the target matching your Rust host:

```console
cargo build --release --locked -p airk-cli --target x86_64-pc-windows-msvc
python scripts/package-native.py --target x86_64-pc-windows-msvc
```

Use Python 3.9+ locally. Output is under ignored `artifacts/native/<target>/`. Retain or deliberately remove only that generated candidate directory before packaging again. Local uncommitted work is recorded as a dirty checkout.

## Checks selected by the change

Every PR starts the complete-diff classifier and quick checks. The classifier uses the PR merge base and both paths of moves/deletions. Unknown paths, workflow/build inputs and Rust changes select full validation, which on a PR means the three PR native hosts plus container, extension and four-host contract checks. Manual CI always builds complete candidates.

On a main push, the plan job tries to promote the merged PR's candidates before selecting work:

1. The main commit must be the merge commit of exactly one merged PR from `iokaio/airk`; forks are never promoted.
2. The newest successful `pull_request` CI run on that PR's final head must have succeeded in its plan, quick, container, extension, `Required checks`, all four contract jobs and the three PR native jobs.
3. Its six native/VSIX artifacts must be unexpired, digest-bound to that run and attempt, and named for one merge-ref commit.
4. Each downloaded native receipt must record that merge-ref commit, a clean checkout and a Git tree identical to the main commit's tree.

When all four conditions hold, main uploads `airk-promotion-<commit>-<run>-<attempt>` and runs only quick checks and the macOS Intel native job; `Required checks` then requires exactly those. Anything else falls back to the complete four-host main build, including any API failure or a push that changes `Cargo.lock`, a `Cargo.toml`, `rust-toolchain.toml`, `.cargo/`, `Dockerfile`, `scripts/ci-prune-target.py`, `.config/nextest.toml` or `.github/workflows/ci.yml`. Those inputs key the main-only dependency caches, so the fallback also refreshes the caches through real jobs.

| PR change | Required work |
|---|---|
| Markdown documentation only | Formatting, workflow validation, repository/link checks, classifier tests and whitespace |
| Extension README/media or listing illustrations | Quick checks plus extension compilation/tests and packaging contracts; illustrations also select platform script contracts |
| Extension TypeScript/package configuration | Real native adapter and candidate checks on Linux, Windows and Apple Silicon (Intel after merge), plus extension checks |
| Script contract tests only | Quick checks and Python contracts on all four platforms |
| Rust, dependency, Docker, workflow, executable script or unknown input | Native checks on Linux, Windows and Apple Silicon (Intel after merge), four-host Python contracts, extension and container validation |

`Required checks` always runs. It requires success for every selected job and an intentional skip for every unselected job. A failed plan, cancellation, unexpected skip or missing result cannot pass. `signed-off` remains independently required; branch protection is unchanged.

## Test execution and measurements

The required extension suite exercises upgrade routing using the version in `package.json`, including existing 1.4, 1.5 and 1.6 installations, explicit repair/retry and quiet reopen. A future version bump that is missing from the automatic-upgrade policy fails these tests. Bootstrap tests separately cover interrupted predecessor activation and preserve mismatched-runtime, managed-ownership and rollback protections. Packaging contracts require the standalone uninstall hook to survive script stripping.

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
- Each native target is built and tested once per merged change: Linux, Windows and Apple Silicon in PR CI, macOS Intel on main. Main records the identical-tree PR producer and its own Intel build as one candidate (`airk-ci-producer/2`), or builds all four itself when promotion does not apply (`airk-ci-producer/1`). GitHub and Marketplace release workflows authenticate and promote those exact artifacts instead of invoking the Rust matrix again. A separate tag workflow verifies published release provenance; unverified tags fail with an instruction to validate a main candidate.
- The native matrix runs together. Compiler concurrency is four on x64 and remains two on Apple Silicon for memory headroom. Test scheduling is independently bounded. Platform clippy checks remain; formatting runs once. Intel's existing 120-minute job ceiling remains until hosted measurements justify reducing it; other full jobs retain 90 minutes.
- SHA-pinned `rust-cache` keeps dependency outputs, excludes workspace crates and writes only from main push CI. PRs may restore main caches but cannot update them. Incremental compilation remains disabled; runtime/release optimization settings are unchanged.
- Docker tests precede optimized compilation. BuildKit layer caching is supplemented by explicit registry/git/target cache-mount injection and extraction. Workspace outputs are pruned before exporting dependency caches. PR builds do not write shared Docker caches. A cache miss performs the same required checks.
- Docker dependency cache keys separate the Linux x64/toolchain/Dockerfile/pruning configuration from Cargo manifests and the lockfile. If a version or dependency change misses the exact key, a fallback restores the newest cache with the same configuration prefix. Cargo still checks fingerprints and the locked graph, rebuilding changed dependencies; main saves the updated cache under the new exact key. There is no fallback across toolchain or Dockerfile changes. The first run in the `v3` namespace is cold, and `fresh_build` still disables restores and saves.
- Dispatch **CI** with `qualification` enabled for the full platform/container suites and long native campaigns; add `fresh_build` to disable native and Docker cache restores for a cold run. A successful manual run on the exact main commit can be supplied as `candidate_run_id`. Missing/expired candidates fail early with this rebuild instruction; releases never silently start another long build.
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
| GitHub Release publication | Manual GitHub release workflow; automatic token with `contents: write` in the final `github-releases` publishing job. A separate version-PR fallback has contents/PR write access only to prepare the next unused patch. No personal token needed. |
| Downloads website | GitHub Pages for `airk.ioka.io`, main-only `github-pages` environment; automatic token with `pages: write` and `id-token: write` only for deployment. Refreshes after release publication, website changes on main, or a manual dispatch. |

The separate [Signed macOS candidates workflow](../.github/workflows/macos-signing.yml) consumes only the four documented macOS environment secrets, on an explicit dispatch from main. The [GitHub release workflow](../.github/workflows/github-release.yml) promotes authenticated main CI artifacts, signs Windows through Azure OIDC and both Mac architectures through the Apple environment, then optionally publishes the complete verified download set under a clean package-version tag. [Native signing instructions](native-signing.md) describe provisioning, cleanup and artifact verification. Ordinary CI and Extension release do not consume native-signing secrets. Marketplace publication remains separate. See [release workflow](release-workflow.md), [extension deployment](extension-deployment.md) and [macOS build instructions](macos-build.md).

The separate [Extended qualification workflow](../.github/workflows/qualification.yml) is manual only. It measures a configurable original corpus on Windows/Linux, runs process-kill recovery, and optionally downloads the pinned public CPython corpus or MiniLM model. It uploads numeric reports only, never source trees, model assets or index state. This longer workflow is separate from required PR checks and is not automatically started by a code change. macOS installed-service qualification remains separate from signing.

Debug/test builds optimize the SHA-256 dependency because integration fixtures authenticate complete native images and offline model assets. Existing digest, ownership and deadline checks remain enabled; release builds retain their existing optimization profile. macOS installer fixtures use canonical temporary roots and app-bundle-aware payload paths rather than relaxing redirect checks.

Managed MCP smoke separates cold quick status from a confirmed observation. A two-second `status_timeout` must disclose unknown readiness and null evidence; it is not proof that an index is unavailable. Source-bound and pinned MCP frontends retain one session-owned installation-verification task across request timeouts, report its real failures, and cancel it when the session closes. Smoke requires cold quick-status recovery without a diagnostics warm-up, then checks registration, publication counts and cache invalidation. Explicit timeout hints and the existing typed safe-read refusals permit bounded same-scope observations within the original deadline; neither counts as readiness. Permanent errors, submitted queries and transport failures do not authorize replay. Pre-publication diagnostics still require confirmed unavailable evidence and zero inference loads/sessions. Production deadlines, payload/control/health verification and direct-query receipt safeguards remain enabled.

### Focused status and Mac loader regressions

Run these checks while investigating cold-status timing, before another hosted run:

```text
npm run test:status --prefix packages/vscode
cargo test -p airk-mcp quick_status -- --nocapture
python -m unittest scripts.tests.test_service_runtime_build.ServiceRuntimeBuildTests.test_development_mac_helper_loads_its_adhoc_signed_library -v
```

The process tests launch small Node children to verify actual exit codes, typed refusal handling, diagnostic preservation and admission deadlines. The Rust tests cover timeout, refusal and recovery without invented evidence; `quick_status_forwards_typed_admission_refusal_after_a_deadline_without_evidence` reproduces the hosted sequence deterministically: a deadline expires, then the abandoned query's admission slot causes an unsubmitted busy refusal that the frontend forwards unchanged.

Session reads in native tests go through `requestWithReadRetry` or `observeQuickStatus`, which repeat only a complete typed hint within the original deadline and report any other failure with a bounded copy of the reply. [The contract test](../scripts/tests/test_native_read_contract.py) rejects unexplained raw native MCP calls and error assertions that omit the reply.

To reproduce hosted Windows timing, run `pwsh -File scripts/ci-local.ps1 -Cores 4 -Repeat 5` (add `-Load 2` for competing CPU work). It restricts itself and every child process, including detached service roles, to the requested processors, then repeats the routine native smokes without retries. The loader test runs a real ad-hoc signed helper/library pair on macOS and explicitly skips other hosts; it does not qualify ONNX inference.

For real native cold-status coverage, set `AIRK_TEST_BINARY` to the main executable in an existing qualified runtime pair, then run `node --test packages/vscode/out/test/nativeSmoke.test.js`. This covers source-bound and pinned frontends, registration, cache invalidation and zero inference loads. It skips when the binary is unset. Actual semantic inference and helper shutdown remain covered separately by `python scripts/check-service-clients.py --runtime-dir <runtime-directory> --files 16 --model --report <report.json>`; a loader or status pass alone does not establish semantic readiness.

The real-model kernel step passes an absolute `AIRK_TEST_MODEL_DIR` rooted at the checked-out GitHub workspace. Cargo runs each integration target from its crate directory, so a repository-relative asset path is not a valid test input. The pinned asset verification and model/retrieval assertions remain unchanged.

For the 1.5.12 process changes, routine native CI also runs the offline installed-runtime lifecycle fixture. It verifies generated role executables and helper identity, real MCP EOF/abrupt client loss through stable and universal launchers, preservation of another live session and the shared service, and owned retirement. This focused installer gate covers the changed lifecycle on the existing four native hosts; it does not enable the broad opt-in qualification matrix.
