# AIRK 1.2.4

Version 1.2.4 fixes a reproduced extension stall after temporary publication contention, avoids unnecessary small source batches after slow semantic work, and improves failure diagnostics. Agent Lean remains the default for new collections; existing choices, storage format and embedding identity retain their meanings.

## Continue after temporary publication contention

Another native writer can prevent a status read while it publishes. Previously, the extension could finish a semantic batch, fail its next status read with `current index is busy applying an update`, and permanently block the remaining batches. A regression reproduced this with one file left to enrich even after the competing writer was gone.

The extension now keeps semantic work pending when the native scheduler reports `busy` or a semantic command/status read reports the specific current-gate `WouldBlock` error. It yields through the existing scheduler interval and tries again. Pause, workspace closure and storage changes still apply between attempts. Missing retained objects, permission failures, interrupted journals requiring recovery and integrity failures still stop work with an error.

## Keep source batches useful during slow enrichment

Scheduled scanning interleaves source batches and semantic enrichment. The source-batch timer previously started before enrichment, so a slow semantic batch could immediately exhaust the next source interval and repeatedly publish very small batches. The timer now starts after enrichment finishes. Count/byte limits and pause checks remain in force. This removes avoidable publication work; it does not establish a universal indexing-time improvement.

## Make failures diagnosable

Each collection has a **Diagnose & Repair** button; the command palette also exposes **AIRK: Diagnose & Repair**. It observes the selected helper and collection, shows current readiness separately from previous attempts, and can copy a bounded report. Diagnostics do not request source text or launch a scan. Recent helper identity, failure, repair and completion events remain on the local machine across extension reloads.

Saved readiness alone does not prove retained data is intact. Refresh checks a bounded page of retained source and extraction inputs. **Verify retained data** continues through pages of the same exact publication, with cancellation and a two-minute time budget. The report identifies how many files were checked, failures and oversized inputs that were skipped. An incomplete check or a changed publication is not reported as a fully verified index. This check does not verify every graph or lexical index record.

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

If a newly captured object disappears before publication, indexing can recapture once after releasing the failed scan and proving that no Prepared/Applying journal needs recovery. Repeated loss stops with the precise object diagnostic. Read-only requests and interrupted-journal recovery do not reconstruct missing evidence from changed live files.

Repair requires enough verified inputs to preserve the publication contract. For example, if an older extraction is missing and its source has changed or been deleted, updating the prior index records can still require that missing extraction. Retry and full reindex are not guaranteed to recover that case; AIRK preserves the failure and does not silently discard the existing collection.

## Validation and limits

Regression coverage exercises native scheduler contention, contention before/after a semantic batch, genuine failure blocking, delayed enrichment batch boundaries, raw/encoded missing-object diagnostics, real child-process exit/timeout/output-limit behavior, and helper identity logging. Fault injection deletes newly captured source or extraction objects immediately before progressive publication. Tests check one successful recapture, bounded failure after repeated loss, changed source identities, and refusal to substitute live files into a pending journal. The 1.2.3 capture-lifetime regressions remain part of the suite.

These changes do not establish a new cause for every historical `retained object missing` report. The 1.2.3 capture-lifetime fix remains in place; a new occurrence requires its helper identity, exact missing-object diagnostic and current collection status. Do not describe an unreproduced failure as fixed merely because CI is green.

Native and extension versions and lockfiles are synchronized at 1.2.4. Publication is a separate release step.
