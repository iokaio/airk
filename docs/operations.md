# Operations

## Native state and collection selection

Use --root for the source workspace, --data-dir for the user-controlled index location and --collection for its stable local collection ID. Example:

~~~console
airk --root /projects/a --data-dir /fast-disk/airk --collection project-a setup --client codex
airk --root /projects/a --data-dir /fast-disk/airk --collection project-a index
airk --root /projects/a --data-dir /fast-disk/airk --collection project-a coverage --refresh
airk --root /projects/a --data-dir /fast-disk/airk --collection project-a usage report
~~~

Windows accepts a path such as D:/indexes/airk. State is stored under <data-dir>/collections/<id>. Each collection binds to a canonical primary root and optional [source aliases](source-roots.md). Native setup prints actual absolute paths. Source and alias changes take effect through successful indexing; old evidence retains its published locations. Unchanged runs keep their generation and reuse extracted facts/chunks; they still scan and hash eligible files.

Unsupported binary, empty extracted document and oversized files are reported as skipped. Arbitrary UTF-8 files remain eligible without an extension allowlist. The next successful index removes excluded entries. Ignore-excluded content is not traversed and does not generate per-file skip records. Reports retain at most 1000 detailed skip records and count further omissions. Corpus/extracted-text limits or filesystem failures retain the old snapshot.

Inspect retention after setup or upgrade with `airk collections retention show`. New collections use automatic retention; existing collections without explicit policy remain manual. Policy origin distinguishes new-collection defaults from upgrades and operator choices. `collections retention preview` and `run` without `--apply` do not clean up. Use [retention policy and maintenance](retention.md) to preview/set policy or perform an explicit bounded pass. A successful publication remains successful if its later cleanup reports pending work or an error; inspect the maintenance receipt separately. MCP reads never perform cleanup.

## One-container deployment

From the AIRK checkout, set AIRK_WORKSPACES to the intended workspace parent directory, then:

~~~console
docker compose up -d --build
docker exec airk airk --root /workspaces/project-a --data-dir /data --collection project-a index
docker exec airk airk --root /workspaces/project-a --data-dir /data --collection project-a symbol authenticate
~~~

Use the scoped [Docker MCP configuration](../examples/mcp/docker.json) in each workspace. It uses docker exec -i against the existing airk container. No second container starts for a workspace or agent. Sources are read-only; /data is a persistent named volume. Runtime network access and exposed ports are unnecessary. docker compose stop stops the host while retaining data; docker compose start restarts it.

The host keeps scoped CLI/MCP workers in one container with separate process caches and shared immutable disk components. The native scheduler serializes submitted indexing across the data root; an explicit `schedule run --watch` process drains queued work without installing a system service. Do not mount private reference folders or a whole drive unnecessarily. The Docker build allowlist excludes temp/, Git metadata and local indexes.

Several agents may use the same scoped native command or docker-exec command at once. Reuse --root, --data-dir and --collection; do not run docker run for each client or create an agent-specific index. Readers share published disk state and refresh on later calls after indexing. Collection writer locks serialize mutations; native reader leases protect immutable generations during pruning and prevent collection removal while cached readers remain connected.

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

~~~console
airk --data-dir /fast-disk/airk collections list
airk --data-dir /fast-disk/airk --collection project-a collections prune
airk --data-dir /fast-disk/airk --collection project-a collections prune --apply
airk --data-dir /fast-disk/airk collections remove old-project
docker exec airk airk --data-dir /data collections list
docker exec airk airk --data-dir /data collections remove old-project
~~~

Listing reports logical file bytes, file count, generation, indexed-source count and snapshot status. Filesystem allocation/compression may differ from summed file sizes. Pruning first previews obsolete generation IDs/bytes; `--apply` removes only unpinned obsolete components. Removal validates the managed collection, obtains writer/worker/usage lifecycle locks and all generation leases, verifies containment and refuses link entries. It removes queued registration to prevent a worker recreating the deleted collection. Source files and other collections remain untouched. Disconnect active readers first; corrupt/unmanaged state needs diagnosis rather than blind deletion.

Each collection stores a SQLite publication pointer, content-addressed source/extraction objects and immutable inventory, exact-fact and Tantivy/DiskANN generations. Sidecars and retained legacy recovery files count toward usage. Legacy state migrates at the next successful publication. Shared [logical-byte quotas](storage-quotas.md) reserve growth before participating writes. Explicit [storage relocation](storage-relocation.md) copies and verifies state, records recovery markers and retains the old managed payloads. [Extension controls](vscode-extension.md) additionally offer a soft automatic-indexing budget, storage selection, trim previews and removal; nothing is silently evicted.

## Recovery

A current writer uses an operating-system advisory lock and an atomic `airk-advisory-v1` marker in `writer.lock`. Process exit releases ownership, so a later writer can safely recover that versioned marker. A legacy unversioned marker still fails closed: establish that no worker uses the collection before manually removing it. A recorded PID alone is insufficient because PIDs can be reused. Never unlink active stable locks under `collections/.writer-locks/`, `collections/.reader-locks/`, or the `scheduler-worker.lock`/`usage.lock` files.

Corrupt/unsupported snapshots fail closed. Preserve them locally if diagnosis is needed, then choose fresh state or rebuild after deliberate cleanup. Never reassign one collection to another source root. Moving a workspace requires new state in the current scaffold. Do not publish snapshots in issues or commits.

SQLite WAL transactions with full synchronous commits publish catalog/revision/facts/edges and the immutable component pointer together. A child-process test demonstrates scheduler/writer lock release and recovery after exit without destructors; this is narrower than a complete process-kill/power-loss campaign. Network filesystems and hostile concurrent filesystem mutation remain unqualified. PDF processing may allocate before post-extraction limits; source files must be trusted local inputs at this stage.

SQLite busy waits are bounded at two seconds; failed publication retains the previous revision. Ordinary MCP reads open read transactions without the collection writer lock and may finish on the older revision while a writer commits. [Code facts and optional SCIP import](code-facts-and-relationships.md) explain exact lookup, source/build binding and stale-analysis invalidation.
