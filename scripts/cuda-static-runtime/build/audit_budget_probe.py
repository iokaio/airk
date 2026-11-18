# SPDX-License-Identifier: Apache-2.0
"""Measure Windows guard timing on read-only CUDA output samples."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import types
import subprocess
import time

GUARD_COMMAND = r"""
$ErrorActionPreference = 'Stop'
$v = [Console]::In.ReadToEnd() | ConvertFrom-Json
if ((Get-FileHash -LiteralPath $v.guard).Hash.ToLowerInvariant() -cne $v.guard_sha) {
    throw 'Pinned Windows guard changed'
}
. $v.guard
if ($v.budget -eq 16) {
    $old = '-or $BudgetSeconds -gt 10)'
    $new = '-or $BudgetSeconds -gt 16)'
    $definition = (Get-Command Assert-AirkOutputBatch -CommandType Function).Definition
    if ([regex]::Matches($definition, [regex]::Escape($old)).Count -ne 1) {
        throw 'Pinned batch watchdog admission expression differs'
    }
    # Replace only the numeric budget admission in this experiment's process.
    Set-Item Function:Assert-AirkOutputBatch ([scriptblock]::Create($definition.Replace($old, $new)))
}
Assert-AirkOutputBatch -Entries @($v.entries) -ExpectedSid $v.sid -BudgetSeconds $v.budget |
    ConvertTo-Json -Depth 5 -Compress
