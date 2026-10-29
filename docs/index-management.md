# Managing project indexes

Updated for 1.5.12 development, 28 October 2026. The tabbed design originated in 1.5.7; dated release records retain their original validation scope.

Management → Agents also offers optional [project instructions](agent-integration.md#project-instructions-1512), with a copy button and separate open-file buttons for Claude Code, Codex and GitHub Copilot. Existing files are unchanged and missing files open as unsaved drafts; the user reviews and saves the suggestion. The bundled help topic covers other assistants and the distinction between instructions, MCP configuration and actual tool access.

Management uses eight horizontal tabs: Indexing, Activity, Storage, Recovery, Service, Agents, Usage and Logs. Project tabs share a persistent project selector. The banner explains the observed state and offers a recovery action when one is known. Otherwise Update index or Pause indexing is the main action. Search availability describes the published index; current work describes its own attempt. Old counters are labeled retained when worker, service, generation or job identity differs.

The three everyday settings are Search mode (Text only / Text + semantic), Updates (Automatic / Manual), and Evidence detail. Pause preserves the update preference. Recent activity reads durable native job and configuration outcomes. Acceptance, applied settings and ready search are different milestones.

Enabling semantic verifies/prepares the selected offline model before saving the enabled policy. It preserves an existing custom selection and schedules source publication followed by eligible semantic processing. A publication showing disabled can lack model configuration or semantic work even while the saved policy is enabled. It is not proof of user opt-out. Failed model preparation preserves a previously disabled policy. Complete semantic setup is available for an enabled but disabled publication when no current work is observed.

Search mode and Evidence detail are accepted durably before inspection. Their native owner checks and applies the requested scalar setting, then continues indexing without the editor. Advanced changes retain their explicit preparation guards and bounded preparation wait. Management checks unknown job/configuration acknowledgements using a read-only lookup of the original request key; missing or expired receipts remain unknown internally and are logged. Receipt lookup failures do not interrupt current project observations or globally block new actions; no unknown request is replayed automatically. See [automatic recovery](index-self-healing.md) for persisted retries, prerequisite waits and cancellation rules.

## Progress and waiting

Overview and Management → Indexing start with **In your index**: published files, searchable text chunks, semantic vectors and completed/selected semantic files. The coverage bar measures files with published semantic data, not overall progress or time remaining. Discovery and pending source updates are distinct. Counts come from one dated publication observation; partial console metadata is used only until the complete MCP snapshot arrives, without filling missing counts from a different revision. Disconnected observations stay labeled. Copy details includes these counts and their publication identity.

**Working now** follows with the current phase, sample age and real advancement. Source jobs show capture/extraction counts; vector jobs show completed semantic units, files committed in this attempt and published batches. Semantic preparation includes computing vectors and constructing search structures, so real work can advance before the next batch is published. Cleanup has its own highlighted stage and keeps published results visible. It does not lead with zero captured/extracted files. Routine running banners are folded into progress; errors, waits and recovery actions stay prominent.

**This attempt** is elapsed time for the reported job, not the entire initial index. Source counters reset per attempt; reported semantic units can include retained work from earlier attempts. Unknown stays unknown. Failed attempts are labeled failed; a completed attempt does not mean all source and semantic work is complete. Phases can repeat by batch; there is no overall percentage or estimated completion time.

A queued job after a completed attempt is labeled Work queued and names both jobs. A finished worker sample is not evidence that the queued job is running. Later successful publication or recovery supersedes an old recovery alert while preserving its failed historical receipt.

Worker startup exhaustion offers Restart project worker. Acknowledging restart is not recovery: a worker must connect and work advance. Pause, helper admission, memory pressure, extraction capacity and native locks have separate wait observations. Eligible quota/full-disk and missing-source failures retain the accepted job and wait for restored access or capacity. Recovery attempts and waiting reasons appear in the health banner and Recent activity. Unknown reasons retain their native code and diagnostic route.

## Registration and removal

Reviewed registration atomically saves native setup and watching for a new or reactivated binding. The service prepares reserved storage, prepares a model when semantic is enabled, then indexes sources and eligible semantic units. Closing the editor after registration cannot lose setup intent. Text indexing remains scheduled if model setup fails. Repeating registration for an active project does not restart setup or overwrite preferences.

Reopening a workspace verifies the selected runtime version and payload against a saved successful installation receipt. A matching completed installation skips terminal installation, runtime cleanup and setup notifications; new workspace folders can still register. Runtime completion is saved before project preparation. If first project preparation fails, the extension says AIRK is installed and directs the user to Management; the unchanged project warning is not repeated on every restart. Explicit service Stop is reported as stopped. Interrupted runtime installation, terminal/cleanup issues and explicit Install or Repair CLI retain the installation path. This does not make every editor window an installation owner or restart saved indexing.

Browse active or removed projects is a Management list filter; Explorer keeps active project choices separately. Removal stops automatic work, cancels pending/running jobs and drains the owned worker before automatically deleting the project’s index, settings and usage ledger. Source files are untouched. Pending cleanup resumes without a second approval; completed removal leaves only opaque identifiers and a timestamp and disappears from the project list.

## Advanced controls

Recent activity shows bounded native job history. Unacknowledged local requests remain internal and are logged without confirmation notices or retry/forget controls. Storage and removal shows accounted size, reservations, quota, inspection age and guarded cleanup. Advanced diagnostics and recovery contains raw identities, unreadable directories, worker restart, guarded settings and repair previews. Service is machine-wide; Agents, Usage and Logs have their own tabs. Indexing, Activity, Storage and Recovery preserve the selected Management project across tab changes. Only the active tab mounts its content. Existing Management URLs still reach their corresponding tabs, preserving links and assistance drafts.

## Reports and retained logs

Version 1.5.9 adds `job_failed` records for failed jobs observed by the supervisor, including failures that leave their worker running. Records carry the exact project, job ID and durable safe code. Preparation and discovery codes identify their observed stage and recognized SQLite/OS or inventory cause. Unknown causes stay unspecified; raw exceptions, SQL, paths and file content are excluded. These records diagnose new failures; they cannot recover missing details from earlier generic receipts. See the [1.5.9 record](release-1.5.9.md).

Logs offers Generate report, Open report, Copy report and Save report. Open/copy/save use the exact captured report, even if background observations have changed. Reports include the visible project page, cached job/configuration outcomes, recovery budgets, watcher observations and up to 128 cached native events, within a 256 KiB report bound; omissions are explicit. Paths and user/machine names are redacted by default. Reports are not uploaded.

Save retained service logs exports the complete available finite native ring separately: FIFO segments within a 200 MB high-water mark, purged oldest-first to at most 100 MB, rather than the recent 32-record display. Invalid records are counted and omitted; rotated-away records cannot be recovered. This is a local CLI export, not an enlarged service IPC response. The equivalent terminal command is `airk service logs --runtime-dir <private-control-directory> --all --format json`; use the inspected installation's log control directory, not its executable or project data directory.

Checks are recorded in the [release record](release-1.5.7.md). The repeated-recovery eligibility bug is reproduced and fixed; the original remote two-second operation failure remains unattributed. Local test evidence does not establish Linux/macOS or installed-editor qualification.

## Troubleshooting in 1.5.8

Refresh status reports when the service was checked, whether project work/settings changed, and whether the connection failed. It reloads observations without starting a repair. A queue without a reported blocker is explicitly unknown. Finished-job counters are collapsed while another request waits. Native metadata includes the owning repair plan and admission gate, including plan pause independently of project pause. Obsolete pending maintenance requests are settled with a failure receipt without replaying an old approval; eligible desired source work can then continue.

Use Copy details beside health, progress, Activity, semantic status, Service, Storage or Recovery. Copies are labeled cached metadata with identities and observation times, bounded to 64 KiB. Displayed paths/names may be included; no source contents or jobs are requested. Logs retains the separate redacted diagnostic report.

Overview and Management subscribe to the same automatic observations while visible. Connected service work updates about every two seconds; published/semantic readiness is checked at a bounded five-second cadence without waiting for a new publication token. Disconnected observations use backoff. Closed/hidden consumers release their polling subscription; polling does not pause native indexing. Check status now is a Service troubleshooting action. Active repair rows refresh automatically rather than retaining ready rows under a running header.

Recovery puts its plain-language project/action summary and Start repair above technical details. The fallback checkbox appears only for an explicitly required fallback and is bound to the displayed plan/digest. Changed previews require review again. Accepted submission is distinguished from running work and completion. Routine safe recovery belongs to the service and does not require this advanced approval flow.

Quiet reopen ensures the verified on-demand service is available while retaining the completed installation receipt. Explicit Stop remains authoritative. Runtime activation replaces stale console clients. Loading/connecting stays distinct from an observed empty registry in both main views. Windows connection migration preserves the editor document's EOL mode as well as saved policies and unrelated settings.

In 1.6.4, a shared 15-second poll deadline prevents a lost metadata response from freezing Overview or Management. Timeout marks observations unconfirmed and reconnects automatically. Local raw-log Open/Save bypasses polling and retains its outcome when changing tabs. Empty captures open explanatory text or save an empty file; large exports use native file captures rather than stdout buffering. Raw diagnostics also retain setup, connection-check and polling/action errors. See [diagnostic retention](service-logging.md).


## Missing source folders and project choices

From 1.6.4, trusted open folders register and start indexing automatically after upgrades unless explicitly opted out. Confirmed missing project folders leave active indexing automatically. Removing a project stops its work, drains owned workers and deletes its AIRK index files and saved settings; only an opaque project identifier and removal time remain in native history. Source files are untouched. Cleanup resumes after interruptions, including for registrations removed before this upgrade. Explorer keeps active project choices available while Management browses removed projects.

Removal deletes the project’s derived index and saved settings after owned workers drain. A minimal identifier/time record prevents automatic recovery from undoing an explicit removal. Checks run while the native service is active; ten seconds is the minimum between confirmations, not a guaranteed removal deadline.
