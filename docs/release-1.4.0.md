# AIRK 1.4.0 development release record

This is a draft for the approved 1.4.0 clean break, updated October 23, 2026. Native, extension and lockfile versions remain 1.3.2 during opt-in implementation. It is not an available release or a qualification receipt. The [complete plan](airkservice-architecture.md) and [completion checklist](service-implementation-checklist.md) track remaining work.

Implemented native development paths include authenticated service brokerage, direct project retrieval, durable fenced work, shared inference, service-owned source watching, bounded publication and replacement storage, native user defaults, immutable offline installation, resumable launcher repair and separately bound writable state. Approved organization-managed read-only payloads now support external supervision or user-on-demand startup without copying provider executables. Detachment preserves provider files and durable state; externally supervised processes remain running.

The default extension/CLI/container cutover, service console, bulk repair, remaining lifecycle/resource integration and final qualification are incomplete. Token-efficiency changes, including terse defaults, Full selection, compact facts and retained change baselines, remain Phase 7 requirements until their complete implementation and tests land. Development evidence is recorded in the affected operating guides. Historical 1.3.x release records continue to describe those versions.

Interrupted first installations can now be removed before activation, with exclusive owner checks. Retirement validates ancillary receipts before deletion and completes cleanup after the payload directory is already gone, preserving changed or unknown state.

Registered projects with missing source roots can serve existing ownership-matched retained evidence. New scans and rebuilds fail explicitly without publishing root-wide deletions; status reports source availability independently of index identity and readiness.

Source admission now rejects overlaps with service control, verified installer state/install directories and all registered project data roots. Transactional checks cover additional-root changes and registration races; native indexing rechecks pending configuration before capture.

Installed payloads now require complete inventoried topology before selection/execution. Unknown neighboring files block startup and are preserved during resumable retirement.

Native activation now records resumable drain/launcher/probe/restoration intent, keeps public selection behind exact candidate and registry readiness, preserves OS restart ownership and supports explicit pre-commit abort. Windows on-demand and non-elevated Task Scheduler crash fixtures passed explicit abort, exact candidate recovery and owned cleanup; native LaunchAgent proof and broader installed-target qualification remain open.

Windows worker and extraction cleanup now share bounded native Job Object ownership and confirm descendant exit before returning capacity. Native manager commands also apply their deadline to input and output completion and reject oversized diagnostic output. Unix descendant containment remains an implementation gate; the cross-platform command and existing extraction regressions continue to run separately.

Stage commits remain local and human DCO-signed. No push, publication or service VSIX is authorized before the user's implementation, test and review gate is satisfied. Final versions and packaging receipts will be recorded here at cutover.

The native management path now keeps a bounded transactional event log with restart-stable snapshot paging, explicit retention gaps and content-free failure codes. Explicit retry preserves the failed/canceled receipt and resets the applicable finite automatic policy only once per accepted key. The trusted console still needs to present these operations.

Native project inspection now records bounded cached revision/settings observations with worker fencing and independent root availability. Explicit inspection can use an idle paused project while its source work remains paused. Native guarded plan execution is described below; service-console presentation and the complete qualification matrix remain open.

Native maintenance now provides durable one/selected/all-project previews, engine/revision/settings guards, explicit fallback approval, private fenced execution, pause/cancel, bounded retry and independent partial-failure receipts. Storage estimates remain observations; actual writes use durable quota admission. Extension-console presentation and the complete qualification matrix remain open. See [maintenance contracts](service-maintenance.md).

Native process diagnostics now use a bounded owned ring, content-free records, explicit loss/error telemetry and JSON/readable cached export without a live service or project access. Logging failures preserve foreign state and keep health available. Native global log-location binding and console presentation remain cutover work. See [logging contracts](service-logging.md).

Format-140 SQLite connections now retain admitted WAL/SHM filenames across final close, while checkpointing and enforcing the existing disk and alias limits. Windows/Linux identity, recovery and native process regressions pass. A deterministic Windows test reproduces zero-link validation handles under the former unlink behavior; it does not establish the sole cause of every historical intermittent failure. See [publication evidence](service-publication.md).
