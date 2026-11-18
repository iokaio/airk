# Graph-only provenance preparation and retained input custody

This owner implements a separate preparation operation and a graph-only execution
route. Implementation and synthetic controls are not a native GPU result. A fresh
reviewed collector build, linked image, manifest, metadata and preparation receipt
are prerequisites. Component, runtime-admission and placement modes retain their
existing verification paths.

The historical full parent/aggregate verification took 124.155 seconds. It is
performed once in preparation under a separate maximum 600-second monotonic
deadline, bounded by the campaign deadline. That time is not model latency.
Graph execution retains its original 120-second whole-phase deadline and
five-second process-cleanup limit. Neither operation retries automatically.

## Preparation and approval

Run the maintained wrapper in the verified human shell. All paths below are
absolute, hashes are actual reviewed SHA256 values, and destinations are fresh
guarded paths. The execution output root is frozen during preparation.

```text
python -B -S scripts/cuda-component-execution.py --prepare-graph-provenance
  --approved-script-sha256 SHA
  --metadata-receipt PATH --metadata-receipt-sha256 SHA
  --image-packet PATH --image-packet-sha256 SHA
  --powershell PATH --output-root FRESH_EXECUTION_ROOT
  --graph-preparation FRESH_PREPARATION_RECEIPT

python -B -S scripts/cuda-component-execution.py --prepare-graph-diagnostic
  --approved-script-sha256 SHA
  --graph-preparation REVIEWED_RECEIPT --graph-preparation-sha256 SHA
  --plan FRESH_GRAPH_PLAN

python -B -S scripts/cuda-component-execution.py --execute-reviewed-graph-diagnostic
  --approved-script-sha256 SHA --plan REVIEWED_GRAPH_PLAN --plan-sha256 SHA
```

The first command checks the complete maintained environment/5, incremental
build-proof/3 and historical proof/2 ancestry through the existing verifier,
including actual historical tools, headers, objects, archives and original
metadata input seals. Current input files and verifier sources are held immutable
while those checks run. The second command requires the explicitly reviewed
preparation receipt SHA and reopens its exact current boundary; it does not run
the full historical verifier again or launch a model. Its own preparation work
has a maximum 120-second deadline. The third command requires a separately
reviewed graph plan SHA.

The private preparation schema is
`airk.private.cuda-graph-provenance-preparation/1`. It records the complete seed
plan, exact binding and source identities, human/default-owner/session identity,
actual UTC and monotonic observations, opened/closed handle counts, close errors,
and each current object's FilePin, volume/file identity and owner/ACL policy.
Preparation is unqualified and does not admit a runtime or product profile.
The measured preparation interval ends after full verification, final input
reobservation and handle release. Receipt retention follows the existing
separately bounded checked-guard write/audit policy and is explicitly disclosed
outside that interval. A failed write or audit returns no successful receipt pin.

## Current boundary versus historical derivation

The boundary is derived from relationships, not a caller-supplied allowlist:
current source owners, verifier roster and recipe helpers; raw producer, recipe
and historical proof documents; source patch/new source/object/session/aggregate;
actual CargoJSON, image and manifest/runtime producer; metadata receipt/plan/build
and all six metadata outputs; complete runtime files; required native source
roster; current model files, source manifest, numeric fixture and reference
producer; maintained ownership helpers and output-policy sources.

Historical headers, unused objects and the 95 unchanged parent aggregate inputs
remain authenticated derivation evidence in the preparation result. They are not
copied from the seed plan's original input seals into each execution boundary.
Preparation verifies those old seals; execution does not claim a new historical
build-tree census. The boundary limit is 512 objects including the preparation
receipt and final graph plan. Unexpected omissions, additions, conflicting pins
and missing original model/source seals refuse.

The output-policy implementation is the maintained
`scripts/cuda-static-runtime/build/r8/provision_cuda_runtime.py`. Its body must
equal the retained historical policy source with only the documented SPDX
prefix. The historical source remains evidence and is never imported by this
graph route. The maintained policy's CPU-owner import is pinned; PowerShell uses
the plan's absolute interpreter and remaining original deadline. No output
policy, identity, owner or ACL is weakened.

## Per-run custody and settlement

Execution starts its original clock before plan/preparation authentication.
Windows opens each current file read-only, with read sharing only and
open-reparse-point semantics. It rejects reparse paths, multiple links, pending
deletion, directories, identity changes and policy drift. Hashing uses bounded
chunks and checks the same deadline. Reopening establishes actual current
identity, not a claim of continuous custody between preparation and execution.

The reviewed plan must exactly equal the prepared seed plan with only the
deterministically derived current input seals and preparation pin added.
Execution rereads binding documents through retained handles and replays current
producer/recipe/ancestor relationships, native manifest and runtime producer,
actual Cargo link, verifier roster and actual nonforwarded executable collector
exports. The PE readers use the retained image handle. Current execution imports
use maintained pinned sources; source handles remain held while dependent
maintained modules import. Historical programs are not executed.

Handles remain held across preflight, native Job execution and settlement,
result validation, final relationship checks, output owner audits and final
output hashes. Final input identity/content/policy checks and all handle-close
attempts run before the final monotonic completion observation. A failed
recheck, close error or expired clock makes the diagnostic incomplete.
Pre-dispatch and native failures also release the acquired boundary. File-handle
settlement never substitutes for the independently required native process/Job
cleanup proof.

The native graph result remains a disposable diagnostic, with no Scorer,
forward, hybrid CPU policy, FP16 readiness or qualification. Selected log
truncation is disclosed only as already permitted by this graph mode; the older
placement mode retains its strict log-completeness rule.

## Validation and readiness

Run `python -B -S scripts/tests/test_cuda_graph_preparation.py` and the existing
`scripts/tests/test_cuda_component_execution.py` controls. The controls use
literal data and injected handles/policies/clocks, including a real bounded PE
parser over a synthetic retained stream. They do not establish actual Windows
share-mode enforcement, real acquisition cost, native compilation or GPU success.

Before the first native run, review the combined source/contract, freeze every
current owner, rebuild producer/helper seals, and measure this exact boundary's
read-only acquisition/replay/recheck cost. If it cannot fit with native work
within 120 seconds, report the blocker. Do not widen the deadline, accept a
cached success flag, omit inputs, retry unchanged failed inputs or admit an
operator-name CPU exception.
