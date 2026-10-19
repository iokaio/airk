# SPDX-License-Identifier: Apache-2.0
"""Measure local CLI reporting overhead on an original synthetic workspace.

Alternates enabled/disabled trials, then exercises simultaneous independent workers.
No private source or provider traffic is used. Includes process startup/serialization.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--trials", type=int, default=30)
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--require-complete", action="store_true", help="Fail if any submitted successful call is missing from the report")
    args = parser.parse_args()
    if not (2 <= args.trials <= 1000 and 1 <= args.workers <= 16):
        parser.error("trials must be 2..1000; workers must be 1..16")
    binary = args.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="airk-usage-benchmark-") as temporary:
        base = Path(temporary)
        root = base / "sources"
        root.mkdir()
        for i in range(16):
            (root / f"sample-{i}.txt").write_text((f"Original telemetry fixture {i}. Bounded evidence saves reading.\n" * 100), encoding="utf-8")
        command = [str(binary), "--root", str(root), "--data-dir", str(base / "state"), "--collection", "usage-benchmark"]

        def run(arguments, disabled=False):
            env = dict(os.environ)
            env.pop("AIRK_USAGE_MODE", None)
            if disabled:
                env["AIRK_USAGE_MODE"] = "disabled"
            start = time.perf_counter_ns()
            result = subprocess.run(command + arguments, env=env, capture_output=True, timeout=120)
            duration = (time.perf_counter_ns() - start) / 1_000_000
            if result.returncode:
                raise RuntimeError("synthetic helper invocation failed")
            return duration, json.loads(result.stdout)

        run(["index"], True)
        run(["search", "telemetry", "--mode", "lexical"], True)
        run(["search", "telemetry", "--mode", "lexical"])
        run(["usage", "clear"])
        durations = {"enabled": [], "disabled": []}
        for trial in range(args.trials):
            for mode in (["enabled", "disabled"] if trial % 2 == 0 else ["disabled", "enabled"]):
                elapsed, _ = run(["search", "telemetry", "--mode", "lexical"], mode == "disabled")
                durations[mode].append(elapsed)
        before = run(["usage", "report"])[1]
        with ThreadPoolExecutor(max_workers=args.workers) as pool:
            concurrent = list(pool.map(lambda _: run(["search", "telemetry", "--mode", "lexical"])[0], range(args.trials)))
        after = run(["usage", "report"])[1]

        def stats(values):
            return {"samples": len(values), "median_ms": statistics.median(values),
                    "minimum_ms": min(values), "maximum_ms": max(values)}

        recorded_before = before["report"]["operations"].get("search", {}).get("calls", 0)
        recorded_after = after["report"]["operations"].get("search", {}).get("calls", 0)
        result = {"schema": "airk-usage-overhead/1", "platform": platform.system(), "architecture": platform.machine(),
                  "binary_version": subprocess.check_output([str(binary), "--version"], text=True, timeout=30).strip(),
                  "trials_per_mode": args.trials, "workers": args.workers,
                  "enabled": stats(durations["enabled"]), "disabled": stats(durations["disabled"]),
                  "median_difference_ms": statistics.median(durations["enabled"]) - statistics.median(durations["disabled"]),
                  "concurrent": stats(concurrent), "serial_recorded": recorded_before,
                  "serial_unrecorded": args.trials - recorded_before,
                  "concurrent_recorded": recorded_after - recorded_before,
                  "concurrent_unrecorded": args.trials - (recorded_after - recorded_before),
                  "ledger_bytes": after["health"]["database_bytes"],
                  "notice": "Synthetic local process-per-call test including startup and serialization. Not persistent MCP latency, provider savings, physical disk I/O or cross-platform qualification. Unrecorded counts are independently known from this harness only."}
        args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(result, indent=2))
        if args.require_complete and (result["serial_unrecorded"] != 0 or result["concurrent_unrecorded"] != 0):
            raise SystemExit("Usage completeness regression: submitted successful calls differ from recorded calls")


if __name__ == "__main__":
    main()
