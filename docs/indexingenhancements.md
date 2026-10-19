# Indexing and retrieval optimization implementation plan

Review date: 2026-10-19. Measurement baseline: `dd3675701242e91a03a0808f2f20c6d8f0af44a7` (1.1.4). Implementation branch: `feat/indexoptimizations`. The execution ledger below distinguishes completed work from proposals. Historical measurements describe the baseline, not later implementations. The accompanying [measurement harness](../scripts/benchmark-indexing.py) runs against isolated state and does not change the user's collection.

## Execution contract and stage ledger

Implement the stages below in order, validate each stage, and commit it locally before starting the next implementation stage. Do not push. Design reviews may run independently; implementation changes for different stages must not be mixed into one commit. Preserve unrelated work, source read-only behavior, collection/root isolation, exact current evidence, original locations and bounded resource use. DiskANN remains the final vector engine. No new external service or inference download is part of this work.

The target is one current logical index with incremental component updates and explicit semantic readiness. Token-aware semantic units are independent of exact citation partitions and selected structural detail. MiniLM remains an optional local discovery route. **Rebuild-only upgrade:** no backward-compatible index readers, dual-format runtime or migration of old generations/caches/history will be implemented. Users must rebuild derived indexes after format/pipeline changes. Reject incompatible state with an actionable rebuild message; preserve source files and saved collection/root/policy configuration. Primitives alone do not constitute a completed current-index implementation.

| Stage | Implementation and dependency | Completion gate | Status |
| --- | --- | --- | --- |
| S0 | Reconcile this plan, preserve baseline measurements and add the isolated benchmark harness | Documentation/repository/whitespace checks; local planning commit | Complete |
| S1 | O1 + exact-preserving O3: distinguish inputs, blanks, duplicate reuse and actual neural forwards; skip blank scheduling and deduplicate equal inputs within bounded batches | Cold/warm/full counters reconcile, duplicate citations remain, corrupt cache fails closed, serial/parallel output parity | Complete |
| S2 | O4: versioned ONNX length buckets, bounded compiled-plan ownership, then true tensor batches | Native numerical/retrieval equivalence, explicit pipeline identity, memory limits and real local-model measurements; old pipelines require rebuild | Complete |
| S3 | O2: independent token-aware vector units, stable identities and explicit unit-to-evidence mappings; establish the source/configuration-preserving rebuild boundary | Long-tail retrieval passes; complete exact/lexical evidence and original locations survive; bounded fan-out/path filters; obsolete formats rebuild without compatibility readers | Complete |
| S4 | O3: persisted vector-selection policy and exact-input graph deduplication using S3 mappings | Lexical-only selection is inspectable; duplicate aliases/path filters and delete/move behavior remain correct | Complete |
| S5 | O5: reuse S4's bounded input registry, add binary cache packs and authenticated ownership/GC | Corruption, partial writes, full-rebuild bypass, quota and pending/recovery ownership tests; compare cache I/O | Complete |
| S6 | O8: push selected detail into adapters, retain independent semantic boundaries, bound extraction concurrency and reconcile scan hints | Selected-evidence/location parity, deterministic results and budgets; same-size changes and missed events still detected | Complete |
| S7 | O6 foundation: qualify writable DiskANN updates, stable IDs, deletion/live mapping and replay; versioned current-state metadata, OS read/write gate and durable journal | Isolated provider and multiprocess fault tests; no runtime format switch yet | Complete |
| S8 | O6 integration: per-artifact catalog/Tantivy/vector updates, reader refresh, integrity, recovery, rebuild-only format adoption and historical API/UI capability changes | Every read path uses the gate; crash/disk-full/replay tests and explicit old-format rejection; one-file work scales with changes; switch only after these pass | In progress |
| S9 | O7: base lexical/fact commit followed by durable, coalesced semantic enrichment of the same current index | Pending/partial/failed coverage, supersession, restart and source-binding tests; first-ready and fully-ready timings | Pending |
| S10 | O9: bounded graph-record caching, scratch/positioned I/O and persistence improvements under the S8 journal | Exact record/corruption tests, ANN recall, quota and cold/update benchmarks; retain only measured improvements | Pending |
| S11 | Integrated qualification and user documentation across O1–O9 | Full required checks, judged retrieval suite and repeated workload measurements; report achieved gains and remaining platform/scale limits | Pending |

Each implementation commit updates its ledger status and records the relevant validation and measured limitations. Runtime changes require formatting, clippy, workspace tests, repository and whitespace checks from [CONTRIBUTING](../CONTRIBUTING.md), plus focused tests that exercise the changed behavior. Run Python/extension checks when their surfaces change and complete them at integrated qualification. Local Windows success does not qualify macOS/Linux, power loss or larger-than-memory behavior. Tests requiring local licensed model assets must be explicitly run when those assets are available, not counted as passed because they are ignored by default.

**Experiment decisions:** Shape sizes, tensor batch sizes, vector target lengths, overlap, graph parameters and shard/delta layouts are candidates to measure, not simultaneous mandatory defaults. If a candidate fails quality, memory or throughput gates, record the evidence and retain the qualified implementation. Quantization, alternative inference runtimes and dimension reduction are future research, not required changes. This does not defer S1–S11 or permit marking unused scaffolding as implemented. Default semantic-unit/eligibility changes require relevance evidence; dropping history uses the explicit S8 rebuild boundary.

**Stage records:** S0 is committed as `6b0d694`; it retains all original experiments and makes this ledger the authoritative delivery order. Repository checks (667 public files), harness syntax and whitespace checks passed.

S1 is committed as `25023e3`. It adds `embedding_inputs`, `embedding_blank_inputs`, `embedding_duplicate_inputs` and `embedding_actual_forwards`. `embedding_inferences` retains its existing logical uncached-input meaning, including blank inputs and duplicate misses; use `embedding_actual_forwards` to count neural executions. ONNX input grouping is limited to 32 inputs, uses exact bytes, validates the input budget before blank filtering, reads/persists each distinct entry once per batch, and scatters vectors back to every original citation. Forced rebuild bypasses persistent lookup but still avoids duplicate execution inside the batch. Static word-vector evaluations report zero neural forwards. Usage aggregates and the extension's measurement details preserve sample counts and distinguish missing values from measured zeroes. The rebuild-only format decision supersedes the original compatibility/migration proposal.

S1 qualification: focused preparation/corruption/budget tests and the provisioned MiniLM release test passed. The ten-input real-model fixture has four effective blank inputs and one duplicate: cold/full runs execute five neural forwards, warm runs zero, with exact serial vector and order parity. Four blank inputs already avoided the encoder before this change; prefiltering removes their scheduling/cache overhead, while deduplication removes one redundant forward in this fixture. No whole-repository wall-time speedup is inferred from these counts.

S1 checks passed: workspace formatting and clippy; 729 Rust tests with 9 ignored across 43 suites; Python 123 tests reported with 4 skipped; extension 82 passed with 3 skipped; repository and whitespace checks. Both ignored real-model qualifications were additionally run and passed. The separate 36-function CLI fixture preserved serial/parallel retrieval and indexed vector counts: 72 forwards on cold/full builds, then 72 cache hits and one forward after adding a file. Optimized compilation time is excluded from these runtime observations. Full graph reconstruction remains until S8.


S2 is committed as `5da9b0b`. The implementation uses the smallest fitting 32/64/128/256/512 bucket, capped by the declared model context. Tokenization runs once per executed input, reserves special tokens, and applies the validated single-sequence postprocessor once. Plans compile lazily from one parsed model template; at most eight plans and 1 GiB of retained declared tensor allocation are admitted, with separately bounded concurrent inference workspaces. A temporary serialized compilation can coexist with retained plans; the estimate is not a process RSS cap. Fully cached builds still report resident template declarations. Query plans are reused within an open model instance.

S2 enables genuine four-row tensors for full groups in the 32-token bucket only when the verified MiniLM model/tokenizer hashes and Windows x64 target match the qualification. Other shapes, models, platforms, queries and remainder inputs use B1. The complete embedding descriptor binds assets, preprocessing policy, runtime and actual OS/architecture; incompatible pipelines require a full rebuild. Eight-row execution remains a qualification experiment: longer shapes exceeded the declared graph bound and the short-input measurements did not justify selecting it.

S2 adds executed-input token counts, embedded and padding token counts, truncation counts, plan compilations/compile time and peak retained declarations. Cache hits are not retokenized to invent totals. Compilation time is nested inside inference time, which is nested inside publication time.

S2 qualification: 16 tokenizer boundary/Unicode/special-token cases match the former fixed-256 input IDs, masks and type IDs. Qualified bucket and B4 outputs are bit-exact against that reference, including reordered groups and padded tails. The ten-input cache fixture now executes two forwards for five distinct nonblank inputs; warm execution performs zero forwards, tokenizations or plan compilations. In a quiet release repeat, 32 short inputs with four workers had three-sample warm medians of 683.926 ms at fixed B1/256, 81.801 ms at bucketed B1/32 and 72.600 ms at B4/32. Bucketing reduced this fixture time by 88.0%; B4 reduced the bucketed time by another 11.2%. B8/32 measured 72.430 ms, while B8/256 exceeded the declared allocation bound, so B8 was rejected. Cold query plan compilation took 62.454 ms. These are small Windows x64 fixture measurements excluding loading, tokenization and compilation from the warm comparison, not corpus or cross-platform qualification.

The quiet release CLI fixture preserved 72 vectors and retrieval results with 18 forwards instead of S1's 72. Inference took 0.674877 s with one worker and 0.256442 s with four; the latter includes 0.063676 s plan compilation. Adding one file produced 72 cache hits and one forward. A subsequent blank-only change produced 73 cache hits, zero forwards/plans/tokenizations and a 98,655,744-byte template declaration estimate. Full publication still took about two seconds and rebuilt the graph, which remains S8 work. Obsolete pipeline rejection and a full rebuild were tested against isolated prior state; source bytes and saved collection/model configuration remained unchanged.

S2 checks passed: formatting and clippy; 732 Rust tests with 10 ignored across 43 suites; Python 123 tests reported with 4 skipped; extension 82 passed with 3 skipped; repository (667 public files) and whitespace checks. The ignored native batching/cache, CLI model and model publication/reopen/lifecycle qualifications were run explicitly and passed. License-notice generation completed for 467 packages and retained existing manual release-review flags for adobe-cmap-parser, htmlescape, pdf-extract and type1-encoding-parser, whose declarations lack separate license text. Runtime measurements and detailed results are in local ignored `target/indexing-s2-*` logs; they are not repository artifacts.

S3 is committed as `cdf4966` and qualified for the bounded fixtures below: semantic planning now covers complete body spans before embedding, with a provisional default of 256 tokens including specials and up to 32 tokens of written context, no overlap, at most 64 citation members and 16 KiB per input. Six deliberately similar long documents with distinct tails gave top-1 retrieval for 1/6 queries when clipped to the former source prefix and 6/6 with each tested window size. All window candidates had zero truncation and repeatable vectors. This fixture establishes tail coverage, not general relevance.

| S3 candidate | Semantic units | Actual forwards after exact batch reuse | Planning time | Warm inference median, three samples |
| --- | ---: | ---: | ---: | ---: |
| 128 tokens | 36 | 14 | 37.138 ms | 157.118 ms |
| 192 tokens | 24 | 9 | 31.278 ms | 261.776 ms |
| 256 tokens | 18 | 8 | 27.251 ms | 176.510 ms |

The 256-token candidate halves graph/mapping units versus 128 and reduces planning time by 26.6%, while planning plus warm inference is 4.9% slower in this small fixture. The 128-token candidate wins inference time; 192 offers neither the lowest time nor fewest units. The provisional 256 default therefore favors smaller downstream indexes while preserving the tested coverage. Compilation, model loading, graph construction and cache persistence are excluded from the inference comparison. Measurements used the provisioned local MiniLM model on Windows x64, with no concurrent test/build workload; evidence is in ignored `target/indexing-s3-native-planner.log`. S11 must still judge broader retrieval and end-to-end workloads.

