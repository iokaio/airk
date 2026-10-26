# Native indexing scheduler

> Historical 1.2/1.3 standalone scheduler record, retained on October 26, 2026. The default 1.4.5 runtime uses the [native service](service-console.md) and [format-150 redb publication](service-publication.md). SQLite-specific layouts, reset/relocation commands and qualification below do not describe the current runtime. See [fresh setup and local qualification](release-1.4.5.md).

The native scheduler persists indexing requests under the selected data root. From 1.3.1, each collection has its own worker lock. Separate projects sharing a data root can index concurrently; windows on the same collection share one worker. The existing four execution slots bound aggregate work within that storage root, with CPU/memory budgets per admitted worker. Capacity exhaustion queues work without marking it failed. Admission precedes retained model parsing; model loading and inference reuse the same lease. Synchronous indexing also retains its source-pass lease through semantic completion. No service is installed automatically and ordinary MCP reads do not run a scheduler or scan source files.

Full rebuild requests are durably enqueued even while the collection writer is busy; only the executing worker prepares the rebuild. Waiters retrieve the report for their satisfied intent revision, including coalesced intents. Completion history keeps four passes per binding, bounded globally to 128 records / 16 MiB. If an older result has expired or predates this history, the waiter reports that it was superseded instead of returning the latest pass's report. Queue status still exposes the latest report.

```console
airk --root /projects/example --data-dir /indexes --collection example schedule enqueue
airk --root /projects/example --data-dir /indexes --collection example schedule status
airk --root /projects/example --data-dir /indexes --collection example schedule run
airk --root /projects/example --data-dir /indexes --collection example schedule run --watch
airk --root /projects/example --data-dir /indexes --collection example index --scheduled
```

`schedule enqueue` registers the selected root and records an automatic request; `--full` requests re-extraction. `schedule run` executes at most one due collection, returning `idle`, `busy`, `indexed` or `failed`. `--watch` keeps an explicit foreground worker running until Ctrl+C; `--poll-ms` accepts 250–60000 milliseconds. The watch worker receives native filesystem events through `notify`: ReadDirectoryChangesW on Windows, FSEvents on macOS and inotify on Linux.

AIRK accepts `index --paths src/retry.rs tests/retry.rs` and `schedule enqueue --paths ...`. The former directly publishes source/lexical/fact evidence in the selected collection; semantic work may remain. Add `--scheduled` to join the durable queue for only the selected collection. Direct publication still uses the recovery journal. Paths are logical collection paths, including `@alias/relative/path` for configured additional roots. Up to 1,024 paths and 1 MiB of path bytes are admitted. Source hashes and normal ignore/protected-read rules still apply to every admitted file. Unhinted bindings retain their indexed evidence. Hints received during a running pass survive completion; overflow requests a complete inventory. Exact hinted removals can retire that path, while directory/global-policy changes and incomplete discovery use a complete scan. After registering with `schedule enqueue`, terminal agents can keep `schedule run --watch` running to handle edits, semantic work and periodic reconciliation without an editor.

Already settled submitters use `--coalesced` with `index --scheduled` or `schedule enqueue` to bypass the native quiet/minimum-interval wait. Pause and failure policy still apply. Native watcher submissions set this only after their own quiet/max-delay rule. The editor default minimum interval is 1 s, with its 2 s quiet period retained.

Native callbacks never block on indexing or SQLite. A bounded channel retains at most 256 events, each with at most eight paths and 32 KiB of path data. Access events and managed-data changes are discarded before channel admission. The worker coalesces source changes by collection, waits two seconds of quiet (at most thirty seconds), and persists automatic queue intents. Common native source exclusions suppress build/private-directory hints. Ignore-file eligibility, hashes and extraction remain native indexing decisions. Events arriving during an index remain pending for the next worker tick.

The watcher refreshes existing registrations and configured source aliases every ten seconds; it never discovers unrelated roots. A changed alias configuration schedules a pass. Every enqueue revalidates the canonical owner and aliases, so replacing a root with a symlink cannot rebind a collection. Supported backends are configured not to follow symlinks. Paused or blocked collections retain hints without clearing their policy. Successful event submissions use the same durable watermarks as editor submissions.

Channel overflow, backend errors and explicit native rescan signals conservatively mark all registered bindings dirty. Watch failures are reported, retried during registration refresh, and retain periodic reconciliation as a fallback. Events are hints, not a guarantee of completeness: network filesystems, platform watch limits and process downtime can lose notifications. Between indexing jobs and while idle, the worker performs at most one bounded coverage inventory per tick and no more than one per registered root every five minutes, even when no editor is open. Saved additions, changed hashes and pending removals enqueue another revision. It skips collections with pending work, paused collections and blocked indexing failures. Scan failures/partial inventories retain an error and wait the normal interval. Scan start/completion watermarks survive worker exits. Stop the foreground process to stop its watching and reconciliation.

## Explicit per-user installation

