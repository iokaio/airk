# Reviewed runtime upgrades and rollback

This is the implemented development contract for the [service console](service-console.md). Native and extension versions remain 1.3.2 while default adapter cutover is incomplete. It does not describe a released VSIX.

The trusted console offers **Review bundled runtime upgrade**, **Review previous runtime** and **Review saved runtime activation**. The host reads the current installation, selected approval, pending native activation and exact candidate before asking for confirmation. The review identifies the execution host, preceding selection, target digest/version and drain/start effects. It does not claim to have counted other connected clients. Organization-managed payloads disable competing user upgrade/rollback controls.

The webview supplies only a fixed action at its current render revision. It cannot supply a digest, path, command or approval. The host freezes each review and retains its object identity privately. A copied/forged review, changed binding or lost workspace authorization fails before submission. Bundled upgrade stages only the already verified distribution; rollback/resume verify the exact approved installed candidate through metadata-only `runtime inspect-candidate`.

## Selection guard and recovery

`runtime activate --install-dir <installation> --runtime-sha256 <target> --expected-active <preceding>` checks the captured preceding selection under the installer lock before drain, launcher repair or startup. A competing selection change invalidates the review. An existing pending activation must match both target and predecessor, and its saved ownership/state checks still apply. It cannot be retargeted by a later window.

The installer retains the existing durable activation phases and result receipt. A failed copy can leave bounded resumable staging; an interrupted activation remains visible with its saved target and phase. The console refreshes metadata after an uncertain result without automatically repeating the mutation. A user can review and resume that exact native activation. No second extension journal is needed for this native transaction: its identity and progress are already persisted before drain.

When the selected target and saved successful result prove the exact completed target/predecessor pair, replay only acknowledges that receipt. It does not start a service, reopen a project or interpret startup state. In particular, a later explicit Stop remains stopped. A missing/crossed result is not completion proof.

The installer verifies candidate startup and registry readiness before changing public selection. Failure retains the previous selection or its explicit recovery state. Sources, project identities, accepted work receipts and derived-state ownership remain independent of executable selection. A successful probe is not a claim that all source/semantic work is complete.

## Versions, pins and observation

Discovery and automatic preparation preserve an existing approved pin even when its version differs from the extension. They do not silently replace it on reconnect. An exact-version native runtime is required for the extension's metadata console and project-configuration transport. A different selected version remains available for installer diagnostics, saved snapshots and a reviewed bundled upgrade; the view explains the mismatch and offers the matching-extension path.

Rollback candidates must be independently approved and belong to the same major/minor runtime family as the extension. The review shows their actual version. Candidate startup and schema checks decide whether an approved older patch can use the retained state; approval alone is not format compatibility. No old-format reader, direct writer or proxy fallback is introduced. Cross-minor transitions remain the plan's explicit clean-break boundary.

Installer/lifecycle inspection now opens the completed registry read-only, without migration, pruning or initialization. A regression reads a version-2 development registry and verifies unchanged database bytes/version. Unknown or unreadable saved schemas remain explicit failures. Writable schema upgrades belong to authorized startup/mutation, not a preview of another running runtime.

Activation starts and verifies the selected service. A deliberately stopped service requires explicit Start before a new upgrade/rollback; reviewing an exact saved completion never clears that preference. The optional OS-manager ownership checks, immutable slots, bounded copies, startup circuit and rollback rules remain those in [installation](service-installation.md).

## Development evidence

Native tests cover changed selection, crossed pending intent, exact completed replay without startup/state initialization, existing activation crash/abort boundaries and read-only lifecycle inspection. Extension tests cover immutable host reviews, digest guards, managed-policy refusal, different-version pins, compatible-family candidate metadata, trust loss during confirmation, exact reviewed submission, saved-phase presentation and diagnostic redaction.

Windows and Linux each pass 107 CLI unit tests. The extension passes 190 tests with six skips; Python passes 129 tests with four skips. Workspace all-feature/all-target Clippy, separately run workspace documentation tests, formatting, repository and whitespace checks pass. Linux evidence uses image `cb64f3c4d44bcf8b6ec36e098b95bf9628ddb25f85dc88c662d009f2f0b1ae19`.

The real native/controller workload passes reviewed upgrade/rollback, accepted-job/source preservation, stale native selection refusal and completed replay after explicit Stop. The separate fault run has recovered the exact saved activation after terminating only its owned installer at Probe and has completed rollback; its final Stop/start assertions and cleanup are still pending. Neither workload substitutes for final default-adapter qualification.

Those workloads use frozen development pair 43: source receipt `8f5ad8e0c775d421012a3a78818e492d3dd2289dec2e99b233b082866a38b4b3` (631 inputs), main `4930b0c522879681366e8f1908275e2e60fe5b8768892ebee9281017667047f5`, helper `de148e706493dd1c542abf302eff0b6e599882996c55e0162fcfcf3e4c20b771`. Pair 42 is the preceding approved selection. Pair 43 predates the read-only inspection fix, which is covered by the later Windows/Linux unit and Clippy checks. Native Mac and installed-editor qualification remain separate. No VSIX has been built.
