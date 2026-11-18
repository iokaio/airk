# Fresh Windows FP32 Compact components

The existing [Compact17 owner](CONTRACT.md) also supplies the guarded Windows
Job, process tree settlement, output checks and measured receipt to this
launcher. The new entry authenticates a private copy of that owner and binds
the row-specific result and operation journal validators. Each run selects one
case from the exact original three-row Compact fixture and reopens the sealed
independent reference producer. The hashes in the launcher identify the
original fixture and reference producer as well as the selected input.

| Tokens | Exact fixture row | Journal |
| --- | --- | --- |
| 128 | `all-live-1` | Load0, Numeric2 |
| 8192 | `native-all-live-8192` | Load0, Numeric3 |

Both cases require eight candidates, eight finite scores and two complete
independent reference arrays. All tokens are live. Each comparison uses the
original maximum raw error of .001, maximum probability mass error of .05 and
exact argmax. The owner validates the returned observations and independently
replays the comparisons against both pinned arrays. It retains the native
result and ordered start/return journal unchanged. A component pass requires
complete CUDA-only final placement and clean native settlement.

Prepare after a successful [fresh graph diagnosis](FRESH-GRAPH.md) with the
same preserved image, current request, fixture, model and descriptor. Provide
the exact SHA of the graph execution receipt. The adapter authenticates that
receipt, its exact fresh graph plan and its native result, then seals the
consumed graph sources and output pins into the component plan. The diagnostic
may record CPU placement. The separate component still requires all-CUDA
placement. Both stages are evidence for their own measured scopes.

Run from the verified human Windows shell. Every path here is absolute.

```text
python -B -S scripts/cuda-component-execution/fresh_context.py --prepare
  --tokens 128 --approved-script-sha256 SHA
  --metadata-receipt PATH --metadata-receipt-sha256 SHA
  --image-packet PATH --image-packet-sha256 SHA
  --graph-receipt PATH --graph-receipt-sha256 SHA
  --powershell ABSOLUTE_PATH --output-root FRESH_PRIVATE_CHILD --plan FRESH_PLAN

python -B -S scripts/cuda-component-execution/fresh_context.py --execute-reviewed-component
  --tokens 128 --approved-script-sha256 SHA --plan REVIEWED_PLAN --plan-sha256 SHA
```

Review the complete plan and source before execution. Use 8192 in both commands
for a separate T8192 plan and fresh result root. The owned child keeps the
original 120-second execution window, 16 GiB process, 24 GiB Job and 24 GiB
device planning limits, the original cold/warm/headroom requirements and
five-second cleanup bound. The adapter clips the existing runner's cutoff to
its outer original window so source bootstrap cannot add execution time.
A successful run retains the existing component receipt, all native output
proofs and exact input seals in the fresh output root. Product admission, model
qualification and release qualification have separate gates.

Run the focused inert controls with
`python -B -S scripts/tests/test_cuda_fresh_context.py`. The tests use literal
records for fixture selection, native score and journal refusals, and graph
handoff. A separate control authenticates and checks the actual frozen runner's
private binding.
