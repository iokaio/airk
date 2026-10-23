# AIRK 1.3.1

Native, extension and lockfile versions are synchronized at 1.3.1. GitHub downloads and Marketplace publication remain separate workflows, gated by successful CI on the exact main commit.

Different projects sharing one storage root can index concurrently. Native worker ownership now belongs to each collection instead of the entire storage root. Windows opening the same project still share one worker and its durable request revisions. Editor `index --scheduled` and `schedule wait` calls execute only their selected collection; explicit root-wide CLI workers skip collections held by another process.

Source indexing, semantic enrichment and scheduled coverage inventories use the collection worker. Semantic batches yield for new source work or pause in their own collection; another project's pending source work no longer interrupts them. Crash recovery resets only the locked collection's abandoned running claim. Queue transitions and quota reservations retain bounded shared transactions. Collection deletion protects only the target collection, while storage relocation retains exclusive lifecycle protection across the root.

The existing four execution slots and CPU/memory budgets bound concurrent indexing. Exhausted slots leave requests queued without recording an indexing failure. Admission precedes retained model parsing, and loading and inference reuse one lease. Synchronous indexing retains its source-pass admission through semantic completion. The dashboard distinguishes waiting for indexing capacity from a project already indexing in another window or process, preserving committed progress across waits. Shared disk, CPU and memory remain finite; this does not promise identical indexing speed under concurrent workloads.

## Bundled offline model

Native helpers include the pinned FP32 all-MiniLM-L6-v2 weights, tokenizer, manifest and Apache-2.0 license (about 91 MB before package compression). The extension installs from its helper without downloading from Hugging Face or any other site. Native setup uses `airk collections vectors install-bundled`. Bundled and catalog manifests must agree; asset sizes/digests are verified at build and installation. Existing custom models, model identity and CPU batching policy are preserved. Incomplete existing inputs fail rather than being overwritten. Opt-out and native quota/atomic-install protections remain in effect. See [model setup and offline build preparation](onnx-models.md).

## Upgrade

No additional index rebuild is required when upgrading from 1.3.0. Upgrades from 1.2 still require the [1.3.0 lexical format rebuild](release-1.3.0.md). Collection IDs, saved settings and the durable 1.2 queue namespace are preserved.

New workers hold a shared lease on the former global worker lock for their entire operation. Older helpers require that lock exclusively, preventing old global claim recovery from overlapping new collection workers. If an older worker is active, close or restart older AIRK editor/watch workers and retry with 1.3.1 or newer. Lock files remain in place; process exit releases actual ownership automatically.

The requested `.gitignore` addition excludes local `.mcp.json` configuration.

## Follow-up in 1.3.2

The empty-index progressive publication fix and dashboard corrections are recorded in the [1.3.2 release notes](release-1.3.2.md). The validation below remains the historical 1.3.1 record.

## Validation

Regression coverage includes independent collection locks, rejected cross-collection mutations, legacy/new worker exclusion, a real child-process scan concurrent with another project's publication and deletion, selected-project joins, bounded-capacity deferral before model parsing, resumption with only one available slot, and editor wait messages. Existing crash, pause, quota, publication and relocation checks remain required. Local and hosted results will be recorded separately; installed-editor acceptance and performance under representative concurrent workloads are separate qualification.

Before the bundled-model change, local Windows validation on 2026-10-22 passed the locked workspace build, Rust formatting, workspace/all-target Clippy with warnings denied, workspace tests including doctests, repository/link checks, dependency-notice generation and whitespace checks. The workspace test run retained sixteen existing ignored tests. All 200 kernel integration tests and 157 store unit tests passed. The native-backed extension suite passed 148 tests with two explicit skips; Python contracts ran 128 tests with four explicit skips. Notice generation bundled 410 third-party packages and retained the same three upstream missing-separate-license-text review flags. These results qualify the local checks, not installed-editor acceptance or other operating systems; hosted checks must pass before release promotion.

Bundled-model validation on Windows x64 on 2026-10-22 passed the final offline/concurrent installer and quota/opt-out regressions, model-object integrity tests, 128 Python contracts (four skips), and 146 native-backed extension tests (two skips). Explicit build checks rejected missing inputs, a mismatched manifest, incorrect size and a same-size altered model digest. The locally packaged helper passed fresh offline setup through the compiled extension, exact asset hashing, managed CLI identity comparison, real indexing, default/vector/hybrid retrieval, opt-out/re-enable and invalid-asset rollback. The package audit found no runtime downloader or build provisioning script in its 24 runtime modules. This is a local unsigned Windows candidate; manual installed-editor acceptance remains separate. No publication or deployment was performed.

The bundled-model candidate also passed the locked complete workspace build/tests (including doctests), final all-target workspace Clippy with warnings denied, formatting, repository/link checks, generated notices and whitespace checks. Sixteen existing native tests remained ignored. Hosted qualification for the complete 1.3.1 candidate is pending PR CI; manual installed-editor acceptance remains separate. Initial hosted Apple Silicon checks passed the Rust suite but exposed a linked temporary-directory ancestor in the new public model smoke fixture. The fixture now resolves its temporary root before constructing managed paths, preserving the native linked-ancestor rejection. The corrected Apple Silicon model smoke and Linux/container checks passed. Intel Mac passed 955 tests but its concurrent installer fixture encountered the native bounded lock wait: the fixture now requires at least one concurrent installation to succeed, permits only the explicit installation-contention error to retry after both peers exit, and still verifies exact installed hashes and custom-input preservation. The native 30-second wait and all other failures remain unchanged.
