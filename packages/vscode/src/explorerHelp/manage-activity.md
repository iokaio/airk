---
id: management.activity
title: Indexing activity
route: manage/activity
area: management
aliases: jobs, progress, cancel, retry, why is indexing slow
related_airk_help: jobs, job-management
---
# Recent activity and progress

The Activity tab shows recent indexing activity and requests. It shows latest settings/current/queued native requests and bounded project events. Native outcomes survive closing the editor. Acceptance means queued, settings success means applied, and published semantic coverage reports search readiness separately.

The main progress card shows phase, observation age, last advancement at the sample, captured files, newly extracted payloads, publications and eligible total when known. Captured includes reusable work; extracted is not an overall completion fraction. More progress details exposes phase timing, committed/prepared work, semantic units and native observations. No overall percentage or ETA is invented.

Retained samples from a previous worker, service, generation or job are labeled. A stopped worker can leave a saved job marked running; its health banner offers the appropriate restart. Recovery requires a connected worker and real advancement, not just a successful restart request.

Cancel current or queued requests from Recent activity. An unknown transport outcome retains its exact request key; check that saved request before submitting new work. Forgetting a local receipt does not cancel native work.

In 1.5.8, a queued request keeps the old completed sample under Previous job result and counters. The worker/gate rows identify an owning maintenance plan and its pause/phase when observed. An absent wait reason is explicitly unknown, not confirmation that work is progressing. Copy details exports the cached request identities, wait/gate fields and observation times as labeled text.

Overview and Management update automatically while open. Connected service observations run about every two seconds and published/semantic readiness is checked at a bounded cadence even when the publication is unchanged. Native jobs continue when the views close. Manual Check status now is available in Service for troubleshooting.
