---
id: management.storage
title: Storage and cleanup
route: manage/storage
area: management
aliases: storage, quota, unregister, cleanup, delete index, legacy
related_airk_help: storage, maintenance
---
# Storage and cleanup

Open this section from the tabs across the top of Management. Everyday controls remain in the Indexing tab: Search mode, Updates and Evidence detail.

Management › Storage chooses the index storage folder for new registrations, removes projects and shows pending cleanup, and reviews legacy 1.3 startup entries.

## Steps

1. **Choose index storage folder…** reviews a new storage binding; existing indexes stay where they are.
2. Choose **Remove project from AIRK…** and review the confirmation. Removal stops work, drains owned workers and automatically deletes the project's AIRK index, usage ledger and saved settings. It also works when the source folder is gone.
3. **Browse active or removed projects › Removed projects** shows pending removal until cleanup finishes. Cleanup resumes after interruptions without a second deletion approval.

## Limits

Sources and shared models are preserved. Removal deletes saved project settings; only opaque identifiers and removal time remain in native history. Unknown files, links, ownership failures and active leases defer cleanup and report failure. A deleted index must be rebuilt after explicit re-registration.

Installation migrates a registered historical default `data/service-1.4` index root to `data/index-v1`, including the corresponding Linux and preview paths. Published revisions and settings survive this owned directory rename. Custom roots remain in place. Interrupted migration resumes before service startup; occupied destinations and redirected paths are refused. Do not rename or merge index directories manually.

## Storage measurements

Each project shows its registered storage root and collection, accounted index size, reserved bytes, collection quota and quota remaining. The inspection age is shown. **Inspect index & storage** queues a fresh native snapshot; Refresh only reloads cached observations. Sizes are native quota accounting, not a recursive disk scan or the size of original source files.

Open **Capacity and index details** for exact byte counts, storage-root quota remaining, physical growth allowance, replacement estimate and vector counts. Shared-root capacity must not be added across projects. Physical growth allowance respects the native free-space reserve. Replacement estimates include existing accounted size and a fresh SQLite/WAL allowance; they do not reserve space or predict future source growth.

Active and pending-removal lists support bounded project pages. Missing measurements stay not observed; retained measurements can predate removal. **Legacy 1.3 startup review** is collapsed by default and expands only when needed.
