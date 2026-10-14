# Protected PRs and manual extension release

## Main branch protection

The live [AIRK main ruleset](https://github.com/iokaio/airk/rules/25054101) was enabled on 13 October 2026. Its desired configuration is [.github/main-ruleset.json](../.github/main-ruleset.json). Protection is a GitHub setting; committing JSON alone does not enable it.

The comparison found Munarium using classic branch protection and VCP using a ruleset. Both require PRs without a mandatory approving-review count. AIRK adopts Munarium's stricter settings and VCP's explicit empty bypass list:

- Every main change goes through a PR, including administrator changes; no bypass actor is configured.
- No force pushes or branch deletion; merge history must be linear (squash/rebase).
- Resolve review conversations; stale reviews are dismissed after new pushes. The minimum approval count remains **zero**, matching both repositories.
- The branch must be current and pass GitHub Actions **Required checks** and **signed-off**. AIRK's stable required gate depends on all native matrix jobs and the container job; these names deliberately replace the other repositories' product-specific check names.

The [DCO workflow](../.github/workflows/dco.yml) checks all PR commit authors/co-authors for matching final `Signed-off-by` trailers. The real contributor supplies those trailers. CI does not create signatures, mutate identity or waive them. Large PRs beyond GitHub's complete commit response are rejected for manual restructuring.

The new required check names become available when a PR runs these workflow changes. Until then, a PR without them will remain blocked rather than weakening protection. Changing/removing a gate requires updating both the workflow and live rule through an authorized repository-settings change.

## Build manually

Open **Actions → Extension release → Run workflow**. Enter the exact checked-in version (initially `0.1.0`), select `preview` or `stable`, and leave `publish` false. Builds may be reviewed from a feature branch; publication requires `main`.

The [release workflow](../.github/workflows/extension-release.yml) invokes the same [CI workflow](../.github/workflows/ci.yml) used for PRs: native build/tests, extension tests with the real helper, repository/packaging checks, the four platform packages, and Linux container validation. It does not skip checks when publishing. Candidate artifacts expire after seven days.

Ordinary CI and its manual dispatch create preview packages. The release dispatcher can explicitly choose stable metadata. The source version/display name are never silently edited by a publishing job; review them before selecting a channel. See the [0.1.0 checklist](release-checklist-0.1.0.md).

## Configure publication once

The repository's `vscode-marketplace` environment is restricted to the `main` branch. The publishing job alone receives `id-token: write`; build jobs have read-only repository access. No Marketplace credentials or signing secrets are consumed during ordinary CI.

An authorized owner of Marketplace publisher **iokaio** must configure a trusted-publishing policy for repository **iokaio/airk** and workflow **.github/workflows/extension-release.yml**, with the intended branch/environment claims. This external publisher setup has not been performed by the repository changes. The pinned VSCE 4.0.0 supports GitHub OIDC and exchanges it for a short-lived Marketplace credential, without a PAT fallback. See [VSCE's official trusted publishing documentation](https://github.com/microsoft/vscode-vsce#trusted-publishing).

If the publisher account does not expose that trust-policy facility, publication remains blocked until the owner provisions a supported authentication route. Do not put credentials in workflow inputs, logs or source. Microsoft's [extension publishing guide](https://code.visualstudio.com/api/working-with-extensions/publishing-extension) also describes Entra authentication and publisher prerequisites; it is separate from AIRK's runtime, which does not require an account.

## Publish deliberately

After the checklist and candidate review, an authorized maintainer dispatches the workflow on `main` with `publish` true. The workflow builds all targets, downloads only artifacts from that same run/attempt/commit, and independently verifies the complete platform set, version/channel, archive and payload hashes, lock identity, embedded native provenance and clean checkout. VSCE receives those exact VSIX paths; it does not rebuild from the source tree in the publishing job.

Selecting a publication channel and dispatching is the publication action. Adding this workflow does not publish an extension, create a GitHub Release, tag a version or push repository code. OIDC publisher trust, signing and notarization are independent prerequisites where required by the release policy.

Do not cancel a publishing run or automatically skip duplicates to conceal a partial failure. Marketplace uploads of four platform files are not one transaction. Inspect which platform/version/channel entries succeeded, retain the receipts, and decide how to complete or supersede that version using the publisher's documented controls. Ordinary builds can be retried safely into fresh run artifacts.
