# AIRK 1.1.4

Version 1.1.4 includes the [code-review improvements](airkimprovements.md), complete-source usage comparisons, collection vector controls, local model preparation, batched embedding work, and the shared CLI/MCP tool console. Native and extension manifests and lockfiles use the same version.

## Retrieval and discovery

- Warm readers authenticate generation components and shared model assets against their publication receipts. Changed bytes cannot silently produce different evidence under the same token.
- Hybrid search fuses private candidates before loading budgeted result text. Immutable chunk ordinals distinguish separate chunks even when their path and source line are identical. Sparse path filters and source aliases use bounded matching vector records; exhaustion remains explicit.
- Inventory, symbols, imports, code/document facts and outlines have revision-bound continuation cursors. Compact outlines expose names, kinds, locations and keys without rich metadata. Changed filters/publications fail explicitly, and oversized records report the budget they need. See [discovery contracts](discovery.md).
- MCP runs complete accepted requests, including usage accounting, on bounded blocking workers. Control calls have reserved capacity; excess work receives a retryable error. Shutdown drains accepted work. Usage comparisons authenticate only their required catalog projection. See [execution and verification](request-execution.md).

## Ingestion and extension

PDF parsing uses a local worker with page/output checks, wall/CPU deadlines and platform memory controls. Linux/Windows enforce allocation limits; macOS samples physical memory and terminates over-budget workers, allowing sampling overshoot. Source reads verify opened handles and reject redirected traversal; Windows providers enter their containment job before execution. CLI setup refuses hard-linked, redirected and oversized shell profiles. The [hardening document](ingest-hardening.md) describes platform differences and remaining race-model limits.

The extension preserves the collection's vector opt-out and prepares explicitly pinned local model assets through its trusted setup flow. Indexing batches compatible embedding work. The AIRK Tools terminal uses the same MCP handlers and schemas as coding agents. Dashboard rendering, collection tree and agent setup now have separate owner modules; existing command IDs remain valid.

New generations retain compact measurements of complete cited sources, including JSON/MCP framing. Comparisons use the response's exact revision and count each source once. Negative reductions remain visible, unavailable reasons remain explicit, and evidence requests are separate from background activity. Historical missing comparisons stay unmeasured; estimates do not establish provider tokens or billed savings.

## Upgrade

1. Install the matching native package or platform VSIX from [GitHub Releases](https://github.com/iokaio/airk/releases), or update the Marketplace extension when its separate publication is available.
2. Reload VS Code and run **Index now**. The source-context and isolated-PDF profile identities deliberately publish new evidence where required, even if source bytes are unchanged.
3. Restart connected agent MCP sessions so they use the new helper. An older helper may reject the new generation specification.
4. Check the intended collection with `airk_status`, then retrieve a known phrase. Discovery clients can adopt cursors incrementally; existing calls retain their defaults.

Existing 1.1.3 usage history stays readable. Vector policy changes apply per collection. Library hosts that use PDF extraction must dispatch the ingest worker at process startup; the distributed CLI does this before creating its runtime. The worker never falls back to unrestricted in-process parsing.

## Validation and qualification

Local validation uses Windows, Rust/Cargo 1.98.0, Python 3.13.7 and Node 24.21.0. Node differs from the documented 24.15.0 development pin. Final local check results appear below. Hosted evidence is linked through [PR/CI history](https://github.com/iokaio/airk/actions/workflows/ci.yml) and the [release workflow](https://github.com/iokaio/airk/actions/workflows/github-release.yml); publication is established by the versioned release record, not by this document.

| Final local check | Result |
|---|---|
| Workspace build, formatting and clippy with warnings denied | Passed. |
| Complete workspace tests and doc-tests | Passed, including all 211 kernel integration tests; designated worker/model/campaign ignores remain explicit. |
| Extension with rebuilt native helper | 82 passed, 2 skipped: POSIX shell startup and separately supplied old-helper upgrade. |
| Python contracts | 123 tests, 4 platform/optional-input skips. |
| Repository links/SPDX/guidance and whitespace | Passed for 665 public files; contributor guidance is byte-identical. |
| License bundle | Passed for 467 packages; four declaration-only upstream gaps remain below. |
| Persistent MCP qualification | Passed all nine judgments on both fixture sizes, with 27 warm samples per usage mode and 8 concurrent samples per size. |

The persistent qualification recorded recall@5 = 1.0 and MRR@5 = 0.944 for both fixture sizes and usage modes. Warm p50/p95 latency was 76.2/188.1 ms (usage off) and 132.1/244.8 ms (on) with 32 distractors; with 256 it was 113.8/240.7 ms (off) and 186.1/306.8 ms (on). Four-process p50/p95 was 254.3/277.6 ms and 336.8/382.1 ms. Windows peak working set for the serial processes ranged from 25.4 to 37.4 MiB. These debug measurements ran alongside other validation on uncontrolled caches. The tested binary SHA-256 was `eff9620daa8e13168f51088458328770fc17683a3e3478976a1b47293a2b7950`; its receipt identifies a stable, dirty working tree and is not a clean-commit build attestation. Hosted CI supplies the final committed-source receipts.

Regression coverage includes warm/fresh component corruption with usage enabled/disabled, hybrid padding and same-line identity, sparse prefixes/aliases, bounded vector comparisons, complete discovery pagination and history expiry, request cancellation/drain/accounting, shell-profile hardlinks/replacement, protected source handles, PDF failures/page limits, and immediate provider descendants.

The qualification harness runs nine fixed retrieval judgments outside its indexed fixtures on 32- and 256-distractor workloads, with usage enabled/disabled and four concurrent MCP processes. Receipts identify binary SHA-256, source commit/tree state, judgment digest, platform, latency distributions, ranking/recall and available process counters. Each native CI job retains its own receipt. Synthetic vectors are original adapter fixtures, not learned model quality evidence; operating-system caches are uncontrolled.

A separate Windows debug measurement used 16 and 256 small files and ten repeated samples. Usage baseline verification read 199,080 / 960,945 logical bytes versus 232,802 / 1,421,472 whole-generation component bytes, excluding shared model assets. Baseline lookup p50/p95 was 10.9/16.3 ms and 27.2/27.6 ms. Outer request p50/p95 was 11.7/16.4 ms and 48.6/50.3 ms with usage disabled, and 35.4/58.4 ms and 88.6/105.4 ms with usage enabled. These are small debug fixtures, not performance targets or physical-I/O measurements. Whole-catalog hashing remains proportional to catalog size.

Notice generation verifies 467 packages. Four upstream packages still lack standalone license texts: `adobe-cmap-parser`, `htmlescape`, `pdf-extract` and `type1-encoding-parser`. Their authentic publisher declarations now have pinned source URLs/digests; this records provenance and does not invent legal approval. See [third-party notices](../THIRD_PARTY_NOTICES.md).

Real pretrained model quality, installed-editor upgrade/rollback and service lifecycle, hostile/network filesystems, larger-than-memory workloads, physical full-disk/power-loss, and causal provider savings remain separate qualification tasks. Hosted platform results qualify only their recorded scenarios.
