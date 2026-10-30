# 1.7.2 local development candidate

Date: 2026-10-30. Native workspace, local Cargo lock entries, VSIX and npm lock entries are synchronized at 1.7.2 for local upgrade testing. This candidate extends [1.7.1](release-1.7.1.md); no publication is claimed.

Explorer now has a **Workbench** button immediately left of Help in its top toolbar. The label stays fixed; its pressed state and tooltip follow actual Workbench visibility. It uses the existing Show/Hide behavior. The former sidebar link above Management is removed.

The project dropdown moves from the top toolbar into the Overview and Management panels. Both controls use the existing shared selection and retain workspace eligibility and evidence-session isolation. Other Explorer screens use that selection.

Local Windows validation passed: 1,466 native workspace tests with 17 ignored; 419 extension tests with 15 skipped; 204 script tests with five skipped. Formatting, Clippy, repository, synchronized-version and whitespace checks passed. The rebuilt release runtime passed embedded-model verification, real offline inference/DiskANN checks and a two-project native-client smoke with source preservation. Native ZIP and VSIX allowlisted payload/hash verification passed. An explicit scoped review pass retained project eligibility, host visibility handling and evidence-session isolation.

The unsigned local VSIX is `artifacts/workbench-review-1.7.2/installer-final/publish/airk-1.7.2-win32-x64.vsix` (69,547,112 bytes). SHA-256: `dfbfff18d9c3c599130d9aebf69de643be35dda12d017e1060a523d6c7d2a118`. The adjacent manifest/checksums and `artifacts/workbench-review-1.7.2/review-evidence.json` retain package evidence.

Browser previews timed out at CDP `Page.enable` with both Edge and Chrome; no visual-preview success is claimed. The listing identifies the retained Explorer image as a 1.7.1 preview. At the local packaging checkpoint, installed-editor acceptance, other platforms, hosted CI and publication had not run. Three existing separate-license-text review items remain: `adobe-cmap-parser`, `pdf-extract` and `type1-encoding-parser`.

## Installed candidate acceptance and release preparation

On 2026-10-30, the maintainer reported that the 1.7.2 installation worked perfectly and authorized commit, push, PR creation, CI monitoring, publication and release after green checks. This is manual acceptance of the local Windows candidate; it does not establish cross-platform qualification. The existing 1.7.2 version increment is retained for this PR. Publication remains gated on successful PR and exact-main CI and verified release workflows.

Pre-PR Windows checks passed the full all-feature workspace run (1,466 passed, zero failed, 17 ignored, including doctests), formatting, all-target/all-feature Clippy, extension compilation/tests (419 passed, 15 skipped), script contracts (204 passed, five skipped), five classifier-training contracts, repository checks, version synchronization, license-notice generation and whitespace validation. The fresh release pair passed all 15 checks in `scripts/ci-local.ps1`: five iterations each of managed MCP smoke, real-model/two-project native-client checks and installed-runtime staging, upgrade, rollback, launcher repair, client lifetime and uninstall, restricted to four logical processors with no retries. Local evidence is retained under ignored `artifacts/ci-local/20261030-093928-60ecc435/`. The complete staged diff and validation evidence received an explicit primary-agent review without delegation; no independent review is claimed.
