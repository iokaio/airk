# Proposed graph provenance preparation boundary

Status: contract proposal for primary review; not implemented or an execution authorization. Existing component, runtime-admission and placement modes remain unchanged. The current graph implementation still performs full verification and must fail when its original120s deadline expires.

## Why a distinct preparation operation

Observed maintained historical parent plus aggregate verification took124.155s. Repeating that full tool/source/build-tree census before and after native graph capture cannot fit one120s diagnostic. No limit or qualification gate changes. Build provenance can be verified in a separate preparation operation and preserved under its own source/input identity, while execution reopens the exact immutable artifacts it consumes.

Preparation has one separate600s monotonic deadline, never borrowed from or reported as model latency. It verifies the complete environment/5 -> incremental proof/3 -> historical proof/2 chain with the maintained owner, including all original source/tool/object/archive relationships. It then records a fresh preparation receipt under a reviewed create-only path. Actual timestamps, elapsed time, human/default-owner identity and output custody are recorded. Timeout, source drift, incomplete ancestry, missing output or retention failure refuses preparation.

## Proposed retained preparation receipt

Private schema `airk.private.cuda-graph-provenance-preparation/1` binds:

- The preparation owner and exact maintained verifier-source roster, including the checkpoint extraction helpers, maintained output policy and execution guard.
- Environment/5, canonical build-proof/3 and aggregate-proof/1 content hashes; raw fresh producer, recipe and all historical receipt identities needed to replay those relationships as data.
- The actual new source, object, session archive and aggregate, the preserved native image, native build receipt, actual CargoJSON, fresh native runtime manifest and the runtime producer copy named by that manifest.
- The preserved image packet and its static graph binding, exact collector exports, and the newly emitted metadata receipt/plan/request/profile that bind this fresh manifest. The old3179 metadata receipt and f9fac producer remain historical; neither is updated or accepted as the new identity.
- An exhaustive deduplicated boundary table of absolute FilePins plus actual volume/file identity, link count, size and owner/ACL policy, and the observed verification result. Aliases are recorded as distinct objects; equal bytes do not imply equal file IDs.

The primary reviewer approves the complete receipt SHA before it can be supplied to graph-plan preparation. A caller-provided `verified:true` flag or a receipt whose source/data relationship cannot be replayed is insufficient. The receipt itself is source/input evidence, not qualification, runtime admission, or a reusable session.

## Per-run reopening under the original120s

Graph plans pin that exact preparation receipt and the same metadata/image/binding/source boundary. At execution, the parent starts the original120s clock before preflight. It rechecks schema and all data relationships without importing or executing historical artifact code: fresh raw producer -> canonical proof/3 -> aggregate -> environment/5; actual Cargo link -> preserved image; native manifest/static_core_build -> raw fresh producer; metadata request/profile -> that exact manifest; and both actual nonforwarded executable collector exports.

For every boundary object, acquire a retained read-only Windows handle with read sharing only, no write/delete sharing and open-reparse-point semantics. Reject aliases/reparse points, unexpected links, owner/policy drift or a different file identity. Hash the opened object in bounded chunks under the original cutoff and compare its exact FilePin. Retain those handles throughout dispatch, native Job settlement and final proof. Reobserve identities, hashes and policy before release; no path-only stat or cached boolean substitutes for the opened object. Source/manifest/receipt handles also prevent replacement while their code/data is being consumed. Native protected runtime handles, actual process image authentication, native manifest checks and existing resource/cleanup proofs remain independently required.

The execution boundary includes only current artifact/source evidence needed for this image and invocation. It excludes old CUDA/SDK headers, unused compiler objects, the95 unchanged input archives and other historical build-tree files after their exact identities have been authenticated by full preparation. Their hashes remain in proof/3 and its ancestors as historical derivation evidence; execution does not claim a new build-tree audit. A changed current archive, image, manifest, raw producer, recipe, source, CargoJSON, metadata or preparation receipt invalidates the graph plan.

Preparation of the exact boundary and later acquisition must distinguish current identity equality from continuous custody: no claim that a file was never touched between operations. The runtime claim is exact current bytes/identity at acquisition, protected through this invocation, followed by actual final reobservation. No clock-age heuristic replaces those facts.

All closure acquisition, chunk hashing, owner/policy checks, relationship replay, PE reads, native execution and final proof use the same original120s deadline. The existing5s process cleanup is not extended. A failed or expired final recheck cannot complete a diagnostic. Receipt-only retention remains separately disclosed as in the existing parent contract. Unknown native cleanup remains unknown even if file-handle cleanup succeeds. The closure reports actual opened/retained/closed handles and errors; release is not a fabricated native cleanup proof.

## Required controls before use

Reject omitted/extra boundary paths, repeated/conflicting pins, false preparation SHA, old producer/manifest/metadata substitutions, wrong Cargo/image export linkage, changed verifier/policy source, reopened wrong file ID, content or ACL drift, write/replacement races, and every expired acquisition/hash/deep-validation/final-release boundary. Test that a valid prepared boundary avoids full historical verifier invocation during execution while retaining exact relationship checks, and that only this graph mode uses the new route. Test retained-handle cleanup on preparation refusal, pre-dispatch refusal, native failure and final-proof failure.

Measure this exact boundary's acquisition/hash/audit cost read-only before the first native graph run. The observed124.155s full-verifier path is not retried unchanged. If the current artifact boundary still cannot fit alongside native work, report the blocker; no larger120s budget, truncation waiver, unchecked cache or diagnostic qualification follows.