"""


def check_file(pin):
    path = Path(pin["path"])
    if not path.is_absolute() or not path.is_file() or path.stat().st_size != pin["bytes"]:
        raise ValueError("reviewed audit probe input differs")
    if hashlib.sha256(path.read_bytes()).hexdigest() != pin["sha256"]:
        raise ValueError("reviewed audit probe input hash differs")
    return path


def measure(powershell, guard, guard_sha, sid, entries, seconds):
    request = {
        "guard": str(guard), "guard_sha": guard_sha, "sid": sid, "budget": seconds, "entries": entries,
    }
    started = time.monotonic()
    try:
        observed = subprocess.run(
            [str(powershell), "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", GUARD_COMMAND],
            input=json.dumps(request), text=True, capture_output=True, timeout=seconds, check=True,
        )
    except subprocess.TimeoutExpired:
        return {"watchdog_seconds": seconds, "elapsed_seconds": time.monotonic() - started, "status": "TIMEOUT"}
    outcome = json.loads(observed.stdout)
    rows = outcome["results"]
    if outcome["schema"] != "airk.human-output-audit/1" or len(rows) != len(entries):
        raise ValueError("audit probe response differs")
    status = "PASS" if outcome["expired"] is False and all(
        row == {"index": index, "passed": True, "error": None} for index, row in enumerate(rows)
    ) else "REFUSED"
    return {"watchdog_seconds": seconds, "elapsed_seconds": time.monotonic() - started,
            "guard_elapsed_seconds": outcome["elapsed_seconds"], "status": status}


def reviewed_module(path, sha, name):
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != sha:
        raise ValueError("reviewed scheduler source differs")
    owner = types.ModuleType(name)
    owner.__file__ = str(path)
    exec(compile(raw, str(path), "exec"), owner.__dict__)
    return owner


def scheduler_observations(recipe, args):
    """Compare scheduling while retaining the producer's current ownership checks."""
    if not args.compile_owner_sha256:
        raise ValueError("a reviewed compile owner SHA is required for scheduling")
    for groups, workers, batch in args.schedule or ():
        if not (1 <= groups <= 2 and 1 <= workers <= 8 and 1 <= batch <= 128):
            raise ValueError("invalid group, worker or batch size")
    plan = recipe
    if plan.get("human_sid", args.human_sid) != args.human_sid:
        raise ValueError("scheduler human identity differs")
    compiler = reviewed_module(Path(__file__).with_name("fresh_compile.py"),
                               args.compile_owner_sha256, "audit_compile")
    guard_pin = plan["owners"]["guards"]
    guard_path = check_file(guard_pin)
    guard = reviewed_module(guard_path, guard_pin["sha256"], "audit_guard")
    if guard.SID != args.human_sid:
        raise ValueError("scheduler guard identity differs")
    deadline = time.monotonic() + 240
    paths = sorted(compiler.tree_paths(Path(plan["fresh_output"]) / "b", deadline))
    count = 1536
    if len(paths) < count:
        raise ValueError("1536 generated paths required for scheduling comparison")
    sample = [paths[index * len(paths) // count] for index in range(count)]
    records = []
    for groups, workers, batch in args.schedule or ():
        audit = compiler.output_guard(guard, plan, batch_size=batch, workers=workers)
        started = time.monotonic()
        with ThreadPoolExecutor(max_workers=groups) as executor:
            futures = [executor.submit(audit, sample[offset::groups], True, deadline)
                       for offset in range(groups)]
            for check in futures:
                check.result()
        records.append({"groups": groups, "workers_per_group": workers, "batch_size": batch,
                        "elapsed_seconds": time.monotonic() - started, "status": "PASS"})
    if args.rolling_schedule:
        if not args.verify_owner_sha256:
            raise ValueError("a reviewed verifier owner SHA is required for rolling scheduling")
        verifier = reviewed_module(Path(__file__).with_name("fresh_verify.py"),
                                   args.verify_owner_sha256, "audit_verify")
        for workers, batch in args.rolling_schedule:
            started = time.monotonic()
            verifier.audit_compiler_outputs(compiler.output_guard, guard, plan, sample, True,
                                            deadline, batch_size=batch, workers=workers)
            records.append({"rolling_workers": workers, "batch_size": batch,
                            "elapsed_seconds": time.monotonic() - started, "status": "PASS"})
    return {
        "path_count": len(paths), "paths": count,
        "sample_sha256": hashlib.sha256(json.dumps([str(path) for path in sample]).encode()).hexdigest(),
        "qualification": False, "observations": records,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--recipe", required=True, type=Path)
    parser.add_argument("--recipe-sha256", required=True)
    parser.add_argument("--human-sid", required=True)
    parser.add_argument("--compile-owner-sha256")
    parser.add_argument("--verify-owner-sha256")
    parser.add_argument("--schedule", nargs=3, type=int, action="append",
                        metavar=("GROUPS", "WORKERS", "BATCH_PATHS"))
    parser.add_argument("--rolling-schedule", nargs=2, type=int, action="append",
                        metavar=("WORKERS", "BATCH_PATHS"))
    args = parser.parse_args()
    raw = args.recipe.read_bytes()
    if hashlib.sha256(raw).hexdigest() != args.recipe_sha256:
        raise ValueError("reviewed configure recipe hash differs")
    recipe = json.loads(raw)
    if recipe["schema"] != "airk.private.cuda-fresh-configure-recipe/1":
        raise ValueError("fresh CUDA configure recipe required")
    guard_pin = recipe["checked_guard"]
    guard = check_file(guard_pin)
    powershell = check_file(recipe["powershell"])
    if args.schedule or args.rolling_schedule:
        print(json.dumps(scheduler_observations(recipe, args)))
        return
    if args.compile_owner_sha256 or args.verify_owner_sha256:
        parser.error("scheduler source pins require an explicit scheduling mode")
    work = Path(recipe["fresh_output"])
    sample = sorted((work / "b").rglob("*"), key=lambda path: (len(str(path)), str(path)), reverse=True)[:128]
    if len(sample) != 128:
        raise ValueError("128 generated paths required for the comparison")
    entries = [{"path": str(path), "root": str(work.parent), "created": True} for path in sample]
    records = [measure(powershell, guard, guard_pin["sha256"], args.human_sid, entries, cap) for cap in (10, 16)]
    print(json.dumps({
        "paths": len(entries), "maximum_path_chars": max(len(row["path"]) for row in entries),
        "sample_sha256": hashlib.sha256(json.dumps(entries, sort_keys=True).encode()).hexdigest(),
        "qualification": False, "observations": records,
    }))
    raise SystemExit(0 if records[-1]["status"] == "PASS" else 1)


if __name__ == "__main__":
    main()
