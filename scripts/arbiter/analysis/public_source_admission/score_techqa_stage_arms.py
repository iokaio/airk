# SPDX-License-Identifier: Apache-2.0
"""Score the TechQA stage arms: delivered publisher spans with and without Focus.

Scoring reads the frozen rule, the closed run and both arms' retained Answers.
It applies no pass line. ``--focus-census`` is the model-free check made between
the arms: it tokenises the first Focus pairs of every arm A initial search and
lists the pairs the static profile is expected to refuse.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import runpy
import statistics
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import run_techqa_stage_arms as arms
import score_techqa_answer_pair as pair
from verify_techqa_delivered_evidence import returned_interval

require = pair.require
CATEGORIES = ("both", "a_only", "b_only", "neither", "unresolved", "error")
_RUBRIC = {}


def covered_location(goal, intervals):
    if not _RUBRIC:
        path = arms.ROOT / "tools/retrieval-train/goal_rubric.py"
        require(pair.digest(path) == pair.RUBRIC_SHA, "frozen coverage rule changed")
        _RUBRIC["covered"] = runpy.run_path(str(path))["covered_location"]
    return _RUBRIC["covered"](goal, intervals)


def identity(hit):
    citation = hit["citation"]
    row = [citation["path"], citation["content_hash"], citation["start_line"], citation["end_line"], hit["text"]]
    return hashlib.sha256(json.dumps(row, separators=(",", ":")).encode()).hexdigest()


def missing(start, end, intervals):
    """Uncovered byte sub-ranges of the publisher span."""
    gaps, position = [], start
    for left, right in sorted(intervals):
        if right <= position:
            continue
        if left >= end:
            break
        if left > position:
            gaps.append([position, left])
        position = right
        if position >= end:
            return gaps
    return gaps + [[position, end]]


def evidence(packet, token, members, target, body):
    """Coverage of the one documented publisher span by the delivered records."""
    start, end = target["byte_start"], target["byte_end_exclusive"]
    try:
        publication = packet["assessment"]["scope"]["publication"]
        require(publication["token"] == token, "Answer publication differs from the indexed publication")
        score = pair.coverage(packet, publication, members, target, body(target["original_path"]))
    except (ValueError, KeyError, TypeError) as error:
        return {"coverage": "unresolved", "reason": str(error)[:200]}
    gaps = missing(start, end, [(row["byte_start"], row["byte_end"]) for row in score["delivered_target_ranges"]])
    delivered_bytes = end - start - sum(right - left for left, right in gaps)
    if score["documented_set_covered"] is True:
        state = "covered"
    elif score["documented_set_covered"] is None:
        state = "unresolved"
    else:
        state = "partial" if delivered_bytes else "not_delivered"
    return {"coverage": state, "span_bytes": end - start, "delivered_span_bytes": delivered_bytes,
            "missing_ranges": gaps, "unresolved_target_hits": score["unresolved_delivered_target_hits"],
            "gold_document_cited": bool(score["delivered_target_ranges"] or score["unresolved_delivered_target_hits"])}


def delivered(packet, body, hashes):
    """Every delivered hit in record order with its located byte interval."""
    rows = []
    for position, record in enumerate(packet["records"]):
        hits = record["canonical_result"].get("hits")
        for hit in hits if isinstance(hits, list) else []:
            name = hit["citation"]["path"]
            try:
                require(hashes.get(name) == hit["citation"]["content_hash"], "delivered revision differs")
                extent = returned_interval(body(name), hit)
            except (ValueError, KeyError, TypeError):
                extent = None
            rows.append({"record": position, "operation": record["operation"], "file": name,
                         "bytes": len(hit["text"].encode()), "interval": extent})
    return rows


def duplicates(rows):
    """Bytes delivered more than once. Hits that cannot be located are counted apart."""
    seen, total, inside = {}, 0, 0
    lost = [row for row in rows if row["interval"] is None]
    for row in rows:
        if row["interval"] is None:
            continue
        span = {"file": row["file"], "byte_start": row["interval"][0], "byte_end": row["interval"][1]}
        earlier = seen.setdefault(row["file"], [])
        inside += bool(earlier) and covered_location(span, earlier)
        earlier.append(span)
        total += span["byte_end"] - span["byte_start"]
    union = {}
    for name, spans in seen.items():
        size, position = 0, 0
        for left, right in sorted((span["byte_start"], span["byte_end"]) for span in spans):
            size += max(0, right - max(left, position))
            position = max(position, right)
        union[name] = size
    return {"hits": len(rows), "delivered_bytes": total, "union_bytes": sum(union.values()),
            "duplicate_bytes": total - sum(union.values()), "hits_inside_earlier": inside, "files": len(union),
            "union_bytes_by_file": union, "unresolved_hits": len(lost),
            "unresolved_bytes": sum(row["bytes"] for row in lost)}


def focus_states(packet):
    """Per search: applied, failed or not applied, with the reason and whether the order changed."""
    rows = []
    for position, record in enumerate(packet["records"]):
        if record["operation"] != "search":
            continue
        focus = record["canonical_result"].get("focus")
        if focus is None:
            rows.append({"record": position, "state": "not_applied", "reason": "no Focus observation"})
        elif focus["kind"] == "not_applied":
            rows.append({"record": position, "state": "not_applied", "availability": focus["availability"],
                         "reason": focus["reason"], "work_unknown": focus["work_unknown"]})
        else:
            receipt = focus["receipt"]
            rows.append({
                "record": position, "state": "applied" if receipt["applied"] else "failed",
                "availability": receipt["availability"], "reason": receipt["reason"],
                "order_changed": receipt["ranked_order"] != receipt["original_order"],
                "presented": receipt["presented_count"], "scored": receipt["scored_count"],
                "observed_input_tokens": receipt["observed_input_tokens"],
                "reserved_input_tokens": receipt["reserved_input_tokens"], "work": receipt["work"],
                "whole_hits_presented": all(row["utf8_start"] == 0 and row["utf8_end"] == row["source_bytes"]
                                            for row in receipt["presentations"]),
                "receipt_bytes": len(json.dumps(focus, separators=(",", ":")).encode())})
    return rows


def initial_search(packet, question, target, body):
    """Gold ranks in the first search, after Focus and, from the receipt, before it."""
    record = packet["records"][0]
    require(record["operation"] == "search" and record["arguments"]["query"] == question,
            "first delivered record is not the default search for the sealed question")
    hits = record["canonical_result"]["hits"]
    focus = record["canonical_result"].get("focus") or {}
    before = list(range(len(hits)))
    if focus.get("kind") == "receipt":
        original, ranked = focus["receipt"]["original_order"], focus["receipt"]["ranked_order"]
        require(len(original) == len(ranked) == len(hits) and sorted(original) == sorted(ranked)
                and len(set(original)) == len(original), "Focus receipt does not bind the returned hits")
        before = [original.index(candidate) for candidate in ranked]
    start, end = target["byte_start"], target["byte_end_exclusive"]
    gold, order = [], [None] * len(hits)
    for position, hit in enumerate(hits):
        order[before[position]] = identity(hit)
        if hit["citation"]["path"] != target["original_path"]:
            continue
        try:
            extent = returned_interval(body(target["original_path"]), hit)
        except ValueError:
            extent = None
        gold.append({"rank": position + 1, "rank_before_focus": before[position] + 1,
                     "overlaps_span": None if extent is None else extent[0] < end and extent[1] > start})

    def best(field, spans_only):
        ranks = [row[field] for row in gold if row["overlaps_span"] or not spans_only]
        return min(ranks) if ranks else None

    return {"hits": len(hits), "order_before_focus": order, "order_changed": before != list(range(len(hits))),
            "gold_document_hits": gold, "gold_document_rank": best("rank", False),
            "gold_document_rank_before_focus": best("rank_before_focus", False),
            "gold_span_rank": best("rank", True), "gold_span_rank_before_focus": best("rank_before_focus", True)}


def arm_row(call, packet, question, token, members, target, body):
    """One arm of one case. A call without a terminal Answer keeps its own state."""
    row = {"call_state": call["state"], "elapsed_ns": call.get("elapsed_ns")}
    if call["state"] != "ok":
        return dict(row, coverage=call["state"])
    try:
        stop = pair.terminal_packet(packet)
        require(packet["question"] == question, "delivered question differs from the sealed request")
    except (ValueError, KeyError, TypeError) as error:
        return dict(row, call_state="tool_error", coverage="tool_error", reason=str(error)[:200])
    row.update(stop=stop, records=len(packet["records"]), omitted_count=packet.get("omitted_count"),
               work=packet.get("work"), scope_observation=packet.get("scope_observation"),
               index_id=packet["assessment"]["scope"].get("index_id"),
               operations=[record["operation"] for record in packet["records"]])
    row.update(evidence(packet, token, members, target, body))
    row["duplicates"] = duplicates(delivered(packet, body, {m["path"]: m["original_sha256"] for m in members}))
    row["focus"] = focus_states(packet)
    try:
        row["initial"] = initial_search(packet, question, target, body) if packet["records"] else None
    except (ValueError, KeyError, TypeError) as error:
        row.update(initial=None, initial_reason=str(error)[:200])
    return row


def pairing(a, b):
    """Both arms must start from the same request, index, publication and initial hits."""
    checks = {"request": a.get("request") is not None and a.get("request") == b.get("request")}
    if a["call_state"] == b["call_state"] == "ok":
        checks["index"] = a["index_id"] == b["index_id"]
        checks["scope"] = a["scope_observation"] == b["scope_observation"]
        first, second = a.get("initial"), b.get("initial")
        if first and second:
            checks["initial_hits"] = sorted(first["order_before_focus"]) == sorted(second["order_before_focus"])
            checks["order_before_focus"] = first["order_before_focus"] == second["order_before_focus"]
        elif "initial_reason" in a or "initial_reason" in b:
            checks["initial_hits"] = False
    return checks


def category(a, b, checks):
    if a["call_state"] != "ok" or b["call_state"] != "ok":
        return "error"
    if "unresolved" in (a["coverage"], b["coverage"]) or not all(checks.values()):
        return "unresolved"
    first, second = a["coverage"] == "covered", b["coverage"] == "covered"
    return "both" if first and second else "a_only" if first else "b_only" if second else "neither"


def tally(rows):
    counts = {name: sum(row["category"] == name for row in rows) for name in CATEGORIES}
    return {"denominator": len(rows), **counts, "net_b_minus_a": counts["b_only"] - counts["a_only"]}


def group_summary(rows, groups):
    """Paired counts per document-linked group, with every group weighted equally."""
    table = []
    for group in groups:
        members = [row for row in rows if row["case"] in group["cases"]]
        require(len(members) == len(group["cases"]) > 0, "group roster differs from scored cases")
        table.append({"group": group["group"], **tally(members)})
    weight = {name: round(sum(row[name] / row["denominator"] for row in table), 6) for name in CATEGORIES}
    return {"groups": len(table), "equal_weight": weight,
            "equal_weight_net_b_minus_a": round(weight["b_only"] - weight["a_only"], 6),
            "net_positive_groups": sum(row["net_b_minus_a"] > 0 for row in table),
            "net_zero_groups": sum(row["net_b_minus_a"] == 0 for row in table),
            "net_negative_groups": sum(row["net_b_minus_a"] < 0 for row in table), "rows": table}


def nearest_rank(values, percent):
    ordered = sorted(values)
    return ordered[max(1, math.ceil(percent * len(ordered) / 100)) - 1]


def distribution(values, scale=1.0):
    values = [value / scale for value in values]
    if not values:
        return {"n": 0}
    return {"n": len(values), "median": round(statistics.median(values), 3),
            "p95_nearest_rank": round(nearest_rank(values, 95), 3), "max": round(max(values), 3)}


def rank_bands(ranks):
    return {"rank_1_4": sum(rank is not None and rank <= 4 for rank in ranks),
            "rank_5_8": sum(rank is not None and 4 < rank <= 8 for rank in ranks),
            "absent": sum(rank is None for rank in ranks)}


def arm_figures(sides):
    """One arm's population figures. Counts only; no question, answer or source text."""
    answered = [side for side in sides if side["call_state"] == "ok"]
    searched = [side for side in answered if side.get("initial")]
    receipts = [[state for state in side["focus"] if "work" in state] for side in answered]
    scored = [sum(state["scored"] for state in states) for states in receipts]
    inference = [sum(state["work"]["inference_us"] or 0 for state in states) for states in receipts if states]
    first = [side["focus"][0]["state"] if side["focus"] else "no_search" for side in answered]
    every = [state for side in answered for state in side["focus"]]
    totals = {key: sum(side["duplicates"][key] for side in answered)
              for key in ("hits", "delivered_bytes", "union_bytes", "duplicate_bytes", "hits_inside_earlier",
                          "unresolved_hits", "unresolved_bytes")}
    return {
        "coverage": {state: sum(side["coverage"] == state for side in sides)
                     for state in sorted({side["coverage"] for side in sides})},
        "stops": {stop: sum(side["stop"] == stop for side in answered)
                  for stop in sorted({side["stop"] for side in answered})},
        "records": distribution([side["records"] for side in answered]),
        "omitted_records": sum(side["omitted_count"] or 0 for side in answered),
        "gold_document_cited": sum(bool(side.get("gold_document_cited")) for side in answered),
        "initial_search": {
            "cases": len(searched),
            "gold_document_rank": rank_bands([side["initial"]["gold_document_rank"] for side in searched]),
            "gold_document_rank_before_focus": rank_bands(
                [side["initial"]["gold_document_rank_before_focus"] for side in searched]),
            "gold_span_rank": rank_bands([side["initial"]["gold_span_rank"] for side in searched]),
            "gold_span_rank_before_focus": rank_bands(
                [side["initial"]["gold_span_rank_before_focus"] for side in searched]),
            "order_changed": sum(side["initial"]["order_changed"] for side in searched)},
        "focus": {
            "initial_search": {state: first.count(state) for state in sorted(set(first))},
            "all_searches": {state: sum(row["state"] == state for row in every)
                             for state in sorted({row["state"] for row in every})},
            "not_applied_or_failed_reasons": {
                reason: sum(row["reason"] == reason for row in every if row["state"] != "applied")
                for reason in sorted({row["reason"] for row in every if row["state"] != "applied"})},
            "pairs_scored": sum(scored), "cases_with_scored_pairs": sum(count > 0 for count in scored),
            "inference_ms_per_case": distribution(inference, 1000),
            "inference_ms_per_scored_pair": round(sum(inference) / 1000 / sum(scored), 3) if sum(scored) else None,
            "receipt_bytes": sum(row.get("receipt_bytes", 0) for row in every)},
        "duplicates": totals,
        "duplicate_bytes_per_case": distribution([side["duplicates"]["duplicate_bytes"] for side in answered]),
        "span_bytes": sum(side.get("span_bytes", 0) for side in answered),
        "delivered_span_bytes": sum(side.get("delivered_span_bytes", 0) for side in answered),
        "answer_elapsed_ms": distribution([side["elapsed_ns"] for side in answered], 1e6),
        "build_processes_seen_before_calls": sum(bool(side.get("build_processes_before")) for side in sides),
        "build_process_observations_missing": sum(side.get("build_processes_before") is None for side in sides),
    }


