# Native indexing scheduler

The native scheduler persists indexing requests under the selected data root. All collections sharing that root share one worker lock, so separate editor windows and CLI workers serialize scheduled indexing. Different data roots are independent. No service is installed automatically and ordinary MCP reads do not run a scheduler or scan source files.

```console
airk --root /projects/example --data-dir /indexes --collection example schedule enqueue
airk --root /projects/example --data-dir /indexes --collection example schedule status
airk --root /projects/example --data-dir /indexes --collection example schedule run
airk --root /projects/example --data-dir /indexes --collection example schedule run --watch
airk --root /projects/example --data-dir /indexes --collection example index --scheduled
```

`schedule enqueue` registers the selected root and records an automatic request; `--full` requests re-extraction. `schedule run` executes at most one due collection, returning `idle`, `busy`, `indexed` or `failed`. `--watch` keeps an explicit foreground worker running until Ctrl+C; `--poll-ms` accepts 250–60000 milliseconds.

Between indexing jobs and while idle, the watch worker reconciles registered roots even when no editor is open. It performs at most one bounded native coverage inventory per tick and no more than one inventory per root every five minutes. Saved additions, changed hashes and pending removals enqueue another revision. It skips collections with pending work, paused collections and blocked indexing failures. Scan failures/partial inventories retain an error and wait the normal scan interval, preventing busy retries. Scan start/completion watermarks survive worker exits. Replacing a registered root with a symlink to a different directory fails the binding check. The worker never discovers unrelated roots or installs an operating-system service; stop its foreground process to stop autonomous reconciliation.

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

Native tests exercise reopening pending state, revision coalescing, full-rebuild escalation, pause/manual behavior, blocked failures, root binding, exclusive worker ownership and an actual child-process exit without destructors while holding both locks. A separate test modifies saved source with no editor and verifies native reconciliation queues and indexes the change. Filesystem-specific power-loss durability and installed service management are outside that test evidence.
