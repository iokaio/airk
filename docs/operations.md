# Operations

> Historical 1.2/1.3 operator commands record, retained on October 26, 2026. The default 1.4.5 runtime uses the [native service](service-console.md) and [format-150 redb publication](service-publication.md). SQLite-specific layouts, reset/relocation commands and qualification below do not describe the current runtime. See [fresh setup and local qualification](release-1.4.5.md).

## Upgrading to 1.2

AIRK 1.2 starts fresh collection/configuration ownership in `collections-1.2/`; it does not import the old `collections/` indexes or per-collection settings. Old state remains untouched and still consumes disk space. Review roots, extraction detail, vector selection, ingestion limits and quotas for the new collection, then index saved sources. Shared model assets require digest verification. See the [current-index contract](current-index-runtime.md).

Refresh generated client configuration and reconnect agents after replacing a helper or changing storage. The extension connection guide preserves existing client settings. Verify helper identity and actual tool discovery separately.

## Native state and collection selection

Use --root for the source workspace, --data-dir for the user-controlled index location and --collection for its stable local collection ID. Example:

~~~console
airk --root /projects/a --data-dir /fast-disk/airk --collection project-a setup --client codex
airk --root /projects/a --data-dir /fast-disk/airk --collection project-a index
airk --root /projects/a --data-dir /fast-disk/airk --collection project-a coverage --refresh
airk --root /projects/a --data-dir /fast-disk/airk --collection project-a usage report
~~~

Windows accepts a path such as D:/indexes/airk. State is stored under <data-dir>/collections-1.2/<id>. Each collection binds to a canonical primary root and optional [source aliases](source-roots.md). Native setup prints actual absolute paths. Source and alias changes take effect through successful indexing; old evidence retains its published locations. Unchanged runs keep their current token and reuse extracted facts/chunks; they still scan and hash eligible files.

Unsupported binary, empty extracted document and oversized files are reported as skipped. Arbitrary UTF-8 files remain eligible without an extension allowlist. The next successful index removes excluded entries. Ignore-excluded content is not traversed and does not generate per-file skip records. Reports retain at most 1000 detailed skip records and count further omissions. Corpus/extracted-text limits or filesystem failures retain the previous current index.

Publication attempts bounded unreferenced-object cleanup. `airk collections maintain` repeats it without publishing; inspect `removed_files`, `removed_bytes`, `pending` and indexing's `maintenance_error`. Historical retention is unsupported.

## One-container deployment

From the AIRK checkout, set AIRK_WORKSPACES to the intended workspace parent directory, then:

~~~console
docker compose up -d --build
docker exec airk airk --root /workspaces/project-a --data-dir /data --collection project-a index
docker exec airk airk --root /workspaces/project-a --data-dir /data --collection project-a symbol authenticate
~~~

Use the scoped [Docker MCP configuration](../examples/mcp/docker.json) in each workspace. It uses docker exec -i against the existing airk container. No second container starts for a workspace or agent. Sources are read-only; /data is a persistent named volume. Runtime network access and exposed ports are unnecessary. docker compose stop stops the host while retaining data; docker compose start restarts it.

The host keeps scoped CLI/MCP workers in one container with separate process caches and shared current disk state. The native scheduler serializes each collection independently and admits different projects concurrently within storage-root execution budgets; an explicit `schedule run --watch` process drains queued work without installing a system service. Do not mount private reference folders or a whole drive unnecessarily. The Docker build allowlist excludes temp/, Git metadata and local indexes.

Several agents may use the same scoped native command or docker-exec command at once. Reuse --root, --data-dir and --collection; do not run docker run for each client or create an agent-specific index. Readers share published disk state and refresh on later calls after indexing. Collection writer locks serialize mutations; request-scoped read gates exclude publication/cleanup/removal while evidence is being read; idle connections retain no old generations.

## Scheduled indexing and local inspection

```console
airk index --scheduled
airk schedule enqueue
airk schedule run --watch
airk schedule status
airk schedule pause
airk schedule resume
airk coverage --refresh
airk usage report --format markdown
```

