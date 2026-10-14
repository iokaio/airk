# Native indexing scheduler

The native scheduler persists indexing requests under the selected data root. All collections sharing that root share one worker lock, so separate editor windows and CLI workers serialize scheduled indexing. Different data roots are independent. No service is installed automatically and ordinary MCP reads do not run a scheduler or scan source files.

```console
airk --root /projects/example --data-dir /indexes --collection example schedule enqueue
airk --root /projects/example --data-dir /indexes --collection example schedule status
airk --root /projects/example --data-dir /indexes --collection example schedule run
airk --root /projects/example --data-dir /indexes --collection example schedule run --watch
airk --root /projects/example --data-dir /indexes --collection example index --scheduled
```

`schedule enqueue` registers the selected root and records an automatic request; `--full` requests re-extraction. `schedule run` executes at most one due collection, returning `idle`, `busy`, `indexed` or `failed`. `--watch` keeps an explicit foreground worker running until Ctrl+C; `--poll-ms` accepts 250–60000 milliseconds. The watch worker receives native filesystem events through `notify`: ReadDirectoryChangesW on Windows, FSEvents on macOS and inotify on Linux.

Native callbacks never block on indexing or SQLite. A bounded channel retains at most 256 events, each with at most eight paths and 32 KiB of path data. Access events and managed-data changes are discarded before channel admission. The worker coalesces source changes by collection, waits two seconds of quiet (at most thirty seconds), and persists automatic queue intents. Common native source exclusions suppress build/private-directory hints. Ignore-file eligibility, hashes and extraction remain native indexing decisions. Events arriving during an index remain pending for the next worker tick.

The watcher refreshes existing registrations and configured source aliases every ten seconds; it never discovers unrelated roots. A changed alias configuration schedules a pass. Every enqueue revalidates the canonical owner and aliases, so replacing a root with a symlink cannot rebind a collection. Supported backends are configured not to follow symlinks. Paused or blocked collections retain hints without clearing their policy. Successful event submissions use the same durable watermarks as editor submissions.

Channel overflow, backend errors and explicit native rescan signals conservatively mark all registered bindings dirty. Watch failures are reported, retried during registration refresh, and retain periodic reconciliation as a fallback. Events are hints, not a guarantee of completeness: network filesystems, platform watch limits and process downtime can lose notifications. Between indexing jobs and while idle, the worker performs at most one bounded coverage inventory per tick and no more than one per registered root every five minutes, even when no editor is open. Saved additions, changed hashes and pending removals enqueue another revision. It skips collections with pending work, paused collections and blocked indexing failures. Scan failures/partial inventories retain an error and wait the normal interval. Scan start/completion watermarks survive worker exits. Stop the foreground process to stop its watching and reconciliation.

## Explicit per-user installation

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

`scheduler.sqlite3` contains bounded job metadata and the latest index report for each registered collection: canonical root binding, requested/completed/running revision, full/manual request watermarks, event/completion timestamps, scan start/completion timestamps and errors, pause state and indexing error. Registration is capped at 1024 collections; stored error/report payloads and database pages are capped. SQLite transactions publish intent transitions durably. Reusing an ID with a different root fails closed. Earlier queue schemas upgrade without discarding pending work or pause state.

Automatic batches wait for two seconds of quiet, with a thirty-second maximum delay and five-second minimum interval after completion. Extension timing controls batch its own events before submission. A manual request runs immediately when the shared worker becomes available. Edits submitted while a job runs advance the requested revision, including automatic notifications from another editor; completing the captured revision leaves later work pending. Completed batches advance their pending timestamp so a continuously edited root yields to older waiting collections.

```console
airk schedule pause
airk schedule resume
airk schedule forget
```

Pause persists even before the first request. It prevents automatic jobs but permits explicit manual indexing, without resetting the pause. A failed index preserves the prior evidence and records a blocked pending request. Resume or a new explicit enqueue clears the block; the worker does not retry a persistent error continuously. Forget removes scheduling registration and pending intents without deleting indexed evidence. Collection removal obtains the worker lock and removes its pending registration so a worker cannot recreate the deleted index.

## Crash recovery and locking

`scheduler-worker.lock` is a stable operating-system advisory lock file. Closing or crashing the owner releases its lock; the file itself remains. A later worker resets abandoned running claims and retries their retained revisions. Lock ownership is established by the operating system, not by guessing whether a stored PID was reused.

Collection writers also use stable advisory locks under `collections/.writer-locks/`. A versioned `writer.lock` marker records the active process and is normally removed at completion. Current-version markers left by a crash are recoverable after acquiring the advisory lock. Older unversioned markers still require the deliberate recovery procedure in [operations](operations.md). Do not unlink active advisory lock files: replacing their inode can break mutual exclusion.

Direct `airk index` remains available and takes the collection writer lock; it does not participate in the machine-wide scheduled queue. The extension uses scheduled indexing. Reader generation leases and collection writer locks remain authoritative for publication and retention.

Native tests exercise reopening pending state, revision coalescing, full-rebuild escalation, pause/manual behavior, blocked failures, root binding, exclusive worker ownership and an actual child-process exit without destructors while holding both locks. Separate tests modify saved source with no editor and verify native events and periodic reconciliation queue the change. Filesystem-specific power-loss durability and login/reboot recovery remain outside that test evidence.
