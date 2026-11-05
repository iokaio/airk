# Permanent source exclusions

P9 implements permanent exclusions in the unpublished 1.8.8 working build.
Local Windows and focused Mac ARM qualification passed; measured coverage and
remaining platform/release limits are tracked in the
[implementation ledger](research/implementation-v190.md).

Permanent exclusions belong to an index. In **Explorer → Indexing → Index detail
→ Sources**, select the native source-root identity, an exact file or a directory
subtree, and review the complete native operation before applying it. A subtree
also blocks future eligible descendants. Query filters and temporary index
selections do not create permanent exclusions. Another index covering the same
source has its own policy.

Native admission binds the review to the index, source and policy revisions and
the actual native runtime. It drains the affected index's native owners before
persisting the successor policy and its exact configuration job. A stale review
must be prepared again. A lost acknowledgement requires observation of the saved
operation, rather than submission with another request key. Evidence MCP remains
read-only; asking an evidence tool to verify or retrieve a source cannot trim it.
Native request, receipt and metadata-frame byte budgets also apply: an oversized
review is refused rather than silently dropping existing rules or affected Projects.
Read-only review can report retryable busy while accepted work settles. That
receipt does not authorize replaying a mutation or bypassing native admission.
For a policy-change review, `file_data_changes` describes the queued change to derived
index files. It does not authorize changes to original source files.

The exclusion takes precedence over includes, ignore negation and ordinary
indexing choices. Native readers enforce it separately from Project membership,
including a membership covering every source. Previous grants require explicit
review and renewal. Policy-bound removal publishes the remaining retained
evidence without reading or changing source documents. Ordinary evidence stays
unavailable while that removal is pending or its policy binding does not match.
Rebuild and repair must retain the exclusion. Unsupported earlier policy readers
refuse the new persistent format.

The stable managed collection owner records the required policy reference without
changing its original ownership digest. A missing enforcement capsule cannot be
treated as an empty policy, including when no retained publication remains.
Earlier native owners reject the required field before opening or purging the
collection.

Retiring an index removes its derived incarnations while preserving the bounded
enforcement capsule, stable owner and exact current policy-job receipt. The
inactive registration remains solely to bind that receipt and the catalog
tombstone; it schedules no work. If an admitted trim failed before writing a
capsule, retirement preserves a pending enforcement copy, without fabricating a
removal publication. Policy observations retain their original job and dates;
retirement has its own completion proof. Fresh cleanup resumes and ordinary
re-registration of the retired backing refuse rather than clearing its policy.
Native source-root changes on a policy-bearing index also refuse until they have
a qualified policy-reconciliation adapter. Removing the last deny retains the
required policy format and does not silently erase its source bindings.

Naming checks use exact UTF-8 equality and positively qualified ASCII case
equivalence. They do not guess filesystem Unicode collation from a general text
comparison API. Related names with unresolved equivalence, or an unsupported
case-sensitive/casefold boundary, produce an explicit unsupported outcome.
Unrelated paths proved disjoint by earlier components remain eligible. Unicode
document content is unaffected by these filename checks. The ledger records
the actual filesystem cases tested; this is not universal filesystem qualification.

If an excluded file is an ignore control input that the current ignore adapter
cannot omit safely, discovery, watcher planning and saved-source verification
refuse that traversal. They do not read the excluded rules or silently change
ignore semantics. Retained evidence already published under the policy remains
readable. Explicitly reviewed unblocking can restore traversal.

With active restrictions, ignore controls are captured through protected
no-follow readers and supplied as lines to the pattern matcher. Discovery,
coverage, watcher planning and verification share this adapter. Normal nested
rule precedence and explicit credential-file whitelists remain effective;
permanent denies still win. Configuration snapshots allow at most 64 KiB per
control, 1 MiB of charged control input and 32,768 cached directories per matcher.
Redirected/unreadable controls, ambiguous global excludes-file values and Git
worktree/commondir indirection refuse until qualified; they are not silently
omitted. Control inputs use this separate bounded allowance rather than the
saved-document verification byte count. Empty restrictions retain the ordinary
ignore adapter. These are supported-input limits, not a filesystem sandbox claim.

