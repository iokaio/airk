# SPDX-License-Identifier: Apache-2.0
"""One row per ``gpu_fixture.py answer`` record of a shakedown: what the reply shows, for the coordinator.

Evaluation tooling, never product; a shakedown's figures are a pipeline test, never a result. Per record:
time, body size and the rendered text-only size, parts and stops, decision counts by source and stage, Focus
calls and pairs (log) with the native Focus pairs dispatched and scored, card calls with tokens and inference
time, the verification block (requested, checked, unchecked, limits hit, rows omitted), read failures, notes
covered and the arm check. A record whose call failed reports the error code from the saved output.

``python -I -B shakedown_table.py --scenarios R/scenarios --arm-check '{...}' --out table.json RECORD ...``
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import coverage  # noqa: E402
import loop_reply  # noqa: E402


def row_for(record_path: Path, scenarios: Path, arm_check: dict) -> dict:
    rec = common.read_json(record_path)
    label = rec.get("label")
    row = {"label": label, "elapsed_ms": rec.get("elapsed_ms"), "is_error": rec.get("is_error")}
    raw = Path(rec["result"]["path"]).read_bytes()
    if rec.get("is_error") or "answer" not in rec:
        found = re.search(rb'\\?"code\\?"\s*:\s*\\?"([A-Za-z0-9_]+)', raw)
        row["error_code"] = found.group(1).decode() if found else "unknown"
        return row
    envelope = json.loads(raw)
    body = common.raw_body(raw)
    summary = loop_reply.summary(body)
    text_only = len(json.dumps({"content": envelope.get("content"), "isError": envelope.get("isError")},
                               separators=(",", ":"), ensure_ascii=False).encode("utf-8"))
    sv = body.get("source_verification") if isinstance(body.get("source_verification"), dict) else None
    dispatch = (rec.get("device") or {}).get("dispatch") or []
    focus_rows = [d for d in dispatch if str(d.get("task", "")).startswith("airk.focus")]
    row.update({
        "body_bytes": rec.get("reply_bytes"), "rendered_text_only_bytes": text_only,
        "parts": summary["parts"], "stops": summary["stops"], "partial": summary["partial"],
        "decision_counts": summary["decision_counts"], "fallback_reasons": summary["fallback_reasons"],
        "focus_calls": summary["focus_calls"], "focus_pairs": summary["focus_pairs"],
        "focus_pairs_dispatched": len(focus_rows),
        "focus_pairs_scored": sum(1 for d in focus_rows if d.get("outcome") == "success" and d.get("finite")),
        "cards": [{"task": d["task"].split(".")[1], "tokens": (d.get("work") or {}).get("input_tokens"),
                   "inference_ms": round(((d.get("work") or {}).get("inference_us") or 0) / 1000),
                   "outcome": d.get("outcome")} for d in dispatch if not str(d.get("task", "")).startswith("airk.focus")],
        "model_timings": summary["model_timings"], "degradations": summary["degradations"],
        "read_failures": summary["read_failures"],
        "verification": None if sv is None else {
            "status": sv.get("status"), "mode": sv.get("mode"), "bytes": len(json.dumps(sv, separators=(",", ":")).encode()),
            "requested": (sv.get("counts") or {}).get("requested_sources"),
            "checked": (sv.get("counts") or {}).get("checked_sources"),
            "unchecked": (sv.get("counts") or {}).get("unchecked_sources"),
            "limits_hit": (sv.get("limits") or {}).get("hit"), "source_rows_omitted": sv.get("source_rows_omitted"),
            "all_references_accounted": sv.get("all_references_accounted")},
        "packing": summary["packing"],
        "arm_problems": loop_reply.check_arm(body, arm_check),
        "evidence_digest": rec.get("evidence_digest")})
    question = common.read_json(Path(rec["request"]["path"])).get("question")
    for path in sorted(Path(scenarios).glob("*.json")):
        scenario = common.load_scenario(path)
        if scenario.get("instruction") == question:
            first = coverage.first_reply(scenario, body, rec.get("reply_bytes"))
            row.update(scenario=scenario["id"], essential=first["essential_covered"],
                       essential_total=first["essential_total"], supporting=first["supporting_covered"])
    return row


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--scenarios", type=Path, required=True, help="one repository's scenario folder")
    parser.add_argument("--arm-check", default='{"context": "arbiter", "device": "cuda", '
                                              '"precision": "float16", "mode": "top_score", "focus": true}')
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("records", type=Path, nargs="+")
    args = parser.parse_args(argv)
    rows = [row_for(p, args.scenarios, json.loads(args.arm_check)) for p in args.records]
    common.write_json(args.out, {"schema": "airk.private.answer-eval-shakedown-table/1", "generated": common.now_iso(),
                                 "note": "pipeline test, never a result", "rows": rows})
    for row in rows:
        print(brief(row))
    return 0


def brief(r: dict) -> str:
    """One line per row for a message."""
    if "error_code" in r:
        return f"{r.get('label')}: FAILED {r['error_code']} after {r.get('elapsed_ms')} ms"
    v = r["verification"] or {}
    cards = " ".join(f"{c['task'][8:]}:{c['tokens']}t/{c['inference_ms']}ms" for c in r["cards"])
    counts = "; ".join(f"{stage} " + ",".join(f"{src} {n}" for src, n in by.items())
                       for stage, by in r["decision_counts"].items())
    return (f"{r.get('scenario', r.get('label'))}: {r['elapsed_ms'] / 1000:.1f} s, body {r['body_bytes']}, rendered "
            f"{r['rendered_text_only_bytes']}, parts {r['parts']} {r['stops']} | {counts} | Focus {r['focus_calls']} "
            f"calls {r['focus_pairs']} pairs, native {r['focus_pairs_dispatched']} dispatched "
            f"{r['focus_pairs_scored']} scored | cards {cards} | verification {v.get('status')} requested "
            f"{v.get('requested')} checked {v.get('checked')} unchecked {v.get('unchecked')} hit {v.get('limits_hit')} "
            f"rows omitted {v.get('source_rows_omitted')} ({v.get('bytes')} B) | read failures {r['read_failures'] or 0}"
            f" | essential {len(r.get('essential') or [])}/{r.get('essential_total')} supporting "
            f"{len(r.get('supporting') or [])} | arm {'ok' if not r['arm_problems'] else r['arm_problems']}"
            f" | degradations {r['degradations']}")


if __name__ == "__main__":
    sys.exit(main())
