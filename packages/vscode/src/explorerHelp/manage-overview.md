---
id: management.overview
title: Manage project indexing
route: manage/overview
area: management
aliases: management, setup, search mode, pause indexing, update index, semantic disabled
related_airk_help: indexing, jobs, models
---
# Manage project indexing

Management uses the same horizontal tabs as the other Explorer areas. Indexing is the default tab. Choose a project and read its health banner, current work and published search availability. The main action follows its state: update, pause, resume, restart a blocked worker or retry a failed job.

Search mode selects Text only or Text + semantic. Enabling semantic prepares the offline model, preserves an existing model selection and schedules source publication followed by semantic work. Disabled published coverage is not proof the saved setting is off. Settings can be queued; Recent activity shows their native outcomes.

Updates selects Automatic or Manual. Pause is temporary and preserves that preference. Evidence detail changes supported source structure independently of semantic search. Search mode and Evidence detail are accepted durably before native inspection; preparation and indexing continue with the editor closed. Advanced operations retain their separate preparation guards.

Choose Activity for requests and progress, Storage for storage and removal, Recovery for advanced diagnostics and repair, Service for this machine, Agents for connections, Usage for the selected Explorer project, or Logs for diagnostic reports. Only the selected tab loads its section. The project chosen in Management stays selected across its tabs. Existing links continue to reach the corresponding tab.

Adding a project records setup with registration. The native service prepares reserved storage, prepares the selected model when semantic mode is enabled, then indexes sources and eligible semantic units. It continues after the editor closes. A failed model setup leaves source indexing scheduled. Exhausted worker startup retries require the displayed restart action; acceptance alone does not prove recovery.

Removed projects retain data and settings. Browse removed projects to preview cleanup and approve deletion separately. Finish a captured cleanup before registering again. Original sources are never deleted.
