# Storage and retrieval scale qualification

`scripts/benchmark-scale.py` measures an explicitly bounded workload using the public CLI. Its default original fixture has 16,384 files of 8,192 bytes each: 128 MiB of source, exceeding the bootstrap's 10,000-file and 64 MiB defaults. The harness explicitly configures collection scan limits and a 16 GiB collection/32 GiB root quota. This larger qualification profile is recorded in its report; normal collection defaults remain conservative and unchanged.

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
