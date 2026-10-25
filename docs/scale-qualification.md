# Storage and retrieval scale qualification

`scripts/benchmark-scale.py` measures an explicitly bounded workload using the public CLI. Its default original fixture has 16,384 files of 8,192 bytes each: 128 MiB of source, exceeding the bootstrap's former 10,000-file and 64 MiB defaults. The harness explicitly configures collection scan limits and a 16 GiB collection/32 GiB root quota. This qualification profile is recorded in its report; the historical measurements below use that explicit profile, independently of the [automatic ingestion defaults introduced in 1.1.0](ingestion-limits.md).

~~~console
python scripts/build-service-runtime.py --profile release --destination target/scale-runtime
python scripts/benchmark-scale.py --binary target/scale-runtime/airk --output target/scale-qualification --cpython
~~~

On Windows use `target/scale-runtime/airk.exe`; on Mac use `target/scale-runtime/AIRK.app/Contents/MacOS/airk`. The main must belong to the verified separate runtime pair. The output directory must be new. It retains original fixtures, local index state, command JSON, stderr and `report.json` for inspection. These files are ignored development artifacts; do not package or publish retained index/source contents. Omit `--cpython` for a completely offline run.

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

## Windows 1.1.1 detail-level baseline

On 2026-10-17, `scripts/benchmark-detail.py` compared Essential, Balanced and Detailed using the same optimized Windows helper and identical source inputs at each level. SHA-256: `d2f138764998f0b150d154a24424550fefc6e3418dd916d95dafd85e8acd1f5e`. Each level had a fresh shared data root containing independent collections for AIRK and a separate local application workspace. Indexing used `index --scheduled --automatic`, automatic source sizing, 100/500 GiB storage defaults and zero retained predecessors. No embedding model was installed. Usage recording was disabled for this controlled measurement. Builds and tests were idle; operating-system caches were not flushed.

| Workspace | Detail | Initial seconds | Unchanged seconds | One-file edit seconds | Initial logical MiB | Initial peak RSS MiB |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| AIRK | Essential | 8.65 | 5.57 | 6.48 | 16.54 | 59.91 |
| AIRK | Balanced | 12.66 | 7.71 | 11.89 | 95.78 | 122.18 |
| AIRK | Detailed | 18.68 | 10.76 | 20.04 | 290.07 | 180.30 |
| Separate application workspace | Essential | 80.33 | 12.68 | 24.42 | 268.71 | 166.57 |
| Separate application workspace | Balanced | 121.51 | 30.70 | 76.14 | 1273.25 | 884.34 |
| Separate application workspace | Detailed | 206.93 | 70.41 | 183.69 | 3279.20 | 1953.49 |

AIRK indexed 579 files / 5,429,561 source bytes; the separate application indexed 7,628 files / 90,479,733 source bytes. Edits were made only in isolated source copies. Each update re-extracted exactly one file and reused every other indexed file. Original Git-visible source inventories were unchanged. These single-machine observations are not maximum-capacity or cross-platform qualification.

Balanced reduced initial logical storage versus Detailed by 67.0% for AIRK and 61.2% for the application. All initial MCP symbol, lexical, indexed-read, citation-hash, foreign-collection and traversal/override checks passed. Essential deliberately returned no native structured symbols while retaining text retrieval. Application symbol requests took 1.95/1.98/2.01 seconds in Balanced and 6.61/6.61/6.48 seconds in Detailed for the first and two repeated requests in one helper connection. The first request is a fresh helper process, not a flushed-device measurement.

Five incremental cases finished with one retained generation. The application's Detailed case finished with two: its post-commit cleanup took 15.4 seconds, crossed the cooperative 10-second budget and left a durable pending deletion intent. Its policy still specified zero predecessors, and only the active generation was protected. These one-shot CLI measurements did not run a subsequent idle continuation. Initial storage in the table contains only one generation; incremental retained state must not be compared without accounting for this difference.

The application reported two JSON files over the existing shared parser-work budget and 124 binary assets. AIRK reported 14 PNG assets and one legacy-encoded Markdown file as non-UTF-8; that Markdown file was subsequently normalized to UTF-8. Detail selection reduces retained evidence after parsing and does not remove parser ceilings. Private source copies, paths, excerpts and index state remain under ignored `artifacts/detail-real-final/`.

## Final Windows enhancement comparison (2026-10-17)

The final optimized AIRK 1.1.1 helper, SHA-256 `ff7e33811873f4ed205329e33d14f8cde299f1f62c60cc2939a1fb366c5b5e3b`, was compared with the detail-level baseline above. The final machine had 64 logical CPUs (AMD Ryzen Threadripper PRO 5975WX), 137,285,533,696 bytes of physical memory and Windows x64. Builds and editor tests were idle during measurement. No model was installed; usage recording was disabled. Collection/root quotas were the 100/500 GiB defaults.

The run used frozen AIRK and application source copies matching the baseline's indexed file counts, source bytes and catalog row counts at every detail level. The final roots have longer absolute paths than the baseline roots; their repeated root metadata contributes to the small Essential storage increase. Initial indexes used fresh state, and one-file edits were made only in additional isolated copies. Source-hash preflight, copying and subsequent MCP checks are excluded from index timings. OS caches were not flushed. These are individual observations, not statistical averages or maximum-capacity qualification.

