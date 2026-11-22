# SPDX-License-Identifier: Apache-2.0
"""Re-judge a graded pass with other judge models and compare the verdicts.

For every judged result of the pass index, the judging packet is rebuilt from
the session files. It must be byte-identical to the original ``packet.json``,
or the row is reported as having a changed packet. The packet is then judged
again by ``--judges`` fresh sessions on ``--model``/``--effort``, under
``<pass>/rejudge-<label>/<key>/<judge>/``. The comparison is with the
original verdicts:

* **Grades.** Per-result Rubric A grade agreement, under coverage rule 1 and
  rule 2. The coverage files are ``coverage.json`` and ``coverage-rule2.json``.
* **Facts.** Per-fact agreement on correct, wrong and missing.
* **Rubric B.** The difference in means.

Writes ``<pass>/rejudge-<label>.json``.

``python -I -B rejudge.py --pass-dir <R>/passes/pilot --label sonnet --model claude-sonnet-5-5 --effort medium``
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import sys
from pathlib import Path
from statistics import mean

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import grade  # noqa: E402
import judge  # noqa: E402


def _verdict(path: Path) -> dict | None:
    if not path.is_file():
        return None
    record = common.read_json(path)
    return record.get("verdict") if record.get("status") == "valid" else None


def _grades(coverage: dict | None, verdicts: list[dict | None], status: str) -> dict:
    g = [grade.grade_one(coverage, v, status) if v is not None else None for v in verdicts]
    while len(g) < 2:
        g.append(None)
    adjudicated = grade.adjudicate(g[0], g[1], None)
    return {"judges": [None if x is None else x["grade"] for x in g], "adjudicated": adjudicated["grade"],
            "source": adjudicated["source"], "correct_finish": adjudicated["correct_finish"]}


def compare_row(row: dict, original: list[dict | None], new: list[dict | None], status: str) -> dict:
    out = {"key": row["key"]}
    for rule, name in ((1, "coverage.json"), (2, "coverage-rule2.json")):
        path = Path(row["session_dir"]) / name
        coverage = common.read_json(path) if path.is_file() else None
        a, b = _grades(coverage, original, status), _grades(coverage, new, status)
        out[f"rule{rule}"] = {"original": a, "new": b, "adjudicated_agree": a["adjudicated"] == b["adjudicated"]}
    pairs = agree = 0
    for o in original:
        for n in new:
            if o is None or n is None:
                continue
            ov = {x["id"]: x["verdict"] for x in o.get("notes") or []}
            nv = {x["id"]: x["verdict"] for x in n.get("notes") or []}
            for note_id, verdict in ov.items():
                if note_id in nv:
                    pairs += 1
                    agree += verdict == nv[note_id]
    out["fact_pairs"], out["fact_agree"] = pairs, agree
    rb_o = [grade.rubric_b(v) for v in original if v is not None]
    rb_n = [grade.rubric_b(v) for v in new if v is not None]
    rb_o, rb_n = [x for x in rb_o if x is not None], [x for x in rb_n if x is not None]
    out["rubric_b_original"] = round(mean(rb_o), 3) if rb_o else None
    out["rubric_b_new"] = round(mean(rb_n), 3) if rb_n else None
    out["misleading"] = {"original": [v.get("misleading_followed") if v else None for v in original],
                         "new": [v.get("misleading_followed") if v else None for v in new]}
    return out


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--pass-dir", type=Path, required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--model", default=common.JUDGE_MODEL)
    parser.add_argument("--effort", default=common.JUDGE_EFFORT, choices=common.EFFORTS)
    parser.add_argument("--judges", default="j1,j2")
    parser.add_argument("--concurrency", type=int, default=4)
    parser.add_argument("--claude")
    parser.add_argument("--scenarios-dir", type=Path, default=common.DEFAULT_R / "scenarios")
    args = parser.parse_args(argv)
    index = common.read_json(args.pass_dir / "pass-index.json")
    scenarios = {s["id"]: s for s in map(common.load_scenario, common.scenario_files(args.scenarios_dir))}
    out_root = args.pass_dir / f"rejudge-{args.label}"
    names = args.judges.split(",")
    rows = [r for r in index["results"] if not r.get("reused_from")
            and (Path(r["session_dir"]) / "session.json").is_file()
            and common.read_json(Path(r["session_dir"]) / "session.json").get("status") == "complete"]
    packets = {}
    for row in rows:
        rebuilt = judge.build_packet(scenarios[row["scenario_id"]], Path(row["session_dir"]), row["key"])
        original = Path(row["judges"][0]).parent / "packet.json"
        packets[row["key"]] = {"same_packet": original.is_file() and common.read_json(original) == rebuilt,
                               "original_packet_sha256": common.sha256_file(original) if original.is_file() else None}
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        futures = [pool.submit(judge.run_judge, scenario=scenarios[row["scenario_id"]],
                               session_dir=Path(row["session_dir"]), key=row["key"], out_dir=out_root / row["key"],
                               judge=name, model=args.model, effort=args.effort, claude=args.claude)
                   for row in rows for name in names]
        records = [f.result() for f in futures]
    cost = sum(r.get("cost_usd") or 0 for r in records)
    comparisons = []
    for row in rows:
        original = [_verdict(Path(p)) for p in row["judges"]]
        new = [_verdict(out_root / row["key"] / name / "verdict.json") for name in names]
        status = common.read_json(Path(row["session_dir"]) / "session.json").get("status")
        comparisons.append({**compare_row(row, original, new, status), **packets[row["key"]]})
    total_pairs = sum(c["fact_pairs"] for c in comparisons)
    report = {"schema": "airk.private.answer-eval-rejudge/1", "generated": common.now_iso(), "pass": index["series"],
              "label": args.label, "model": args.model, "effort": args.effort, "judges": names,
              "results": len(comparisons), "invalid_verdicts": sum(1 for r in records if r.get("status") != "valid"),
              "cost_usd": round(cost, 4), "cost_usd_per_result": round(cost / len(comparisons), 4) if comparisons else None,
              "packets_identical": all(c["same_packet"] for c in comparisons),
              "grade_agreement_rule1": sum(c["rule1"]["adjudicated_agree"] for c in comparisons),
              "grade_agreement_rule2": sum(c["rule2"]["adjudicated_agree"] for c in comparisons),
              "fact_agreement": round(sum(c["fact_agree"] for c in comparisons) / total_pairs, 4) if total_pairs else None,
              "fact_pairs": total_pairs,
              "rubric_b_mean_difference": round(mean(c["rubric_b_new"] - c["rubric_b_original"] for c in comparisons
                                                     if c["rubric_b_new"] is not None
                                                     and c["rubric_b_original"] is not None), 3),
              "rows": comparisons}
    common.write_json(args.pass_dir / f"rejudge-{args.label}.json", report)
    print(json.dumps({k: v for k, v in report.items() if k != "rows"}, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
