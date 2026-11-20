# SPDX-License-Identifier: Apache-2.0
"""Independent validation of Compact unprofiled development timing populations."""
import math
import struct
import time
import uuid


def require(condition, message):
    if not condition:
        raise ValueError(message)


def schedule(mode):
    if mode == "qkv-fit-probe":
        return [(8192, 0, "sample")]
    if mode == "qkv-resource20":
        return [(8192, 0, "warmup")] + [(8192, 0, "sample")] * 20
    require(mode == "qkv-mixed", "unknown Compact timing population")
    return [(width, cycle, "sample") for cycle in (1, 2, 3) for width in (32, 128, 1024, 8192, 32)]


def timing_rows(mode, rows, sealed):
    fixed = schedule(mode)
    require(len(rows) == len(fixed), "timing population length differs")
    elapsed = []
    for row, (width, cycle, kind) in zip(rows, fixed, strict=True):
        source = sealed[width]
        require((row["tokens"], row["cycle"], row["sample_kind"]) == (width, cycle, kind)
                and row["input_digest"] == source["input_digest"]
                and struct.pack("<8f", *row["raw_scores"]) == struct.pack("<8f", *source["raw_scores"])
                and row["forward_synchronized"] is row["within_deadline"] is True
                and type(row["native_call_elapsed_us"]) is type(row["elapsed_us"]) is int
                and 0 < row["native_call_elapsed_us"] <= row["elapsed_us"] < 120_000_000,
                "timing width, ordinal, score, deadline or synchronization differs")
        if kind == "sample":
            elapsed.append(row["elapsed_us"])
    if mode == "qkv-fit-probe":
        return {"first_calls": 1, "first_call_us": elapsed[0]}
    ordered = sorted(elapsed)
    return {"warm_samples": len(ordered), "nearest_rank_p50_us": ordered[math.ceil(len(ordered)*.5)-1],
            "nearest_rank_p95_us": ordered[math.ceil(len(ordered)*.95)-1], "maximum_us": ordered[-1]}


def same_adapter(before, after, native):
    require(before["dxgi"]["luid"] == after["dxgi"]["luid"]
            and before["nvidia_smi_mib"]["uuid"] == after["nvidia_smi_mib"]["uuid"]
            and uuid.UUID(before["nvidia_smi_mib"]["uuid"].removeprefix("GPU-")).bytes
            == bytes(native["adapter_before"]["uuid"]), "WDDM adapter identity differs")


def memory_coverage(mode, series, events, pid, luid):
    require(time.get_clock_info("monotonic").implementation == "QueryPerformanceCounter()"
            and all(e["clock"] == "windows_performance_counter" for e in events),
            "memory and operation observations need the same Windows counter epoch")
    require(series["pid"] == pid and series["adapter_luid"] == luid and series["before_resume"]
            and series["observed_exit_monotonic_ns"] >= events[-1]["observed_monotonic_ns"],
            "WDDM samples disagree with native PID, adapter or completion")
    samples = series["samples"]
    cycles = {}
    for i, (_, cycle, _) in enumerate(schedule(mode), 1):
        start, stop = events[2*i]["observed_monotonic_ns"], events[2*i+1]["observed_monotonic_ns"]
        first, _ = cycles.get(cycle, (start, stop))
        cycles[cycle] = (first, stop)
    statistics = []
    for cycle, (start, stop) in cycles.items():
        observed = [s for s in samples if start <= s["start_monotonic_ns"] <= s["end_monotonic_ns"] <= stop
                    and all(type(s["process"].get(field)) is int for field in ("Dedicated Usage", "Shared Usage"))]
        require(observed, "no resident WDDM process sample within measured population")
        statistics.append({"cycle": cycle, "sample_count": len(observed), "start_qpc_ns": start, "end_qpc_ns": stop,
                           "observed_process_dedicated_max_bytes": max(s["process"]["Dedicated Usage"] for s in observed),
                           "observed_process_shared_max_bytes": max(s["process"]["Shared Usage"] for s in observed),
                           "observed_working_set_max_bytes": max(s["working_set_bytes"] for s in observed),
                           "observed_private_commit_max_bytes": max(s["private_commit_bytes"] for s in observed)})
    return statistics
