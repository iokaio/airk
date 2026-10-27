# Contributing to AIRK

The narrowly pinned [1.4.4 release-tool repair](docs/release-workflow.md#pinned-144-release-tool-repair) is the only exception to rebuilding native candidates on main. Complete diff, version, ancestry and runtime-identity checks must prove that only the permitted release tooling/tests/docs changed. Its PR/main gates still run quick checks and all four host contract suites; source or extension changes restore normal CI. It applied only to the pinned 1.4.4 source and cannot apply to 1.5.0 or later sources; removing its tooling is a separate change.

Inference-helper builds embed the pinned FP32 MiniLM assets; the default main CLI excludes inference/model weights. Before Cargo checks/builds, run `python scripts/provision-local-model.py --destination target/local-vector-model --download`, or supply verified offline assets there (or through `AIRK_BUNDLED_MODEL_DIR`). Cargo never downloads model assets. See [model provisioning](docs/onnx-models.md).

Contributions to code, extraction fixtures, retrieval quality and documentation are welcome. Read the [architecture](docs/architecture.md) and [roadmap](docs/roadmap.md) before choosing a change. Small focused pull requests are easiest to review; discuss changes to collection identity, index formats, MCP contracts and default dependencies in an issue first.

## Development

Install Git, Rust as pinned in [rust-toolchain.toml](rust-toolchain.toml), native C/C++ build tools for Tree-sitter grammars, and PowerShell 7 for repository checks. On Windows use the Visual Studio C++ build tools; macOS uses Xcode command line tools; Linux needs a C/C++ toolchain. Python 3.11+ is needed for the complete packaging/release contract suite and third-party license bundles, not running AIRK.

For the native Mac workflow and architecture-specific release steps, see [the macOS build guide](docs/macos-build.md). For the app and extension together, see [Windows build/review](docs/windows-local-build.md). Extension development also needs Node.js 24.15.0/npm; use `npm ci --ignore-scripts --no-audit --no-fund --prefix packages/vscode` and `npm test --prefix packages/vscode`. `npm run compile` compiles the host modules with `tsc`, typechecks the AIRK Explorer webview (`tsconfig.webview.json`) and the Explorer Assistance MCP server (`tsconfig.assistance.json`), and bundles them with the pinned esbuild into flat `out/explorerWebview.js`/`.css` and `out/explorerAssistance.js` files. The Assistance bundle may load Node built-ins only. Strategies prompt and plan output is checked against golden files in `packages/vscode/src/test/fixtures/explorer/`; after an intended output change, run the test once with `AIRK_UPDATE_GOLDEN=1` and review the diff before committing. esbuild works without install scripts through its locked optional platform packages. New host modules must be added to the `MODULES` list in `scripts/package-extension.py`, and new bundles to `BUNDLES`.

```console
python scripts/build-service-runtime.py --destination target/service-runtime --profile debug
cargo build --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
python scripts/check-service-clients.py --runtime-dir target/service-runtime --files 16 --model
pwsh -NoProfile -File scripts/check-repo.ps1
python scripts/license-notices.py --output target/notices
python -m unittest discover -s scripts/tests -v
git diff --check
```

Routine Linux CI uses pinned cargo-nextest 0.9.148 with the checked-in `ci` profile and all features, followed by serial `cargo test --doc --workspace --locked`; local `cargo test` remains supported. Windows and macOS run focused process, CLI library, installer and retrieval regressions with the lightweight default feature set and two test processes. The four native integration binaries share a one-slot nextest group; their internal concurrency and deadlines remain unchanged, while library cases can run in parallel. Existing process diagnostics are retained on failure. Full workspace Clippy/tests/doctests on those hosts are opt-in qualification. Feature-enabled tests exercise the helper; the runtime builder separately verifies the lightweight main and inference payload. Kernel inference/cache performance regressions now live in `crates/airk-kernel/tests/vector_performance.rs`: run that target with `--features local-inference -- --ignored --test-threads=1` and `AIRK_TEST_MODEL_DIR` set to the absolute directory of verified offline assets; Cargo starts integration tests from the crate directory. They are component measurements, separate from the native client and helper qualification scripts. CI builds/tests native binaries on Windows, macOS and Linux and checks the container recipe on Linux. Report what actually ran, including unavailable checks. The local Windows result alone does not qualify all platforms. See [CI and downloadable native candidates](docs/ci-and-builds.md) for the explicit architecture matrix, build credentials and spending controls.

Routine CI excludes four long workload/measurement tests through the nextest profile's default filter. An explicit **CI** dispatch with `qualification` enabled restores the full workspace on every native host, container workspace tests, the full installed-runtime extension lifecycle/multi-project suite, and extended campaigns. Routine container builds run image smoke without duplicating the Linux workspace suite; use `--build-arg AIRK_QUALIFICATION=true` for local container qualification. To run the complete non-ignored suite locally with nextest, add `--ignore-default-filter`; ordinary `cargo test` remains unfiltered. All native release builds, managed-install/MCP smoke and package verification remain routine gates. Record manual multi-project/multi-editor release checks separately from automated qualification.

With Docker available, use `docker build --target test -t airk:local-test .` for the Linux test stage, then `docker build -t airk:dev .` and `python scripts/check-container.py --image airk:dev` for an isolated one-container smoke test. It creates/removes only its uniquely named test container and volume.

## Changes and fixtures

- Keep Rust source under `crates/`, extension source under `packages/vscode/`, and add an SPDX Apache-2.0 header to original Rust, TypeScript/JavaScript, TOML, Python, PowerShell and YAML files. Preserve third-party licenses.
- Add meaningful tests for parsing, provenance, budgets, collection isolation, persistence or failure behavior. Use original tiny fictional fixtures.
- Update support tables and documentation with behavior. Grammar loading alone does not establish complete language coverage; document partial extraction.
- Keep `Cargo.lock` tracked. New dependencies need license/provenance review and a reason that fits the local retrieval scope. Do not add remote inference, external database requirements or automatic provider fallbacks.
- Do not copy source from Munarium, VCP or `temp/`. Future reuse requires a separate reviewed decision and attribution. Do not include private corpora, state snapshots, credentials or local reference files in submissions.
- Keep `AGENTS.md` and `CLAUDE.md` byte-identical when editing contributor guidance.

## Candidate and release versions

Follow [versioning](docs/versioning.md) before distributing a changed installable candidate or preparing a feature/fix PR. This branch is synchronized at **1.5.2** across the Rust workspace/local lock entries, extension manifest and npm lock entries. Verify with `python scripts/release-version.py check --expected 1.5.2`. Do not repeat this bump for the same change or relabel tested binaries after qualification. Installer-only local testing does not waive full main CI before publishing.

## Submitting

Explain the concrete behavior change, relevant tests and limitations in the PR. Original contributions use Apache-2.0. Sign your commits using the [Developer Certificate of Origin](https://developercertificate.org/):

```console
git commit -s
```

The sign-off belongs to the authorized human contributor. Do not invent author identities or add agent signatures. Disclose borrowed material and review all submitted code. Community participation follows the [code of conduct](CODE_OF_CONDUCT.md); sensitive reports follow [SECURITY.md](SECURITY.md).
