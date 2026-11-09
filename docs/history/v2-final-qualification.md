# Architecture v2 local qualification

Recorded 14–15 October 2026. This record distinguishes implemented features and completed local checks from outstanding qualification. It is not a release approval or a claim that the complete plan has passed its final matrix.

## Evidence sources and completed checks

Local receipts include `target/v2-benchmark-report.json`, `target/v2-final-python-packaging.log`, `target/v2-final-tests-linux.log`, `target/v2-audit-linux-workspace.log`, `target/v2-audit-native.log` and `target/v2-case-rename-native.log`. They remain local build artifacts; no private index contents are reproduced here.

| Check | Observed outcome |
| --- | --- |
| Python repository/script tests | Final suite: 52 tests in 7.290 seconds, 51 passed and one skipped. The skip is `test_launcher_memory_is_not_reported_as_executable_rss`, requiring Linux fork/exec RSS accounting. Signing/release unit fixtures do not establish actual signing or publication. |
| Windows `acceptance_v2` | All five passed again after the audit fixes in 5.04 seconds: real case-only rename; fresh handles after ignore, oversize and extraction-omission readmission; two reader leases across writer/history/relocation/removal. |
| Windows focused audit checks | Three observation regressions, nine registry cases and the provider binding fixture passed. Ingest all-target warnings-denied clippy passed. The deterministic deferred-generation cleanup case also passed in 4.01 seconds. |
| Linux container workspace clippy | Completed successfully with `--workspace --all-targets --locked --offline -- -D warnings` in the inspected attempt. |
| Linux container debug workspace build | Completed successfully with `--workspace --locked --offline` in the inspected attempt. |
| Linux container full workspace tests | Final no-fail-fast run: 651 passed, two failed, four ignored across 81 result groups. The provider fixture exceeded the executable-size cap; the retention campaign hit a truthful cleanup deferral at edit 189. Corrections and targeted receipts are described below; this is not an uninterrupted all-green invocation. Both corrected CLI policy cases passed. |
| Linux focused audit follow-up | 26 checks passed: observation 3, registry 9, provider 1, acceptance 5, general-language A 5, notebook 1 and retention defaults 2. Final workspace/all-target clippy passed with warnings denied. |
| Original-fixture benchmark | `correctness_checks_passed: true`, including the mixed workload's separate correctness result. Scope and measurements follow below. |
| Clean Linux release build | Passed with two jobs and a fresh Cargo target cache; registry and base-image caches were reused. Cargo compilation took 17m36s; full Docker build took 1,136.808 seconds. The initial runtime image was 266,905,406 bytes. This measurement preceded the final audit fixes; the incremental final image and smoke results are recorded separately. |
| Final Linux release and container smoke | Incremental audited release passed in 5m59s. `scripts/check-container.py --image airk:v2-release` passed: sixteen-tool stdio MCP, two independent collections, offline/read-only runtime, persistent restart, notices, usage and source-safe removal. |
| Windows release and CLI smoke | Release build passed in 2,340.598 seconds with one Cargo job and an existing target cache; this is not a clean-build measurement. DOCX/XLSX tables, storage lifecycle and usage-input CLI smoke checks passed. |
| Extension/helper contracts | All 33 tests passed with no skips on Node 24.15.0, using real old fifteen-tool and new sixteen-tool Windows helpers. The old cached helper survives replacement and rejects explicitly enabled newer history storage without altering sources or publication identity. |

The full run's four ignored cases are two subprocess fixtures invoked through their parent tests, the separately measured lexical experiment, and an explicitly provisioned ONNX campaign that remains unavailable. The provider parser/binding fixture now hashes a tiny original stand-in rather than the grammar-heavy debug test executable; it never launches a provider and the production 256 MiB cap is unchanged. The policy rejection fixture uses unsupported version 99 rather than now-supported version 2.

The original retention test assumed that all generation cleanup completed synchronously despite the one-second cooperative postcommit deadline. At edit 189 it observed three full revisions. The corrected test requires a persisted same-head pending receipt without quota pressure, bounded maintenance continuations, unchanged head and protected current/predecessor revisions before enforcing the original two-generation and byte-plateau limits. The deterministic Windows deadline test passed. The full corrected native run passed all 200 publications in 995.13 seconds, with 53 bounded generation-cleanup continuations. Content/search tail bytes stayed at 27,046–27,066; total logical tail bytes were 5,243,238–5,765,441, including independently bounded growing lineage/catalog state. At most two source/payload pairs survived and pending intents were zero. Receipt: `target/v2-retention-final-native.log`. No corrected full Linux rerun is claimed.

## Benchmark scope

This was a **debug Linux container run under concurrent builds**, configured with 300 generated files, one small edit, one metadata worker, one read sample and the original local-model workload enabled. The initial inventory contains 302 files after the harness's additional fixtures. It is a correctness and instrumentation receipt, not a release-performance distribution or a large-repository qualification.

