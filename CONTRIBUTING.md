# Contributing to AIRK

Native builds embed the pinned FP32 MiniLM assets. Before Cargo checks/builds, run `python scripts/provision-local-model.py --destination target/local-vector-model --download`, or supply verified offline assets there (or through `AIRK_BUNDLED_MODEL_DIR`). Cargo never downloads model assets. See [model provisioning](docs/onnx-models.md).

Contributions to code, extraction fixtures, retrieval quality and documentation are welcome. Read the [architecture](docs/architecture.md) and [roadmap](docs/roadmap.md) before choosing a change. Small focused pull requests are easiest to review; discuss changes to collection identity, index formats, MCP contracts and default dependencies in an issue first.

## Development

Install Git, Rust as pinned in [rust-toolchain.toml](rust-toolchain.toml), native C/C++ build tools for Tree-sitter grammars, and PowerShell 7 for repository checks. On Windows use the Visual Studio C++ build tools; macOS uses Xcode command line tools; Linux needs a C/C++ toolchain. Python 3.11+ is needed for the complete packaging/release contract suite and third-party license bundles, not running AIRK.

For the native Mac workflow and architecture-specific release steps, see [the macOS build guide](docs/macos-build.md). For the app and extension together, see [Windows build/review](docs/windows-local-build.md). Extension development also needs Node.js 24.15.0/npm; use `npm ci --ignore-scripts --no-audit --no-fund --prefix packages/vscode` and `npm test --prefix packages/vscode`.

```console
cargo build --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
pwsh -NoProfile -File scripts/check-repo.ps1
python scripts/license-notices.py --output target/notices
python -m unittest discover -s scripts/tests -v
git diff --check
```

Full CI uses pinned cargo-nextest 0.9.148 with the checked-in `ci` profile, followed by `cargo test --doc --workspace --locked`; local `cargo test` remains supported. CI builds/tests native binaries on Windows, macOS and Linux and checks the container recipe on Linux. Report what actually ran, including unavailable checks. The local Windows result alone does not qualify all platforms. See [CI and downloadable native candidates](docs/ci-and-builds.md) for the explicit architecture matrix, build credentials and spending controls.

With Docker available, use `docker build --target test -t airk:local-test .` for the Linux test stage, then `docker build -t airk:dev .` and `python scripts/check-container.py --image airk:dev` for an isolated one-container smoke test. It creates/removes only its uniquely named test container and volume.

## Changes and fixtures

- Keep Rust source under `crates/`, extension source under `packages/vscode/`, and add an SPDX Apache-2.0 header to original Rust, TypeScript/JavaScript, TOML, Python, PowerShell and YAML files. Preserve third-party licenses.
- Add meaningful tests for parsing, provenance, budgets, collection isolation, persistence or failure behavior. Use original tiny fictional fixtures.
- Update support tables and documentation with behavior. Grammar loading alone does not establish complete language coverage; document partial extraction.
- Keep `Cargo.lock` tracked. New dependencies need license/provenance review and a reason that fits the local retrieval scope. Do not add remote inference, external database requirements or automatic provider fallbacks.
- Do not copy source from Munarium, VCP or `temp/`. Future reuse requires a separate reviewed decision and attribution. Do not include private corpora, state snapshots, credentials or local reference files in submissions.
- Keep `AGENTS.md` and `CLAUDE.md` byte-identical when editing contributor guidance.

## Submitting

Explain the concrete behavior change, relevant tests and limitations in the PR. Original contributions use Apache-2.0. Sign your commits using the [Developer Certificate of Origin](https://developercertificate.org/):

```console
git commit -s
```

The sign-off belongs to the authorized human contributor. Do not invent author identities or add agent signatures. Disclose borrowed material and review all submitted code. Community participation follows the [code of conduct](CODE_OF_CONDUCT.md); sensitive reports follow [SECURITY.md](SECURITY.md).