def aggregate(rows):
    """Both arms' figures and what changed between them."""
    report = {"cases": len(rows), "arms": {arm: arm_figures([row[arm] for row in rows]) for arm in ("A", "B")}}
    report["pairing_failures"] = {name: sum(row["pairing"].get(name) is False for row in rows)
                                  for name in sorted({key for row in rows for key in row["pairing"]})}
    both = [row for row in rows if row["A"]["call_state"] == row["B"]["call_state"] == "ok"]
    moves = [f"{row['A']['coverage']}->{row['B']['coverage']}" for row in rows]
    report["coverage_a_to_b"] = {move: moves.count(move) for move in sorted(set(moves))}

    def compared(left, right):
        return {"b_more": sum(b > a for a, b in zip(left, right)), "equal": sum(b == a for a, b in zip(left, right)),
                "b_fewer": sum(b < a for a, b in zip(left, right))}

    report["b_against_a"] = {
        "records": compared([row["A"]["records"] for row in both], [row["B"]["records"] for row in both]),
        "union_bytes": compared([row["A"]["duplicates"]["union_bytes"] for row in both],
                                [row["B"]["duplicates"]["union_bytes"] for row in both]),
        "delivered_span_bytes": compared([row["A"].get("delivered_span_bytes", 0) for row in both],
                                         [row["B"].get("delivered_span_bytes", 0) for row in both])}
    # What Focus did to the publisher document's rank in arm B's initial search.
    # Only a hit inside the scored prefix (rank 1 to 4 before Focus) can move.
    ranks = [(row["B"]["initial"]["gold_document_rank_before_focus"], row["B"]["initial"]["gold_document_rank"])
             for row in both if row["B"].get("initial")]
    present = [(before, after) for before, after in ranks if before is not None]
    report["gold_document_rank_movement"] = {
        "movable_rank_1_4_before_focus": sum(before <= 4 for before, _ in present),
        "improved": sum(after < before for before, after in present),
        "unchanged": sum(after == before for before, after in present),
        "worsened": sum(after > before for before, after in present),
        "first_before_focus": sum(before == 1 for before, _ in present),
        "first_after_focus": sum(after == 1 for _, after in present)}
    return report


