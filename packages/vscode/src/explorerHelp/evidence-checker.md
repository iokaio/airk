---
id: explorer.evidence.checker
title: Check citations in a report
route: evidence/checker
area: explorer
aliases: validate agent report, check plan citations, verify pr description
related_airk_help: limits
---
# Check citations in a report

Paste an agent report, a plan or a PR description. The checker validates each `path:line`, `path:start-end` or `path#Lx-Ly` citation against the published index and the saved files.

## Results

valid, changed since index, not saved, missing, not found, line out of range, or outside the collection (absolute and parent paths are never sent to AIRK).

## Limits

Text with no supported citation is reported as unsupported, never as a pass. One check examines at most 50 citations and 25 distinct paths.
