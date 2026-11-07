---
id: indexing.library
title: Indexing — Index library
route: indexing/library
area: explorer
aliases: indexes, sources, catalog
related_airk_help: indexing
---
# Index library

Manage the indexes stored on this machine. Each index contains searchable data extracted from one or more source folders. Open index to manage sources, watching and settings. Search scope chooses which existing indexes this window searches; Index sets organizes reusable selections.

The library loads a bounded metadata page automatically. Refresh indexes refreshes or retries a failed read. Next page reads more entries, and the filter applies only to the displayed page. Opening this screen never creates an index or starts indexing.

Expand Set up a new index, enter its source folders, new storage folder and collection name, choose watching and select Review new index. Review the exact locations, then apply. Existing source folders can overlap another index; storage belongs to one independent index. When replacing a deleted index, choose a new storage folder and collection name. You do not need to split an old index to build a different one covering some of the same files.

Delete index is available directly on each active library row and in its opened details. Review the index name and effects, then choose Delete index and stop maintenance. This stops updates and searching, removes the index from index sets and defaults, and schedules cleanup of its owned derived data. Original source files are preserved. Physical cleanup may remain pending after logical deletion; consult the recorded result and Management storage status. Deletion is not secure erasure and cannot recall exported evidence or backups.

Split index remains a separate option for moving selected source coverage into an independent destination while retaining the original remainder. It is optional when your intent is simply to delete and recreate an index.

Source changes and permanent exclusions retain the existing exact native review. Unconfirmed operations remain in Operation status and recovery. Complete catalog details and exports keeps original responses, revisions, continuations, receipts and future fields. Choosing a new source or opening a library entry does not silently widen the current search scope.