def load(descriptor, index_id, request):
    """Authenticate one arm's retained command and Answer."""
    call = {"state": descriptor["state"], "elapsed_ns": descriptor.get("elapsed_ns")}
    if descriptor["state"] != "ok":
        return call, None
    command, envelope = pair.read(descriptor["command"]), pair.read(descriptor["stdout"])
    require(command["exit_code"] == 0 and command["elapsed_ns"] == descriptor["elapsed_ns"]
            and command["argv"][-7:] == ["tools", "call", "airk_answer", "--index", index_id,
                                         "--args-file", request["path"]]
            and envelope.get("isError") is not True, "retained Answer command differs from the sealed request")
    if "structuredContent" in envelope:
        return call, envelope["structuredContent"]
    content = envelope["content"]
    require(len(content) == 1 and content[0]["type"] == "text", "unexpected tool projection")
    return call, json.loads(content[0]["text"])


def score(run, freeze, capture):
    frozen = {row["case"]: row for row in freeze["cases"]}
    census = {row["case"]: row for row in pair.read(capture["focus_census"])["rows"]}
    rows = []
    for entry in capture["cases"]:
        case = frozen[entry["case"]]
        members = pair.read(case["observation"])["ordered_members"]
        by_path = {member["path"]: member for member in members}
        root, cache = Path(case["source_root"]), {}

        def body(name):
            if name not in cache:
                cache[name] = pair.bound_body(root, by_path[name])
            return cache[name]

        question = pair.read(case["request"])["question"]
        sides = {}
        for arm in ("A", "B"):
            descriptor = entry["arms"][arm]
            require(Path(descriptor["command"]["path"]).parent == run / "commands" / entry["case"]
                    if "command" in descriptor else descriptor["state"] == "timeout", "foreign arm receipt")
            call, packet = load(descriptor, entry["index_id"], case["request"])
            sides[arm] = dict(arm_row(call, packet, question, entry["publication_token"], members, case["target"], body),
                              request=case["request"]["sha256"],
                              build_processes_before=descriptor.get("build_processes_before"),
                              answer_sha256=descriptor.get("stdout", {}).get("sha256"))
        checks = pairing(sides["A"], sides["B"])
        expected = census.get(entry["case"], {}).get("expected_refusal")
        observed = sides["B"]["focus"][0]["state"] if sides["B"].get("focus") else None
        rows.append({"case": entry["case"], "group": case["group"], "exposed": case["exposed"], "A": sides["A"],
                     "B": sides["B"], "pairing": checks, "category": category(sides["A"], sides["B"], checks),
                     "census": {"expected_refusal": expected, "observed_initial_focus": observed}})
    return rows


