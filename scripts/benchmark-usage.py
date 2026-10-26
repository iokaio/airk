# SPDX-License-Identifier: Apache-2.0
"""Measure local CLI reporting overhead on an original synthetic workspace.

Alternates enabled/disabled trials, then exercises simultaneous independent workers.
No private source or provider traffic is used. Includes process startup/serialization.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
from contextlib import ExitStack, nullcontext
import hashlib
import importlib.util
import json
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time

spec = importlib.util.spec_from_file_location("airk_usage_fixture", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)


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
    with tempfile.TemporaryDirectory(prefix="airk-usage-benchmark-") as temporary, ExitStack() as stack:
        base = Path(temporary).resolve(strict=True)
        root = base / "sources"
        root.mkdir()
        for i in range(16):
            (root / f"sample-{i}.txt").write_text((f"Original telemetry fixture {i}. Bounded evidence saves reading.\n" * 100), encoding="utf-8")
        state = base / "state"
        state.mkdir()
        fixture = stack.enter_context(clients.Fixture(binary.parent, base))
        assert fixture.binary == binary
        fixture.environment.pop("AIRK_USAGE_MODE", None)
        fixture.register("usage-benchmark", root, state)
        fixture.configure("usage-benchmark", {"operation": "vectors", "enabled": False})
        fixture.configure("usage-benchmark", {"operation": "usage", "mode": "aggregates_only"})
        command = [str(binary), *fixture.arguments("usage-benchmark")]

        def run(arguments, writer_held=False):
            env = dict(fixture.environment)
            env.pop("AIRK_USAGE_MODE", None)
            # Exact accounting measures one submitted call; do not hide rejection retries.
            guard = fixture.hold_publication("usage-benchmark") if arguments[0] == "search" and not writer_held else nullcontext()
            with guard:
                start = time.perf_counter_ns()
                if arguments[0] == "search":
                    value = fixture.query("usage-benchmark", *arguments, retry=False, admission_only=True)
                else:
                    result = subprocess.run(command + arguments, env=env, capture_output=True, timeout=120)
                    value = json.loads(result.stdout) if result.stdout else {}
                    if result.returncode or value.get("isError"):
                        raise RuntimeError(f"Synthetic native client failed: exit {result.returncode}")
                duration = (time.perf_counter_ns() - start) / 1_000_000
            if "job" in value:
                fixture.wait(value)
            return duration, value

        fixture.job("usage-benchmark", "index")
        run(["usage", "configure", "--mode", "disabled"])
        run(["search", "telemetry", "--mode", "lexical"])
        run(["usage", "configure", "--mode", "aggregates_only"])
        run(["search", "telemetry", "--mode", "lexical"])
        run(["usage", "clear"])
        durations = {"enabled": [], "disabled": []}
        for trial in range(args.trials):
            for mode in (["enabled", "disabled"] if trial % 2 == 0 else ["disabled", "enabled"]):
                run(["usage", "configure", "--mode", "disabled" if mode == "disabled" else "aggregates_only"])
                elapsed, _ = run(["search", "telemetry", "--mode", "lexical"])
                durations[mode].append(elapsed)
        before = run(["usage", "report"])[1]
        run(["usage", "configure", "--mode", "aggregates_only"])
        with fixture.hold_publication("usage-benchmark"), ThreadPoolExecutor(max_workers=args.workers) as pool:
            concurrent = list(pool.map(lambda _: run(["search", "telemetry", "--mode", "lexical"], writer_held=True)[0], range(args.trials)))
        after = run(["usage", "report"])[1]

        def stats(values):
            return {"samples": len(values), "median_ms": statistics.median(values),
                    "minimum_ms": min(values), "maximum_ms": max(values)}

        recorded_before = before["report"]["operations"].get("search", {}).get("calls", 0)
        recorded_after = after["report"]["operations"].get("search", {}).get("calls", 0)
        result = {"schema": "airk-usage-overhead/1", "platform": platform.system(), "architecture": platform.machine(),
                  "runtime_manifest_sha256": hashlib.sha256((fixture.runtime / "airk-install.json").read_bytes()).hexdigest(),
                  "binary_version": subprocess.check_output([str(binary), "--version"], text=True, timeout=30).strip(),
                  "trials_per_mode": args.trials, "workers": args.workers,
                  "enabled": stats(durations["enabled"]), "disabled": stats(durations["disabled"]),
                  "median_difference_ms": statistics.median(durations["enabled"]) - statistics.median(durations["disabled"]),
                  "concurrent": stats(concurrent), "serial_recorded": recorded_before,
                  "serial_unrecorded": args.trials - recorded_before,
                  "concurrent_recorded": recorded_after - recorded_before,
                  "concurrent_unrecorded": args.trials - (recorded_after - recorded_before),
                  "ledger_bytes": after["health"]["database_bytes"],
                  "notice": "Synthetic local process-per-call test including startup and serialization. Each measured query is submitted once with concurrent publication excluded by the fixture writer lock; lock acquisition is outside the measured interval, while bounded explicitly unsubmitted admission waits are included. Not persistent MCP latency, provider savings, physical disk I/O or cross-platform qualification. Unrecorded counts are independently known from this harness only."}
        args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(result, indent=2))
        if args.require_complete and (result["serial_unrecorded"] != 0 or result["concurrent_unrecorded"] != 0):
            raise SystemExit("Usage completeness regression: submitted successful calls differ from recorded calls")


if __name__ == "__main__":
    main()
