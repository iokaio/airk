---
id: explorer.review.changes
title: Review changed files
route: review/changes
area: explorer
aliases: review, changed files, git diff, git changes
related_airk_help: workflows
---
# Review changed files

Review › Changes lists the selected project's saved changes from the editor's Git extension (staged, unstaged, untracked and merge changes against HEAD) and joins each file with its indexed record.

For every changed file it shows the change kind, whether AIRK has indexed it, and a dated native observation of whether saved bytes matched the indexed bytes. The native owner checks current source policy before source capture; the extension observes unsaved editor state without reading source bytes. A changed file whose bytes differ from the index is described by the indexed version until you index again. A past match does not establish continued freshness or authorize an edit.

Complete changed-file JSON retains each native response. Verification shares a total 512 MiB source-byte budget, an 8 MiB response budget and the action's deadline across files and any bounded restart. Missing accounting or exhausted source-byte budgets leave later checks explicitly unknown.

## Steps

1. Open Review › Changes; it reads Git and AIRK when the screen opens.
2. Choose **Refresh** after saving or staging more changes.
3. Index the project (AIRK Admin › Index now) if changed files differ from the index, then refresh.

AIRK keeps no history: Git supplies the changes and AIRK describes the current publication only. Each page shows at most 50 changed files, with **Previous**, **Next**, a total count and page position. There is no 50-file total cutoff. Continuation checks the Git change-list identity and publication; if either changes, choose **Refresh** to start a new report rather than mixing pages from different observations.

A failed page keeps the last successful page labeled and disables paging until Refresh restarts the list. Page changes are announced; the Changes table can be focused and scrolled with the keyboard. Disconnection hides prior reports.
