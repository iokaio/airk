---
id: explorer.overview
title: Overview and status
route: overview
area: explorer
aliases: status, readiness, is airk ready, status pill, health
related_airk_help: index-health, freshness, service
---
# Overview and status

The Overview shows the selected project and its published index. The status pill in the header and the status bar show one word for the worst current condition.

## What the rows mean

- **Service connection**: whether this window reaches the AIRK service (ready, degraded, disconnected, incompatible or untrusted).
- **Published index**: whether a readable publication exists for the selected project, with its publication token.
- **Source work**: the native scheduler state (idle, pending, running, paused, blocked, failed). It is not proof that saved files match the index.
- **Semantic coverage**: vector readiness (disabled, pending, partial, ready, failed), reported separately from lexical search.

## Steps

1. Pick a project in the header if more than one is registered.
2. Choose **Refresh status** to read published status again.
3. Use a journey card to go to Search, Code, Review, Strategies or Management.
4. Choose a Developer, Tester, Architect or Writer lens. It only reorders journeys. **Role workflow help**, **Choose indexing by purpose** and **Document type guidance** explain which evidence and indexing choices suit the task.

Every workspace offers **Help for this screen**. The header's **Help** searches bundled Explorer and engine guidance offline; **About** opens project background, public links and the Ioka Discord community invitation.

## Limits

Ready means a publication exists. It does not prove that the saved files still match the index; use Evidence › Inspector for one file.
