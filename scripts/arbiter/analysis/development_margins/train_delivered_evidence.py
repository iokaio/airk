# SPDX-License-Identifier: Apache-2.0
"""Measure returned publisher-span coverage on original Focus1024 TRAIN capture runs."""
import hashlib
import json
from pathlib import Path
import runpy


HERE = Path(__file__).resolve().parent
collect = runpy.run_path(str(HERE / "collect_focus1024_train.py"))
inputs = collect["inputs"]
sources = runpy.run_path(str(HERE / "census.py"))
returned_interval = runpy.run_path(str(HERE.parent / "public_source_admission" /
                                       "verify_techqa_delivered_evidence.py"))["returned_interval"]
covered = runpy.run_path(str(HERE.parents[3] / "tools/retrieval-train/goal_rubric.py"))["covered_location"]
require = inputs["require"]


def delivered(answer, publication, body, document, gold):
    intervals = []
    unresolved = bool(answer["paragraphs"])
    for record in answer["records"]:
        result = record["canonical_result"]
        if record["operation"] not in ("context", "search"):
            unresolved = True
            continue
        require(result["publication"] == publication, "delivered evidence publication differs")
        for hit in result["hits"]:
            citation = hit["citation"]
            if citation["path"] != gold["file"]:
                continue
            require(citation["content_hash"] == document["sha256"], "delivered source revision differs")
            extent = returned_interval(body, hit)
            if extent is None:
                unresolved = True
            else:
                intervals.append({"file": gold["file"], "byte_start": extent[0], "byte_end": extent[1]})
    if covered(gold, intervals):
        return "covered", intervals
    return ("unresolved" if unresolved else "incomplete"), intervals


def verify_delivery_scope(run, request_path, status, command, publication, answer):
    scope = status["structuredContent"]
    index, = scope["read_scope"]["index_ids"]
    require(not status["isError"] and command["exit_code"] == 0
            and command["argv"][1:] == ["--output", "json", "--service-runtime-dir", str(run / "fixture/control"),
                                       "tools", "call", "airk_answer", "--index", index, "--args-file", str(request_path)]
            and scope["publication"]["token"] == publication["token"]
            and answer["assessment"]["scope"]["publication"] == publication
            and answer["scope_observation"] == scope["scope_observation"]
            and answer["read_scope"] == scope["read_scope"],
            "delivered Answer command, publication or request scope differs")
    return index


def verify_answer(run, case, original, obs, answer):
    status, _ = collect["observed"](run / "commands/request-scope.stdout")
    command, _ = collect["observed"](run / "commands/answer.command.json")
    request_path = inputs["INPUTS"] / case / "request.private.json"
    request = inputs["load"](request_path, original["private_files"]["request.private.json"])
    index = verify_delivery_scope(run, request_path, status, command, obs["publication"], answer)
    binding = dict(index_id=index, publication=obs["publication"], scope_observation=answer["scope_observation"],
                   context=original["cpu_context_bootstrap_manifest"], focus=collect["focus"]["FOCUS"],
                   pipeline=original["required_minilm_pipeline"])
    admission_dir = run / "fixture/context-cal-admissions"
    if obs["state"] == "budget_stop_zero":
        classify = runpy.run_path(str(HERE / "zero_capture.py"))["zero_disposition"]
        require(classify(answer, request, binding, admission_dir) == obs["zero_disposition"],
                "zero-admission Answer differs from captured attempt")
        return
    compare = runpy.run_path(str(HERE.parent / "public_source_admission/run_techqa_predispatch_pilot.py"))["confirmed_census"]
    require(len(obs["admissions"]) == len(answer["assessment_history"]), "incomplete assessment history")
    for item in obs["admissions"]:
        native = inputs["load"](admission_dir / item["file"], item["sha256"])
        for field, value in compare(native, answer, request, binding).items():
            require(item[field] == value, "delivered Answer differs from captured native admission")


def measure(roster_row, source_rows, publisher):
    obs = collect["collect"](roster_row)
    case = obs["case"]
    row = {"case": case, "capture_state": obs["state"], "evidence": "unobserved"}
    if obs["state"] not in ("budget_stop_zero", "admitted"):
        return row
    original = inputs["validate_one"](case)
    run = Path(obs["run_directory"])
    require(run.parent == inputs["CAMPAIGN"] and Path(obs["capture_path"]) == run / "capture.private.json",
            "selected capture directory differs")
    envelope, answer_sha = collect["observed"](run / "commands/answer.stdout")
    answer = envelope["structuredContent"]
    require(not envelope.get("isError") and publisher["ANSWERABLE"] == "Y"
            and original["publisher_order"] == publisher["DOC_IDS"]
            and answer["question"] == publisher["QUESTION_TITLE"] + "\n\n" + publisher["QUESTION_TEXT"],
            "native question or publisher menu differs")
    verify_answer(run, case, original, obs, answer)
    document, = (d for d in source_rows if d["case"] == case and d["id"] == publisher["DOCUMENT"])
    filename = document["id"] + ".txt"
    require(document["path"] == case + "/" + filename, "publisher source path differs")
    body = (inputs["SOURCES"] / document["path"]).read_bytes()
    require(hashlib.sha256(body).hexdigest() == document["sha256"] and len(body) == document["bytes"],
            "publisher source bytes differ")
    text = body.decode("utf-8")
    left, right = int(publisher["START_OFFSET"]), int(publisher["END_OFFSET"])
    row.update(answer_sha256=answer_sha, owned_sha256=obs["owned_sha256"],
               capture_sha256=obs["capture_sha256"], publication=obs["publication"])
    if not 0 <= left < right <= len(text) or text[left:right] != publisher["ANSWER"]:
        return dict(row, evidence="unresolved_publisher_offsets")
    gold = {"file": filename, "byte_start": len(text[:left].encode()), "byte_end": len(text[:right].encode())}
    state, intervals = delivered(answer, obs["publication"], body, document, gold)
    return dict(row, evidence=state, expected=gold, returned_intervals=intervals,
                records=len(answer["records"]), assessments=len(answer["assessment_history"]))


def main():
    archive_sha = "0e9b77bc1ad7f7e95d05b095a34b1ece75c679575747949288a1d0f7470f4f46"
    train = inputs["load"](sources["ARCHIVE"] / "local-parse-r3/original-training_Q_A.json", archive_sha)
    publisher = {entry["QUESTION_ID"]: entry for entry in train}
    require(len(publisher) == len(train) == 600, "original TRAIN census differs")
    roster = inputs["load"](inputs["INPUTS"] / "roster.private.json", collect["ROSTER_SHA"])
    require(len(roster) == len({r["case"] for r in roster}) == 15, "development population differs")
    docs = inputs["load"](inputs["SOURCES"] / "manifest.private.json", inputs["SOURCES_SHA"])
    rows = [measure(r, docs["files"], publisher[r["case"]]) for r in roster]
    counts = {s: sum(r["evidence"] == s for r in rows) for s in sorted({r["evidence"] for r in rows})}
    print(json.dumps({"role": "TRAIN development, Focus1024, Context observed before forward",
                      "publisher_sha256": archive_sha, "source_menu_sha256": inputs["SOURCES_SHA"],
                      "roster_sha256": collect["ROSTER_SHA"], "population": len(rows), "counts": counts,
                      "paired_outcomes": 0, "rows": rows}, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
