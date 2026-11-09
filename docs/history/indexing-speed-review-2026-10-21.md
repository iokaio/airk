# Indexing feedback review — 2026-10-21

This records the review and implementation decisions for the unreleased 1.2.5 candidate (format 126). The supplied review was read-only and quoted earlier repository measurements; it did not establish new speedups. Its source line numbers describe the pre-change checkout. AIRK MCP was unavailable for this implementation, so claims were checked against live source directly.

## What is being optimized

Measure an agent's edit-to-fresh cited evidence, terminal-only refresh cost, first useful lexical/fact evidence, semantic readiness and query latency during indexing. Full-build throughput is an additional metric. An early publication alone does not establish that an agent's needed evidence was included.

Every complete reconciliation still hashes admitted live files. A scoped pass hashes every admitted hinted file and preserves other indexed bindings. Source mtimes never establish reuse or freshness. Collection/root isolation, one writer, DiskANN, source hashes, citations, budgets and read-only MCP remain in effect.

## Disposition of the recommendations

The dispositions and measurements below record the original review implementation. The subsequent [seven-item follow-up](indexing-follow-up-2026-10-21.md) supersedes R4(a)/R13 with a guarded process cache and explicitly changed verification contract, and adds revision-based reconciliation suppression to R3. It has separate qualification; the measurements below do not measure that cache.

| Review item | Implementation and decision |
|---|---|
| R1 — retained reuse lookup | Implemented presence, regular-file and exact stored-length checks for source/extraction objects, including encoded objects. The live source is still captured and hashed before reuse. Missing objects cause recapture; permissions, links and length mismatches fail. Content authenticity is verified whenever retained bytes are consumed and by retained-data diagnostics. A no-op rescan is explicitly not a complete retained-data integrity check. |
| R2 — path-scoped refresh | Implemented direct `index --paths`, durable `index --scheduled --paths` / `schedule enqueue --paths`, editor path accumulation and native watcher path hints. Scheduled hints are bounded and coalesced by revision; hints received during a running pass survive its completion. Overflow promotes to a complete inventory. Configuration changes, directory hints, ignore/configuration-file hints and incomplete inventories use a complete scan. |
| R3 — scheduling and inventories | Settled editor/watcher requests bypass the second native debounce via `--coalesced`. The editor minimum interval defaults to 1 s; its 2 s quiet period still applies. Edits invalidate coverage without an event-time or post-index coverage scan. Periodic and explicit inventories remain; an accepted periodic observation of stale/missing/deleted sources queues a complete pass subject to indexing mode and pause. Returned readiness replaces the post-run vector-status helper call. Model setup, upgrade detection and storage preflight checks remain. |
| R4(a) — FTS5 seal | Empty FTS preparation returns before taking its preparation snapshot. Full verification at the publication boundary remains. A revision-keyed process cache is not accepted as proof that mutable SQLite bytes are unchanged. No removal of the per-publication seal is claimed. |
| R4(b) — reclamation | Semantic drains hold the existing capture guard across intermediate commits and perform one ownership-proved sweep at the drain boundary. Cleanup failure is returned separately from successful publication in the scheduled report. Each publication still admits its own quota reservation. |
| R4(c) — quota | Keep physical measurement and durable grants. Caching measurements across independent reservations without accounting for other processes, collections, abandoned grants and temporary/replay space could under-admit storage. Incremental accounting needs a separate ledger design and failure qualification. |
| R4(d) — model verification | Keep byte authentication. The asset size, mtime or unchanged declared model digest does not prove its current bytes. Existing same-size corruption and model-open failure tests remain applicable. Model loading/plan setup is amortized across the bounded drain instead. |
| R5 — multiple semantic batches | The scheduled CLI/extension invocation reuses a model for up to 8 durable 32-file batches, with a 60 s soft budget checked between batches. A running batch remains atomic and can exceed that budget. Pause and new source work cause a yield between batches; other pending source work also limits a drain. Editor edits during a drain send one early durable hint so the native worker can yield. Successful drains continue immediately and ready editor collections rotate fairly. A source-work yield leaves a one-second opportunity for another worker; busy retries retain capped backoff. The root-wide scheduler still advances one batch per worker pass. |
| R6 — inference utilization | Interleave sorted inference groups across workers and restore result order. Existing worker/memory ceilings and qualified tensor shapes remain. Overlapping inference with graph mutation, larger proof caches, more workers and new tensor shapes require measurements and the real-model oracle; they are not enabled speculatively. AIRK's neural backend is `tract-cpu-onnx`, so ONNX Runtime thread settings do not tune this implementation. |
| R7 — useful evidence order | Complete discovery uses deterministic documentation/instruction and manifest priorities, with entry points before ordinary sources and tests/fixtures/vendor/generated paths later. Every eligible path is still visited. Semantic batch selection favors the most recently completed durable path hints, then those same categories. Pending manifests and individual publication batches remain canonically path-sorted. No additional parser pass is introduced to rank declarations. |
| R8 — scan pipeline | Reused results enter the bounded ordered extraction queue without emptying it. Fresh parallel work drains only enough results to admit the next input. This preserves callback/failure order and memory bounds. Oversized, skipped and deliberately serial extraction paths retain their barriers. Protected-read prefetch and background retention remain future experiments. |
| R9 — object durability | Keep existing per-object synchronization. Moving synchronization to a publication boundary requires bounded tracking of every newly referenced object, all required directory synchronization, correct failure recovery and durability qualification. Merely delaying calls is not evidence of fewer durable writes. |
| R10 — duplicated CPU | Compact artifact comparisons now compare all header and retained-reference fields directly, with serialization retained for noncompact inputs. An unchanged progressive pass reuses its already loaded previous inventory. Declared header contexts use focused discovery. Parser/registry/payload caches and broader statement changes remain profiling candidates. |
| R11 — double application | Retain preparation, rollback and exact captured replay. The proposed shortcut removes normal-path verification against the predeclared target; equivalent integrity has not been demonstrated. Require an explicit publication design, recovery/corruption evidence and the existing ≥20% publication-time improvement gate before replacement. |
| R12 — measurements | Scheduled semantic results retain model, inference, planning, graph, cache and publication/drain counters and record actual work once in the existing usage ledger. Index reports return semantic/discovery readiness and vector counts. The editor logs returned timings. Status/change freshness exposes the latest completed scheduled source-pass timings, whether it was scoped, and the last complete inventory timestamp. Finer walk/lookup/quota/fsync stage instrumentation remains a profiling follow-up. |
| R13 — cross-request lexical cache | Not implemented: an unchanged head does not prove unchanged FTS5 shadow rows. Retain current physical checks and corruption detection on fresh reads. A safe replacement must authenticate physical state, including missing matches, rather than just cache a revision. |
| R14 — terminal agent loop | Documented below and in the native scheduler guide. MCP remains read-only; status observations never start indexing and do not prove live-source freshness. |

