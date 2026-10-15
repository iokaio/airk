# Protected PRs, GitHub downloads and extension release

## Numbered GitHub Releases

The [GitHub release workflow](../.github/workflows/github-release.yml) builds, signs and prepares downloads on GitHub-hosted machines. Run **Actions → GitHub release → Run workflow** on **main**, choose **preview** or **stable**, and enable **publish** to publish the verified downloads. Leave publish disabled for a full build/signing rehearsal with downloadable Actions artifacts.

The release tag is automatically numbered from this workflow's run number: for example, `v0.1.0-build.42`. The base version comes from the checked-in Cargo workspace and must match the VS Code extension. App/extension versions remain the checked-in version (currently `0.2.0`); no commit, lockfile or package metadata is rewritten. Every new dispatch advances the build number, including failed/rehearsal runs. Reruns retain the number and change the attempt. An existing release or tag is never overwritten; after a partial publication, inspect the retained draft and dispatch a new build.

Before a draft exists, retry with **Re-run all jobs** so every input belongs to the new attempt. Re-running only failed signing/publishing jobs cannot consume artifacts from an older attempt.

The workflow:

1. Runs the full existing four-platform CI and Linux container checks, with compiler/dependency caches disabled for the release build.
2. Downloads the exact successful run/attempt's native candidates. Separate native signing jobs verify their commit, locks, payloads and checksums, then restore only the tested executable.
3. Signs Windows through Ioka's Azure Artifact Signing account/profile and signs/notarizes both Mac architectures through the existing Apple environment. Linux remains unsigned. Each native packager rechecks the OS signature, and each platform VSIX contains its matching helper.
4. Verifies all four native/VSIX pairs, including accepted Apple notarization, Azure identity, clean checkout and binding to the original tested binaries. It stages only the eight packages, eight uniquely named manifests, release record, notes and combined SHA256SUMS.
5. With publish enabled, creates a draft at the exact source commit, uploads those files, compares GitHub's asset digests against the local bundle, then publishes. Preview releases are marked prerelease; stable releases are marked latest. An upload/verification failure leaves the draft unpublished.
6. After successful publication, invokes the downloads page workflow to refresh the public website. A build/signing rehearsal does not deploy the site.

Published assets live on [AIRK's GitHub Releases page](https://github.com/iokaio/airk/releases). They are release downloads, independent of the seven-day Actions artifact retention. The [downloads website](#downloads-website) provides an overview and direct links to those same files. This workflow does not publish to Visual Studio Marketplace. The existing Marketplace workflow below remains separate and currently builds unsigned packages.

The **native-signing** and **github-releases** environments must allow only the `main` branch, not tags. Windows uses the same Azure signing service as VCP: account `ioka-llc-signing`, profile `WritingForgePro`, endpoint `https://wus2.codesigning.azure.net`. Its Azure application needs AIRK-specific GitHub federation, with `AZURE_SIGNING_CLIENT_ID`, `AZURE_SIGNING_TENANT_ID` and `AZURE_SIGNING_SUBSCRIPTION_ID` set as **native-signing environment variables**. No client secret is used. The existing **macos-signing** environment supplies the Apple secrets; see [native signing](native-signing.md).

Only signing and Pages deployment jobs receive OIDC permission, and only the release publishing job receives repository content write permission. The website deployment receives Pages write permission. Neither ordinary PR CI nor the build jobs receive signing credentials. Release assembly and contract tests require Python 3.11+, matching the pinned hosted Python.

GitHub documents [release creation and draft uploads](https://cli.github.com/manual/gh_release_create). Full installed-editor, oldest-OS and dependency notice review remain in the [release checklist](release-checklist-0.1.0.md). A successful workflow establishes its recorded checks, not completion of that checklist.

## Downloads website

The [Downloads page workflow](../.github/workflows/downloads.yml) publishes AIRK's own GitHub Pages site at **https://airk.ioka.io/**. It uses [the page template](../site/index.html), local CSS/SVG assets and [the static builder](../scripts/build-downloads.py). No visitor-side API calls, third-party scripts, external fonts or website runtime are needed.

The page describes AIRK, explains VSIX installation, and offers the most recently published numbered release in each channel: stable first, then preview. It links directly to each platform's extension and native CLI package, release notes, checksums and build record. Downloads remain in `iokaio/airk` GitHub Releases. The generator ignores drafts and unrelated tag formats, checks the complete expected asset set and direct AIRK download URLs, and refuses malformed/incomplete numbered releases. API failures stop deployment and retain the previous page. Before the first published release, the page shows an explicit pending-download state with source/build instructions.

After these changes are merged to `main`, website changes deploy automatically. For an independent refresh use **Actions → Downloads page → Run workflow → main**. Each successful **GitHub release** publication calls the same workflow explicitly; it does not depend on a `release` event from `GITHUB_TOKEN` starting another workflow. If only website deployment fails after a release is public, rerun **Downloads page**, not the full signed release. Retracted/deleted releases require a manual page refresh.

Only the three generated page files are uploaded from `artifacts/downloads-site`. The checkout, native packages, private indexes and local review artifacts are never the Pages artifact. The `github-pages` environment allows only the `main` branch. Its deployment job receives `pages: write` and `id-token: write`.

### Domain configuration

On 14 October 2026, AIRK's GitHub Pages configuration was created with **GitHub Actions** as the source and **airk.ioka.io** as its custom domain. The owner added the following DNS record, verified at the authoritative nameserver:

| DNS type | Host | Value | TTL |
|---|---|---|---|
| CNAME | `airk` | `iokaio.github.io` | 1 hour |

VCP's `downloads.ioka.io` record and Pages configuration stay separate. Both subdomains point to GitHub's shared host; each repository's custom-domain setting selects its site. A `CNAME` file in the uploaded artifact does not configure a domain for an Actions-based deployment. GitHub documents the [subdomain setup](https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/managing-a-custom-domain-for-your-github-pages-site#configuring-a-subdomain).

GitHub must issue the certificate before **Enforce HTTPS** can be enabled in [AIRK's Pages settings](https://github.com/iokaio/airk/settings/pages). Domain setup alone does not deploy the page: the workflow must be merged and run. This page and GitHub Releases are separate from Visual Studio Marketplace publication.

### Local preview

With GitHub CLI authenticated, Python 3.11+ can build from the public release catalog:

~~~console
python scripts/build-downloads.py
python -m http.server 8765 --bind 127.0.0.1 --directory artifacts/downloads-site
~~~

Open `http://127.0.0.1:8765/`. The output directory must be new; use `--output artifacts/downloads-preview-2` for a subsequent build. For an offline preview, `--catalog <path>` accepts a saved GitHub releases array; an empty array renders the pending-download state.

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

Open **Actions → Extension release → Run workflow**. Enter the exact checked-in version (currently `0.2.0`), select `preview` or `stable`, and leave `publish` false. Builds may be reviewed from a feature branch; publication requires `main`.

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
