---
id: indexing.window
title: Indexes — Search scope
route: indexing/window
area: explorer
aliases: search scope, active indexes, window selection, description, edit description
related_airk_help: indexing
---
# Search scope

Choose the local indexes and index sets this VS Code window searches. Check the entries you want, then choose Apply selection. Shared indexes are searched once even when included through several sets. Uncheck everything and apply to stop searching indexes in this window.

The search scope is a union: every index in the checked index sets plus every checked index, each read through the index sets that hold it. Applying a set alone searches all of its indexes without checking them. An index in no index set is marked "In no index set"; when you apply it, AIRK first creates a visible index set named after it, then applies your selection. Apply selection is available when the selection differs from the applied one, or when the window reads its folder's index automatically (applying makes it your choice). A scope holds at most 32 index sets and 32 indexes.

When the selection covers more indexes than this machine searches at once (the service's configured concurrent evidence requests, `query_concurrency`, as the last status read reported it), a note near Apply says how much longer searches take, for example "This scope searches 8 indexes; this machine searches 4 at a time, so searches take about twice as long." It is only information: Apply and Save as index set still work. While index-set memberships are not completely read the count is a lower bound ("at least"); without a reported concurrency no note is shown.

**Save as index set** creates a new index set with the name you enter, holding the checked indexes and the indexes of the checked sets; when that set would hold more indexes than this machine searches at once, a note under the button says so. **Connect an agent** opens Agents.

Each index and index set shows its description under its name. **Edit description** opens a text box for it (at most 1 KiB; line breaks are allowed): **Save** stores it (`airkctl library describe … --yes`) and **Clear** removes it; the choices are then read again. Descriptions help coding agents that may choose indexes themselves (Agents, "Let the agent choose indexes") pick the right ones. Saving a description changes the index library's revision, so the window's scope is selected again at the new revision.

Until you apply or clear a scope, the window reads its folder's index automatically through the index sets that hold it. A scope you apply or clear is kept; the automatic scope never replaces it.

Choices load automatically when the connection is ready. Refresh choices retries a failed read. More indexes and More index sets load additional bounded pages; a partial list does not mean other entries are absent. Saved choices appear as checkboxes; apply them when a fresh search connection is needed. Selection active confirms the applied window scope.

You own these indexes locally. Search selection handles the underlying connection metadata without separate grant or association forms. Coding agents connected in Agents follow the scope you apply here (their default scope changes on their next call) unless following is turned off for them there; agents connected in AIRK Admin for a reviewed scope keep that scope. Selecting indexes does not create them or start indexing.

Create and delete indexes in Index library. Organize reusable selections in Index sets. Help me choose indexes contains optional recommendations for the current selection; recommendations do not start an evidence query or source scan.

Operation status and recovery retains exact pending requests and their observations. Complete catalog details and exports keeps full native results, including revisions, continuations, errors and receipts. A failed or uncertain apply is reported and reconciled rather than replayed automatically. Workspace trust, runtime ownership and index boundaries remain enforced.
