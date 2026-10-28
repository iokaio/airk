# Managing project indexes

Version 1.5.7 development, 28 October 2026.

Management uses eight horizontal tabs: Indexing, Activity, Storage, Recovery, Service, Agents, Usage and Logs. Project tabs share a persistent project selector. The banner explains the observed state and offers a recovery action when one is known. Otherwise Update index or Pause indexing is the main action. Search availability describes the published index; current work describes its own attempt. Old counters are labeled retained when worker, service, generation or job identity differs.

The three everyday settings are Search mode (Text only / Text + semantic), Updates (Automatic / Manual), and Evidence detail. Pause preserves the update preference. Recent activity reads durable native job and configuration outcomes. Acceptance, applied settings and ready search are different milestones.

Enabling semantic verifies/prepares the selected offline model before saving the enabled policy. It preserves an existing custom selection and schedules source publication followed by eligible semantic processing. A publication showing disabled can lack model configuration or semantic work even while the saved policy is enabled. It is not proof of user opt-out. Failed model preparation preserves a previously disabled policy. Complete semantic setup is available for an enabled but disabled publication when no current work is observed.

Search mode and Evidence detail are accepted durably before inspection. Their native owner checks and applies the requested scalar setting, then continues indexing without the editor. Advanced changes retain their explicit preparation guards and bounded preparation wait. Management checks unknown job/configuration acknowledgements using a read-only lookup of the original request key; missing or expired receipts remain unknown. See [automatic recovery](index-self-healing.md) for persisted retries, prerequisite waits and cancellation rules.

## Progress and waiting

Progress shows phase, observation age, time since real advancement at that sample, captured files, newly extracted payloads, published transactions and eligible total when known. Captured files include reused extraction. Newly extracted is not a numerator for an overall percentage. Phase timings and completed semantic units remain available. Phases can repeat by batch; there is no estimated completion time.

A queued job after a completed attempt is labeled Work queued and names both jobs. A finished worker sample is not evidence that the queued job is running. Later successful publication or recovery supersedes an old recovery alert while preserving its failed historical receipt.

Worker startup exhaustion offers Restart project worker. Acknowledging restart is not recovery: a worker must connect and work advance. Pause, helper admission, memory pressure, extraction capacity and native locks have separate wait observations. Eligible quota/full-disk and missing-source failures retain the accepted job and wait for restored access or capacity. Recovery attempts and waiting reasons appear in the health banner and Recent activity. Unknown reasons retain their native code and diagnostic route.

## Registration and removal

Reviewed registration atomically saves native setup and watching for a new or reactivated binding. The service prepares reserved storage, prepares a model when semantic is enabled, then indexes sources and eligible semantic units. Closing the editor after registration cannot lose setup intent. Text indexing remains scheduled if model setup fails. Repeating registration for an active project does not restart setup or overwrite preferences.

Reopening a workspace verifies the selected runtime version and payload against a saved successful installation receipt. A matching completed installation skips terminal installation, runtime cleanup and setup notifications; new workspace folders can still register. Runtime completion is saved before project preparation. If first project preparation fails, the extension says AIRK is installed and directs the user to Management; the unchanged project warning is not repeated on every restart. Explicit service Stop is reported as stopped. Interrupted runtime installation, terminal/cleanup issues and explicit Install or Repair CLI retain the installation path. This does not make every editor window an installation owner or restart saved indexing.

Browse active or removed projects is a list filter, separate from Add project. Removal stops automatic work, cancels queued work and preserves sources, settings and retained index data. Storage and removal provides a bounded cleanup preview and separately confirmed deletion. Captured cleanup must finish before re-registration. A completed cleanup receipt is shown as ready for registration review; unknown cleanup state is never presented as complete.

## Advanced controls

Recent activity shows bounded job history and exact saved requests. Storage and removal shows accounted size, reservations, quota, inspection age and guarded cleanup. Advanced diagnostics and recovery contains raw identities, unreadable directories, worker restart, guarded settings and repair previews. Service is machine-wide; Agents, Usage and Logs have their own tabs. Indexing, Activity, Storage and Recovery preserve the selected Management project across tab changes. Only the active tab mounts its content. Existing Management URLs still reach their corresponding tabs, preserving links and assistance drafts.

## Reports and retained logs

Version 1.5.9 adds `job_failed` records for failed jobs observed by the supervisor, including failures that leave their worker running. Records carry the exact project, job ID and durable safe code. Preparation and discovery codes identify their observed stage and recognized SQLite/OS or inventory cause. Unknown causes stay unspecified; raw exceptions, SQL, paths and file content are excluded. These records diagnose new failures; they cannot recover missing details from earlier generic receipts. See the [1.5.9 record](release-1.5.9.md).

Logs offers Generate report, Open report, Copy report and Save report. Open/copy/save use the exact captured report, even if background observations have changed. Reports include the visible project page, cached job/configuration outcomes, recovery budgets, watcher observations and up to 128 cached native events, within a 256 KiB report bound; omissions are explicit. Paths and user/machine names are redacted by default. Reports are not uploaded.

Save retained service logs exports the complete available finite native ring separately: four files of at most 1 MiB each, rather than the recent 32-record display. Invalid records are counted and omitted; rotated-away records cannot be recovered. This is a local CLI export, not an enlarged service IPC response. The equivalent terminal command is `airk service logs --runtime-dir <private-control-directory> --all --format json`; use the inspected installation's log control directory, not its executable or project data directory.

Checks are recorded in the [release record](release-1.5.7.md). The repeated-recovery eligibility bug is reproduced and fixed; the original remote two-second operation failure remains unattributed. Local test evidence does not establish Linux/macOS or installed-editor qualification.

## Troubleshooting in 1.5.8

Refresh status reports when the service was checked, whether project work/settings changed, and whether the connection failed. It reloads observations without starting a repair. A queue without a reported blocker is explicitly unknown. Finished-job counters are collapsed while another request waits. Native metadata includes the owning repair plan and admission gate, including plan pause independently of project pause. Obsolete pending maintenance requests are settled with a failure receipt without replaying an old approval; eligible desired source work can then continue.

Use Copy details beside health, progress, Activity, semantic status, Service, Storage or Recovery. Copies are labeled cached metadata with identities and observation times, bounded to 64 KiB. Displayed paths/names may be included; no source contents or jobs are requested. Logs retains the separate redacted diagnostic report.

Overview and Management subscribe to the same automatic observations while visible. Connected service work updates about every two seconds; published/semantic readiness is checked at a bounded five-second cadence without waiting for a new publication token. Disconnected observations use backoff. Closed/hidden consumers release their polling subscription; polling does not pause native indexing. Check status now is a Service troubleshooting action. Active repair rows refresh automatically rather than retaining ready rows under a running header.

Recovery puts its plain-language project/action summary and Start repair above technical details. The fallback checkbox appears only for an explicitly required fallback and is bound to the displayed plan/digest. Changed previews require review again. Accepted submission is distinguished from running work and completion. Routine safe recovery belongs to the service and does not require this advanced approval flow.

Quiet reopen ensures the verified on-demand service is available while retaining the completed installation receipt. Explicit Stop remains authoritative. Runtime activation replaces stale console clients. Loading/connecting stays distinct from an observed empty registry in both main views. Windows connection migration preserves the editor document's EOL mode as well as saved policies and unrelated settings.