The S3 release CLI fixture now represents the same 36-function source with five vectors instead of S2's 72. A separate blank file verifies blank accounting: six semantic inputs cover all 3,644 source bytes, five require inference, and none is truncated. Serial and parallel runs return identical evidence; adding a new file reuses five cached vectors and performs one forward, and a subsequent blank-only change performs no forward or plan compilation. Fewer vector nodes do not imply proportionally faster inference: these combined inputs use longer token shapes, and cold plan compilation remains material. In a quiet release repeat, inference took 0.516919 s with one worker and 0.304657 s under a four-worker ceiling (three used), including 0.126337 s of plan compilation in the latter. The parallel inference time was 18.8% higher than the earlier recorded S2 fixture run. Publication still took 1.86–2.06 s: S3 is a coverage/storage improvement, not a demonstrated parallel cold-indexing speedup. Complete-component rebuilding remains S8 work.

Retrieval qualification includes whole-body results for late sibling matches, occurrence-based hit limits, exact long-tail windows, scoped aliases, bounded shared-node fan-out, original page/slide/block/row distinctions, and nested/discontiguous mapped locations. A 16 KiB mapped body through four nested regions is captured within a shared 32-node work allowance. When a decoded source atom cannot be subdivided, a focused excerpt keeps its authenticated enclosing citation and explicitly reports truncation. A separately executed 4,097-node scope test passed; it remains ignored in the default suite because its debug run took 88.53 seconds.

The actual saved S2 CLI was used to create an isolated old index with an aliased source root, Essential detail and vectors disabled. The new build rejected ordinary indexing, rebuilt successfully with `index --full`, preserved both original sources and saved configuration bytes, and returned the expected lexical evidence. Current-format recognition also rejects old SQLite coordination that merely claims the new manifest label; receipt-bound identities are required. The reset creates a fresh incarnation and clears its durable marker only after verified publication.

S3 checks passed: workspace build, formatting, clippy, 768 Rust test executions with 12 ignored across 44 reported suites, repository checks (675 public files), and whitespace checks. Python reported 123 tests with 4 skipped; the extension passed 82 with 3 skipped. The new native semantic-planner qualification, release CLI serial/parallel/cache test, transformer publication/reopen test and large scoped-node qualification were run explicitly. Full workspace validation includes the final receipt-bound reset guard and revised whole-body expectations. Evidence is in ignored `target/indexing-s3-*` logs. Windows x64 results do not qualify other platforms or larger corpora; S11 remains required.


S4 is committed as `4827bc2`. Its implementation: an explicit version-one [vector selection policy](vector-selection.md) supports bounded conjunctions of path prefix, basename, suffix, adapter and language selectors. The default has no exclusions. Selection is captured in inventory version four and authenticated manifest counts; ordinary indexing applies policy-only changes, while metadata publications preserve the applied policy. Status and file information distinguish configured policy from committed eligibility. Excluded files retain lexical, fact and exact-read evidence. Saved enable/selection configuration survives the required full rebuild; prior derived formats are rejected.

Exact embedding keys now identify graph nodes across the complete update. Every positive occurrence remains in the independently bounded evidence map; deleting or moving the first occurrence does not designate or destroy a canonical source. Only exact model input bytes under the same complete embedding descriptor share a node. Successful no-vector outcomes are also remembered during the update; failed computations are not. Full rebuild bypasses the persistent vector cache but retains this transient deduplication. A private SQLite registry is quota-reserved, limited to one million rows and 128 MiB of pages, with a 16 MiB page cache and one uncommitted transaction. It is discarded with the update reservation and never published as a search index.

S4 accounting separates planned `vector_units`, `vector_reused_inputs`, distinct `vector_graph_nodes`, and policy-selected/excluded artifact counts. `embedding_inputs` and its cache/inference counters describe only distinct inputs actually submitted after registry reuse. `vector_registry_rows` includes successful no-vector results, while `vector_registry_bytes` measures allocated SQLite pages, not physical device I/O. The registry removes repeated per-update cache lookups, so S5 must measure any additional memo benefit against this new baseline rather than attribute the same saved lookup twice.


S4 qualification: the production mapping test retains 130 occurrences on one node, reports bounded unscoped fan-out, and retrieves a later occurrence through its path filter. Move/delete, distinct context/whitespace/pipeline keys, successful no-vector reuse, full bypass and registry failure tests pass. Selection tests preserve lexical/fact/read evidence, collection isolation, maximum-size saved policy, committed file-info validators, pending policy through metadata-only publication and failed indexing, and no-op behavior when equivalent rules are reordered. A forged outer receipt cannot authorize false selection counts or a false policy digest. The explicit 4,097-node scope qualification passed in 92.39 seconds.

The native MiniLM test passes for 40 identical files: one graph node, 40 retained occurrences, 39 registry reuses and one neural forward on both fresh and forced-full builds. Adding a 41st alias performs one cache lookup and zero forwards; excluding one basename removes only its semantic contribution and retains its lexical evidence and committed reason. The registry uses one row and 12,288 allocated bytes. The existing serial/parallel/incremental native test also passes.

Three alternating S3/S4 release comparisons on the same 40-copy fixture produced these medians. Both binaries used the same installed model and four-worker ceiling; each comparison used fresh isolated collection state. “Cold” means a new collection, not flushed operating-system caches. No other build/test workload ran during measurement.

| Measurement | S3 | S4 |
| --- | ---: | ---: |
| Graph nodes / retained occurrences | 40 / 40 | 1 / 40 |
| Graph file bytes | 72,356 | 3,560 |
| Cold neural forwards | 1 | 1 |
| Cold index wall time | 1.743 s | 1.694 s |
| Cold cache lookup time | 8.058 ms | 0.007 ms |
| Cold graph insertion time | 17.597 ms | 0.280 ms |
| Full-rebuild neural forwards | 40 | 1 |
| Full-rebuild inference time, including plan compilation | 439.561 ms | 76.383 ms |
| Full-rebuild wall time | 2.867 s | 2.447 s |

Graph bytes fell 95.1%; full-rebuild wall time fell 14.7%. The ordinary cold build already reused persistent embeddings across duplicate files, so it shows no forward reduction and only a small 2.8% wall-time difference. These duplicate-heavy fixture results do not establish a general corpus speedup. Original occurrences and scoped retrieval remain correct. Evidence is in ignored `target/indexing-s4-native-cli.log`, `target/indexing-s4-comparison.json` and `target/indexing-s4-scopes.log`.

S4 checks: locked workspace build, formatting, clippy, repository checks (680 public files), whitespace, Python (123 reported, four skipped), extension (82 passed, three skipped), and license notices (467 packages, unchanged manual release-review flags) passed. The complete workspace run covered 791 tests across 45 reported suites with 13 ignored: 790 passed initially, and one old-format lineage fixture failed after the intentional compatibility removal. That fixture was changed to express unsupported coverage in the current format; all nine lineage tests then passed. No runtime check was suppressed. The two ignored native CLI tests and the large scoped-node qualification were executed explicitly and passed. Windows x64 qualification remains local; broader relevance and platform/scale qualification belong to S11.

S5 is committed as `56e0ba7` and qualified for the bounded fixtures below. Positive embeddings now use [immutable binary packs](embedding-cache-packs.md) instead of individual JSON files. The numerical descriptor and exact input key remain unchanged; the retrieval engine format changes and prior indexes require a full rebuild. A receipt-authenticated version-two sidecar binds live keys, physical records, pack digests, model pipeline and dimensions. A verified small model manifest supplies cache metadata without adding another heavyweight weight-file hash; normal model opening still verifies assets. The S4 input registry already eliminates repeated equal-key lookups, so S5 adds an authenticated pack-index cache, not a second vector memo.

Packs preserve raw float bits and the original numerical checksum. Lookup verifies the header/index and selected record; finalization fully verifies newly sealed packs, while maintenance verifies unique surviving packs within its separate work budget. Full rebuild recomputes results and verifies exact equality before reusing stored records. Private writes are quota charged, then sealed and promoted without overwriting an object. Sparse packs compact below 50% live records; pack-count pressure compacts the live set in physical pack order. Independent bounds cover records, packs, manifest size, handles, decoded index allocation and cumulative application reads. Large fragmented builds can hit the existing 256 MiB read cap during lookup plus compaction plus verification, even when warm lookup alone fits. Explicit failure preserves the committed index; the bound is not silently removed.

Cache reports now count application opens/bytes, overlapping record/index/full verification work, syncs, decoded-index hits/peak allocation, sealed/reused packs and compaction records. `embedding_cache_finalize_us` includes persistence work moved out of individual stores; compare it together with `embedding_cache_store_us`. These counters end at cache finalization and exclude later publication/maintenance I/O, which remains included in wall time. They measure neither physical device I/O nor process RSS.

S5 regular checks passed: locked workspace build, formatting, workspace clippy, 813 Rust tests across 45 reported suites with 14 ignored, repository checks (685 public files), whitespace, Python (123 reported, four skipped), extension (82 passed, three skipped), and license notices (467 packages with unchanged manual release-review flags). Focused cases cover exact float bits and signed zero, altered header/index/records, unused-record maintenance checks, pipeline/dimension mismatch, quota/read-bound failures, partial writes and dead owners, sparse/pack-pressure compaction, pending publication, old readers, foreign collections and relocation. The actual S4 CLI created an isolated obsolete JSON-cache index with an alias, Essential extraction and an explicit vector exclusion. S5 rejected ordinary indexing, rebuilt it into packs with `index --full`, and preserved source bytes and all saved configuration bytes. The excluded alias retained its reason and lexical evidence. All three ignored native CLI qualifications and the transformer publication/reopen qualification were additionally executed and passed.

The new native MiniLM pack fixture has 64 distinct positive inputs. Its cold build wrote one 107,088-byte pack with two adapter opens and one sync. A rename reused 64 inputs with zero forwards, one open, 63 index-cache hits and no pack writes. Editing one file reused 63 inputs and performed one forward; a forced full build performed 64 forwards, compared all results against the two existing packs, and wrote no pack. Scoped retrieval and source bytes remained unchanged by indexing. Cold persistence took 22.028 ms including 8.102 ms finalization; warm rename lookup read 107,088 application bytes. These are single qualification observations, not a general MiniLM speedup claim.

Three alternating quiet S4/S5 release comparisons used 256 distinct text inputs and an original dense 384-dimensional static word-vector model. This isolates cache storage without neural execution. Each repetition used fresh isolated collection state, followed by forced full indexing and a rename; the same scoped result and all original source bytes were checked throughout. Operating-system caches were not flushed. Both binaries used the same fixture and worker ceiling.

| S5 cache comparison, median of three | S4 JSON objects | S5 binary packs |
| --- | ---: | ---: |
| Cache object count / bytes | 256 / 1,231,520 | 1 / 428,112 |
| Cold store plus finalization time | 405.784 ms | 52.049 ms |
| Forced-full store plus finalization time | 100.540 ms | 5.720 ms |
| Warm-rename lookup time | 55.475 ms | 4.727 ms |
| Cold total wall time (range) | 2.546 s (2.501–3.433) | 2.210 s (2.205–2.247) |
| Forced-full total wall time (range) | 1.199 s (1.191–1.261) | 1.152 s (1.143–1.172) |
| Warm-rename total wall time (range) | 1.081 s (1.055–1.110) | 1.063 s (1.050–1.094) |

The fixture uses 65.2% fewer cache bytes and spends 87.2% less time in cold cache persistence, including finalization. Cold total wall time falls 13.2%, but warm wall-time ranges overlap and support no strong latency claim. S5 cold cache counters report two opens, one sync, 428,192 written bytes (including the header rewrite), and 445,600 read bytes (including verification). Forced-full and warm-rename reuse each open one pack, read 428,112 bytes, obtain 255 index-cache hits and write zero cache bytes. S4 did not expose these I/O counters; absent values were not treated as zero. Full graph/catalog/Tantivy reconstruction and model authentication remain material and are addressed by later stages. Evidence is in ignored `target/indexing-s5-*` logs, with reusable native assertions in [the CLI qualification test](../crates/airk-cli/tests/vector_performance.rs). Windows x64 and these small fixtures do not qualify larger-than-memory workloads or other platforms.

