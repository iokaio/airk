# SPDX-License-Identifier: Apache-2.0
"""Compare two Windows guard timeouts on the same read-only CUDA output sample."""

import argparse
import hashlib
import json
from pathlib import Path
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--recipe", required=True, type=Path)
    parser.add_argument("--recipe-sha256", required=True)
    parser.add_argument("--human-sid", required=True)
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
