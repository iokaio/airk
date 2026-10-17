# Extension deployment: adapting VCP's approach

AIRK's native-helper VS Code extension and native/VSIX candidate packaging are implemented. Publication and full installed lifecycle qualification remain separate work. [Extension behavior](vscode-extension.md) defines runtime and storage controls; [Windows local build/review](windows-local-build.md) provides the PowerShell entry point.

Reviewed inspiration: VCP's `docs/development/editor-packaging.md`, `docs/development/p8-distribution.md`, `docs/development/marketplace-publication.md`, and its `beta-candidate.yml` / `publish-beta.yml` workflows at revision `2d252594cbf082f9011e2f9db3cd75d0d867f19b`. These inform the delivery design; AIRK does not copy VCP's scripts, extension code or dependencies.

## Practices carried forward

Use a pinned compiler and locked dependency graph. Build a native candidate before constructing an editor package. Stage an explicit payload, verify archive contents and retain version/hash/build records. Keep test evidence tied to the exact commit and artifacts. Publishing remains a separate operation with its own permissions and installed-product checks. The native portion now uses [AIRK's packager](../scripts/package-native.py) and [four-platform hosted CI](ci-and-builds.md).

VCP's Windows deployment uses a separately installed engine, machine-scoped paths and a Windows VSIX. AIRK's preferred optional extension bundles its native helper in platform-specific VSIX packages so users do not install a compiler or Docker. Use `win32-x64`, `linux-x64`, `darwin-arm64` and `darwin-x64` for the current native matrix. Add ARM64 Windows/Linux only alongside native test coverage. Remote workspaces require the helper on the workspace host; VCP's local Windows-only qualification cannot establish AIRK remote support.

## Implemented VSIX construction

The current native/helper/extension version is **1.1.0**; 0.1.0 was the initial beta. The visible extension name is **AIRK — Artifact Indexing and Retrieval Kernel, the Local MCP Power Tool for Coding Agents**, its identity is `iokaio.airk`, and its description has no beta label. VSCE sets the actual pre-release flag in the VSIX; a version below 1.0 by itself does not establish that channel. Keep a purely numeric version, because VS Code does not support a `-beta` suffix in its extension version field. [Official pre-release rules](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#pre-release-extensions) describe the flag and separate release/pre-release versions.

| Platform | Current VSIX filename |
|---|---|
| Windows x64 | airk-1.1.0-win32-x64.vsix |
| Linux x64 | airk-1.1.0-linux-x64.vsix |
| macOS Apple Silicon | airk-1.1.0-darwin-arm64.vsix |
| macOS Intel | airk-1.1.0-darwin-x64.vsix |

The planned 1.1.0 release uses the stable channel. Stable patch updates use `1.1.1`, `1.1.2` and subsequent `1.1.x` versions; future previews must use a separate, deliberately selected version line. A regular release must use a distinct version; the same Marketplace version cannot be both release and pre-release. Platform packages share the version and extension identity, with their target and bundled native helper distinguished in the artifact. No Marketplace listing or publication is created by the local build.

The extension has its own package, checked-in npm lock and pinned Node/TypeScript/VSCE versions. CI restores with `npm ci --ignore-scripts` and compiles on the same native runner as its helper. [The extension packager](../scripts/package-extension.py) builds a fresh allowlisted stage containing compiled runtime modules, manifest, required media/docs, the verified helper and legal notices. Exclude tests, development dependencies, source maps, indexes, fixtures, reference files and arbitrary checkout contents.

The pinned official VSCE tool packages the stage with its explicit platform target. Archive verification checks all payload entries, platform/version/extension identity and Unix executable bits for Mac/Linux packages. Those packages are built on Unix. The extension manifest records native/VSIX digests, tool versions, source commit and npm lock hash, tying each VSIX to the verified native candidate. The extension/helper versions must agree. Microsoft describes [platform packaging and publication](https://code.visualstudio.com/api/working-with-extensions/publishing-extension).

User-selected storage and any executable override must be machine-scoped and trusted; workspace-controlled paths must not silently replace the helper. Extension updates preserve collections and user-selected storage. Removing the extension must not remove source workspaces or indexed collections automatically.

## Qualification and publication

Install the exact VSIX in a clean editor profile outside the checkout. Verify native startup, actual MCP links, reload/restart, version mismatch, unavailable helper, upgrade/rollback, storage selection and collection isolation. Retain failures and unexecuted checks explicitly. Passing kernel tests does not establish installed-extension behavior.

The [manual release workflow](release-workflow.md) authenticates a successful main CI candidate, repackages the tested native helpers into the chosen VSIX channel, and revalidates those exact files before optional publication using VSCE's Microsoft Entra authentication after GitHub OIDC login. Publication is disabled by default. Promotion requires an explicit dispatch on protected main; publishing also requires Entra federation and Marketplace publisher Contributor membership. A separate identity-verification mode reports the Marketplace profile ID without building or publishing. Neither candidate construction nor ordinary PR builds receive publishing credentials.

Signing, notarization and publication prerequisites are listed separately in [CI configuration](ci-and-builds.md#optional-distribution-credentials). Ordinary native/VSIX builds need none of those credentials. Ordinary CI does not publish; only the explicitly dispatched extension-release publishing job can publish to Marketplace. GitHub Release and container-registry publication remain separate actions.
