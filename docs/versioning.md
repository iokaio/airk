# Versioning from local installation to publication

Runtime installation and upgrades display a VS Code progress notification labeled Installing AIRK runtime, Updating AIRK runtime or Checking AIRK installation, with the current stage: version and payload checks, staging, native activation, agent connection updates, project registration and cleanup. Progress uses actual stages without an invented percentage. The 1.8.14 implementation checkpoints and drains an admitted modern service before replacement; clients reconnect to its verified HTTPS endpoint. Success and failure remain explicit. VS Code owns the earlier VSIX installation UI, before AIRK can activate. Final installed qualification is recorded separately below.

Historical 1.5.4–1.8.13 installation records the exact activation intent before stopping the verified optional manager and terminating approved predecessor processes, including older versions and Windows clients holding the stable launcher. Captured native process identities prevent PID reuse from redirecting termination. All captured processes share a five-second exit wait; verification, manager control, copying and startup are additional. Historical macOS installation uses authenticated graceful shutdown. The 1.8.14 migration retains version-specific predecessor retirement, rather than assuming those old processes implement the new checkpoint protocol. See [installation](service-installation.md) for ownership and platform limits.

The current local candidate is **1.8.29** (12 November 2026): see the
[1.8.29 record](releases/release-1.8.29.md). Manifests and lockfiles are synchronized at 1.8.29.
1.8.28 was not used. 1.8.27 ([record](releases/release-1.8.27.md)), 1.8.26 ([record](releases/release-1.8.26.md)), 1.8.25 ([record](releases/release-1.8.25.md)), 1.8.24 ([record](releases/release-1.8.24.md)), the
first split build, and the 1.8.23 paragraph below are dated history.

The 1.8.23 candidate was prepared on 10 November 2026. It corrected
the 1.8.22 manual-test failure with empty installation and Reconnect that cannot
create an index. Source manifests and lockfiles are synchronized at 1.8.23 and
the candidate build is in progress. No 1.8.23 native package or VSIX has been
built or qualified.
See the [1.8.23 record](releases/release-1.8.23.md). The previous
[1.8.22 artifact](releases/release-1.8.22.md) keeps its original version, hashes
and failed evidence.
The user gave standing authorization on 10 November 2026 for every new
installer build to take the next unused patch number. The integrator bumps the
manifests immediately before the VSIX/native package build, not during
implementation. A 1.8.21 VSIX of this work was built and not handed out
(its installed-editor check failed); the manifests were then bumped to 1.8.22.
1.8.17 was a working number only; no 1.8.17 package was built.