Supply the same root/data-dir/collection options as the workspace. The [native scheduler](native-scheduler.md) persists revisions, full/manual requests, pause and blocked failures. Explicit indexing can run while automatic work is paused. A foreground watch worker consumes registered intents and periodically runs bounded coverage inventories for those registered roots, queueing saved changes without an editor. It never discovers arbitrary source trees. Paused/blocked collections are skipped; scan errors wait the five-minute reconciliation interval. The extension also supplies coalesced requests and periodically reconciles [coverage](index-coverage.md). Usage commands expose [content-free aggregates and signed estimates](usage-reporting.md); disabling/clearing usage does not delete the index.

Lexical retrieval needs no model. Optional vector/hybrid modes require explicitly supplied, digest-checked [local model assets](local-models.md). Model changes publish a new coherent generation during indexing; invalid assets preserve the preceding revision. No remote fallback is used.

## Space management

```console
airk --data-dir /fast-disk/airk collections list
airk --root /projects/a --data-dir /fast-disk/airk --collection project-a collections maintain
airk --data-dir /fast-disk/airk collections remove old-project
```

Listing reports logical managed bytes and current indexed counts. Cleanup proves ownership before deleting bounded unreferenced current objects. Removal obtains writer, scheduler, usage and request exclusion and removes only the selected managed collection and its registration. Source files and other collections remain untouched. Active requests can block it; idle kernels cannot. Unknown ownership, corrupt state and links require diagnosis rather than blind deletion.

Quotas reserve growth before participating writes. Explicit [relocation](storage-relocation.md) copies and verifies current state, records recovery markers and preserves old managed payloads. Update client configuration explicitly after relocation. The extension provides current cleanup, storage limits and removal controls.

## Recovery

### Diagnose and repair in the extension

Use **Diagnose & Repair** on the affected collection, or **AIRK: Diagnose & Repair** from the command palette. The view checks the helper version/path, current discovery and semantic readiness, native queue, vector setting, extraction detail and storage measurements. It reads the selected collection without initiating an index scan. **Copy report** includes these bounded observations and recent local events; source and query text are not requested. Current observations and previous failed attempts are displayed separately.

Refresh also verifies a bounded page of retained source and extraction objects. **Verify retained data** continues at the same exact publication for up to two minutes; cancellation, a changed publication, skipped oversized inputs or an incomplete page sequence remain visible. It is disabled during the editor's active scan/enrichment for that collection and stops if work starts during verification. Complete retained-input verification does not imply every lexical or graph record was checked. A damaged diagnostic history file is reported separately and does not prevent live diagnosis or repair.

The report checks known project agent configurations and the Codex user configuration for saved AIRK entries targeting this collection. A different helper path can explain why an agent still uses an older cached executable after an update. Review the current entry through **Connect Coding Agent**, merge it into the client's settings and restart its MCP session. Unsupported, oversized or redirected configuration reads remain partial; no client configuration is automatically changed, and a matching entry does not prove a live connection.

The scoped CLI equivalent is `airk collections diagnose`. Defaults are 128 files and 32 MiB per page; use `--max-files` and `--max-bytes` to reduce the work. Continuation requires both `--revision` from `publication.token` and `--after` from `next_after`. A changed revision rejects continuation; restart from the current head. `complete` means enumeration reached the end: also inspect `failed_objects`, `skipped_large`, verified input counts and whether verification began at the first page. The command does not recover journals or modify retained objects.

The view's other helper commands keep normal startup recovery: a valid captured journal can be replayed when opening the collection. The dedicated retained-data verifier observes pending publication without replaying it. Neither path rebuilds evidence from changed source files during journal recovery.

| Observation | Action |
| --- | --- |
| Discovery ready, semantic partial, no failure | Refresh diagnostics and compare completion counts; the source pass is complete and semantic work remains. |
| Temporary publication contention | The extension retries semantic work after 1, 2, 4, 8, 16 and then at most 30 seconds. Progress, manual retry or resume resets the delay. |
| Paused queue | **Resume window indexing** resumes the open collections in this window. |
| Failed or blocked indexing | **Retry indexing** rechecks saved source and can recreate missing retained objects. |
| Retry cannot recover derived evidence | Review the exact error. **Rebuild index** recomputes compatible derived evidence and preserves source files, but corruption, repair bounds or an unavailable journal can still prevent it. |
| Permission, quota, bundled-helper or journal-integrity failure | Review the reported path and cause; repair the underlying condition before retrying. Rebuild is not guaranteed to repair an unavailable journal. |

