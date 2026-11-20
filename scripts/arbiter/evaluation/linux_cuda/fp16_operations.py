# SPDX-License-Identifier: Apache-2.0
"""Reconcile Linux CLOCK_MONOTONIC operation receipts with the measured native PID."""
import time


def verify(mode, value, events, samples):
    if "CLOCK_MONOTONIC" not in time.get_clock_info("monotonic").implementation:
        raise ValueError("Python monitor and Rust journal require Linux CLOCK_MONOTONIC")
    rows = value["rows"]
    expected = {"graph": [], "t32": [32], "wider": [128, 1024, 8192],
                "fixed": [8192] * 21, "mixed": [32, 128, 1024, 8192, 32] * 3}[mode]
    if [row["tokens"] for row in rows] != expected or len(events) != 2 * (1 + len(rows)):
        raise ValueError("width or operation population differs")
    identity = {"input_sha256": value["request"]["sha256"], "profile": value["profile"],
                "process_id": value["pid"], "native_binary_sha256": value["native_binary_sha256"]}
    expiry = value["original_expires_monotonic_ns"]
    previous = 0
    cycles = {}
    for i in range(1 + len(rows)):
        row = rows[i-1] if i else None
        name, ordinal = ("load", 0) if not i else ("numeric", i)
        if i and mode == "fixed":
            name, ordinal = ("warmup" if i == 1 else "sample"), i-1
        if i and mode == "mixed":
            name = "sample"
        start, stop = events[2*i:2*i+2]
        for seq, event, phase in ((2*i+1, start, "started"), (2*i+2, stop, "returned")):
            if (event["schema"] != "airk.arbiter.native-operation/1" or event["sequence"] != seq
                    or event["kind"] != name or event["ordinal"] != ordinal or event["identity"] != identity
                    or event["phase"] != phase or event["clock"] != "unix_monotonic"
                    or event["operation_input_digest"] != (row["input_digest"] if row else None)
                    or event["expires_monotonic_ns"] != expiry
                    or not previous <= event["observed_monotonic_ns"] < expiry):
                raise ValueError("Linux native journal identity, digest, ordering or deadline differs")
            previous = event["observed_monotonic_ns"]
        call = row["native_call_elapsed_us"] if row else value["load_native_call_us"]
        if (start["started_monotonic_ns"] != start["observed_monotonic_ns"]
                or stop["started_monotonic_ns"] != start["started_monotonic_ns"]
                or start["native_success"] is not None or start["native_call_elapsed_us"] is not None
                or stop["native_success"] is not True or stop["native_call_elapsed_us"] != call
                or not 0 <= call < 120_000_000
                or call * 1000 > stop["observed_monotonic_ns"] - start["observed_monotonic_ns"]):
            raise ValueError("operation start, completed native return or elapsed time differs")
        if i and mode in ("fixed", "mixed"):
            cycle = row["cycle"]
            begin, _ = cycles.get(cycle, (start["observed_monotonic_ns"], 0))
            cycles[cycle] = (begin, stop["observed_monotonic_ns"])
    coverage = []
    for cycle, (begin, end) in cycles.items():
        subset = [s for s in samples if begin <= s["sample_started_ns"] <= s["monotonic_ns"] <= end]
        if not subset:
            raise ValueError("no independent kernel/device sample within timing cycle")
        coverage.append({"cycle": cycle, "sample_count": len(subset),
                         "peak_observed_rss_bytes": max(s["host"]["VmRSS"] for s in subset),
                         "peak_observed_adapter_used_bytes": max(s["adapter"]["used"] for s in subset)})
    return coverage