S6 is committed as `b7eea4b`. It moves selected detail into qualified producers before expensive optional evidence construction. Nineteen common grammar profiles preserve the full-analysis selected payload at all three detail levels; Balanced skips ordinary call evidence while retaining framework calls, and Essential still parses and walks definitions for independent semantic boundaries but skips rich evidence and continuity fingerprints. Markdown/plain prose, native page/slide text and composite children have corresponding qualified paths. Unit-only prose continuity no longer constructs discarded facts. DOCX tables, spreadsheets and specialized language/repository adapters retain full extraction followed by selection where early selection has not been qualified. Original text, mapped locations, parse status and semantic coverage remain required. Local syntax IDs are normalized separately from fact IDs, including shared syntax nodes and non-emitted owners; missing selected owners fail explicitly. Transient semantic seeds never become a second persisted fact representation.

S6 also moves fresh extraction and payload preparation into a lazy, bounded pool. Protected source reads/hashes, reuse admission and retention remain coordinated in captured discovery order; no directory-wide filename sort or cross-run ordering guarantee is introduced. At most four jobs are outstanding across queued, running and completed work, with a 4 MiB per-job source limit and 16 MiB total admitted input ceiling. Native PDF/DOCX/XLSX/PPTX and larger sources execute serially after earlier jobs drain. Cancellation closes channels and joins bounded work; earlier failures retain their ordering. Every admitted source is still read and hashed, including warm reuse and same-size edits with restored timestamps. Watch events remain hints; this stage does not replace reconciliation with timestamps.

`AIRK_EXTRACT_WORKERS=1..4` controls requested concurrency independently of extraction/reuse identity; the default remains one because the measured gains vary by workload and large-payload memory has not been qualified. Kernel admission uses one eighth of detected memory, capped at 4 GiB, with a 512 MiB allowance when unknown. It reserves 256 MiB for the coordinator and 580 MiB per worker (two serialized-result allowances, input and parser allowance); a request for four admits one/three/four on 8/16/32 GiB machines. This is a conservative heuristic, not an RSS cap: Rust objects and parser expansion do not equal serialized size. A completed payload is validated against the 256 MiB serialized limit before entering the result channel. Ten extraction metrics distinguish requested/admitted/spawned workers, serial/parallel jobs, summed job wall time, coordinator wait, source peaks and encoded result capacity. Timers overlap and must not be summed as CPU or total elapsed time; aggregated peaks/capacities are not process memory measurements. See [ingestion limits](ingestion-limits.md).

S6 requires rebuilding derived state: `native-facts/9`, adapter suffix `/sizing-1/detail-2`, and retrieval engine `generation-9/.../detail-projection-2` prevent old payload reuse. Chunk version 3 and semantic-projection schema 1 retain their meaning. Current standalone contract-1 and composite contract-2 payloads remain supported representations of current extraction; this is not an old-index compatibility path. The existing full-reset protocol preserves source and saved configuration.

S6 regular checks passed: locked workspace build, formatting, clippy, 839 Rust tests across 46 suites with 17 intentionally ignored, repository checks (691 public files), whitespace, Python (123 reported, four skipped), extension (82 passed, three skipped), and license notices (467 packages with the same existing manual release-review flags). New tests cover complete selected-payload parity, independent boundary capture, shared syntax owners, all-detail one/two/four-worker parity, source admission, ordered earlier failures, cancellation, native/large-file serial dispatch, and same-size edits with restored timestamps. The first standalone ingest run exposed an old test fixture that reconstructed raw facts from already prepared chunks; it now captures the actual neutral payload for reuse, matching production. No production compatibility path or runtime assertion was added to accommodate that fixture.

S6 isolated release qualifications passed after all builds and regular tests stopped. The syntax fixture contains 150 TypeScript definitions and 3,150 calls: seven-sample medians are 60.477/28.895/23.885 ms for Detailed/Balanced/Essential, with 3,300/300/0 rich-evidence constructions and 150/150/0 fingerprint sets. These compare selected levels, not an old-versus-new whole-index speedup. In the 131,416-byte prose fixture, ten warmed Markdown unit-only iterations took 5.165 ms versus 154.586 ms for full facts plus units; plain-text iterations took 3.431 versus 152.551 ms. Essential heading-only work took 3.770/2.024 ms for Markdown/plain text versus 154.565/149.244 ms for full facts. Complete output parity was checked separately; these operation-level savings do not imply equivalent end-to-end gains.

The model-free worker qualification uses 96 files, 48 Rust definitions or 36 Markdown sections per file, five alternating samples at each count, identical payload hashes and warm full-source hashing. It excludes durable store/index construction:

| Corpus / median | One worker | Two workers | Four workers |
| --- | ---: | ---: | ---: |
| Code cold scan/extraction | 488.241 ms | 309.384 ms | 237.328 ms |
| Code warm reuse/hash | 63.932 ms | 62.688 ms | 62.068 ms |
| Prose cold scan/extraction | 104.022 ms | 58.378 ms | 59.441 ms |
| Prose warm reuse/hash | 51.728 ms | 53.158 ms | 52.388 ms |

The end-to-end comparison uses three alternating S5/S6 fresh-process samples per detail/corpus, 48 TypeScript files with 24 definitions and 13 calls each, or 48 Markdown files with 48 requirement sections each. No embedding model is installed, isolating extraction and publication from inference. Scoped lexical results and source bytes are checked; every cold/full run extracts 48 files and every unchanged warm run extracts zero. Full forced rebuilds were also checked once per configuration, which is insufficient for a repeated-performance claim. Windows process peak working set was sampled every 5 ms; this is an observed process metric, not the admission heuristic or a portable RSS bound.

| Corpus / detail | S5 cold median (range), seconds | S6 serial cold median (range), seconds | S6 requested/admitted four, seconds |
| --- | --- | --- | --- |
| Code / Essential | 0.798 (0.790–1.287) | 0.550 (0.515–0.556) | Not measured |
| Code / Balanced | 1.545 (1.525–1.564) | 1.332 (1.315–1.376) | 1.154 (1.146–1.160) |
| Code / Detailed | 2.583 (2.481–2.605) | 2.631 (2.620–2.649) | Not measured |
| Prose / Essential | 0.435 (0.429–0.672) | 0.388 (0.386–0.393) | Not measured |
| Prose / Balanced | 0.602 (0.585–0.604) | 0.613 (0.589–0.615) | 0.589 (0.551–0.624) |
| Prose / Detailed | 3.957 (3.956–3.984) | 4.068 (4.023–4.082) | Not measured |

Serial cold code improves 31.1% at Essential and 13.8% at Balanced; four workers improve the Balanced fixture another 13.3%. Essential prose improves 10.9%. Balanced prose ranges overlap; Detailed cold medians are 1.8%/2.8% slower for code/prose, so this stage does not claim a universal Detailed improvement. Warm medians remain close, as reused files do no extraction. Balanced code sampled peak working set is 36.29/36.47/36.74 MiB for S5/S6 serial/S6 four; prose is 32.35/32.11/33.94 MiB. These small fixtures do not qualify near-limit parser/native expansion or justify a general parallel default. The bounded pool remains opt-in; native expansion stays serial.

All three native MiniLM CLI qualifications passed (37.81 seconds), including serial/parallel inference, exact aliases, packed-cache reuse, forced rebuild and scoped retrieval. An actual S5 index was rejected with an actionable rebuild message, then rebuilt by S6 with both sources and the saved root/extraction/vector/retention configuration bytes preserved. Evidence is in ignored `target/indexing-s6-*` logs; repeatable selected-work and worker tests live in ingest. Windows x64 is the qualified local platform; broader workload/platform qualification remains S11.

S7 is complete as an isolated foundation. The [current-index foundation](current-index-foundation.md) uses the actual DiskANN algorithms over authenticated fixed-size records in local SQLite. This makes graph records, their Merkle paths and the current head one recoverable transaction; it does not replace DiskANN with a SQL vector scan. Private quota-backed overlays coalesce repeated neighbor writes into exact final afterimages. Stable physical IDs, live tombstones and bounded search refill separate graph traversal from current-result eligibility. Authentication binds scope, incarnation, graph instance/specification, ID, exact input key, live state and complete bytes.

The request gate excludes readers during publication. Prepared retains a readable base; Applying remains unavailable after process death until captured replay completes. Durable quota allowances survive staging cleanup. Owner/bootstrap headers permit bounded recovery before SQLite opens a hot journal, and current metadata uses a new format with no compatibility reader. The S7 foundation remains isolated: S8 must integrate catalog/lexical/vector changes, authenticated candidate mappings, durable database growth, all request routes, GC/reset and historical API removal before claiming the one-current-index runtime is complete. Process-death qualification is distinct from power-loss and other-platform qualification.

S7 qualification and local commit `d6cf9f9`: real DiskANN/store integration inserted 96 vectors, appended 10 with stable IDs, then committed an exact one-record tombstone delta without copying the graph; three synthetic ten-neighbor queries each retained at least nine brute-force neighbors. A separate test deleted the 80 closest of 106 nodes, recovered captured graph afterimages after private staging was gone, refilled ten live results across multiple passes, and resurrected the same physical ID. Store tests cover corrupt records/proofs/pending rows, missing or foreign ownership, quota failure, process death before/during initialization, independent reader/writer gates, Applying unavailability, and a flushed hot rollback journal before exact replay. Workspace build, formatting, clippy, repository (712 public files), whitespace, Python (123 reported, four skipped), and license notices (467 packages, unchanged four manual flags) passed. The locked workspace run passed 873 tests, zero failed and 18 ignored across 47 suites. No production indexing speedup, power-loss durability, other-platform behavior or release timing result is inferred from this foundation; the ignored graph timing test was not run.

S8 delivery is split into reviewable local commits. First, permit quota-admitted current database growth and test crash boundaries. Next, authenticate the current per-artifact catalog and exact-key/occurrence mappings, including proof of absent keys and bounded broad reads. Then update only affected Tantivy documents with stable lexical identities and a replayable commit. Finally, connect graph, catalog and lexical deltas through all kernel read/index routes, adopt the rebuild-only format, remove historical capabilities, qualify old-token expiry, and update CLI/MCP/extension contracts. A substep does not mark S8 complete; the live runtime changes only after all components and routes are coherent. Whole-component hash assumptions must be replaced by bounded authenticated candidate reads before the switch.

S8 capacity substep: the current writer now reserves the larger database limit and rollback allowance, syncs a monotonically larger owner header, reopens SQLite at that limit, and retires its unused grant. Quota pressure leaves the old limit/head readable; Prepared cannot grow; a killed writer before or after the header replacement leaves exactly the old or new bound with an adoptable orphan allowance. A subsequent transaction at the new limit committed and expired its old token. The live runtime is not yet using this path. Locked workspace build, formatting, clippy, repository checks (712 public files) and whitespace passed; the workspace run passed 876 tests, zero failed and 18 ignored across 47 suites. The three focused capacity tests passed, including the killed-process boundary. Capacity growth is a prerequisite for S8, not a user-visible indexing speed claim.

## Recommendation

Keep exact source evidence, useful structural facts, and vector retrieval, but stop making their granularity and publication cost inseparable. Prioritize:

