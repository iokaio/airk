# Versioning from local installation to publication

Runtime installation and upgrades display a VS Code progress notification labeled Installing AIRK runtime, Updating AIRK runtime or Checking AIRK installation, with the current stage: version and payload checks, staging, native activation, agent connection updates, project registration and cleanup. Progress uses actual stages without an invented percentage. The 1.8.14 implementation checkpoints and drains an admitted modern service before replacement; clients reconnect to its verified HTTPS endpoint. Success and failure remain explicit. VS Code owns the earlier VSIX installation UI, before AIRK can activate. Final installed qualification is recorded separately below.

Historical 1.5.4–1.8.13 installation records the exact activation intent before stopping the verified optional manager and terminating approved predecessor processes, including older versions and Windows clients holding the stable launcher. Captured native process identities prevent PID reuse from redirecting termination. All captured processes share a five-second exit wait; verification, manager control, copying and startup are additional. Historical macOS installation uses authenticated graceful shutdown. The 1.8.14 migration retains version-specific predecessor retirement, rather than assuming those old processes implement the new checkpoint protocol. See [installation](service-installation.md) for ownership and platform limits.

The current branch source version is **1.8.14**, requested on 7 November 2026 for implementation of [One AIRK](one-airk.md). Implementation and qualification are in progress; the version adjustment alone supplies none of the planned single-executable HTTPS behavior. See the [working record](releases/release-1.8.14.md). Publication follows manual acceptance, successful CI on the exact promoted main source and the release workflows. [Storage formats](storage-formats.md) retain their independent identities: index-v1 and service-v1. Existing directory migrations preserve publications and historical ownership contracts; a numeric version adjustment does not require reindexing. The working extension retains supported forward runtime lines 1.4 through 1.9. Downgrades and unknown lines remain blocked from automatic activation, and a supported numeric line cannot bypass a schema or capability refusal. A rollback to another minor line requires its matching extension. The shipped 1.6.0 extension omitted its own line from this policy; 1.6.1 corrected that installer defect.

Use one numeric `major.minor.patch` version for the Rust workspace, every local AIRK crate in `Cargo.lock`, `packages/vscode/package.json`, and both root package entries in `packages/vscode/package-lock.json`. The compiled main, runtime metadata, native ZIP and VSIX must agree with that source version. The requested implementation version is `1.8.14`; a release dispatch/tag must use the separately reviewed, fully validated source version. The prior 1.9.0 research target does not silently promote this implementation. The working-version adjustment authorizes no CI dispatch or publication.

The generated offline template catalog in `crates/airk-mcp/src/explorer_templates.json` also embeds the extension version. After synchronizing manifests, compile the extension TypeScript modules and run `node packages/vscode/scripts/build-explorer.cjs --write-mcp-catalog`; review the generated diff. Normal extension compilation rejects catalog drift.

The numeric working-version adjustment alone introduces no registry or collection-policy
migration. The 1.8.14 implementation separately requires registry contract 13
before writing in-process provider history or metadata-only embedded model
references. Unchanged predecessor state can remain at contract 12; the installer
must verify target capabilities before Stop or rollback. New state must not be
relabelled to permit an older reader. Project/index, deny and split formats require independent reviewed
format admission and recovery gates; a supported application version alone cannot
make an unsupported persistent format readable. Follow the [implementation
ledger](research/implementation-v190.md) before activating later candidate features.

## Before distributing a changed candidate

Increment the patch before handing testers another changed installable build. A different filename, artifact directory, workflow run ID or manifest digest does not change the VS Code extension version. Repeatedly distributing changed `1.4.0` VSIXs caused same-version replacement of the installed extension directory on Windows, where VS Code rejected installation before AIRK could display its restart notification.

Update all four version-bearing files together before building. Do not edit executable versions, manifests, receipts or ZIP/VSIX contents after tests to make them match. Rebuild the native runtime when its compiled version changes. Check agreement without network access or publication:

```console
python scripts/release-version.py check --expected 1.8.14
```

CI runs the same agreement check in its unconditional quick job. It rejects mismatched manifests/lockfiles and, when supplied, a mismatched `expected_version`. Local build, native packaging and extension packaging verify compiled identities and bind the exact payload/lock hashes. New changed installable candidates need the next unused numeric patch; the checked version is not bumped automatically on each CI rerun.

## Installation and running clients

