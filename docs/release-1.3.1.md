# AIRK 1.3.1

Native, extension and lockfile versions are synchronized at 1.3.1. GitHub downloads and Marketplace publication remain separate workflows, gated by successful CI on the exact main commit.

Different projects sharing one storage root can index concurrently. Native worker ownership now belongs to each collection instead of the entire storage root. Windows opening the same project still share one worker and its durable request revisions. Editor `index --scheduled` and `schedule wait` calls execute only their selected collection; explicit root-wide CLI workers skip collections held by another process.

Source indexing, semantic enrichment and scheduled coverage inventories use the collection worker. Semantic batches yield for new source work or pause in their own collection; another project's pending source work no longer interrupts them. Crash recovery resets only the locked collection's abandoned running claim. Queue transitions and quota reservations retain bounded shared transactions. Collection deletion protects only the target collection, while storage relocation retains exclusive lifecycle protection across the root.

The existing four execution slots and CPU/memory budgets bound concurrent indexing. Exhausted slots leave requests queued without recording an indexing failure. Admission precedes retained model parsing, and loading and inference reuse one lease. Synchronous indexing retains its source-pass admission through semantic completion. The dashboard distinguishes waiting for indexing capacity from a project already indexing in another window or process, preserving committed progress across waits. Shared disk, CPU and memory remain finite; this does not promise identical indexing speed under concurrent workloads.

## Upgrade

No additional index rebuild is required when upgrading from 1.3.0. Upgrades from 1.2 still require the [1.3.0 lexical format rebuild](release-1.3.0.md). Collection IDs, saved settings and the durable 1.2 queue namespace are preserved.

New workers hold a shared lease on the former global worker lock for their entire operation. Older helpers require that lock exclusively, preventing old global claim recovery from overlapping new collection workers. If an older worker is active, close or restart older AIRK editor/watch workers and retry with 1.3.1 or newer. Lock files remain in place; process exit releases actual ownership automatically.

The requested `.gitignore` addition excludes local `.mcp.json` configuration.

## Validation

Regression coverage includes independent collection locks, rejected cross-collection mutations, legacy/new worker exclusion, a real child-process scan concurrent with another project's publication and deletion, selected-project joins, bounded-capacity deferral before model parsing, resumption with only one available slot, and editor wait messages. Existing crash, pause, quota, publication and relocation checks remain required. Local and hosted results will be recorded separately; installed-editor acceptance and performance under representative concurrent workloads are separate qualification.

Local Windows validation on 2026-10-22 passed the locked workspace build, Rust formatting, workspace/all-target Clippy with warnings denied, workspace tests including doctests, repository/link checks, dependency-notice generation and whitespace checks. The workspace test run retained sixteen existing ignored tests. All 200 kernel integration tests and 157 store unit tests passed. The native-backed extension suite passed 148 tests with two explicit skips; Python contracts ran 128 tests with four explicit skips. Notice generation bundled 410 third-party packages and retained the same three upstream missing-separate-license-text review flags. These results qualify the local checks, not installed-editor acceptance or other operating systems; hosted checks must pass before release promotion.