Global patterns retain the ordinary adapter's process-working-directory base.
A control failure is latched for the whole matcher snapshot: repairing the file
does not silently change that observation; a fresh operation captures new controls.

Administrative settings, usage, semantic imports and custom model/calibration
inputs authorize both the selected spelling and its resolved path before bounded
capture. Protected readers retain that exact path and refuse later redirected
components. Pinned inputs verify their digest on execution. Model assets and
calibration definitions/tokenizers use protected captures rather than reopening
unverified source paths during installation. These protections preserve input
limits and CPU-profile selection; they do not make source files immutable.

Coverage refresh and managed rebuild validation use the same discovery predicate
before source capture or hashing. Coverage caches bind the policy digest; a
pre-trim cache cannot return excluded paths or counts after admission. A revoked
policy epoch refuses inventory work and delivery rather than publishing a cache
under another policy. Permanently pruned files are not scanned to estimate a
complete blocked-file count.

The native `blocked` flag remains true while application is pending or the policy
contains deny/include restrictions. After the last restriction is explicitly
unblocked and applied, it becomes false; retaining the required policy format
alone does not mean evidence is still blocked.

The complete policy observation distinguishes these outcomes:

- The durable policy is recorded and blocked paths cannot be delivered.
- The exact configuration job is pending or failed; removal is incomplete.
- The removal publication, or an explicit empty-index outcome, is recorded.
- Physical cleanup is pending or complete, with measured reclamation counts,
  unknown byte counts and any cleanup error preserved.

Physical cleanup can remain pending after logical removal succeeds. Recovery
resumes the recorded policy operation through native admission; it does not
silently remove the deny. Retained upgrade backups, delivered results and saved
exports are separate copies. This feature makes no secure-erasure claim.
Policy status describes the current operation. An exact cleanup-resume request
for an operation superseded by a later reviewed policy change refuses instead of
replaying old work; its original job receipt remains separately queryable.
The read-only exact resume-receipt lookup binds the caller, request key, index
and operation to the accepted job, complete configuration and native result.
Workbench reconciles an uncertain response using this receipt even when a newer
reviewed operation is current, retaining the newer status separately. Missing,
expired or pruned evidence remains unresolved and never authorizes replay.

Removal can retire the complete imported-analysis component and embedding cache
to prevent cross-file references from retaining blocked evidence. The application
report names these retired components. Regenerate imported analysis explicitly
under the current source policy when it is needed. Semantic and discovery work
retain their own readiness observations; removal does not imply they completed.

**Unblock path** is a separate reviewed change naming the exact trim rule.
It does not remove split-transfer fences. Ordinary indexing does not unblock
paths, and unblocking does not promise immediate restoration of previously
removed evidence. Apply the supported explicit indexing operation afterward
when that evidence is needed, preserving pause, manual-update and vector policy.
A terminal failed or canceled trim may also require a separately reviewed
unblock to leave an unsupported naming boundary. That change fences the prior
job's retry authority and preserves its failure receipt; it does not claim the
original removal succeeded.

CLI management uses the same native catalog source-mutation draft, reviewed apply,
policy status and explicit resume operations as Explorer. Full observations remain
available in structured details and copies. A policy read failure uses the typed
`source_policy_unavailable` result; narrower evidence must not be presented as a
complete successful result when the requested read cannot be safely enforced.

For example, save this read-only request as `policy-status.json`, substituting the
selected native index ID:

```json
{"catalog_policy_status":{"index_id":"idx_00000000000000000000000000000000"}}
```

```console
airk service rpc --runtime-dir <private-control-directory> --request-file policy-status.json
```

The response preserves the complete policy snapshot and its job observation.
Use the native draft and reviewed apply path for mutations; editing the index's
retained files or request receipts is not a supported management operation.

The native service owns cross-process mutation and worker drainage. Independently
constructed offline library processes are not an alternative supported management
transport and do not establish that native admission or drainage occurred.
Diagnostic rebuild preparation also requires the native owner's explicit policy
authority when damaged retained metadata cannot establish it. It must not discard
the remaining policy evidence and infer an empty policy after reopening.
Installed editor interaction, platform qualification, exact runtime identities
and any unavailable coverage belong to the dated ledger rather than this usage
contract.