## Contract decisions

An explicit hinted path may be retired after protected discovery and exact path reconciliation establish that it is absent, excluded, oversized, nonregular or unsupported. This is a deliberate extension of complete-inventory deletion: unrelated absence is never inferred from a scoped pass. A directory hint, including a deleted directory with committed descendants, requests a complete inventory instead. A scoped update does not complete an interrupted discovery epoch or advance the last complete inventory timestamp. An already complete epoch retains its identity while its committed-file counts reflect explicit additions/removals.

Incremental quota accounting, deferred object synchronization and a replacement for double application are not adopted. Their current guarantees are retained until a replacement has relevant accounting, crash/recovery and corruption evidence. These decisions keep independent changes reviewable and measurable.

## Terminal agent loop

```console
airk --root /projects/example --data-dir /indexes --collection example index --paths src/retry.rs tests/retry.rs
airk --root /projects/example --data-dir /indexes --collection example status
airk --root /projects/example --data-dir /indexes --collection example search retry --mode lexical
```

`index --paths` publishes directly in the selected collection and returns after lexical/fact publication; semantic work may remain pending. Add `--scheduled` to join the shared durable queue, including an already active writer; that queue may execute other registered collections first. Direct updates still use the durable publication/recovery journal. Paths are slash-separated logical paths, with `@alias/relative/path` for configured additional roots. Hints accept at most 1,024 paths and 1 MiB of total path bytes. Register the root once with `schedule enqueue`, then use `schedule run --watch` in an explicit foreground process to keep terminal-only work fresh and continue semantic work; its native watcher and periodic full reconciliation cover missed hints. `semantic run` advances retained pending work explicitly without registration; registered collections can use the bounded `semantic run --scheduled` drain.

