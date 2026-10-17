# Build and review the Windows app and VS Code extension

From the repository root in PowerShell 7:

```powershell
pwsh -NoProfile -File scripts/build-windows.ps1
```

This builds the Windows x64 release executable and a matching platform VSIX. It runs Rust formatting, clippy and workspace tests, archive tests, repository checks, TypeScript compilation, extension tests and a real native MCP-adapter test. Both archives are verified against allowlisted payload hashes. No signing or Marketplace credentials are needed, and nothing is published or deployed.

## Developer prerequisites

- Windows x64 and PowerShell 7.
- Git, rustup and the repository-pinned Rust toolchain.
- Visual Studio C++ build tools with the Windows SDK for native grammar compilation.
- Node.js 24.15.0 (the CI version) and npm; VSCE requires Node 22 or newer.
- Python 3.9+ via `py` or `python`, for notices and archive verification.
- Initial network access for locked Cargo/npm dependencies and missing Rust components.
- VS Code 1.140+ with its `code` launcher on PATH, only for editor testing/review.

The extension runtime needs none of the developer tools. Its VSIX bundles AIRK, and the editor supplies its JavaScript runtime. The executable is unsigned and the extension defaults to the stable channel; oldest-OS and installed upgrade/rollback qualification remain separate work.

## Results and repeat builds

Every run creates a fresh ignored `artifacts/local/<timestamp>-<id>/` directory:

| Path within the run | Contents |
|---|---|
| native/publish/ | Native ZIP, native manifest and SHA256SUMS |
| vscode/publish/ | Matching VSIX, extension manifest and SHA256SUMS |
| build.log | Original local build/check output |
| result.json | Native/VSIX paths and whether checks/editor tests ran |

The build-tree executable is `target/x86_64-pc-windows-msvc/release/airk.exe`. The script prints its path, the VSIX path and the build directory. It leaves prior run directories intact and never deletes index state or source files. Intermediate staging and compiler outputs are not distributed.

If you deliberately want to omit routine checks during an iterative build:

```powershell
pwsh -NoProfile -File scripts/build-windows.ps1 -SkipChecks
```

Compilation, helper version validation and native/VSIX archive verification still run. `result.json` explicitly records that checks were skipped.

## Review in an isolated editor

```powershell
pwsh -NoProfile -File scripts/build-windows.ps1 -OpenReview
```

This installs the generated VSIX in a separate extensions directory and opens a private VS Code profile with a copy of the public project-a fixture. It does not install into or alter your normal profile. Grant that fixture workspace trust, then select the AIRK status item. Try Index Now, Reindex, indexing modes, pause/resume, storage selection, collection status and Verify Native MCP.

To install the reviewed package into your normal profile later, use **Extensions: Install from VSIX** and choose the printed VSIX path. Installation is not performed in the normal profile by this script.

## Automated packaged-editor smoke

```powershell
pwsh -NoProfile -File scripts/build-windows.ps1 -RunEditorTests
```

The test loads the verified VSIX payload in an isolated VS Code extension host, using the installed Code.exe beside its code.cmd launcher. It checks contributed commands, MCP server definitions, the cached helper, incremental/full indexing, the real sixteen-tool stdio handshake, and switching index storage while retaining the original snapshot. It uses only a generated fixture and profile; workspace trust is disabled only in that test process. Other installed extensions are disabled for this smoke, and the normal user's settings/trust are untouched.

This is a packaged-extension development-host smoke, not complete installed Marketplace, remote-host, upgrade/rollback or clean-machine qualification. It does not exercise any paid coding model. VS Code startup may show a temporary test window even though the process is launched with its console hidden.

`-RunEditorTests -OpenReview` runs the smoke first, then opens the same private profile for review. The test profile has manual indexing selected; change modes through AIRK controls if you want to inspect automatic behavior.

## Installed extension and two-window qualification

After packaging a verified VSIX and compiling the extension tests, run:

```console
node scripts/check-editor-windows.cjs "<Code.exe>" "<resources/app/out/cli.js>" "<verified Windows VSIX>"
```

Use the installed editor executable and its matching CLI script (the installed `bin/code.cmd` identifies the latter, which can be under a versioned directory). This test installs the VSIX and a tiny local test-driver extension into a fresh directory under ignored `artifacts/editor-qualification/`. Ordinary editor windows use private user profiles and an explicitly private shared-application-data directory. The driver is not shipped in AIRK packages. Only generated fictional workspaces are edited.

The scenarios cover two simultaneous windows sharing one workspace/collection, two simultaneous windows using different workspaces/collections in the same storage root, a two-root workspace, and an untrusted workspace. Checks include concurrent native scheduling and MCP reads, post-edit retrieval, positive/negative collection isolation, rejected collection overrides and path traversal, the sixteen-tool handshake, registration settings off/on, and storage changes that preserve the previous indexes. The trust scenario runs with workspace trust enabled and verifies that indexing is rejected, server definitions are empty and storage remains untouched. Trusted scenarios disable the trust prompt in their private processes only.

Each window opens the real AIRK dashboard. The harness clicks its registration switch off/on and its Verify connection button through the rendered webview, then checks the real sixteen-tool verification result. Untrusted controls must remain disabled. The harness captures workbench screenshots through a loopback-only debugging endpoint, records separate extension-host process IDs and writes per-window results plus a combined summary. Citation paths and SHA-256 hashes must match the generated source files, including peer edits. Private profiles and indexes are not release payloads; two fictional-workspace [screenshots](ui/README.md#installed-vs-code-captures) are included for review. The storage-switch check changes the private profile setting; it does not automate the operating-system folder picker. External coding-agent approval screens, Marketplace installation, upgrade/rollback, remote hosts and macOS/Linux remain separate qualification.

On October 17, 2026, AIRK 1.1.1 passed all four scenarios in VS Code 1.142.0 on Windows, across six installed-editor windows. The verified VSIX SHA-256 was `4265b3f4a90a8e15a2357253204a68fd903f147a9c5448f411b98e7efbe77dee`. Both same-workspace windows reported the same collection ID; different-workspace windows reported distinct IDs and never returned the other collection's evidence. The multi-root storage change preserved the original index. The untrusted window launched no index and exposed no MCP definitions. Each window used a distinct private profile while sharing AIRK storage with its scenario peers; shared-profile settings synchronization was not tested.

Repeating the earlier package exposed intermittent concurrent-start failures: a live collection writer incorrectly blocked the scheduled request, and quota accounting could fail when a managed temporary path disappeared during enumeration. The qualified package defers live-lock contention without losing the queued revision and tolerates disappearing managed entries while preserving growth reservations. Native regression tests cover both cases. Startup screenshot timing was also corrected in the harness so an untrusted window has time to create its webview content.

## Troubleshooting

- The script fails at the first unsuccessful tool/check; retain its build.log.
- Missing C/C++ compiler: install the Visual Studio C++ build tools/Windows SDK.
- Locked dependency restoration fails: check network/registry access; do not regenerate locks or enable dependency scripts just to suppress a failure.
- Missing helper or platform mismatch: use the produced win32-x64 VSIX, not the raw TypeScript package directory. Native binaries are staged only during packaging.
- Writer lock after an interrupted native job: follow [operations and recovery](operations.md); do not delete a live writer's lock.
- Index budget reached: manage unused collections or raise the machine-scoped soft budget, then Resume. Manual indexing intentionally overrides the soft limit.

Hosted equivalents and credentials are described in [CI and builds](ci-and-builds.md).