def single_arm(run, freeze):
    """Arm A alone, for a run that closed before arm B was issued. It makes no paired claim.

    The run wrote no capture, so the index and publication come from each case's
    retained setup and probe search, and the compiler-process samples are absent.
    """
    frozen = {row["case"]: row for row in freeze["cases"]}
    rows = []
    for batch in freeze["plans"][arms.plan_name(run)]["batches"]:
        for name in batch["cases"]:
            case, folder = frozen[name], run / "commands" / name
            command, stdout = folder / "answer-a.command.json", folder / "answer-a.stdout"
            row = {"case": name, "group": case["group"], "exposed": case["exposed"]}
            if not command.is_file():
                state = "timeout" if stdout.is_file() else "not_issued"
                rows.append(dict(row, A={"call_state": state, "coverage": state, "elapsed_ns": None}))
                continue
            with (folder / "setup.stdout").open("rb") as stream:
                _, applied = map(json.loads, stream)
            probe = json.loads((folder / "pipeline.stdout").read_bytes())["structuredContent"]
            observed = json.loads(command.read_bytes())
            try:
                failed = json.loads(stdout.read_bytes()).get("isError") is True
            except (ValueError, AttributeError):
                failed = True
            state = "cli_error" if observed["exit_code"] != 0 else "tool_error" if failed else "ok"
            descriptor = {"state": state, "elapsed_ns": observed["elapsed_ns"],
                          "command": {"path": str(command), "sha256": pair.digest(command)},
                          "stdout": {"path": str(stdout), "sha256": pair.digest(stdout)}}
            members = pair.read(case["observation"])["ordered_members"]
            by_path = {member["path"]: member for member in members}
            root, cache = Path(case["source_root"]), {}

            def body(path, root=root, by_path=by_path, cache=cache):
                if path not in cache:
                    cache[path] = pair.bound_body(root, by_path[path])
                return cache[path]

            call, packet = load(descriptor, applied["receipt"]["index_id"], case["request"])
            side = arm_row(call, packet, pair.read(case["request"])["question"], probe["publication"]["token"],
                           members, case["target"], body)
            rows.append(dict(row, A=dict(side, request=case["request"]["sha256"], build_processes_before=None,
                                         answer_sha256=descriptor["stdout"]["sha256"])))
    return rows


