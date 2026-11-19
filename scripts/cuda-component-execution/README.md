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
