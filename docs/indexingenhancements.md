# Indexing and retrieval optimization implementation plan

Review date: 2026-10-18. Measurement baseline: `dd3675701242e91a03a0808f2f20c6d8f0af44a7` (1.1.4). Implementation branch: `feat/indexoptimizations`. The execution ledger below distinguishes completed work from proposals. Historical measurements describe the baseline, not later implementations. The accompanying [measurement harness](../scripts/benchmark-indexing.py) runs against isolated state and does not change the user's collection.

## Execution contract and stage ledger

Implement the stages below in order, validate each stage, and commit it locally before starting the next implementation stage. Do not push. Design reviews may run independently; implementation changes for different stages must not be mixed into one commit. Preserve unrelated work, source read-only behavior, collection/root isolation, exact current evidence, original locations and bounded resource use. DiskANN remains the final vector engine. No new external service or inference download is part of this work.

The target is one current logical index with incremental component updates and explicit semantic readiness. Token-aware semantic units are independent of exact citation partitions and selected structural detail. MiniLM remains an optional local discovery route. The staged storage migration must preserve existing collections until the new format, migration and recovery protocol pass their gates; primitives alone do not constitute a completed migration.

| Stage | Implementation and dependency | Completion gate | Status |
| --- | --- | --- | --- |
| S0 | Reconcile this plan, preserve baseline measurements and add the isolated benchmark harness | Documentation/repository/whitespace checks; local planning commit | Complete |
| S1 | O1 + exact-preserving O3: distinguish inputs, blanks, duplicate reuse and actual neural forwards; skip blank scheduling and deduplicate equal inputs within bounded batches | Cold/warm/full counters reconcile, duplicate citations remain, corrupt cache fails closed, serial/parallel output parity | Pending |
| S2 | O4: versioned ONNX length buckets, bounded compiled-plan ownership, then true tensor batches | Native numerical/retrieval equivalence, explicit pipeline identity, memory limits and real local-model measurements; legacy pipelines remain readable | Pending |
| S3 | O2: independent token-aware vector units, stable identities and explicit unit-to-evidence mappings | Long-tail retrieval passes; complete exact/lexical evidence and original locations survive; bounded fan-out and path filters | Pending |
| S4 | O3: persisted vector-selection policy and exact-input graph deduplication using S3 mappings | Lexical-only selection is inspectable; duplicate aliases/path filters and delete/move behavior remain correct | Pending |
| S5 | O5: bounded update memo, binary cache packs and authenticated ownership/GC | Corruption, partial writes, full-rebuild bypass, quota and pending/recovery ownership tests; compare cache I/O | Pending |
| S6 | O8: push selected detail into adapters, retain independent semantic boundaries, bound extraction concurrency and reconcile scan hints | Selected-evidence/location parity, deterministic results and budgets; same-size changes and missed events still detected | Pending |
| S7 | O6 foundation: qualify writable DiskANN updates, stable IDs, deletion/live mapping and replay; versioned current-state metadata, OS read/write gate and durable journal | Isolated provider and multiprocess fault tests; no runtime format switch yet | Pending |
| S8 | O6 integration: per-artifact catalog/Tantivy/vector updates, reader refresh, integrity, recovery, migration and historical API/UI capability changes | Every read path uses the gate; crash/disk-full/replay tests, migration and old-client rejection; one-file work scales with changes; switch only after these pass | Pending |
| S9 | O7: base lexical/fact commit followed by durable, coalesced semantic enrichment of the same current index | Pending/partial/failed coverage, supersession, restart and source-binding tests; first-ready and fully-ready timings | Pending |
| S10 | O9: bounded graph-record caching, scratch/positioned I/O and persistence improvements under the S8 journal | Exact record/corruption tests, ANN recall, quota and cold/update benchmarks; retain only measured improvements | Pending |
| S11 | Integrated qualification and user documentation across O1–O9 | Full required checks, judged retrieval suite and repeated workload measurements; report achieved gains and remaining platform/scale limits | Pending |

