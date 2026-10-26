# Format-150 lexical adapter

Implementation notes, October 26, 2026. Production format-150 collections use
this adapter. Final workload and package qualification remain in progress;
see the [redb acceptance gates](storagekv-design.md) and [release evidence](../release-1.4.5.md).

## Encodings and authentication

The adapter uses scoped ordered trees for term headers, paths, document bindings,
posting directories and forward lists. This refines the proposed height-20
hashed term map: all of these structures use the same bounded ordered-page
implementation, with distinct digest domains. Catalog height-16 encodings and
DiskANN height-32 encodings remain unchanged. The new lexical representation
has its own version and does not interpret preceding engine manifests.

| Storage | Key | Value |
| --- | --- | --- |
| `lex_directory` | Scoped page digest, 32 bytes | Canonical ordered leaf or branch, at most 64 KiB |
| `lex_block` | Term digest followed by first doc key, 40 bytes | Sorted delta block, at most 4,096 keys and 64 KiB |
| `lex_document` | `u64` doc key | Original citation, local chunk ordinal and stable ID; JSON at most 64 KiB |
| Manifest | Head-authenticated canonical JSON | Version 3, engine `airk-kv/lexical-1`, scope digest, term/path/document roots, monotonic allocator and captured-update digest |

The remaining lexical table definitions are reserved scaffolding; the adapter
does not consult their raw rows to establish membership or completeness.

All integer fields below use big endian except the explicitly described varints.
A directory page starts with a one-byte tag and a `u16` entry count (1–128).
Tag 0 contains ordered key/value entries, each prefixed by its `u32` byte length.
Keys contain 1–8,192 bytes; values contain 0–16,384 bytes. Tag 1 contains ordered,
nonoverlapping child references: digest (32 bytes), height (`u8`), count (`u64`),
first key and last key (each with a `u32` byte length). Height is bounded by 16.
Every reference is checked against its decoded page, including its ranges,
count and height. Trailing bytes, duplicate keys and noncanonical encodings fail.

Directory page hashes include the canonical source/collection/incarnation scope
and logical tree domain. Each root commits its digest, first/last keys, count
and height. A missing referenced page is corruption. Membership, absence,
predecessor/successor lookup and half-open ranges use these references; an engine
range scan never supplies a completeness proof.

A posting block contains a `u16` count, its first `u64` doc key and positive
unsigned base-128 varint deltas. Reject redundant varint bytes, overflow, zero
deltas, trailing bytes and a first-key mismatch. Posting-directory values contain
the scoped payload digest, last doc key and `u32` count (44 bytes total).
The term header separately commits the complete posting count and directory
root. Both are checked during full traversal. Overflow splits at the median;
adjacent underfilled blocks merge in key order only when count and byte bounds fit.

Document keys are `(path_number << 16) | local_chunk`. Existing path replacement
retains its number; deletion and later reinsertion allocate a new number.
Numbers are never reused, and allocator exhaustion requires a rebuild. Document
bindings authenticate payload digests. Candidate reads verify those digests,
the stable ID, path allocation and local chunk count before returning a citation.
Stable IDs and long-term aliases retain the preceding adapter's identity domains.

Path entries commit the path number, chunk count and forward root. A forward
tree contains sorted distinct term digests with empty values. Its page count is
independent of the chunk count; a one-chunk path can have more than 8,192 terms.
Replacement verifies the old forward list and affected posting ranges before
removing old memberships. Missing forward terms/pages fail instead of silently
leaving stale postings. Full candidate verification checks paths, documents,
postings and forward references in both directions.

## Work limits and exact results

| Work | Current bound |
| --- | --- |
| Chunks per path | 8,192 |
| Distinct terms per path | 262,144 |
| Term/document pairs per changed path | 4,194,304 |
| Chunk text per changed path | 64 MiB |
| Normalized term set per chunk | 32 MiB |
| Query terms / normalized term bytes | 64 / 16 KiB |
| Query directory/posting page reads | 16,384 |
| Query encoded bytes processed | 64 MiB |
| Query directory entries and decoded posting keys | Shared ceiling of 1,048,576 |
| Query candidates | 262,144 |
| Query elapsed time | 5 seconds, with cancellation checks |
| Publication verification reads | 1,048,576 pages, 8 GiB, 67,108,864 entries/decoded keys |
| Captured journal | Existing shared ceiling of 4,000,000 operations and 1 GiB |

Limits are checked before retaining normalized terms, decoding posting arrays
or copying oversized engine rows. A read conservatively admits the maximum row
size before materialization. Directory/posting exhaustion and query interruption
have typed errors; no exhausted query returns an arbitrary partial success.

Intersections start with the shortest posting list. Request-local membership
caches reuse a decoded block for monotonically increasing candidates. Literal
prefix filtering follows authenticated document/path resolution. All matching
candidates are traversed; a heap retains the smallest `limit + 1` stable IDs.
Results preserve exact stable ordering and the existing `incomplete` meaning.

These bounds still require workload/performance qualification. They are logical
work limits, not a measured process RSS or physical device-I/O guarantee.

## Planning and replay

Planning pins one base snapshot and uses a reservation-owned redb staging file.
Staged directory, block and document rows contain put/delete markers. Updates
retire superseded pages transactionally; pages created and retired wholly within
planning leave no pending operation. Engine snapshots retain their older pages.

A failed plan cannot seal. Sealing verifies the candidate and durably closes
the private write transaction. Canonically ordered afterimages and the manifest
stream into one durable Prepared transaction without loading the publication
into RAM. Applying is a separate durable transition. Installation, verification,
head advancement and pending cleanup share one atomic transaction. A source
error while streaming preparation aborts the transaction.

Captured replay needs neither private staging nor source text or tokenization.
Tests remove staging before installation, retain old readers during Applying,
and reopen both Prepared and Applying states. Whole-publication verification at
sealing/installation is currently conservative; its cost must pass the accepted
performance gate before this adapter is a qualified production replacement.

## Replay admission qualification

Before durable Applying, the native owner rehearses the exact captured table
installation and target verification. It uses a clean ephemeral savepoint,
allocates real padding, samples file high-water before restoration can trim a
free tail, restores the exact Prepared roots and commits allocator bookkeeping.
The owner then persists a descriptor-bound receipt and retains that measured
ceiling in its existing lifetime quota grant. No new logical allowance is
required by the qualified restart/replay case.

The padding policy uses 2 MiB for bounded head/descriptor/master-tree work plus
64 bytes per observed allocated page for multiple copies of eight-byte free-page
numbers and tree overhead. Records leave leaf-header slack so an intended 1 MiB
block does not round to 2 MiB. This is AIRK's conservative policy for pinned
redb 4.4.0; it is not a public engine API guarantee of future allocation. Windows
tests cover mixed small/large rows, fragmentation, a live old reader, process exit
at Applying, exact full root/collection limits, unchanged lifetime grants and an
8 MiB quota. Broader power-loss and native platform qualification remain open.
The extra probe transaction and writes enter the required performance comparison.

A real backend denial can poison redb's cached handle. New admission reports a
typed drain/reopen condition while retained snapshots exist, then reopens the
same owned scope. A quota-denial test verifies cold old-record reads, the exact
Prepared descriptor and safe abort after this reconciliation. Cached success is
not evidence of continuous availability through failed I/O.

Status separates actual file length from admitted and unused allowance. Manual
compaction's reclaimed bytes describe logical file shrink; necessary admitted
replay/growth allowance remains protected and is not claimed as released quota.
