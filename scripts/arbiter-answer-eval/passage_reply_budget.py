# SPDX-License-Identifier: Apache-2.0
"""Measure full MCP stdio frames for an offline, source-revision-bound Gather Context brief."""
import io
import json

import common
import mcp_wrapper


def encode(body, request_id, framing, error=False):
    content = json.dumps(body, ensure_ascii=False, separators=(",", ":"))
    result = {"content": [{"type": "text", "text": content}], "isError": error}
    stream = io.BytesIO()
    mcp_wrapper.write_message(stream, mcp_wrapper._result(request_id, result), framing)
    return stream.getvalue()


def effective_wire_limit(args, requested_cap):
    """Apply the admitted Gather budget, native frame ceiling and experiment cap together."""
    cases = common.read_json(common.ANSWER_CASES)
    caller = args.get("budget", {}).get("max_bytes", cases["default_budget"]["max_bytes"])
    return min(requested_cap, caller, cases["reply_frame_bytes"])


def render(task, needs, revision, passages, candidates, max_bytes, request_id=1, framing="line", *, decorate=None, owners=None):
    """Greedily admit complete passages in preselected order while preserving citation identity.

    Omission count includes input-selector omissions; reserve their counter and all need slots
    before placing any text. A too-small budget yields a bounded error or a caller-side refusal.
    """
    if max_bytes <= 0 or framing not in ("line", "header"):
        raise ValueError("Invalid reply cap or framing")
    if candidates < len(passages):
        raise ValueError("Inconsistent candidate count")

    if owners is None:
        owners = lambda p: [p.owner] if p.owner else []

    def frame(rows):
        body = {"schema": "airk.gather-context.brief/1", "task": task, "source_revision": revision,
                "needs": [], "passages": [], "omitted_count": candidates - len(rows)}
        for i, text in enumerate(needs, 1):
            matches = [p.at for p in rows if i in owners(p)]
            body["needs"].append({"need": i, "text": text, "status": "candidate_evidence" if matches else "no_candidate_evidence",
                                  "where": [{"at": at} for at in matches]})
        for p in rows:
            body["passages"].append({"at": p.at, "needs": owners(p), "text": p.text})
        if decorate is not None:
            decorate(body)
        return encode(body, request_id, framing)

    chosen = []
    blob = frame(chosen)
    if len(blob) > max_bytes:
        refused = encode({"error": {"code": "reply_budget_too_small"}}, request_id, framing, error=True)
        return (refused if len(refused) <= max_bytes else None), chosen
    for p in passages:
        proposed = frame([*chosen, p])
        if len(proposed) <= max_bytes:
            chosen.append(p)
            blob = proposed
    return blob, chosen

def compare(campaign, receipt, allocation_file, reference, caps):
    from pathlib import Path
    import adaptive_passage_budget as replay
    import prompt_need_audit as audit
    from agent_cost_batch import working_set

    selected = audit.read_json(allocation_file)
    if selected["receipt_sha256"] != audit.digest(receipt.read_bytes()) or selected["reference_sha256"] != audit.digest(reference.read_bytes()):
        raise ValueError("Receipt or reference identity changed")
    annotations = audit.read_json(reference)["tasks"]
    tasks = {r["id"]: r for r in working_set(campaign)}
    result = {}
    for line in receipt.read_bytes().splitlines():
        item = json.loads(line)
        if item["type"] != "task":
            continue
        key = item["task"]
        args = item["arguments"]
        snapshot = Path(item["source"])
        stream = replay.input_stream(item, snapshot, len(args["needs"]))
        by_location = {p.at: p for p in stream}
        task = audit.read_json(tasks[key]["task"])
        revision = audit.read_json(tasks[key]["snapshot"].with_suffix(".json"))["commit"]
        if task.get("source_commit", task.get("commit")) != revision:
            raise ValueError("Reference revision differs from snapshot receipt")
        notes = {n["id"]: n for n in task["notes"]}
        links = annotations[key]["need_notes"]
        arms = {}
        for arm, allocation in selected["tasks"][key]["arms"].items():
            passages = [by_location[r["at"]] for r in allocation["passages"]]
            arms[arm] = {}
            for cap in caps:
                hard_limit = effective_wire_limit(args, cap)
                wire, chosen = render(args["task"], args["needs"], revision, passages, len(stream), hard_limit)
                if wire is None:
                    raise ValueError("No room for a protocol reply at comparison cap")
                actual = json.loads(wire)["result"]
                if actual["isError"]:
                    raise ValueError("Too little room for mandatory metadata at comparison cap")
                body = json.loads(actual["content"][0]["text"])
                verified = {"passages": [{"at": p["at"], "sha256": audit.digest(p["text"].encode()), "needs": p["needs"]}
                                         for p in body["passages"]],
                            "budget": allocation["budget"], "reply_needs": body["needs"],
                            "passage_bytes": sum(len(p["text"].encode()) for p in body["passages"])}
                graded = audit.score_variant(verified, snapshot, notes, links, expected_budget=allocation["budget"])
                arms[arm][str(cap)] = {"effective_wire_cap": hard_limit, "wire_bytes": len(wire), "wire_sha256": audit.digest(wire),
                                       "passage_bytes": verified["passage_bytes"], "passages": verified["passages"],
                                       "omitted_count": body["omitted_count"], "dropped_by_wire": len(passages) - len(chosen),
                                       "totals": audit.totals(graded)}
        result[key] = arms
    if result.keys() != selected["tasks"].keys():
        raise ValueError("Missing or duplicate tasks in wire replay")
    return {"allocation_sha256": audit.digest(allocation_file.read_bytes()), "renderer_sha256": audit.digest(Path(__file__).read_bytes()),
            "framing": "prototype stdio newline-delimited JSON-RPC, request ID 1; revision is source commit",
            "tasks": result}


if __name__ == "__main__":
    import argparse
    from pathlib import Path
    ap = argparse.ArgumentParser(description=__doc__)
    for name in ("campaign", "receipt", "allocation", "reference", "output"):
        ap.add_argument(name, type=Path)
    ap.add_argument("--caps", type=int, nargs="+", default=[8192, 14000, 20000])
    args = ap.parse_args()
    result = compare(args.campaign, args.receipt, args.allocation, args.reference, args.caps)
    with args.output.open("x", encoding="utf-8") as stream:
        json.dump(result, stream, ensure_ascii=True, indent=1)
        stream.write("\n")