# Operations

## Native state and collection selection

Use --root for the source workspace, --data-dir for the user-controlled index location and --collection for its stable local collection ID. Example:

~~~console
airk --root /projects/a --data-dir /fast-disk/airk --collection project-a setup --client codex
airk --root /projects/a --data-dir /fast-disk/airk --collection project-a index
~~~

Windows accepts a path such as D:/indexes/airk. State is stored under <data-dir>/collections/<id>. One collection currently binds to one canonical root. Native setup prints actual absolute paths. Source changes do not refresh until reindexed. Unchanged runs keep their generation and reuse extracted facts/chunks; they still scan and hash eligible files.

Binary/unrecognized, empty document and oversized files are reported as skipped. The next successful index removes their previous entries. Ignore-excluded content is not traversed and does not generate per-file skip records. Corpus-limit or filesystem errors retain the old snapshot.

## One-container deployment

From the AIRK checkout, set AIRK_WORKSPACES to the intended workspace parent directory, then:

~~~console
docker compose up -d --build
docker exec airk airk --root /workspaces/project-a --data-dir /data --collection project-a index
docker exec airk airk --root /workspaces/project-a --data-dir /data --collection project-a symbol authenticate
~~~

Use the scoped [Docker MCP configuration](../examples/mcp/docker.json) in each workspace. It uses docker exec -i against the existing airk container. No second container starts for a workspace or agent. Sources are read-only; /data is a persistent named volume. Runtime network access and exposed ports are unnecessary. docker compose stop stops the host while retaining data; docker compose start restarts it.

The host currently keeps scoped CLI/MCP workers in one container, not one shared in-process search cache. A future daemon will centralize memory and scheduling. Do not mount private reference folders or a whole drive unnecessarily. The Docker build allowlist excludes temp/, Git metadata and local indexes.

Several agents may use the same scoped native command or docker-exec command at once. Reuse --root, --data-dir and --collection; do not run docker run for each client or create an agent-specific index. Readers share published disk state and refresh on later calls after indexing. A collection's exclusive writer lock only serializes mutations. Disconnect all readers before removing a shared collection; current maintenance does not track every agent's active read lease.

## Space management

~~~console
airk --data-dir /fast-disk/airk collections list
airk --data-dir /fast-disk/airk collections remove old-project
docker exec airk airk --data-dir /data collections list
docker exec airk airk --data-dir /data collections remove old-project
~~~

Listing reports logical file bytes, file count, generation, indexed-source count and snapshot status. Filesystem allocation/compression may differ from summed file sizes. Removal validates a managed collection, takes its writer lock, verifies containment and refuses link entries; original workspace files remain untouched. Disconnect its workers first. Another collection is preserved. Corrupt/unmanaged state requires manual diagnosis rather than blind deletion.

Each collection stores one active SQLite revision/catalog with exact facts/relationships and optional SCIP bindings. WAL/shared-memory sidecars and retained legacy JSON recovery files count toward usage. SQLite reuses freed pages but does not automatically shrink; long-lived readers can retain WAL space. Legacy JSON migrates at the next successful index. Old-vector-generation pruning, physical compaction, hard quotas and automatic cleanup remain planned. [Extension controls](vscode-extension.md) offer a soft byte budget, storage selection and explicit collection removal; nothing is silently evicted.

## Recovery

A crash can leave writer.lock. Confirm no indexing worker uses that exact collection, then manually remove its lock and rerun indexing. PID reuse means the recorded PID alone is insufficient. AIRK never steals a lock automatically.

Corrupt/unsupported snapshots fail closed. Preserve them locally if diagnosis is needed, then choose fresh state or rebuild after deliberate cleanup. Never reassign one collection to another source root. Moving a workspace requires new state in the current scaffold. Do not publish snapshots in issues or commits.

SQLite WAL transactions with full synchronous commits publish catalog/revision/facts/edges together. Full power-loss/process-kill behavior, network filesystems and hostile concurrent filesystem mutation remain unqualified. PDF processing may allocate before post-extraction limits; source files must be trusted local inputs at this stage.

SQLite busy waits are bounded at two seconds; failed publication retains the previous revision. Ordinary MCP reads open read transactions without the collection writer lock and may finish on the older revision while a writer commits. [Code facts and optional SCIP import](code-facts-and-relationships.md) explain exact lookup, source/build binding and stale-analysis invalidation.
