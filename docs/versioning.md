# Versioning from local installation to publication

Installation and upgrades now display a VS Code progress notification with the current stage: version and payload checks, staging, native activation, agent connection updates, workspace setup and cleanup. Progress uses actual stages without an invented percentage. A connected-client preflight pauses the upgrade with a visible Retry Update action; success and failure remain explicit. VS Code owns the earlier VSIX installation UI, before AIRK can activate.

For Windows on-demand installations, activation now checks whether the owned stable launcher can be replaced before publishing an activation intent or draining the working service. A connected client that holds the launcher blocks the update at this preflight boundary. The extension shows MCP Servers, Retry Update and Show Details, and keeps the preceding service running. After clients disconnect, Retry Update performs the fenced activation and cleanup. The native owner drain polls every 100 ms with a 45-second deadline after shutdown acknowledgment; executable verification and control/setup work are additional steps. External MCP clients are not killed by this drain. Verified OS-managed login services retain their existing owned stop/maintenance path because that manager can itself hold the launcher; that platform/lifecycle matrix remains separately qualified.

The current branch version is **1.4.5**. It is an unpublished development version; the complete SQLite-free redb cutover is required before any VSIX build. See [development status](release-1.4.5.md). The native service namespace remains `service-1.4`; a patch increment does not create a fresh project/index namespace.

Use one numeric `major.minor.patch` version for the Rust workspace, every local AIRK crate in `Cargo.lock`, `packages/vscode/package.json`, and both root package entries in `packages/vscode/package-lock.json`. The native main, inference helper, runtime metadata, native ZIP, VSIX, Marketplace version and GitHub release tag must agree. For this branch, release dispatch uses `1.4.5` and the public tag is `v1.4.5`.

## Before distributing a changed candidate

Increment the patch before handing testers another changed installable build. A different filename, artifact directory, workflow run ID or manifest digest does not change the VS Code extension version. Repeatedly distributing changed `1.4.0` VSIXs caused same-version replacement of the installed extension directory on Windows, where VS Code rejected installation before AIRK could display its restart notification.

Update all four version-bearing files together before building. Do not edit executable versions, manifests, receipts or ZIP/VSIX contents after tests to make them match. Rebuild the native pair when its compiled version changes. Check agreement without network access or publication:

```console
python scripts/release-version.py check --expected 1.4.5
```

CI runs the same agreement check in its unconditional quick job. It rejects mismatched manifests/lockfiles and, when supplied, a mismatched `expected_version`. Local build, native packaging and extension packaging verify compiled identities and bind the exact payload/lock hashes. New changed installable candidates need the next unused numeric patch; the checked version is not bumped automatically on each CI rerun.

## Installation and running clients

A distinct extension version installs into a distinct VS Code extension directory. The extension installs an approved native pair outside that directory, under a digest-specific runtime slot. The stable `airk-launcher` path selects the approved active pair. Never point agent configuration directly at an extension-bundled executable or a particular runtime slot.

Native activation fences and drains owned service processes, verifies the replacement before committing selection, and preserves the previous selection on failed activation. Retirement follows successful activation. Locked old client executables require disconnecting those clients and retrying owned cleanup; the installer must report incomplete cleanup instead of treating a process name as ownership proof. Project data, saved bindings and watching choices are preserved.

Recognized existing workspace Codex, Claude Code and Copilot connection fields are refreshed to the stable launcher and registered binding. Existing clients still need reconnection. The completion notification offers MCP Servers, Reload Window and Show Details. Copilot's AIRK server can be restarted in MCP controls. External agent sessions need their own restart. An editor reload is different from fully exiting VS Code when its extension manager rejects a same-version reinstall; AIRK cannot show a pre-install notification when its code has not run.

Installer acceptance must include both a clean installation and an upgrade over the previous version while its editor and MCP clients are running. Use the isolated harness, retaining logs, process/runtime observations, notification screenshots and failures:

```console
node scripts/check-agent-install.cjs <Code.exe> <new-vsix> <previous-vsix>
```

The optional previous VSIX selects the running-upgrade scenario. Without it, the harness tests a clean install. Neither scenario modifies the normal editor profile or publishes artifacts. A passing local Windows check does not qualify other platforms or the entire release suite.

## CI and release handoff

1. Before a feature/fix PR, the maintainer reviews the patch increment, synchronized locks, README, documentation index, current release notes and Marketplace listing. A bump already made for that PR is not repeated. Keep dated earlier receipts and release records identifiable as history.
2. Merge the reviewed, human DCO-signed changes through the protected PR flow. Run full CI on the exact final main commit before publication. Skipping full tests for local installer iteration does not waive CI release gates.
3. Promote the complete platform candidate set from one authenticated successful main CI run/attempt/commit. Rebuilds of changed source need a new candidate/version; successful artifact retries retain their original identity. Dirty local installer artifacts are not publication inputs.
4. Rehearse Marketplace and GitHub release using the checked-in version. Set the Extension release workflow's `version` to `1.4.5` for this branch, and select the intended channel. Rehearsal never increments or publishes. Verify all compiled versions, platform payloads, lock hashes, signing/provenance and qualification limits.
5. After explicit publication authorization, publish the verified artifacts through the [release workflow](release-workflow.md). GitHub release uses `v1.4.5`; Marketplace uses `1.4.5`. Publication must not silently allocate or rewrite a version after testing. Complete partial publication with the original matching artifacts and retry only the failed jobs. Never overwrite an existing version with different bytes.

The release-time draft version PR is a fallback when both destinations already contain a completed version. It does not replace the manual pre-PR increment or permit publishing before the new version's reviewed source and main CI are ready. Release workflows already honor an unused manually incremented version; do not add a second bump for this 1.4.5 change. See [CI and builds](ci-and-builds.md), [extension deployment](extension-deployment.md), and [Windows builds](windows-local-build.md).