**Through 1.9.0 inclusive, installation starts empty after full owned uninstall.**
Installing the same build also requires full uninstall first. Ordinary editor
reopening reconnects; it is not another installation. No index is created until
explicit Index library setup, and Reconnect cannot create an index. Source
folders are never changed. In-place version upgrades require both versions to
be greater than 1.9.0 and the supported state contract. This user decision
(10 November 2026) supersedes the
[historical rebuild contract](upgrade-rebuild-fallback.md). The
[manual-test ledger](simplify/one-process-ledger.md#manual-test-installer-correction-10-november-2026)
records 1.8.22's failures and the authorized correction. See
[installation](service-installation.md#replacing-an-installation-through-190).

Fresh installs still record `airk-state-contract.json` (schema 1,
`state_contract: "airk-state/1"`, `baseline_version: "1.8.16"`) with its exact
install/control/namespace binding. Registry, collection, source and transport
formats keep their independent compatibility checks.
The plan is to stabilize and qualify this development line before a future
public **1.9.0** release. Native/extension source manifests and lockfiles are
synchronized at 1.8.23; generated catalog, build and artifact checks are in
progress. 1.9.0 is not a version bump or publication
authorization for this task.
See the [current record](releases/release-1.8.23.md) and
[release-readiness ledger](simplify/release-readiness.md); the
[1.8.14 working record](releases/release-1.8.14.md) retains dated implementation
and failed-attempt evidence. Publication still requires exact-source qualification
and successful release workflows.

Use one numeric `major.minor.patch` version for the Rust workspace, every local AIRK crate in `Cargo.lock`, `packages/vscode/package.json`, and both root package entries in `packages/vscode/package-lock.json`. The compiled main, runtime metadata, native ZIP and VSIX must agree with that source version. The next candidate build uses `1.8.23`; a release dispatch/tag must use the separately reviewed, fully validated source version. The intended public 1.9.0 release requires separate qualification and promotion. The working-version adjustment authorizes no CI dispatch or publication.

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

Update all four version-bearing files together before building. Do not edit executable versions, manifests, receipts or ZIP/VSIX contents after tests to make them match. Rebuild the native runtime when its compiled version changes. After the synchronized candidate bump, check agreement without network access or publication:

```console
python scripts/release-version.py check --expected 1.8.23
```

CI runs the same agreement check in its unconditional quick job. It rejects mismatched manifests/lockfiles and, when supplied, a mismatched `expected_version`. Local build, native packaging and extension packaging verify compiled identities and bind the exact payload/lock hashes. New changed installable candidates need the next unused numeric patch; the checked version is not bumped automatically on each CI rerun.

## Installation and running clients

A distinct extension version installs into a distinct VS Code extension directory. The prepared 1.8.23 extension will install its matching approved main outside that directory, under a digest-specific runtime slot. Native bootstrap selects that image directly. Agent configuration uses the verified loopback HTTPS endpoint and scoped credential reference; it must not select an extension-bundled executable or a particular runtime slot. Historical pairs used a stable `airk-launcher` selector; replacement retires owned entries and requires reviewed fresh client reconnection.

Native installation first completes owned uninstall through 1.9.0 inclusive;
same-build installation also requires full uninstall. The successor starts
empty, without replaying source selections, indexing jobs or watching. Recovery
binds the exact durable installation operation and never resets its successor
again on replay. Unknown or locked executables require reported cleanup, never
name-based deletion. Source files remain untouched. Compatible version upgrades
require both versions greater than 1.9.0 and retain their separate state-contract,
index, job and format-preservation checks.

After replacement, Codex, Claude Code and VS Code clients require reviewed reconnection with fresh namespace authority; old grants and credentials are not transferred. Within a supported compatible line, owned connection configuration can be refreshed to the current HTTPS endpoint without silently widening scope. Unsupported client/trust combinations remain explicit refusals; serializer support alone does not qualify an installed client. Existing clients still need reconnection. Successful activation completes quietly. Once active, the 1.8.12 loader handles later detected installed-versus-running extension replacements automatically, disposing the old AIRK instance and reopening its single Explorer panel if open. An older loaded extension needs VS Code to activate the new loader first; use the **Restart Extensions** action in VS Code’s Extensions pane for that one-time transition. Runtime installation remains fenced while a mismatch is pending. Explorer dismissal preferences hide notices without changing those gates. A connection or cleanup problem offers one Review details action; per-file migration errors remain in the output. Copilot's AIRK server can be restarted in MCP controls. External agent sessions need their own restart. An editor reload is different from fully exiting VS Code when its extension manager rejects a same-version reinstall; AIRK cannot show a pre-install notification when its code has not run.

Installer acceptance must include both a clean installation and an upgrade over the previous version while its editor and MCP clients are running. Use the isolated harness, retaining logs, process/runtime observations, notification screenshots and failures:

```console
node scripts/check-agent-install.cjs <Code.exe> <new-vsix> <previous-vsix>
```

The optional previous VSIX selects the running-upgrade scenario. Without it, the harness tests a clean install. Neither scenario modifies the normal editor profile or publishes artifacts. A passing local Windows check does not qualify other platforms or the entire release suite.

## CI and release handoff

Finish the manual-test correction and qualification, then manually accept the
newly identified candidate before release preparation. Source manifests and
lockfiles are synchronized at 1.8.23; candidate compilation and packaging are in
progress. Existing 1.8.22 artifacts keep their hashes and failed results. A
source/catalog version adjustment is not a tested runtime or release artifact.
Earlier 1.8.12, 1.8.8 and 1.9.0 development binaries retain their original versions
and hashes; none is relabeled as a new implementation or promoted artifact.

1. Before a feature/fix PR, the maintainer reviews the patch increment, synchronized locks, README, documentation index, current release notes and Marketplace listing. A bump already made for that PR is not repeated. Keep dated earlier receipts and release records identifiable as history.
2. Merge the reviewed, human DCO-signed changes through the protected PR flow. Run full CI on the exact final main commit before publication. Skipping full tests for local installer iteration does not waive CI release gates.
3. Promote the complete platform candidate set from one authenticated successful main CI run/attempt/commit. Rebuilds of changed source need a new candidate/version; successful artifact retries retain their original identity. Dirty local installer artifacts are not publication inputs.
4. Rehearse Marketplace and GitHub release using the checked-in version. Set the Extension release workflow's `version` to the reviewed tested version, and select the intended channel. The prepared 1.8.23 candidate is not a publication input until its own qualification completes. Rehearsal never increments or publishes. Verify all compiled versions, platform payloads, lock hashes, signing/provenance and qualification limits.
5. After explicit publication authorization, publish the verified artifacts through the [release workflow](release-workflow.md). GitHub and Marketplace use that same reviewed tested numeric version; its GitHub tag adds the `v` prefix. Publication must not silently allocate or rewrite a version after testing. Complete partial publication with the original matching artifacts and retry only the failed jobs. Never overwrite an existing version with different bytes.

The release-time draft version PR is a fallback when both destinations already contain a completed version. It does not replace the maintainer's reviewed version choice or permit publishing before the promoted source and main CI are ready. The user requested the 1.8.19 local candidate on 9 November 2026, superseding the 1.8.16 retry version and the unbuilt 1.8.17 working number while retaining its persisted-state baseline and historical receipts. The user requested 1.8.20 on 10 November 2026 for the login-startup correction, because 1.8.19 was already installed for manual testing. The one-process artifact 1.8.22 keeps its original version and failed evidence. The next candidate, 1.8.23, uses the user's standing authorization of 10 November 2026 for the next unused patch number per installer build; its source manifests and lockfiles are now synchronized. Later promotion remains separately reviewed and qualified, without an automatic patch increase per phase. See [CI and builds](ci-and-builds.md), [extension deployment](extension-deployment.md), and [Windows builds](windows-local-build.md).
