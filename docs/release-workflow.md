# Protected PRs, GitHub downloads and extension release

For current behavior and upgrade guidance, see the [1.3.2 release notes](release-1.3.2.md); the [1.3.0 release notes](release-1.3.0.md) cover the preceding storage-format change. The [1.2.2 semantic indexing fix](release-1.2.2.md) updated both the bundled helper and extension. The [October 17 release summary](releases-2026-10-17.md) records earlier 1.1 releases. Documentation changes alone do not dispatch these workflows.

## Versioned GitHub Releases

The [GitHub release workflow](../.github/workflows/github-release.yml) promotes tested main CI binaries, signs and prepares downloads on GitHub-hosted machines. Run **Actions → GitHub release → Run workflow** on **main**, choose **stable** (the default), and enable **publish** to publish the verified downloads. Leave publish disabled for a promotion/signing rehearsal with downloadable Actions artifacts.

Changes to the release workflow or its release/signing contracts on `main` also run a short validation job without signing credentials. This explicit push event registers and validates the workflow independently of manual dispatch. Push runs skip release preflight and all dependent build, signing, publication and deployment jobs; those remain available only through manual dispatch.

New release tags match the checked-in package version (for this change, **`v1.3.2`**), with no workflow build suffix. Cargo, the extension and both lockfiles must agree. Workflow run IDs/attempts stay in provenance records, not public version names. Existing `-build.N` releases remain supported by tag validation and the downloads page; they are never renamed or overwritten.

Both manual release workflows check Marketplace and GitHub before promotion when **publish** is enabled:

| Checked-in version state | Result |
|---|---|
| Unused, including a manual increment | Publish that exact version; no automatic bump |
| Published only in the other destination | Finish this version in the requested destination |
| Already published in the requested destination only | Skip duplicate publication; finish the other workflow |
| All four Marketplace platforms and a complete GitHub release exist | Prepare the next unused patch in a draft version PR; do not publish |
| Retry of a completed release run | Skip; never allocate another version on a rerun |
| API failure, conflicting channel or incomplete GitHub asset set | Fail for inspection; never interpret an unavailable catalog as an unused version |

The fallback selects a patch unused in either catalog or existing tags, including draft reservations. Ordinary CI, identity checks and rehearsals never bump versions or create PRs. Both release workflows share a publication concurrency group. A newer main commit stops PR creation so a maintainer's manual increment takes precedence.

