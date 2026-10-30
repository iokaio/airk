# 1.7.3 stable release candidate

Date: 2026-10-30. Native workspace, local Cargo lock entries, extension and npm lock entries are synchronized at 1.7.3. Publication requires green PR/exact-main CI and the manual release workflows.

The intended Marketplace channel is **stable** (a release, rather than a pre-release); GitHub publication also selects stable. This record describes unpublished source until those workflows and public-output verification succeed.

The repository review follow-up adds a 1,024-attribute limit per element for structured XML/SVG and Office XML, preserves duplicate-name rejection and MSBuild's existing stricter limit, and preserves literal attribute whitespace during the parser API migration. ZIP 9's fallible member-name enumeration is handled explicitly: enumeration errors fail extraction, legacy CP437 member names remain readable, and duplicate XLSX parts are rejected before workbook reads. Installation/privacy guidance now covers native probes with auto-install disabled, hidden/sensitive sources, retained excerpts, custom-storage permissions and uninstall limits. Existing storage permissions are unchanged by agreement with the user. See the [review response](../reviews/reporeview01-response.md).

## Dependency upgrades

Versions below are the resolved lockfile versions, compared with 1.7.2.

| Dependency | Before | 1.7.3 | Reason |
|---|---|---|---|
| quick-xml | 0.39.4 | 0.41.0 | Patched XML parser advisories, including attribute-iteration CPU exhaustion ([RUSTSEC-2026-0194](https://rustsec.org/advisories/RUSTSEC-2026-0194.html)). Attribute budgets supplement the dependency fix. |
| zip | 4.6.1 | 9.1.3 | Clears the three reported ZIP extraction advisories ([RUSTSEC-2026-0367](https://rustsec.org/advisories/RUSTSEC-2026-0367.html), [RUSTSEC-2026-0368](https://rustsec.org/advisories/RUSTSEC-2026-0368.html), [RUSTSEC-2026-0369](https://rustsec.org/advisories/RUSTSEC-2026-0369.html)). AIRK continues bounded, in-memory Office part reads; this adds no filesystem extraction. |
| tokio | 1.53.2 | 1.53.3 | Patched Unix AsyncFd constructor unsoundness warning ([RUSTSEC-2026-0386](https://rustsec.org/advisories/RUSTSEC-2026-0386.html)). No first-party AsyncFd caller was found. |
| @azure/identity | 4.14.0 | 4.14.1 | Publishing-tool dependency update adopting patched MSAL Node. |
| @azure/msal-node | 6.0.1 | 7.1.2 | Patches authorization-code parameter disclosure ([GHSA-xcx6-jx8p-x3h5](https://github.com/advisories/GHSA-xcx6-jx8p-x3h5)). |
| @azure/msal-common (Node dependency) | 16.14.1 | 16.15.0 | Required by the patched MSAL Node dependency chain. |
| qs | 6.16.0 | 6.17.1 | Patches the reported stringify denial of service ([GHSA-6pq8-jhm2-2vcw](https://github.com/advisories/GHSA-6pq8-jhm2-2vcw)). |

ZIP 9 also adds cfg_aliases 0.2.2, nix 0.31.3 and typed-path 0.13.0 to the Cargo lockfile and removes arbitrary 1.5.0/derive_arbitrary 1.5.0. Existing getrandom/windows-sys versions are reused. ZIP still enables only the selected Deflate feature with default features disabled. The npm updates affect development/publishing tools; the installed extension's jsonc-parser runtime dependency is unchanged. No additional native/extension version bump is allocated for these changes.

cargo-audit 0.22.2 was installed as a local validation tool. The updated lockfile audit exits successfully with zero vulnerability matches and the existing unmaintained paste/ttf-parser warnings; those warnings are not suppressed. The npm audit reports zero findings. These are dated dependency checks, not a guarantee that future advisory databases will remain clean.

## Packaging and publication

The VSIX README retains absolute HTTPS screenshot URLs. Version 1.7.2 bundled the images but rewrote their links to relative paths while VSCE link rewriting was disabled; Marketplace serves the README separately and could not resolve those image paths. Packaging now rejects relative/non-HTTPS image sources in the finished VSIX README.

GitHub release promotes the exact CI native and compiled VSIX artifacts. Full producer authentication occurs at preflight and again before publication; intermediate jobs validate the pinned record and downloaded artifact digests locally. Windows and Mac signing, Mac notarization, native self-test, payload/lock/source checks, final platform-set validation and uploaded-asset digest verification remain. Platform jobs retain small verified source receipts; assembly authenticates its twelve same-run artifact IDs and server digests, avoiding a second download of the original native ZIPs. Signed VSIXs retain CI's extension bytes and replace only native payloads; Linux packages are copied unchanged when the requested channel matches. Channel changes update only VSIX channel metadata and its receipt. Current releases no longer compile TypeScript or fetch Rust dependencies. The narrowly pinned historical 1.4.4 repair keeps its original packaging path.

Read-only GitHub API requests and artifact downloads retry recognized transient HTTP/network failures within four attempts and a 90-second deadline. Authentication failures, missing/changed producers, corrupt payloads and malformed responses remain failures. Publication mutations are never automatically replayed. Marketplace and GitHub releases remain independent manual workflows; merging does not publish either destination.

## Local validation and remaining qualification

Windows local checks passed: workspace formatting and all-target/all-feature Clippy; focused ingest tests (279 passed, seven ignored); extension tests after clean npm install (419 passed, 15 skipped); Python packaging/release contracts (223 tests, five skipped); repository, synchronized-version and whitespace checks. After restoring the per-element attribute ceiling to 1,024, both XML attribute/whitespace regressions passed again. A completed full workspace result is not confirmed in this record. On 2026-10-30, the user reported that the local 1.7.3 installation was working perfectly and requested committing and opening the PR without further local tests. That manual acceptance does not establish full workspace or cross-platform qualification.

A fresh release-profile 1.7.3 native pair and stable Windows x64 VSIX were built and verified with all listed upgrades. The runtime source fingerprint matches the restored 1,024-limit source. The VSIX's version, release-channel metadata, current Cargo lock identity, exact README bytes and absolute screenshot URLs were checked. This is an unsigned local package from a dirty checkout for manual testing, not a publishable CI candidate. Notice generation bundled 366 third-party packages; the existing separate-license-text review warnings for adobe-cmap-parser, pdf-extract and type1-encoding-parser remain.

The fresh native pair passed offline client checks with two projects and sixteen files per project. They verified the catalogs of eighteen native aliases and 28 MCP tools, representative evidence/plan requests, six connection lifecycles, model verification and source-byte preservation. Hosted CI, signed-package execution on other platforms, installed-editor screenshot rendering and publication have not been run for this candidate. Historical 1.7.2 validation does not qualify the 1.7.3 candidate.
