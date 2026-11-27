# SPDX-License-Identifier: Apache-2.0
"""Run native Gather Context on chosen cases, with and without caller inputs.

Evaluation tooling for a development diagnostic, never a judged pass. For
each chosen case it sends the task to a ready fixture lifetime twice: once
alone (``task``), and once with the ``needs`` and ``anchors`` a calling agent
wrote for it (``needs``). It keeps each raw reply as
``<arm>/<case>.reply.json``, the shape ``gather_context_loss_stage.py``
reads, and one ``run.json`` with the requests, the observed model stages and
timings. The budget is the one the settled control used, read from a
retained case file.

    python -B gather_context_needs_run.py --handle <lifetime>/handle.json --scenarios <dir> \
        --needs <caller-needs.json> --budget-from <settled .case.json> --output <new dir> \
        --cases munarium-C3 ...

A caller-needs file is ``{"cases": {"<id>": {"needs": [...], "anchors":
[...]}}}``. Needs beyond eight and anchors beyond sixteen are refused here,
as the request contract refuses them.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

import common
import loop_reply
from gather_context_capture import _native_call

SCHEMA = "airk.private.gather-context-needs-run/1"
ARMS = ("task", "needs")
MAX_NEEDS, MAX_ANCHORS = 8, 16
DELIVERABLES = ("code_change", "test", "document", "analysis")


def caller_inputs(written: dict, case_id: str) -> dict:
    row = written.get(case_id)
    if not isinstance(row, dict) or not row.get("needs"):
        raise ValueError(f"no caller needs for {case_id}")
    needs, anchors = list(row["needs"]), list(row.get("anchors") or [])
    if len(needs) > MAX_NEEDS or len(anchors) > MAX_ANCHORS:
        raise ValueError(f"{case_id} states more needs or anchors than a request may carry")
    if not all(isinstance(item, str) and item.strip() for item in needs + anchors):
        raise ValueError(f"{case_id} has an empty need or anchor")
    inputs = {"needs": needs}
    if anchors:
        inputs["anchors"] = anchors
    deliverable = str(row.get("deliverable") or "").strip().lower().replace(" ", "_")
    if deliverable:
        if deliverable not in DELIVERABLES:
            raise ValueError(f"{case_id} names an unknown deliverable")
        inputs["deliverable"] = deliverable
    return inputs


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--handle", required=True, type=pathlib.Path)
    parser.add_argument("--scenarios", required=True, type=pathlib.Path)
    parser.add_argument("--needs", required=True, type=pathlib.Path)
    parser.add_argument("--budget-from", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument("--cases", required=True, nargs="+")
    parser.add_argument("--timeout", type=float, default=600.0)
    args = parser.parse_args(argv)
    written = json.loads(args.needs.read_text(encoding="utf-8"))["cases"]
    budget = json.loads(args.budget_from.read_text(encoding="utf-8"))["transport_request"]["arguments"]["budget"]
    handle = common.load_handle(args.handle.resolve(strict=True))
    args.output.mkdir()
    for arm in ARMS:
        (args.output / arm).mkdir()
    rows = []
    stages_at_start = handle.live_stages()
    for case_id in args.cases:
        repository, slot = case_id.split("-", 1)
        scenario = json.loads((args.scenarios / repository / f"{slot}.json").read_text(encoding="utf-8"))
        inputs = caller_inputs(written, case_id)
        for arm in ARMS:
            outcome, dispatch = _native_call(handle, repository, scenario["instruction"], {"budget": budget},
                                             args.timeout, inputs if arm == "needs" else None)
            with (args.output / arm / f"{case_id}.reply.json").open("xb") as file:
                file.write(outcome.stdout)
            body = common.envelope_body(outcome.envelope)
            done = bool(outcome.ok and isinstance(body, dict) and not loop_reply.is_ticket(body))
            work = body.get("work", {}) if done else {}
            parts = body.get("parts", []) if done else []
            rows.append({"case": case_id, "arm": arm, "completed": done, "error": outcome.error,
                         "elapsed_ms": dispatch["elapsed_ms"], "calls": work.get("calls"),
                         "rounds": work.get("rounds"), "parts": len(parts),
                         # A reused job means this request was not gathered for: the arm is void.
                         "reused": ((body.get("delivery") if done else None) or work.get("delivery")
                                    or {}).get("reused"),
                         "caller_sub_needs": sum(need.get("origin") == "caller" for part in parts
                                                 for need in part.get("sub_needs", [])),
                         "sub_needs": sum(len(part.get("sub_needs", [])) for part in parts),
                         "stops": [part.get("stop") for part in parts],
                         "reply_bytes": len(common.envelope_text(outcome.envelope) or ""),
                         # The kernel's own per-stage counts and timings, when the image reports them.
                         "gathering": body.get("gathering") if done else None,
                         "standing_sub_needs": sum(need.get("origin") == "standing" for part in parts
                                                   for need in part.get("sub_needs", [])),
                         "arguments": dispatch["arguments"],
                         "reply_sha256": common.sha256_bytes(outcome.stdout)})
            print(f"{case_id} {arm} completed={done} reused={rows[-1]['reused']} calls={work.get('calls')} "
                  f"sub_needs={rows[-1]['sub_needs']} caller={rows[-1]['caller_sub_needs']} "
                  f"{dispatch['elapsed_ms']} ms", flush=True)
    run = {"schema": SCHEMA, "handle_sha256": handle.sha256, "stages_at_start": stages_at_start,
           "stages_at_end": handle.live_stages(), "budget": budget, "rows": rows}
    with (args.output / "run.json").open("x", encoding="utf-8", newline="\n") as file:
        file.write(json.dumps(run, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
