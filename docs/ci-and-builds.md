# GitHub CI, native artifacts and spending controls

A historical one-release exception covered the [pinned 1.4.4 tooling repair](release-workflow.md#pinned-144-release-tool-repair). That pinned repair required its complete cumulative diff to stay within an explicit release-tool/test/document allowlist, retain version 1.4.4 and match the original 664-file runtime identity. Only that exact repair allowed PR/main CI to run quick checks and Python contracts on all four hosts without repeating native/container builds. Other changes, missing history, manual CI dispatches and version 1.4.5 restore the ordinary plan. The original full main CI remained its authenticated producer. This exception does not apply to the current 1.8.16 source or its 1.8.14 implementation base.

For local development, use the [scoped Rust iteration workflow](../CONTRIBUTING.md#local-rust-iteration): one coordinated Cargo owner, warm per-checkout targets and retained timings. Its Windows incremental override applies to development Clippy only. Required CI/release scopes and checked-in compilation profiles below are unchanged; a local scoped pass does not qualify them.

The current implementation version is **1.8.16**, the clean-break baseline for
all earlier installations. Supported later upgrades require the persisted state
contract; public 1.9.0 is a future release after qualification. Reset/rebuild and
owned-artifact cleanup replace historical preserved-index migration acceptance.
The current implementation builds on the unpublished
1.8.14 single-executable implementation; it is not a qualified release. See the
[current baseline record](releases/release-1.8.16.md), the broader
[1.8.14 working record](releases/release-1.8.14.md) and
[implementation plan](one-airk.md). The 1.8.8 and 1.8.12 checkpoints below remain
dated historical evidence. The separate [dependency audit](dependency-advisories.md)
workflow runs only when lockfiles or the advisory policy change, weekly, and on
request; routine CI is unchanged. Prior release and qualification records retain
their original scope.

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

Native smoke and qualification tools distinguish a raw diagnostic status from an available publication. They share scoped, bounded observation helpers and preserve explicit failed readiness, typed read-retry limits and exact usage accounting; see the [MCP consumer contract](mcp.md). The mandatory 16-file/model service-client and document-table smokes remain routine on every native host. Local verification uses a freshly constructed matching source-sealed runtime (one modern main; pairs only for qualified predecessor fixtures) before a corrective push; hosted CI is not the primary debugging loop.

MCP smoke checks require the exact unified 29-tool catalog, including rejection of missing, duplicate and retired advertised names. Container and native client checks share that expected public catalog, while standalone extension frontend checks use the current 29-tool probe. In the unpublished 1.8.14 implementation, CLI `tools list/help/call/shell` and public HTTPS use the same public catalog and alias validation. The 19 native evidence/help/plan/scope operations remain a backend subset; native administrative commands retain their separate authority.

The model-enabled native-client gate must exercise actual inference through the modern in-process owner, real scoped HTTPS reads and native shutdown. The schema-1 predecessor branch retains its actual helper/EOF/zero-exit checks; successful replies followed by a native shutdown abort are not qualification. These remain existing smoke gates, not an extra speculative matrix.

Historical Mac loader-contract coverage compiles a tiny helper/library fixture to verify that development bundle signatures permit loading their ad-hoc library. Those ad-hoc fixture candidates clear runtime flags because they have no Team ID; [Developer ID release signing](native-signing.md) applies hardened runtime and verifies the exact team on every signing component.

The full extension suite's two simultaneous-project scenarios declare their required resident capacity on the disposable service through `configure_resources`, verify admission and stay within detected host memory. An 8 GiB host's default 2 GiB service budget intentionally has one resident slot; waiting longer cannot satisfy a two-live-project test under that budget. Production defaults and low-budget admission checks remain unchanged. Mock onboarding/trust fixtures use canonical temporary roots so the real uninstall-receipt directory guard stays enabled on macOS.

Native builds embed the two pinned INT8 MiniLM exports. Before Cargo checks/builds, run `python scripts/provision-local-model.py --destination target/local-vector-model --download`, or supply verified offline assets there (or through `AIRK_BUNDLED_MODEL_DIR`). Modern 1.8.14 builds require verified static CPU ORT 1.22.1 inputs; the historical `provision-onnx-runtime.py` NuGet archive supplies dynamic libraries and cannot satisfy that requirement. Static provisioning and each target's actual model/import qualification remain required before release. Cargo must not download or substitute model assets, an ambient ORT installation or Pyke's different ORT version. See [model provisioning](onnx-models.md).

The modern builder compiles only `airk` with `local-inference`, which embeds the approved assets. Its schema-2 `airk-runtime.json` binds that one executable; the separate complete installer inventory still binds every legal/resource/build file. No embedding executable, launcher copy, external model or ORT library belongs in a modern payload. Schema-1 historical pair receipts remain strictly decoded for predecessor qualification. The build inspects actual native imports and runs both embedded inventory verification and `--verify-model-smoke`; the archive packager repeats the real embedded model smoke after extraction. These checks are distinct from client parity, process ancestry, minimum-OS and numerical-oracle qualification.

On macOS the modern main stays inside `AIRK.app`. Signing seals and verifies that exact bundle, then reseals the runtime/build/installer metadata outside it. The 1.8.8–1.8.13 standalone copied-launcher checks remain historical format checks. Modern signing must preserve the exact channel plists and the complete native/VSIX/promotion/Marketplace inventory graph; it never copies the sealed main out of its app or drops final signed payload verification.

The opt-in kernel vector-component qualification selects the S8 profile on ARM64 and the U8 profile on x64. Its `AIRK_TEST_MODEL_DIR` must match the native CPU policy; selecting the other verified export is still refused. The Neo P6 campaign exposed and corrected the previous x64-only fixture selection without changing the component workloads, assertions or routine CI selection.

The [CI workflow](../.github/workflows/ci.yml) uses standard GitHub-hosted runners. No self-hosted machine, Docker account, Azure account, Apple account, custom repository secret or repository variable is needed for the current builds. GitHub supplies the job token and artifact-upload authorization automatically. Repository Actions must be enabled and permitted to use the SHA-pinned actions, including rust-cache, install-action and the Docker cache-mount adapter.

Main requires PRs and the stable `Required checks` / `signed-off` gates. Use **Actions → CI** for manual builds. **GitHub release** signs and assembles the authenticated main candidates; **Extension release** consumes that exact signed assembly for Marketplace through `signed_run_id`. Final Marketplace publication requires matching published GitHub assets first. The [release workflow guide](release-workflow.md) describes artifact verification and external Entra identity/publisher membership prerequisites. Publishing jobs are separate from read-only builds.

## Current 1.8.14 source gates

The implementation is unpublished. No older hosted run, source-only test,
reused static-library receipt or renderer mock qualifies a new native installer.
The [working record](releases/release-1.8.14.md) records actual commands, identities,
failures/skips and remaining Windows/Mac Neo acceptance. Keep dated evidence
below attributable to its original source and payload.

The native build uses `airk --features local-inference` only for the distributed
runtime. Verify embedded assets with `--verify-bundled-models`, execute
`--verify-model-smoke`, inspect real target imports and validate the complete
archive/VSIX/signed-promotion graph. Default lightweight test targets and explicit
legacy-helper components remain separate gates, not modern payload roles.
Registry contract 13 requires in-process-provider/embedded-source semantics
regardless of schema-12 physical DDL compatibility; target capability checks,
checkpoint recovery and old-process ownership must be exercised by native tests.

CI/native/release static provisioning runs
`provision-static-onnx-runtime.py --download --download-tools --build --parallel 2`
for the exact target and consumes its five verified environment fields through
`--github-env`. Local runners may use `--environment-file`; never evaluate that
JSON as shell code or import arbitrary environment keys. Modern builds reject
missing/mismatched static input receipts. Original compiled library reuse keeps
its original build identity and does not become a fresh build observation.

The container uses a dedicated cached static-inference build stage. Production
runtime contains no Python; the test/smoke target adds Python for actual HTTPS
checks under `--network none`. Its public-copy input receipt is
`actual-public-container-copy-inputs/1`, not invented Git provenance. No sources,
private state, credentials or build caches belong in exported artifacts. See the
[container builder](../scripts/build-service-container.py).

The [public input manifest](../scripts/container-inputs.json) retains the existing
crate, model and license trees and admits external inputs file by file. Cheap
inventory tests compare the manifest with Docker COPY/context rules and resolve
Rust includes, manifest-relative reads and the CLI source-map table. External
documentation, parser corpora and extension fixtures are test inputs; they remain
outside the distributed runtime's production-source identity. Parent traversal
rules exclude neighboring files, and the existing private-state exclusions stay
in force. Gzip build-tool extraction validates, copies and hashes one sequential
pass, then sorts receipts and verifies the tree. A late invalid member leaves
earlier safe extracted bytes as failed-attempt evidence. Extraction speed and
the final container remain unqualified until the recorded Docker run completes.

MCP qualification uses the actual saved HTTPS URL and protected native
connection/grant, not a per-client stdio helper or assumed fixed port. Actual
Code/client CA/auth provisioning, port relocation, complete CLI/MCP/Workbench
results and typed read-retry exclusions remain required. Internal HTTPS success
is not external-client trust qualification. Linux Code's explicit distro/NSS
trust prerequisite remains disclosed in [agent setup](agent-integration.md).

Modern native-client fixtures stage and activate an approved managed payload,
create reviewed native authority and use actual HTTPS for the lifecycle checks.
Their successful shutdown evidence is the exact managed Stop receipt:
`managed_stop_complete` requires that proof. `service_clean_exit` is null because
the fixture does not own the manager's service process as a `Popen` child.
Predecessor-upgrade fixtures retain the genuine 1.8.12 pair/launcher baseline,
compare complete read envelopes and honor only typed safe read retries. Held
Windows HANDLE/Linux pidfd observations bind exact approved old private images
to PID/start identity/file hash, then observe exit and rescan. Those bounded
observations do not certify global process completeness or authorize killing
unrecognized owners. Actual runs of the updated fixtures remain qualification
work; their implementation is not a passing result.

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
- Every native host runs the short `airk-process` ownership regressions and a separate `cargo check -p airk-cli --no-default-features --all-targets` lightweight component gate. Routine Windows and both Mac CLI regressions select the actual shipping features with `--no-default-features --features local-inference`, at most two test processes, and the library plus `native_read_retry`, `legacy_native`, `pdf_worker`, `unix_guardian`, `native_worker_storage`, `combined_reads` and `explorer_assistance` integration binaries. They cover installer behavior, inference-owner receipts, typed contention, source preservation and native process regressions without compiling the entire workspace test matrix. The earlier 157-test lightweight Windows inventory is historical evidence; it does not identify the current shipping-feature test set.
- All four hosts must build the release runtime, exercise short direct-client/shared offline-model and DOCX/XLSX smoke, verify managed installation and registered MCP before first publication, and verify native archives and platform VSIX packages. The VSIX includes digest-verified listing illustrations. The managed-install/MCP smoke runs once per host, followed by two extra cold-client regression passes retained after the earlier Intel setup-deadline regression. Qualification runs three full passes; no failed pass is retried.
- PR CI runs the Linux x64, Windows x64 and Apple Silicon native jobs. macOS Intel runs on main after merge. The [plan script](../scripts/ci-plan.py) emits the native matrix; manual and qualification dispatches always cover all four hosts.
- The Linux container job builds the runtime image and runs the one-container collection smoke. It does not repeat workspace compilation/tests in routine builds. Container timing reports explicitly record this scope and contain no synthetic test results.

Dispatch **Actions / CI** with `qualification` enabled to restore workspace Clippy/tests/doctests on every native host, the full real native extension lifecycle and multi-project suite, container workspace tests/doctests, and the long workload, performance and fault campaigns. Those campaigns include the 200-edit retention plateau, large-catalog measurements, multi-page maintenance backlog, sparse-catalog measurements, ignored real-model performance regressions, model retrieval, code benchmark, ranking/recall, quota/relocation lifecycle, usage calibration and process-termination checks. Full qualification uses `--ignore-default-filter`; its assertions and workloads remain intact. Local `cargo test` is still unfiltered. A local container qualification build uses `docker build --build-arg AIRK_QUALIFICATION=true .`; default container builds omit the duplicate workspace suite.

The separate [Extended qualification workflow](../.github/workflows/qualification.yml)
is an explicit Linux x64/Windows x64 scale and abrupt-publication-recovery
campaign, with optional pinned public corpus and transformer checks. Its matrix
records the exact native target, provisions pinned model assets and static ORT
with `--download --download-tools --build --parallel 2 --github-env`, and builds
the verified single-executable runtime before the campaign. It does not use the
historical dynamic ORT provisioner and does not replace four-host or installed
client qualification.

On October 29, 2026, at the repository owner's request, main stopped rebuilding the targets that the merged PR had already tested. Measured runs for one change had built the full matrix twice: PR CI 40824252870 took 41 minutes and main CI 40828289966 took 38 minutes on the same tree. The releases that followed took 7 and 2 minutes. In the 16 failed CI runs from October 25 to 29, Windows failed most often (10 runs), so it stays in the PR gate. Every CI failure on macOS Intel coincided with an Apple Silicon failure, so Intel moved after merge, where it is also the slowest host. Main CI 40985769764 then failed on Windows in the pinned quick-status smoke, while PR CI 40980983335 passed on the identical tree. That fixture treated a documented typed read refusal as fatal. Shared retry helpers, repeated smoke passes and a local four-processor runner now target that class of failure; see [focused status regressions](#focused-status-and-mac-loader-regressions).

This policy changed on October 26, 2026 at the repository owner's request after per-platform workspace execution alone took 16-25 minutes on the Windows/macOS runners, in addition to test compilation and release builds. Dated pilot measurements below describe earlier policies. Exact-main candidate authentication, all four native artifact sets, version agreement and publication provenance remain mandatory. Routine CI does not claim the optional qualification campaigns or manual multi-project/multi-editor acceptance ran.

## Native build reuse and Darwin failure diagnostics

**Unreleased CI follow-up, November 1, 2026.** Native compilation/regressions and runtime validation are separate jobs on the same selected host matrix. `Native build / <target>` runs the existing selected native checks, builds the versioned release runtime and seals a complete allowlisted payload. `Native / <target>` downloads that handoff, verifies the exact commit/tree, source identity, run, target, qualification mode, release profile and every payload hash before executing it, then runs the existing client/model, installer, document and MCP gates and packages the native/VSIX candidates. Release optimization, supported targets, assertions, deadlines and release provenance checks remain intact.

The intermediate artifact is `airk-runtime-<target>-<commit>-<run-id>` and contains only `runtime.zip` and `transfer.json`. It is an **unqualified build handoff**, not a release candidate. The nested ZIP preserves executable modes across GitHub's artifact transport. Full install inventory, source/build receipts and every versioned image hash are verified again after extraction: one sealed main for modern schema 2, both original roles only for schema-1 predecessor coverage. It contains no test state, sources, indexes or credentials. It expires after seven days.

A failed validation job can rerun against the retained build without compiling again. For a releasable candidate, retain the existing complete-attempt rule: **rerun all CI jobs** after a partial failure. The build jobs now reuse an earlier successful build in that same run after authenticating the GitHub artifact digest, run origin, its creation inside one successful build job's interval, the original build attempt and all local source/payload checks. They explicitly skip already-passed compile/regression work; validation and packaging run again. Missing/expired handoffs and uploads bound only to a failed/cancelled build rebuild normally; ambiguous, corrupt or mismatched evidence fails closed. `fresh_build` disables this reuse as well as compiler/Docker cache restores. No cross-run or changed-source reuse is introduced, and failed tests are never converted into successful observations. Existing final candidate names and per-attempt release authentication remain unchanged.

CI dev/test compilation sets `CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_PROFILE_TEST_DEBUG=0`, with a separate `debug0` native dependency-cache key. This omits debug information while preserving the test selection and symbol-based backtraces; release settings are unchanged. Build and validation jobs retain separate timing artifacts. This is a build-cost trial, not a measured Darwin speedup.

Routine validation runs one full seven-test MCP smoke and two additional passes of the two four-cold-client tests. The extra cold-client observations preserve the failure-sensitive coverage introduced for the earlier Intel failure; the current selection retains them after the later correction rather than moving every repetition to qualification. Explicit qualification runs the full installed-runtime extension suite (which supplies its first MCP pass), followed by two full MCP passes, for three in total. Every pass is independent, stops on failure and retains the original first-status and typed-retry assertions.

The motivating Intel run [41380069062, attempt 1](https://github.com/iokaio/airk/actions/runs/41380069062/job/125069025091) passed 311 platform regressions (382.1 seconds including compilation) and its release build (812.3 seconds), then failed the four cold source-client test in smoke pass three. The first two full passes passed; Apple Silicon passed its native job. The reported error was a generic direct-project connection failure, not an observed OOM, disk exhaustion or job timeout. At that failed checkpoint the cause was unresolved; later passing results do not erase that observation. In [attempt 2](https://github.com/iokaio/airk/actions/runs/41380069062/job/125080184647), both pinned and source-bound four-cold-client tests failed in the first full smoke pass with the same generic connection error; the other native hosts passed. This is recurring failure evidence, not a diagnosed runner limit.

**Later correction and hosted evidence (2–3 November 2026 UTC):** PR #56,
`c899eca6...`, reproduced a late status waiter losing the explicit shared setup
expiry cause. The correction retains typed `status_timeout`/preparation expiry
without replaying accepted or uncertain work, as recorded in the
[1.8.7 follow-up](releases/release-1.8.7.md#cold-setup-deadline-follow-up-november-2-utc).
Main CI [41398425043](https://github.com/iokaio/airk/actions/runs/41398425043)
(`0dbd2f0...`) and [41551033290](https://github.com/iokaio/airk/actions/runs/41551033290)
(`b624b31c...`) subsequently passed both Apple Silicon and Intel native build and
runtime jobs. Each runtime job passed its full seven-test MCP smoke and both
additional two-test cold-client passes, plus native/VSIX packaging. The exact
PR-tip and first main source trees agree. These are results for those recorded
sources; they do not qualify the current unpublished 1.8.12 tree or establish a
runner-capacity diagnosis. The current repetition and release gates remain.

The same recorded `0dbd2f0...` source passed both Mac signing jobs in
[GitHub release 41405017894](https://github.com/iokaio/airk/actions/runs/41405017894),
including the pair signing/notarization command and verified signed native/VSIX
outputs. [Marketplace rehearsal 41405848046](https://github.com/iokaio/airk/actions/runs/41405848046)
kept publication skipped; [41406112490](https://github.com/iokaio/airk/actions/runs/41406112490)
then completed publication. This historical 1.8.7 distribution evidence predates
the new loader inventory and does not qualify its 1.8.12 signed-promotion fix.

**Installed-editor checkpoints (7 November 2026):** the earlier failure caused
by removed saved-association/explicit-grant UI is retained as historical
failure evidence. The current `scripts/check-native-editor.cjs` and installed
driver now use **Search scope**, **Index library** and **Index sets**, reviewed
permanent source exclusion, a native index-rename stale race, index-set deletion
that fences a retained session, and exact applied-selector revalidation after
reload. The first migrated actual native-enabled `same` run failed a later
boundary: one queued job produced `SourceChangeBusy`, which the host incorrectly
classified as `unexpected_service_response`. The corrected host validates that
known pre-insert refusal, retains the full native body and human-readable
queued/running counts, and does not replay the operation. Source regressions
passed: the complete extension suite selected **943 cases**, with **922 passed / 0 failed / 21 optional skips**; **39 Node helper tests** passed; Python completed **498 tests, 489 passed / 9 expected skips / 0 failed**, in **30.249 s**. Repository checks passed for **1,844 public files**. Actual native-enabled acceptance with that host is pending.

The fixture now observes the exact index's initial work through a bounded,
read-only **90-second idle precondition** before its permanent exclusion review.
That observation is not mutation admission. Short canonical Unix profiles,
private editor processes, shell integration off and exact-owned cleanup after
qualified native Stop remain fixture boundaries; uncertain ownership/state is
retained. See [the current scenario contracts](windows-local-build.md#installed-extension-and-two-window-qualification).

No hosted workflow invokes this desktop GUI harness, including `ci.yml` with
`qualification` enabled; that selection is unchanged. `npm test`, installed
native-process/MCP/client/package gates, bootstrap/tab handoff and Restricted
Mode remain separate observations. The local Mac notarization, native lifecycle
and model checks passed on their recorded artifacts, but do not qualify the
pending native-enabled UI journey. The reviewed new old-VSIX upgrade helper has
not been executed and is not a prequalified native legacy fixture.


When both `AIRK_PROCESS_DIAGNOSTICS=1` and `AIRK_NATIVE_DIAGNOSTIC_DIR` are set for disposable fixtures, project/derived worker stderr is continuously drained into a 4 KiB tail, with dropped-byte counts, an explicit stderr-EOF observation and the observed exit status after the existing owner drains the child. An inherited diagnostic descriptor never extends native shutdown; incomplete stderr drain remains visible. The MCP frontend records a bounded underlying connection error chain on stderr without changing its public result or retry authority. Failed smoke fixtures retain bounded frontend stderr, child PID/exit/signal, the original failure and numeric memory/disk observations before cleanup. Native build/validation jobs also retain runner memory, disk and process-count snapshots; unavailable observations remain explicit. No service databases, source trees, full command lines or environment values are uploaded. These diagnostics are opt-in and may contain fictional fixture paths. They do not warm the first status call or authorize another read.

The worker exit text uses the host's native `ExitStatus` display, whose wording differs between Windows and Unix. The collector regression independently requires numeric exit code 7 and an exact match with that child's displayed status; it does not assume Windows wording on macOS/Linux. PR #56's first Apple Silicon build exposed that assertion portability bug (312 passed, one failed), before release-pair construction or runtime validation. That assertion failure is separate from the later-corrected Intel setup-deadline failure described above.

The related process-return/error-message audit covers native adapters/tests, extension process helpers/tests and CI scripts. Numeric exit codes and typed OS error kinds remain the comparison boundary; AIRK-owned display strings have their own stable contracts. OS-specific service command output stays within its platform branch. No other confirmed Windows return-wording assumption was found in that scoped audit.

## Storage concurrency and fixture capacity

The `native_read_retry`, `legacy_native`, `pdf_worker`, `unix_guardian`, `native_worker_storage` and `combined_reads` binaries contain complete owned service/process fixtures. CI assigns them a nextest group with one slot, preserving concurrency exercised inside each test and leaving unrelated library cases parallel. The one-slot group also applies to these six binaries in Linux and full qualification; only the focused Windows/macOS step has the two-process global cap. Existing timeouts, exact registration/read assertions and zero automatic retries remain in force. The focused step enables existing process/fixture phase diagnostics for failure output. All-feature coverage remains on Linux and in explicit qualification; every modern native release still builds and exercises the real in-process inference owner; explicit schema-1 fixtures retain historical helper coverage.

The October 26 Windows run at `59887cd` passed 154 focused tests but timed out the PDF fixture and failed another fixture during registration with repeated unknown registry responses. Both Macs passed that selection; all three other native hosts passed release/model/install/package gates. A local two-core/four-process reproduction passed the overlapping startup cluster, so CPU contention and the exact hosted actor stall remain unproven. The CI correction removes unnecessary debug inference payloads and bounds independent fixture concurrency as resource policy; it does not label that unknown cause as diagnosed. A local default-feature run with the new scheduling passes all 156 runnable tests in 25.5 seconds. See the [dated release record](releases/release-1.4.4.md).

Windows concurrent publication exposed `SQLITE_PROTOCOL` with the bundled SQLite 3.53.2: its VFS classified canonical extended local paths (`\\?\C:\...`) as UNC network paths and selected shared-handle WAL locking. The exact upstream SQLite 3.53.3 bundle in [Cargo.toml](../Cargo.toml) includes the [path classification fix](https://github.com/sqlite/sqlite/commit/2429af2e0ce2e4c643786f8d2533f79d974e3270). The [dependency notice](../THIRD_PARTY_NOTICES.md) records the source pin and package versions. The store's `sqlite_wal` integration test exercises concurrent read transactions and connection churn on a canonical path longer than 260 characters, without AIRK retries. The kernel's `publication_v2` test additionally checks publication, pinned metadata and search consistency.

Retention lifecycle fixtures configure a 512 MiB root and 128 MiB default collection allowance **before initialization**. SQLite precharges its maximum page allowance, so leaving a tiny fixture at the production 256 GiB default can instead fail on changing hosted-runner physical capacity. All fixture collections share the explicit policy; pressure tests then set their deliberate tighter limits. The 200-edit plateau and exact retained-object assertions remain in place. Physical admission remains enforced.

Real index/provider pressure tests live in the kernel's unit harness so successful single-retry reclamation can use a private, instance-scoped test budget (30 seconds) without exposing a production setting. Their first publication contains real data, and manual mode preserves a known obsolete backlog; no preparatory timed cleanup determines whether reclaimable payloads exist. The index test first proves that the same publication fails with typed quota pressure in manual mode and releases admission without advancing the head. Separate tests exhaust the explicit one-second test budget before retirement authorization: both index and provider publication must then fail with typed quota pressure, preserve every generation and the head, report pending maintenance, and release reservations. Explicit bounded maintenance must recover before a new request succeeds. Provider invocation counts remain exactly one per request, including the failed automatic retry. The test budget field is absent from production builds.

Recovery and idle-worker tests inspect maintenance receipts and continue explicitly reported pending work within a finite pass limit. A single bounded maintenance pass does not promise complete cleanup. Fault tests drain setup backlog and, if a pass expires before the injected stage, continue pending maintenance without publishing until that stage is reached. The recovery test deliberately exhausts the deadline after retirement authorization, reopens again, and requires complete cleanup with zero pending intents, the exact protected generations/objects, unchanged publication identity and no leaked reservations. Failed or quota-blocked maintenance is rejected instead of retried. Reader-release tests likewise require eventual removal while preserving the active and predecessor revisions.

Usage accounting has a separate best-effort contract: successful and failed jobs can both lose optional observations when nonblocking locks or the 10 ms quota-admission budget prevent recording. Scheduler integration tests run in isolated processes and require exactly one persisted observation or one visible drop for each job, correct collection attribution, exact recorded work, and no extra observation for idle or disabled workers. Held usage locks deterministically exercise loss for successful and failed jobs. Exact payload, reuse, failure, revision-baseline, feedback and privacy assertions use the existing strict store path in unit fixtures; production admission budgets and behavior are unchanged.

## Downloadable candidates

Ordinary CI and local extension packaging default to the stable channel for the checked-in version. A stable candidate is still an unpublished build artifact; release workflows verify and publish it separately. The [1.8.14 working-build record](releases/release-1.8.14.md) describes current implementation and qualification limits, while the [October 17 release summary](releases/releases-2026-10-17.md) records earlier published versions. The container smoke verifies the exact unified 29-tool MCP catalog; the unsupported history endpoint remains absent. Future previews require an explicit channel and a distinct version.

Successful native jobs upload an artifact named `airk-native-<target>-<commit>-<run-id>-<attempt>` to the workflow run's Artifacts section. It contains:

- `airk-<version>-<target>.zip`: native executable, AIRK legal files and dependency notices.
- `manifest.json`: commit, Git tree, dirty-checkout flag, compiler versions, lock/toolchain hashes, full payload digests and archive digest. PR runs build GitHub's merge ref, so their artifact names and receipts carry that merge-ref commit; releases accept such a receipt only when its tree equals the released main commit's tree.
- `SHA256SUMS`: SHA-256 digests for the ZIP and manifest.

[The packager](../scripts/package-native.py) stages only these allowed inputs. It verifies every archive entry against the inventory, preserves Unix executable permissions, and runs the archived executable's version/help commands. It refuses an existing staging destination. Neither the repository tree nor `temp/`, index state, Cargo build directories or credentials are archived. GitHub retains build and test output in the run logs; failed jobs do not upload a successful candidate.

The manifest is a build record, not a signature or a complete reproducible-build attestation. Current dependency notice gaps remain documented in [third-party notices](../THIRD_PARTY_NOTICES.md). Public release qualification, signing and installed-editor tests are separate from candidate construction. The same run also uploads `airk-vsix-<target>-<commit>-<run-id>-<attempt>` containing the platform VSIX, extension manifest and SHA256SUMS. The VSIX manifest binds the native archive, its receipt, the npm lock and all staged runtime payload hashes. The extension package includes the pinned MIT-licensed `jsonc-parser` runtime and its original license for configuration editing; build dependencies remain excluded. See [extension deployment](extension-deployment.md) and [Windows local builds](windows-local-build.md).

To reproduce a candidate locally, first provision/verify the pinned model and static ORT inputs as described in [CONTRIBUTING](../CONTRIBUTING.md), and pass the verified five-variable child environment. The following runtime/package commands require those prerequisites and the target matching your Rust host:

```console
python scripts/build-service-runtime.py --profile release --destination artifacts/runtime/x86_64-pc-windows-msvc
python scripts/package-native.py --target x86_64-pc-windows-msvc --runtime-dir artifacts/runtime/x86_64-pc-windows-msvc
```

Use Python 3.11+ locally (the native builder imports standard-library `tomllib`). Output is under ignored `artifacts/native/<target>/`. Retain or deliberately remove only that generated candidate directory before packaging again. Local uncommitted work is recorded as a dirty checkout.

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

`Required checks` aggregates dependency results even after failures. Its condition is `always() && (github.event_name == 'pull_request' || github.ref != 'refs/heads/main' || !cancelled())`: only explicitly canceled or superseded non-PR runs on `main` skip this aggregate instead of generating a second failure from interrupted work. This follows [GitHub's status-check semantics](https://docs.github.com/en/actions/reference/workflows-and-actions/expressions#status-check-functions).

PRs and non-main refs retain unconditional aggregation, including manually dispatched feature-branch runs after cancellation: [GitHub treats a skipped required job as successful for merging](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-jobs-with-conditions), so skipping a gate on the PR's head could bypass its protection. Whenever the gate runs, it still requires success for every selected job and an intentional skip for every unselected job; failed, canceled, unexpectedly skipped, missing or extra results fail closed, including when the gate is invoked directly. A skipped main aggregate does not qualify a canceled CI run or its artifacts for publication. `signed-off` remains independently required; branch protection, platform selection and release provenance checks are unchanged.

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

The 1.7.3 GitHub-release candidate promotes CI native and compiled VSIX artifacts without release-time TypeScript compilation or Rust dependency fetching. Producer authentication is consolidated at preflight and publication; platform jobs verify pinned artifact digests locally and retain source receipts for assembly. Linux packages are reused unchanged when channels match. Signing, notarization, native self-test, complete payload/source binding and uploaded-asset verification remain required. See [release promotion](release-workflow.md).

## Controls that reduce unnecessary work

- Branch pushes outside main use PR validation; newer commits cancel superseded runs for that PR. There is no schedule.
- Each native target is built and tested once per merged change: Linux, Windows and Apple Silicon in PR CI, macOS Intel on main. Main records the identical-tree PR producer and its own Intel build as one candidate (`airk-ci-producer/2`), or builds all four itself when promotion does not apply (`airk-ci-producer/1`). GitHub and Marketplace release workflows authenticate and promote those exact artifacts instead of invoking the Rust matrix again. A separate tag workflow verifies published release provenance; unverified tags fail with an instruction to validate a main candidate.
- The native matrix runs together. Compiler concurrency is four on x64 and remains two on Apple Silicon for memory headroom. Test scheduling is independently bounded. Routine Clippy remains on Linux; Windows/macOS workspace Clippy is opt-in qualification. Formatting runs once. Intel's existing 120-minute job ceiling remains until hosted measurements justify reducing it; other full jobs retain 90 minutes.
- SHA-pinned `rust-cache` keeps dependency outputs, excludes workspace crates and writes only from main push CI. PRs may restore main caches but cannot update them. Incremental compilation remains disabled; runtime/release optimization settings are unchanged.
- Docker tests precede optimized compilation. BuildKit layer caching is supplemented by explicit registry/git/target cache-mount injection and extraction. Workspace outputs are pruned before exporting dependency caches. PR builds do not write shared Docker caches. A cache miss performs the same required checks.
- Docker dependency cache keys separate the Linux x64/toolchain/Dockerfile/pruning configuration from Cargo manifests and the lockfile. If a version or dependency change misses the exact key, a fallback restores the newest cache with the same configuration prefix. Cargo still checks fingerprints and the locked graph, rebuilding changed dependencies; main saves the updated cache under the new exact key. There is no fallback across toolchain or Dockerfile changes. The first run in the `v3` namespace is cold, and `fresh_build` still disables restores and saves.
- Dispatch **CI** with `qualification` enabled for the full platform/container suites and long native campaigns; add `fresh_build` to disable native and Docker cache restores for a cold run. A successful manual CI run on the exact main commit can be supplied to GitHub release as `candidate_run_id`; Marketplace instead requires the canonical signed assembly through `signed_run_id`. Missing/expired candidates fail early with this rebuild instruction; releases never silently start another long build.
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

The separate [Extended qualification workflow](../.github/workflows/qualification.yml) is manual only. It measures a configurable original corpus on Windows/Linux and runs abrupt-publication recovery; optional inputs add the pinned public CPython corpus and transformer qualification. Its modern runtime always requires the pinned embedded model and static ORT build inputs. It uploads numeric reports only, never source trees, model assets or index state. This longer workflow is separate from required PR checks and is not automatically started by a code change. GUI/agent-host qualification remains separate from signing. Both hosted Mac
signing workflows now share a schema-2 post-sign gate: after removing temporary
signing credentials, execute the actual signed app's embedded-model smoke and
the canonical private service-install fixture, then check that its full runtime
inventory stayed unchanged. A failed probe blocks packaging/promotion. The
structured `post-sign-validation.json` is outside the sealed runtime and is
printed to the job log; this gate is not evidence of a successful hosted run
until that run actually completes. See [native signing](native-signing.md).

Debug/test builds optimize the SHA-256 dependency because integration fixtures authenticate complete native images and offline model assets. Existing digest, ownership and deadline checks remain enabled; release builds retain their existing optimization profile. macOS installer fixtures use canonical temporary roots and app-bundle-aware payload paths rather than relaxing redirect checks.

Managed MCP smoke separates cold quick status from a confirmed observation. In the 1.8.0 F06a candidate, cold setup has an original-start ten-second allowance and the established status observation has two seconds; `status_timeout` must identify its stage and disclose unknown readiness and null evidence. It is not proof that an index is unavailable. Source-bound and pinned MCP frontends retain one session-owned installation-verification attempt across request timeouts and report its real failures. F04's candidate closes setup admission at transport EOF before SDK draining, then joins the owned driver/blocking worker; an already accepted exact operation may settle. Its [phase gates](history/optimizations.md) require actual EOF during pending setup as well as ordinary lifetime tests.

F06a qualification requires exact first status on an existing healthy saved index without diagnostics or retry warm-up, independent source/pinned frontends, four-client receipt isolation, public/legacy alias parity and actual pending-request EOF/cancel drain. Unindexed unavailable cases and four waiters in one process are separate coverage. Preserve failed first observations; subsequent recovery cannot replace them. Then check registration, publication counts and cache invalidation. Explicit timeout hints and the existing typed safe-read refusals permit bounded same-scope observations within the original deadline; neither counts as readiness. Permanent errors, submitted queries and transport failures do not authorize replay. Pre-publication diagnostics still require confirmed unavailable evidence and zero inference loads/sessions. Production deadlines, payload/control/health verification and direct-query receipt safeguards remain enabled.

The opt-in CLI `runtime_install::native_connection_tests` use `AIRK_F03_RUNTIME` and `AIRK_F03_APPROVED_SHA256` for a freshly built, independently reviewed pair; the historical variable names also apply to later candidates. Set `TEMP`/`TMP` to an owned fixture directory outside source roots and `AIRK_NATIVE_EOF_DIAGNOSTIC_DIR` to a fresh external diagnostics parent. The EOF matrix creates exclusive per-scenario directories and retains child stderr and the first status reply there. Run the selected tests with `--ignored --test-threads=1`. Never supply an installed user runtime or existing user state as fixture storage. These tests preserve their exact timeout/receipt assertions and exercise unregistered source, registered source and installed pinned-project initialization, catalog/help and EOF while the installer lock is held.

### Focused status and Mac loader regressions

Run these checks while investigating cold-status timing, before another hosted run:

```text
npm run test:status --prefix packages/vscode
cargo test -p airk-mcp quick_status -- --nocapture
python -m unittest scripts.tests.test_service_runtime_build.ServiceRuntimeBuildTests.test_development_mac_helper_loads_its_adhoc_signed_library -v
```

The process tests launch small Node children to verify actual exit codes, typed refusal handling, diagnostic preservation and admission deadlines. The Rust tests cover timeout, refusal and recovery without invented evidence; `quick_status_forwards_typed_admission_refusal_after_a_deadline_without_evidence` reproduces the hosted sequence deterministically: a deadline expires, then the abandoned query's admission slot causes an unsubmitted busy refusal that the frontend forwards unchanged.

Session reads in native tests go through `requestWithReadRetry` or `observeQuickStatus`, which repeat only a complete typed hint within the original deadline and report any other failure with a bounded copy of the reply. [The contract test](../scripts/tests/test_native_read_contract.py) rejects unexplained raw native MCP calls and error assertions that omit the reply.

To reproduce hosted Windows timing, run `pwsh -File scripts/ci-local.ps1 -Cores 4 -Repeat 5` (add `-Load 2` for competing CPU work). Use `-SkipBuild -RuntimeDir <path>` to qualify a separately staged pair while preserving the default runtime directory. It restricts itself and every child process, including detached service roles, to the requested processors, then repeats the routine native smokes without retries. The loader test runs a real ad-hoc signed helper/library pair on macOS and explicitly skips other hosts; it does not qualify ONNX inference.

For real native cold-status coverage, set `AIRK_TEST_BINARY` to the main executable in an existing qualified runtime pair, then run `node --test packages/vscode/out/test/nativeSmoke.test.js`. This covers source-bound and pinned frontends, registration, cache invalidation and zero inference loads. It skips when the binary is unset. Actual semantic inference and helper shutdown remain covered separately by `python scripts/check-service-clients.py --runtime-dir <runtime-directory> --files 16 --model --report <report.json>`; a loader or status pass alone does not establish semantic readiness.

The real-model kernel step passes an absolute `AIRK_TEST_MODEL_DIR` rooted at the checked-out GitHub workspace. Cargo runs each integration target from its crate directory, so a repository-relative asset path is not a valid test input. The pinned asset verification and model/retrieval assertions remain unchanged.

For the 1.5.12 process changes, routine native CI also runs the offline installed-runtime lifecycle fixture. It verifies generated role executables and helper identity, real MCP EOF/abrupt client loss through stable and universal launchers, preservation of another live session and the shared service, and owned retirement. This focused installer gate covers the changed lifecycle on the existing four native hosts; it does not enable the broad opt-in qualification matrix.


## Isolated real-corpus measurements

`scripts/benchmark-real-corpora.py` measures the selected AIRK and BigDotNetProject repositories without modifying either source tree. Supply explicit corpus roots on every platform, an isolated release runtime from `scripts/build-service-runtime.py`, its independently recorded `airk-install.json` SHA-256, and a new report path outside the runtime and both source roots. Record the builder's source identity and command: the approved builder checks the main's dependency boundary and builds it without default features; a payload seal alone does not prove that feature selection.

```powershell
python scripts/benchmark-real-corpora.py --runtime-dir $runtime --runtime-sha256 $approved --airk D:/code/Github/airk --BigDotNetProject D:/code/Github/BigDotNetProject --report $newReport --semantic --excerpt-path README.md --samples 1
```

The one-sample default is for development smoke; request `--samples 30` explicitly for the comparison campaign. `--state-parent` defaults to the system temporary directory and must remain inside it. Runtime, control, models, indexes, logs and reports stay outside both source roots. The runner verifies the approved inventory, executable pair and compiled versions before corpus registration; it disables watchers and submits owned jobs once. Read-only Git-visible source identities are checked before and after work; a changed identity invalidates the campaign. Public reports contain aliases, opaque identities, aggregate counts, timings and fixed failure codes, never source paths, queries, excerpts or raw errors. Existing reports are not overwritten.

Modern single-main campaigns use the shared authenticated HTTPS client and exact reviewed scope; fresh-session timings measure a new TLS/MCP session, not a new frontend process or a cold filesystem. Historical schema-1 campaigns retain stdio and label their fresh-process basis separately. Managed reopen uses canonical exact-instance Stop followed by explicit Start, and uncertain Stop retains the owned fixture for inspection. Initial indexing, fresh MCP sessions, retained-index reopen and unchanged-source reconcile are separate observations. Each session records its exact first status reply before other tools; a transport/protocol failure retains timing and prior measurements and aborts without replay. Detailed diagnostics collect semantic identities only after the initial measured observations. Healthy reopen restarts the owned service once per corpus, records shutdown/start duration and the exact first frontend sample, then makes at most 32 separately counted independent availability observations before any remaining requested frontend samples. It retains failed first rows and records elapsed time from restart to each first usable probe; uncertain transport/protocol failures abort without replay. Cancellation runs after this healthy-reopen scenario, targets an unchanged reconcile and records when completion wins the race. `--semantic` and an existing common `--excerpt-path` are needed for vector/hybrid and excerpt evidence; missing usable probes remain visible.

Reports identify probe order and lexical/vector/hybrid mode and retain an opaque query SHA-256 for comparison without query text. They record OS/release, architecture, logical CPU count and Python version; storage class, antivirus activity and effective affinity remain unmeasured, and cache eviction is not performed. Registration/model-setup durations are separate from timed indexing; model setup may already publish an initial head, whose pre-index identity is deliberately unobserved to preserve first-status measurement.

`completed` and exit zero mean the requested workflow completed, not that every probe succeeded. `observation_outcome` and bounded per-phase/per-probe counters retain native errors, transport failures, empty/typed-no-content results and missing observations even if later calls succeed. Initial-index polling uses counters plus first/error/usable/last events rather than an unbounded log. First-status summaries separately count requested sessions, recorded sessions, recorded first-call observations and missing observations; percentiles include recorded call durations with failures, not exclusively received native replies.

Expiration before submission is recorded as `budget_exhausted` with `submission=not_submitted`; an uncertain timeout after sending remains a transport failure and is never replayed. Cleanup retains fixed failed-stage labels and the owned-child reap outcome. A successful forced reap does not erase an earlier graceful-cleanup failure. The initial-index loop observes status and the accepted job, probes each kind until first usable, and then waits about 200 ms between turns; initial readiness times therefore include this measurement workload. A budgeted failure leaves later cohorts unmeasured and must not be reported as successful corpus qualification.

These measurements use an owned explicitly selected service, not the installed launcher. Fresh processes do not imply cold filesystem caches. Installed-editor state, modified-source incremental work, sustained interruption and background-audit interference require separate evidence. Tests of the runner are not native or corpus qualification; record approved runtime smoke and actual campaign results in the [optimization execution ledger](history/optimizations.md).
