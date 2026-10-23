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
