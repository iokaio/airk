---
id: management.overview
title: Manage project indexing
route: manage/overview
area: management
aliases: management, setup, search mode, pause indexing, update index, semantic disabled
related_airk_help: indexing, jobs, models
---
# Manage project indexing

Management uses the same horizontal tabs as the other Explorer areas. Indexing is the default tab. **In your index** shows published files, text chunks, vectors and semantic file coverage. **Working now** shows current source, semantic or cleanup work separately. Errors and recovery actions remain prominent. The main action follows its state: update, pause, resume, restart a blocked worker or retry a failed job.

Semantic jobs work from already indexed content: their useful counters are completed semantic units, committed files and published batches. Cleanup highlights Clean up and keeps the published totals visible. **This attempt** is the current job's elapsed time, not total initial indexing time. The coverage bar measures published semantic files, not an ETA. Both views update automatically. Copy details includes publication and attempt observations for diagnosis.

Search mode selects Text only or Text + semantic. Enabling semantic prepares the offline model, preserves an existing model selection and schedules source publication followed by semantic work. Disabled published coverage is not proof the saved setting is off. Settings can be queued; Recent activity shows their native outcomes.

Updates selects Automatic or Manual. Pause is temporary and preserves that preference. Evidence detail changes supported source structure independently of semantic search. Search mode and Evidence detail are accepted durably before native inspection; preparation and indexing continue with the editor closed. Advanced operations retain their separate preparation guards.

Choose Activity for requests and progress, Storage for storage and removal, Recovery for advanced diagnostics and repair, Service for this machine, Agents for connections, Usage for the selected Explorer project, or Logs for diagnostic reports. Only the selected tab loads its section. The project chosen in Management stays selected across its tabs. Existing links continue to reach the corresponding tab.

Adding a project records setup with registration. The native service prepares reserved storage, prepares the selected model when semantic mode is enabled, then indexes sources and eligible semantic units. It continues after the editor closes. A failed model setup leaves source indexing scheduled. Exhausted worker startup retries require the displayed restart action; acceptance alone does not prove recovery.

Removing a project stops its work and automatically deletes its AIRK index data, usage ledger and saved settings after the owned worker drains. Removed projects appear only while cleanup is pending. Cleanup resumes after interruptions; there is no second deletion approval. Original source files are untouched.
