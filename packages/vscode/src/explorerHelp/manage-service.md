---
id: management.service
title: Service and runtime
route: manage/service
area: management
aliases: start, stop, restart, upgrade, rollback, embedding, resources
related_airk_help: service, upgrades, resources
---
# Service and runtime

Stopping the AIRK Service and reviewing its runtime are in AIRK Admin, Service. **Stop for restart** records a restart request and stops the service; the management page does not start a replacement. After it stops, run `airkctl service start` locally or use AIRK: Start AIRK Service in VS Code. Then run `airkctl admin open` for a new browser session or reopen management in the editor after verifying the service connection.

Advanced management uses AIRK Admin. Explorer also keeps Agents, Activity, Usage and evidence reads inside this window; choose the corresponding main destination for those native service results.

Choose Open AIRK Admin to open the admin pages. Choose where... picks In VS Code (the Simple Browser, with a single-use sign-in link kept only in memory) or In my browser; the choice is remembered in the airk.admin.openIn setting. If the VS Code browser reports a certificate problem, choose Open in my browser.