Each implementation commit updates its ledger status and records the relevant validation and measured limitations. Runtime changes require formatting, clippy, workspace tests, repository and whitespace checks from [CONTRIBUTING](../CONTRIBUTING.md), plus focused tests that exercise the changed behavior. Run Python/extension checks when their surfaces change and complete them at integrated qualification. Local Windows success does not qualify macOS/Linux, power loss or larger-than-memory behavior. Tests requiring local licensed model assets must be explicitly run when those assets are available, not counted as passed because they are ignored by default.

**Experiment decisions:** Shape sizes, tensor batch sizes, vector target lengths, overlap, graph parameters and shard/delta layouts are candidates to measure, not simultaneous mandatory defaults. If a candidate fails quality, memory or throughput gates, record the evidence and retain the qualified implementation. Quantization, alternative inference runtimes and dimension reduction are future research, not required changes. This does not defer S1–S11 or permit marking unused scaffolding as implemented. Default semantic-unit/eligibility changes require relevance evidence; dropping history requires the explicit S8 migration.

**Stage record:** S0 retains all original experiments and makes this ledger the authoritative delivery order. Repository checks (667 public files), harness syntax and whitespace checks passed. No runtime optimization has landed yet.

## Recommendation

Keep exact source evidence, useful structural facts, and vector retrieval, but stop making their granularity and publication cost inseparable. Prioritize:

1. **Token-aware vector units independent of citation chunks and continuity units.** Combine undersized related fragments, attach their written heading/signature context, and split oversized passages before embedding. Preserve fine source locations and lexical/fact retrieval independently.
2. **Length-bucketed inference, then real tensor batching.** The current ONNX adapter runs one input at a time at a fixed length of 256, including very short inputs. Four workers already exist; simply adding parallelism is not a new solution.
3. **Keep one current index and update its changed components.** A one-file edit currently reconstructs the entire Tantivy index and DiskANN graph. Replace retained searchable generations with incremental updates, a collection-wide multiprocess read/write gate, and durable recovery. Preserve lightweight revision identity and exact current evidence; let source control own source history. This changes availability and historical-read guarantees; O6 defines the replacement contract.
4. **Make vector eligibility and detailed enrichment explicit policies.** Lockfile records, repetitive boilerplate, and some generated data are good candidates for lexical-only treatment. Do not remove tests, configuration, or short declarations indiscriminately.
5. **Offer explicit staged readiness if time to first usable evidence matters more than time to full hybrid readiness.** Make the current lexical/fact index usable first, then add compatible vectors through further committed updates of that same index. Expose pending/partial coverage; silently mixing stale vectors with changed citations is unacceptable.

The strongest immediate evidence is excessive short-input padding and the all-corpus rebuild after small edits. Less detail is already configurable and substantially reduces storage, but it also changes vector boundaries. The tests below demonstrate a real retrieval regression from blindly making chunks larger.

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

## What the existing code indexes

The authoritative dependency boundaries remain those in [architecture](architecture.md). No external database, agent reasoning, orchestration engine, or cloud inference is needed for these proposals.

| Stage and implementation | Current inputs and output | Work on an ordinary update |
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

### Detail is currently coupled to chunking

| Level | Selected evidence | Chunking consequence |
| --- | --- | --- |
| Detailed | Supported facts including calls, code continuity, document-unit continuity | Declaration hints plus document-unit hints; often many small chunks and intervening gaps |
| Balanced | Removes generic `call` facts/edges, keeps other supported structural/document facts and code continuity | Still has declaration hints; does not generate Detailed document-continuity hints |
| Essential | Removes optional facts/relationships and continuity | Partitions host text without declaration/document-continuity hints; original native document boundaries still constrain chunks |

Balanced retains supported framework categories other than the generic `call` category; it is not simply “definitions and nothing else.” Code hints favor outermost declarations, so nested methods can remain facts within a larger containing chunk. Essential does not mean token-aware semantic chunking. The legacy 40-line helper in ingest is not the final v2 publication chunk contract.

### Exactly what goes into DiskANN

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

Under the proposed single-current-index contract, prepare extraction and embeddings while readers continue using the current state, then gate reads during the cross-component apply/commit. Do not hold that gate throughout ordinary inference. Every response must still describe one coherent committed state. A failed partial update may require a recovery pause or rebuild instead of serving a retained prior generation. Full rebuilds and model migrations may take longer pauses; short update pauses are a measurement target, not an existing guarantee.