The VS Code extension does not need this service to finish semantic indexing. It calls `semantic run --scheduled` for active trusted collections after indexing and on reopen. The extension reuses one model for up to eight durable batches with a 60 s soft budget checked between commits; the root-wide worker still advances one semantic batch per turn. A drain yields for pause or pending source work in its own collection and never executes another collection's jobs. Returned reports include readiness, vector counts and timing/cache counters. Closing the workspace stops further calls. The service below is optional for continuing work when the editor is closed.

An installed worker uses the same queue, root bindings, aliases, quotas and locks as a foreground worker. Installation is optional, never performed by MCP reads or extension activation. An explicit data directory is required:

```console
airk --root /projects/example --data-dir /indexes --collection example service plan
airk --root /projects/example --data-dir /indexes --collection example service install
airk --root /projects/example --data-dir /indexes --collection example service status
airk --root /projects/example --data-dir /indexes --collection example service stop
airk --root /projects/example --data-dir /indexes --collection example service start
airk --root /projects/example --data-dir /indexes --collection example service uninstall
```

`plan` returns the definition and destination without writing configuration or starting a process. `status` reads the owned configuration and queries the OS manager; a manager failure is reported as unknown, not as a stopped service. `install` writes a configuration and integrity receipt, enables login startup and starts the worker. `stop` stops this session's process while preserving startup at the next login; `start` restarts it. `uninstall` stops the worker and removes only its matching registration, definition and receipt. Collection evidence and source files remain intact. If registration fails after writing configuration, the error reports the retained state; inspect status and use start or uninstall.

| Platform | Per-user manager | Login behavior |
|---|---|---|
| Windows | Task Scheduler, current-user SID, `InteractiveToken`, `LeastPrivilege` | Runs while that user is logged in; no password or administrator principal is configured. |
| Linux | `systemctl --user`, unit under the user's configuration directory | Starts with the user manager. AIRK does not enable system-wide services or lingering. |
| macOS | `launchctl` in the user's `gui/<uid>` domain, plist under `~/Library/LaunchAgents` | Starts in that user's graphical login session. |

The service identity hashes the current user and canonical data root; collections sharing a root share one installed worker. Arguments use platform escaping and direct process APIs, never a shell command assembled from workspace paths. Manager subprocesses have a thirty-second timeout and bounded retained output. Windows helpers run without creating a console window. Paths containing control characters are rejected. Existing or modified definitions are not overwritten; receipts bind the definition hash and data root. Keep the executable at the installed path; after moving it, uninstall and reinstall explicitly. Stop and uninstall the old-root service before storage relocation, then install against the new root. The original launch root remains an immutable startup binding and must remain accessible.

