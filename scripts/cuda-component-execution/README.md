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