## Experiments performed

Built the current optimized Windows x64 CLI with `cargo build --release --locked -p airk-cli`. Binary SHA-256: `613014cf1f1ae2c9378ad3a61f31bc96b6148070e9e114ed922f279cc9b438dd`. The host exposed 64 logical CPUs; inference used at most four workers. Existing licensed local model assets were used without downloads. Runtime usage recording was disabled in the experiment processes.

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

**Checks:** Exact read/hash/location parity, byte-budget compliance, Unicode/CRLF and mapped-region cases; query terms in the last part of long declarations/documents; rare short symbols; table rows/headers; token cap including prepended context; stable inputs after unrelated edits. The long-tail test above must pass before broader coarsening is enabled. Measure vectors, tokens and quality together: splitting overlong chunks can increase vector count while fixing missing semantic coverage.

### O3. Explicit vector eligibility and duplicate handling (combine with O2)

**Plan:** Add a collection-bound, digest-versioned vector-selection policy independent of source admission and fact detail. Begin with opt-in lexical-only lockfiles, minified/generated manifests, repetitive machine records and configurable path/profile rules. Preserve paths, source reads, lexical dependency/version searches and necessary facts. Do not alter `.gitignore` or hide source to achieve a vector-only optimization.

Skip whitespace before cache/batch assembly and report it separately. It already avoids neural execution, so this saves scheduling/lookup overhead, not thousands of forwards. Deduplicate equal misses within a batch before inference, then scatter results in original order. Consider a bounded per-update memo so repeated cache hits do not reopen the same object.

Graph deduplication is a separate second step: one vector per exact input/pipeline plus a bounded citation posting list. It needs O2's mapping/filter/fusion changes. A path-scoped query must still find a duplicate under that path even when the canonical vector was first seen elsewhere. Cap fan-out and report truncation; preserve collection isolation. Do not approximate-deduplicate similar code without a separate quality study.

**Owners:** core vector policy, kernel policy persistence, retrieval selection/cache/grouping, store mappings. **Checks:** identical content at multiple paths/aliases, copy/delete/move, path filters, duplicate-hit diversity, quotas, same-size cache corruption and full-rebuild bypass. A short-length threshold alone is insufficient: terse requirements and names can be the most useful evidence.

### O4. Length buckets, true batches and model-plan reuse (highest inference payoff)

**Plan:** Add explicitly versioned fixed shape buckets, initially lengths 32/64/128/256, selecting the smallest fitting shape after tokenization. Compile and retain a bounded plan cache with memory charged for all live plans. Preserve the current truncation/pooling semantics until O2 supplies complete windows. Then evaluate actual batch dimensions such as 1/4/8 rather than calling `[1,256]` thirty-two times. Group by shape, restore input order before graph insertion, and bound batch bytes, tokenizer state, inference memory and result buffering.

Use a persistent bounded worker pool instead of creating scoped threads for each group if measurements justify it. Cross-file batching needs a bounded queue: the current one-artifact-at-a-time design must not become a whole-corpus text buffer. A persistent native scheduler may retain a verified model plan across jobs with explicit digest changes and eviction; standalone CLI calls cannot inherit process memory.

**Owners:** retrieval `onnx.rs`, `embedding_batch.rs`, `model.rs`; core embedding descriptor; kernel worker lifetime. Keep the current tract adapter first; an alternative local runtime or quantized export is a separately licensed and qualified experiment, not necessary to test shorter shapes.

**Checks:** Native-versus-oracle vector comparisons over varied lengths/languages/Unicode; masks/special tokens; padding invariance within agreed tolerances; exact cache invalidation when inference identity changes; serial/batch retrieval equivalence; peak RSS and cancellation/failure cleanup. Avoid blindly raising workers: model load, cache persistence, graph insertion and catalog work remain serial or separately bounded.

### O5. Pack and reuse embedding/cache/model objects more efficiently

