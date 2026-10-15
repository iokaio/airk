# Building AIRK for macOS

Build the native Rust executable on a Mac or macOS CI runner. Docker is optional and is not needed for this workflow. Local Apple Silicon build/tests and signing/notarization were observed on 14 October 2026; see the [validation record](validation.md) for exact scope and remaining qualification.

## Prerequisites

- Git and a checkout of this repository.
- Rust installed through rustup. The repository pins Rust 1.98.0, rustfmt and clippy in [rust-toolchain.toml](../rust-toolchain.toml).
- Xcode Command Line Tools, including Clang and the macOS SDK/linker. Native Tree-sitter grammars need a C/C++ toolchain during compilation.
- Internet access for initial compiler/dependency downloads, or a prepared cache.

Install Apple's build tools and complete the installation dialog:

```sh
xcode-select --install
```

Install rustup using the [official Rust installation instructions](https://doc.rust-lang.org/book/ch01-01-installation.html). Then open a new terminal so its Cargo tools are on PATH. From the AIRK checkout:

```sh
rustup show
rustc --version
cargo --version
xcrun --find clang
xcrun --sdk macosx --show-sdk-path
```

Running rustup/Cargo inside the checkout selects the repository's pinned toolchain. Confirm that rustc reports 1.98.0 before collecting build evidence.

## Build and test the host architecture

Run from the repository root:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p airk-cli
./target/release/airk --version
./target/release/airk --help
```

The executable is `target/release/airk`. Cargo builds for the active Rust host architecture. A successful compile is only the first check; retain test results and any platform-specific failures.

The built executable needs neither Rust nor Python installed to run. Python 3.9+ is needed only for dependency notice generation, and PowerShell 7 only for the repository hygiene script:

```sh
pwsh -NoProfile -File scripts/check-repo.ps1
python3 scripts/license-notices.py --output target/notices
git diff --check
```

Inspect notice-generation warnings before publishing. The current dependency attribution gaps are documented in [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md).

## Smoke-test indexing and native MCP setup

Use the original public fixture and a separate derived-state directory:

```sh
./target/release/airk --root examples/workspaces/project-a --data-dir target/macos-smoke-state --collection macos-smoke setup --client vscode
./target/release/airk --root examples/workspaces/project-a --data-dir target/macos-smoke-state --collection macos-smoke symbol authenticate
./target/release/airk --root examples/workspaces/project-a --data-dir target/macos-smoke-state --collection macos-smoke status
./target/release/airk --data-dir target/macos-smoke-state collections list
```

Setup prints a configuration fragment; it does not overwrite editor settings. Merge its entry into the selected client's configuration, then verify that the agent connects and can call `airk_status`, `airk_symbols` and `airk_search`. Use `--client codex`, `claude` or another supported renderer as appropriate; [agent integration](agent-integration.md) explains the formats and client steps.

The Cargo test suite also exercises real stdio initialization and all sixteen tools through the native binary. Confirm extraction fixtures, collection isolation, source-preserving removal and Unix symlink tests pass on the Mac.

When finished, disconnect the fixture's MCP worker and remove only its derived collection:

```sh
./target/release/airk --data-dir target/macos-smoke-state collections remove macos-smoke
```

## Apple Silicon and Intel builds

Provide separate release binaries for both targets:

| Hardware | Rust target | Explicit-target executable |
|---|---|---|
| Apple Silicon | aarch64-apple-darwin | target/aarch64-apple-darwin/release/airk |
| Intel Mac | x86_64-apple-darwin | target/x86_64-apple-darwin/release/airk |

On a Mac with the Apple SDK/toolchain available:

```sh
rustup target add --toolchain 1.98.0 aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --locked -p airk-cli --target aarch64-apple-darwin
cargo build --release --locked -p airk-cli --target x86_64-apple-darwin
```

Cross-building also compiles native grammar code and requires a matching Clang/SDK configuration. Adding a Rust target on Windows or Linux alone does not supply the Apple linker and SDK. Prefer macOS runners, and test each binary on its intended architecture. [Rust's macOS target documentation](https://doc.rust-lang.org/rustc/platform-support/apple-darwin.html) describes the targets, SDK discovery and cross-compilation requirements.

AIRK's minimum supported macOS versions have not been selected. Before release, choose and record the deployment baseline for each architecture, set `MACOSX_DEPLOYMENT_TARGET` consistently during builds, and test the oldest macOS version claimed. Compiler support alone does not establish AIRK's support range.

## CI candidates and release work still required

The current [CI workflow](../.github/workflows/ci.yml) explicitly builds and tests Apple Silicon on `macos-15` and Intel on `macos-15-intel`, then uploads verified native ZIPs and matching platform VSIX packages with notices, manifests and SHA-256 checksums. These are unsigned candidates, retained for seven days. No Apple credentials are required. See [CI artifacts and spending controls](ci-and-builds.md). Check the candidate's actual CI run for both architecture results; hosted checks do not qualify signing, installed services or the full editor/client matrix.

For macOS delivery:

1. Collect successful hosted results and native candidates for both architectures.
2. Test actual MCP client links, Unicode/case-sensitive paths, file locking, SQLite WAL transactions/migration, collection isolation and state on a user-selected drive.
3. Review the generated candidate's dependency notices and checksums before releasing. Users should not need a compiler or Docker to install a release.
4. For signed distribution, use AIRK's separate keychain and `airk-notary` profile with the existing company Developer ID identity, or the manual hosted workflow described in [native signing/notarization](native-signing.md). The public certificate alone cannot sign; the matching private key is required. Signing creates a fresh artifact copy and does not publish it. See [Apple's Developer ID guidance](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/).
5. Review the generated Mac VSIX and verify clean installation on its intended architecture. CI bundles the matching helper in each platform package; local packaging needs Node/npm for the pinned extension build tools. See [extension deployment](extension-deployment.md).

DiskANN uses AIRK's portable disk provider; local embeddings support explicitly provisioned static word vectors and CPU ONNX transformer inference. Their disk-I/O, locking, memory-mapping, CPU and model behavior must still be qualified on both Mac architectures. Windows tests do not establish that compatibility or representative semantic quality; see [local models](local-models.md), [ONNX qualification](onnx-models.md) and the [release checklist](release-checklist-0.1.0.md).

## Troubleshooting

- Missing Clang/linker/SDK: finish the Command Line Tools installation and check the xcrun commands above. If a custom Xcode selection is in use, verify its SDK rather than changing system-wide configuration blindly.
- Wrong architecture: compare `rustc -vV` and `file` output for the executable; build the intended explicit target and test on that architecture.
- Unsupported Rust version: work from this checkout, inspect its toolchain file and use rustup to install the pinned compiler/components.
- A failed parser, persistence or MCP test: retain the failure and fix it before describing macOS as qualified. Do not disable tests to obtain a green build.
