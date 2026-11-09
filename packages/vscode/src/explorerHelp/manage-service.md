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
- Set **Concurrent requests…** to **4, 8, 16, 32 or 64** (default **16**). The same service-host setting controls MCP evidence admission, independent native channels and each project’s processing allowance. One status/control slot is reserved in addition. Changes apply to subsequent requests; accepted work drains after a decrease.
- Change the resource preset or custom limits. Request capacity is preserved when changing presets.
- Start, stop or force-stop the inference helper.

## Limits

Changes apply on this service host. Running project workers acknowledge the new capacity before it is saved; AIRK frontends pick it up before subsequent requests, and other Management windows show it on their next refresh. Accepted work drains after a reduction. The CLI can read and change the same policy with `airk service rpc` using `resources` and `configure_resources` (`configuration.query_concurrency`).

Health confirms responsiveness only. An external service manager controls lifecycle, upgrades and rollback when one is configured.

After a native Stop, Restart or lifecycle recovery returns a complete response, **Captured native lifecycle response (historical)** retains that exact request and result, including native diagnostic fields. It also records the actual pre-operation health response when one was live and the force argument used. This is a capture of the native coordinator's response, not a fresh process inspection or current evidence authority. A same-bound reconnect can retain it with the historical label; unrelated client, runtime, namespace, trust or controller changes withdraw it.

Use the separate **Copy complete JSON**, **Open complete JSON** or **Export complete JSON** actions for the full capture, including machine details and unknown native fields. Ordinary diagnostic reports remain redacted and only identify the capture's availability. If the complete capture exceeds its budget, the original request remains tracked for native recovery and the UI reports the capture unavailable. It does not convert accepted native work into a no-effect failure.

## Processes on this machine

A user-owned installation uses `airk-service` for the shared service, `airk-index` for project workers and `airk-source` for source watching (`.exe` on Windows). Since 1.8.14, offline inference runs inside the service; installations before 1.8.14 also ran an `airk-embed` helper. Agents and Explorer can each have a separate MCP frontend and launcher. Two VS Code windows do not imply only two AIRK processes. MCP processes follow their client lifetime; background indexing can continue after an editor closes. An idle, connected agent remains connected.

The installer creates the role names from one downloaded engine. Organization-managed read-only payloads and development builds retain their original executable names.

## Upgrading and uninstalling 1.6.4

Supported 1.4, 1.5 and 1.6 runtimes upgrade before connection checks. On Windows and Linux, activation stops verified owned predecessor processes; reconnect agents afterward. macOS uses authenticated graceful drain. Uninstall completes through VS Code after restart and removes owned launchers and recorded AIRK agent entries. Index/service data and original sources remain. Project removal in Storage separately deletes that project's derived index and saved settings.

Service fact rows whose complete value is Not observed are hidden. Diagnostics retain the complete observation; zero, No and none remain visible. Health alone does not confirm live-source freshness or indexing progress.