The version PR updates Cargo/extension manifests and lockfiles on `release/version-<version>`, reuses an existing open PR, and never pushes directly to protected main. Enable **Settings → Actions → General → Allow GitHub Actions to create and approve pull requests** for this fallback. Only its dedicated job receives `contents: write` and `pull-requests: write`; it does not approve or merge the PR. A maintainer must review the generated patch and replace the automation commit with their own authorized signed-off commit before merging. No human DCO certification is fabricated. GitHub can require approval before running workflows for a PR created using `GITHUB_TOKEN`; see [GitHub token behavior](https://docs.github.com/en/actions/concepts/security/github_token). After merge and successful main CI, dispatch the release workflows using the new version. The job summary links the PR and explicitly says publication is waiting.

For a partial Marketplace upload, the publisher downloads the already-published platforms and compares their extension payloads with the verified candidate. Only absent platforms are uploaded; mismatched content fails rather than mixing builds. Keep the original rehearsal for retries. A matching retained GitHub draft must be resumed through its original run's failed publish job.

Use **Re-run failed jobs** after a signing, assembly or publication failure. Successful signing outputs remain scoped to the same release run, while all original binaries remain bound to one authenticated producer run/attempt. A publishing retry selects the newest successfully assembled bundle from that same release run, including an earlier attempt. It checks existing draft identity and every present digest before uploading only missing files. An already published, complete matching release is an idempotent success. A conflicting or unexpected asset fails closed for inspection.

Before dispatching, wait for successful **CI** on the exact main commit. Leave `candidate_run_id` empty to select successful main push CI. If the candidate has expired or is missing, dispatch **CI** on that commit (optionally `fresh_build` for cold qualification), then supply its successful manual run ID. A failed CI attempt must be rerun completely to produce a complete single-attempt candidate set. PR artifacts and different commits are never eligible; no automatic rebuild occurs in a release job.

The workflow:

1. Authenticates the exact main CI workflow, repository, event, commit, successful platform/container/contract jobs, run attempt and complete eight-artifact set. It records immutable artifact IDs and server digests in a producer record.
2. Downloads the producer's native candidates by artifact ID, verifies each downloaded artifact digest, and rechecks producer identity before signing. Separate native signing jobs verify their commit, locks, payloads and checksums, then restore only the tested executable.
3. Signs Windows through Ioka's Azure Artifact Signing account/profile and signs/notarizes both Mac architectures through the existing Apple environment. Linux remains unsigned. Each native packager rechecks the OS signature, and each platform VSIX contains its matching helper.
4. Verifies all four native/VSIX pairs, including accepted Apple notarization, Azure identity, clean checkout and binding to the original tested binaries. It stages only the eight packages, eight uniquely named manifests, release record, notes and combined SHA256SUMS.
5. With publish enabled, creates a draft at the exact source commit, uploads those files, finds the draft through the paginated release list, and compares GitHub's asset digests against the local bundle. It verifies and publishes by the same release ID because GitHub's release-by-tag endpoint only returns published releases. Preview releases are marked prerelease; stable releases are marked latest. An upload/verification failure leaves the draft unpublished, and a retry resumes only an identical draft or a verified subset of its expected assets.
6. After successful publication, invokes the downloads page workflow to refresh the public website. A build/signing rehearsal does not deploy the site.

Published assets live on [AIRK's GitHub Releases page](https://github.com/iokaio/airk/releases). They are release downloads, independent of the seven-day Actions artifact retention. The [downloads website](#downloads-website) provides an overview and direct links to those same files. This workflow does not publish to Visual Studio Marketplace. The Marketplace workflow below remains separate and promotes unsigned native candidates into the requested VSIX channel.

The **native-signing** and **github-releases** environments must allow only the `main` branch, not tags. Windows uses the same Azure signing service as VCP: account `ioka-llc-signing`, profile `WritingForgePro`, endpoint `https://wus2.codesigning.azure.net`. Its Azure application needs AIRK-specific GitHub federation, with `AZURE_SIGNING_CLIENT_ID`, `AZURE_SIGNING_TENANT_ID` and `AZURE_SIGNING_SUBSCRIPTION_ID` set as **native-signing environment variables**. No client secret is used. The existing **macos-signing** environment supplies the Apple secrets; see [native signing](native-signing.md).

Only signing and Pages deployment jobs receive OIDC permission, and only the release publishing job receives repository content write permission. The website deployment receives Pages write permission. Neither ordinary PR CI nor the build jobs receive signing credentials. Release assembly and contract tests require Python 3.11+, matching the pinned hosted Python.

GitHub documents [release creation and draft uploads](https://cli.github.com/manual/gh_release_create). Review the installed editor, supported host matrix and dependency notices for this candidate alongside the [1.2 qualification record](indexingenhancements.md#final-12-qualification). A successful workflow establishes its recorded checks, not completion of every manual review.

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
- The branch must be current and pass GitHub Actions **Required checks** and **signed-off**. AIRK's stable required gate verifies the exact jobs selected by the complete-diff classifier; these names deliberately replace the other repositories' product-specific check names.

The [DCO workflow](../.github/workflows/dco.yml) checks all PR commit authors/co-authors for matching final `Signed-off-by` trailers. The real contributor supplies those trailers. CI does not create signatures, mutate identity or waive them. Large PRs beyond GitHub's complete commit response are rejected for manual restructuring.

The new required check names become available when a PR runs these workflow changes. Until then, a PR without them will remain blocked rather than weakening protection. Changing/removing a gate requires updating both the workflow and live rule through an authorized repository-settings change.

## Build manually

Open **Actions → Extension release → Run workflow**. Enter the exact checked-in version (for this change, `1.3.2`), select `stable` (the default), and leave `publish` false. Promotion requires `main`; use the CI workflow to build and review feature-branch candidates.

The [release workflow](../.github/workflows/extension-release.yml) authenticates a successful main [CI workflow](../.github/workflows/ci.yml) containing native tests, actual-helper extension tests, packaging checks and container validation. It rebuilds only the small TypeScript/VSIX layer for the selected channel using those tested native packages. Candidate artifacts expire after seven days; `candidate_run_id` selects an explicit successful manual CI rebuild when needed.

After reviewing a successful rehearsal, publish its exact VSIX files by supplying its numeric Actions run ID as **rehearsal_run_id**, keeping the same version/channel, and enabling **publish**. The rehearsal and publishing run must use the same exact main commit. This skips all four packaging jobs; publication downloads the rehearsed archives by authenticated artifact IDs and hashes, verifies their version/channel, payloads and native provenance again, then passes those files to VSCE. The publishing job still installs the pinned publishing tool but does not compile TypeScript or Rust.

For example, substitute the reviewed rehearsal's actual run ID:

~~~console
gh workflow run extension-release.yml --ref main -f version=1.3.2 -f channel=stable -f publish=true -f rehearsal_run_id=123456789
~~~

Only successful, non-publishing rehearsals qualify: preflight and all four packaging jobs must succeed, while publication and identity-only jobs must be skipped. Artifact upload times must belong to those successful jobs in the selected attempt. Preflight records the attempt, artifact IDs and server digests; publication rechecks that record before and after downloading. Rerunning the rehearsal, expired/missing artifacts, another commit, or a mismatched package version/channel stops publication without silently repackaging. Create and review a new rehearsal if needed. Leaving **rehearsal_run_id** empty preserves the existing build-and-publish path. Supplying it for a non-publishing or identity-only run is rejected.

Ordinary CI, local packaging and release dispatchers default to stable packages. Preview packaging remains an explicit option for a distinct future preview version; do not publish the same version in both channels. The source version/display name are never silently edited by a publishing job; review them before selecting a channel. See the [1.3.2 release notes](release-1.3.2.md) and [1.2 qualification record](indexingenhancements.md#final-12-qualification).

## Configure publication once

The repository's `vscode-marketplace` environment is restricted to the `main` branch. Only the publishing and explicit identity-verification jobs receive `id-token: write`; build jobs have read-only repository access. No Marketplace credentials or signing secrets are consumed during ordinary CI.

Publication uses GitHub OIDC to authenticate a dedicated Microsoft Entra application, then VSCE's `azureCredential` mode to obtain a short-lived Azure DevOps access token. It does not use a client secret or PAT fallback. Direct Marketplace `--oidc` is not used: a [VSCE maintainer confirmed on 5 October 2026](https://github.com/microsoft/vscode-vsce/issues/1275#issuecomment-5990771209) that Marketplace backend support was incomplete despite its README. The pinned VSCE version remains unchanged; its Entra authentication is independent of that exchange endpoint.

An authorized owner provisions the following identity and membership before publication:

1. In Microsoft Entra **App registrations**, register a single-tenant application named `airk-marketplace-publisher`. No redirect URI or client secret is needed. Record its Application (client) ID and Directory (tenant) ID.
2. Under **Certificates & secrets → Federated credentials → Add credential → Other issuer**, set **Issuer** to `https://token.actions.githubusercontent.com`. Select **Explicit subject identifier** and set **Value** to `repo:iokaio@152147161/airk@1418150478:environment:vscode-marketplace`. Set **Name** to `airk-marketplace-main` and leave **Audience** as `api://AzureADTokenExchange`. This is AIRK's immutable GitHub subject, verified through the repository OIDC customization API; do not replace it with the older name-only format. [Entra federation documentation](https://learn.microsoft.com/en-us/entra/workload-id/workload-identity-federation-create-trust#other-identity-providers) describes the fields.
3. In GitHub's **vscode-marketplace** environment, set variables `AZURE_MARKETPLACE_CLIENT_ID` and `AZURE_MARKETPLACE_TENANT_ID` to those IDs. The login uses `allow-no-subscriptions: true`; this identity needs Marketplace membership, not an Azure resource role.
4. Dispatch **Extension release** on **main**, enter the checked-in version, enable **verify_identity**, and leave **publish** disabled. This mode skips builds and publication. It authenticates through Entra and reports only the Marketplace profile ID in the job log and summary. It does not prove publisher membership.
5. Add that profile ID as a member of Marketplace publisher **iokaio** with the **Contributor** role. Use the returned profile ID, not the application's client or object ID. Microsoft's [service-principal publishing instructions](https://learn.microsoft.com/en-us/azure/devops/extend/publish/command-line?view=azure-devops#publish-with-a-microsoft-entra-token) document the profile lookup and publisher membership.

Identity verification and publication cannot be enabled together. GitHub's environment restriction and workflow preflight enforce main-only identity access. Azure login cleans up its session after each job. Do not put credentials in workflow inputs, logs or source. These distribution identities are separate from AIRK's runtime, which does not require an account.

## Publish deliberately

After the checklist and candidate review, an authorized maintainer dispatches the workflow on `main` with `publish` true. The workflow promotes all targets from one authenticated main CI run/attempt/commit, and independently verifies the complete platform set, version/channel, archive and payload hashes, lock identity, embedded native provenance and clean checkout. VSCE receives those exact VSIX paths; it does not rebuild from the source tree in the publishing job.

Selecting a publication channel and dispatching with publish enabled is the publication action. Adding this workflow does not publish an extension, create a GitHub Release, tag a version or push repository code. Entra federation and publisher membership, signing and notarization are independent prerequisites where required by the release policy.

Do not cancel a publishing run or automatically skip duplicates to conceal a partial failure. Marketplace uploads of four platform files are not one transaction. Inspect which platform/version/channel entries succeeded, retain the receipts, and decide how to complete or supersede that version using the publisher's documented controls. Ordinary builds can be retried safely into fresh run artifacts.