Local tests exercise definition escaping, ownership checks, status parsing, callback overflow and actual native mutations in temporary primary/alias roots. Ordinary tests do not register login services. On 14 October 2026, the explicit opt-in `scripts/check-user-service.py` fixture passed against Windows Task Scheduler: install/start, initial background indexing, automatic reindex after a source edit, stop/start, uninstall and verification that its unique registration was removed. Windows task definitions use UTF-16LE with a BOM, as verified by that real manager exercise. Login/reboot recovery, systemd user-session and launchd lifecycle still require their corresponding platform release checks; generated definitions alone are not lifecycle qualification. See the upstream [notify contract](https://docs.rs/notify/8.2.0/notify/), [Task Scheduler principal schema](https://learn.microsoft.com/en-us/windows/win32/taskschd/taskschedulerschema-logontype-principaltype-element), [systemd service command syntax](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html) and [Apple launch-agent guidance](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html).

`index --scheduled` persists a manual request and waits for its result for up to 240 seconds. Multiple callers can join the same pending pass. `--automatic` uses automatic scheduling policy and requires `--scheduled`. An interrupted or timed-out caller leaves its request available for another worker. A full rebuild requested during an incremental pass remains a subsequent full pass.

## Revisions, failures and pause

From 1.2.4, the extension retries the specific transient current-publication `WouldBlock` result before a semantic batch or during its status refresh. A native `busy` result also yields to the existing scheduling interval. Permission and journal-recovery errors still block. In 1.2.4, the next source-batch timer began after inline semantic publication finished. From 1.3.0, progressive scanning defers inference to the existing semantic queue after source discovery and adapts source publication cadence to transaction cost; see [current indexing](current-index-runtime.md). See [diagnosis and recovery](release-1.2.4.md).

The 1.3.0 source extension observes `index --progress` without putting progress on stdout. It stops a native index child after ten minutes without a validated progress observation, while keeping the one-hour total limit. Each editor-driven scheduled semantic batch has a twenty-minute process limit. A timeout fails the current scheduler request and records its error; it does not erase committed publications or trigger automatic destructive repair. Diagnose & Repair exposes retry, Resume and confirmed rebuild actions. The MCP helper can start and be checked while these jobs continue; search requires a published base revision.

`scheduler-1.2.sqlite3` contains bounded job metadata and the latest index report for each registered collection: canonical root binding, requested/completed/running revision, full/manual request watermarks, event/completion timestamps, scan start/completion timestamps and errors, pause state and indexing error. Registration is capped at 1024 collections; stored error/report payloads and database pages are capped. SQLite transactions publish intent transitions durably. Reusing an ID with a different root fails closed. The 1.2 namespace starts a fresh queue and does not adopt older requests or pause settings.

Automatic batches wait for two seconds of quiet, with a thirty-second maximum delay and five-second minimum interval after completion. Extension timing controls batch its own events before submission. A manual request runs immediately when its collection worker and execution capacity become available. Edits submitted while a job runs advance the requested revision, including automatic notifications from another editor; completing the captured revision leaves later work pending. Completed batches advance their pending timestamp so a continuously edited root yields to older waiting collections.

```console
airk schedule pause
airk schedule resume
airk schedule forget
```

Pause persists even before the first request. It prevents automatic jobs but permits explicit manual indexing, without resetting the pause. A failed index preserves the prior evidence and records a blocked pending request. Resume or a new explicit enqueue clears the block; the worker does not retry a persistent error continuously. Forget removes scheduling registration and pending intents without deleting indexed evidence. Collection removal obtains only that collection’s worker lock and removes its pending registration so a worker cannot recreate the deleted index.

## Crash recovery and locking

`scheduler-1.3-workers/<collection>.lock` is a stable operating-system advisory lock file. Closing or crashing the owner releases its lock; the file itself remains. A later worker resets only that collection’s abandoned running claim and retries its retained revisions. Root-wide `schedule run` workers skip held collections; `index --scheduled` and `schedule wait` execute only their selected collection.

Each new worker also holds a shared lease on `scheduler-1.2-worker.lock` throughout its work. Helpers through 1.3.0 require exclusive ownership of that file, so old and new scheduling rules cannot overlap. An active old worker produces an explicit instruction to close or restart older AIRK editor/watch workers and retry with 1.3.1 or newer. New workers coexist without replacing or unlinking lock files. Storage relocation retains exclusive root lifecycle protection, including collection workers. Lock ownership is established by the operating system, not by guessing whether a stored PID was reused.

Collection writers also use stable advisory locks under `collections-1.2/.writer-locks/`. A versioned `writer.lock` marker records the active process and is normally removed at completion. Current-version markers left by a crash are recoverable after acquiring the advisory lock. Older unversioned markers still require the deliberate recovery procedure in [operations](operations.md). Do not unlink active advisory lock files: replacing their inode can break mutual exclusion.

Direct `airk index` remains available and takes the collection writer lock; it does not participate in the durable scheduled queue. The extension uses scheduled indexing. Request-scoped read gates and collection writer locks remain authoritative for publication and current-object cleanup.

Coverage refresh and maintenance in another editor can briefly hold a collection writer lock outside the indexing queue. A scheduled index that encounters this live-lock contention returns `busy` and releases its running claim while preserving the queued revision, full/manual flags and pause policy. The waiting caller retries within its existing 240-second allowance. Contention does not mark the collection blocked; genuine indexing failures still do. Quota accounting also tolerates managed files or directories removed by another writer during its walk, while continuing to count outstanding growth reservations and rejecting links or other I/O failures.

Native tests exercise reopening pending state, revision coalescing, full-rebuild escalation, pause/manual behavior, blocked failures, root binding, exclusive worker ownership and an actual child-process exit without destructors while holding both locks. Separate tests modify saved source with no editor and verify native events and periodic reconciliation queue the change. Filesystem-specific power-loss durability and login/reboot recovery remain outside that test evidence.

Collection-scoped semantic tests verify that foreign source requests remain queued without interrupting the selected collection’s bounded drain. A child-process regression holds one project in an active scan while another publishes and is deleted; same-project worker exclusion and unchanged foreign running claims remain required.


## Resuming a queued request (1.3.0)

`airk index --scheduled` persists a request before joining its collection worker. If its 240-second join budget expires, the CLI reports `scheduled_index_pending` with `requested_revision` in structured error mode. This means the request remains queued; it does not mark the native job blocked. `airk schedule wait <revision> --progress` continues that exact request without enqueueing another revision or clearing errors or pause. Completion is checked before the join deadline so a completed request returns its saved report immediately. Unknown revisions fail explicitly.

VS Code uses this continuation automatically and displays waiting separately from active scan progress. Trust, shutdown and collection binding are checked between invocations; native pause and blocked errors stop the join. Each helper invocation remains bounded. A late native completion can supersede this specific historical wait failure in diagnostics, but cannot supersede unrelated errors or claim complete retained-data verification.

The dashboard distinguishes **Waiting for indexing capacity** from **This project is already indexing in another window or process**. These observations preserve previously displayed committed progress. Older helpers report their explicit restart error instead of an anonymous shared-worker wait.