**Plan:** Measure tiny-file opens, JSON float serialization, per-object sync and repeated authentication first. Introduce immutable binary embedding packs with a bounded key-to-offset index, per-record checksums, model descriptor binding and authenticated pack digests. Append only to private packs; finalize, sync and reference them through the O6 commit protocol. Reuse packs across updates, with references owned by the current index, bounded pending work and recovery journal. Reclaim unreferenced packs after active reads drain; charge staging and compaction to quota. Immutable cache objects are compatible with a single current index and do not require retained searchable generations. Deduplicate cache reads within one update without treating timestamps as authentication.

The existing cache has 65,536 unique-reference, 16 MiB manifest and 256 MiB read budgets. Repeated duplicate lookups can consume the read budget even though the reference set is deduplicated. Packing must preserve bounded reads and fail-closed corruption behavior; simply removing these caps is not an optimization. Empty/no-vector outcomes may be represented only with a versioned, model-specific reason; prefiltering blank input is simpler initially.

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

- **Catalog and lexical index:** Replace copy-then-rebuild with per-artifact updates in the owning store/retrieval adapters. Keep normalized facts, compatibility semantics, source-size accounting and necessary secondary indexes. Refresh Tantivy searchers and SQLite handles at the sequence boundary; long-lived MCP processes must not return a cached old snapshot as current.
- **DiskANN:** First prove insertion, replacement/deletion, persistent ID allocation and reopen/recovery behavior with the pinned 0.59.0 library and AIRK's provider. Existing private graph construction is not proof of safe persistent mutation. Journal logical operations and sufficient recovery inputs; if partial adjacency writes cannot be replayed safely, fail closed and rebuild. Tombstones require a live mapping, bounded accumulation and candidate refill/oversampling so deleted neighbors do not crowd out relevant live hits. Keep the final vector tier in DiskANN.
- **Alternative component layouts:** If direct graph mutation is unsafe or too costly, evaluate replaceable bounded graph shards or a base plus bounded delta within the same current logical index. These are internal storage strategies, not retained searchable generations. Bound search fan-out, delta size and compaction; measure recall/path filtering and temporary storage. Full-copy-then-update still incurs O(index-size) copying and is not the desired incremental result.
- **Integrity and caches:** Replace whole-immutable-generation validation assumptions with authenticated current metadata and appropriate record/segment integrity checks. Never cache a whole-file digest indefinitely while mutating that file. Immutable source objects, model assets and O5 embedding packs remain useful; retain references for current evidence, active reads and bounded pending/recovery work, then reclaim them.
- **Maintenance:** Compaction, large policy/model migrations and explicit full rebuilds can still require O(index-size) work and longer unavailable periods. Schedule and expose these costs. One current logical index does not mean exactly one physical representation at every instant: journals, engine segments, staging and temporary replacement files need bounded disk headroom, without becoming historical query targets.

#### Migration and delivery

Introduce an explicit incompatible storage/API capability version. Stop and drain old workers before migration; old binaries must fail closed on the new layout. Bootstrap from the verified current inventory/components where compatible, otherwise rebuild with preserved collection/root and policy bindings. Never mutate source files. Existing protected checkpoints must receive an explicit export/retirement path; this proposal does not authorize silently deleting them. Once migrated, stop creating generation directories, historical leases and retained snapshot inventories. Replace history controls with current revision, readiness, recovery and maintenance status.

Implement the writer/read gates and crash journal first, then per-file catalog/Tantivy updates, then the qualified DiskANN update adapter. Keep the existing immutable format until the replacement passes fault tests; do not temporarily mutate its published directories under old readers. Introduce O2 mappings through a declared schema migration, independently of the first minimal current-index implementation.

**Owners:** core revision/readiness/capability contracts; store current metadata, journal, locks, object ownership and quotas; retrieval persistent/provider/search adapters; kernel commit/recovery and scheduler; MCP/CLI/extension revision, cursor and history surfaces.

