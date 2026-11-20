# SPDX-License-Identifier: Apache-2.0
"""Authenticate the retained CAL-only rubric, exact source files and maintained evaluators."""

import ast
import hashlib
import json
from pathlib import Path
import runpy


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
AUDIT = "39e36a6fcc40f28412ff97ede00917b3bcd5147675c70510691b01e901607287"
CLOSURE = "3c80120a9fbefa6bce92761673c71bb5d8fce055acec22bd52d4037f847b6b84"
SOURCE = "603f4c64357dd4dfa1d51d1556c19ac17977d4b5ed0fcaa1d2aa44bca43274c7"
RUBRIC = "3fae6cdf245d9fa0b18f1c4d63e29c135cdfbc873543f08a72f611222022544d"
SELECTED_RUBRIC = "ba9e791f79d33f0631ffd4c5543ec52891b4fe922c2f108ef2828bf5a27e64dc"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


source_closure = HERE.parent / "closure.py"
require(digest(source_closure.read_bytes()) == CLOSURE, "selected-goal input reader changed")
closures = runpy.run_path(str(source_closure))
checked = closures["checked_bytes"]


def reference(path, sha):
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": sha}


def parsed(raw):
    return json.loads(raw, object_pairs_hook=closures["unique_pairs"])


def read(ref, path=None):
    if path is not None:
        require(Path(ref["path"]) == path, "linked path differs from intended input")
    return parsed(checked(ref))


def scoring_owners():
    tools = ROOT / "tools/retrieval-train"
    source_path = tools / "goal_sources.py"
    rubric_path = tools / "goal_rubric.py"
    checked(reference(source_path, SOURCE))
    live = checked(reference(rubric_path, RUBRIC)).decode()
    # The old rubric's build command differs. Inspect only its pinned source text.
    historic_path = ROOT / "artifacts/advanced-split/models/selected-native-goal-driver-r1/source/goal_rubric.py"
    previous = checked(reference(historic_path, SELECTED_RUBRIC)).decode()
    def scoring_bodies(text):
        tree = ast.parse(text)
        return {n.name: ast.dump(n, include_attributes=False) for n in tree.body
                if isinstance(n, ast.FunctionDef) and n.name in ("covered_location", "next_read_utility")}
    bodies = scoring_bodies(live)
    require(len(bodies) == 2 and bodies == scoring_bodies(previous), "selected human-reference formula differs")
    return runpy.run_path(str(source_path)), runpy.run_path(str(rubric_path))["next_read_utility"]


def load_inputs():
    audit = read(reference(CAMPAIGN / "cal100-launch-input-audit-r1.json", AUDIT))
    refs = audit["inputs"]
    corpus, rubric, manifest, scopes = (read(refs[n]) for n in ("corpus", "rubric", "manifest", "scopes"))
    ids = read(audit["census"])["question_ids"]
    require(len(ids) == len(set(ids)) == 100 and ids == manifest["calibration_ids"] and
            all(value["experiment_partition"] == "CAL" for value in (corpus, rubric, manifest, scopes)) and
            manifest["corpus_sha256"] == scopes["corpus_sha256"] == refs["corpus"]["sha256"], "CAL-only inputs differ")
    require([r["id"] for r in scopes["questions"]] == ids, "scope census differs")
    tasks = {v["id"]: v for v in rubric["tasks"]}
    orig = {v["id"]: v for v in corpus["tasks"]}
    require(len(tasks) == len(rubric["tasks"]) == len(orig) == len(corpus["tasks"]) == 100 and
            set(ids) == set(tasks) == set(orig), "foreign/missing/duplicate CAL case")
    for q in scopes["questions"]:
        task = tasks[q["id"]]
        original = orig[q["id"]]
        require(task["question"] == original["question"] == q["question"] and
                task["split"] == original["split"] == q["split"] == "calibration" and task["source_component"],
                "question or split identity differs")
    fixture = Path(refs["manifest"]["path"]).parent
    index_root = fixture / "sources/hotpot"
    require(scopes["source_roots"] == [str(index_root)] and len(manifest["files"]) == 753, "CAL source root differs")
    for row in manifest["files"]:
        path = fixture / row["file"]
        require(path.parent == index_root, "CAL source path outside root")
        checked({"path": str(path), "bytes": row["bytes"], "sha256": row["sha256"]})
    sources, utility = scoring_owners()
    metadata = sources["FrozenSourceMetadata"](fixture, refs["manifest"]["sha256"])
    def ledger():
        return sources["SourceLedger"](fixture, index_root, refs["manifest"]["sha256"], metadata_cache=metadata)
    policies = set()
    for descriptor in read(refs["inputs"]):
        path = Path(descriptor["profile"])
        profile = read(reference(path, descriptor["profile_sha256"]))
        require(profile["manifest"] == audit["profiles"][profile["model"]], "bootstrap profile differs")
        if "airk.context_action.v1" in profile["task_policies"]:
            policies.add(profile["task_policies"]["airk.context_action.v1"])
    require(len(policies) == 1, "one exact native Context task policy required")
    return audit, ids, tasks, ledger, utility, policies.pop(), sources, index_root