def single_arm_report(rows, freeze):
    judged = [row["A"] for row in rows if not row["exposed"]]
    exposed = [row["A"] for row in rows if row["exposed"]]
    return {"arm": "A", "arm_b_issued": False, "paired_outcomes": 0,
            "statement": "Retrieval alone. Arm B was not issued, so this says nothing about what Focus adds.",
            "evidence_limits": freeze["evidence_limits"],
            "population": {"judged_cases": len(judged), "exposed_cases": [row["case"] for row in rows if row["exposed"]]},
            "judged_figures": arm_figures(judged) if judged else None,
            "exposed_figures": arm_figures(exposed) if exposed else None}


def report(rows, freeze):
    judged = [row for row in rows if not row["exposed"]]
    exposed = [row for row in rows if row["exposed"]]
    scored = {row["case"] for row in judged}
    touched = [group for group in freeze["groups"] if not group["exposed"] and scored & set(group["cases"])]
    # A group is reported only when every one of its cases was scored. Scoring a
    # single fixture lifetime leaves the groups that span lifetimes incomplete.
    groups = [group for group in touched if set(group["cases"]) <= scored]
    result = {"measurement_rule": freeze["measurement_rule"], "evidence_limits": freeze["evidence_limits"],
              "population": {"judged_cases": len(judged), "judged_groups": len(groups),
                             "incomplete_groups_not_reported": len(touched) - len(groups),
                             "exposed_cases": [row["case"] for row in exposed]}}
    if judged:
        complete = [row for row in judged if any(row["case"] in group["cases"] for group in groups)]
        result.update(by_case=tally(judged), judged_figures=aggregate(judged),
                      by_group=group_summary(complete, groups) if groups else None)
    if exposed:
        result["exposed"] = {"by_case": tally(exposed), **aggregate(exposed), "rows": [
            {"case": row["case"], "group": row["group"], "category": row["category"], "pairing": row["pairing"],
             **{arm: {key: row[arm].get(key) for key in ("coverage", "stop", "records", "omitted_count",
                                                         "span_bytes", "delivered_span_bytes", "elapsed_ns")}
                for arm in ("A", "B")}} for row in exposed]}
    agreement = {}
    for row in rows:
        key = f"expected_refusal={row['census']['expected_refusal']},observed={row['census']['observed_initial_focus']}"
        agreement[key] = agreement.get(key, 0) + 1
    result["focus_census_agreement"] = agreement
    return result


