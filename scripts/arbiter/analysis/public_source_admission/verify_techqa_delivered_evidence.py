# SPDX-License-Identifier: Apache-2.0
"""Check literal human evidence coverage in the original, exposed TechQA pilot Answer."""
import hashlib
import json
from pathlib import Path
import runpy


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"


def pinned_json(path, digest):
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != digest:
        raise ValueError("source evidence changed")
    return json.loads(raw)


def returned_interval(body, hit):
    """Locate delivered bytes inside their cited lines. Repeated spans are unresolved."""
    citation = hit["citation"]
    first, last = citation["start_line"], citation["end_line"]
    lines = body.split(b"\n")
    if (type(first) is not int or type(last) is not int or not 1 <= first <= last <= len(lines)
            or citation["location"] != {"Lines": {"start": first, "end": last}}):
        raise ValueError("canonical line range changed")
    start = sum(len(line) + 1 for line in lines[:first - 1])
    limit = min(len(body), start + sum(len(line) + 1 for line in lines[first - 1:last]))
    window = body[start:limit]
    text = hit["text"].encode("utf-8")
    if not text:
        return None
    offset = window.find(text)
    if offset < 0 or window.find(text, offset + 1) >= 0:
        return None
    return start + offset, start + offset + len(text)


def main():
    roster = pinned_json(CAMPAIGN / "techqa-exact-gold-source-corpus-r1/manifest.private.json",
                         "a670e02fe6abdaab184a6950c33d74caf4ef4b972530a1aff300d68dd51b44c8")
    pilot = pinned_json(CAMPAIGN / "techqa-predispatch-pilot-inputs-r1/pilot.private.json",
                        "d1bc5ae7f75c9a0574eb8c4bba53dcbfa7001927e58c476fb6f4bdd8ed9aadab")
    full = pinned_json(CAMPAIGN / "techqa-predispatch-pilot-run-r2/commands/answer.stdout",
                       "9997a58787a9eed1e845b3d1941c499c63ad105549b0e80e377999cea9dbeaa0")["structuredContent"]
    census = pinned_json(CAMPAIGN / "techqa-predispatch-pilot-census-r1/census.private.json",
                         "196c0f35e6eb5444b4e18837197f2156583a05fcd42c20865260c365ae28d8b8")
    case, = (item for item in roster["cases"] if item["selector"]["question_id"] == pilot["question_id"])
    document, expected = case["document"], case["source_span"]
    path = document["path"]
    if (pilot["question_id"] != "DEV_Q002" or path != case["publisher_document_id"] + ".txt"
            or case["publisher_document_id"] not in pilot["publisher_order"] or full["paragraphs"]):
        raise ValueError("this pilot requires the frozen source and record-only Answer")
    body = (Path(pilot["source_root"]) / path).read_bytes()
    begin, end = expected["byte_start"], expected["byte_end_exclusive"]
    if (len(body) != document["bytes"] or hashlib.sha256(body).hexdigest() != document["sha256"]
            or not 0 <= begin < end <= len(body)
            or hashlib.sha256(body[begin:end]).hexdigest() != expected["span_sha256"]):
        raise ValueError("original gold range or source changed")
    intervals = []
    unknown = hits = 0
    for record in full["records"]:
        result = record["canonical_result"]
        if record["operation"] not in ("search", "context") or result["publication"] != census["source_publication"]:
            raise ValueError("record publication or tool changed")
        for hit in result["hits"]:
            hits += 1
            citation = hit["citation"]
            if citation["path"] != path:
                continue
            if citation["content_hash"] != document["sha256"]:
                raise ValueError("cited revision differs")
            extent = returned_interval(body, hit)
            if extent is None:
                unknown += 1
            else:
                intervals.append({"file": path, "byte_start": extent[0], "byte_end": extent[1]})
    rubric = runpy.run_path(str(ROOT / "tools/retrieval-train/goal_rubric.py"))
    target = {"file": path, "byte_start": begin, "byte_end": end}
    present = rubric["covered_location"](target, intervals)
    return {"case": pilot["question_id"], "records": len(full["records"]), "hit_count": hits,
            "gold": target, "delivered_gold_document_intervals": intervals, "unresolved_gold_hits": unknown,
            "publisher_span_covered": True if present else (None if unknown else False),
            "native_assessments": len(full["assessment_history"]), "paired_outcome": False,
            "exposure": "width-selection engineering case, original static-512 Focus",
            "source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}


if __name__ == "__main__":
    print(json.dumps(main(), sort_keys=True))
