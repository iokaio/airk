# AIRK 1.8.6

Prepared November 1, 2026. Native and extension versions, lockfiles and the offline template catalog are synchronized at 1.8.6. Publication is pending the PR, exact-main CI and the protected GitHub and Marketplace workflows; source changes alone do not establish publication.

## MCP request capacity

Successful MCP `status` and `diagnostics` responses now include top-level `query_concurrency`, for example `"query_concurrency": 16`, in both JSON text and structured content. Public names and the `airk_status`/`airk_diagnostics` aliases behave alike, in terse and full modes.

The field reports configured frontend evidence capacity observed for that request's admission, not free slots or guaranteed admission. Default capacity is 16; Management → Service → Concurrent requests… offers 4, 8, 16, 32 and 64. The reserved status/control slot is additional. A multi-project bridge uses the minimum configured backend limit. The bridge reuses its existing admission policy read: reporting adds no configuration read, collection access or service RPC. Cached index snapshots retain their publication, observation time and cache age while the capacity field reflects the new request's policy observation. Error receipts are preserved.

A queued native-channel caller now searches the complete fixed pool after acquiring its permit. This prevents a panic when it queued under a lower limit and another caller raises capacity before it resumes. A deterministic regression retains the first four channel leases throughout the increase and checks both queued callers complete without releasing accepted work.

## Included unpublished changes

This release includes the preceding local 1.8.3–1.8.5 work:

- Scoped identifier discovery, extraction filters, explicit suffix/contains matching, qualified counts, bounded reference paging/file summaries and source-bound navigation hints across MCP, CLI and Workbench. See the [1.8.3 record](release-1.8.3.md) and [1.8.4 record](release-1.8.4.md).
- Bounded concurrent admission, independent native channels and matching project-worker capacity, with live policy updates and draining reductions. See the [1.8.5 implementation record](release-1.8.5.md).
- Status observation supersession and cold-admission fixes, Windows directory-permission handling, and retained Workbench tool/session choices. Historical local observations and their limitations remain in the original records and [MCP investigation results](../mcp-improvements-test-results.md).

The change retains source read-only behavior, collection/root isolation, exact revision identity, bounded queues and typed retry contracts. No new dependencies, agent orchestration or external services are added.

## Validation and publication

The bridge regression exercises all capacity choices, both tool aliases, terse/full replies, live policy changes over a reused cached snapshot, preserved publication/time/cache metadata, matching JSON text and structured content, unchanged error receipts, and exactly one policy read per request. Existing admission and native-channel tests cover reserved status capacity, drain behavior and independent channels.

Local Windows checks before the source commit:

- All 13 MCP bridge tests passed, including live `query_concurrency` reporting without an additional policy read. The queued-channel increase regression also passed in the workspace CLI library run.
- Formatting, workspace/all-target/all-feature Clippy, repository checks, exact 1.8.6 version synchronization and staged whitespace checks passed. AGENTS.md and CLAUDE.md remain byte-identical.
- Extension tests: 537 passed, 19 skipped, zero failures. Python repository/release contracts: 291 run, six skipped, zero failures.
- Release notices cover 366 third-party packages; dependency advisory checks reported no policy failures, retaining the existing reviewed dispositions.
- The initial full workspace run stopped on three actual-process recovery tests because the default temporary namespace did not qualify as a native private boundary. An initial fixture setup also inherited a noncanonical ACL and did not resolve that precondition. In a new canonical owner-only fixture base, the exact already-built recovery executable passed all three tests with one explicit subprocess-helper ignore. No production boundary or test assertion was weakened. The complete workspace rerun with that documented fixture override is in progress.
- A fresh release runtime pair was built from native source digest `7c8502610fcca8e7b7ceb12cf1530c002ff10da87b12244b2af7cb0d38defa03` (748 inputs). Five four-CPU managed/native/installed smoke rounds and the final twenty-scenario MCP replay are pending; the replay must identify the committed source and matching runtime before PR creation.

The complete combined change received an explicit primary-agent review of contracts, service/client/worker admission, source/revision fences, cursor/filter behavior, Windows storage eligibility, Workbench persistence/exports and CI selection. Review found and corrected the live-increase channel-pool race described above and removed links to ignored local-only scenario ledgers. This was a single-agent review, not an independent review.

Hosted CI and publication are still pending. Local Windows results do not establish macOS/Linux qualification or manual installed-editor acceptance.
