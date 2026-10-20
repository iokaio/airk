# AIRK 1.2.0

This release replaces searchable generations with one authenticated current index per collection. A fresh 1.2 collection is required; source files remain untouched. See [current-index operation and recovery](current-index-runtime.md) for the rebuild boundary and [the optimization ledger](indexingenhancements.md) for implementation and qualification evidence.

## Agent evidence modes

**Agent Lean** selects declarations, signatures, imports, local structure, entry points and useful document facts. It omits generic call occurrences, declaration fingerprints, standalone dates and broad document element inventories. Semantic inputs and exact citations remain independent of selected fact detail. Agent Lean defaults to compact MCP code/document facts; set `compact: false` for richer indexed details. Compact document evidence retains truncation flags and table cell/formula/cache qualifiers.

**Rich Structure** is the display name for existing Balanced (`balanced`) policy. **Search Only** is the display name for Essential (`essential`); vectors remain separately configurable. Existing saved policies keep their meanings, and Detailed remains an advanced option. New collections keep the qualified Balanced default until paired agent-task evidence supports promoting Agent Lean.

```console
airk collections extraction detail agent_lean
airk index
airk collections vectors selection show
```

Agent Lean's default semantic exclusions cover reviewed lockfile basenames and minified JS/CSS suffixes, while retaining lexical evidence. Version-2 vector policies support explicit inclusions and exclusions; potentially conflicting explicit rules fail validation. Existing version-1 policies retain their behavior. Preview examines at most 4,096 current files and returns at most 32 affected paths; unit counts remain unknown rather than triggering inference.

## Progressive availability and execution

Scheduled scans can publish changed exact-evidence batches for existing as well as empty collections. The scan's unknown total, committed subset and pause/failure state are explicit. Only a complete validated inventory permits the final absence sweep. A small semantic batch can follow each base batch before discovery completes. Pending semantic work survives restart; uncaptured work is rediscovered and source hashes revalidated. An explicit manual index can override an existing automatic pause without clearing it. A new Pause action revokes that admission. Pausing stops admission at bounded observation/commit boundaries and leaves earlier commits queryable.

Semantic planning uses a bounded producer queue across files, so small file tails can share model batches. Graph mutations retain one owner. Completed units are counted during actual planning instead of a duplicate planning pass. Exact-input/cache identities, original locations and alias occurrences are preserved.

`AIRK_EXECUTION_MODE` accepts `automatic`, `interactive` or `throughput`. CPU/memory admission respects existing extraction/inference ceilings and reserves CPU capacity. Four advisory slots coordinate index processes sharing a storage root; throughput mode opportunistically takes two and can admit up to 4 GiB of working memory on large hosts. Automatic/Interactive retain their conservative memory limits. Other roots/applications are not measured. These are admission heuristics, not an RSS cap or a claim that every allowed worker count improves performance.

```console
airk execution
airk index --scheduled --progress
```

Progress is JSON on stderr; final report JSON remains on stdout. MCP status includes applied detail, discovery/semantic readiness, an execution plan and independent scheduler freshness. Idle scheduling does not verify live source. Agent Lean fact replies include bounded observations when their revision can still be matched. Capabilities supports compact adapter information, an exact `adapter` filter and `rich: true` expansion before indexing.

## Optional lossless retained-object compression

Compression starts **off**. Per-collection choices are `off`, `automatic` and `space`. Automatic requires at least roughly 10% encoded savings; Space admits smaller savings over framing. Objects under 4 KiB or over 64 MiB remain raw. Retained source/extraction objects and newly written current-file metadata use versioned LZ4 blocks with authenticated indexes, encoded/decoded digests and explicit bounds. Original files, source hashes, extraction identities and floating-point graph values are unchanged.

```console
airk compression show
airk compression set automatic
airk compression repack --max-files 32
```

Repacking visits at most 32 files and 64 MiB of logical source/payload data per command, returns `next_after` when more remains, and reports oversized skipped files. Repeat with `--after <next_after>`; current mappings are selected again on every pass. New immutable replacements are quota-admitted and verified before their references publish through the existing journal. Source/payload repacking preserves facts, lexical segments, vector occurrences and graph records. Metadata adopts the selected encoding on subsequent metadata writes. Cleanup proves current ownership before reclaiming old representations. The extension's **Compression** control configures the policy and can run cancellable bounded repack passes.

Storage reporting separates referenced logical bytes from unique retained source/payload encoded bytes. It excludes other index components; filesystem allocation remains unmeasured. Block-range reads decode only intersecting admitted blocks. Normal source verification can check the pinned encoded representation without allocating a decoded source copy. Full reads still verify exact decoded identity. Compression is not applied to active SQLite, Tantivy, DiskANN or model files.

## Qualification

Final local checks, workload measurements, platform CI and release-producer identities are recorded in the optimization ledger and release artifacts. Optional features do not imply complete language resolution, uniform compression ratios, larger-than-memory qualification, power-loss durability, or measured agent billing savings. The existing release workflow promotes successful CI binaries from the exact main commit, then signs and publishes matching native/extension packages.
