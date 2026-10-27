---
id: management.projects
title: Registered projects and Index now
route: manage/projects
area: management
aliases: index now, reindex, registered projects, collections, remove project, unregister, deleted source, root_unavailable, unreadable directories, watcher recovery
related_airk_help: indexing, jobs, job-management
---
# Registered projects and Index now

Management › Projects lists the projects registered with the AIRK service. Values come from the service's cached project page; the Overview shows the selected project's live published status.

## Steps

1. Choose **Select** to make a project the Explorer's current project.
2. Choose **Index now** to queue a native index job.
3. Choose **Remove project…** to stop indexing a registered project, including one whose source folder has been deleted. AIRK inspects retained settings and asks before unregistering it.
4. After removal, choose **Preview / resume index cleanup**, then **Delete reviewed index state** to reclaim the retained index. These controls also appear in Management › Storage. Sources and saved settings are preserved.
5. Expand **Unreadable directories** to see observed watcher paths and error categories. The list also appears in Management › Indexing. It is bounded and may be incomplete: each root walk stops at its first failure. Retries run about every ten seconds; entries clear after their root can be fully traversed. The log records unavailable/recovered transitions without repeating each failed retry, including across service restarts. Restored access does not mean indexing has finished.

## Limits

Accepting a job means the work is queued; it does not mean indexing is finished. The request is saved before it is sent, so an uncertain outcome can be checked again without creating a second job.
