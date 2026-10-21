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

**Retry indexing** checks saved files and continues committed work for the selected collection. **Resume window indexing** resumes automatic work for the open collections in that window. **Rebuild index** re-extracts eligible saved files and recomputes derived evidence after confirmation. All actions revalidate workspace trust and collection/storage identity; source files are preserved.

Missing or damaged cached helper bytes are restored from the digest-verified bundled copy. A damaged bundled helper, permission failure or locked destination remains an explicit error; AIRK does not execute an unverified replacement.

- The extension logs the verified helper version and executable path once per activation. Collection failures include timestamps, so earlier attempts can be distinguished from current work.
- Diagnostic collection checks normalize equivalent Windows drive and extended path spellings, so the helper's canonical root is not rejected as a different workspace.
- Helper failures retain exit code, signal, termination/timeout information and elapsed time. Stderr is bounded while preserving its beginning and end.
- Retained-object errors identify the indexed file, source or extraction object, exact derived-object path and content digest. Encoded objects identify both stored and logical content. Diagnostics do not include source contents; paths can still reveal private project names, so review logs before sharing them publicly.

Source coverage and semantic completion are separate. `discovery_readiness.state: ready` means the source pass completed; `semantic_readiness.state: partial` with no failure means committed semantic work remains. An old error in Output is not proof that the current attempt has failed. Check the current helper, current readiness and whether completion counts advance.

Keep the collection data when investigating a failure. Repeated wipes discard the evidence needed to diagnose it. Use the collection's **Open AIRK Tools** console and `call airk_status` to inspect the same helper and collection. The [operations guide](operations.md) covers recovery and scoped CLI inspection.

## Repair missing derived inputs during indexing

An indexing pass verifies source bytes and extraction profiles before reusing retained objects. If a retained source or extraction object is missing, that reuse becomes a cache miss and AIRK recreates the derived objects from newly captured source bytes. Changed source is published with its new hash; it is never represented as the missing older content. Corruption and permission failures remain errors.

If a newly captured object disappears before publication, indexing can recapture once after releasing the failed scan and proving that no Prepared/Applying journal needs recovery. This admission check retries typed gate contention for up to two seconds, releasing handles between attempts so a diagnostic reader can finish. Repeated loss stops with the precise object diagnostic. Read-only requests and interrupted-journal recovery do not reconstruct missing evidence from changed live files.

When an older extraction is missing and its source has changed or been deleted, indexing now derives the old path's posting memberships from a bounded traversal of the authenticated committed map. It retires those memberships through ordinary captured publication changes. It does not fabricate or recreate the missing old extraction. Corrupt proofs or payloads, incomplete membership, traversal limits and unavailable captured journal inputs still fail explicitly; AIRK does not discard the collection to make repair succeed.

## Validation and limits

Regression coverage exercises native scheduler contention, contention before/after a semantic batch, structured CLI failures with `RUST_BACKTRACE` and `RUST_LIB_BACKTRACE` enabled, capped retry delays, early timer callbacks, verifier/writer overlap, genuine failure blocking, delayed enrichment batch boundaries, raw/encoded missing-object diagnostics, real child-process exit/timeout/output-limit behavior, and stale agent helper configuration. Fault injection deletes newly captured source or extraction objects immediately before progressive publication. Tests check one successful recapture, bounded failure after repeated loss, changed/deleted prior files with missing extractions, reopened exact/lexical/semantic results, and refusal to substitute live files into a pending journal. The 1.2.3 capture-lifetime regressions remain part of the suite.

Model-backed recovery fixtures use canonical temporary storage paths, including on macOS where the temporary directory can use the `/var` alias. Production checks continue to reject linked model paths.

Storage-control fixtures use explicit operation barriers and bounded waits. Failure cleanup releases held operations, drains teardown and restores mocks. An injected missing callback verifies that the test reports the failure and terminates after teardown.

These changes do not establish a new cause for every historical `retained object missing` report. The 1.2.3 capture-lifetime fix remains in place; a new occurrence requires its helper identity, exact missing-object diagnostic and current collection status. Do not describe an unreproduced failure as fixed merely because CI is green.

Native and extension versions and lockfiles are synchronized at 1.2.4. Publication is a separate release step.