For scheduled work, inspect `freshness.state`, requested/completed watermarks, `freshness.last_index.path_scoped` and `freshness.last_inventory_unix_ms`. An idle queue or a recently updated path does not establish that all live sources match the index. Returned citations remain bound to their indexed revision.

## Measurement and qualification

`scripts/benchmark-indexing-feedback.py` runs five alternating before/after trials with isolated generated corpora. It checks no-op reuse and verifies the changed file's source hash in a returned lexical citation. It records CLI wall times, first-publication and stage counters, and median/p95 edit-to-evidence. Usage recording is disabled for the probes. Fixture generation, vector opt-out and initial setup are separate from feedback measurements. The script measures vectors-off lexical/fact feedback, not neural relevance or device I/O.

```console
python scripts/benchmark-indexing-feedback.py --before target/speed-review/baseline-airk.exe --after target/debug/airk.exe --files 54 --trials 5 --output target/speed-review/feedback-54.json
```

Raw generated state and measurement receipts stay under ignored `target/`. The historical 54-file MiniLM, real repositories and 10k/100k workloads in the review remain distinct corpora; a synthetic fixture of the same file count does not reproduce them. Full production, memory/I/O and cross-platform qualification require their own runs.

With verified local model assets, `--semantic-model target/local-vector-model --files 54 --bytes-per-file 128` measures scheduled semantic completion and required helper/status calls. It applies no artificial sleeps. Both variants must reach complete file coverage and return hybrid citations matching source hashes. The baseline has no semantic timing report, so its missing counters remain unavailable rather than zero. This is a bounded MiniLM fixture, not a semantic relevance benchmark.

### Windows measurements

Five alternating pairs per workload used the same preserved before/after debug executables, both version 1.2.5 and format 126. Builds and tests did not overlap the timing runs. Each variant received a fresh collection and generated source tree; OS caches were not controlled. Times below include process startup. With five samples, the reported p95 is the slowest sample, not a reliable population tail estimate.

| Workload / metric | Before median | After median | Reduction | Before / after p95 |
|---|---:|---:|---:|---:|
| 54 files × 16 KiB, unchanged rescan | 0.629 s | 0.535 s | 14.9% | 0.642 / 0.540 s |
| 54 files × 16 KiB, edit to cited lexical evidence | 1.303 s | 1.128 s | 13.4% | 1.313 / 1.146 s |
| 1,000 files × 1 KiB, unchanged rescan | 4.338 s | 3.847 s | 11.3% | 4.355 / 3.907 s |
| 1,000 files × 1 KiB, edit to cited lexical evidence | 7.873 s | 6.528 s | 17.1% | 7.935 / 6.576 s |
| 54 files × 128 bytes, MiniLM semantic completion after source publication | 22.553 s | 17.612 s | 21.9% | 22.763 / 17.671 s |

Every timing trial verified source hashes in returned citations. Both semantic variants completed all 54 selected files. The baseline needed two semantic helper calls and two status calls; the candidate needed one semantic helper call and no status calls. The harness adds no editor sleep, so this measures drain/helper improvements without attributing hypothetical debounce savings.

Cold source indexing was effectively unchanged: 7.414 → 7.374 s on the 54-file vectors-off fixture and 75.640 → 75.098 s on the 1,000-file fixture. With MiniLM configured, source publication took 32.643 → 32.709 s. Lexical query medians stayed about 39 ms and 50–51 ms; hybrid queries stayed about 5.6 s. These results establish no cold-build or query speedup. Queries were issued after the update/drain, so latency during concurrent indexing remains unqualified.

On the 1,000-file edit, reported scan/extraction fell from 1.201 s to 0.055 s (about 95%), while publication remained 3.556 → 3.494 s. The total update still took 6.476 s before its verification query. Scoped discovery removes substantial source work, but does not bound all setup, verification, quota and publication costs by the number of changed paths. Add finer timers around those remaining operations before replacing their integrity/accounting design. Model execution setup during deferred source publication is another measured follow-up: MiniLM first-publication timing remained about 26 s in this fixture.

