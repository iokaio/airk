# SPDX-License-Identifier: Apache-2.0
"""Census all frozen Compact numeric rows and freeze a disjoint TRAIN development menu."""
import hashlib
import json
import math
from pathlib import Path


ROOT = Path(__file__).resolve().parents[4]
MODELS = ROOT / "artifacts/advanced-split/models"
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
ARCHIVE = MODELS / "actual-composed-service-r2/public-human-label-admission-r1/techqa-original-publisher-archive-r2"
ORIGINAL = MODELS / "weights-v2/software-v2-independent-numeric-r1/runs-native-r1/revision-r2/assembled-compact"
SUPPLEMENT = CAMPAIGN / "fp32-expanded-reference-preflight-r1/compact-current-supplemental-reference"
TARGETS = (32, 128, 1024, 8192)


def pinned(path, digest):
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != digest:
        raise ValueError(f"input identity differs for {path.name}")
    return json.loads(data)


def source_menus(train, fence):
    by_id = {row["original_TRAIN_id"]: row for row in fence["records"]}
    dev_ids = set(fence["entire_DEV_source_id_closure"])
    ids = [row["QUESTION_ID"] for row in train]
    if len(set(ids)) != len(ids) or len(by_id) != len(fence["records"]) or set(ids) != by_id.keys():
        raise ValueError("source/fence population differs")
    menus = []
    for row in train:
        key = row["QUESTION_ID"]
        evidence = by_id[key]
        menu = row["DOC_IDS"]
        answerable = row["ANSWERABLE"] == "Y"
        disjoint = set(menu).isdisjoint(dev_ids)
        if answerable != evidence["answerable"] or disjoint != evidence["whole_original_context_closure_disjoint"]:
            raise ValueError("source/fence conditions disagree")
        if answerable and disjoint:
            if not menu or len(set(menu)) != len(menu):
                raise ValueError("empty or duplicated menu")
            request = row["QUESTION_TITLE"] + "\n\n" + row["QUESTION_TEXT"]
            menus.append({"id": key, "request": request, "menu_ids": menu,
                          "request_sha256": hashlib.sha256(request.encode("utf-8")).hexdigest()})
    return menus


def margin(row):
    scores = row["reference_original_fp32"]
    if len(scores) < 2 or not all(math.isfinite(score) for score in scores):
        raise ValueError("at least two finite live scores required")
    if sum(row["input"]["option_mask"]) != len(scores):
        raise ValueError("live option count differs")
    ordered = sorted(scores, reverse=True)
    gap = ordered[0] - ordered[1]
    return {"name": row["name"], "tokens": len(row["input"]["input_ids"]),
            "digest": row["input"]["input_digest"], "original_fp32_margin": gap,
            "near_0020": gap <= 0.020, "near_0140": gap <= 0.140}


def main():
    pins = {}
    def load(path, sha):
        pins[str(path.relative_to(ROOT).as_posix())] = sha
        return pinned(path, sha)

    train = load(ARCHIVE / "local-parse-r3/original-training_Q_A.json",
                 "0e9b77bc1ad7f7e95d05b095a34b1ece75c679575747949288a1d0f7470f4f46")
    fence = load(ARCHIVE / "full-original-DEV-source-fence-r5.json",
                 "a0c1eeaf4302a7390d38494dfcde62950daeb239e2306cafdca53d3938473a1c")
    menus = source_menus(train, fence)
    if len(train) != 600 or len(menus) != 15 or any(len(item["menu_ids"]) != 50 for item in menus):
        raise ValueError("source population differs")
    rows = []
    for directory, sha in (
            (ORIGINAL, "3962186640b42648b345fa200516ed44a3de4ccab0d783e671c9c22c3b8b1bbd"),
            (SUPPLEMENT, "c4360d8a759c68c305fba0df7bd04001a6b043d8a2a57b36f0d8ef0f898519b9")):
        rows.extend(margin(row) for row in load(directory / "numeric-fixture.json", sha)["rows"])
    if sorted(r["tokens"] for r in rows) != [17, *TARGETS]:
        raise ValueError("numeric fixture widths differ")
    print(json.dumps({"schema": "airk.private.compact-development-margin-inputs/1",
                      "purpose": "development only", "input_sha256": pins,
                      "original_fp32_numeric_rows": rows, "train_source_menus": menus,
                      "unique_docs": len({doc for row in menus for doc in row["menu_ids"]}),
                      "native_source_menu_captures": 0, "paired_outcomes": 0,
                      "applicable_threshold_rule": None, "candidate_scores": None},
                     allow_nan=False, ensure_ascii=True))


if __name__ == "__main__":
    main()
