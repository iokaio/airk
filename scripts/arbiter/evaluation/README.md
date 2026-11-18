# Arbiter evaluation source

Maintained evaluation and independent reference recipes live here. Private inputs, models, execution plans and receipts remain under artifacts. Each component README and contract defines its supported entry points and required fresh source seals.

## Private Windows Compact FP32 Answer comparison

Run the paired fixture on an idle Windows host from the checked human
PowerShell session. Use the same reviewed service and CLI images, synthetic
corpus, Answer arguments, offline MiniLM files, model asset closure, and raw
all-unavailable calibration in both attempts. Keep each run in its own
fresh private artifacts directory. The only Compact profile difference is
the reviewed CPU or strict CUDA execution binding. Read the native fixture
[contract](../../../crates/airk-service/src/service_runtime/evaluation_fixture_tests/CONTRACT.md)
for the selector behavior.

Run:

    python -B -S scripts/arbiter/evaluation/run_compact_answer.py --plan <absolute-private-plan.json> --plan-sha256 <sha256> --output <fresh-artifacts-dir> --check

The plan schema is airk.private.paired-compact-answer/1. Supply backend (cpu
or cuda), its exact manifest, and absolute path/SHA-256 pins for service,
cli, answer, scopes, corpus, inputs, and minilm. The corpus pin describes
the files in its sibling sources directory. The minilm pin is model.json
and minilm_assets names the exact payload digests. The CUDA plan also
contains the reviewed graph receipt and SHA-256. Remove --check for a real
dispatch. Run one backend at a time. The Windows Job independently verifies
32 GiB process and aggregate limits. Confirm both fixtures close and the Jobs
settle, then compare request and corpus digests, search pipelines, and the
complete ordered lists of observed native input digests and actual token
counts. Read actual CLI round-trip time from answer_round_trip_ns and
guarded inference time from the work receipts. Keep the complete CLI
envelopes for interpretation. This comparison measures the all-unavailable
calibration path and retains the real Answer abstentions.