Every cell below is **baseline → final**. Logical storage is measured after the initial publication; update timings include that command's post-publication maintenance.

| Workspace | Detail | Initial seconds | Unchanged seconds | One-file edit seconds | Initial logical MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| AIRK | Essential | 8.65 → 7.34 | 5.57 → 5.57 | 6.48 → 6.46 | 16.54 → 16.65 |
| AIRK | Balanced | 12.66 → 10.85 | 7.71 → 7.13 | 11.89 → 10.45 | 95.78 → 84.09 |
| AIRK | Detailed | 18.68 → 15.47 | 10.76 → 8.59 | 20.04 → 15.49 | 290.07 → 207.92 |
| Separate application workspace | Essential | 80.33 → 65.59 | 12.68 → 12.12 | 24.42 → 22.57 | 268.71 → 269.83 |
| Separate application workspace | Balanced | 121.51 → 98.46 | 30.70 → 23.80 | 76.14 → 58.07 | 1273.25 → 976.80 |
| Separate application workspace | Detailed | 206.93 → 176.35 | 70.41 → 53.53 | 183.69 → 145.97 | 3279.20 → 2462.20 |

Balanced storage fell 12.2% for AIRK and 23.3% for the application; Detailed fell 28.3% and 24.9%. The application's Balanced SQLite catalog shrank from about 715 to 418 MiB, while its source and extraction-object sizes were unchanged. Balanced remains about 60% smaller than Detailed on both corpora. Essential storage was approximately unchanged. Peak indexing memory did not materially improve: application Balanced was 884.34 → 868.97 MiB and Detailed was 1953.49 → 1955.13 MiB. The final AIRK peaks were 60.67/117.95/181.13 MiB for Essential/Balanced/Detailed; application Essential was 169.41 MiB.

The following exact-symbol timings use a fresh helper connection for the first call and the mean of two subsequent calls in that same process. A fresh process is not a flushed-device measurement. Essential intentionally retains no native symbol facts, so its empty structured responses are excluded from this table.

| Workspace | Detail | First call, milliseconds | Repeated call, milliseconds |
| --- | --- | ---: | ---: |
| AIRK | Balanced | 175.54 → 157.76 | 174.27 → 40.87 |
| AIRK | Detailed | 537.76 → 378.48 | 534.83 → 89.09 |
| Separate application workspace | Balanced | 1947.14 → 1454.39 | 1992.57 → 341.47 |
| Separate application workspace | Detailed | 6614.86 → 5050.40 | 6547.12 → 1045.89 |

Repeated symbol calls were 4.3–6.3 times faster. Current component bytes are still hashed on every request; the optimization reuses semantic validation, not timestamp trust. Initial indexing, unchanged passes and one-file updates still perform substantial generation-wide work. The measurements do not establish broad relevance, semantic quality or complete language understanding.

All six cases passed symbol/lexical/indexed-read checks, citation path/hash validation, foreign-collection negative queries, collection-override rejection and traversal rejection. AIRK indexed 579 files and 5,429,561 source bytes; the application indexed 7,628 files and 90,479,733 bytes. Each edit re-extracted exactly one file and reused all other indexed files. Input inventories remained unchanged. The extraction omissions described in the baseline remain; matching counts do not mean every source file was supported.

The final six updates each reached **one retained generation with completed cleanup**. The explicit continuation probe returned no work in all six cases; no additional cleanup pass was required. The baseline application's Detailed update had left two generations and pending cleanup, so its update time did not include equivalent completed reclamation. Separate MCP reads after cleanup checked retained source access, citations and collection isolation again.

Qualification found and corrected two cleanup costs: the former 256 MiB verification allowance prevented large catalogs from completing object GC, and loading a full catalog merely to discover that optional history was disabled could consume the time allowance. The final default permits 8 GiB of streamed verification per pass, and skips that unnecessary history load only when no recovery intent is pending. Authentication, metadata/time/removal bounds and corruption rejection remain in force. The earlier stalled application Detailed state also completed through one continuation without reindexing. See [retention](retention.md) for remaining bounds.

The final Windows VSIX also passed [installed-editor qualification](windows-local-build.md#installed-extension-and-two-window-qualification), including two simultaneous windows sharing a collection and two windows with isolated collections in shared storage. The test found and corrected live writer contention incorrectly blocking queued indexing and quota enumeration failing on concurrently removed managed paths. [Actual workbench PNGs](ui/README.md#installed-vs-code-captures) show MCP verification and untrusted behavior.

Reproduce with `scripts/benchmark-detail.py --binary <optimized-helper> --config <private-config.json> --output artifacts/<fresh-directory> --incremental-copies --continue-cleanup`. Config requires two independent source roots, a distinguishing query and an indexed file path for each; see the script's configuration example. The harness now requires completed cleanup receipts and performs post-cleanup retrieval checks. Local final receipts are under ignored `artifacts/enhancements-qualified-final/`; post-cleanup checks are also recorded in `after-cleanup-mcp.json`. The earlier diagnostic run remains under `artifacts/enhancements-real-final/`. Private sources, excerpts, profiles and indexes are not publication artifacts. Cross-platform, remote-host, upgrade and larger-than-memory qualification remain separate.

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
