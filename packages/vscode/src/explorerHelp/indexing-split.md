---
id: indexing.split
title: Indexing - Split review and recovery
route:
area: explorer
aliases: split, partition, recovery
related_airk_help: indexing
---
# Split review and recovery

Open Indexing, Index library. Its initial metadata page loads automatically; use Refresh indexes after a failed read or Next page for more entries. Choose Split index on an eligible active row, or Open index followed by Split index. Opening the review starts no native job.

1. Load the source folders: click Read complete native split baseline. The full native baseline supplies roots, policy and every affected Index set; a library page alone is not a complete census.
2. Choose what moves to the new index: select a Source folder from the native dropdown, enter a path relative to it, and choose Subtree or Exact file. For example, subtree docs transfers eligible descendants; exact file docs/guide.md transfers only that file. A blank subtree path selects that source root subject to native validation.
3. Choose the destination and review limits: enter a destination name, absolute data root and collection. Expand Index set memberships for the destination and review each deliberate choice. Enter destination growth, temporary, original rewrite and cleanup byte ceilings; semantic readiness and destination watching are explicit options. Choose Prepare complete native split review, inspect the complete native result, then Apply reviewed split.

The destination is independent and the original remains as the remainder. Exact files stay exact, subtrees include future eligible descendants, permanent exclusions apply to both, and source files stay in place.

Split review requires explicit native policy contract 2. Read the complete native source baseline before choosing exact files or subtrees, a separate destination storage location, each affected Index set membership, and funded growth, temporary, rewrite and cleanup ceilings. Recommendations under Search scope, Help me choose indexes can also open this review only. Ancestor ignore controls can be required outside the selected document paths; permanent exclusions still apply. Preparing a split performs no indexing or grant change.

Operation status and recovery contains Read native split status for a known native operation ID and exact receipt/recovery controls. Apply submits the exact reviewed native draft once. Before submission, local tracking reserves 192 KiB for that split within the shared 2 MiB and 64-entry journal limits; insufficient tracking capacity refuses submission. Check native receipts performs read-only reconciliation after a lost response or restart. Missing or expired evidence does not erase the last known accepted snapshot or authorize replay. Explicit recovery requires a fresh native status read. Cancel before the original fence can require destination cleanup; after the fence it requires restrictive completion rather than restoring previously excluded evidence.

Complete catalog details and exports and the operation details retain complete native drafts, capabilities, snapshots and future fields. Historical recovery acknowledgments, the last known full accepted snapshot and the latest current-state lookup have separate labels. Physical cleanup and funded allocation release must finish before completion; a successful logical cutover alone does not establish reclaimed bytes.

A metadata continuation acknowledgment preserves the recorded phase, proofs and existing job history. It returns an active operation to pending, or preserves canceled destination cleanup while clearing its exact attention latch. It does not report a new worker job. The complete native acknowledgment remains separate from later operation snapshots.

Original remainder verification is a bounded, read-only native worker job. Its witness retains original publication, path, expected hash and observation interval without source text. Source verification does not transfer evidence before the fence. Frozen terminal publication and funding acknowledgments have their own revision, progress and attention latch. A completed logical receipt with unfinished finalization stays tracked; explicit Resume binds the reviewed envelope revision. A historical accepted acknowledgment cannot clear a later finalization failure.

Native wire budgets are separate: each operation receipt is at most 14 KiB, a complete split snapshot is at most 46 KiB, and a recovery acknowledgment is at most 16 KiB inside the 64 KiB response frame. Oversized data is refused without silent truncation.
