---
id: management.activity
title: Indexing activity
route: manage/activity
area: management
aliases: jobs, progress, cancel, retry, why is indexing slow
related_airk_help: jobs, job-management
---
# Indexing activity

Management › Activity shows each project's current job, queued job, phase and counters, and the native wait reason sampled for that job. Recent transitions and process transitions are listed below.

## Steps

1. Read the wait reason and the age of the last real advance.
2. Use **Retry job** for a failed or canceled job, or **Cancel job** / **Cancel queued job** for running or queued work.

## Limits

There is no ETA. Ages are real advancement observations. A missing wait observation does not prove there is no wait.
