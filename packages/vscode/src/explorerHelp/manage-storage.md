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

Management › Storage chooses the index storage folder for new registrations, lists unregistered projects for reviewed cleanup, and reviews legacy 1.3 startup entries.

## Steps

1. **Choose index storage folder…** reviews a new storage binding; existing indexes stay where they are.
2. Remove an active project with **Remove project from AIRK…** in Storage and removal first, even if its source folder is gone. Choose **Browse active or removed projects › Removed projects**, then **Preview / resume index cleanup** for that project.
3. **Delete reviewed index state** becomes available only after a successful preview, and asks again before deleting.

## Limits

Sources and saved settings are always preserved. Unknown files and links are refused. A deleted index must be rebuilt.

## Storage measurements

Each project shows its registered storage root and collection, accounted index size, reserved bytes, collection quota and quota remaining. The inspection age is shown. **Inspect index & storage** queues a fresh native snapshot; Refresh only reloads cached observations. Sizes are native quota accounting, not a recursive disk scan or the size of original source files.

Open **Capacity and index details** for exact byte counts, storage-root quota remaining, physical growth allowance, replacement estimate and vector counts. Shared-root capacity must not be added across projects. Physical growth allowance respects the native free-space reserve. Replacement estimates include existing accounted size and a fresh SQLite/WAL allowance; they do not reserve space or predict future source growth.

Registered and unregistered lists support the existing bounded project pages. Missing measurements stay not observed. Retained measurements for unregistered projects may predate removal; cleanup still needs its exact preview and confirmation. **Legacy 1.3 startup review** is collapsed by default and expands only when needed.
