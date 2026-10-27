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

For every changed file it shows the change kind, whether AIRK has indexed it, and whether the saved bytes still match the indexed bytes. A changed file whose bytes differ from the index is described by the indexed version until you index again.

## Steps

1. Open Review › Changes; it reads Git and AIRK when the screen opens.
2. Choose **Refresh** after saving or staging more changes.
3. Index the project (Management › Projects › Index now) if changed files differ from the index, then refresh.

AIRK keeps no history: Git supplies the changes and AIRK describes the current publication only. At most 50 changed files are shown.
