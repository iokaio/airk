# redb storage experiment: implementation and abandonment

October 26, 2026. **Decision: abandon the redb cutover and retain SQLite.**

The owner accepted this decision after reviewing the completed implementation and local qualification. Branch `feat/storagekv` preserves the experimental 1.4.5 code and this record; it is not a release candidate to ship or merge into main. The final implementation and qualification checkpoint is commit `6b4ca21923b844306a00a4e6ba2d4207e6c4288c`. The final instruction is to document the decision, commit remaining changes and push this branch, with no further work. No native 1.4.5 archive or VSIX was packaged, installed or published.

## What we set out to test

The experiment replaced the 1.4.4 format-140 SQLite/WAL implementation with redb 4.4.0 and a format-150 AIRK-owned index. The hypothesis was that a native Rust key/value engine could remove SQLite and collection WAL/SHM files, avoid repeated lexical preparation work and make lexical authentication proportional to the evidence traversed, while retaining AIRK's correctness and isolation guarantees.

This was a complete storage cutover, not an isolated database benchmark. The accepted scope required removing every SQLite runtime, development, test, build and transitive dependency. No SQLite fallback, migration reader or conversion utility remained. Frozen comparison data and historical descriptions were retained as evidence.

## What this branch implemented

- Moved collection catalogs, lexical records, DiskANN graph/vector records, publication journals, service registrations and jobs, usage/scheduler ledgers and private staging to redb. DiskANN's algorithm and codecs remained unchanged. Original sources, immutable retained packs and model assets stayed outside the database behind validated references.
- Replaced SQLite lexical machinery with AIRK-owned authenticated maps, postings, document/path identities and term normalization. Preserved bounded AND/prefix retrieval, stable ordering, original locations, citation identities and explicit corruption/work-limit failures. The briefly considered Tantivy addition was not adopted.
- Implemented durable Prepared/Applying publication, captured replay material, funded recovery, coherent committed snapshots, retained-reader protection, ownership/fencing, automatic capacity admission and bounded cleanup. Performance work reused verified metadata and scoped table handles without removing integrity checks or work-budget accounting.
- Added logical database sizes to `airk_status` and a manually invoked VS Code compaction operation with reader/writer exclusion, before/after identity verification and measured reclaimed bytes.
- Implemented a fresh-setup boundary instead of migration: ownership-checked removal of recognized old AIRK databases under process exclusion, new registration and full indexing. Old database-backed registrations, jobs and usage were not imported; sources, unknown files, unrelated projects and separately stored settings were preserved.
- Integrated compatible native lifecycle and CI fixes from `feat/airksvc` and subsequently `main`, including signing fixes through `217f226`. Fixed issues exposed during qualification, including Windows metadata replacement with retained readers, capacity-admission races and old-runtime upgrade quiescence.

The detailed implementation contract remains in the [storage design](storagekv/storagekv-design.md). The [1.4.5 qualification record](release-1.4.5.md) preserves intermediate failures, final measurements, runtime identities and platform limits. Same-project indexing from another checkout/location was recorded separately as future work, not implemented by this experiment.

## What the final comparison showed

The final comparison used the official 1.4.4 Windows release runtime and the experimental 1.4.5 Windows release pair, with matching checked-in release build policy. It retained the original workload and the maximum 10% p95 regression gate. Seven repetitions and twelve query samples per repetition were collected. All seven gates passed, with content/location equivalence and exact per-runtime publication/citation evidence across restart.

| Case | SQLite 1.4.4 p95 | redb 1.4.5 p95 | Change |
| --- | ---: | ---: | ---: |
| Common lexical query | 179.79 ms | 177.82 ms | 1.1% faster |
| Rare lexical query | 162.95 ms | 164.56 ms | 1.0% slower |
| Prefix lexical query | 166.68 ms | 167.72 ms | 0.6% slower |
| Replace 32 files | 2,250.80 ms | 1,904.52 ms | 15.4% faster |
| Deletion | 1,234.14 ms | 1,088.14 ms | 11.8% faster |
| Vector query | 286.30 ms | 246.74 ms | 13.8% faster |
| Semantic replacement | 3,868.87 ms | 3,542.53 ms | 8.4% faster |

