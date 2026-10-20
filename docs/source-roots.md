# Source folders and aliases

A collection has one permanent primary root and up to sixteen explicitly registered additional source folders. This lets code in one repository share a collection with separate requirements, specifications or supporting documents. AIRK does not discover other folders automatically. Every source remains read-only.

Use the same `--root`, `--data-dir` and `--collection` options for all commands:

```console
airk --root /projects/app --data-dir /indexes --collection app collections roots add requirements /projects/specifications
airk --root /projects/app --data-dir /indexes --collection app index
airk --root /projects/app --data-dir /indexes --collection app collections roots list
airk --root /projects/app --data-dir /indexes --collection app search latency --path-prefix @requirements/
airk --root /projects/app --data-dir /indexes --collection app collections roots locate @requirements/service.xlsx
```

On Windows, use Windows paths such as `D:/projects/app`; quote paths containing spaces. Aliases contain one to thirty-two lowercase ASCII letters, digits, hyphens or underscores. `primary` is reserved. Additional sources cannot overlap one another or the primary root, and cannot be inside index storage.

Primary-root paths remain unchanged. Additional files use `@alias/original/relative/path` throughout search, facts, file reads and path filters. `collections roots locate` resolves that indexed path to its original canonical source folder and relative path. File information also exposes the original source location. Document coordinates still refer to the original page, paragraph, table row, worksheet/cell or slide; the alias does not replace those coordinates.

The primary root cannot contain a file or directory named `@alias` for a registered alias. Registration and indexing reject that collision instead of silently combining two different sources. The full prefixed path must fit the normal indexed-path limit. File-count, source/extracted byte, raw-read and traversal budgets apply across the entire collection, rather than restarting for each folder.

## Configuration and published evidence

Source configuration is stored in `roots.json` under the managed collection and bound to its primary root. Adding or removing an alias changes the configuration for the next index. It does not rewrite an existing revision. List output distinguishes configured and indexed mappings and reports `pending_index` until publication succeeds.

Each published revision retains its own source map. An already indexed `@requirements/service.xlsx` continues to identify the prior folder after an alias is removed or configured to a replacement. The replacement takes effect only after a successful index publishes the new map and content together. Collection ID/root validation still applies, and MCP arguments cannot register a folder or select another collection.

```console
airk collections roots remove requirements
airk index
```

Removing an alias retains the source folder and its files. The next successful index removes its evidence from the active revision; prior exact tokens expire and unreferenced objects become eligible for bounded cleanup. Rebinding requires an explicit remove followed by add. If a registered folder disappears or resolves to a different canonical directory, indexing fails while preserving the last published revision.

Coverage includes all configured sources and does not display a completed percentage while source mappings await publication. A running native reconciliation worker checks registered aliases during its bounded scans. The extension's filesystem notifications cover its editor workspace; use the native watch worker or explicit indexing for edits in additional folders outside that workspace.
