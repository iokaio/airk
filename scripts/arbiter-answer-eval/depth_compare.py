# SPDX-License-Identifier: Apache-2.0
"""Compare two depth measurements (depth.py outputs) scenario by scenario.

For each scenario the script compares:

* the essential notes in the pool under rule 2, under rule 1 and on the
  same path;
* the deepest rule-2 rank and the hits returned;
* the search latency.

It also gives each pair's **evidence identity**: the ordered evidence digest
of both replies (``evidence.evidence_digest``), and whether every per-note
rank is equal. A control run must be identical to its reference
(``--expect-identical``). The script exits 1 when it is not, so the caller can
stop and report.

``python -I -B depth_compare.py --a <ref>/depth.json --b <new>/depth.json --label-a baseline --label-b search_v2 [--expect-identical] [--out cmp.json]``
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from statistics import mean, median

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import evidence  # noqa: E402


def counts(row: dict) -> dict:
    notes = row.get("notes") or []
    return {"rule2": sum(n["rule2"] is not None for n in notes), "rule1": sum(n["rule1"] is not None for n in notes),
            "same_path": sum(n["same_path"] is not None for n in notes), "essential": len(notes),
            "deepest": row.get("deepest_rank"), "hits": row.get("hits_returned"), "ms": row.get("elapsed_ms"),
            "ok": row.get("ok")}


def digest(row: dict) -> str | None:
    path = row.get("result_path")
    if not path or not Path(path).is_file():
        return None
    body = common.raw_body(Path(path).read_bytes())
    return evidence.evidence_digest(body) if body is not None else None


def compare(a: dict, b: dict) -> dict:
    rows_a = {r["scenario_id"]: r for r in a["rows"]}
    rows_b = {r["scenario_id"]: r for r in b["rows"]}
    table = []
    for sid in sorted(set(rows_a) | set(rows_b)):
        ra, rb = rows_a.get(sid), rows_b.get(sid)
        ranks_equal = bool(ra and rb) and [(n["id"], n["rule2"], n["rule1"], n["same_path"]) for n in ra["notes"]] == \
            [(n["id"], n["rule2"], n["rule1"], n["same_path"]) for n in rb["notes"]]
        da, db = (digest(ra) if ra else None), (digest(rb) if rb else None)
        table.append({"scenario": sid, "a": counts(ra) if ra else None, "b": counts(rb) if rb else None,
                      "ranks_equal": ranks_equal, "evidence_equal": da is not None and da == db})
    total = lambda side, key: sum((r[side] or {}).get(key) or 0 for r in table)  # noqa: E731
    ms = lambda side: [r[side]["ms"] for r in table if r[side] and r[side]["ms"] is not None]  # noqa: E731
    return {"scenarios": len(table), "identical": all(r["ranks_equal"] and r["evidence_equal"] for r in table),
            "totals": {side: {k: total(side, k) for k in ("rule2", "rule1", "same_path", "essential")}
                       for side in ("a", "b")},
            "latency_ms": {side: {"mean": round(mean(ms(side))) if ms(side) else None,
                                  "median": round(median(ms(side))) if ms(side) else None} for side in ("a", "b")},
            "table": table}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--a", type=Path, required=True)
    parser.add_argument("--b", type=Path, required=True)
    parser.add_argument("--label-a", default="a")
    parser.add_argument("--label-b", default="b")
    parser.add_argument("--expect-identical", action="store_true")
    parser.add_argument("--out", type=Path)
    args = parser.parse_args(argv)
    result = compare(common.read_json(args.a), common.read_json(args.b))
    result.update(a=str(args.a), b=str(args.b), label_a=args.label_a, label_b=args.label_b)
    if args.out:
        common.write_json(args.out, result)
    print(f"| scenario | {args.label_a} r2/r1/path | {args.label_b} r2/r1/path | deepest {args.label_a} | "
          f"deepest {args.label_b} | hits | ms {args.label_a} | ms {args.label_b} | identical |")
    print("|---|---|---|---|---|---|---|---|---|")
    for r in result["table"]:
        a, b = r["a"] or {}, r["b"] or {}
        print(f"| {r['scenario']} | {a.get('rule2')}/{a.get('rule1')}/{a.get('same_path')} of {a.get('essential')} | "
              f"{b.get('rule2')}/{b.get('rule1')}/{b.get('same_path')} | {a.get('deepest')} | {b.get('deepest')} | "
              f"{a.get('hits')}->{b.get('hits')} | {a.get('ms')} | {b.get('ms')} | "
              f"{r['ranks_equal'] and r['evidence_equal']} |")
    print(json.dumps({k: result[k] for k in ("identical", "totals", "latency_ms")}))
    return 1 if args.expect_identical and not result["identical"] else 0


if __name__ == "__main__":
    sys.exit(main())
