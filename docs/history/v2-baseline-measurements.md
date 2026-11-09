# Architecture v2 baseline measurements

These phase 0 receipts measure the schema 3 / `native-facts/7` implementation before catalog caching or storage migration. They use only original generated TypeScript, Markdown and package JSON fixtures. They establish a comparison method, not a latency guarantee or native-platform qualification.

## Reproduction and accounting

Build the native binary using the repository toolchain, then run:

```console
python scripts/benchmark-v2.py --binary target/debug/airk.exe --output target/v2-baseline.json
python -m unittest discover -s scripts/tests -p test_benchmark_v2.py -v
```

On macOS/Linux, use the corresponding native binary path. Python 3.11+ and the standard library are sufficient. The harness reuses `benchmark-scale.py` process measurement, corpus hashing and logical-byte accounting. It generates a private temporary source/state directory, never indexes the checkout, and writes only a content-free measurement report to the requested destination. Do not commit generated state. Reports contain temporary source locations and are local artifacts; the compact public receipts below omit those locations.

The full default is 300 code files plus two document/project files, 200 successful single-file edit publications, a 300-file move, and five fresh MCP workers with twenty repeated metadata calls each. Each edit verifies one changed extraction and reuse of every other file. The unchanged scan must preserve its generation. Reads must return the same metadata, except request IDs; moved metadata must cite the new path and original-byte hash. Index/read operations must preserve the source fixture digest.

Cold means the first `airk_file_info` request in a newly initialized MCP worker, with OS caches uncontrolled. Warm requests reuse that worker and its kernel; this differs from launching a fresh CLI process for every request. MCP initialization is measured separately. Percentiles use nearest rank. CLI wall time includes process startup and the 20 ms sampling loop inherited from the scale harness. Native usage reports retain their own stage/resource accounting. Sampled process counters do not measure physical disk traffic; Windows counters describe logical transfers. State sizes are logical file lengths, including usage records, measured after each completed publication; they exclude peak staging usage.

## Windows original-fixture receipt, 14 October 2026

Environment: Windows 11 build 26300, x64, twelve logical CPUs, Intel family 6 model 165 stepping 2, 68,622,794,752 bytes physical memory. Input was the local debug `airk 0.1.0` binary, 133,658,112 bytes, SHA-256 `6b63fcd304a8030bd15557dce61dd8d093d711e25d9c9eab2021b903a3c0e2ff`. Default ingestion and quota policies; lexical mode, no embedding model. Machine caches/background load were uncontrolled. Repository tests overlapped part of the initial campaign; no causal attribution of the measured costs is made.

The full 300-file/200-edit campaign was stopped after eight edit output files because the debug full-build campaign was slow. It produced no completed report and is **not** a successful 200-edit receipt. The baseline was split into the following independent runs; this does not replace the later combined scale/retention qualification gate.

```console
python scripts/benchmark-v2.py --binary target/debug/airk.exe --edits 2 --output target/v2-baseline-moves.json
python scripts/benchmark-v2.py --binary target/debug/airk.exe --files 3 --edits 200 --output target/v2-baseline-edits.json
```

The completed move run began with 302 files / 20,088 source bytes. Its path/content inventory SHA-256 was `542ce13da6ebdb104fff475659c98c249608f4f964a58cac3063221327a6da39`.

| Observation | Samples | p50 | p95 |
|---|---:|---:|---:|
| Cold-worker metadata | 5 | 71.401 ms | 94.223 ms |
| Warm-worker metadata | 100 | 65.568 ms | 74.505 ms |
| MCP initialization | 5 | 17.936 ms | 32.320 ms |
| Single-file edit CLI wall time | 2 | 12,653.713 ms | 12,869.122 ms |

Initial native scan/extraction took 2,483,178 microseconds and publication took 12,249,624 microseconds. The unchanged scan reused all 302 files without a new generation. Moving 300 files produced generation 4 with 300 indexed, 300 removed and two reused; native scan/extraction took 1,855,996 microseconds and publication 11,579,164 microseconds. This is the current path-bound extraction behavior, not content-move reuse. Retained logical state after the two edits was 1,764,343 and 2,442,993 bytes.

The supplemental small-corpus run completed **200 successful edit publications**, reaching generation 201 before its three-file move. It began with five files / 293 source bytes, path/content inventory SHA-256 `c6b20ea63456d1a22634ef23ae31b87995c83204924e60ad953db31eb96e1475`, using the same binary digest and machine. Every edit indexed one file and reused four. Retained logical bytes grew from 182,926 after the first edit to 12,257,049 after the 200th. No pruning policy was enabled; this is an accumulation baseline, not a bounded-retention qualification.

| Supplemental observation | Samples | p50 | p95 |
|---|---:|---:|---:|
| Cold-worker metadata | 5 | 46.200 ms | 47.588 ms |
| Warm-worker metadata | 100 | 42.656 ms | 45.308 ms |
| MCP initialization | 5 | 16.721 ms | 19.138 ms |
| Single-file edit CLI wall time | 200 | 1,465.714 ms | 2,492.389 ms |

The two campaigns overlapped and used uncontrolled background load. Their different file counts and retention lengths make them separate workload observations, not a controlled scaling comparison. Both completed correctness checks. The final harness also retains each edit's native index receipt and captures the machine/binary identity before the run; those additive reporting fields were introduced after the long campaigns began. The reported long-campaign values above are the actual completed measurements, not reconstructed stage counters.

## Structural baseline and outstanding measurement work

Source inspection corroborates the cache baseline: `Kernel::file_info` calls `Kernel::catalog`; `FileStore::catalog_bounded` reads/verifies `inventory.json` and deserializes `Snapshot` on each invocation before loading the artifact. Repeated warm-worker timing alone cannot establish the presence or absence of that parsing. Phase 2 needs a structural invocation-count assertion alongside these timing comparisons. The existing search cache does not establish metadata caching.

The harness records per-operation OS counters, available native usage measurements, binary/corpus hashes, edit generations and retained logical sizes. It does not yet count parser calls, source reads, inference calls, full-inventory deserializations, individual component writes or peak staged bytes. These values must remain unavailable rather than inferred from elapsed time or reused-file counts. Each subsequent integration should instrument the owning component and preserve missing counters in reports.

Further original workloads remain required: branch-like replacement, repeated boilerplate ambiguity, composite/notebook edits, explicit local models, paused/blocked/failed scheduler runs, pinned readers and automatic retention plateau. The 200-edit baseline has no automatic-retention policy and cannot qualify the phase 4 bounded plateau. No real Claude Code/Codex paired task, token/billing saving, clean build duration, release-package size, macOS/Linux run or power-loss campaign is claimed here.

Use these observed distributions as provisional comparison references. Before choosing absolute performance thresholds, repeat under controlled machine load in release builds with representative corpora. Mandatory structural targets remain: no full inventory parse for warm single-file metadata; compatible equal content avoids parsing; equal embedding input avoids inference; unprotected storage reaches the selected retention-policy plateau.

Focused harness validation: seven Python tests passed for deterministic fixture hashes, nearest-rank percentiles, persistent worker reuse, distinct request IDs, changed/wrong-file metadata rejection, tool-error rejection, EOF and bounded timeout. The integration owner records the required broader repository checks separately.
