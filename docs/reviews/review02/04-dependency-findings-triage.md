# Resolve or document outstanding Rust and publishing-tool dependency findings

The reviewed lockfiles contain advisory matches and unmaintained dependencies without a corresponding disposition found in the reviewed CI policy. The publishing toolchain deserves review as well as the installed runtime because it operates in release environments.

## Version and audit evidence

Source: `c62c1e6936500801dbf41dffdd6e21453e48335d`, 1.7.2 candidate, audited 2026-10-30. The read-only Cargo audit checked 373 packages against RustSec database commit `2295466c4e9df26647fd67d8fa1c705ac2fc7f81`; yanked-package checking was not performed. npm audit used copied manifests with `--package-lock-only --ignore-scripts`. Neither audit installed AIRK or executed repository code. These are dated audit observations; rerun before choosing updates.

| Dependency | Finding | Reachability established by this review |
|---|---|---|
| `quick-xml` 0.39.4 | RUSTSEC-2026-0194 and RUSTSEC-2026-0195 | Attribute-processing issue tracked separately; no first-party `NsReader` use found for 0195 |
| `zip` 4.6.1 | RUSTSEC-2026-0367, 0368 and 0369 | Inspected Office code reads archive members into memory; affected filesystem extraction APIs were not found in those calls |
| `tokio` 1.53.2 | [RUSTSEC-2026-0386](https://rustsec.org/advisories/RUSTSEC-2026-0386.html), `AsyncFd` unsoundness warning | No first-party `AsyncFd` caller found; transitive reachability not fully established |
| `paste` 1.0.15; `ttf-parser` 0.25.1 | RUSTSEC-2024-0436; RUSTSEC-2026-0192, unmaintained | Maintenance warnings, not demonstrated AIRK exploits |
| `@azure/identity` 4.14.0; `@azure/msal-node` 6.0.1 | [GHSA-xcx6-jx8p-x3h5](https://github.com/advisories/GHSA-xcx6-jx8p-x3h5) | Two high package entries for one underlying advisory, in the development/publishing dependency graph |
| `qs` 6.16.0 | [GHSA-6pq8-jhm2-2vcw](https://github.com/advisories/GHSA-6pq8-jhm2-2vcw) | Moderate development-tool finding; affected behavior not traced into AIRK execution |

The npm findings are not established vulnerabilities in the shipped extension runtime. Source anchors are the [Rust lockfile](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/Cargo.lock), [npm lockfile](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/package-lock.json), and [packaging allowlist](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/scripts/package-extension.py).

## Proposed acceptance criteria

- **AC1:** Every listed finding receives a dated disposition: compatible update/removal, a scoped and reviewable exception with call-path evidence, or a separately tracked unresolved action.
- **AC2:** The npm authentication finding is assessed against the actual publishing commands and credential use, without assuming either compromise or irrelevance from its development-only classification.
- **AC3:** Dependency changes pass affected parser/runtime/package tests and preserve lockfile consistency and license notices.
- **AC4:** Fresh audit output records remaining findings and their dispositions. Unmaintained warnings and unavailable checks are not silently counted as passed.