def pair_tokens(encode, recipe, view, query, text):
    """Token count of one Focus pair under the publisher recipe; the recipe refuses rather than truncates."""
    body = f"<Instruct>: {recipe['INSTRUCTION']}\n<Query>: {view(query)}\n<Document>: {view(text)}"
    return sum(len(encode(part)) for part in (recipe["PREFIX"], body, recipe["SUFFIX"]))


def census_row(encode, recipe, view, packet, limit, candidates):
    """Expected refusal when any of the first presented pairs exceeds the static width."""
    record = packet["records"][0]
    require(record["operation"] == "search", "first delivered record is not a search")
    hits = record["canonical_result"]["hits"][:candidates]
    counts = [pair_tokens(encode, recipe, view, record["arguments"]["query"], hit["text"]) for hit in hits]
    return {"state": "tokenised", "hit_bytes": [len(hit["text"].encode()) for hit in hits], "pair_tokens": counts,
            "expected_refusal": any(count > limit for count in counts)}


def focus_census(run, freeze):
    """Model-free. Reads only arm A's retained initial searches; runs no service call."""
    from tokenizers import Tokenizer, __version__
    import diagnose_techqa_focus_pair as replay
    runtime = freeze["runtime"]
    compiler = pair.read(runtime["focus_compiler"])
    tokenizer_path = Path(runtime["focus_tokenizer"]["path"])
    reference = arms.ROOT / "scripts/arbiter/export/qwen/qwen3-reranker06-experiment-r1/focus_reference.py"
    require(compiler["max_tokens"] == runtime["focus_max_tokens"] == 1024
            and compiler["tokenizer"] == runtime["focus_tokenizer"]["sha256"] == pair.digest(tokenizer_path)
            and pair.digest(reference) == freeze["sources"][reference.relative_to(arms.ROOT).as_posix()],
            "Focus compiler, tokenizer or publisher recipe differs from the freeze")
    recipe = runpy.run_path(str(reference))
    tokenizer = Tokenizer.from_file(str(tokenizer_path))
    tokenizer.no_padding()
    tokenizer.no_truncation()

    def encode(value):
        return tokenizer.encode(value, add_special_tokens=False).ids

    rows = []
    for batch in freeze["plans"][arms.plan_name(run)]["batches"]:
        for case in batch["cases"]:
            folder = run / "commands" / case
            row = {"case": case, "state": "no_arm_a_search", "expected_refusal": None}
            # A case without a usable arm A search stays listed; it never stops the run.
            try:
                envelope = json.loads((folder / "answer-a.stdout").read_bytes())
                require((folder / "answer-a.command.json").is_file() and envelope.get("isError") is not True,
                        "arm A produced no terminal Answer")
                row.update(census_row(encode, recipe, replay.model_view, envelope["structuredContent"],
                                      compiler["max_tokens"], runtime["focus_max_candidates"]),
                           answer_sha256=pair.digest(folder / "answer-a.stdout"))
            except (OSError, ValueError, KeyError, TypeError, IndexError) as error:
                row["reason"] = str(error)[:200]
            rows.append(row)
    return {"schema": "airk.private.techqa-stage-arms-focus-census/1", "tokenizers_version": __version__,
            "tokenizer_sha256": compiler["tokenizer"], "static_limit": compiler["max_tokens"],
            "max_candidates": runtime["focus_max_candidates"], "model_forwards": 0, "rows": rows,
            "limit": "Python publisher recipe over arm A's returned hit text, not the native compiler."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", type=Path, required=True, action="append",
                        help="a closed run; repeat it with its --capture-sha256 to score several fixture lifetimes")
    parser.add_argument("--freeze", type=Path, required=True)
    parser.add_argument("--freeze-sha256", required=True)
    parser.add_argument("--capture-sha256", action="append", default=[])
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--focus-census", action="store_true", help="model-free pair census of arm A; needs tokenizers")
    parser.add_argument("--arm-a-only", action="store_true",
                        help="score retrieval alone for a settled run that closed before arm B; no paired outcome")
    args = parser.parse_args()
    require(sys.flags.isolated and sys.dont_write_bytecode and (args.focus_census or sys.flags.no_site),
            "use checked Python -I -B -S (the census needs site packages: -I -B)")
    runs, output = args.run, args.output
    run, kind = runs[0], arms.plan_name(runs[0])
    require(len(set(runs)) == len(runs) and all(entry.resolve(strict=True) == entry for entry in runs)
            and output.is_absolute() and not output.exists() and output.parent in (*runs, arms.CAMPAIGN)
            and (len(runs) == 1 or not (args.focus_census or args.arm_a_only)),
            "closed private runs and a fresh private output required")
    freeze = pair.read({"path": str(args.freeze), "sha256": args.freeze_sha256})
    require(freeze["schema"] == arms.SCHEMA and freeze["sources"] == arms.seals()
            and freeze["measurement_rule"] == arms.RULE and freeze["measurement_rule"]["pass_line"] is None,
            "freeze, sealed helper or measurement rule differs")
    if args.focus_census:
        result = focus_census(run, freeze)
        summary = {"plan": kind, "cases": len(result["rows"]),
                   "tokenised": sum(row["state"] == "tokenised" for row in result["rows"]),
                   "expected_refusals": sum(row["expected_refusal"] is True for row in result["rows"]),
                   "max_pair_tokens": max((count for row in result["rows"] for count in row.get("pair_tokens", [])),
                                          default=None), "model_forwards": 0}
    elif args.arm_a_only:
        owned = json.loads((run / "owned.json").read_bytes())
        closed = json.loads((run / "fixture/closed.json").read_bytes())
        require(not (run / "capture.private.json").exists() and not args.capture_sha256
                and owned["freeze"] == {"path": str(args.freeze), "sha256": args.freeze_sha256}
                and owned["owned_descendants_settled"] is True and owned["source_seals_unchanged"] is True
                and owned["owned_job_forced_termination"] is owned["timed_out"] is False
                and owned["owned_job_final_member_pids"] == [] and owned["owned_job_final_active_processes"] == 0
                and closed["shutdown_completed"] is closed["service_lease_reacquired"] is True,
                "arm A alone needs a settled, naturally closed run of this freeze that wrote no capture")
        rows = single_arm(run, freeze)
        summary = single_arm_report(rows, freeze)
        summary.update(plan=kind, run_exit_code=owned["exit_code"], service=freeze["runtime"]["service"],
                       cli=freeze["runtime"]["cli"], image_scope=freeze["runtime"]["image_scope"],
                       freeze_sha256=args.freeze_sha256, owned_sha256=pair.digest(run / "owned.json"),
                       closed_sha256=pair.digest(run / "fixture/closed.json"), harness_sha256=arms.harness(),
                       harness_matches_freeze=arms.harness() == freeze["harness_at_freeze"],
                       release_qualified=False)
        result = {"schema": "airk.private.techqa-stage-arms-arm-a/1", "summary": summary, "rows": rows}
    else:
        require(len(args.capture_sha256) == len(runs), "each run needs its own pinned capture")
        rows, lifetimes = [], []
        for entry, pinned in zip(runs, args.capture_sha256):
            capture = pair.read({"path": str(entry / "capture.private.json"), "sha256": pinned})
            owned = json.loads((entry / "owned.json").read_bytes())
            require(capture["freeze"] == {"path": str(args.freeze), "sha256": args.freeze_sha256}
                    and capture["plan"] == arms.plan_name(entry) and owned["freeze"] == capture["freeze"]
                    and owned["exit_code"] == 0 and owned["owned_descendants_settled"] is True
                    and owned["source_seals_unchanged"] is True
                    and owned["owned_job_forced_termination"] is owned["timed_out"] is False
                    and owned["owned_job_final_member_pids"] == [] and owned["owned_job_final_active_processes"] == 0
                    and capture["service"] == freeze["runtime"]["service"] and capture["cli"] == freeze["runtime"]["cli"],
                    "run did not settle under this freeze")
            closed = pair.read(capture["closure"]["closed.json"])
            require(closed["shutdown_completed"] is closed["service_lease_reacquired"] is True
                    and closed["instance"] == capture["fixture_instance"], "fixture closure differs")
            rows += score(entry, freeze, capture)
            lifetimes.append({"run": str(entry), "plan": capture["plan"], "capture_sha256": pinned,
                              "owned_sha256": pair.digest(entry / "owned.json"),
                              "fixture_instance": capture["fixture_instance"],
                              "focus_dispatches": capture["focus_dispatches"]})
        require(len({row["case"] for row in rows}) == len(rows), "a case was scored in more than one run")
        summary = report(rows, freeze)
        summary.update(runs=lifetimes, service=freeze["runtime"]["service"], cli=freeze["runtime"]["cli"],
                       image_scope=freeze["runtime"]["image_scope"], freeze_sha256=args.freeze_sha256,
                       harness_sha256=arms.harness(),
                       harness_matches_freeze=arms.harness() == freeze["harness_at_freeze"],
                       focus_dispatches=sum(entry["focus_dispatches"] for entry in lifetimes),
                       outputs_judged=False, release_qualified=False)
        result = {"schema": "airk.private.techqa-stage-arms-score/1", "summary": summary, "rows": rows}
    with output.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(result, stream, indent=2, sort_keys=True, allow_nan=False)
        stream.write("\n")
    print(json.dumps(summary, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