Receipts: `target/speed-review/feedback-54.json`, `feedback-1000.json`, `semantic-54.json`. Executable SHA-256 identities are `406d835c559b519e37cb9034015715573cc8fefd365ee65339a31a300d337c2f` (before) and `5042909afdc66d7f85dd34211d2223edc2ba42949b9c1f21bfcc621414ae217b` (after). The semantic model is the pinned `all-MiniLM-L6-v2`; its canonical model manifest SHA-256 is `2799022183337b422fa6f34beaf4bb4bef5eb2ff2e357de791c75e5122895e29`. These local debug results do not predict release-build performance, real-repository relevance, RSS, physical I/O, crash durability or other platforms.

### Storage follow-up

Separate single-pair runs used the same executables and fixture definitions, measured after all helpers exited and citations passed. These are logical file bytes, including collection metadata and scheduler state, not allocated filesystem blocks or peak temporary storage. Their timings are not added to the five-pair performance samples.

| Complete state | Before | After | Change |
|---|---:|---:|---:|
| 54 × 16 KiB, vectors off, after one-file edit | 2,437,686 bytes | 2,445,820 bytes | +8,134 bytes (+0.33%) |
| 54 × 128 bytes, all semantic files complete, including installed/captured model assets | 182,755,109 bytes | 182,763,244 bytes | +8,135 bytes (+0.0045%) |
| Same semantic state excluding both model asset copies | 988,043 bytes | 996,178 bytes | +8,135 bytes (+0.82%) |

There is no measured storage reduction. In both pairs `current.sqlite3` was 8,192 bytes larger; small metadata differences include variant path lengths. Retained source/extraction bytes were identical, as were the semantic embedding packs (36,864 bytes), resource descriptors and installed/captured model assets. Model assets were 90,883,533 bytes per copy in both variants. This work changes scheduling and repeated computation rather than the model, dimensions or stored representation. Receipts are `target/speed-review/size-lexical-54.json` and `size-semantic-54.json`; the harness now records storage categories for future runs. A single small fixture does not establish large-repository storage behavior.

### Faster-model follow-up

The current model is `all-MiniLM-L6-v2`. A smaller [paraphrase-MiniLM-L3-v2](https://huggingface.co/sentence-transformers/paraphrase-MiniLM-L3-v2) is a candidate, with 384-dimensional outputs and a documented 128-token input limit, compared with the current 256-token limit. Neither equivalent code/document retrieval quality nor a speedup under AIRK's runtime has been established. Matching output dimensions does not make vectors from different models interchangeable.

The [Sentence Transformers efficiency guide](https://sbert.net/docs/sentence_transformer/usage/efficiency.html) describes INT8 CPU acceleration. A quantized export of the current L6 model is the first recommended experiment for retaining its training and contextual capacity. Its published runtime results do not establish support or speed in AIRK's `tract` backend. Require export/operator compatibility, end-to-end latency and held-out repository recall/ranking checks before selecting it; quantization changes vectors and pipeline identity. This is a follow-up recommendation, not an implemented model change. Candidate inference counters were approximately 2.1 s in the 17.6 s semantic drain on the tiny fixture; a model change alone does not remove publication, asset verification and process/setup costs.

### Verification

The Windows workspace test run passed 931 tests with 16 ignored. After the final direct path-routing adjustment, the affected current-index unit suite passed all 24 tests and the CLI suites passed. The current workspace build, formatting and Clippy with warnings denied passed. The native-backed extension suite passed 142 tests with 2 skipped. Python repository tests ran 125 tests successfully with 4 skipped. Repository/whitespace checks and license-notice generation passed. The explicitly enabled real-MiniLM oracle verified exact serial/parallel, cached and bypassed vectors.

Regression coverage includes scoped additions/removals and fallback, root isolation, preserving concurrent hints, manual versus automatic scheduling, source-work yield, corruption/missing-object behavior, bounded extraction, canonical semantic publication and returned progress/timings. No new dependency or index format was introduced by this speed work. No VSIX was installed or published; installed-editor acceptance, release binaries, larger/real workloads and other-platform qualification remain open.
