# AIRK 1.2.4

Version 1.2.4 fixes a reproduced extension stall after temporary publication contention, avoids unnecessary small source batches after slow semantic work, and improves failure diagnostics. Agent Lean remains the default for new collections; existing choices, storage format and embedding identity retain their meanings.

## Continue after temporary publication contention

Another native writer can prevent a status read while it publishes. Previously, the extension could finish a semantic batch, fail its next status read with `current index is busy applying an update`, and permanently block the remaining batches. A regression reproduced this with one file left to enrich even after the competing writer was gone.

The extension now keeps semantic work pending when the native scheduler reports `busy` or a semantic command/status read returns a structured current-gate contention outcome. Only the native gate's typed `WouldBlock` error authorizes that retry; stderr wording and Rust backtrace settings do not. Repeated contention waits 1, 2, 4, 8, 16 and then at most 30 seconds between attempts. Successful progress, manual retry and resume reset the delay. Pause, workspace closure and storage changes still apply between attempts. Missing retained objects, permission failures, interrupted journals requiring recovery and integrity failures still stop semantic work with an error.

Helper startup preserves gate contention if a writer starts after scope discovery. Temporary gate contention does not publish failed semantic state. If recording a genuine semantic failure also fails, AIRK returns the original error type with the recording problem as additional context; a secondary busy gate cannot turn corruption or a permission failure into a retryable outcome.

## Keep source batches useful during slow enrichment

Scheduled scanning interleaves source batches and semantic enrichment. The source-batch timer previously started before enrichment, so a slow semantic batch could immediately exhaust the next source interval and repeatedly publish very small batches. The timer now starts after enrichment finishes. Count/byte limits and pause checks remain in force. This removes avoidable publication work; it does not establish a universal indexing-time improvement.

## Recheck scheduled work when timers fire

An editor timer can fire before the source-scan deadline, including after a wall-clock adjustment. Previously, the scheduler could treat that callback as permission to run semantic work even when none was pending. An error from that premature command could then block the source scan. A deterministic regression reproduces the wrong command and blocked source work.

The scheduler now rechecks which work is pending and due before dispatching it. If neither a source scan nor semantic work is due, it schedules another wake-up. Pending source work remains queued; this does not clear genuine failures or override pause and storage-selection holds.

## Make failures diagnosable

Each collection has a **Diagnose & Repair** button; the command palette also exposes **AIRK: Diagnose & Repair**. It observes the selected helper and collection, shows current readiness separately from previous attempts, and can copy a bounded report. Diagnostics do not request source text or launch a scan. Recent helper identity, failure, repair and completion events remain on the local machine across extension reloads.

Saved readiness alone does not prove retained data is intact. Refresh checks a bounded page of retained source and extraction inputs. **Verify retained data** continues through pages of the same exact publication, with cancellation and a two-minute time budget. Verification is disabled while the editor is indexing or enriching that collection, and stops if work starts between pages or before accepting a result. The report identifies how many files were checked, failures and oversized inputs that were skipped. An incomplete check or a changed publication is not reported as a fully verified index. This check does not verify every graph or lexical index record.

Diagnostics also inspect known project agent configuration files and the Codex user configuration for this collection. They flag a different helper path, including a saved 1.2.3 cached helper after an update, or a different root/storage binding. Unsupported syntax and bounded or redirected reads remain explicitly partial. This is configuration inspection, not proof of a connected agent or an exhaustive search of machine settings. Use **Connect Coding Agent** to review the current configuration and update the client entry; AIRK does not silently rewrite it.

An entry pointing at the extension's managed `cli/bin` copy counts as current when its SHA-256 digest matches the verified extension helper. Comparison requires a successful matching helper probe, stable regular files, a five-second read allowance and a 256 MiB limit per executable. An unavailable comparison is reported as unresolved. Arbitrary configured commands are never launched or hashed for this exception.

**Retry indexing** checks saved files and continues committed work for the selected collection. **Resume window indexing** resumes automatic work for the open collections in that window. **Rebuild index** re-extracts eligible saved files and recomputes derived evidence after confirmation. All actions revalidate workspace trust and collection/storage identity; source files are preserved.

Missing or damaged cached helper bytes are restored from the digest-verified bundled copy. A damaged bundled helper, permission failure or locked destination remains an explicit error; AIRK does not execute an unverified replacement.

- The extension logs the verified helper version and executable path once per activation. Collection failures include timestamps, so earlier attempts can be distinguished from current work.
- Diagnostic collection checks normalize equivalent Windows drive and extended path spellings, so the helper's canonical root is not rejected as a different workspace.
- Helper failures display the parsed native message and retain exit code, signal, termination/timeout information and elapsed time. Raw stderr is bounded while preserving its beginning and end. Structured parsing precedes display truncation, including when JSON escaping expands Windows paths beyond 8 KiB.
- Retained-object errors identify the indexed file, source or extraction object, exact derived-object path and content digest. Encoded objects identify both stored and logical content. Diagnostics do not include source contents; paths can still reveal private project names, so review logs before sharing them publicly.

Source coverage and semantic completion are separate. `discovery_readiness.state: ready` means the source pass completed; `semantic_readiness.state: partial` with no failure means committed semantic work remains. It does not prove that the scheduler is running: a failure can block the queue even if the index cannot record failed semantic readiness. An old error in Output is not proof that the current attempt has failed. Check the current helper, queue state, current readiness and whether completion counts advance.

Keep the collection data when investigating a failure. Repeated wipes discard the evidence needed to diagnose it. Use the collection's **Open AIRK Tools** console and `call airk_status` to inspect the same helper and collection. The [operations guide](../operations.md) covers recovery and scoped CLI inspection.

