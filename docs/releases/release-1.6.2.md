# AIRK 1.6.2 project removal and Explorer fixes

Historical local candidate record. This candidate was not published; its changes are consolidated in the [1.6.4 public release](release-1.6.4.md). Validation below retains its original version and scope.

Native and extension versions and their lockfiles are synchronized at **1.6.2**. This is an unpublished local candidate; it has not been merged or published.

## Removal owns complete cleanup

Removing a project stops new indexing, cancels queued/running jobs, disables monitoring and drains its owned workers. A supervised child then deletes the ownership-matched collection, including semantic/text indexes and saved project settings. The registration, project configuration, progress, source-watch metadata and detailed job receipts are removed after files are gone. Only opaque project/source identifiers and removal timestamp remain in native removal history. Source files are untouched. Shared model assets and other projects remain independent.

The durable removal barrier prevents re-registration during deletion. Bounded cleanup resumes after service interruption. Unknown files, aliases, ownership failures and active request leases defer cleanup and report failure rather than authorizing deletion. A pending removal stays visible until cleanup finishes. Previously removed registrations enter this cleanup path on the format-9-to-10 registry migration; read-only saved metadata never migrates state. The index format and storage locations remain unchanged. Older runtimes cannot open registry schema 10.

A deleted source folder is confirmed by two metadata probes at least ten seconds apart while its immediate parent is accessible at its canonical location. The checks run outside the broker in the supervised source child, include paused/manual projects, and never open source content. Missing ancestors/drives, permission errors, replacement files and redirected paths remain unavailable rather than being treated as deletion. Checks run while the service is active; ten seconds is a minimum between confirmations, not a guaranteed cleanup deadline. Removed registrations are no longer source-probed.

## Automatic indexing and project selection

Trusted open folders register with initial indexing and monitoring automatically, including after upgrades. Explicit automatic-setup opt-out, service Stop, project removal, pause and manual-update settings remain authoritative. Missing workspace folders do not block preparation of other open folders. Explicit Add Project can register a previously removed project again after cleanup.

Read-only request receipts identify removed projects so their saved requests and editor launch details can be withdrawn without replaying indexing. Missing/expired receipts alone still preserve uncertainty.

Management's Removed projects filter no longer empties Explorer's active project choices or causes Overview to report a missing registration while showing retained published status. Removed projects no longer display the index-content panel or offer a separate cleanup approval.

## Validation

Local Windows validation on October 29, 2026:

- Workspace unit/integration test binaries passed. The initial doctest phase encountered stale dependency artifacts (`E0463`) after follow-up rebuilds; a separate serial all-feature workspace doctest run passed.
- Final removal regressions passed, including 4,097 objects across an interrupted/restarted cleanup, source preservation, unknown-file/hard-link refusal, read-gate deferral, stale-registration fencing and opt-out preservation across collection names.
- Native CLI library tests passed (174 passed, one ignored); configuration/removal process tests passed (seven cases). The final source/heartbeat-focused suite passed (11 cases).
- Extension tests passed: 398 passed, 14 environment-dependent cases skipped. This includes upgrade-time automatic registration, missing-folder isolation, active/removed project selection, matching removal receipts and editor launch-receipt isolation.
- All-feature workspace Clippy with warnings denied, formatting, repository and whitespace checks passed. Native/extension/lockfile versions agree at 1.6.2.
- Python contracts passed: 186 tests, four environment-dependent skips. Notices were bundled for 365 dependencies; the existing separate-license-text review notices for adobe-cmap-parser, pdf-extract and type1-encoding-parser remain release-review items.
- The final optimized runtime pair passed source/payload checks and native archive verification. Real-model native client checks passed with original fixture sources preserved. VSIX archive validation is recorded with the local candidate artifacts.

A read-only live diagnostics review observed the installed 1.6.1 repo index available, all 1,191 eligible files semantically ready, no queued/active work at the sampled times, native watching enabled and caught up, and no current reported failure. A follow-up read resolved the intentional cold `process_identity_pending` observation and confirmed matching frontend/worker/service versions, payloads and builds. The source watermark advanced automatically between observations. These are observations of the configured live repo, not proof about removed projects or the 1.6.2 installation.

The combined change received an explicit local review pass; it was not independently reviewed. Installed-editor upgrade testing and macOS/Linux qualification remain unperformed.
