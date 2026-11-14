---
id: management.activity
title: Indexing activity
route: manage/activity
area: management
aliases: jobs, progress, cancel, retry, why is indexing slow, pause, resume
related_airk_help: jobs, job-management
---
# Indexing activity

Activity lists the status and job events this window received from AIRK since it connected (the latest 64), and the service's recent log records (`airkctl service logs`). Refresh activity reads them again. Each keeps its complete native result under Complete response or Complete events.

The everyday actions on the selected index are on the Overview under **Index actions**: Index now, Pause or Resume, Turn watching on or off, and Retry or Cancel for the current, queued or failed job. Each runs one `airkctl` command through this window's session and shows the native result, refused or not. Live progress updates by itself while work runs and the Explorer is visible.

The complete job history, plans and recovery are in AIRK Admin, Activity: choose Full activity in AIRK Admin.