**Checks:** Edit/delete/rename/alias/model changes; two independent reader processes plus competing writers; crashes before and after each durable marker and engine commit; torn/corrupt graph writes; disk-full recovery; stale process-local handles; expired revisions/cursors; replay twice without duplicate IDs; GC with pending recovery. Measure changed-node/row work, gate hold/wait time, first-query latency and recovery time on cold, 1%, 10%, deletion-heavy and rename-heavy workloads. Hashing source inputs and authenticating components remain separate costs; dropping generations does not remove them.

### O7. Explicit staged readiness and deferred enrichment

**Plan:** Add a versioned readiness contract: lexical/facts ready, vectors pending/partial/ready/failed, selected versus completed vector coverage, and exact input/policy/model identity for pending work. Commit a coherent base update to the single current index using O6. A durable kernel scheduler job consumes bounded, pinned captured inputs, computes missing vectors outside the exclusive read gate, then applies them through another O6 transaction. Every visible readiness/content change advances the committed sequence; no older complete hybrid generation is retained as a fallback.

When the base update changes or deletes a unit, remove its old vector from live eligibility immediately. Unchanged vectors remain eligible when their exact input, current citation mapping and pipeline identities match, regardless of which earlier update computed them. Reject superseded enrichment results, coalesce obsolete jobs and release their pinned inputs. A partially completed job may commit compatible units only with accurate partial coverage. Never attach an old vector to a newer, incompatible source span.

Define default-search behavior explicitly through saved policy: disclose lexical-only or partial hybrid results while enrichment is pending, or report unavailable if complete vectors are required. Explicit vector requests must disclose partial coverage or pending/unavailable rather than silently falling back. Reads/status must not initiate expensive indexing jobs. Apply the same discipline to deferred detailed call/document-continuity enrichment, with unsupported/pending coverage rather than false absence. All routes still read a coherent current state under O6, even when that state's declared coverage is incomplete.

**Owners:** core readiness contract; kernel scheduler/commit; store pending-input ownership and journal; MCP/CLI result disclosure; extension progress. **Checks:** restart, pause, superseding edits, model/policy changes, disk pressure, failed vectors after a successful base commit, cancellation, bounded pinned inputs and old cursor expiry. Measure time to first lexical evidence, time to full hybrid evidence and total work separately. Build on proven O6 updates so deferred vectors do not trigger another whole-corpus lexical/catalog rebuild.

### O8. Avoid constructing discarded facts; tune parsing and scan work

**Plan:** Push selected detail/capabilities into adapters before expensive optional fact construction. Balanced can skip generic call payloads while retaining required framework cues; Essential can use a declared text-extraction path where safe. Preserve document text, source maps, parse-status limitations and original locations. Decouple declaration/document boundary discovery needed by O2 from optional continuity fingerprints so semantic chunking does not require Detailed evidence.

Add bounded file-level extraction parallelism only with byte-weighted admission and deterministic output ordering. Preserve existing PDF worker isolation and the one-artifact memory bound or replace it with an explicit bounded equivalent. For repeated scans, coalesce scheduler events and use dirty-path hints to prioritize work, with overflow/full reconciliation and periodic content-hash verification. Do not turn mtime/size into proof of unchanged content, or weaken source-handle protections.

**Owners:** ingest adapters/syntax/detail/continuity; kernel scan/scheduler/watcher. **Checks:** same retained selected facts/locations as the existing lower-detail mode, framework cue parity, failure budgets, non-UTF-8/large sources, alias changes, missed/overflowed filesystem events and same-size edits. Detailed-only caller/change queries must report deliberate omission when a lighter policy is selected.

### O9. Tune graph I/O and construction after reducing unnecessary work

**Plan:** Instrument record reads/writes and repeated neighbor loads; evaluate a bounded graph-record cache, reusable scratch buffers, platform-appropriate positioned I/O, preallocation charged to quota, and batched adjacency persistence. Apply these either to private rebuilds or journaled mutation under O6's exclusive gate; invalidate cached records after updates and recovery. Preserve record integrity, journal ordering and final sync/reopen checks. Evaluate graph construction degree/search-list parameters against recall, build CPU and bytes; do not lower them solely to make indexing look faster.

