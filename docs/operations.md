# Operations

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

The host keeps scoped CLI/MCP workers in one container with separate process caches and shared current disk state. The native scheduler serializes submitted indexing across the data root; an explicit `schedule run --watch` process drains queued work without installing a system service. Do not mount private reference folders or a whole drive unnecessarily. The Docker build allowlist excludes temp/, Git metadata and local indexes.

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

Listing reports logical managed bytes and current indexed counts. Cleanup proves ownership before deleting bounded unreferenced current objects and lexical forks. Removal obtains writer, scheduler, usage and request exclusion and removes only the selected managed collection and its registration. Source files and other collections remain untouched. Active requests can block it; idle kernels cannot. Unknown ownership, corrupt state and links require diagnosis rather than blind deletion.

Quotas reserve growth before participating writes. Explicit [relocation](storage-relocation.md) copies and verifies current state, records recovery markers and preserves old managed payloads. Update client configuration explicitly after relocation. The extension provides current cleanup, storage limits and removal controls.

## Recovery

Operating-system locks release after process exit. Never unlink active stable locks in `collections-1.2/.writer-locks/`, `.current-gates/`, `scheduler-1.2-worker.lock` or `usage.lock`. A PID alone is not proof of abandoned ownership.

The current database records Prepared and Applying. Prepared keeps the base readable; Applying blocks evidence reads until captured replay completes. Kernel, CLI and MCP startup recover pending updates from retained inputs without consulting changed live source. Repeated recovery is idempotent. A runtime-bootstrap recipe covers interrupted initial setup. Database growth has an admitted durable capacity/rollback allowance before changing the owner header.

Unsupported versions, foreign roots and corruption fail explicitly. They are never interpreted as an empty collection. Do not manually remove an object merely because it looks unreferenced: pending work can still own it. Use a fresh collection when deliberately abandoning incompatible derived state, and preserve source identity. `index --full` rebuilds compatible current state with saved 1.2 policies.

Process-exit qualification is separate from power-loss, full-device races, network filesystems and other-platform qualification. See [current storage](current-index-runtime.md) for work limits and availability semantics.
