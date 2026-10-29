# AIRK 1.6.0 release candidate

Native and extension versions are synchronized at **1.6.0**. The Windows x64
local VSIX is built and package-verified. Publication requires successful PR and
main CI followed by the protected release workflows; this record does not claim
those future outcomes. Previous release records retain their historical scope.

## Changes

- Embed both pinned MiniLM INT8 weight files, tokenizer and notices in the helper.
  Deliver the CPU inference libraries and their notices in the verified runtime
  payload. Installation, first use and repair require no model download.
- Select unsigned INT8 on x64 without VNNI and signed INT8 on VNNI-capable x64 and
  Apple Silicon. Keep the catalog's platform evidence distinct from CPU support.
- Make a clean break from the tract FP32 embedding implementation. No bundled
  FP32 model, old FP32 inference path or implicit FP32 fallback.
- Durably schedule semantic-index replacement for enabled registered projects on
  upgrade, including projects whose VS Code workspace is closed. Retire old FP32
  vectors/caches, resume interrupted work, preserve text/fact search and explicit
  semantic opt-outs, and display the upgrade rebuild in Overview and Management.
- Bind model weights, preprocessing and backend policy to the semantic pipeline;
  never mix vectors or queries from incompatible pipelines. Use batch size one
  for dynamic INT8 inference because the experiment found batch-dependent output.
- Reject edited, obsolete or nonselected ONNX definitions before changing saved
  settings. Failed imports preserve the current model and searchable publication.
- Retry typed, temporary storage contention for native settings/report reads
  within one deadline, preserving the original request and cursor.
- Package pinned native CPU inference libraries and their licenses on Windows,
  Linux and both Mac architectures. The bundled Mac library requires macOS 13.3
  or later. Keep inference dependencies and weights out of the main executable.
- Retain the [Guidance plan](explorer/guidance-plan.md) and its design mockups as
  documentation for a later release. No Guidance functionality is implemented.

## Upgrade behavior

The service records semantic setup durably for registered projects, including
closed workspaces. Paused projects wait until resumed. Replacement uses retained
source evidence, replaces the graph and embedding cache in one publication, and
queues new semantic work. Text and facts remain available. Semantic opt-outs stay
off and retire their old vectors. Source files are unchanged.

Both models ship in the installer. Setup, first use and repair do not fetch model
files or inference libraries. Only maintainers provisioning build inputs use the
explicit download commands. Build and package checks reject FP32 weights or any
additional model profile.

## Experiment evidence before integration

See [Semantic index optimization](semantic-index-optimization.md) for reproducible inputs,
asset hashes, measurement scope and limitations. The local Windows AVX2 benchmark
used the published 1.5.12 helper and ONNX Runtime 1.22.1, with two fresh processes
per profile and identical 2,048-passage Bookbuilder samples.

| Profile | Passages/second | Peak working set (MiB) | Known-file top-five hits |
|---|---:|---:|---:|
| Released native FP32 | 12.70–12.76 | 321.9 | 10/10 |
| ORT FP32 control | 17.02–17.23 | 214.3–215.7 | 10/10 |
| ORT unsigned INT8 | 21.50–21.59 | 123.0–126.2 | 10/10 |
| ORT signed INT8 | 24.89–25.01 | 121.9–123.3 | 9/10 |

These are embedding component and small file-location retrieval measurements,
not full index/hybrid-search or cross-platform qualification. The source-hash
audit found no changes to the 4,556 inspected source files. Captured passages,
query labels, vectors and raw reports remain private and ignored.

## Local validation — Windows x64, 28 October 2026

- Full workspace all-feature tests and doctests passed with no failures,
  including the long catalog, retention and cleanup workloads. Opt-in tests
  remain identified as ignored; the real-model checks below ran separately.
- Actual helper inference for both INT8 exports: normalized and bounded outputs,
  exact per-text results across regrouping, cache/bypass and tail retrieval.
- Semantic upgrade regression: retained text survives without live source files;
  old graph/cache replacement, restart, idempotence and opt-out are covered.
- Registry upgrade regression: closed and paused registrations retain durable
  setup work; removed projects are excluded.
- Final release pair: concurrent direct/MCP clients, protocol negotiation,
  recovery, diagnostic receipts, collection isolation and preserved sources.
- Offline installed runtime: election, process names, client shutdown, stable
  launcher repair, exact-payload upgrade/rollback, retirement and resumable
  uninstall.
- Real transformer qualification: default/vector/hybrid retrieval, persistent
  DiskANN, invalid-input rejection, opt-out/re-enable and source preservation.
- Public DOCX/XLSX CLI smoke and the installed native extension MCP smoke.
- Final-source Linux container build and smoke: two isolated collections,
  read-only stdio MCP, offline runtime, persistent restart and paired helper.
- Formatting, all-target/all-feature Clippy, version synchronization, repository
  checks, whitespace and workflow linting. Python contracts: 180 passed, four
  platform-dependent skips. Extension suite: 371 passed, 12 native-dependent
  skips, with its real installed-MCP smoke run separately above.
- Local unsigned Windows VSIX: **65.56 MB**, with its complete allowlisted payload
  verified. SHA-256:
  `8909d7be1fd7dd9e65d608d1a685811b455d99e915af4f19362b29bc27c9d5dc`.

These checks do not establish native Mac signing/notarization, ARM or VNNI
performance, or full Bookbuilder indexing time. Hosted platform CI and release
receipts establish their own scope; the earlier component benchmark is not a
whole-application memory guarantee.

## Mac packaging correction — 28 October 2026

The first PR CI run exposed a bundle-layout error: ad-hoc signing treated the
inference library's license text in `Contents/MacOS` as unsigned nested code.
Both Mac architectures failed at that signing step; Windows and Linux completed
their native, installed-runtime, MCP and package checks, and the container passed.
The notices now reside in `Contents/Resources/onnxruntime`, while the native
library stays beside the helper. Staging and every packaging/release validator
use the same path mapping; hashes, licensing and signature checks remain required.

The correction passes 181 Python contracts with four platform-dependent skips,
including staging/inventory parity for both Mac architectures, Windows and Linux.
The Windows/Linux layout is unchanged, and the corrected local Windows native
pair rebuilt and passed package verification. Native Mac signing and the remaining
release checks still require successful hosted results after this correction.
