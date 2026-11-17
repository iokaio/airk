# Final-plan section 7 maintained source mapping

These links preserve the inspected historical anchors; they are not substitute ready-to-run commands. No active driver was switched and no seal or numerical rule was relaxed by source rescue. Rust calibration/final-distribution selectors remain native-owned and must be inspected in current source before execution.

| Section 7 role | Original suffix under selected-native-goal-driver-r1 | Maintained source (original body recoverable) |
| --- | --- | --- |
| CAL packets | `chunked-cal-r1/collector/prepare_packets.py` | [prepare_packets--e06392f45e04.py](selected_native_goal/prepare_packets--e06392f45e04.py) |
| CAL owned controller | `chunked-cal-r1/collector/run_controller.py` | [run_controller--e2ea7b1af45e.py](selected_native_goal/run_controller--e2ea7b1af45e.py) |
| CAL capture child | `chunked-cal-r1/collector/capture_chunk.py` | [capture_chunk--33787f2facfa.py](selected_native_goal/capture_chunk--33787f2facfa.py) |
| CAL aggregation | `chunked-cal-r1/proof/aggregate_chunks.py` | [aggregate_chunks--2253e47d9143.py](selected_native_goal/aggregate_chunks--2253e47d9143.py) |
| Certificate preparation | `cal-completion-owned-r1/prepare_phase.py` | [prepare_phase--268797ca6c5c.py](selected_native_goal/prepare_phase--268797ca6c5c.py) |
| Certificate owned execution | `cal-completion-owned-r1/run_phase_reviewed.py` | [run_phase_reviewed--c51b6bdcebd9.py](selected_native_goal/run_phase_reviewed--c51b6bdcebd9.py) |
| Certificate producer | `source/prepare_selected_cal_reference_certificates.py` | [prepare_selected_cal_reference_certificates--463c3a11d2f8.py](selected_native_goal/prepare_selected_cal_reference_certificates--463c3a11d2f8.py) |
| Raw Context preparation | `native-cal-owned-r1/prepare_raw_packet.py` | [prepare_raw_packet--7eac62ed1ecc.py](selected_native_goal/prepare_raw_packet--7eac62ed1ecc.py) |
| Raw Context owned execution | `native-cal-owned-r1/run_raw_reviewed.py` | [run_raw_reviewed--ba3309192ec5.py](selected_native_goal/run_raw_reviewed--ba3309192ec5.py) |
| Selected A6 confirmation | `source/run_goal_condition.py` | [run_goal_condition--92b117e9f99b.py](selected_native_goal/run_goal_condition--92b117e9f99b.py) |

Composed anchor: `artifacts/advanced-split/actual-composed-service-preparation-r1/run_composed.py` maps with a reversible SPDX-prefix-only change to [composed_service_r1/run_composed.py](composed_service_r1/run_composed.py). Its supporting source roles and reused OS observer are documented [beside it](composed_service_r1/README.md).

## Concrete next operational owners

After the source-preservation commit, implement reviewed operational adapters under `scripts/arbiter/evaluation/selected_goal/` and `scripts/arbiter/evaluation/composed_service/`. These are proposed paths, not files supplied by this rescue.

1. Selected CAL: make `selected_goal/prepare_packets.py`, `run_controller.py` and `aggregate_chunks.py` resolve a single maintained sibling closure, explicit private inputs/output roots and the current owned-process boundary. Preserve thirteen-phase/census associations and original closure/failure rules. Keep `capture_chunk.py` an owned child; never let it become a standalone bypass.
2. Selected certificates/raw Context: adapt `prepare_phase.py`, `run_phase_reviewed.py`, `prepare_raw_packet.py` and `run_raw_reviewed.py` with the exact current native selectors, whole-census coverage, immutable capture/corpus/rubric/source/task-policy inputs and separately reviewed current model profile. Preserve independent reference vs native observation separation.
3. Selected A6: adapt `selected_goal/run_goal_condition.py` from the selected **confirmation** anchor above, including its full helper closure. The existing `tools/retrieval-train/run_goal_condition.py` development/final interface is not interchangeable. Required sibling names include `aggregate_native_retrieval`, `audit_sources`, `eval_agent_smoke`, `goal_sources`, `goal_trial`, `native_goal_client`, `qwen_export`, plus the source roster explicitly sealed by the selected driver. Resolve each through exact source-map/committed owners, not a basename search that could mix experiment versions.
4. Composed: adapt `composed_service/run_composed.py`, `session_controller.cjs`, `evidence_checks.py` and `assemble_observations.py`, importing the single maintained OS observer owner. Bind reviewed Rust-emitted plan/packet, actual SDK modules and native images. Reconcile historical 30-second request/event caps with the approved current 120-second native/client contract before execution; do not simply widen a Python constant. Retain actual session shutdown and observation-only/nonqualification semantics.

Current-policy O helpers are separately preserved under [composed_orchestration](composed_orchestration/README.md). `goal_trial` and `native_goal_client` have distinct historical versions; the source map records the exact current-orchestration hashes and original paths. New adapters must intentionally select those semantics where current policy is required and independently review the resulting combined closure. Archival identity alone proves neither current API compatibility nor private-input availability. No model, native build or operational rerun was performed for this mapping.