1. **Token-aware vector units independent of citation chunks and continuity units.** Combine undersized related fragments, attach their written heading/signature context, and split oversized passages before embedding. Preserve fine source locations and lexical/fact retrieval independently.
2. **Length-bucketed inference, then real tensor batching.** The baseline ONNX adapter ran one input at a time at a fixed length of 256, including very short inputs. S2 implements bounded buckets and qualified tensor batches; four-worker scheduling already existed before this work.
3. **Keep one current index and update its changed components.** Through S3, a one-file edit still reconstructs the entire Tantivy index and DiskANN graph. S7/S8 replace retained searchable generations with incremental updates, a collection-wide multiprocess read/write gate, and durable recovery. Preserve lightweight revision identity and exact current evidence; let source control own source history. This changes availability and historical-read guarantees; O6 defines the replacement contract.
4. **Make vector eligibility and detailed enrichment explicit policies.** Lockfile records, repetitive boilerplate, and some generated data are good candidates for lexical-only treatment. Do not remove tests, configuration, or short declarations indiscriminately.
5. **Offer explicit staged readiness if time to first usable evidence matters more than time to full hybrid readiness.** Make the current lexical/fact index usable first, then add compatible vectors through further committed updates of that same index. Expose pending/partial coverage; silently mixing stale vectors with changed citations is unacceptable.

The baseline evidence identifies excessive short-input padding and the all-corpus rebuild after small edits. Lower detail already reduces storage, but its old coupling to vector boundaries caused a retrieval regression when chunks were blindly enlarged. S3 separates semantic planning from detail selection; S7/S8 address the remaining complete-component rebuild.

**Design decision:** O6 now recommends a single current logical index, replacing the earlier immutable-generation/shard recommendation. Reducing retention to one generation alone would save historical storage but leave the full rebuild path intact. The speed improvement depends on changing that path to update only affected records and graph nodes. Historical descriptions and measurements below still describe the existing implementation; none measures the proposed architecture.

## Evidence and scope

AIRK MCP was used first for inventory, search, outlines, exact code facts, bounded reads, and freshness/coverage inspection. The initial collection observation was:

| Item | Observed value |
| --- | --- |
| Files / source bytes | 641 / 6,172,458 |
| Chunks / indexed vectors | 15,322 / 10,366 |
| Published extraction detail | Detailed |
| Retrieval | Tantivy + Microsoft DiskANN 0.59.0, model ready, vectors enabled |
| Imported facts / relationships | 0 / 0 |
| Exact revision | `airk-r1:b261827304de8a26854c8953dd744bdf:70:fb39d0a14c6ce68d058259313d6e98b89c23b545944ea4704aee1a2e23351912` |

`airk_changes` reported idle scheduling with requested/completed watermark 131, but explicitly `live_source_verified=false`. Its file-history coverage had an overflow boundary after generation 42; code/document coverage reported unsupported after generation 70. These are independent qualifications, not proof that the live workspace is complete or unchanged. No cursor remained for that request.

The bounded read of `crates/airk-retrieval/src/persistent.rs`, lines 328–383, returned validator `02fae7e11fbda7045cd9b089b6f8f79f512335168bc1317a708cf47a2ec596a4` at that revision. Live SHA-256 was `0f162165cfab6f7e5769403d6a07be5d2436d43b0d7c6ffa1f7c8b24c5708472`, matching indexed evidence. The live vector provider also matched its indexed hash, `8165bde0a990674a0dbab5972b0471ed5bab979d0aae603ac4ef98152e84121f`. Other affected implementation paths were inspected directly before writing this review.

The collection advanced during the session. The later exact definition lookup for `build_streaming_components` returned lines 262–454 at `airk-r1:b261827304de8a26854c8953dd744bdf:71:7df95c20d9b896821faaa5dc3ed9b378eaadc5829ecb8b4bf0ba5eea87006166`. Counts below from isolated experiments must not be attributed to either live revision. No production collection settings were changed by the experiments.

### Retrieval interactions that inform the recommendations

With prefix `crates/airk-retrieval/`, query **avoid recomputing unchanged embeddings**, limit 5, and a 3,500-byte budget at generation 70:

| Mode | Observed evidence |
| --- | --- |
| Lexical | No hits: all normalized query terms must match |
| Vector | Cache-related passages in `model.rs` and `embedding_batch.rs`, including the batch embedding implementation |
| Hybrid | The same useful passages; response budget marked truncation |

This is a concrete reason to retain natural-language vector discovery. Conversely, exact `airk_code_facts` found the named builder directly, without semantic search. A bounded outline initially spent much of its budget on imports; focused definition lookup was more efficient. These interactions favor separate retrieval routes and granularity controls, not deleting structural evidence or assuming that every useful fact needs a vector. They are illustrative queries, not a judged relevance benchmark. Concurrent MCP timings were not used as performance measurements.

## What the baseline code indexes

The authoritative dependency boundaries remain those in [architecture](architecture.md). No external database, agent reasoning, orchestration engine, or cloud inference is needed for these proposals.

| Stage and implementation | Baseline inputs and output | Work on an ordinary update |
| --- | --- | --- |
| Discovery: [ingest/lib.rs](../crates/airk-ingest/src/lib.rs), `discovery`, `scan_observed_profiled` | Owner root plus aliased roots; ignore rules including `.airkignore`; protected, capped source reads; SHA-256; profile selection | Walks and reads/hashes eligible candidates to establish content identity. Reuse is not an mtime-only shortcut. Unsupported candidates still consume applicable discovery/read budgets. |
| Extraction: [documents.rs](../crates/airk-ingest/src/documents.rs), native syntax and format adapters | UTF-8 text, selected grammar facts, document/configuration facts, original locations, mapped composite regions, diagnostics | Reuses verified source/profile-compatible extraction unless full rebuild or invalidation requires extraction. Normal UTF-8 fallback is not restricted to recognized extensions. |
| Detail/continuity: [detail.rs](../crates/airk-ingest/src/detail.rs), [continuity/mod.rs](../crates/airk-ingest/src/continuity/mod.rs) | Optional facts/edges are selected; code/document continuity descriptors and chunk hints are constructed | Selection happens after raw extraction, before continuity and durable payloads. Lower detail does not currently eliminate all upstream parsing/fact construction. |
| Partition: [chunking.rs](../crates/airk-ingest/src/chunking.rs) | Exact host-text partitions, declaration/document hints, mapped child chunks | Maximum 8 KiB and 160 lines per host chunk. Gaps, including whitespace, remain represented. UTF-8 and CRLF stay intact. Native document locations constrain boundaries. |
| Retained objects: [blobs.rs](../crates/airk-store/src/blobs.rs) | Content-addressed source bytes and path-free extracted payloads; compact inventory binds paths, roots, profile and payload hashes | Reuses authenticated objects; preserves exact indexed reads independent of live source edits. |
| Lexical: [persistent.rs](../crates/airk-retrieval/src/persistent.rs), `build_streaming_components` | One Tantivy document per chunk: normalized terms from path + chunk text; path; ordinal; optional vector ID; serialized hit/citation/text | Rebuilds for a changed publication using a single Tantivy indexing thread and 20 MB writer budget. |
| Embeddings: [embedding_batch.rs](../crates/airk-retrieval/src/embedding_batch.rs), [onnx.rs](../crates/airk-retrieval/src/onnx.rs) | Chunk text only, local model, tokenization and normalized vector | Groups at most 32 chunks from one artifact; checks cache; runs misses through bounded workers. This is worker batching, not a multi-input ONNX tensor. |
| DiskANN: [vector.rs](../crates/airk-retrieval/src/vector.rs) | One vector node for each chunk returning an embedding | Sequential insertion into a new private graph, including cached embeddings; full-precision vectors and adjacency are stored on disk. |
| Exact catalog: [sqlite/publication.rs](../crates/airk-store/src/sqlite/publication.rs) | Normalized facts/relationships, inventory/lineage/continuity, source-context sizes and compatibility views | Can copy unchanged native rows from an authenticated, leased prior catalog, but still writes a new catalog and constructs indexes. |
| Publication: [kernel/publication.rs](../crates/airk-kernel/src/publication.rs) | Inventory, model references, engine specification, receipt, retained components | Sync, digest verification, reopen validation, immutable rename, and atomic active-pointer commit; quotas, recovery and retention remain enforced. |

Built-in exclusions include `.git`, `.airk`, `node_modules`, `target`, `.venv`, `__pycache__`, `.aws`, `.ssh`, `.env` variants and selected key files; configured data state is excluded. This repository also ignores `temp/`, build artifacts and other private/generated material through its Git ignore rules. Hidden files are not universally excluded. Lockfiles, tests, documentation, licenses and ordinary configuration can therefore be indexed when otherwise eligible. Preserve existing exclusions and source read-only behavior.

Document extraction includes text-layer PDF, DOCX paragraph/table evidence, XLSX stored rows/cells, PPTX slide text, Markdown, delimited data and other adapters. Spreadsheet formulas are retained, not executed. OCR and imported producer analysis remain explicit, separately attributed inputs. See [formats](formats.md) for the exact supported surface; native syntax is not resolved compiler semantics.

### Baseline detail is coupled to chunking

| Level | Selected evidence | Chunking consequence |
| --- | --- | --- |
| Detailed | Supported facts including calls, code continuity, document-unit continuity | Declaration hints plus document-unit hints; often many small chunks and intervening gaps |
| Balanced | Removes generic `call` facts/edges, keeps other supported structural/document facts and code continuity | Still has declaration hints; does not generate Detailed document-continuity hints |
| Essential | Removes optional facts/relationships and continuity | Partitions host text without declaration/document-continuity hints; original native document boundaries still constrain chunks |

Balanced retains supported framework categories other than the generic `call` category; it is not simply “definitions and nothing else.” Code hints favor outermost declarations, so nested methods can remain facts within a larger containing chunk. Essential does not mean token-aware semantic chunking. The legacy 40-line helper in ingest is not the final v2 publication chunk contract.

### What the baseline adds to DiskANN

For each retained artifact, the builder passes every chunk's **text** to the embedding adapter. It does not separately embed each SQLite fact, symbol, relationship, continuity fingerprint, path, source hash, or citation. A fact can influence boundaries; its serialized record is not itself an embedding input.

The provisioned model in these tests is all-MiniLM-L6-v2, 384 dimensions, WordPiece, maximum 256 tokens including special-token processing, attention-mask pooling and L2 normalization. The ONNX adapter:

- Truncates within its byte bound, checks whitespace-only input, then tokenizes with maximum length 256 and fixed padding to 256.
- Runs a tensor of shape `[1, 256]`; masked output pooling does not remove padded positions from the encoder's computation.
- Returns no vector for whitespace-only text. Static word-vector models can also return no vector when no vocabulary terms match; their costs are different.
- Stores successful embeddings in a collection-local cache keyed by exact input bytes and the complete model/tokenizer/preprocessing/pooling/normalization/inference descriptor. Identical text can reuse inference across paths, while each citation still gets a separate graph node today.

The graph contains a reserved node zero and sequential public node IDs starting at one. Each AIRK record has `4 * (dimensions + 48 + 1) + 32` bytes: 1,764 bytes at 384 dimensions, plus the file header/reserved node. The adjacency capacity is 48; the configured construction parameters also include 32 and 64. Record reads seek through a mutex-protected file, allocate/decode a record and verify its checksum; adjacency updates rewrite records. There is no per-node `sync_all`: graph finalization syncs the completed graph. Removing fictitious “per-vector fsync” is therefore not an applicable graph optimization. Individual embedding-cache object creation does have separate persistence costs.

There are three different kinds of reuse:

1. Extraction reuse avoids parsing unchanged source/profile inputs.
2. Embedding reuse avoids inference for unchanged exact chunk text and pipeline.
3. Catalog row reuse avoids rehydrating/rebinding some unchanged native facts.

None currently gives changed publications full Tantivy-segment or DiskANN-graph reuse. A true no-op index already avoids rebuilding: [kernel/lib.rs](../crates/airk-kernel/src/lib.rs) checks whether publication is needed and otherwise pins/verifies the prior revision. A full reindex intentionally bypasses extraction and embedding lookup. Model changes, detail/profile changes and other publication changes have their own invalidations.

### What has to happen synchronously?

