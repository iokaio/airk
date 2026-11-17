# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('builder',ROOT/'scripts/build-service-runtime.py'); b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)
paths=['crates/airk-cli/src/service_management.rs','crates/airk-cli/tests/service_runtime.rs']
receipt={'reviewer':'primary agent; explicit review after implementation','native':b.source_identity(),'files':{p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths},'decision':'Readiness fixture correction and behavior-preserving frozen-receive seam accepted for batched validation. Health intentionally bypasses metadata; same-instance SELECT-only Projects readiness observes required fixture precondition. RequestEpoch receives no new retry. Unexpected replies and phase retained; original Stop, owned exclusion and complete durable-state assertions preserved. Exact frozen branch counters, acknowledgment, timeout, disconnect and refusal unchanged. Cached failure emitter remains unidentified; deterministic test isolates actor ordering only.','independent_remaining_diff_review':'final-independent-review.md','t0_expected':13,'t0_scope':'8 service_management library, 3 service_runtime::checkpoint library, 2 integration metadata_readiness_ helpers; no actual service replay in T0','next_t2_expected':4,'stop':'Any failure or changed inputs; another service_runtime T2 failure requires two-strike user review before a third attempt.'}
(Path(__file__).parent/'replay-combined-review.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
