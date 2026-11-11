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

Since 1.8.22, one AIRK process stays resident per installation: the service. Project indexing, cleanup, source watching and offline inference run inside it. Each VS Code window adds one `airk service session`, which exits with the window. Commands you type, installer steps, at most one short-lived PDF parser and a tools shell you open are the only other AIRK processes. Agents connect over HTTPS and start no AIRK process, apart from a credential helper that exits at once. If one project's work cannot stop, that project is quarantined and shows a stuck-owner status while every other project keeps running; restart the service to recover it. Stable, preview and custom installations each have their own service. Background indexing continues after an editor closes. An idle, connected agent remains connected.

Earlier releases ran separate `airk-index`, `airk-source` and launcher processes. Organization-managed read-only payloads and development builds retain their original executable names.

## Upgrading and uninstalling

Until 1.9.0, installing over an earlier AIRK replaces it: AIRK proves the old folders are its own, removes its sign-in startup and owned trust, stops its processes and installs fresh. Source folders are never touched, and no index is created until you add one. Agent entries that the old installation used are recorded first; Management > Agents then offers Reconnect. Uninstall completes through VS Code after restart and removes owned launchers and recorded AIRK agent entries. Index/service data and original sources remain. Project removal in Storage separately deletes that project's derived index and saved settings.

Service fact rows whose complete value is Not observed are hidden. Diagnostics retain the complete observation; zero, No and none remain visible. Health alone does not confirm live-source freshness or indexing progress.