| Resource | SQLite 1.4.4 | redb 1.4.5 | Change |
| --- | ---: | ---: | ---: |
| Peak sampled simultaneous native RSS | 92,590,080 bytes | 133,255,168 bytes | +43.9% |
| Peak sampled logical data size | 7,524,678 bytes | 14,855,128 bytes | +97.4% |
| Final logical data size | 7,353,838 bytes | 14,283,777 bytes | +94.2% |
| Combined native executable size | 295,350,784 bytes | 296,046,080 bytes | +0.24% |

These are small, synthetic, end-to-end AIRK measurements: 32 Rust files with eight functions each and a fixed small word-vector fixture. Timings include CLI/service/worker and application work; publication completion uses polling. The versions ran sequentially with warm OS caches, not a cold-cache protocol. Sampled RSS can miss peaks and count shared pages more than once; logical file lengths are not allocated disk blocks. These results do not establish large-corpus behavior or isolate the database engine's contribution. Earlier debug comparisons failed gates and remain recorded as failures; the passing release comparison does not erase them.

The detailed local report is retained at `artifacts/local/redb-1.4.5-release-comparison-1/report.json`. It and runtime/test artifacts are intentionally ignored and are not included in the branch push. The tracked qualification record preserves the results and identities for review.

## Why public redb benchmarks do not imply the same AIRK gains

The [redb 4.4.0 benchmarks](https://github.com/cberner/redb/blob/v4.4.0/README.md#benchmarks) compare particular database operations; they do not show redb winning every workload or space comparison. Their [comparison adapter](https://github.com/cberner/redb/blob/v4.4.0/crates/redb-bench-compare/benches/common/mod.rs) uses ordinary key/value operations, not AIRK's complete authenticated lexical, publication, quota and service pipeline.

AIRK still has to parse and normalize content, maintain authenticated structures, validate ownership and budgets, prepare recoverable publication, perform durable commits and return exact evidence. Replacing an underlying engine does not remove that work. Replacing SQLite's lexical facilities also transfers indexing responsibilities into AIRK. The measured result combines that new implementation with the engine change; it cannot attribute every gain or cost to redb itself. These are explanations of the workload difference, not a profile proving which individual operation dominates.

## Why we are abandoning adoption

There were real advantages: complete SQLite dependency removal, no collection WAL/SHM files and modest measured gains in publication/vector cases. But lexical latency was essentially unchanged, executable size did not materially shrink, and this workload used substantially more memory and logical storage. Snapshot reads, quota enforcement and reclaiming unused space are not benefits unique to redb.

Passing a non-regression gate established that the tested release was acceptable against that threshold; it did not establish a compelling reason to migrate. The measured gains do not justify maintaining a custom authenticated lexical implementation, a new storage/recovery format and an upgrade requiring fresh setup and full indexing. This is a cost/benefit decision for AIRK, not a conclusion that redb is generally slow or unsuitable.

Qualification was substantial: Windows workspace tests passed 1,206 cases with 16 explicit ignores; Linux qualification passed 1,222 executed tests with 17 skips. Formatting, Clippy, doctests, repository/dependency checks, native upgrade/model/client/lifecycle checks and applicable extension scenarios passed as detailed in the qualification record. Installed-VSIX acceptance, native macOS/signing, the broader installed-platform matrix and hardware power-loss behavior were not qualified. Abandonment is not being presented as an unexplained test failure or a completed product release.

The branch and its history are retained for reference. Backend-independent fixes may warrant separate review later, but no extraction, revert, merge, branch switch, additional implementation or packaging is part of this preservation task. The experimental code remains redb-only and versioned 1.4.5; retaining SQLite for the product does not silently restore SQLite inside this branch.