For a coherent complete hybrid publication under the current contract, all selected components must be ready and authenticated before switching the pointer. Discovery/content identity, selected source extraction, citation binding, quota admission and commit verification cannot be omitted while still claiming that same result.

However, semantic embedding and graph construction are not prerequisites for exact file reads, lexical search or structural queries. Individual call evidence and document-unit continuity are not prerequisites for those basic routes either. They can be reduced by explicit policy, or built later under a new staged-publication contract. Expensive OCR/producer work should remain explicit. Delaying work improves time to first evidence; it does not automatically reduce total CPU or disk work.

Under the proposed single-current-index contract, prepare extraction and embeddings while readers continue using the current state, then gate reads during the cross-component apply/commit. Do not hold that gate throughout ordinary inference. Every response must still describe one coherent committed state. A failed partial update may require a recovery pause or rebuild instead of serving a retained prior generation. Full rebuilds and model changes may take longer pauses; short update pauses are a measurement target, not an existing guarantee.

## Experiments performed

Built the baseline optimized Windows x64 CLI with `cargo build --release --locked -p airk-cli`. Binary SHA-256: `613014cf1f1ae2c9378ad3a61f31bc96b6148070e9e114ed922f279cc9b438dd`. The host exposed 64 logical CPUs; inference used at most four workers. Existing licensed local model assets were used without downloads. Runtime usage recording was disabled in the experiment processes.

These are single-machine exploratory measurements. Processes were fresh, OS caches uncontrolled, and the workspace test suite was also running during the initial indexing experiments. They are not isolated latency qualification, physical SSD measurements, or cross-platform claims. Counts and correctness outcomes are more transferable than the exact times. Stage timings overlap: `publish_us` includes embedding/graph work and other publication work; do not add it to its children. Wall time also includes startup, setup/verification and maintenance outside the displayed stage counters.

### A. This repository: three detail levels, vectors deliberately off

Each level used a fresh isolated collection. The corpus had **642 eligible files / 6,184,957 source bytes**, including the then-current measurement harness but not this document. It differs from the earlier MCP publication. No live/private index was copied. Token counts were computed from the newly generated retained payloads using the local tokenizer without truncation or padding.

The sorted path/source-hash list has aggregate SHA-256 `6ca2b9ac6d11e20878a5a59ab8f43dce7d195f682aa4f6f48dd18e139a85da61`, using the harness's compact JSON encoding. This identifies the measured corpus without reproducing retained source or inventory contents.

| Measurement | Detailed | Balanced | Essential |
| --- | ---: | ---: | ---: |
| Published chunks | 15,331 | 8,448 | 1,364 |
| Nonblank vector candidates, not built vectors | 10,372 | 6,530 | 1,364 |
| Blank chunks | 4,959 | 1,918 | 0 |
| Distinct exact nonblank inputs | 8,559 | 5,074 | 1,342 |
| Facts in catalog | 65,979 | 16,672 | 0 |
| Relationships in catalog | 101,867 | 5,443 | 0 |
| Payload continuity descriptors | 13,466 | 6,211 | 0 |
| Median nonblank token length, specials included | 61 | 54 | 1,464 |
| Nonblank inputs at most 32 tokens | 3,734 | 2,561 | 33 |
| Nonblank inputs exceeding 256 tokens | 1,457 | 1,587 | 1,144 |
| Estimated unused fixed-256 input slots | 65.2% | 59.7% | 8.6% |
| Cold wall / scan-extract / publish, seconds | 16.197 / 7.359 / 8.792 | 10.348 / 5.974 / 4.328 | 6.477 / 5.315 / 1.115 |
| No-op wall, seconds | 4.817 | 2.582 | 0.773 |
| Logical state bytes after cold publication | 251,955,255 | 103,110,550 | 19,878,818 |

The padding estimate is `1 - sum(min(unpadded_tokens, 256)) / (nonblank_inputs * 256)`. It describes encoder slots, not measured wasted FLOPs or a predicted wall-time reduction. Untruncated tokens beyond 256 are not embedded by the current model. Mapped child text can overlap host text, so total chunk bytes need not equal source bytes.

Balanced reduced nonblank candidates by **37.0%**, but increased the over-context fraction from **14.0% to 24.3%**. Essential's over-context fraction was **83.9%**. The 1,813 duplicate nonblank Detailed inputs represent a **17.5% upper bound on node-count savings from exact-input graph deduplication**, before mapping/filter/ranking costs. Embedding-cache reuse already captures part of the inference opportunity. These effects cannot be multiplied into a claimed speedup.

Detailed candidates by path category:

| Category | Nonblank candidates | Interpretation |
| --- | ---: | --- |
| Code/configuration/other | 4,412 | Includes inline tests not identifiable by path alone |
| Documentation and Markdown | 3,104 | Useful for conceptual/requirements retrieval; avoid wholesale exclusion |
| Test directories (`/test/`, `/tests/`) | 2,366 | Behavior examples are useful; consider merging fixture fragments rather than deleting tests |
| Cargo/npm lockfiles | 490 | Strong lexical-only policy candidate; exact dependency/version retrieval must survive |

`Cargo.lock` alone contributed 477 nonblank chunks. Other high-count files included `packages/vscode/src/test/trust.test.ts` (416), `docs/architecture-v2.md` (208), and `docs/architecture-v2-ledger.md` (192). A blanket test exclusion would remove a substantial portion of behavioral evidence. Excluding lockfiles from vectors alone would remove only about 4.7% of candidates here, not solve indexing by itself.

Balanced reduced documentation candidates from 3,104 to 190, while test-directory candidates barely changed (2,366 to 2,345). This explains why an extraction-detail switch is a blunt semantic-index control. Essential still spent 5.315 seconds in scan/extraction: reducing persisted facts does not remove every raw parser cost.

### B. Real native ONNX/DiskANN indexing on an original fixture

Nine files, 12,239 bytes: four Markdown topic documents, four Rust files with twelve documented functions each, and JSON configuration. Topics were account recovery, bread, embedding reuse and atomic publication. Each detail/vector combination used fresh state.

| Detail | Chunks / vectors when enabled | Cold vectors off | Cold vectors on | No-op vectors on | Add one file, vectors on | Delete that file, vectors on |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Detailed | 201 / 149 | 0.311 s | 6.060 s | 0.833 s | 2.555 s | 2.474 s |
| Balanced | 105 / 101 | 0.215 s | 4.805 s | 0.825 s | 2.464 s | 2.375 s |
| Essential | 9 / 9 | 0.193 s | 2.394 s | 0.807 s | 2.336 s | 2.265 s |

Detailed full rebuilds, both with lookup bypassed, took **14.960 seconds with one worker** and **6.516 seconds with four**. Their inference phases were 12.471 and 4.020 seconds; graph insertion was 0.118 and 0.120 seconds. This confirms existing worker parallelism is useful; it does not establish a new optimization or a production scaling curve.

Adding one 48-byte file reused all nine original artifacts and **149 cached vectors**, but still inserted the complete graph. Inference fell to **0.091 seconds**, graph insertion remained **0.119 seconds**, model load was **0.276 seconds**, and publication totaled **2.009 seconds**. Deleting the added file had 149 cache hits, about **0.0014 seconds** of embedding-call time, **0.118 seconds** graph insertion, and **1.936 seconds** publication. Substantial cost remained outside neural inference.

The addition reported 53 “inferences,” but 52 were blank chunks returning no vector before model execution. This counter measures calls, not necessarily neural forwards. Separating attempted inputs, empty outcomes and actual forwards is part of the telemetry plan.

Across the four topic queries, vector and hybrid top results were on the expected topic at all three levels. Lexical retrieved the explicit bread/publication terms but missed the two paraphrased queries. This easy, repetitive fixture supports keeping vectors, but cannot establish that Essential preserves quality on mixed or long files.

### C. Adversarial long-tail retrieval

Two Markdown files had the same introductory sentence repeated sixty times, followed by different final paragraphs: bread preparation in `a_bread.md`, password recovery in `z_account.md`. The shared prefix exceeded the model context but fit within a normal source chunk. Query: **Recover account access after forgetting a login secret**, vector mode, limit 1.

| Level | Chunks / vectors | Vector top hit | Lexical and hybrid top hit for `password recovery` |
| --- | ---: | --- | --- |
| Detailed | 6 / 4 | `z_account.md`, correct | `z_account.md` |
| Balanced | 2 / 2 | `a_bread.md`, incorrect | `z_account.md` |
| Essential | 2 / 2 | `a_bread.md`, incorrect | `z_account.md` |

At the coarser levels, the distinguishing tail was outside the embedded prefix. The different full texts still had different cache keys; equal encoded prefixes produced indistinguishable semantic inputs. **Do not adopt one vector per file or simply raise chunk size.** Keep complete lexical text, but use token-aware vector windows so the tail also participates. Hybrid cannot be assumed to repair this when the user's paraphrase has no lexical match.

### D. Independent-engine padding and batching feasibility

Using the same local ONNX assets with **ONNX Runtime 1.22.1**, one intra-op/inter-op thread, 32 original inputs of at most 15 tokens including specials, one warm-up and three timed repetitions per shape:

| Tensor length / batch size | Median seconds for all 32 inputs |
| --- | ---: |
| 256 / 1 | 1.872 |
| 128 / 1 | 0.901 |
| 64 / 1 | 0.458 |
| 32 / 1 | 0.259 |
| 64 / 8 | 0.416 |
| 32 / 8 | 0.206 |

Masked pooled normalized vectors had maximum component difference 0 in this fixture and minimum cosine approximately 1.0 relative to the 256/1 baseline. This is **an independent-engine feasibility probe**, not a tract benchmark, a reason to replace the runtime, or proof of bit-exact equivalence on all inputs. Compilation time, longer text, Unicode, masking behavior, thread count and peak memory need native qualification. Pipeline identity must change when shape/runtime behavior changes even if this small sample matches exactly.

### E. MiniLM value versus time consumed

The measured benefit of all-MiniLM-L6-v2 is vocabulary-independent discovery. In the four-topic fixture, vector/hybrid retrieval found the expected topic for all four queries; lexical retrieval missed two paraphrases. In the real AIRK interaction, **avoid recomputing unchanged embeddings** found useful cache implementation evidence through vectors while lexical search returned no hits. Exact named definitions, paths, dependency versions, source reads and structural queries do not require the model. These observations demonstrate complementary value, not a measured 50% improvement over general repository search: four easy queries and one interactive example are not a representative relevance evaluation.

| Workload | Observed cost | What it means |
| --- | --- | --- |
| Detailed cold fixture, vectors off/on | 0.311 / 6.060 seconds; 5.749 seconds additional elapsed time | The enabled vector path dominated this small cold workload, but the difference includes model loading, persistence and publication as well as inference. |
| Separate Detailed full rebuild, four workers | 4.020 seconds inference within 6.516 seconds elapsed, about 62% | Neural execution is a major cold-build target. Removing all inference alone would still leave other work; overlapping timers are not additive. |
| One-file addition with cached original inputs | 0.091 seconds inference within 2.555 seconds elapsed, about 3.6%; 0.276 seconds model load | Inference is a small part of this incremental workload. Rebuilding and verifying the current retrieval components still costs time. |
| Deletion with cached surviving inputs | About 0.0014 seconds embedding-call time within 2.474 seconds elapsed | The update is slow even when there is essentially no neural work. Removing MiniLM does not address that rebuild design. |

**Recommendation:** Keep local MiniLM as an optional semantic route, prioritize lexical/fact readiness with O7, improve its input coverage with O2, and reduce its cost with O3/O4/O5. For users who mostly know exact names and literals, an explicit vectors-off policy is reasonable; keep all admitted source, lexical and selected fact evidence. Do not silently disable vectors for conceptual queries or claim that disabling them yields the same retrieval behavior. The long-tail failure above also shows that paying for an embedding does not guarantee useful coverage if the relevant text is truncated.

