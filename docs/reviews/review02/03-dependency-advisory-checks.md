# Add an explicit dependency advisory policy and automated checks

AIRK locks dependency versions and verifies downloaded bytes, but the inspected CI does not check those versions against vulnerability advisories. A reproducible build can therefore continue passing while its lockfiles contain newly disclosed vulnerabilities.

## Current behavior and use case

Reviewed source: `c62c1e6936500801dbf41dffdd6e21453e48335d`, on 2026-10-30. [CI](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/.github/workflows/ci.yml#L130) installs npm dependencies with `--ignore-scripts --no-audit`; no RustSec/npm advisory gate or dependency-update bot configuration was found. Disabling install scripts is a useful control and should be preserved. Separate read-only Rust and npm lockfile audits both returned findings during the review.

This is a proposed maintenance control. It does not imply that every advisory is reachable or that an audit can prove dependency safety. Existing findings need individual disposition; adding a workflow alone does not resolve them.

## Proposed acceptance criteria

- **AC1:** Automated checks cover the resolved Cargo and npm lockfiles on dependency changes and periodically when the lockfiles have not changed, with advisory database identity/time retained in results.
- **AC2:** Maintainers define which findings block changes/releases, how build/publishing dependencies and maintenance warnings are handled, and who reviews exceptions. The policy is explicit rather than an accidental tool default.
- **AC3:** Exceptions identify package, advisory, evidence, scope and review/expiry conditions. A transitive or development dependency is not automatically exempt.
- **AC4:** Checks preserve script-free npm installation and do not run project/dependency code merely to inspect lockfiles. Scanner/network failures are distinguishable from a clean result.
- **AC5:** A controlled workflow test demonstrates detection of a known finding, application of a scoped exception, and failure when that exception no longer applies. Routine CI remains lean.
