# SPDX-License-Identifier: Apache-2.0
"""Compute (or recompute) a pass's canonical first-call digests without running sessions.

``python -I -B pass_digests.py --spec S --only focus-cpu`` replays the canonical
first call of each scenario for the named results against their handles and
stores successful digests in ``<R>/passes/<series>/digests.json`` (failures go
to ``digest-failures.jsonl``). Use it while a stage configuration is live,
before the stage changes: for example the Focus digests before Compact is
enabled, so the reuse rule can compare them. It prints, per scenario, the
digest of every result and whether results with the same tool set agree.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import run_pass  # noqa: E402


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--spec", type=Path, required=True)
    parser.add_argument("--r-root", type=Path, default=common.DEFAULT_R)
    parser.add_argument("--only", required=True, help="comma-separated result names (e.g. focus-cpu)")
    args = parser.parse_args(argv)
    flow = run_pass.Pass(common.read_json(args.spec), args.r_root)
    only = set(args.only.split(","))
    entries = [e for e in flow.spec["results"] if flow.result_name(e) in only]
    for entry in entries:
        flow.check_stages(entry)
    flow.guard("start", entries)
    try:
        for entry in entries:
            for scenario in flow.entry_scenarios(entry):
                value = flow.digest(scenario, entry)
                print(json.dumps({"key": flow.key(scenario, entry), "ok": value.get("ok"),
                                  "digest": (value.get("digest") or "")[:16], "elapsed_ms": value.get("elapsed_ms")}))
    finally:
        flow.guard("end", entries)
    for scenario in flow.scenarios:
        row = {}
        for entry in flow.spec["results"]:
            stored = flow.digests.get(flow.key(scenario, entry))
            row[flow.result_name(entry)] = (stored or {}).get("digest", "")[:16] or None
        print(json.dumps({"scenario": scenario["id"], "digests": row}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
