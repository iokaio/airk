# Contributing to AIRK

## Clone to checks

From a clone with the prerequisites below installed, run `pwsh -File scripts/bootstrap.ps1 --download` on Windows or `sh scripts/bootstrap.sh --download` on Linux/macOS.
Omit `--download` to verify supplied offline inputs and use only cached Cargo/npm dependencies; `--check-only` checks installed tools without provisioning or creating logs.
`--no-tests` provisions/verifies and builds; `--fast` skips Clippy and does not satisfy full phase acceptance.
The wrapper reuses pinned provisioning/build/test owners, changes no global tools/PATH and starts no installed AIRK service.
Per-run logs, actual suite summaries and a JSON receipt are under ignored `artifacts/bootstrap/`; on failure start with its last step log and the actionable prerequisite message.
Fresh-machine duration and test counts depend on platform/cache/source and are **not yet qualified**; historical phase counts are in the [ledger](docs/research/implementation-v190.md).
Skipped tests and separately gated fixtures remain visible in the [ignore inventory](docs/testing/ignore-triage.md); a completed wrapper run is not installed-editor or cross-platform qualification.

Bootstrap also requires cargo-audit 0.22.2, cargo-deny 0.20.2 and cargo-vet 0.10.3.
Prepare these tools explicitly using the [dependency-check setup](docs/dependency-advisories.md#contributor-setup-and-offline-checks).
The wrapper checks their exact versions and runs the canonical dependency gate before provisioning or building project dependencies.
Offline results disclose that npm advisories and fresh/yanked Rust coverage were not checked; they cannot qualify a fresh release audit.

Main normally promotes the merged PR's tested Linux, Windows and Apple Silicon candidates for an identical source tree and builds macOS Intel; see [CI and builds](docs/ci-and-builds.md#checks-selected-by-the-change). The narrowly pinned [1.4.4 release-tool repair](docs/release-workflow.md#pinned-144-release-tool-repair) is the only other exception to rebuilding native candidates on main. Complete diff, version, ancestry and runtime-identity checks must prove that only the permitted release tooling/tests/docs changed. Its PR/main gates still run quick checks and all four host contract suites; source or extension changes restore normal CI. It applied only to the pinned 1.4.4 source and cannot apply to 1.5.0 or later sources; removing its tooling is a separate change.

**Current installation policy:** installations through 1.9.0 inclusive fully
uninstall the owned predecessor and start with an empty catalog. Installing the
same build also requires a full uninstall first. Ordinary editor reopening
reconnects; it is not another installation. No saved index, job or watch policy
starts indexing after install, and Reconnect does not create an index. Original
sources stay untouched. Qualify shutdown, durable interrupted recovery,
owned-artifact removal and the empty successor before claiming installed
acceptance. The `airk-state/1` persisted baseline remains 1.8.16; the installer
policy does not change that schema. Keep native, extension and lockfile versions
synchronized for each new candidate. See the
[contract](docs/upgrade-rebuild-fallback.md),
[manual-test correction ledger](docs/simplify/one-process-ledger.md#manual-test-installer-correction-10-november-2026) and
[release-readiness ledger](docs/simplify/release-readiness.md).

**Candidate source prerequisites:** the distributed runtime builds only `airk` with `local-inference`, embedding the two pinned INT8 MiniLM exports/tokenizer and statically linked CPU ORT 1.22.1. The no-feature main/library target remains a lightweight component-check configuration, not the modern distributable. The `embedding-worker` feature and `airk-embed` target have been removed from this tree; schema-1 pair fixtures remain explicit pre-baseline reset/component coverage; no helper, launcher image or dynamic ORT library belongs in a modern schema-2 payload. Workbench's separate fixed intent scorer still has no ONNX dependency.

Before Cargo inference checks/builds, run `python scripts/provision-local-model.py --destination target/local-vector-model --download`, or transfer the exact verified offline inputs (`AIRK_BUNDLED_MODEL_DIR` is supported). Provision static ORT with `python scripts/provision-static-onnx-runtime.py --target <native-target> --destination target/static-onnx-runtime/<native-target> --download --download-tools --build --parallel 2`. Supply only its five verified child-environment values: `LIBONNXRUNTIME_NO_PKG_CONFIG`, `ORT_SKIP_DOWNLOAD`, `ORT_PREFER_DYNAMIC_LINK`, `ORT_LIB_LOCATION` and `ORT_LIB_PROFILE`. `--environment-file <path>` writes the verified result for a local runner; `--github-env` is restricted to the real GitHub runner command file. The runtime builder revalidates the archive/build receipt and every value. The old `provision-onnx-runtime.py` dynamic NuGet package cannot satisfy modern static linkage. These are explicit build-time downloads, never Cargo model/network substitution. See [model provisioning](docs/onnx-models.md) and [current build gates](docs/ci-and-builds.md). Source implementation is unpublished; actual native model/import and final package qualification remain required.

Contributions to code, extraction fixtures, retrieval quality and documentation are welcome. Read the [architecture](docs/architecture.md) and [roadmap](docs/roadmap.md) before choosing a change. Small focused pull requests are easiest to review; discuss changes to collection identity, index formats, MCP contracts and default dependencies in an issue first.

## Development

Install Git, Rust as pinned in [rust-toolchain.toml](rust-toolchain.toml), native C/C++ build tools for Tree-sitter grammars, and PowerShell 7 for repository checks. On Windows use the Visual Studio C++ build tools; macOS uses Xcode command line tools; Linux needs a C/C++ toolchain. Python 3.11+ is needed for the complete packaging/release contract suite and third-party license bundles, not running AIRK.

For the native Mac workflow and architecture-specific release steps, see [the macOS build guide](docs/macos-build.md). For the app and extension together, see [Windows build/review](docs/windows-local-build.md). Extension development also needs Node.js 24.15.0/npm; use `npm ci --ignore-scripts --no-audit --no-fund --prefix packages/vscode` and `npm test --prefix packages/vscode`. `npm run compile` compiles the host modules with `tsc`, typechecks the AIRK Explorer webview (`tsconfig.webview.json`) and the Explorer Assistance MCP server (`tsconfig.assistance.json`), and bundles them with the pinned esbuild into flat `out/explorerWebview.js`/`.css` and `out/explorerAssistance.js` files. The optional Explorer adapter bundle may load Node built-ins only. The native unified MCP catalog snapshots its strict UI schemas in crates/airk-mcp/src/unified_explorer.json; the offline template catalog in `crates/airk-mcp/src/explorer_templates.json` also embeds the extension version. After an intentional adapter schema or package version change, compile the TypeScript modules first, then regenerate with `node packages/vscode/scripts/build-explorer.cjs --write-mcp-catalog` and review the diff. Normal compilation rejects drift. Strategies prompt and plan output is checked against golden files in `packages/vscode/src/test/fixtures/explorer/`; after an intended output change, run the test once with `AIRK_UPDATE_GOLDEN=1` and review the diff before committing. esbuild works without install scripts through its locked optional platform packages. New host modules must be added to the `MODULES` list in `scripts/package-extension.py` (paths below `out/`, including `airkctl/...`), and new bundles to `BUNDLES`. Import index modules by name (`./airkctl/index`), not by directory: the packager applies the isolated update loader's `x`, `x.js`, `x.json` resolution rule and refuses anything else.

### Local product builds (split, Windows)

The split products are built locally, before CI, by three wrappers (split plan §10.5.1, U21/U37). Each requires `-ForceBuild`, writes a fresh timestamped folder under `artifacts/` (never reusing an earlier artifact; the Cargo target and npm caches are still reused), stops at the first failing step with a non-zero exit, prints the produced file and its receipt, and never installs, starts, trusts or publishes anything:

```powershell
pwsh -NoProfile -File scripts/build-local-installers.ps1 -ForceBuild          # both, service first
pwsh -NoProfile -File scripts/build-local-service-installer.ps1 -ForceBuild   # AIRK Service installer
pwsh -NoProfile -File scripts/build-local-vsix.ps1 -ForceBuild                # universal VSIX
```

- **Service installer** (`build-local-service-installer.py`): checks Cargo, rustc, Node/npm, PowerShell 7, Git and Inno Setup (`-Iscc` overrides the default path); runs the admin bundle check (`scripts/build-admin-assets.py`: `npm ci` and `npm run check` in `packages/admin`); builds release `airk` with shipping inference and lightweight `airkctl` through `build-service-runtime.py`'s Cargo owner (with the `airkctl` dependency proof and the verified static ORT inputs described above); stages the §9.5 layout and compiles `installers/windows/airk-service.iss`. The local Trusted Signing adapter (U70) is not implemented yet, so a signed installer cannot be built; `-Unsigned` produces a labeled `...-UNSIGNED-installer.exe` packaging check that setup refuses to install. Output: `artifacts/local-service-installer/<time>/`.
- **VSIX** (`build-local-vsix.py`): checks Node/npm and Git; runs `npm ci --ignore-scripts` in `packages/vscode`, the I2 generated-type drift check (`python scripts/dev/contract_types.py i2 --check`), `npm run compile`, then `scripts/package-extension.py`. It needs no native build and no installed service. Output: `artifacts/local-vsix/<time>/`.
- Both refuse a version before the split threshold (`split_receipts.SPLIT_VERSION`, 1.8.24): increment the patch version first, as for every installer build.
- Status (12 November 2026): the wrappers' command construction, ordering, fail-fast and build-only behavior are covered by mocked Python cases (`scripts/tests/test_local_wrappers.py`), and the VSIX packager has produced and verified a real universal VSIX from the compiled extension. No full wrapper run, Cargo build, ISCC compilation, signing or local installation has been performed yet.

### Local Rust iteration

The local helper checks formatting first, then runs scoped Clippy for the affected owner with Cargo timings enabled and warnings denied:

```console
python scripts/dev-check.py -p airk-store
```

If formatting already passed for the same edits, `--skip-fmt` reuses that completed check. Scoped iteration uses `--no-default-features`; `--features` adds the explicit comma-separated features needed by the change. An explicit broad pass is `python scripts/dev-check.py --workspace --all-features`. `--target-dir` selects an existing persistent target, including the isolated P10 target; otherwise keep the same checkout's target warm between runs. Coordinate one Cargo owner among parallel agents and queue compilation, Clippy and tests. Do not precede Clippy with `cargo check` for the same feature/target configuration; the separate lightweight-main check below remains a required gate.

The helper defaults to Windows development incremental compilation on (`CARGO_INCREMENTAL=1`) and off on other hosts (`0`); use `--incremental on` or `--incremental off` for an explicit per-run override. This is a local development choice, with no change to release profiles or CI qualification. Avoid routine Windows `cargo clean` or fresh target directories. Batch required Mac Neo checks over its existing SSH connection, preserve the complete results/timing reports, and run `cargo clean` there after testing even if a check failed, to recover its limited drive space.

Structural steps ([simplification plan](docs/simplify-plan.md)) compare selected test identities and outcomes, never counts: `python scripts/dev/test_identities.py record --output <file> -- <cargo test selection>` records them per test binary, and `compare` reports added, removed and changed identities. `python scripts/dev/simplify_metrics.py` prints the plan's structural metrics; they are reported, never gates. The S5 boundary gates are enforced: `python scripts/dev/boundary_check.py` fails on an unreviewed `#[path]` mount, `include!` of Rust source or orphan file, a workspace crate edge outside the allowed list, or a module joining or merging a top-level module cycle. Its expectations live in `scripts/dev/boundary-exceptions.json`. A change there is a reviewed diff; `--write-baseline` regenerates it from the tree only for that review, keeping existing reasons. Leaving a cycle always passes, so shrink the baseline in the same change that untangles a module.

Two receipt helpers support the ledger. `python scripts/dev/receipt_runner.py --tier T0 <name> <command…>` runs one check and writes a receipt (consumed-input digests before and after, selected-test count, Cargo timings and nextest JUnit copies) to the ignored `artifacts/split/receipts/`; a test command that selects no tests fails. `python scripts/dev/cli_golden.py capture --binary <exe> --cases fixtures/cli-golden/cases.json --out <file>` records exit status, stdout and stderr for each reviewed case, with and without `AIRK_ERROR_FORMAT=json`, against owner-only fixture folders, normalizing machine paths and user names; `compare <expected> <actual> [--replace OLD=NEW]` checks a later capture against it.

Every helper invocation includes Cargo `--timings`. On the next necessary build or Clippy run, record the exact command, selected packages/features/targets, persistent target directory, incremental setting, elapsed time, result and timing-report location. Retain evidence outside the target before Neo cleanup. Use those observations to investigate cost; do not launch speculative benchmarks or broad builds just to compare workflow settings. A scoped pass proves only its selected configuration. Required workspace, lightweight component, in-process inference, applicable legacy-helper, runtime, packaging and release checks below remain in force.

### Required qualification

```console
cargo fmt --all --check
cargo check -p airk-cli --no-default-features --all-targets --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
python scripts/build-service-runtime.py --destination target/service-runtime --profile debug
cargo build --workspace --locked
cargo test --workspace --all-features --locked
python scripts/check-service-clients.py --runtime-dir target/service-runtime --files 16 --model
pwsh -NoProfile -File scripts/check-repo.ps1
python scripts/dev/boundary_check.py
python scripts/dev/d6_archive_docs.py --check
python scripts/license-notices.py --output target/notices --release
python scripts/dependency-audit.py
python -m unittest discover -s scripts/tests -v
git diff --check
```

Routine Linux CI uses pinned cargo-nextest 0.9.148 with the checked-in `ci` profile and all features, followed by serial `cargo test --doc --workspace --locked`; local `cargo test` remains supported. Windows and macOS run focused process, CLI library, installer and retrieval regressions with `--no-default-features --features local-inference` and two test processes, exercising the shipping in-process inference path. A separate scoped `--no-default-features --all-targets` CLI compile gate retains lightweight component coverage; it does not qualify the distributed runtime. The six native integration binaries share a one-slot nextest group; their internal concurrency and deadlines remain unchanged, while library cases can run in parallel. Existing process diagnostics are retained on failure. Full workspace Clippy/tests/doctests on those hosts are opt-in qualification. Feature-enabled tests cover the in-process inference owner; explicit legacy helper components retain predecessor coverage. The modern runtime builder separately verifies the sealed main, embedded model execution and static import inventory. Kernel inference/cache performance regressions now live in `crates/airk-kernel/tests/vector_performance.rs`: run that target with `--features local-inference -- --ignored --test-threads=1` and `AIRK_TEST_MODEL_DIR` set to the absolute directory of verified offline assets; Cargo starts integration tests from the crate directory. They are component measurements, separate from the native client and helper qualification scripts. CI builds/tests native binaries on Windows, macOS and Linux and checks the container recipe on Linux. Report what actually ran, including unavailable checks. The local Windows result alone does not qualify all platforms. See [CI and downloadable native candidates](docs/ci-and-builds.md) for the explicit architecture matrix, build credentials and spending controls.

PR CI builds and tests native candidates on Linux x64, Windows x64 and Apple Silicon, plus the container, extension and contract checks. macOS Intel runs after merge on main. When the main tree is identical to the merged PR's tested merge ref, main promotes that PR run's three native candidates and builds only Intel. A main push that changes dependency, toolchain, Docker, nextest or CI inputs, or that cannot authenticate the PR producer, rebuilds all four targets. Native builds and runtime validation use separate jobs with a verified same-run payload handoff. Routine managed MCP validation runs one full pass plus two extra cold-client regression passes; qualification runs three full passes. The earlier Intel failure and correction remain historical evidence; 1.8.23 hosted qualification remains pending. Failed passes are not retried. Rerun all CI jobs for a complete release attempt: successful build jobs can reuse their exact earlier payload and passed regression evidence in that run; `fresh_build` disables reuse. See [native build reuse](docs/ci-and-builds.md#native-build-reuse-and-darwin-failure-diagnostics).

The local hosted-runner reproduction defaults to four logical processors. Standard Apple Silicon runners have three cores and 7 GB RAM; see [runner specifications](https://docs.github.com/en/actions/reference/runners/github-hosted-runners). Deadline races and memory pressure on those hosts may not appear on a high-core workstation. Before a PR that touches native service, MCP, installer or smoke-test code, run `pwsh -File scripts/ci-local.ps1` on Windows. It builds a fresh source-sealed runtime (a modern main, or an explicitly selected historical pair), then repeats the managed MCP, native-client and installed-process smokes five times while every child process is restricted to four processors. `-Cores`, `-Repeat`, `-Load` and `-Suites` adjust it; `-SkipBuild` reuses `artifacts/runtime/x86_64-pc-windows-msvc`; add `-RuntimeDir <path>` to select a separately staged versioned runtime without replacing that default. Its logs stay in ignored `artifacts/ci-local/`. Native test reads must use the shared typed-retry helpers in `packages/vscode/src/test/nativeReadRetry.ts`. A raw MCP call that must run exactly once, such as a deliberate negative test, needs a `// native read:` comment that gives the reason. Every native error assertion includes the reply. `scripts/tests/test_native_read_contract.py` enforces these rules.

Routine CI excludes three current long workload/measurement tests through the nextest profile's default filter. The historical fourth, `measure_small_and_larger_sparse_catalog_edits`, retired with its uncalled prototype in accepted R2 commit `69677a29`; see the [experiment decision](docs/history/v2-incremental-catalog-experiment.md). It is not pending CLI coverage or a renamed current campaign. An explicit **CI** dispatch with `qualification` enabled restores the full workspace on every native host, container workspace tests, the full installed-runtime extension lifecycle/multi-project suite, and extended campaigns. Routine container builds run image smoke without duplicating the Linux workspace suite; use `--build-arg AIRK_QUALIFICATION=true` for local container qualification. To run the complete non-ignored suite locally with nextest, add `--ignore-default-filter`; ordinary `cargo test` remains unfiltered. All native release builds, managed-install/MCP smoke and package verification remain routine gates. Record manual multi-project/multi-editor release checks separately from automated qualification.

With Docker available, use `docker build --target test -t airk:local-test .` for the Linux test stage, then `docker build -t airk:dev .` and `python scripts/check-container.py --image airk:dev` for an isolated one-container smoke test. It creates/removes only its uniquely named test container and volume.

## Changes and fixtures

Native-owner fast-path tests require a supported write boundary. In a sandbox or shared temporary namespace with foreign write grants, set `AIRK_PRIVATE_CENSUS_BASE` to an isolated, owner-controlled test directory with inheritable owner permissions; both the store and actual-process fixtures support it. Do not change a user's existing directory ACL or accept foreign writes to obtain eligibility. Ordinary developer directories may use strict verified-read fallback. Windows read/execute sharing is compatible with the native write boundary; it does not establish per-object confidentiality. Record initial capability failures and the qualified fixture rerun separately.

On Unix, inspect the test shell's descriptor limits with `ulimit -Sn` and `ulimit -Hn` before full native qualification. The 64-channel capacity fixture needs more than the 256-descriptor soft limit inherited by some macOS SSH shells. If the existing hard limit permits it, use `ulimit -Sn 4096` in the owned test shell before starting the suite. This changes only that shell and its children; do not change global limits or reduce the fixture's concurrency or assertions. Preserve any initial limit-related failure separately from the qualified rerun.

`AIRK_TEST_MODEL_DIR` must select the verified profile for the native CPU: `minilm-l6-v2-s8-v1` on ARM64 and `minilm-l6-v2-u8-v1` on x64. The provisioner prepares both profiles; a mismatched selection remains an error. The three optional native fixtures for historical `setup-local-test.py` use the pre-1.4 standalone CLI and require separate `AIRK_LEGACY_TEST_BINARY` / `AIRK_LEGACY_TEST_MODEL_DIR` inputs. Current service pairs supplied through `AIRK_TEST_BINARY` are qualified through the managed runtime/client/installation fixtures; they cannot exercise that removed standalone command syntax. Report unavailable historical fixtures as skipped, independently of current-service results.

- Keep Rust source under `crates/`, extension source under `packages/vscode/`, and add an SPDX Apache-2.0 header to original Rust, TypeScript/JavaScript, TOML, Python, PowerShell and YAML files. Preserve third-party licenses.
- Add meaningful tests for parsing, provenance, budgets, collection isolation, persistence or failure behavior. Use original tiny fictional fixtures.
- Update support tables and documentation with behavior. Grammar loading alone does not establish complete language coverage; document partial extraction.
- Keep `Cargo.lock` tracked. New dependencies need license/provenance review and a reason that fits the local retrieval scope. Do not add remote inference, external database requirements or automatic provider fallbacks.
- Do not copy source from Munarium, VCP or `temp/`. Future reuse requires a separate reviewed decision and attribution. Do not include private corpora, state snapshots, credentials or local reference files in submissions.
- Keep `AGENTS.md` and `CLAUDE.md` byte-identical when editing contributor guidance.

## Candidate and release versions

Follow [versioning](docs/versioning.md) before distributing a changed installable candidate or preparing a feature/fix PR. The current candidate is **1.8.27** ([record](docs/releases/release-1.8.27.md)); 1.8.26 ([record](docs/releases/release-1.8.26.md)), 1.8.25 ([record](docs/releases/release-1.8.25.md)), 1.8.24, the first split build ([record](docs/releases/release-1.8.24.md)), and 1.8.23 ([record](docs/releases/release-1.8.23.md)) are dated history. Native/extension manifests and lockfiles are synchronized at 1.8.27. Verify agreement with `python scripts/release-version.py check --expected 1.8.27`. Do not repeat that bump for the same candidate or relabel earlier binaries. The [task ledger](docs/simplify/one-process-ledger.md#manual-test-installer-correction-10-november-2026) and [release-readiness ledger](docs/simplify/release-readiness.md) retain exact validation identities and limits. Candidate build, package checks, installed acceptance and platform qualification remain separate. Local/manual testing does not waive full main CI before publishing.

## Submitting

Explain the concrete behavior change, relevant tests and limitations in the PR. Original contributions use Apache-2.0. Sign your commits using the [Developer Certificate of Origin](https://developercertificate.org/):

```console
git commit -s
```

The sign-off belongs to the authorized human contributor. Do not invent author identities or add agent signatures. Disclose borrowed material and review all submitted code. Community participation follows the [code of conduct](CODE_OF_CONDUCT.md); sensitive reports follow [SECURITY.md](SECURITY.md).
