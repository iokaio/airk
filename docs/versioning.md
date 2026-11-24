# Versioning from local installation to publication

**Current source candidate: 1.8.209 (24 November 2026).** Native and extension
manifests and lockfiles agree. The
[1.8.209 record](releases/release-1.8.209.md) distinguishes implemented changes
from pending artifact, installed, platform and model qualification. The
[local trust plan](simplify/local-trust-plan.md) phase D governs this candidate, and the
[execution ledger](simplify/one-process-ledger.md) retains exact outcomes.
Version 1.9.0 is reserved for final delivery after its applicable gates pass.

Use one numeric `major.minor.patch` version for the Rust workspace, every local
AIRK crate in `Cargo.lock`, `packages/vscode/package.json` and both root package
entries in `packages/vscode/package-lock.json`. Compiled images, runtime metadata,
native packages and VSIX must match that source version. The unused 1.8.209
increment has already been applied for this milestone; do not repeat it for the
same candidate or rewrite an earlier binary, package or receipt to match it.
A later changed installable candidate needs its own reviewed unused version.

```console
python scripts/release-version.py check --expected 1.8.209
```

After a version or interface change, regenerate native Help, the native-emitted
UI fixtures, shared Help manifest and Explorer catalog through their owners,
then compile and seal the affected UI assets before building the matching native
pair. Packaging consumes tested artifacts. Exact build and qualification order
is in [CI and builds](ci-and-builds.md); a source-version check alone proves
neither runtime behavior nor release readiness.

AIRK Service and the optional universal VSIX are separate products. Current
agents use the loopback HTTP endpoint with no MCP token, credential or AIRK
certificate; Host/Origin checks remain. Each read names indexes, index sets or
a workspace default. Use `airkctl agents manual` or **Set up manually** for the
Rust-owned target file and complete entry when automatic editing cannot protect
the entire file and recovery copy. Regenerate that entry after a service port
change. Historical scoped-credential and stable-launcher instructions below are
obsolete for current agent setup; see [agent integration](agent-integration.md)
and [installation](service-installation.md) for current contracts and recovery.

Current-version index regeneration is allowed when an upgrade cannot preserve
the supported contract. Source files, unrelated user work and historical state
identities remain protected. A product version does not override independent
persisted-format, installation, namespace, index or recovery fences. Installed
acceptance uses isolated fixtures and must exercise applicable clean, upgrade,
interruption/recovery, repair, silent and uninstall paths; the user's actual
installation is preserved unless separately authorized.

## Current publication authority

Local candidate implementation, packaging and testing do not authorize a public
release. Current work on `feat/arbiter-final` allows reviewed local human
DCO-signed milestone commits. Push, PR creation, merge, deployment and publication
require their separate authorization. Keep existing owner and ACL policies,
private artifacts and historical evidence intact.

Public release still requires all applicable gates on the identified source,
including workspace/doctest, real transport, supported platform, installed and
artifact qualification. Final-series optional-model publication also requires
all three qualified selected profiles, rights/source maps and the native
publication capture. A local Windows pass does not qualify another platform.
Missing hosts, evidence or public availability remain explicit blockers.

The release workflow continues to require reviewed human DCO changes through
the protected PR/merge flow and full successful main CI on the exact final
commit. Promote one authenticated platform candidate set from that run and
attempt; dirty local artifacts are not substitutes. After concrete artifacts and
all required gates are ready, publication needs explicit approval through the
[release workflow](release-workflow.md). GitHub and Marketplace use the same
reviewed tested version; its GitHub tag adds `v`. Never allocate a different
version after testing, overwrite a release with changed bytes or manufacture a
sign-off. A failed publication job retries only its original matching artifacts.
The version-PR fallback does not replace reviewed version choice or these gates.

<details>
<summary>Historical development and predecessor installation guidance (9–13 November 2026)</summary>

The retained text below describes its original 1.8.x source and artifacts. Its
statements about a “current” 1.8.35 or 1.8.23 version, candidate build progress,
scoped agent credentials, predecessor bootstrap and stopped work are dated
history. They do not override the current source, tokenless agent contract,
installation contract, active implementation instruction or publication rules
above. Original versions, artifact claims and failed evidence are preserved.

Runtime installation and upgrades display a VS Code progress notification labeled Installing AIRK runtime, Updating AIRK runtime or Checking AIRK installation, with the current stage: version and payload checks, staging, native activation, agent connection updates, project registration and cleanup. Progress uses actual stages without an invented percentage. The 1.8.14 implementation checkpoints and drains an admitted modern service before replacement; clients reconnect to its verified HTTPS endpoint. Success and failure remain explicit. VS Code owns the earlier VSIX installation UI, before AIRK can activate. Final installed qualification is recorded separately below.

Historical 1.5.4–1.8.13 installation records the exact activation intent before stopping the verified optional manager and terminating approved predecessor processes, including older versions and Windows clients holding the stable launcher. Captured native process identities prevent PID reuse from redirecting termination. All captured processes share a five-second exit wait; verification, manager control, copying and startup are additional. Historical macOS installation uses authenticated graceful shutdown. The 1.8.14 migration retains version-specific predecessor retirement, rather than assuming those old processes implement the new checkpoint protocol. See [installation](service-installation.md) for ownership and platform limits.

The current local candidate is **1.8.35** (13 November 2026): see the
[1.8.35 record](releases/release-1.8.35.md). Manifests and lockfiles are synchronized at 1.8.35.
1.8.34 ([record](releases/release-1.8.34.md)); 1.8.33 ([record](releases/release-1.8.33.md)); 1.8.32 ([record](releases/release-1.8.32.md)); 1.8.31 ([record](releases/release-1.8.31.md)); 1.8.30 ([record](releases/release-1.8.30.md)); 1.8.29 ([record](releases/release-1.8.29.md)); 1.8.28 was not used. 1.8.27 ([record](releases/release-1.8.27.md)), 1.8.26 ([record](releases/release-1.8.26.md)), 1.8.25 ([record](releases/release-1.8.25.md)), 1.8.24 ([record](releases/release-1.8.24.md)), the
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

</details>
