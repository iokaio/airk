# Versioning from local installation to publication

Runtime installation and upgrades display a VS Code progress notification labeled Installing AIRK runtime, Updating AIRK runtime or Checking AIRK installation, with the current stage: version and payload checks, staging, native activation, agent connection updates, project registration and cleanup. Progress uses actual stages without an invented percentage. On Windows and Linux, an admitted upgrade hard-stops verified preceding AIRK processes and disconnects their clients; success and failure remain explicit. VS Code owns the earlier VSIX installation UI, before AIRK can activate.

From 1.5.4, the installer records the exact activation intent before stopping the verified optional manager and terminating approved predecessor processes, including older versions and Windows clients holding the stable launcher. Captured native process identities prevent PID reuse from redirecting termination. All captured processes share a five-second exit wait; verification, manager control, copying and startup are additional. macOS retains its authenticated graceful shutdown path. See [installation](service-installation.md) for the ownership and platform limits.

The current branch version is **1.8.12**, the unpublished patch requested on 7 November 2026 for implementation and manual testing before promotion to **1.9.0**. See the [working-build record](releases/release-1.8.12.md). Publication follows manual acceptance, successful CI on the exact promoted main source and the release workflows. [Storage formats](storage-formats.md) retain their independent identities: index-v1 and service-v1. Existing directory migrations preserve publications and historical ownership contracts; a numeric version adjustment does not require reindexing. The working extension retains supported forward runtime lines 1.4 through 1.9. Downgrades and unknown lines remain blocked from automatic activation, and a supported numeric line cannot bypass a schema or capability refusal. A rollback to another minor line requires its matching extension. The shipped 1.6.0 extension omitted its own line from this policy; 1.6.1 corrected that installer defect.

Use one numeric `major.minor.patch` version for the Rust workspace, every local AIRK crate in `Cargo.lock`, `packages/vscode/package.json`, and both root package entries in `packages/vscode/package-lock.json`. Compiled main/helper, runtime metadata, native ZIP and VSIX must agree with that source version. The requested manual-test candidate uses `1.8.12`; the planned release dispatch/tag is `1.9.0` / `v1.9.0` after promotion and fresh validation. The working-version adjustment authorizes no CI dispatch or publication.

The generated offline template catalog in `crates/airk-mcp/src/explorer_templates.json` also embeds the extension version. After synchronizing manifests, compile the extension TypeScript modules and run `node packages/vscode/scripts/build-explorer.cjs --write-mcp-catalog`; review the generated diff. Normal extension compilation rejects catalog drift.

The numeric working-version adjustment introduces no registry or collection-policy
migration. Project/index, deny and split formats require independent reviewed
format admission and recovery gates; a supported application version alone cannot
make an unsupported persistent format readable. Follow the [implementation
ledger](research/implementation-v190.md) before activating later candidate features.

## Before distributing a changed candidate

Increment the patch before handing testers another changed installable build. A different filename, artifact directory, workflow run ID or manifest digest does not change the VS Code extension version. Repeatedly distributing changed `1.4.0` VSIXs caused same-version replacement of the installed extension directory on Windows, where VS Code rejected installation before AIRK could display its restart notification.

Update all four version-bearing files together before building. Do not edit executable versions, manifests, receipts or ZIP/VSIX contents after tests to make them match. Rebuild the native pair when its compiled version changes. Check agreement without network access or publication:

```console
python scripts/release-version.py check --expected 1.8.12
```

CI runs the same agreement check in its unconditional quick job. It rejects mismatched manifests/lockfiles and, when supplied, a mismatched `expected_version`. Local build, native packaging and extension packaging verify compiled identities and bind the exact payload/lock hashes. New changed installable candidates need the next unused numeric patch; the checked version is not bumped automatically on each CI rerun.

## Installation and running clients

A distinct extension version installs into a distinct VS Code extension directory. The extension installs an approved native pair outside that directory, under a digest-specific runtime slot. The stable `airk-launcher` path selects the approved active pair. Never point agent configuration directly at an extension-bundled executable or a particular runtime slot.

