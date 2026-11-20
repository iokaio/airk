# CUDA component and diagnostic execution

The maintained owner is `scripts/cuda-component-execution.py`; its exact behavior
and evidence bounds are in this directory's CONTRACT.md. It prepares reviewed
plans and runs one owned native test process under the original deadline, Job
memory limits, current human/default-owner identity and sealed input closure.

Use `--help` to select an explicit preparation or reviewed-execution mode.
Component mode requires its original numeric, CUDA-only placement and operations
journal evidence. Runtime-admission, placement-only and final-graph diagnostics
have separate selectors and schemas. A complete diagnostic is not a component,
model, hybrid-policy, resource-fit or release qualification. Runtime refusal can
be faithfully diagnosed without becoming admission. Graph capture may disclose
incomplete capped selected logs; the older placement mode still refuses overflow.

Preparation needs actual authenticated metadata, a preserved image packet,
reviewed source/tool hashes and a fresh output location. Final-graph plans also
need the new static producer, dedicated provenance verification, native manifest
and actual image collector exports. Historical metadata cannot be relabeled to
stand in for a new producer. See the owning contract before creating a plan.

Run pure controls with `python -B scripts/tests/test_cuda_component_execution.py`.
Tests do not launch native images or models. All filesystem operations require the
verified human execution path and output-parent/postcreation ownership checks.
The helper does not switch accounts, alter ACLs, extend budgets or grant admission.
Retained private receipts identify evidence; implementation belongs in maintained
source, not executable artifact snapshots.

The separate graph-only preparation and retained-input route is documented in [PROVENANCE-PREPARATION.md](PROVENANCE-PREPARATION.md), including the three explicit preparation/review/execution commands. Pure controls are in `scripts/tests/test_cuda_graph_preparation.py`. Fresh source, producer, image and metadata seals plus review remain prerequisites; implementation alone is not execution evidence.

For placement triage, run this read-only report from the verified human shell.

    python -B -S scripts/cuda-component-execution/placement_report.py --graph-receipt <absolute-execution.json> --graph-receipt-sha256 <reviewed-sha256>

It verifies the complete owned receipt and its pinned sibling
native-result.json, validates the full graph, and writes JSON to stdout. It
counts CPU incident values and producer-to-consumer crossings by graph-local
value ID. Each value counts once per graph and once per reported provider
direction if it crosses multiple directions. It preserves unknown types and
subgraph bindings and lists shared values without a local producer separately.
Crossings describe observed placement edges. For fixed-shape INT64 and BOOL
values, the report also counts ranks and static payload bytes. Dynamic and
other shapes remain separate. Allocator overhead and actual transfers require
runtime profiling.

The separate [Compact four-width profiling owner](BOUNDED-HYBRID.md) reuses the
retained graph, held CUDA build provenance and independent references.


## Windows Large FP32 graph capture

Run with the checked Windows human identity, exclusive access to the A6000,
and fresh host and device headroom. Use the exact Large graph-only request
and an image selected from the successful Cargo JSON lib-test output. Stage
the image by verified copy beside the request-pinned CUDA runtime manifest.
Review the staged image hash and actual build receipt before execution.

```text
python -I -B -S scripts/cuda-component-execution/large_graph.py
  --request LARGE_PROBE_REQUEST
  --image STAGED_INFERENCE_EXE --image-sha256 SHA
  --build-receipt BUILD_RECEIPT --build-receipt-sha256 SHA
  --powershell VERIFIED_PWSH --output FRESH_PRIVATE_RUN_DIRECTORY
```

The private result contains the final optimized placement and graph
inventory, with actual 48 GiB process and aggregate Job checks and the
original 120-second deadline. The runner retains the owned process and
settlement receipt, native result and stdout. This operation creates and
settles a disposable session and dispatches zero forwards.


The reviewed Large FP32 batch-view candidate uses this graph-only selector.
Pass its freshly emitted native Probe request as `--request` and add these
two arguments together.

