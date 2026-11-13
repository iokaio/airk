---
id: indexing.window
title: Indexing — Search scope
route: indexing/window
area: explorer
aliases: search scope, active indexes, window selection
related_airk_help: indexing
---
# Search scope

Choose the local indexes and index sets this VS Code window searches. Check the entries you want, then choose Apply selection. Shared indexes are searched once even when included through several sets. Uncheck everything and apply to stop searching indexes in this window.

The search scope is a union: every index in the checked index sets plus every checked index, each read through the index sets that hold it. Applying a set alone searches all of its indexes without checking them. An index in no index set is marked "In no index set"; when you apply it, AIRK first creates a visible index set named after it, then applies your selection. Apply selection is available when the selection differs from the applied one, or when the window reads its folder's index automatically (applying makes it your choice). A scope holds at most 32 index sets and 32 indexes.

**Save as index set** creates a new index set with the name you enter, holding the checked indexes and the indexes of the checked sets. **Connect an agent** opens Management › Agents.

Until you apply or clear a scope, the window reads its folder's index automatically through the index sets that hold it. A scope you apply or clear is kept; the automatic scope never replaces it.

Choices load automatically when the connection is ready. Refresh choices retries a failed read. More indexes and More index sets load additional bounded pages; a partial list does not mean other entries are absent. Saved choices appear as checkboxes; apply them when a fresh search connection is needed. Selection active confirms the applied window scope.

You own these indexes locally. Search selection handles the underlying connection metadata without separate grant or association forms. Coding agents keep the scope they were connected with. Selecting indexes does not create them or start indexing.

Create and delete indexes in Index library. Organize reusable selections in Index sets. Help me choose indexes contains optional recommendations for the current selection; recommendations do not start an evidence query or source scan.

Operation status and recovery retains exact pending requests and their observations. Complete catalog details and exports keeps full native results, including revisions, continuations, errors and receipts. A failed or uncertain apply is reported and reconciled rather than replayed automatically. Workspace trust, runtime ownership and index boundaries remain enforced.
