# SPDX-License-Identifier: Apache-2.0
"""Compare two graded passes per configuration: correct finishes, essential coverage, first-call yield, grades.

Read each pass's sample-one grading receipts. A historical CSV without
grading receipts is accepted only when every scenario/configuration key is
unique. For configurations present in both passes, compare shared scenarios:

* correct finishes and the turns saved on rows both finish;
* essential notes covered over the session;
* the mean first-call yield;
* the mean adjudicated grade;
* the mean tool time.

Writes ``--out`` and prints a table. Analysis only.

``python -I -B compare_passes.py --a <R>/passes/baseline --b <R>/passes/cs1-airk --out <file>``
"""

from __future__ import annotations

import argparse
import csv
import json
import sys
from pathlib import Path
from statistics import mean

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import grade  # noqa: E402


def rows(pass_dir: Path) -> dict[tuple, dict]:
    index_path = pass_dir / "pass-index.json"
    if index_path.is_file():
        index = common.read_json(index_path)
        adjudications = pass_dir / "adjudications.json"
        overrides = common.read_json(adjudications) if adjudications.is_file() else {}
        selected = [row for row in grade.build_rows(index, overrides) if row["sample"] == 1]
        expected = sum(entry.get("sample", 1) == 1 for entry in index["results"])
        common.require(len(selected) == expected,
                       f"{pass_dir}: missing sample-one grading receipts; restore the pass-index inputs")
    else:
        with (pass_dir / "results.csv").open(encoding="utf-8", newline="") as stream:
            selected = [dict(row, correct_finish=row["correct_finish"] == "True")
                        for row in csv.DictReader(stream)]
    out = {}
    for row in selected:
        name = row["configuration"] + ("" if row["device"] in ("", "none") else f"-{row['device']}")
        key = (name, f"{row['repo']}-{row['slot']}")
        common.require(key not in out,
                       f"{pass_dir}: duplicate comparison row for {key[1]}/{key[0]}; "
                       "supply the original pass-index.json and grading receipts to identify sample one")
        out[key] = row
    return out


def num(value, default=0.0):
    try:
        return float(value)
    except (TypeError, ValueError):
        return default


def summarize(a: dict, b: dict) -> dict:
    names = sorted({k[0] for k in a} & {k[0] for k in b})
    table = {}
    for name in names:
        shared = sorted(s for (n, s) in a if n == name and (n, s) in b)
        ra, rb = [a[(name, s)] for s in shared], [b[(name, s)] for s in shared]
        both = [s for s in shared if a[(name, s)]["correct_finish"] is True
                and b[(name, s)]["correct_finish"] is True]
        table[name] = {
            "rows": len(shared),
            "finishes_a": sum(r["correct_finish"] is True for r in ra),
            "finishes_b": sum(r["correct_finish"] is True for r in rb),
            "turns_saved_on_shared_finishes": sum(int(a[(name, s)]["turns_to_finish"]) - int(b[(name, s)]["turns_to_finish"])
                                                 for s in both),
            "essential_a": sum(int(num(r["essential_covered"])) for r in ra),
            "essential_b": sum(int(num(r["essential_covered"])) for r in rb),
            "essential_total": sum(int(num(r["essential_total"])) for r in ra),
            "first_call_yield_a": round(mean(num(r["first_call_yield"]) for r in ra), 4) if ra else None,
            "first_call_yield_b": round(mean(num(r["first_call_yield"]) for r in rb), 4) if rb else None,
            "mean_grade_a": round(mean(num(r["adjudicated_grade"]) for r in ra), 3) if ra else None,
            "mean_grade_b": round(mean(num(r["adjudicated_grade"]) for r in rb), 3) if rb else None,
            "tool_ms_mean_a": round(mean(num(r["elapsed_ms"]) for r in ra)) if ra else None,
            "tool_ms_mean_b": round(mean(num(r["elapsed_ms"]) for r in rb)) if rb else None,
            "reused_b": sum(1 for r in rb if r["reused_from"]),
        }
    return table


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--a", type=Path, required=True)
    parser.add_argument("--b", type=Path, required=True)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args(argv)
    table = summarize(rows(args.a), rows(args.b))
    if args.out:
        common.write_json(args.out, {"a": str(args.a), "b": str(args.b), "generated": common.now_iso(), "table": table})
    cols = ["rows", "finishes_a", "finishes_b", "essential_a", "essential_b", "essential_total", "first_call_yield_a",
            "first_call_yield_b", "mean_grade_a", "mean_grade_b", "tool_ms_mean_a", "tool_ms_mean_b", "reused_b"]
    print("| configuration | " + " | ".join(cols) + " |")
    print("|" + "---|" * (len(cols) + 1))
    for name, row in table.items():
        print(f"| {name} | " + " | ".join(str(row[c]) for c in cols) + " |")
    return 0


if __name__ == "__main__":
    sys.exit(main())
