# Storage and retrieval scale qualification

`scripts/benchmark-scale.py` measures an explicitly bounded workload using the public CLI. Its default original fixture has 16,384 files of 8,192 bytes each: 128 MiB of source, exceeding the bootstrap's former 10,000-file and 64 MiB defaults. The harness explicitly configures collection scan limits and a 16 GiB collection/32 GiB root quota. This qualification profile is recorded in its report; the historical measurements below use that explicit profile, independently of the [automatic ingestion defaults introduced in 1.1.0](ingestion-limits.md).

~~~console
cargo build --release -p airk-cli --locked
python scripts/benchmark-scale.py --binary target/release/airk --output target/scale-qualification --cpython
~~~

On Windows use `target/release/airk.exe`. The output directory must be new. It retains original fixtures, local index state, command JSON, stderr and `report.json` for inspection. These files are ignored development artifacts; do not package or publish retained index/source contents. Omit `--cpython` for a completely offline run.

`--command-timeout-seconds` sets a per-command deadline from 1 to 5,400 seconds; the default is 1,800 seconds. Timeout or any failed acceptance check leaves `passed: false`. Increasing a deadline does not qualify a workload that has not completed.

The optional public corpus is [CPython v3.14.0](https://github.com/python/cpython/tree/v3.14.0), commit `ebf955df7a89ed0c7968f79faec1de49f61ed7cb`. The harness downloads that explicit commit archive from GitHub, verifies SHA-256 `57601e7fdcf7ce150e76e0af106768ec7491c39b246b45a1599c06d0c6f61bdc`, preserves the supplied licenses and extracts only ordinary files/directories under bounded safe paths. It executes none of the downloaded code. Full before/after source inventories verify that AIRK leaves the public tree unchanged.

## Measurements and acceptance checks

Each command runs in a fresh process. The synthetic workload checks complete source inventory, exact sentinel retrieval across the tree, reuse of an unchanged generation, retained evidence after live sources change, then coherent publication of an edit and deletion. CPython checks exact code/document queries with original source hashes and a native Python definition. These fixed checks measure exact retrieval and lifecycle correctness; they do not measure broad relevance or transformer semantic quality.

The report records wall time, operating-system CPU and resident-memory observations, process I/O counters, logical index bytes, binary digest, machine capacity and collection usage reports. Windows counters measure logical process transfer bytes rather than physical SSD traffic. Linux uses final `wait4` CPU and sampled post-exec procfs high-water RSS/storage I/O. Its separate `child_lifetime_peak_rss_bytes` can include inherited launcher memory and is not substituted for executable RSS. Short-lived operations can lack RSS/I/O samples, and sampling can miss growth just before exit. Missing fields mean unknown, never zero. Optional ledger observations can be dropped under their own budgets and do not replace these external measurements.

The index directory starts fresh, but the harness does not flush operating-system caches. Repeated queries have fresh process caches and uncontrolled operating-system caches. Results therefore cannot be labeled cold-device or warm-server latency. Process sampling uses a 20 ms interval, which also limits short-command timing precision. Logical index totals include all retained generations, source objects and ancillary state at the end of the workload.

The transformer adapter has a separate [real-model and independent numerical qualification](onnx-models.md). This scale harness deliberately installs no embedding model: it measures persistent lexical/exact storage, streaming ingestion and revision lifecycle independently of model inference. A larger-than-memory vector qualification needs a separately sized graph, real model/corpus judgments and cache/device measurements.

## Windows 1.1.0 real-workspace indexing and MCP checks

On 2026-10-17, the optimized Windows x64 1.1.0 helper, SHA-256 `64710a9d95293c7eb5453d797d5b76c06d6b43c2d1ff2e8ed11f2238e6fdd20f`, indexed the AIRK working tree and a second local application workspace into separate, extension-derived collection IDs sharing one fresh data root. Both used `index --scheduled --automatic`, ordinary ignore rules, automatic 2 GiB source sizing and the default 4 GiB collection disk quota. Builds and tests were idle during measurement. No optional embedding model was installed.

| Workspace | Indexed files | Eligible indexed source | Initial index wall time | Logical collection state after checks |
| --- | ---: | ---: | ---: | ---: |
| AIRK working tree | 570 | 5,228,442 bytes | 15.879 seconds | 297,300,031 bytes |
| Separate application workspace | 7,628 | 90,479,733 bytes | 185.429 seconds | 3,475,633,240 bytes |

Times include native scheduling, extraction and publication; they exclude compilation, source-hash preflight and subsequent retrieval checks. State was fresh, with zero reused files. OS caches were not flushed. These are individual observations on one machine, not throughput or peak-memory guarantees. The application collection is already close to its default disk quota; retaining another generation can require a deliberate quota increase or maintenance.

An earlier attempt on the application workspace failed after 123.435 seconds at the former 128 MiB aggregate continuity inventory ceiling, without publishing a generation. The shared ceiling is now 512 MiB across publication, retrieval and retained/historical readers. The successful inventory contains 237,649,027 encoded bytes and 216,116 assigned continuity records. A descriptor-by-descriptor comparison with retained extraction payloads confirmed that no continuity was omitted to obtain the successful result. AIRK retained all 11,264 of its descriptors as well. A separate original-fixture regression round-trips a valid inventory above the former 128 MiB ceiling.

Two concurrent stdio MCP connections exposed the expected sixteen tools. All 24 checks passed: status, positive and negative lexical queries, broad queries, exact symbol queries, file discovery and indexed reads; foreign-only terms and paths returned no evidence, and traversal and collection-override arguments were rejected. Every returned collection identity and source citation path/hash was checked against the selected workspace. Before/after SHA-256 inventories also confirmed that all 573 and 7,755 Git-visible source files, respectively, were unchanged. Private excerpts, paths, source inventories and index state remain in ignored local artifacts and are not published here.

The first application MCP search took 2.605 seconds, symbol calls 6.863–6.949 seconds and the first file-list call 8.165 seconds. Subsequent lexical queries and the indexed excerpt read took about 13 ms in the same connection. This exposes metadata/loading costs that still need performance work; it is not a general retrieval-latency qualification.

AIRK skipped three PNG assets and reported 100% eligible-file coverage. The application skipped 124 binary assets plus two JSON files that exceeded the existing shared parser-work budget; its complete inventory reported 7,628 of 7,630 eligible files current, displayed as 99%. Its initial command also reported a dropped local usage observation. Indexing and MCP retrieval passed, but those parser omissions and usage-history incompleteness remain. No installed-editor, Linux/macOS, semantic-relevance or maximum-capacity qualification is implied by this native-helper run.

Local receipts are under ignored `artifacts/filesizing-real-final/`; the earlier failed attempt is under `artifacts/filesizing-real-repositories/`.

## Windows rotating-disk run: incomplete

On 2026-10-14, the Windows x64 optimized binary `bdfec036c89d09e3a69e505127621ad2450b1c64db7515aa9d538664635a4df4` began the default 16,384-file/128 MiB workload on NTFS drive D:, a Seagate ST2000LM007 rotating disk. The first index was stopped after 432.464 seconds: 2,971 extracted artifact objects had been published, with one unfinished temporary artifact file and no committed collection generation. The process had consumed 13.453 CPU seconds and reached 12.55 MiB peak RSS during that partial pass.

This is an incomplete run, not a Windows full-scale pass or a peak-memory bound for a complete index. Low CPU use and the implementation's individual object syncs suggest storage synchronization was a major cost on this rotating disk; no syscall profile was collected to establish that attribution. The default 30-minute command deadline was at risk; the run was stopped rather than relaxing durability or claiming extrapolated completion. Original corpus files were unchanged, and the local harness report records failure. Docker-volume measurements below use a different virtual filesystem and cache configuration and must not be treated as equivalent physical-device throughput.

## Linux Docker-volume run: passed

On 2026-10-14, both workloads passed using optimized Linux x64 helper SHA-256 `e1d1bb145e1c25d64747243a06eb39138fe6c814ecdaf4c410a25cbe7442825a`. Docker ran under WSL2 Linux 5.15.167.4 with eight logical CPUs and 16,775,139,328 bytes of guest physical memory. Source and index state lived in a fresh named Docker volume; only the harness and report export used host bind mounts. The virtual filesystem reported 166,714,957,824 bytes free before the run. Host physical-device caches and Docker backing storage were not isolated or profiled.

The run explicitly configured the larger scan limits described above and a 16 GiB collection/32 GiB root quota. No embedding model or optional OCR/semantic provider was installed. Builds and other CPU-heavy validation were idle during measurement.

| Operation | Wall seconds | CPU seconds | Sampled native peak RSS |
| --- | ---: | ---: | ---: |
| Synthetic: initial 16,384-file, 128 MiB index | 82.701 | 27.543 | 62.727 MiB |
| Synthetic: unchanged pass, all 16,384 reused | 2.373 | 2.364 | 43.027 MiB |
| Synthetic: one edit and one deletion, 16,382 reused | 17.833 | 18.644 | 83.352 MiB |
| CPython: 5,014 indexed files, 97,096,878 source bytes | 87.757 | 70.518 | 652.555 MiB |

Each indexing row had hundreds or thousands of post-exec memory samples. These are observed peaks for this workload, not universal memory bounds; CPU time sums execution across threads. The synthetic and CPython initial index processes respectively reported sampled procfs storage reads/writes of 446,464/355,553,280 bytes and 12,288/3,002,572,800 bytes. Cache hits and the sampling boundary affect those counters: they are neither bytes read through application APIs nor physical-device traffic totals.

All twelve synthetic sentinel queries returned the expected original path. Their fresh-process wall observations were 20.806–21.339 ms, at the harness's 20 ms sampling resolution; CPU measurements were 15.437–17.565 ms. Short queries generally had only one early memory/I/O sample, so this run does not qualify their peak memory or final I/O. The unchanged pass retained generation 1. Live edit/deletion checks still returned retained evidence before publication; the next pass atomically published generation 2 with exactly one indexed file and one removal.

The pinned CPython source inventory contained 5,216 regular files and 113,771,118 bytes. AIRK indexed its discovered, supported subset: 5,014 files and 71,494 chunks, with 176 reported extraction skips. These included binary fixtures, document type declarations, irregular TSV files, an image-only PDF, `Misc/svnmap.txt` exceeding the text limit and `Parser/parser.c` exceeding the per-file syntax-fact limit. A successful collection index does not mean every source file or language construct was understood. The interpreter/document queries, source citation hashes, native Python `Path` definition and complete before/after source inventory checks all passed.

The final logical retained state was 317,814,310 bytes for the synthetic workload, including both generations, and 2,966,195,026 bytes for CPython. A read-only inspection of CPython's immutable SQLite catalog measured **2,133,196,800 bytes**, with 740,644 fact rows and 1,198,367 relationship rows. These relationships include syntax candidates; their count does not imply resolved calls. The run used the explicit larger quota profile, so retained sizes alone do not establish that a full publication would pass under the default policy.

An earlier Linux attempt passed the synthetic gates but failed CPython at the former fixed 1 GiB private-catalog ceiling. The prior generation remained authoritative, and that failed report/state was preserved. The fixed ceiling and artifact-JSON size multiplier were removed, along with the separate 4 GiB publication-attempt ceiling; [quota-derived admission](storage-quotas.md) now covers actual page growth while preserving active-pointer space. The completed CPython catalog above directly demonstrates growth beyond the old ceiling. Earlier RSS figures contaminated by the launcher's inherited high-water mark are excluded from this qualification.

Local receipts are retained under ignored `target/scale-qualification-linux-final-20261014/`, with the earlier failed run under `target/scale-qualification-linux-20261014/`. They include the exact binary digest, configured policy, all command observations and acceptance outcomes. Index/source contents remain local development artifacts. These two corpora fit in guest memory; this is not larger-than-memory DiskANN qualification, broad relevance evaluation, macOS qualification, or a completed Windows large-corpus run.