Normal indexing can recreate missing retained source/extraction objects only from freshly validated source bytes, and a guarded recapture is bounded to one retry. If old extractions are missing for changed or deleted files, indexing can retire their postings using one authenticated current-map traversal per publication, subject to traversal and membership bounds. It never reconstructs missing journal inputs from live files. Repair admission retries typed read-gate contention for up to 120 seconds, backing off polling to 250 ms and checking Pause between attempts. This is bounded grace for a foreign verifier, not a guaranteed disk-time bound. Permission and integrity errors are not ignored. Keep data and diagnostics when a fault repeats; wiping the index removes useful evidence. Diagnostic history is bounded per collection and stays in the extension's local storage across reloads.

Extension helper calls set `AIRK_ERROR_FORMAT=json`. Operational failures then return a versioned stderr object (`airk_error: 1`, `code`, `retryable`, `requested_revision`, `message`) with exit code 1. Only `current_gate_busy` denotes typed current-index gate contention; `scheduled_index_pending` is retryable and carries the positive durable request revision; `scheduled_index_paused` is retryable and leaves the request queued. `operation_failed` remains non-retryable. Rust backtraces do not alter this contract. Ordinary CLI use retains human-readable errors; argument parsing errors keep their normal usage output.

The extension reads the last structured error envelope among progress lines and warnings, validates its fields, then truncates diagnostic text, so escaped Windows paths do not lose their retry classification. It displays the parsed message and retains bounded raw stderr separately. Diagnostics compare SHA-256 digests only for the managed `cli/bin` copy and the verified extension helper; equal contents count as current despite different paths. Other saved executable paths are not run or treated as equivalent by content.

For SQLite access failures, compare the queue error with saved readiness: the queue may be blocked even if semantic readiness remains partial with no failure, because recording that failure also needs a write. Diagnose & Repair treats read-only database errors as an access problem for review. Native Windows writer admission detects silent read-only fallback and retries only verified sharing denial for up to five seconds; persistent sharing and real permission failures remain explicit. Preserve the data and review storage access before retrying; neither repair nor diagnostics changes filesystem permissions.

Retry also retires abandoned publication quota reservations once Ready/no-pending-journal checks prove they are no longer needed. This resolves an existing-grant collision after failed preparation without deleting the collection. Cleanup validates all reservation incarnations before releasing any and preserves reservations owned by pending recovery or another collection.

Operating-system locks release after process exit. Never unlink active stable locks in `collections-1.2/.writer-locks/`, `.current-gates/`, `scheduler-1.2-worker.lock` or `usage.lock`. A PID alone is not proof of abandoned ownership.

The current database records Prepared and Applying. Prepared keeps the base readable; Applying blocks evidence reads until captured replay completes. Kernel, CLI and MCP startup recover pending updates from retained inputs without consulting changed live source. Repeated recovery is idempotent. A runtime-bootstrap recipe covers interrupted initial setup. Database growth has an admitted durable capacity/rollback allowance before changing the owner header.

Unsupported versions, foreign roots and corruption fail explicitly. They are never interpreted as an empty collection. Do not manually remove an object merely because it looks unreferenced: pending work can still own it. Use a fresh collection when deliberately abandoning incompatible derived state, and preserve source identity. `index --full` rebuilds compatible current state with saved 1.2 policies.

Process-exit qualification is separate from power-loss, full-device races, network filesystems and other-platform qualification. See [current storage](current-index-runtime.md) for work limits and availability semantics.

The extension makes at most three bounded native joins per turn, then displays **still queued**, releases its busy slot and revisits the same durable revision after 30 seconds. Pause stops continuation; other collections can proceed. Progress events carry the emitting collection ID, so one worker cannot update another collection's sidebar. Source scanning polls Pause at most every two seconds between artifacts, with additional checks at publication boundaries. A scan reserves up to 2 MiB for failure reporting before capture; if reporting also fails (for example due to I/O or journal recovery), the original typed error is preserved with that additional context.
