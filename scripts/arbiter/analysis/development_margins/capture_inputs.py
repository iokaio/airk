# SPDX-License-Identifier: Apache-2.0
"""Bind each TRAIN development request to its own source scope and native template."""
import argparse
import hashlib
import json
from pathlib import Path
import re


HERE = Path(__file__).resolve().parent
CAMPAIGN = HERE.parents[3] / "artifacts/v190/final-plan/exec-20261116T052942Z"
CENSUS = CAMPAIGN / "compact-development-margin-census-r1/inputs.private.json"
SOURCES = CAMPAIGN / "compact-development-source-menus-r2"
INPUTS = CAMPAIGN / "compact-development-native-inputs-r1"
PROFILE = CAMPAIGN / "techqa-predispatch-pilot-inputs-r1/pilot.private.json"
CENSUS_SHA = "eef3f7c896e5d147236402941ed56d7e4c118c1f4ea8f9477b8b585bf6d9c56c"
SOURCES_SHA = "39fe28355b0ea98e970794607db0f7d8052f0e484eae781006744fbfcc31cdf9"
PROFILE_SHA = "d1bc5ae7f75c9a0574eb8c4bba53dcbfa7001927e58c476fb6f4bdd8ed9aadab"
ROSTER_KEYS = ("runtime_plan_sha256", "runtime_inputs", "minilm_assets", "native_selector",
               "required_environment", "required_minilm_pipeline", "cpu_focus_manifest",
               "cpu_context_bootstrap_manifest")


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def encode(value):
    return (json.dumps(value, sort_keys=True, ensure_ascii=True, indent=2) + "\n").encode()


def pin(path, sha):
    require(path.resolve(strict=True) == path and path.is_file() and not path.is_junction(), "unexpected source path")
    with path.open("rb") as stream:
        require(hashlib.file_digest(stream, "sha256").hexdigest() == sha, "source pin differs: " + path.name)


def load(path, sha):
    pin(path, sha)
    data = path.read_bytes()
    require(digest(data) == sha, "input changed")
    return json.loads(data)


def settings():
    # Only the shared native runtime bindings are reused from the earlier
    # descriptor. Its question, source roots, menus and observations are excluded.
    original = load(PROFILE, PROFILE_SHA)
    shared = {key: original[key] for key in ROSTER_KEYS}
    assets = shared["runtime_inputs"]
    for info in assets.values():
        pin(Path(info["path"]), info["sha256"])
    for support in shared["minilm_assets"]:
        pin(Path(assets["minilm"]["path"]).parent / support["name"], support["sha256"])
    template = load(Path(assets["answer"]["path"]), assets["answer"]["sha256"])
    require(template["budget"]["deadline_ms"] == 120000 and template["budget"]["input_tokens"] == 8192
            and template["response_mode"] == "full" and template["include_status"] is True, "native template differs")
    return shared, template


def build(case, sources, shared, template):
    case_id = case["id"]
    require(re.fullmatch("TRAIN_Q[0-9]{3}", case_id), "TRAIN case identity required")
    tree = SOURCES / case_id
    rows = [row for row in sources["files"] if row["case"] == case_id]
    order = case["menu_ids"]
    require(len(order) == 50 and [row["id"] for row in rows] == order, "TRAIN menu order differs")
    require({p.name for p in tree.iterdir()} == {doc + ".txt" for doc in order}, "source scope differs")
    for row in rows:
        require(row["path"] == case_id + "/" + row["id"] + ".txt", "source relative path differs")
        path = SOURCES / row["path"]
        pin(path, row["sha256"])
        require(path.stat().st_size == row["bytes"], "source byte count differs")
    request = dict(template, question=case["request"])
    require(digest(request["question"].encode()) == case["request_sha256"], "TRAIN question differs")
    files = {"request.private.json": request, "source-scopes.private.json": {"source_roots": [str(tree)]}}
    descriptor = dict(shared, schema="airk.private.compact-development-native-inputs/1",
                      question_id=case_id, question_sha256=case["request_sha256"], source_root=str(tree),
                      publisher_order=order, full_menu_manifest_sha256=SOURCES_SHA, census_sha256=CENSUS_SHA,
                      runtime_bindings_source_sha256=PROFILE_SHA,
                      private_files={name: digest(encode(value)) for name, value in files.items()})
    return descriptor, files


def validate_one(case_id):
    corpus = load(CENSUS, CENSUS_SHA)
    sources = load(SOURCES / "manifest.private.json", SOURCES_SHA)
    cases = [case for case in corpus["train_source_menus"] if case["id"] == case_id]
    require(len(cases) == 1, "unknown development source case")
    descriptor, files = build(cases[0], sources, *settings())
    for name, value in dict(files, **{"pilot.private.json": descriptor}).items():
        pin(INPUTS / case_id / name, digest(encode(value)))
    return descriptor


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", help="verify one existing input without invoking the service")
    args = parser.parse_args()
    if args.case:
        descriptor = validate_one(args.case)
        print(json.dumps({"case": args.case, "descriptor_sha256": digest(encode(descriptor))}))
        return
    corpus = load(CENSUS, CENSUS_SHA)
    sources = load(SOURCES / "manifest.private.json", SOURCES_SHA)
    cases = corpus["train_source_menus"]
    require(INPUTS.resolve(strict=True) == INPUTS and {p.name for p in INPUTS.iterdir()} == {c["id"] for c in cases}
            and all(not any((INPUTS / case["id"]).iterdir()) for case in cases), "fresh prepared case directories required")
    shared, template = settings()
    roster = []
    for case in cases:
        descriptor, files = build(case, sources, shared, template)
        for name, value in dict(files, **{"pilot.private.json": descriptor}).items():
            with (INPUTS / case["id"] / name).open("xb") as dst:
                dst.write(encode(value))
        roster.append({"case": case["id"], "descriptor_sha256": digest(encode(descriptor))})
    with (INPUTS / "roster.private.json").open("xb") as dst:
        dst.write(encode(roster))
    print(json.dumps({"cases": len(roster), "roster_sha256": digest(encode(roster))}))


if __name__ == "__main__":
    main()
