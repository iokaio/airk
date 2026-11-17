# SPDX-License-Identifier: Apache-2.0
"""Private CUDA Resource20 parent predicates; consumes native output, constructs no native request."""
GIB = 1 << 30
ORIGINAL_PAIRS = {
    "arbiter-focus-qwen3-reranker-06b": (4 * GIB, 8 * GIB),
    "arbiter-context-qwen3-17b": (16 * GIB, 32 * GIB),
    "arbiter-context-qwen3-4b": (40 * GIB, 48 * GIB),
}
def need(value, message):
    if not value:
        raise ValueError(message)

def model_pair(native_policy, model):
    need(native_policy.get("schema") == "airk.private.cuda-resource-benchmark-policy/1",
         "native benchmark policy sidecar required")
    rows = native_policy["model_limits"]
    expected = [{"model": name, "host_limit_bytes": host,
                 "process_containment_bytes": job, "job_limit_bytes": job}
                for name, (host, job) in ORIGINAL_PAIRS.items()]
    need(rows == expected and all(type(r[k]) is int for r in rows for k in
         ("host_limit_bytes", "process_containment_bytes", "job_limit_bytes")),
         "complete native model policy differs from independent original pairs")
    need(native_policy.get("whole_process_ms") == 3_600_000
         and native_policy.get("load_operations") == 1
         and native_policy.get("warmup_operations") == 1
         and native_policy.get("sample_operations") == 20
         and native_policy.get("gpu_process_peak_required_for_final_qualification") is True,
         "native resource denominator/whole cutoff differs")
    need(model in ORIGINAL_PAIRS, "model outside original resource roster")
    return ORIGINAL_PAIRS[model]

def documents(plan, wrapper, request, profile, expected, native_policy):
    host, job = model_pair(native_policy, profile["model"])
    need(wrapper.get("schema") == "airk.private.cuda-resource-benchmark-request/1"
         and type(wrapper["outer_deadline_ms"]) is int and wrapper["outer_deadline_ms"] == 3_600_000,
         "actual native wrapper/whole cutoff differs")
    need(all(type(wrapper[k]) is int and wrapper[k] > 0 for k in
         ("host_limit_bytes", "device_limit_bytes", "cold_bytes",
          "headroom_bytes", "job_limit_bytes")), "explicit native capacity bounds required")
    need(type(wrapper["warm_bytes"]) is int and wrapper["warm_bytes"] >= 0, "invalid native warm capacity")
    need((wrapper["host_limit_bytes"], wrapper["job_limit_bytes"]) == (host, job)
         and wrapper["cold_bytes"] + wrapper["headroom_bytes"] <= wrapper["device_limit_bytes"],
         "original model reservation/containment or device bound differs")
    need(profile["cpu_fit"] is None and profile.get("device_execution") is not None
         and profile["memory_reservation_bytes"] == host, "native GPU profile/host reservation differs")
    need(expected.get("schema")=="airk.private.cuda-resource-benchmark-expectations/1"
         and expected["native_input"]==wrapper["gpu_input"], "native expectations source binding differs")
    need(plan["model"] == profile["model"] == expected["model"]
         and plan["profile"] == profile["manifest"] == expected["profile"],
         "native model/profile identity differs")
    need(plan["cpu_fit"] is None and plan["reservation_bytes"] == host, "CPU fit cannot be transferred")
    need(plan["limits"] == {"whole_process_seconds": 3600, "host_committed_bytes": job,
         "log_bytes": 64 << 20, "operation_journal_bytes": 131072, "per_operation_ms": 120000},
         "original parent whole/log/operation/Job limits differ")
    need(request["numeric_fixture"] is None, "Resource20 must not relabel a numerical fixture")
    need(plan["policy"] == request["policy"] == expected["policy"] == native_policy["resource_policy"]
         and plan["policy_digest"] == request["policy_digest"] == expected["policy_digest"]
         and plan["policy"]["deadline_ms"] == 120000, "native policy/digest differs")
    need(plan["expected_resource_input"] == expected["expected_resource_input"]
         and plan["native_operation_roster"] == expected["native_operation_roster"]
         and plan["forward_roster"] == expected["forward_roster"] == native_policy["forward_roster"],
         "original native compiled input/operation/forward expectations differ")
    roster = plan["native_operation_roster"]
    need(len(roster) == 22 and [(r["kind"], r["ordinal"]) for r in roster]
         == [("load", 0), ("warmup", 0)] + [("sample", n) for n in range(1, 21)],
         "complete original 22 operation denominator required")
    need(plan["forward_roster"] == ["warmup-0"] + [f"sample-{n:02}" for n in range(1, 21)],
         "complete original 21 forward denominator required")
    return host, job

def result(body, plan, wrapper, request, profile, expected):
    need(body.get("schema") == "airk.private.cuda-native-resource/1"
         and body.get("complete") is True and body.get("latency_host_placement_passed") is True
         and body.get("final_gpu_qualification") is False and body.get("release_qualified") is False
         and body.get("numeric_status") == "unavailable_in_resource_only_run"
         and body.get("gpu_peak_status") == "unavailable_complete_process_observer"
         and body.get("measured_peak_device_bytes", "absent") is None,
         "distinct GPU latency/host/placement scope required; GPU peak is unavailable")
    need(body.get("model")==plan["model"] and body.get("cpu_fit", "absent") is None
         and body["device_execution"] == profile["device_execution"]
         and body["bounds"] == wrapper and body["native_input"] == wrapper["gpu_input"]
         and body["runtime_manifest"] == wrapper["runtime_manifest"]
         and body["request"] == plan["native_input"]
         and body["expectations"]==wrapper["expectations"]
         and body["native_operation_roster"]==expected["native_operation_roster"],
         "actual native wrapper/input/profile/runtime bindings differ")
    need(body.get("native_control_retained_for_unknown_cuda") is False
         and body.get("native_control_cleanup_observed") is True
         and body.get("native_control_cleanup_error", "absent") is None
         and body.get("native_host_charge_release_observed") is True
         and body.get("process_cuda_poisoned") is False
         and body.get("final_host_peak_error", "absent") is None,
         "actual native release/cleanup/known settlement missing")
    peak = body.get("peak_process_rss_bytes")
    need(type(peak) is int and 0 < peak <= plan["reservation_bytes"],
         "actual final persistent native RSS missing/excessive")
    need(body["native_host_reservation_bytes"] == plan["reservation_bytes"],
         "actual retained host reservation differs")
    actual_job = body["actual_job"]
    cap = plan["limits"]["host_committed_bytes"]
    need(actual_job["process_memory_limit_bytes"] == cap
         and actual_job["job_memory_limit_bytes"] == cap
         and type(actual_job["flags"]) is int and actual_job["flags"] & 0x2300 == 0x2300,
         "actual native original process/Job containment differs")
    series = body["resource"]
    need(series["complete"] is True and series["attempted_samples"] == 20
         and len(series["samples"]) == 20 and series["failure"] is None
         and series["reservation_bytes"] == plan["reservation_bytes"],
         "original full Resource20 result denominator incomplete")
    need(body["input"] == expected["expected_resource_input"] == plan["expected_resource_input"]
         and body["forward_roster"] == body["completed_forwards"] == plan["forward_roster"],
         "complete native input or successful synchronized forward roster differs")
