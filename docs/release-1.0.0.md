# AIRK 1.0.0 stable release

**AIRK — Artifact Indexing and Retrieval Kernel, the Local MCP Power Tool for Coding Agents**

This release prepares native AIRK and the `iokaio.airk` extension at **1.0.0** for the **stable** channel. The release PR does not publish packages. Publication follows a successful build of the merged main commit and the checks below.

## Release scope

AIRK provides a local workspace index with bounded, cited code and document evidence through sixteen read-only MCP tools. The extension includes its native helper, collection and storage controls, indexing schedules, coverage reporting and coding-agent setup instructions, including Google Antigravity. Source workspaces remain authoritative and read-only to retrieval.

All seven workspace crates, the extension manifest and npm root lock metadata use 1.0.0. Local VSIX packaging, ordinary CI, signing candidates and release dispatchers default to stable. Stable VSIX files omit the pre-release flag; explicit preview packaging remains available for a different future preview version. The publisher, extension ID, commands, settings, MCP tools and storage formats are unchanged by the version transition.

The release includes the updated product title and agent setup documentation, the relocation lock fix and CI improvements from the preceding 0.2.1 development commits. Release workflows authenticate and promote one successful exact-main CI candidate, then verify package identities, channels, contents and hashes before publication.

| Platform | Extension download |
|---|---|
| Windows x64 | `airk-1.0.0-win32-x64.vsix` |
| Linux x64 | `airk-1.0.0-linux-x64.vsix` |
| macOS Apple Silicon | `airk-1.0.0-darwin-arm64.vsix` |
| macOS Intel | `airk-1.0.0-darwin-x64.vsix` |

## Upgrading from 0.2.1

Keep the existing `iokaio.airk` installation and collection/storage configuration. The helper cache is version- and digest-specific; an upgrade must preserve source files and existing collections. Back up derived state before testing an upgrade if a rollback to an older helper is required. This release does not introduce a storage migration, and this preparation alone does not prove every historical rollback path.

In VS Code, choose **Switch to Release Version** on the AIRK extension to remain on stable updates. A higher stable version can update an existing preview installation, but preview opt-in can still admit future higher previews. Version 1.0.0 is reserved for the stable publication; do not publish it as a preview first. These are the [VS Code channel and version rules](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#pre-release-extensions).

## Known limits retained in 1.0.0

Stable is the distribution channel, not a claim that all qualification work is complete. Native CI covers the four platforms above, while installed-editor upgrade/rollback, oldest-OS compatibility, remote-host configurations and macOS installed-service behavior require their own evidence. Windows/Linux ARM64 and browser-only extension hosts are not packaged.

Large-repository performance, production semantic quality and practical token savings remain unqualified. Syntax relationships remain candidates unless explicitly backed by source-bound producer assertions. Local models require deliberate provisioning; no remote model service or automatic model download is added. Extraction budgets, hostile-file isolation, filesystem durability, privacy and dependency-notice limitations remain documented in [formats](formats.md), [architecture](architecture.md), [security](../SECURITY.md) and [third-party notices](../THIRD_PARTY_NOTICES.md).

## Publication checklist

Record the exact main SHA, CI/release run URLs and verified artifact receipts. Unchecked items are pending, not successful. The detailed installation, upgrade, license and qualification cases in the [historical review checklist](release-checklist-0.1.0.md) remain useful; this version change does not mark them complete.

- [ ] Review and merge `release/1-0-0`; require all PR checks and DCO to pass.
- [ ] Wait for full main CI on the merged SHA. Authenticate all eight native/VSIX artifacts from the same successful run and attempt; record that producer run before its seven-day artifact expiry.
- [ ] Rehearse **Extension release** on `main` with `version=1.0.0`, `channel=stable`, `publish=false`, `verify_identity=false`. Verify all four final VSIX files report 1.0.0, the intended title and platform, and no true `Microsoft.VisualStudio.Code.PreRelease` property.
- [ ] Install a verified candidate in a clean VS Code profile and exercise indexing, MCP connection and agent setup. Test the 0.2.1-to-1.0.0 upgrade with existing collections and settings; record platform evidence and unresolved qualification limits.
- [ ] Review the distributed notices, exact package hashes, public description and known limits. Confirm the main-only Marketplace environment and publisher identity are configured.
- [ ] Publish **Extension release** on the same main SHA with `version=1.0.0`, `channel=stable`, `publish=true`, `verify_identity=false`. If a manual CI rebuild supplied the candidate, pass its `candidate_run_id`; otherwise the workflow resolves the successful main push automatically.
- [ ] Verify Marketplace shows stable 1.0.0 and the new title for all four platforms. Install the published package and confirm the channel. Inspect any partial upload before retrying; platform publication is not atomic.
- [ ] For matching signed GitHub downloads, rehearse then publish **GitHub release** on that main SHA with `channel=stable`. Verify the numbered `v1.0.0-build.<run-number>` tag, Windows signature, both notarized Mac helpers, unsigned Linux helper, 19 release assets and matching receipts. GitHub download publication is separate from Marketplace publication.
- [ ] Verify the downloads page deployment lists the stable release first and all download URLs, checksums and build provenance resolve. Retain timing and signing evidence; validate tag provenance and publish-resume behavior when exercised.

No workflow changes the version during publication. If source changes or artifacts expire, produce a new authenticated main CI candidate before publishing. Preserve existing published versions and their provenance.
