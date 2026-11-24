# SPDX-License-Identifier: Apache-2.0
"""One summary row per Answer of ``gate.py smoke`` receipts: what each arm's reply shows, for the coordinator.

Evaluation tooling, never product; a smoke is a pipeline test, never a result. The LH acceptance run (23 November
2026) summarized its four switch combinations with a one-off script; this is its maintained owner. Input is one or
more ``smoke.json`` files (``gate.py smoke --out <dir>`` writes ``<dir>/smoke.json``); output is a JSON list on
stdout, one row per smoke Answer:

| Field | Meaning |
| --- | --- |
| ``arm``, ``scenario``, ``elapsed_ms``, ``problems`` | the smoke row: arm, scenario id, call time and the smoke's own problems |
| ``records``, ``failed_records`` | search/read records in the reply and the failures by code |
| ``stops``, ``complete``, ``status`` | each part's stop, and the reply's completion and status |
| ``focus_calls``, ``card_calls`` | the decision log's Focus and Context (card) calls |
| ``log_counts``, ``context_presence`` | the decision log's counts by stage, source and model, and the Context stage's presence |
| ``included_decisions`` | included decisions counted by ``stage\\|source\\|model\\|device\\|precision`` |
| ``degradations``, ``model_work`` | degradation kinds, and the reply's own model work (profiles, calls, device, precision) |

``python -I -B smoke_summary.py SMOKE.json [SMOKE.json ...]``
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import loop_reply  # noqa: E402

MODEL_WORK_KEYS = ("focus_profile", "focus_calls", "focus_pairs", "context_profile", "card_calls", "context_calls",
                   "inference_us", "device", "precision")


def decision_sources(body) -> dict[str, int]:
    """Included decisions counted by ``stage|source|model|device|precision``."""
    counts: dict[str, int] = {}
    for decision in loop_reply.decisions(body):
        key = "|".join(str(decision.get(k)) for k in ("stage", "source", "model", "device", "precision"))
        counts[key] = counts.get(key, 0) + 1
    return counts


def row_summary(arm: str, row: dict, body) -> dict:
    """The summary row of one smoke Answer ``row`` of arm ``arm``, whose reply body is ``body``."""
    log = body.get("decision_log") if isinstance(body.get("decision_log"), dict) else {}
    work = body.get("model_work") if isinstance(body.get("model_work"), dict) else {}
    loop = row.get("loop") if isinstance(row.get("loop"), dict) else {}
    return {"arm": arm, "scenario": row["scenario_id"], "elapsed_ms": row.get("elapsed_ms"),
            "problems": row.get("problems"), "records": len(loop_reply.records(body)),
            "failed_records": loop_reply.read_failures(body), "stops": loop.get("stops"),
            "complete": body.get("complete"), "status": body.get("status"),
            "focus_calls": log.get("focus_calls"), "card_calls": log.get("card_calls"),
            "log_counts": loop_reply.log_counts(body), "context_presence": loop_reply.context_presence(body),
            "included_decisions": decision_sources(body), "degradations": loop_reply.degradation_kinds(body),
            "model_work": {k: work.get(k) for k in MODEL_WORK_KEYS if k in work},
            "model_work_keys": sorted(work)}


def summarize(smoke_path: Path) -> list[dict]:
    document = common.read_json(smoke_path)
    out = []
    for row in document["rows"]:
        body = common.raw_body(Path(row["result_path"]).read_bytes()) if row.get("result_path") else None
        if body is None:  # a failed call kept no reply: the smoke's problems say why
            out.append({"arm": document["arm"], "scenario": row["scenario_id"], "elapsed_ms": row.get("elapsed_ms"),
                        "problems": row.get("problems"), "reply": None})
            continue
        out.append(row_summary(document["arm"], row, body))
    return out


def main(argv: list[str] | None = None) -> int:
    paths = [Path(a) for a in (sys.argv[1:] if argv is None else argv)]
    if not paths:
        print(__doc__.splitlines()[0], file=sys.stderr)
        return 2
    print(json.dumps([r for p in paths for r in summarize(p)], indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
