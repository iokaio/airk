# Dependency advisory policy and dispositions

This page describes how AIRK checks its locked dependencies against published advisories, and records a dated decision for each finding. An advisory check shows what a database reported on a given day. It does not prove that the dependencies are safe.

## Policy

- `scripts/dependency-audit.py` checks `Cargo.lock` with cargo-audit 0.22.2 and `packages/vscode/package-lock.json` with `npm audit --package-lock-only`. It reads the lockfiles only: it installs nothing and runs no project or dependency code.
- Every vulnerability needs a disposition, and so does every unmaintained, unsound or yanked warning. Development and publishing dependencies are not automatically exempt.
- A disposition is one of:
  - a compatible update or removal;
  - a scoped exception recorded in [.cargo/audit-exceptions.json](../.cargo/audit-exceptions.json), with package, version, advisory, scope, evidence, reviewer and a `review_by` date;
  - a separately tracked unresolved action.
- An exception covers only the exact package version and advisory it names. It stops applying when the version changes, it expires on its `review_by` date, and an exception that no longer matches any finding fails the check. This keeps stale approvals from piling up.
- Exit codes:
  - 0: every finding is covered;
  - 1: at least one finding has no disposition, or an exception is stale or expired;
  - 2: the scanner or the advisory database could not be reached. Exit 2 is never a pass.
- The [dependency audit workflow](../.github/workflows/dependency-audit.yml) runs when lockfiles, the exceptions file or the script change, weekly, and on request. It is separate from routine CI. The maintainer reviews exceptions; an unresolved vulnerability blocks a release.

## Dispositions recorded 2026-10-30

These cover the findings in [review 02, draft 04](reviews/review02/04-dependency-findings-triage.md), which was written against the 1.7.2 lockfiles. The fresh check used RustSec database commit `2295466c4e9df26647fd67d8fa1c705ac2fc7f81` (374 packages) and the npm registry advisory feed, on 2026-10-30.

| Dependency | Finding | Disposition |
|---|---|---|
| `quick-xml` 0.39.4 | RUSTSEC-2026-0194, RUSTSEC-2026-0195 | Updated to 0.41.0 in 1.7.3. A per-element limit of 1,024 attributes is kept, and 1.7.6 reports budget rejections separately. No first-party `NsReader` use was found. |
| `zip` 4.6.1 | RUSTSEC-2026-0367, 0368, 0369 | Updated to 9.1.3 in 1.7.3. Office members are read into memory; no filesystem extraction API is used. |
| `tokio` 1.53.2 | RUSTSEC-2026-0386 (`AsyncFd` unsoundness warning) | Updated to 1.53.3 in 1.7.3. |
| `paste` 1.0.15 | RUSTSEC-2024-0436 (unmaintained) | Exception until 2027-01-31. A compile-time proc-macro of `tokenizers` 0.23.2; no runtime code ships from it, and no compatible `tokenizers` release removes it. |
| `ttf-parser` 0.25.1 | RUSTSEC-2026-0192 (unmaintained) | Exception until 2027-01-31. Reached only through `lopdf` 0.42.0 in the pinned `pdf-extract` 0.12.1, inside the bounded PDF worker subprocess. Replacing `pdf-extract` is a separate action. |
| `@azure/identity` 4.14.0, `@azure/msal-node` 6.0.1 | GHSA-xcx6-jx8p-x3h5 | Updated to 4.14.1 and 7.1.2 (patched at 7.1.0) in 1.7.3. See the publishing assessment below. |
| `qs` 6.16.0 | GHSA-6pq8-jhm2-2vcw | Updated to 6.17.1 (patched at 6.16.1) in 1.7.3. It is a development-tool dependency, and the affected behavior was not traced into AIRK execution. |

### Later findings

| Dependency | Finding | Disposition (recorded 2026-10-30) |
|---|---|---|
| `minimatch` 10.2.6 | [GHSA-g3hv-75wg-m249](https://github.com/advisories/GHSA-g3hv-75wg-m249): moderate denial of service through glob-pattern amplification, fixed in 11.0.1 | Exception until 2026-12-31, chosen by the maintainer over forcing a semver-major override. The only path is `@vscode/vsce` 4.0.1-3; vsce 4.1.0 (`latest`) and 4.1.1-0 (`next`) still require `^10.2.6`. vsce matches only AIRK's own fixed `.vscodeignore` and `files` patterns, in CI packaging and the OIDC publish job, and is not in the installed extension's runtime. Revisit when a vsce release accepts `minimatch` 11. |

The dependency audit gate first reported this advisory on the 1.7.6 pull request, after the local audit above had passed.

By default the script fetches the advisory database, and cargo-audit then also checks for yanked packages. A `--no-fetch` run uses the cached database and skips the yanked check.

### Publishing credential assessment

`@vscode/vsce` uses `@azure/identity` and MSAL only in the Marketplace publish job of [extension-release.yml](../.github/workflows/extension-release.yml), through `packages/vscode/scripts/publish.cjs`. That job:

- runs in the protected `vscode-marketplace` environment;
- authenticates with GitHub OIDC federation (`azure/login` with `id-token: write`), so no long-lived Marketplace token or client secret is stored;
- publishes only VSIX files already verified by `scripts/verify-extension-release.py`.

The installed extension's runtime dependency set (`jsonc-parser`) does not include these packages. The finding is therefore classified as a publishing-environment exposure that is now closed by the update. This assessment does not show that any credential was misused or that the job was never exposed.