Native activation fences and drains owned service processes, verifies the replacement before committing selection, and preserves the previous selection on failed activation. Retirement follows successful activation. Admitted Windows/Linux activation disconnects verified old AIRK clients; unknown or unobserved locked executables can still require explicit cleanup; the installer must report incomplete cleanup instead of treating a process name as ownership proof. Project data, saved bindings and watching choices are preserved.

Recognized existing workspace Codex, Claude Code and Copilot connection fields are refreshed to the stable launcher and registered binding. Existing clients still need reconnection. Successful activation completes quietly. Once active, the 1.8.12 loader handles later detected installed-versus-running extension replacements automatically, disposing the old AIRK instance and reopening its single Explorer panel if open. An older loaded extension needs VS Code to activate the new loader first; use the **Restart Extensions** action in VS Code’s Extensions pane for that one-time transition. Runtime installation remains fenced while a mismatch is pending. Explorer dismissal preferences hide notices without changing those gates. A connection or cleanup problem offers one Review details action; per-file migration errors remain in the output. Copilot's AIRK server can be restarted in MCP controls. External agent sessions need their own restart. An editor reload is different from fully exiting VS Code when its extension manager rejects a same-version reinstall; AIRK cannot show a pre-install notification when its code has not run.

Installer acceptance must include both a clean installation and an upgrade over the previous version while its editor and MCP clients are running. Use the isolated harness, retaining logs, process/runtime observations, notification screenshots and failures:

```console
node scripts/check-agent-install.cjs <Code.exe> <new-vsix> <previous-vsix>
```

The optional previous VSIX selects the running-upgrade scenario. Without it, the harness tests a clean install. Neither scenario modifies the normal editor profile or publishes artifacts. A passing local Windows check does not qualify other platforms or the entire release suite.

## CI and release handoff

Finish and manually accept the identified 1.8.12 build first. Then synchronize the
source and catalogs to 1.9.0 and rebuild under that identity before the steps below.
Earlier 1.9.0 development pairs and newly tested 1.8.8 pairs retain their own exact
versions and hashes; neither is relabeled as the final promoted artifact.

1. Before a feature/fix PR, the maintainer reviews the patch increment, synchronized locks, README, documentation index, current release notes and Marketplace listing. A bump already made for that PR is not repeated. Keep dated earlier receipts and release records identifiable as history.
2. Merge the reviewed, human DCO-signed changes through the protected PR flow. Run full CI on the exact final main commit before publication. Skipping full tests for local installer iteration does not waive CI release gates.
3. Promote the complete platform candidate set from one authenticated successful main CI run/attempt/commit. Rebuilds of changed source need a new candidate/version; successful artifact retries retain their original identity. Dirty local installer artifacts are not publication inputs.
4. Rehearse Marketplace and GitHub release using the checked-in version. Set the Extension release workflow's `version` to `1.9.0` for this branch, and select the intended channel. Rehearsal never increments or publishes. Verify all compiled versions, platform payloads, lock hashes, signing/provenance and qualification limits.
5. After explicit publication authorization, publish the verified artifacts through the [release workflow](release-workflow.md). GitHub release uses `v1.9.0`; Marketplace uses `1.9.0`. Publication must not silently allocate or rewrite a version after testing. Complete partial publication with the original matching artifacts and retry only the failed jobs. Never overwrite an existing version with different bytes.

The release-time draft version PR is a fallback when both destinations already contain a completed version. It does not replace the maintainer's reviewed version choice or permit publishing before the promoted source and main CI are ready. Keep 1.8.12 as the requested working version until manual acceptance; the later planned promotion to 1.9.0 is a separate source/build step, not an automatic patch increase per phase. See [CI and builds](ci-and-builds.md), [extension deployment](extension-deployment.md), and [Windows builds](windows-local-build.md).
