# Reviewed index splitting

P10 is implemented in the unpublished **1.8.8 working build**.
Frozen Windows qualification passed all 22 gates; focused Mac ARM batches passed
with their Cargo targets cleaned. [Exact evidence and limits](research/implementation-v190.md#p10--local-phase-checkpoint-5-november-2026)
retain distinct source identities and resource/platform coverage. P10 has passed its local phase checks; the working build remains unpublished.
This guide describes that working-build behavior.

## Review the partition

In **Explorer → Indexing → Index detail → Split an index**, read the complete
native split baseline before preparing a review. It contains the original source
roots and policy plus every affected Project membership. A displayed library page
is a bounded subset and cannot establish that census. Split availability requires
an explicit native policy capability of at least contract 2.

Choose original source-root IDs and exact files or directory subtrees, a separate
destination storage location and collection, Project memberships, and growth,
temporary-space, original-rewrite and cleanup ceilings. Exact files remain exact;
a subtree includes future eligible descendants. Review the native canonical paths,
source-ID mappings, affected Project effects, storage action, quota policies and
control dependencies before applying. Stale identities or revisions require a new
review. Oversized or unsupported reviews refuse without dropping fields.

Review the original index's pending and running jobs before applying. Witness
admission and cutover wait for that queue to drain. Pausing preserves queued work;
it can therefore leave a split waiting until that work is completed or explicitly
canceled through the existing job controls. Splitting does not discard those jobs
or change the original's Pause setting.

The destination is an independent index over the selected portion of the original
policy. The original becomes the remainder. Existing positive includes constrain
both partitions, and permanent denies remain authoritative in both. The split
cannot restore previously excluded evidence or widen eligibility through an ignore
negation. Source files remain read-only: splitting changes derived index data and
catalog membership, never moves, edits or deletes original documents.

Discovery can require qualified ancestor ignore controls outside positively selected
document paths. The complete native review discloses these bounded control
requirements; they do not admit sibling documents or arbitrary model assets.
Permanent denies, protected no-follow reads and capture budgets still apply. A
required denied or unsupported control input produces an explicit refusal.

Review each affected Project's original membership and deliberate destination
choice. Destination choices can omit membership, preserve the reviewed scope, or
use explicit scopes mapped from the original source IDs. Projects not assigned a
destination keep the original remainder membership. Membership choices do not
issue evidence grants or change saved defaults. Existing grants affected by the
cutover require the native review/renewal path before further access.

The capacity ceilings are explicit admission limits, not measured file sizes or a
prediction of savings. Splitting costs a destination build and temporary space;
querying both indexes can also add work. Recommendations may open the review when
the native capability is available. Opening advice, reading the baseline and
preparing a split never starts a job, reserves build capacity or changes a grant.

## Apply and observe native work

Apply submits the exact reviewed native draft once. Native persists its **Accepted**
intent before destination preparation, funding allocation or child work. Partial
preparation and funding remain owned by that operation; a failure cannot be treated
as evidence that no effects occurred. Admission accounts destination growth,
temporary work, original rewrite and cleanup through durable owner allocations.
Child writers use those allocations rather than an unrelated ordinary reservation.
Insufficient capacity leaves an explicit incomplete operation, not a successful
partition.

The destination stays sealed while its independent publication is built and
validated. Ordinary evidence reads cannot use the worker's private permit. The
original remains admitted under its existing policy before the transfer fence.
A bounded read-only native worker must also verify a positive remainder witness
against the original publication and saved source hash. A failure to find a witness
within its bounds leaves nonempty status unknown; it does not prove the remainder
empty. Before sealing the original, native drains affected owners and checks the
exact retained publication and witness hash again.

After the transfer fence, the original publishes the restrictive remainder and
native applies the reviewed Project memberships. The destination becomes eligible
through the recorded activation path. The reviewed destination watching choice
applies at activation; advice and preview cannot enable watchers. Existing native
admission, pause and source-fairness rules remain in force. Optional semantic
readiness is an explicit reviewed requirement, distinct from publication or
physical cleanup.

## Cancellation and exact recovery

Before the original transfer fence, explicit Cancel preserves the original's
admitted policy and stops the split through owned destination cleanup. Accepted
cancellation is not proof that a running builder has drained or that derived bytes
have been purged. Native waits for the exact child and requires cleanup plus release
and acknowledgment of the operation's funding. A failed destination cleanup stays
canceled with attention; explicit recovery can continue cleanup without reopening
the build.

After the fence, cancellation cannot undo the transfer or restore previously
excluded evidence. The operation requires restrictive completion. A pending frozen
finalization cannot be reported as already complete merely because its logical
receipt is terminal; Cancel refuses that incomplete finalization. Resume is the
explicit continuation path.

A lost response, disconnect or restart requires the saved caller, request key,
operation identity and native request digest. Read the exact acceptance or recovery
receipt; never replay a mutation automatically, choose a replacement key, or infer
no effect from missing/expired evidence. Missing evidence preserves uncertainty and
the last known full accepted snapshot. A fresh native status is required before
explicit Resume or Cancel, including the exact receipt digest and any finalization
envelope revision.

A worker retry names an actual accepted child. A metadata continuation preserves
the recorded phase, jobs and proofs; it returns active work to pending or preserves
canceled cleanup while clearing only its attention latch. It does not invent a new
worker. Historical recovery acknowledgments remain separate from later current
snapshots. Re-reading an old accepted acknowledgment cannot clear a newer failure
or attention latch.

## Completion has separate observations

The operation reports its native phase, run state, dated witness, funding receipts
and cleanup evidence. Logical cutover, physical cleanup and final funding
acknowledgments are separate:

- A validated destination is still sealed before the recorded cutover/activation.
- Membership activation does not prove physical reclamation or release of every
  funded writer allocation.
- A frozen terminal target can be `publishing` while the current receipt remains
  nonterminal. A logical terminal receipt can still be `acknowledging`, with an
  explicit attention code. The write barrier and local tracking remain until
  actual finalization finishes.
- Finalization `complete` requires the recorded terminal publication and the actual
  per-root funding acknowledgments. Canceled cleanup has the same final settlement
  requirement. Neither an earlier acceptance nor a historical receipt proves this.

Workbench retains complete native details and JSON/Markdown copies, including future
fields, the historical recovery acknowledgment, last known accepted snapshot and
latest current lookup. It reserves 192 KiB of local tracking capacity before each
split mutation within the existing shared 2 MiB/64-entry journal limits; insufficient
tracking space refuses dispatch. Native budgets are separate: each operation receipt
is at most 14 KiB, a snapshot 46 KiB, a recovery acknowledgment 16 KiB and the complete
response frame 64 KiB. Oversized results refuse rather than silently truncate.

## Native controls and MCP scope

The administrative native RPC and CLI console transport use these complete Core
controls. Read-only controls share the canonical native owner; Apply and Recover
are explicit mutations:

| Control | Purpose |
| --- | --- |
| `catalog_split_source_state` | Read the canonical original roots, policy and complete affected-Project baseline. |
| `catalog_split_draft` | Prepare the complete revision-bound native review from explicit choices. |
| `catalog_split_apply` | Accept the exact reviewed draft once. |
| `catalog_split_status` | Read the current operation snapshot without advancing work. |
| `catalog_split_receipt` | Read the exact caller/key acceptance receipt and current snapshot. |
| `catalog_split_recover` | Submit explicit recovery against the reviewed receipt and finalization revision. |
| `catalog_split_recovery_receipt` | Read the exact recovery acknowledgment independently of later current state. |

Evidence MCP remains read-only and scoped to its reviewed Project/index grant.
It does not expose these machine-library administrative controls. Recommendations
can supply capability-qualified review guidance; Explorer Assistance suggestions
wait for the user and do not submit native work. Existing CLI/MCP/Workbench evidence
reads enforce the resulting source policies through their canonical read handlers.
Actual native and transport qualification must be recorded separately from renderer
or controller tests; see the ledger for the candidate's current limits.
