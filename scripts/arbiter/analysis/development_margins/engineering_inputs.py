# SPDX-License-Identifier: Apache-2.0
"""Bind the original eight-file synthetic engineering fixture for native re-observation."""
import argparse
import json
from pathlib import Path
import runpy


HERE = Path(__file__).resolve().parent
base = runpy.run_path(str(HERE / "capture_inputs.py"))
CAMPAIGN = base["CAMPAIGN"]
SOURCE = CAMPAIGN / "fp32-paired-answer-r1/shared-corpus"
OUTPUT = CAMPAIGN / "compact-development-engineering-inputs-r1"
CASE = "ENGINEERING_FIXED"
FIXTURE = "7c0a69a80cbc450bb48b8b573e84afeb6f1fe10c93cf687b08994fe75ed42736"
ANSWER = "76400d7ffa5fcaa52e5007af8c732af7a00fd8a59c8ae96637666b496684d23f"


def build():
    require, pin, load = (base[k] for k in ("require", "pin", "load"))
    fixture = load(SOURCE / "fixture.private.json", FIXTURE)
    request = load(SOURCE / "answer.json", ANSWER)
    tree = SOURCE / "sources"
    files = fixture["files"]
    require(fixture["schema"] == "airk.private-selected-synthetic-native-fixture/1"
            and fixture["human_semantic_gold_count"] == 0 and Path(fixture["source_root"]) == tree
            and len(files) == 8 and {f["path"] for f in files} == {p.name for p in tree.iterdir()},
            "synthetic engineering source population changed")
    for member in files:
        path = tree / member["path"]
        pin(path, member["sha256"])
        require(path.stat().st_size == member["bytes"], "synthetic engineering document size differs")
    shared, template = base["settings"]()
    require(template == request, "native shared request template differs from engineering fixture")
    scope = {"source_roots": [str(tree)]}
    raw = {"request.private.json": (SOURCE / "answer.json").read_bytes(),
           "source-scopes.private.json": base["encode"](scope)}
    descriptor = dict(shared, schema="airk.private.compact-engineering-native-inputs/1", question_id=CASE,
                      question_sha256=base["digest"](request["question"].encode()),
                      source_root=str(tree), synthetic_fixture_sha256=FIXTURE, human_semantic_gold_count=0,
                      publisher_order=[m["path"] for m in files], source_order_role="native fixture file order",
                      selection="original fixed engineering request, every native assessment ordinal and refusal",
                      private_files={name: base["digest"](data) for name, data in raw.items()})
    raw["pilot.private.json"] = base["encode"](descriptor)
    return descriptor, raw


def validate_one():
    descriptor, files = build()
    for name, raw in files.items():
        base["pin"](OUTPUT / name, base["digest"](raw))
    return descriptor


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-only", action="store_true")
    args = parser.parse_args()
    if args.check_only:
        descriptor = validate_one()
    else:
        base["require"](OUTPUT.resolve(strict=True) == OUTPUT and not any(OUTPUT.iterdir()),
                        "fresh checked engineering input directory required")
        descriptor, files = build()
        for name, raw in files.items():
            with (OUTPUT / name).open("xb") as out:
                out.write(raw)
    print(json.dumps({"case": CASE, "descriptor_sha256": base["digest"](base["encode"](descriptor)),
                      "query_sha256": descriptor["question_sha256"], "native_launched": False}))