A distinct extension version installs into a distinct VS Code extension directory. The extension installs the approved 1.8.14 main outside that directory, under a digest-specific runtime slot. Native bootstrap selects that image directly. Agent configuration uses the verified loopback HTTPS endpoint and scoped credential reference; it must not select an extension-bundled executable or a particular runtime slot. Historical pairs used a stable `airk-launcher` selector and require explicit migration.

Native activation checks target storage capabilities before stopping an owner, closes admission, records an immutable checkpoint and verifies the replacement before committing selection. Failure recovery retains the exact activation journal and compatible previous selection. Retirement follows successful activation. Existing HTTPS clients reconnect; historical launcher clients require their version-specific migration. Unknown or unobserved locked executables can still require explicit cleanup; the installer must report incomplete cleanup instead of treating a process name as ownership proof. Project data, saved bindings and watching choices are preserved.

Reviewed, supported Codex, Claude Code and VS Code connection configurations migrate to the current HTTPS endpoint and protected credential references while preserving their exact authority. Unsupported client/trust combinations remain explicit refusals; serializer support alone does not qualify an installed client. Existing clients still need reconnection. Successful activation completes quietly. Once active, the 1.8.12 loader handles later detected installed-versus-running extension replacements automatically, disposing the old AIRK instance and reopening its single Explorer panel if open. An older loaded extension needs VS Code to activate the new loader first; use the **Restart Extensions** action in VS Code’s Extensions pane for that one-time transition. Runtime installation remains fenced while a mismatch is pending. Explorer dismissal preferences hide notices without changing those gates. A connection or cleanup problem offers one Review details action; per-file migration errors remain in the output. Copilot's AIRK server can be restarted in MCP controls. External agent sessions need their own restart. An editor reload is different from fully exiting VS Code when its extension manager rejects a same-version reinstall; AIRK cannot show a pre-install notification when its code has not run.

Installer acceptance must include both a clean installation and an upgrade over the previous version while its editor and MCP clients are running. Use the isolated harness, retaining logs, process/runtime observations, notification screenshots and failures:

```console
node scripts/check-agent-install.cjs <Code.exe> <new-vsix> <previous-vsix>
```

The optional previous VSIX selects the running-upgrade scenario. Without it, the harness tests a clean install. Neither scenario modifies the normal editor profile or publishes artifacts. A passing local Windows check does not qualify other platforms or the entire release suite.

## CI and release handoff

Finish implementation and manually accept the identified 1.8.14 build first.
The C0 source/catalog adjustment is not a tested runtime or release artifact.
Earlier 1.8.12, 1.8.8 and 1.9.0 development binaries retain their original versions
and hashes; none is relabeled as a new implementation or promoted artifact.

1. Before a feature/fix PR, the maintainer reviews the patch increment, synchronized locks, README, documentation index, current release notes and Marketplace listing. A bump already made for that PR is not repeated. Keep dated earlier receipts and release records identifiable as history.
2. Merge the reviewed, human DCO-signed changes through the protected PR flow. Run full CI on the exact final main commit before publication. Skipping full tests for local installer iteration does not waive CI release gates.
3. Promote the complete platform candidate set from one authenticated successful main CI run/attempt/commit. Rebuilds of changed source need a new candidate/version; successful artifact retries retain their original identity. Dirty local installer artifacts are not publication inputs.
4. Rehearse Marketplace and GitHub release using the checked-in version. Set the Extension release workflow's `version` to the reviewed tested version (`1.8.14` for this implementation), and select the intended channel. Rehearsal never increments or publishes. Verify all compiled versions, platform payloads, lock hashes, signing/provenance and qualification limits.
5. After explicit publication authorization, publish the verified artifacts through the [release workflow](release-workflow.md). GitHub and Marketplace use that same reviewed tested numeric version; its GitHub tag adds the `v` prefix. Publication must not silently allocate or rewrite a version after testing. Complete partial publication with the original matching artifacts and retry only the failed jobs. Never overwrite an existing version with different bytes.

The release-time draft version PR is a fallback when both destinations already contain a completed version. It does not replace the maintainer's reviewed version choice or permit publishing before the promoted source and main CI are ready. Keep the already authorized 1.8.14 implementation version for this work; any later promotion is a separately reviewed source/build step, not an automatic patch increase per phase. See [CI and builds](ci-and-builds.md), [extension deployment](extension-deployment.md), and [Windows builds](windows-local-build.md).
