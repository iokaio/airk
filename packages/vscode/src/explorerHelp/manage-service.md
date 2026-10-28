---
id: management.service
title: Service and runtime
route: manage/service
area: management
aliases: start, stop, restart, upgrade, rollback, embedding, resources
related_airk_help: service, upgrades, resources
---
# Service and runtime

Open this section from the tabs across the top of Management. Everyday controls remain in the Indexing tab: Search mode, Updates and Evidence detail.

Management › Service details shows the native service's health, runtime selection, resource admission and the local inference helper.

## Actions

- Connect, start, restart or stop the service. Stop and restart ask first and name the instance.
- Review a bundled runtime upgrade, the previous runtime, a payload repair or a saved activation. Each review shows the exact selection before anything changes.
- Change the resource preset or custom limits.
- Start, stop or force-stop the inference helper.

## Limits

Health confirms responsiveness only. An external service manager controls lifecycle, upgrades and rollback when one is configured.

## Processes on this machine

A user-owned installation uses `airk-service` for the shared service, `airk-index` for project workers, `airk-source` for source watching and `airk-embed` for inference (`.exe` on Windows). Agents and Explorer can each have a separate MCP frontend and launcher. Two VS Code windows do not imply only two AIRK processes. MCP processes follow their client lifetime; background indexing can continue after an editor closes. An idle, connected agent remains connected.

The installer creates the role names from one downloaded engine. Organization-managed read-only payloads and development builds retain their original executable names.