## Repair missing derived inputs during indexing

An indexing pass verifies source bytes and extraction profiles before reusing retained objects. If a retained source or extraction object is missing, that reuse becomes a cache miss and AIRK recreates the derived objects from newly captured source bytes. Changed source is published with its new hash; it is never represented as the missing older content. Corruption and permission failures remain errors.

If a newly captured object disappears before publication, indexing can recapture once after releasing the failed scan and proving that no Prepared/Applying journal needs recovery. This admission check retries typed gate contention for up to 120 seconds, releasing handles between attempts and backing off polling to 250 ms so a foreign diagnostic reader can finish. Pause stops automatic repair before another scan. This bounded grace does not guarantee that every slow verification page finishes. Repeated loss stops with the precise object diagnostic. Read-only requests and interrupted-journal recovery do not reconstruct missing evidence from changed live files.

When an older extraction is missing and its source has changed or been deleted, indexing now derives the old path's posting memberships from a bounded traversal of the authenticated committed map. It retires those memberships through ordinary captured publication changes. It does not fabricate or recreate the missing old extraction. Corrupt proofs or payloads, incomplete membership, traversal limits and unavailable captured journal inputs still fail explicitly; AIRK does not discard the collection to make repair succeed.

Missing extractions across one publication share a single complete traversal. Recovery collects all affected paths before that traversal and bounds their pending memberships separately. Posting identification checks the authenticated key's collection-scoped domain before strict decoding, so JSON from other domains with `selector` or `shard` fields cannot cause a false posting failure.

## Reject silently read-only SQLite writers

On Windows, a handle that denies write sharing can make SQLite silently open a requested writable connection as read-only. Its setup queries succeed, then the first journal write fails with SQLite error 8. A separate fixture reproduces the same SQLite error with genuine read-only file permissions, so error 8 alone does not authorize a retry.

AIRK now checks that the connection is writable before configuring or using it. If SQLite fell back, a native read/write open that does not create or modify files distinguishes Windows sharing denial (OS error 32) from other access failures. Sharing denial permits bounded reopening for up to five seconds with polling capped at 250 ms. If native access has already recovered, one immediate reopen is allowed. SQLite and probe handles are closed between attempts; the caller keeps its publication guards. Permission denial remains an immediate error, and persistent sharing fails with its native cause. This does not retry arbitrary SQLite write errors or change permissions. Other platforms reject an unexpectedly read-only writer explicitly.

## Release abandoned publication reservations safely

Failed preparation can leave a durable quota reservation even when SQLite rolls back to Ready before a Prepared journal exists. A subsequent retry could stop with `current quota grant already exists; adopt it for recovery`. This was reproduced on a preserved copy of the failed qualification index and in a smaller rollback fixture.

Ready-state recovery and preparation now release these abandoned reservations after proving that no pending journal owns them. Ready recovery also verifies the committed lexical component first. Cleanup validates every candidate reservation's collection incarnation before releasing any; other collections and reservations still needed by Prepared/Applying recovery remain protected. This releases quota accounting, not source files or committed evidence, and does not advance the publication token.

## Validation and limits

Regression coverage exercises native scheduler contention, contention before/after a semantic batch, structured CLI failures with `RUST_BACKTRACE` and `RUST_LIB_BACKTRACE` enabled, capped retry delays, early timer callbacks, verifier/writer overlap, genuine failure blocking, delayed enrichment batch boundaries, raw/encoded missing-object diagnostics, real child-process exit/timeout/output-limit behavior, and stale agent helper configuration. Fault injection deletes newly captured source or extraction objects immediately before progressive publication. Tests check one successful recapture, bounded failure after repeated loss, changed/deleted prior files with missing extractions, reopened exact/lexical/semantic results, and refusal to substitute live files into a pending journal. The 1.2.3 capture-lifetime regressions remain part of the suite.

Additional regressions cover 32 missing prior extractions sharing one traversal, unrelated-domain JSON, readers held beyond two seconds, pause before recapture, escaped structured messages, installed-CLI digest matching, real Windows sharing and permission failures, database/cold-journal preservation, and retry after a rolled-back preparation leaves a quota reservation. Reservation tests preserve pending recovery, foreign incarnations and other collections; Ready recovery also refuses cleanup when committed lexical data fails verification.

Model-backed recovery fixtures use canonical temporary storage paths, including on macOS where the temporary directory can use the `/var` alias. Production checks continue to reject linked model paths.

Storage-control fixtures use explicit operation barriers and bounded waits. Failure cleanup releases held operations, drains teardown and restores mocks. An injected missing callback verifies that the test reports the failure and terminates after teardown.

These changes do not establish a new cause for every historical `retained object missing` report. The 1.2.3 capture-lifetime fix remains in place; a new occurrence requires its helper identity, exact missing-object diagnostic and current collection status. Do not describe an unreproduced failure as fixed merely because CI is green.

A local Windows qualification on 2026-10-21 exposed a separate SQLite read-only publication failure on candidate `80b7cfe`, after 6,697 of 7,626 selected files completed semantic indexing. The queue recorded the failure even though the index could not publish failed readiness. Original derived state was preserved. A controlled copy then reproduced the abandoned-reservation retry failure described above. The sharing and permission fixtures prove distinguishable mechanisms; the process or access condition responsible for the first corpus event has not been identified. Neither stopped run is counted as a completed qualification.

Native and extension versions and lockfiles are synchronized at 1.2.4. Publication is a separate release step.