Before changing the default, compare vectors-off, current hybrid, and optimized/staged hybrid on the judged query set below. Break out exact-name versus paraphrase tasks; record relevant evidence found, MRR/nDCG, time to first usable evidence, total indexing time, actual forwards, query embedding latency, and memory. Query latency/model residency and a full-repository vector rebuild were not measured here. The decision should be based on additional relevant results per indexing/query cost, rather than vector counts alone. O6 and O4 solve different costs: one-current-index updates reduce rebuild overhead; more efficient inference reduces the price of genuinely new semantic inputs.

## What comparable systems do

Only primary documentation/source was used; accessed 2026-10-18. These are design comparisons, not dependencies to introduce.

| System | Relevant documented behavior | AIRK application |
| --- | --- | --- |
| [LlamaIndex ingestion](https://developers.llamaindex.ai/python/framework/module_guides/loading/ingestion_pipeline/) | Hashes node/transformation combinations for caching; document hashes allow unchanged inputs to be skipped and changed documents to be reprocessed | AIRK already has stronger pipeline-bound extraction/embedding reuse. Extend reuse to downstream components; avoid presenting caching itself as missing. |
| [Sourcegraph search configuration](https://sourcegraph.com/docs/admin/search) | Explicit ignores and size/trigram eligibility limits; some omitted content remains available through other search routes, and skipped content is inspectable | Separate source admission from vector eligibility, show omissions, and preserve cheap exact access. Do not copy its limits into AIRK without corpus evidence. |
| [Tantivy 0.26.2 IndexWriter](https://docs.rs/tantivy/0.26.2/tantivy/indexer/struct.IndexWriter.html) | Supports additions, term deletions and commits | Incremental lexical indexing is possible in the underlying library. O6 must coordinate its commits and reader refresh with catalog/vector updates; an individual engine commit does not make the whole AIRK index atomic. |
| [Microsoft DiskANN](https://github.com/microsoft/DiskANN) | Provider abstraction and update/query algorithms; current upstream describes real-time updates | The full rebuild is an AIRK provider/publication choice, not a universal DiskANN requirement. Validate the pinned 0.59.0 API and independently implement any needed local provider support. |
| [Qdrant production ingestion guidance](https://qdrant.tech/documentation/production-operations/vector-search-in-production/) and [bulk import](https://qdrant.tech/documentation/production-operations/bulk-data-import/) | Separates ingestion, index construction and search availability; discusses deferring graph work for bulk ingestion and resource-aware batching | Explicit readiness and bounded background construction can improve first-use latency. AIRK keeps DiskANN/local state; Qdrant/HNSW is not proposed as a replacement. |
| [Sentence Transformers efficiency](https://www.sbert.net/docs/sentence_transformer/usage/efficiency.html) and [encoding API](https://www.sbert.net/docs/package_reference/sentence_transformer/model.html) | Exposes batch sizing and runtime-dependent inference optimizations | Benchmark shape buckets and genuine batches inside the existing adapter. Published GPU/runtime results are not AIRK CPU measurements. |
| [MiniLM model card](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2) | Intended for sentences/short paragraphs, 384 dimensions, default truncation beyond 256 wordpieces | Token-aware vector input limits are essential. An 8 KiB citation bound is not an embedding context bound. |

## Ranked options and implementation plans

The following options can be implemented independently where indicated. Performance benefits are hypotheses unless directly measured above. Keep changes inside owning crates; neutral persisted contracts belong in core, filesystem/cache ownership in store, inference/search in retrieval, and scheduling/publication in kernel.

### O1. Measure stages and expose the existing choices (first)

**Plan:** Extend the index report and usage aggregation with actual model-forward count, tokens before/after truncation, padding slots, selected/skipped/duplicate vector units, graph nodes/reads/writes, lexical-add/commit/merge time, catalog construction/index time, verification time, cache I/O, model compile time, and maintenance time. For O6, measure reader/writer gate wait and hold times, journal bytes, recovery/replay time, tombstones, compaction, and mutated versus rebuilt records. Preserve optional/missing measurements rather than manufacturing zeroes. Record histogram buckets by profile/category, not source text. Separate no-op, cold, full and changed-file workloads in the panel and documentation.

**Owners:** ingest reports; retrieval builder/model/provider; kernel publication and usage; thin CLI/MCP/extension presentation.

**Checks:** Counters reconcile with published counts; empty inputs are distinguished from forwards; nested timers are labeled; interrupted builds never report completion. Benchmark one-file edits and no-op scans separately. Show configured versus published policies and pending vector work.

**Immediate use:** Ordinary Index now preserves caches; Full reindex intentionally does not. Balanced is a reasonable structural baseline for new collections, already the default. Vector opt-out remains an explicit option for exact-search-only workloads. Do not silently change existing collections or recommend Essential as a quality-equivalent substitute.

### O2. Separate semantic units from evidence partitions (highest quality-sensitive payoff)

**Plan:** Add a versioned `VectorUnit` projection with exact input digest, policy/pipeline identity and bounded member spans referencing retained artifacts. Preserve current lexical chunks, facts and exact reads initially. Generate units independently of continuity detail:

1. Attach a written heading or declaration signature to its related prose/body where useful. Do not generate summaries or reason about code.
2. Coalesce adjacent undersized pieces within one file and logical owner/section, targeting an initial experimental range of 128–192 tokens and a hard model-context cap including specials/context. These are tuning candidates, not an adopted default.
3. Split long bodies/sections at token-aware source boundaries; test small bounded overlap only when needed for cross-boundary relevance. Account for duplicated tokens and invalidation explicitly.
4. Never silently merge distinct PDF pages, DOCX paragraphs/tables, worksheet rows, slides or mapped host regions into a single false citation. If one semantic unit has several original locations, retain an explicit bounded list and materialize valid child evidence separately.
5. Keep lexical coverage of every retained source span even when a vector is omitted. Return original passages, not the synthetic heading/signature-plus-text input as if it were contiguous source.

**Owners:** core contracts; ingest boundary projection; retrieval builder and [search.rs](../crates/airk-retrieval/src/persistent/search.rs); store unit references; kernel inventory/readiness reporting.

**Required format changes:** Current `vector_candidate` requires exactly one lexical document per vector ID; `vector_id` binary-searches the ordered vector-ID/chunk-ordinal relation. Both assumptions break with many-to-many unit/citation mappings. Add explicit authenticated mappings, filtered eligibility, bounded fan-out and deterministic fusion identities. Do not merely give multiple documents the same existing vector field.

**Concrete S3 contract:** Load a verified neutral extraction payload and its bound artifact together, rather than reconstructing source maps from display chunks. Capture required, versioned semantic boundary metadata before detail pruning. Short sibling declarations share an owner group; oversized bodies retain their own written signature as split context. Keep groups within one region/native origin and exclude notebook host JSON from retrieval. Build an authenticated `vector-map/` component with bounded unit-to-member postings; semantic candidates retain their complete original body across citation members. Count occurrences rather than members against semantic hit limits and omit repeated synthetic input context from returned evidence. Focus a smaller original window when the response byte budget requires it, retaining mapped/native locations through a bounded source-map projection. Each lexical evidence identity receives at most one semantic fusion vote, so fragmentation does not multiply an occurrence's rank. Count graph nodes and semantic occurrences separately from lexical chunks. The required semantic projection is detail-independent; the 64-member per-input bound may force extra windows when one detail level has exceptionally many tiny citation partitions. Preserve complete body coverage and report this bounded exception instead of claiming universal input invariance.

**Rebuild boundary:** Detect obsolete derived format before catalog, retention or extraction reuse. An explicit `airk index --full` may discard incompatible derived state under writer and reader exclusion, preserving source/configuration and creating a new incarnation without old ancestry. Persist a reset-required marker before removal; interrupted resets remain unavailable and can be retried. Do not interpret arbitrary catalog errors as an empty index. Obsolete SQLite history/checkpoint administration is discarded; unrelated saved configuration files remain intact. JSON-only snapshots without bounded ownership metadata require explicit derived-state removal; no old evidence reader is added to recover them. This boundary is needed in S3; S8 later replaces the underlying generation-based runtime.

**Checks:** Exact read/hash/location parity, byte-budget compliance, Unicode/CRLF and mapped-region cases; query terms in the last part of long declarations/documents; rare short symbols; table rows/headers; token cap including prepended context; stable inputs after unrelated edits. The long-tail test above must pass before broader coarsening is enabled. Measure vectors, tokens and quality together: splitting overlong chunks can increase vector count while fixing missing semantic coverage.

### O3. Explicit vector eligibility and duplicate handling (combine with O2)

**Plan:** Add a collection-bound, digest-versioned vector-selection policy independent of source admission and fact detail. Begin with opt-in lexical-only lockfiles, minified/generated manifests, repetitive machine records and configurable path/profile rules. Preserve paths, source reads, lexical dependency/version searches and necessary facts. Do not alter `.gitignore` or hide source to achieve a vector-only optimization.

Use explicit, bounded exclusion rules over path prefix, basename/suffix, adapter and language; combine selectors within a rule with AND. Default to no exclusions. Generated-file detection is not sufficiently reliable to enable inferred omissions. Save selection independently of the global vector enable switch, bind the applied policy into the authenticated inventory, expose committed per-file decisions and report configured/applied differences. Policy-only changes must trigger indexing. CLI policy show/validate/set accepts bounded JSON; examples can opt into known generated paths or lockfile names without changing the default.

Skip whitespace before cache/batch assembly and report it separately. It already avoids neural execution, so this saves scheduling/lookup overhead, not thousands of forwards. Deduplicate equal misses within a batch before inference, then scatter results in original order. Consider a bounded per-update memo so repeated cache hits do not reopen the same object.

Graph deduplication is a separate second step: one vector per exact input/pipeline plus a bounded citation posting list. It needs O2's mapping/filter/fusion changes. A path-scoped query must still find a duplicate under that path even when the canonical vector was first seen elsewhere. Cap fan-out and report truncation; preserve collection isolation. Do not approximate-deduplicate similar code without a separate quality study.

Persist every occurrence; query expansion limits must not discard later aliases during indexing. Use a quota-backed private key registry through a neutral core interface and the store's existing bounded SQLite staging adapter. This avoids unbounded in-memory keys and preserves DiskANN as the published vector engine. Exact positive results, and successful no-vector outcomes, may be reused within that build; inference failures are errors. The transient registry is discarded, so this does not introduce persistent negative-cache semantics.

**Owners:** core vector policy, kernel policy persistence, retrieval selection/cache/grouping, store mappings. **Checks:** identical content at multiple paths/aliases, copy/delete/move, path filters, duplicate-hit diversity, quotas, same-size cache corruption and full-rebuild bypass. A short-length threshold alone is insufficient: terse requirements and names can be the most useful evidence.

### O4. Length buckets, true batches and model-plan reuse (highest inference payoff)

**Plan:** Add explicitly versioned fixed shape buckets, initially lengths 32/64/128/256, selecting the smallest fitting shape after tokenization. Compile and retain a bounded plan cache with memory charged for all live plans. Preserve the current truncation/pooling semantics until O2 supplies complete windows. Then evaluate actual batch dimensions such as 1/4/8 rather than calling `[1,256]` thirty-two times. Group by shape, restore input order before graph insertion, and bound batch bytes, tokenizer state, inference memory and result buffering.

Use a persistent bounded worker pool instead of creating scoped threads for each group if measurements justify it. Cross-file batching needs a bounded queue: the current one-artifact-at-a-time design must not become a whole-corpus text buffer. A persistent native scheduler may retain a verified model plan across jobs with explicit digest changes and eviction; standalone CLI calls cannot inherit process memory.

**Owners:** retrieval `onnx.rs`, `embedding_batch.rs`, `model.rs`; core embedding descriptor; kernel worker lifetime. Keep the current tract adapter first; an alternative local runtime or quantized export is a separately licensed and qualified experiment, not necessary to test shorter shapes.

**Checks:** Native-versus-oracle vector comparisons over varied lengths/languages/Unicode; masks/special tokens; padding invariance within agreed tolerances; exact cache invalidation when inference identity changes; serial/batch retrieval equivalence; peak RSS and cancellation/failure cleanup. Avoid blindly raising workers: model load, cache persistence, graph insertion and catalog work remain serial or separately bounded.

### O5. Pack and reuse embedding/cache/model objects more efficiently

**Plan:** Measure tiny-file opens, JSON float serialization, per-object sync and repeated authentication first. Introduce immutable binary embedding packs with a bounded key-to-offset index, per-record checksums, model descriptor binding and authenticated pack digests. Append only to private packs; finalize, sync and reference them through the O6 commit protocol. Reuse packs across updates, with references owned by the current index, bounded pending work and recovery journal. Reclaim unreferenced packs after active reads drain; charge staging and compaction to quota. Immutable cache objects are compatible with a single current index and do not require retained searchable generations. Deduplicate cache reads within one update without treating timestamps as authentication.

The existing cache has 65,536 unique-reference, 16 MiB manifest and 256 MiB read budgets. Repeated duplicate lookups can consume the read budget even though the reference set is deduplicated. Packing must preserve bounded reads and fail-closed corruption behavior; simply removing these caps is not an optimization. Empty/no-vector outcomes may be represented only with a versioned, model-specific reason; prefiltering blank input is simpler initially.

S5 first publishes packs through the existing receipt protocol; S8 transfers their ownership to the current index and durable pending work. Keep the numerical embedding descriptor unchanged when only storage changes. Authenticate the bounded pack index independently, verify each targeted record against that trusted index, and verify whole packs at finalization/maintenance. Bound memo memory, pack count, open handles and read bytes independently. Measure memo gains after S4 deduplication to avoid attributing the same avoided lookup twice. Partially live packs require quota-accounted compaction when fragmentation reaches the pack bound; do not retain unlimited dead records or silently exceed limits.

**Owners:** store `vector_cache.rs`, core embedding/cache formats, retrieval cache use, kernel commit/recovery. Shared immutable model assets already exist; do not propose copying weights into every update. **Checks:** partial pack writes, lost manifest, corrupt records, descriptor mismatch, active-read and pending/recovery reference reclamation, relocation, quota pressure, exact duplicates and negative-result invalidation.

### O6. One current index with incremental updates (recommended update architecture)

**Decision:** Remove retained searchable generations from the target storage model. Maintain one current logical index per collection, and update only changed artifacts, lexical documents, catalog rows and vector units. Source control owns source history where available. Git is not a runtime prerequisite: saved uncommitted files, non-Git roots, documents and aliased sources remain supported. AIRK must still retain the exact bytes for its current indexed evidence because live files may change before the next update. Git cannot reconstruct every such state or replace AIRK's consistency/recovery machinery.

Simply setting retention to one, repeatedly overwriting a generation directory, or fixing the generation number at `1` is insufficient. The existing builder would still reconstruct the whole index, and reusing a revision token would invalidate evidence validation. Remove historical storage semantics while retaining a collection incarnation, monotonically increasing committed-update sequence, source/payload hashes, pipeline identities and authenticated current metadata. A sequence identifies an observation; it does not promise a retained historical snapshot.

#### Contract changes

| Concern | Proposed single-current-index behavior |
| --- | --- |
| Current search, facts and reads | Each response observes one coherent committed state and retains original source locations, budgets and collection/root isolation. |
| Old exact revision or cursor | Return an explicit revision-unavailable/cursor-expired result after an incompatible update. Never substitute current bytes or Git content while claiming the old identity. Clients refresh and restart bounded pagination. |
| Remembered evidence and changes | Validate saved source/unit hashes against current evidence where supported. A bounded change-metadata log can report changed/deleted identities and coverage gaps; it is not a historical source/index store. Expired baselines require resynchronization, not a claim of no changes. |
| Concurrent updates and reads | Serialize writers; allow reads during preparation, and gate them during cross-component mutation. Return explicit busy/recovering status when bounded waiting expires. |
| Partial update failure | Replay durable intent or rebuild before resuming reads. There is no general guarantee that a complete old index remains available after mutation begins. |
| Historical checkpoints, rollback and retention UI | Remove from the new runtime contract or report unsupported through a versioned capability boundary. Source-control checkout followed by indexing is a new current state, not an AIRK historical read. |

**Accepted tradeoff:** Less rebuild/storage work and simpler historical-state management in exchange for update pauses, expired historical reads/cursors, and recovery/rebuild downtime after some failures. This is not automatically a simpler transaction implementation: coordinating mutable SQLite, Tantivy and DiskANN safely is substantial work. Preserve coherent responses, not uninterrupted availability or arbitrary old-revision access.

#### Update and recovery protocol

1. **Serialize preparation.** Acquire an OS-backed collection writer lock shared by every CLI, MCP and scheduler process. Read the committed metadata and prepare changed extraction, embeddings, IDs and bounded mutation batches in staging while current readers continue. Bind each result to captured source bytes, extraction/vector policies and expected current identities. Revalidate the expected index before apply; if source changes later, queue another update rather than relabeling prepared bytes as newer content.
2. **Prepare durable intent.** Reserve quota for the journal, staged objects, engine temporary files and recovery/compaction headroom. Persist a transaction ID, expected revision, allocated stable IDs, affected components and replayable inputs. Sync the intent and its dependencies before any current-index mutation. Every replay operation needs an idempotence rule; store ID allocations so a retry cannot duplicate nodes or citations.
3. **Gate readers and mark dirty.** Acquire an exclusive multiprocess read/write gate after the writer lock and drain active read guards. Persist and sync the applying/recovery-required state before mutating components. Readers use the shared gate, check durable readiness, refresh stale handles by committed sequence, and hold the guard through response materialization, including citation reads and revision-bound usage/source-size accounting. No lock is held for an agent's whole conversation. All participants use one documented lock order.
4. **Apply changed records.** Upsert/delete affected catalog rows and source-context sizes, delete/replace affected Tantivy documents, and insert/update/delete the corresponding graph nodes or live mappings. Preserve stable internal identities across unrelated edits; do not reuse changing corpus ordinals as node IDs. Record component progress for idempotent recovery. An individual SQLite transaction or Tantivy commit does not atomically commit the other engines.
5. **Commit visibility.** Sync affected data and metadata, validate cross-component mappings/readiness, and durably commit the next sequence only when the entire intended state is coherent. Refresh process-local readers/caches before serving that sequence. Release the exclusive gate, then reclaim staging and obsolete unreferenced objects through bounded maintenance. A cleanup failure after a durable commit is a maintenance issue, not a failed logical update.
6. **Recover before serving.** On process restart and on every read admission, check the persistent transaction state. An OS lock released by a crashed writer is not proof that the index is ready. Replay the journal under exclusive ownership; if graph/component recovery cannot be trusted, rebuild affected components or the whole current index before marking it ready. Do not serve partially committed engine combinations or report success on failed recovery.

The conceptual state machine is `READY(r) -> PREPARED(r, tx) -> APPLYING(tx) -> READY(r+1)`, with interrupted apply entering `RECOVERY_REQUIRED(tx)`. Preparation still permits reads of `r`; failure before mutation can discard preparation and leave it ready. Applying/recovery states prohibit index reads until consistency is restored. Status/diagnostics may remain available. Validate durable-marker ordering, sync and recovery behavior on each supported platform rather than assuming the filesystem provides cross-engine transactions.

#### Component implementation

- **Catalog and lexical index:** Replace copy-then-rebuild with per-artifact updates in the owning store/retrieval adapters. Keep normalized facts, selected evidence semantics, source-size accounting and necessary secondary indexes. Refresh Tantivy searchers and SQLite handles at the sequence boundary; long-lived MCP processes must not return a cached old snapshot as current.
- **DiskANN:** First prove insertion, replacement/deletion, persistent ID allocation and reopen/recovery behavior with the pinned 0.59.0 library and AIRK's provider. Existing private graph construction is not proof of safe persistent mutation. Journal logical operations and sufficient recovery inputs; if partial adjacency writes cannot be replayed safely, fail closed and rebuild. Tombstones require a live mapping, bounded accumulation and candidate refill/oversampling so deleted neighbors do not crowd out relevant live hits. Keep the final vector tier in DiskANN.
- **Alternative component layouts:** If direct graph mutation is unsafe or too costly, evaluate replaceable bounded graph shards or a base plus bounded delta within the same current logical index. These are internal storage strategies, not retained searchable generations. Bound search fan-out, delta size and compaction; measure recall/path filtering and temporary storage. Full-copy-then-update still incurs O(index-size) copying and is not the desired incremental result.
- **Integrity and caches:** Replace whole-immutable-generation validation assumptions with authenticated current metadata and appropriate record/segment integrity checks. Never cache a whole-file digest indefinitely while mutating that file. Immutable source objects, model assets and O5 embedding packs remain useful; retain references for current evidence, active reads and bounded pending/recovery work, then reclaim them.
- **Maintenance:** Compaction, large policy/model changes and explicit full rebuilds can still require O(index-size) work and longer unavailable periods. Schedule and expose these costs. One current logical index does not mean exactly one physical representation at every instant: journals, engine segments, staging and temporary replacement files need bounded disk headroom, without becoming historical query targets.

#### Rebuild boundary and delivery

Introduce an explicit incompatible storage/API capability version. Stop and drain workers before replacing derived state; require a full rebuild using preserved collection/root and policy bindings. Do not convert old catalogs, caches, histories or checkpoints, and do not retain compatibility readers. Never mutate source files. Incompatible indexed state returns an actionable rebuild error; rebuilding affects only managed derived state under the collection's ownership checks. The new runtime stops creating generation directories, historical leases and retained snapshot inventories. Replace history controls with current revision, readiness, recovery and maintenance status.

Implement the writer/read gates and crash journal first, then per-file catalog/Tantivy updates, then the qualified DiskANN update adapter. Do not switch runtime paths until the replacement passes fault tests, and do not temporarily mutate old published directories under old readers. Introduce O2 mappings through a declared schema/rebuild boundary, independently of the first minimal current-index implementation.

**Owners:** core revision/readiness/capability contracts; store current metadata, journal, locks, object ownership and quotas; retrieval persistent/provider/search adapters; kernel commit/recovery and scheduler; MCP/CLI/extension revision, cursor and history surfaces.

**Checks:** Edit/delete/rename/alias/model changes; two independent reader processes plus competing writers; crashes before and after each durable marker and engine commit; torn/corrupt graph writes; disk-full recovery; stale process-local handles; expired revisions/cursors; replay twice without duplicate IDs; GC with pending recovery. Measure changed-node/row work, gate hold/wait time, first-query latency and recovery time on cold, 1%, 10%, deletion-heavy and rename-heavy workloads. Hashing source inputs and authenticating components remain separate costs; dropping generations does not remove them.

### O7. Explicit staged readiness and deferred enrichment

**Plan:** Add a versioned readiness contract: lexical/facts ready, vectors pending/partial/ready/failed, selected versus completed vector coverage, and exact input/policy/model identity for pending work. Commit a coherent base update to the single current index using O6. A durable kernel scheduler job consumes bounded, pinned captured inputs, computes missing vectors outside the exclusive read gate, then applies them through another O6 transaction. Every visible readiness/content change advances the committed sequence; no older complete hybrid generation is retained as a fallback.

When the base update changes or deletes a unit, remove its old vector from live eligibility immediately. Unchanged vectors remain eligible when their exact input, current citation mapping and pipeline identities match, regardless of which earlier update computed them. Reject superseded enrichment results, coalesce obsolete jobs and release their pinned inputs. A partially completed job may commit compatible units only with accurate partial coverage. Never attach an old vector to a newer, incompatible source span.

Define default-search behavior explicitly through saved policy: disclose lexical-only or partial hybrid results while enrichment is pending, or report unavailable if complete vectors are required. Explicit vector requests must disclose partial coverage or pending/unavailable rather than silently falling back. Reads/status must not initiate expensive indexing jobs. Apply the same discipline to deferred detailed call/document-continuity enrichment, with unsupported/pending coverage rather than false absence. All routes still read a coherent current state under O6, even when that state's declared coverage is incomplete.

**Owners:** core readiness contract; kernel scheduler/commit; store pending-input ownership and journal; MCP/CLI result disclosure; extension progress. **Checks:** restart, pause, superseding edits, model/policy changes, disk pressure, failed vectors after a successful base commit, cancellation, bounded pinned inputs and old cursor expiry. Measure time to first lexical evidence, time to full hybrid evidence and total work separately. Build on proven O6 updates so deferred vectors do not trigger another whole-corpus lexical/catalog rebuild.

### O8. Avoid constructing discarded facts; tune parsing and scan work

**Plan:** Push selected detail/capabilities into adapters before expensive optional fact construction. Balanced can skip generic call payloads while retaining required framework cues; Essential can use a declared text-extraction path where safe. Preserve document text, source maps, parse-status limitations and original locations. Decouple declaration/document boundary discovery needed by O2 from optional continuity fingerprints so semantic chunking does not require Detailed evidence.

Remove demonstrably discarded work first: prose continuity analysis currently repeats classification and fact allocation even though it returns only structural units. Qualify a unit-only path. Then separate definition/signature and heading seeds from rich facts before early detail pruning. Balanced must still preserve framework cues derived from calls; Essential must still produce semantic boundaries, source maps, text and parse-status diagnostics. Adapters without a qualified early-selection path may retain full extraction followed by selection.

Add bounded file-level extraction parallelism only with byte-weighted admission and deterministic output ordering. Keep discovery, protected reads/hashes, reuse admission and retained writes on a coordinator; parallelize bounded cache-miss extraction jobs, cap queued/completed bytes and drain workers on failure. Preserve PDF worker isolation and initially serialize native expansion jobs. Compare one, two and four workers before selecting a production limit. Filesystem events remain coalesced hints with periodic full reconciliation. Every admitted source is still read/hashed before reuse; mtime/size or a missing event is never proof of unchanged content.

**Owners:** ingest adapters/syntax/detail/continuity; kernel scan/scheduler/watcher. **Checks:** same retained selected facts/locations as the existing lower-detail mode, framework cue parity, failure budgets, non-UTF-8/large sources, alias changes, missed/overflowed filesystem events and same-size edits. Detailed-only caller/change queries must report deliberate omission when a lighter policy is selected.

### O9. Tune graph I/O and construction after reducing unnecessary work

**Plan:** Instrument record reads/writes and repeated neighbor loads; evaluate a bounded graph-record cache, reusable scratch buffers, platform-appropriate positioned I/O, preallocation charged to quota, and batched adjacency persistence. Apply these either to private rebuilds or journaled mutation under O6's exclusive gate; invalidate cached records after updates and recovery. Preserve record integrity, journal ordering and final sync/reopen checks. Evaluate graph construction degree/search-list parameters against recall, build CPU and bytes; do not lower them solely to make indexing look faster.

**Owners:** retrieval `vector.rs` and neutral charging callback. **Checks:** corruption detection, exact vector serialization, deterministic stable IDs, concurrent access if introduced, synthetic ANN recall against brute-force neighbors and a representative semantic query set. The small experiment's graph time was much less than inference, so graph micro-tuning is not the first cold-build fix. Larger graphs and high-cache-hit edits may change that ranking.

Quantization/dimension reduction is lower priority: it may reduce graph bytes/search I/O but does not remove transformer forwards, and truncating arbitrary MiniLM dimensions is not justified. Any quantized model/vector format needs independent quality, license, numerical and rebuild-boundary evidence.

## Combining options and delivery order

| Combination | Expected effect | Dependencies and interactions |
| --- | --- | --- |
| O1 + existing Balanced/ordinary Index controls | Better diagnosis; less selected evidence and retained state | Available behavior; long-tail semantic risk must be disclosed until O2 |
| O1 + O3 blank/dedup-miss handling + O5 bounded memo | Low-risk overhead reduction while preserving current vectors/citations | Do first; blank elimination is not a claimed neural speedup |
| O2 + O3 vector selection | Fewer low-value vectors with complete token-aware coverage of retained semantic passages | Can increase vectors for previously truncated long text; many-to-many mapping and quality gates required |
| O2 + O4 | Fewer tiny independent inputs, less padding for remaining short inputs, actual batches | Benefits overlap; repacking can make average inputs longer, reducing padding savings |
| O4 + O5 | Faster cache misses and cheaper cache hits | Little inference gain on already-cached updates; pipeline changes force a one-time cache miss |
| O6 + O5 | Small edits update the current components and reuse packed embeddings | Strong incremental candidate; requires new schema, multiprocess gates, durable recovery and bounded object reclamation |
| O6 + O7 + O2/O4 | Fast initial lexical evidence while vectors catch up efficiently | Enrichment updates the same current index; disclose partial coverage and reject obsolete inputs; no older hybrid generation fallback |
| O8 + O2 | Less discarded parsing/fingerprint work without making vector inputs depend on detail | Requires lightweight independent boundary extraction |
| O6 + O9 | Mutate fewer graph nodes and reduce remaining graph I/O; qualify internal shard/delta alternatives if needed | Journal correctness, bounded compaction and recall come before tuning throughput |
| O1–O9 together | Separate fast exact evidence, efficient semantic enrichment and cheap current-index updates | One policy/format rebuild plan, bounded resources and coherent current revision across all components |

The stage ledger at the start of this document is the delivery order. It first lands independently testable input/inference optimizations, then qualifies and integrates the rebuild-only current-store replacement, then adds deferred enrichment. O1 instrumentation is extended alongside each owning stage: token/shape counters in S2/S3, cache I/O in S5, extraction in S6, gate/journal/component work in S7/S8 and graph I/O in S10. Report unavailable measurements explicitly until their owning stage is implemented.

For every format/policy change, bind it into the appropriate extraction/vector/retrieval specification and authenticated current metadata. Require a rebuild for incompatible formats; never reinterpret old chunks or cache bytes under new semantics. Preserve root/collection isolation, current indexed revision identity, original locations, failure reporting and quotas. O6 deliberately replaces historical retention/rollback guarantees with current-state recovery and explicit unavailability; update architecture, API/capability documentation, rebuild guidance and UI together when implementing it. The user has authorized this index/API redesign and rebuild-only upgrade in this implementation session. Before a future feature/bug-fix PR, remind the human contributor to synchronize the native/extension patch/build versions and lockfiles; the current local commits do not perform a version bump.

## Acceptance and qualification plan

Measure on code-heavy, documentation-heavy, mixed-office and many-small-file corpora. Include cold empty-cache, forced full, no-op, one-file addition/edit/deletion, prefix insertion, rename, source alias change, policy/model change and interrupted/retried builds. Repeat at least five times in alternating variant order after other test/build load finishes; report medians and spread, hardware, model/runtime/binary identities, OS cache conditions, CPU, peak RSS, logical bytes and application I/O separately from physical I/O.

Maintain a judged set of at least 50 representative queries spanning exact names, paraphrases, rare literals, tail-only evidence, table cells, configuration versions, tests and path-scoped duplicates. Proposed gates, to be agreed before rollout:

- Exact file reads, citations, source hashes, selected structural facts and isolation checks: 100% parity for optimizations claiming unchanged evidence.
- Vector-unit changes: pass the adversarial tail case; retain at least 95% of baseline relevant results at 10 and inspect all regressions. Track MRR/nDCG and scope-specific recall so a mean does not hide lost languages or formats. This is a proposed gate, not an achieved quality score.
- Cache/shape changes: compare native vectors against a verified independent oracle with existing numerical tolerances, plus retrieval-level comparisons; exact cache reuse requires the declared exact identity contract.
- Incremental components: changed-input counters scale with affected units/records (or bounded internal shards), no stale hits after deletion, and bounded query overhead. Propose a 2x median improvement in the targeted stage as an engineering target, not a promise that total indexing will halve. Measure gate wait/hold and compaction costs separately from preparation.
- Current-index consistency/recovery: fault-inject before/after every journal marker, component commit and final readiness commit; exercise independent reader/writer processes, stale handles, disk-full and corrupt graph records. Every successful response binds to one coherent committed state; interrupted apply yields explicit recovery/unavailability until replay or rebuild succeeds. Repeating recovery must not duplicate IDs or lose unchanged evidence. Verify old revisions/cursors fail explicitly and incompatible old indexes require rebuild without touching source/configuration.
- Staged readiness: correct pending/failed coverage and no stale-vector/current-citation combinations under all crash/supersession tests; report both completion times. Unchanged vectors may be reused across committed sequences only when their current input/mapping/pipeline identities match.
- Budgets: no unbounded corpus buffering, cache growth, pending-input pins, recovery journals, tombstones, graph fan-out or scheduled retries. Charge temporary replacement/compaction space. The new runtime retains one logical current index; recovery is allowed to pause reads or rebuild instead of preserving an old complete publication.

No single larger-than-memory workload, full real-repository vector rebuild, or production semantic relevance evaluation was run for this review. The repository vector-candidate audit used vectors disabled; full vector timing estimates must not be inferred by multiplying tiny-fixture timings.

## Reproducing the experiments and verification

Use the existing licensed local model directory; none of these commands downloads weights. The contributor harness additionally needs Python `tokenizers`; `--padding-probe` needs NumPy and ONNX Runtime. These are measurement dependencies, not AIRK runtime dependencies. The harness creates fresh state under an ignored `target/indexing-review-*` directory and leaves it for local diagnosis. Do not stage or package the state or raw reports. Its corpus mode reads the selected workspace and only writes the isolated state/output locations.

~~~powershell
cargo build --release --locked -p airk-cli
python scripts/benchmark-indexing.py --binary target/release/airk.exe --model-dir target/local-vector-model --corpus . --output target/indexing-review-corpus.json
python scripts/benchmark-indexing.py --binary target/release/airk.exe --model-dir target/local-vector-model --output target/indexing-review-synthetic.json
python scripts/benchmark-indexing.py --binary target/release/airk.exe --model-dir target/local-vector-model --tail-probe --output target/indexing-review-tail.json
python scripts/benchmark-indexing.py --binary target/release/airk.exe --model-dir target/local-vector-model --padding-probe --output target/indexing-padding-probe.json
~~~

Future repository counts will include this document and later changes. Compare source inventories before interpreting differences. The audit reads only its own fresh, verified-by-publication artifacts, checks payload hashes, and counts catalog rows through read-only SQLite. Payload relationship arrays alone are insufficient: native binding can construct relationships persisted in the catalog.

Baseline verification performed for the original documentation/harness change:

| Check | Result |
| --- | --- |
| Release CLI build, locked dependencies | Passed |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo test --workspace --locked` | Passed: 727 tests, 0 failures, 9 ignored across 43 reported suites including doc-test suites |
| `python -m unittest discover -s scripts/tests -v` | Passed: 123 tests reported, 4 skipped |
| `scripts/check-repo.ps1` | Passed, including documentation links and byte-identical contributor guidance |
| License-notice generation | Passed; bundled 467 third-party packages, with existing separate-license-text review notices for adobe-cmap-parser, htmlescape, pdf-extract and type1-encoding-parser |
| Benchmark harness syntax, corpus audit, native synthetic/tail experiments and independent padding probe | Passed/executed as described above; observed quality failures are explicitly retained in section C |
| Git whitespace check and explicit new-file whitespace check | Passed |

Ignored Rust tests were not implicitly counted as passing; the real model was exercised separately through the native CLI experiments. No Docker, macOS/Linux or editor-host qualification was performed. Generated reports, license bundles and experimental state remain outside the change.

The original single-current-index and MiniLM value/cost revisions were design-only updates. Later implementation and qualification are recorded in the stage ledger at the start of this document. Baseline results do not independently qualify a later mutable-index recovery protocol or establish its speedup; its concurrency, failure-injection, rebuild and retrieval gates require their own implementation evidence.