Reported environment: `Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.36`, eight logical CPUs, 16,775,139,328 physical-memory bytes, 269,427,478,528 disk-total bytes and 55,786,098,688 disk-free bytes before the run. The processor string is empty in the receipt. Binary version is `airk 0.1.0`; debug binary size is 851,278,600 bytes, not a shipping-package size.

| Original fixture identity | Files | Source bytes | Full SHA-256 inventory digest |
| --- | ---: | ---: | --- |
| Initial | 302 | 20,088 | `542ce13da6ebdb104fff475659c98c249608f4f964a58cac3063221327a6da39` |
| After mixed fixtures added | 309 | 21,468 | `03aedd644fb00bbdcee16a1256ca7c2abc470528807670aaf6e26f358c5e6349` |
| Final, after branch replacement | 309 | 21,561 | `5cfebc44e52055908271548466b96acfb219793a4e8a8ae5b2dc720ac2d21321` |
| Local-model source fixture, after edit | 2 | 76 | `69feeb4e5dfea1b0d8c743756d93e4fdc8cbb1289871fa6e24acc662a5a8c073` |

Benchmark binary SHA-256: `45270395f0a1322d1fa6a8ce093abe48aa4c60e16b3cbdb7f6e4059f1217b6db`.

## Observed publication and read measurements

| Operation | CLI wall time, ms | Indexed / reused | Generation |
| --- | ---: | --- | ---: |
| Initial lexical index | 11,905.066 | 302 / 0 | 1 |
| Unchanged lexical index | 5,499.793 | 0 / 302 | 1 |
| One small edit | 13,261.187 | 1 / 301 | 2 |
| Move 300 files | 16,873.476 | 0 / 302 | 3 |
| Add seven mixed-format fixtures | 19,964.972 | 7 / 302 | 4 |
| Edit two mapped children | 16,750.722 | 2 / 307 | 5 |
| Branch replacement | 18,864.325 | 12 / 297 | 6 |
| Final unchanged index | 7,956.406 | 0 / 309 | 6 |

The move receipt reports 300 removed old paths while reusing all 302 payloads. Branch replacement explicitly removes six paths, changes six and creates six replacement paths. Mixed fixtures include Rust, Vue, notebook, Markdown requirements, OpenAPI and repeated boilerplate; recorded outlines and retained citations are checked by the harness.

There is exactly one sample for each metadata measurement: new-worker read 775.208971 ms, initialized-worker read 24.674407 ms and worker initialization 30.358608 ms. The report's `p50_ms` and `p95_ms` fields consequently contain the same single observation. **No p95/tail-latency distribution is established.** Cold means a new MCP worker, not flushed filesystem caches; warm measurements reuse one initialized worker and exclude initialization. CLI wall times include process startup.

The initial and single-edit sampled executable RSS values were 71,262,208 and 73,707,520 bytes. Their CPU counters were 10,070,394 and 12,851,013 microseconds; sampled storage read/write counters were respectively 11,309,056 / 7,700,480 and 126,976 / 5,709,824 bytes. Linux CPU is measured by final `wait4`; RSS/I/O use post-exec procfs sampling at 20 ms. Sampling can miss short-lived/final growth, and lifetime RSS may include launcher memory. Concurrent builds and caches prevent treating these observations as controlled release comparisons.

### Final release metadata samples

A separate short campaign used the final Linux release binary, the same original 302-file initial fixture and the existing `read_samples` harness: five fresh MCP workers, each followed by twenty warm reads. All 105 metadata responses matched the same immutable representation and source bytes remained unchanged. No edit/move/retention campaign was repeated. Cold-worker read p50/p95 were 59.139/60.690 ms; 100 warm reads were 17.664/21.906 ms; worker initialization p50/p95 were 4.780/36.452 ms. Concurrent Windows/Linux compilation continued and OS caches were not flushed, so these are observed small-fixture distributions, not performance guarantees.

The final Linux binary is 187,549,592 bytes, SHA-256 `553fad0a17d5f186b887e95addecd4e53b5df2fe62d255a6961d9eb833703fc1`. Runtime image size is 266,917,950 bytes, image identity `sha256:ea863c2487cf9f631bf47f40d4fe380cd1c5167407200e30ab7754b0ee6d8e2f`. The additional local receipts are `target/v2-release-reads.log`, `target/v2-release-read-output/report.json`, `target/v2-release-linux-audit.log` and `target/v2-container-smoke.log`.

Retained logical bytes were 10,103,292 after the one small edit and 39,108,003 after the extended workload. These include usage metadata and do not establish a retention plateau or storage savings. The separate final 200-edit Rust qualification passed as recorded above.

## Actual local-model counters