```text
  --candidate-metadata OWNED_BATCH_VIEW_METADATA_RECEIPT
  --candidate-metadata-sha256 SHA
```

The admitted receipt identifies the exact predecessor request, batch-view
graph, preparation and generated profile. The launcher checks all 36 CUDA
mask Add nodes and records the two FP32 rank-three input values and output
value for each. Only CPU and CUDA placement in a single root graph is
accepted. CPU incident values must be fixed INT64 or BOOL, within 40 bytes
each, with no implicit CPU inputs. The receipt reports their distinct static
payload size and whether it fits the predecessor's 43,819-byte envelope.
The graph-only result carries no scoring or product authority.

## Windows Large FP32 four-width profiling

Use the same launcher and arguments with a native image containing the
Large profiling selector. Add the exact completed Large graph result and
the two independent fixtures to select scoring.

```text
  --graph LARGE_NATIVE_GRAPH_RESULT
  --retained LARGE_REFERENCE_FIXTURE_17_128_8192
  --supplemental LARGE_REFERENCE_FIXTURE_32_1024
```

One owned session checks the final graph and CPU tensor bounds before scoring
32, 128, 1,024 and 8,192 in that order. It retains the native result, ten
operation journal records, one ORT trace and the owned execution receipt.
The original whole-run deadline is 120 seconds. The Windows process and Job
caps are 48 GiB each. The 44 GiB device plan includes a 40 GiB arena and 4 GiB
headroom. The warm admission increment is 8 GiB. The trace reports per-run
CPU tensor sizes and placement. Endpoint latency, allocation peaks and
cold/warm qualification have separate evidence requirements.

For corrected batch-view scoring, combine the candidate metadata arguments
with the scoring arguments and pass the candidate's exact completed native
graph result as `--graph`. Use an image containing
`cuda_large_fp32_batch_view_bounded_hybrid_profile` and include the reviewed
reference lineage.

```text
  --reference-equivalence LARGE_BATCH_VIEW_REFERENCE_LINEAGE
  --reference-equivalence-sha256 404a486500b1c3955cc5616554ccf2f7c55cd4855981d4477265688db41278d1
```

The native and owned scoring receipts identify the reference basis as
`equivalent_predecessor`. The independent fixtures retain their original
graph identity. Each candidate forward must produce a complete trace of
3,643 nodes, including 1,663 CPU nodes, under the same limits and score
thresholds.

## Windows Large FP32 unprofiled Resource20

The same launcher measures one cold load, one warmup and twenty warm
T8192/N8 forwards on the corrected batch-view graph. Supply the separate
passing four-width receipt and exact journal watchdog pin as described in
[Resource20 execution](COMPACT-RESOURCE20.md#corrected-large-resource20).
The parent retains the owned Job, per-operation journal and sampled adapter
memory alongside the native resource result.

### Windows WDDM capacity measurements

Run `python -I -B -S scripts/cuda-component-execution/wddm_capacity_snapshot.py`
from the verified Windows shell to read the A6000 UUID, driver mode, DXGI
adapter LUID and Windows dedicated/shared counters. An optional `--pid`
requires an existing GPU process instance on that adapter. The remote
session may enumerate another DXGI description for the same GPU. Exactly
one matching physical adapter must have a WDDM counter.

The owned launcher's pre-resume hook can take a sample with
`ProcessSampler(info, adapter_luid)` from `wddm_process_samples.py` as a
context manager. The post-resume hook calls
`observe_process(api, info, deadline, adapter_luid, baseline)` on the
original child handle until exit. This records adapter and process
dedicated/shared/total committed counters, their provider timestamps,
process private commit and working set, and the monotonic sampling window.
Missing process counters before GPU use or after release remain explicit.
Metering is sampled and does not measure an ORT allocator high-water mark.
Retain a separate post-release adapter snapshot under the original owner.
The live CPU child control runs with
`python -B -S scripts/cuda-component-execution/test_wddm_process_samples.py`.