**Owners:** retrieval `vector.rs` and neutral charging callback. **Checks:** corruption detection, exact vector serialization, deterministic stable IDs, concurrent access if introduced, synthetic ANN recall against brute-force neighbors and a representative semantic query set. The small experiment's graph time was much less than inference, so graph micro-tuning is not the first cold-build fix. Larger graphs and high-cache-hit edits may change that ranking.

Quantization/dimension reduction is lower priority: it may reduce graph bytes/search I/O but does not remove transformer forwards, and truncating arbitrary MiniLM dimensions is not justified. Any quantized model/vector format needs independent quality, license, numerical and migration evidence.

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
| O1–O9 together | Separate fast exact evidence, efficient semantic enrichment and cheap current-index updates | One policy/format migration plan, bounded resources and coherent current revision across all components |

The stage ledger at the start of this document is the delivery order. It first lands independently testable input/inference optimizations, then qualifies and integrates the storage migration, then adds deferred enrichment. O1 instrumentation is extended alongside each owning stage: token/shape counters in S2/S3, cache I/O in S5, extraction in S6, gate/journal/component work in S7/S8 and graph I/O in S10. Report unavailable measurements explicitly until their owning stage is implemented.

For every format/policy change, bind it into the appropriate extraction/vector/retrieval specification and authenticated current metadata. Provide an explicit migration boundary; never reinterpret old chunks or cache bytes under new semantics. Preserve root/collection isolation, current indexed revision identity, original locations, failure reporting and quotas. O6 deliberately replaces historical retention/rollback guarantees with current-state recovery and explicit unavailability; update architecture, API/capability documentation, migration guidance and UI together when implementing it. Review discussion is required for the identity/index-format/MCP contract changes under [CONTRIBUTING](../CONTRIBUTING.md). Before a future feature/bug-fix PR, remind the human contributor to synchronize the native/extension patch/build versions and lockfiles; this analysis does not perform a version bump.

## Acceptance and qualification plan

Measure on code-heavy, documentation-heavy, mixed-office and many-small-file corpora. Include cold empty-cache, forced full, no-op, one-file addition/edit/deletion, prefix insertion, rename, source alias change, policy/model change and interrupted/retried builds. Repeat at least five times in alternating variant order after other test/build load finishes; report medians and spread, hardware, model/runtime/binary identities, OS cache conditions, CPU, peak RSS, logical bytes and application I/O separately from physical I/O.

Maintain a judged set of at least 50 representative queries spanning exact names, paraphrases, rare literals, tail-only evidence, table cells, configuration versions, tests and path-scoped duplicates. Proposed gates, to be agreed before rollout:

- Exact file reads, citations, source hashes, selected structural facts and isolation checks: 100% parity for optimizations claiming unchanged evidence.
- Vector-unit changes: pass the adversarial tail case; retain at least 95% of baseline relevant results at 10 and inspect all regressions. Track MRR/nDCG and scope-specific recall so a mean does not hide lost languages or formats. This is a proposed gate, not an achieved quality score.
- Cache/shape changes: compare native vectors against a verified independent oracle with existing numerical tolerances, plus retrieval-level comparisons; exact cache reuse requires the declared exact identity contract.
- Incremental components: changed-input counters scale with affected units/records (or bounded internal shards), no stale hits after deletion, and bounded query overhead. Propose a 2x median improvement in the targeted stage as an engineering target, not a promise that total indexing will halve. Measure gate wait/hold and compaction costs separately from preparation.
- Current-index consistency/recovery: fault-inject before/after every journal marker, component commit and final readiness commit; exercise independent reader/writer processes, stale handles, disk-full and corrupt graph records. Every successful response binds to one coherent committed state; interrupted apply yields explicit recovery/unavailability until replay or rebuild succeeds. Repeating recovery must not duplicate IDs or lose unchanged evidence. Verify old revisions/cursors fail explicitly and protected historical state has an explicit migration path.
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

The subsequent single-current-index and MiniLM value/cost revisions change this plan only. Repository/documentation checks and whitespace checks were repeated for the documentation update; runtime suites and indexing experiments were not rerun. The original results above do not qualify the proposed mutable-index recovery protocol or establish its speedup. Its concurrency, failure-injection, migration and retrieval acceptance tests remain implementation work.
