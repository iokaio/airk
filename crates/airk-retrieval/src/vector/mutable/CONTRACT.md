# Mutable DiskANN synthetic seed pruning

The nonlive reserved physical ID0 is a synthetic traversal seed, not an indexed
data vector. Its all-zero vector must not occlude ordinary data neighbors.
For one real vector only, pruning may retain0 to bootstrap the seed's outgoing
edge. Once two real vectors have been allocated, pruning an ordinary node omits0
from the candidate view through DiskANN's supported absent-view entry behavior.
Pruning source0 itself still has its vector. No source vector or distance is
modified; data-query L2, degree32/max48, build64 and original read/search budgets
remain unchanged. No exact fallback or graph repair policy is introduced here.

The graph specification digest domain changes to mutable-diskann-2 and binds
`seed-zero-bootstrap-only`. Existing graph records retain their prior bytes;
they cannot be read as newly corrected graphs merely because code changed.
Fresh native rebuild/replacement is required, preserving original source and
failed graph/capture evidence. This implements the user's approved replacement
policy and does not promise earlier-version compatibility.

The independent64-orthogonal-vector regression first runs against the old
implementation and records BFS/self omissions. It then runs unchanged against
this minimal correction. It must not be weakened if seed isolation alone does
not preserve reachability. A physical successor policy is not part of this
proposal; any further correction needs that independently observed counterexample
and separate design review. Native synthetic graph success does not establish
human-query retrieval quality, resource qualification, or quality-gate success.

Each Pruner owns a local center marker initialized from the immutable inserted
ID in its private OperationContext. Initial prune candidates are distance-sorted
and cannot identify their center by position. For backedge pruning, DiskANN
0.59.0 calls Neighbors.get_neighbors(actual source) before its prune-list fill;
that owning callback records the center. Neighbor writes record their source
as well. The mutable insertion path owns one accessor through these sequential
callbacks; center state is not shared between Pruners. The returned View freezes
the current mask and hides reserved0 even if the working-set Map cached it in
an earlier source0 fill. No distance function is changed. Companion regressions
exercise initial seed-first candidate order, cached source0-to-ordinary-source
reuse, bootstrap and missing insertion identity refusal.

The test-support `self_probe::regeneration_cache` helper imports no edges and
never opens an old graph through the new MutableDiskIndex specification. Its
caller authenticates one frozen old specification, model/pipeline, record width,
scope and head. The existing GraphRecordReader authenticates every original
record, Core validates its metadata, and the owning unchanged fixed-width codec
decodes the vector. Live records yield only physical ID and Core CachedEmbedding;
duplicates, nonfinite vectors, changed scope/counts and deadlines refuse. This
is an explicit private experimental rebuild seam, not previous-version product
compatibility or permission to reinterpret old graph bytes under the new spec.
