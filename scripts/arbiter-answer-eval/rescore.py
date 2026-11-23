# SPDX-License-Identifier: Apache-2.0
"""Re-score a graded pass under another coverage rule without touching its original results.

For every session of the pass index, coverage is recomputed under ``--rule``
and written beside the original as ``coverage-rule<N>.json``. The pass is then
graded again from those files and the existing judge verdicts. The output is
``results-rule<N>.csv`` and ``summary-rule<N>.json`` in the pass folder.
``coverage.json``, ``results.csv`` and ``summary.json`` are left as they were.

``python -I -B rescore.py --pass-dir <R>/passes/pilot --rule 2``
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
import coverage  # noqa: E402
import grade  # noqa: E402


def rescore(pass_dir: Path, rule: int, scenarios: dict[str, dict]) -> dict:
    index = common.read_json(pass_dir / "pass-index.json")
    name = f"coverage-rule{rule}.json"
    done = set()
    for row in index["results"]:
        directory = Path(row["session_dir"])
        if str(directory) in done or not (directory / "session.json").is_file():
            continue
        coverage.for_session(scenarios[row["scenario_id"]], directory, rule=rule, name=name)
        done.add(str(directory))
    rescored = json.loads(json.dumps(index))
    for row in rescored["results"]:
        row["coverage"] = str(Path(row["session_dir"]) / name)
    adj_path = pass_dir / "adjudications.json"
    rows = grade.build_rows(rescored, common.read_json(adj_path) if adj_path.is_file() else {})
    common.write_text(pass_dir / f"results-rule{rule}.csv", grade.csv_text(rows))
    summary = grade.summarize(rows, index["series"])
    summary["coverage_rule"] = rule
    common.write_json(pass_dir / f"summary-rule{rule}.json", summary)
    return {"rows": rows, "summary": summary}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--pass-dir", type=Path, required=True)
    parser.add_argument("--rule", type=int, choices=[1, 2, 3], required=True)
    parser.add_argument("--scenarios-dir", type=Path, default=common.DEFAULT_R / "scenarios")
    args = parser.parse_args(argv)
    scenarios = {s["id"]: s for s in map(common.load_scenario, common.scenario_files(args.scenarios_dir))}
    result = rescore(args.pass_dir, args.rule, scenarios)
    for row in result["rows"]:
        print(json.dumps({k: row.get(k) for k in ("repo", "slot", "configuration", "device", "adjudicated_grade",
                                                   "correct_finish", "turns_to_finish", "first_call_yield",
                                                   "essential_covered", "essential_total")}))
    print(json.dumps({"headroom": result["summary"]["headroom"]["count"]}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
