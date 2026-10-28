---
id: management.repair
title: Diagnose and repair
route: manage/repair
area: management
aliases: repair, verify, rebuild, recover, reconcile, diagnose
related_airk_help: maintenance, request-recovery
---
# Diagnose and repair

Open this section from the tabs across the top of Management. Everyday controls remain in the Indexing tab: Search mode, Updates and Evidence detail.

Management › Advanced diagnostics and recovery creates a native repair plan, shows its preview page by page, and runs it only after you approve that exact preview.

## Steps

1. Choose an operation (automatic, verify, recover, reconcile, regenerate vectors or full rebuild) and a scope (one project, the projects selected in Projects, or all registered projects).
2. Choose **Create repair preview**.
3. Read every page with **More plan projects**. Approval stays disabled until all pages are read and the plan is ready.
4. Choose **Approve this exact preview**, optionally approving an explicitly identified fallback.

## Requests awaiting confirmation

A request whose outcome is unknown keeps its original key. **Check / retry exact request** sends that same request again; forgetting it does not cancel native work.

In 1.5.8, the preview shows a plain-language project/action summary and Start repair above its technical details. An explicit fallback checkbox appears only when that plan requires one and resets for a new plan or digest. Approved submission gets an accepted/queued/running/finished distinction. Plan rows update automatically while work runs. This advanced reviewed workflow is separate from the native service's routine automatic recovery.
