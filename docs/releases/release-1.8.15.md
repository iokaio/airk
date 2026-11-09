# AIRK 1.8.15 local upgrade correction

The user requested this patch on 8 November 2026 after the 1.8.11 → 1.8.14
upgrade failed with `service incarnation selection is busy` at a collection's
`manifest.lock`. The native workspace, lockfiles, extension and generated template
catalog now identify 1.8.15 so VS Code can install a distinct candidate.

The legacy upgrade preflight previously captured full collection publications
while the predecessor was still running. That observation could contend with a
legitimate manifest owner before the installer reached shutdown. Preflight now
uses an opaque registry-only observation for capacity and accepted-work engine
compatibility. It cannot serve as a complete checkpoint. Full publication capture,
compatibility checks and checkpoint verification remain mandatory after verified
predecessor exit under native exclusion. No lock is removed and no retry budget
or capacity limit is increased.

This candidate also carries the completed-history checkpoint correction recorded
in the [1.8.14 working record](release-1.8.14.md). Earlier packages and receipts
retain their original versions and hashes.

Validation: 54 selected native checkpoint/maintenance regressions passed; the
full mocked extension suite passed 1,096 tests with 21 skips. The 38 selected
Python contract tests and repository checks passed. Formatting and strict workspace Clippy passed. Windows debug r30 runtime, native
package and VSIX verification passed with unchanged frozen inputs. Installed upgrade,
full workspace and cross-platform qualification remain pending. No publication
is authorized by this patch.


Installable local candidate: `artifacts/local/one-airk-1.8.14/windows-debug-vsix-r30/publish/airk-1.8.15-win32-x64.vsix`
(86,416,985 bytes), SHA-256 `9f2968c5eefe50d7d904ff869c1982e4f63fa8727f766b2b85d3297be1f980e6`. This is an unsigned debug build.
Receipts, Cargo timing evidence and the separate native, extension, harness and
fixture identities are retained in the
[implementation ledger](../research/one-airk-implementation.md#1815-local-retry-candidate-live-legacy-preflight-lock-8-november-2026).