The supplied original model is `original bounded benchmark vectors`, schema 1, Apache-2.0, two dimensions and a 39-byte vocabulary. Its full asset SHA-256 is `c04fabdffc7fb80b32abb1f1de970ebb8964e4ab1f1169053d898e718c43c14c`. It is a tiny original word-vector fixture, not ONNX or a pretrained model.

| Operation | Cache hits | Cache misses | Inferences | No-vector results | CLI wall time, ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Cold index | 0 | 2 | 2 | 0 | 410.834 |
| Unchanged index | 0 | 0 | 0 | 0 | 185.133 |
| One-file edit | 1 | 1 | 1 | 0 | 430.728 |

These are reported inference/cache counters, not inference avoidance guessed from byte counts. The unchanged pass retained generation 1 and reused both files. The edit published generation 2 with one indexed and one reused file. Its vector query completed in 61.863 ms and returned exact retained text/citations for `login.txt` and `invoice.txt`; local-model logical state was 404,581 bytes. This verifies this fixture's cache behavior and evidence binding, not retrieval quality on held-out tasks.

## Native package and installed editor

The Windows release executable is 191,555,584 bytes, SHA-256 `77d570e56aa6a89e29e4941f93dca5e6bf98938c1f6501f57c380d3734c874bb`. The verified unsigned ZIP is 30,067,634 bytes, SHA-256 `6444b68d43755e329f4002b60b23b3b8780367ea75b44891a1b471f629978f0c`. The corrected VSIX is 31,180,659 bytes, SHA-256 `8fa8fbd07f98ca49f26f44e337e97c0d0acf130ef9afc53bf9e66ebfb11028f8`.

Candidates reside under `artifacts/local/v2-final-20261014/native/publish` and `artifacts/local/v2-final-20261014/vscode-fixed/publish`. The earlier `vscode/publish` candidate failed installed-editor activation and is superseded. Its runtime allowlist omitted three modules; packaging now checks all eight modules and their literal relative-import closure before staging. The corrected candidate passed a real VS Code extension-host smoke in an isolated profile: command registration, packaged helper, index/full reindex, sixteen MCP tools and storage selection preserving old state. Receipts are `target/v2-package-native.log`, `target/v2-package-extension-fixed.log` and `target/v2-editor-smoke-fixed.log`.

The native manifest truthfully records commit `7ab7e21a1fef3e8016ef43394b9051cd0e831b5d` and a dirty checkout. Later corrections affect tests, documentation and VSIX packaging, not the native payload. The notice bundle covers 466 packages, preserving four existing missing-separate-license-text flags. These are local candidates, not signed releases. The shared registry and all 58 adapter rows are included in this Windows package; this does not provide independent native macOS/Linux qualification for those rows.

## Real Codex client trial

Codex CLI `0.162.0-alpha.17.2` completed an ephemeral read-only trial with the final Windows release over stdio MCP in 32.088 seconds. Four original fixture files supplied a Rust definition, Cargo manifest, README build command and document requirement. Per-invocation configuration ignored user configuration, required the AIRK server and disabled `airk_search`; no persistent client configuration was changed. Configuration follows the [official MCP client contract](https://learn.chatgpt.com/docs/extend/mcp?surface=cli).

All three requested findings were correct: `cedar_total` at `src/lib.rs:1–3`, `cargo test` at `README.md:3`, and the exact indexed publication token. Eight completed AIRK calls used list-files, symbols, status, file-info, read-file twice, changes and context. One read requested lines beyond the indexed source and failed explicitly; the client recovered with bounded context. There was no disabled-search call, shell command, edit or build. The answer correctly distinguished unverified live freshness, selected syntax without type/target resolution, unsupported change dimensions and unavailable baseline coverage. All four original source hashes were unchanged.

Local receipts are `artifacts/local/v2-codex-trial/events.jsonl`, `answer.txt` and `receipt.json`. The eight serialized result objects total 13,651 UTF-8 bytes when compactly serialized by PowerShell `ConvertTo-Json -Depth 100 -Compress`; this is a result-size observation, not wire bytes or a token/billing-savings claim. This one read-only trial establishes neither session resumption nor live-byte verification before edits, concurrent edits, stale-path recovery, document/embedded-code editing or paired-client task quality.

## Outstanding qualification

Paired Claude Code/Codex tasks, independent held-out relevance judgments, macOS execution and native non-container Linux qualification remain outstanding. Claude Code is absent from PATH on this machine; Codex CLI is installed and authenticated. The local-model benchmark does not qualify pretrained ONNX inference or semantic relevance. Physical full-disk, power-loss and network-filesystem campaigns are not established by injected fault tests.

The benchmark also does not expose parser-internal invocation or inventory-deserialization counts, per-stage staging peaks/component-write totals, or a standalone scheduler/pinned-reader campaign. Successful timings cannot establish absence of full-inventory parsing. The original small fixtures do not qualify large repositories, every language construct, every platform, signing, installation or client policy behavior.
