# SPDX-License-Identifier: Apache-2.0
"""Owned opaque source/rubric projection; counts-only observer, no model/native work."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import types
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]

def sha(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()

def validate_decision(value, expected):
    if not (value.get("schema") == "airk.private-selected-Context-source-preparation-r3-decision/1"
            and value.get("approved") is True and value.get("protocol_sha256") == expected
            and value.get("opaque_source_rubric_only") is True and value.get("neural_execution") is False):
        raise ValueError("exact source-preparation release required")

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--protocol-sha256", required=True)
    parser.add_argument("--decision", type=Path, required=True)
    parser.add_argument("--decision-sha256", required=True)
    args = parser.parse_args()
    protocol = HERE / "protocol.proposed.json"
    receipt = HERE / "execution-r1.json"
    if receipt.exists() or sha(protocol) != args.protocol_sha256 or sha(args.decision) != args.decision_sha256:
        raise ValueError("fresh exact released launch required")
    p = json.loads(protocol.read_bytes())
    validate_decision(json.loads(args.decision.read_bytes()), args.protocol_sha256)
    seal_path = Path(p["runtime_seal"]["path"])
    if sha(seal_path) != p["runtime_seal"]["sha256"]:
        raise ValueError("runtime seal changed")
    seal = json.loads(seal_path.read_bytes())
    pins = p["inputs"] + seal["files"]
    before = {row["path"]: sha(row["path"]) for row in pins}
    if any(before[row["path"]] != row["sha256"] for row in pins):
        raise ValueError("input/source/runtime drift")
    prior = json.loads(Path(p["reuse_CAL_source"]["execution"]["path"]).read_bytes())
    first = prior["steps"][0]
    if not (prior.get("complete") is False and len(prior["steps"]) == 2
            and first.get("operation") == "materialize_sources" and first.get("partition") == "CAL"
            and first.get("strict_normal_close") is True and first.get("inputs_unchanged") is True
            and first.get("output_sha256") == p["reuse_CAL_source"]["manifest"]["sha256"]
            and first.get("question_scopes_sha256") == p["reuse_CAL_source"]["scopes"]["sha256"]
            and prior["steps"][1].get("operation") == "goal_rubric"
            and prior["steps"][1].get("inputs_unchanged") is True):
        raise ValueError("passed CAL source/retained failed rubric chain differs")
    failed_r2 = json.loads(Path(p["prior_failed_R2"]["path"]).read_bytes())
    if not (failed_r2.get("complete") is False and len(failed_r2["steps"]) == 1
            and failed_r2["steps"][0].get("partition") == "CAL"
            and failed_r2["steps"][0].get("operation") == "goal_rubric"
            and failed_r2["steps"][0].get("inputs_unchanged") is True
            and failed_r2["steps"][0]["owned_result"].get("owned_job_forced_termination") is False):
        raise ValueError("retained R2 failure chain changed")
    anchor = Path(p["output_anchor"])
    if not anchor.is_dir() or any(anchor.iterdir()):
        raise ValueError("fresh protected current-SID anchor required")
    import subprocess
    observed = subprocess.run(["pwsh", "-NoProfile", "-File", p["anchor_observer"]["path"], "-Anchor", str(anchor)], capture_output=True, text=True, timeout=10)
    if observed.returncode:
        raise ValueError("current-SID anchor admission refused")
    helper_path = Path(p["job_helper"]["path"])
    helper = types.ModuleType("selected_source_owned_job")
    helper.__file__ = str(helper_path)
    exec(compile(helper_path.read_bytes(), str(helper_path), "exec"), helper.__dict__)
    result = {"schema": "airk.private-selected-Context-source-preparation-r3-execution/1", "protocol_sha256": args.protocol_sha256,
              "decision_sha256": args.decision_sha256, "complete": False, "observer_parsed_question_gold_corpus": False,
              "neural_execution": False, "steps": [], "anchor_observation": observed.stdout}
    def save():
        receipt.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    save()
    try:
        for spec in p["steps"]:
            name = spec["partition"]
            operation = spec["operation"]
            corpus = p["corpora"][name]
            fixture = Path(p["reuse_CAL_source"]["manifest"]["path"]).parent if name == "CAL" else anchor / name / "source-fixture"
            output = anchor / name / ("source-fixture" if operation == "materialize_sources" else "rubric")
            argv = [seal["runtime_executable"], "-I", p["bounded_entrypoint"]["path"], "--operation", operation,
                    "--script-sha256", p["operations"][operation]["sha256"], "--", "--corpus", corpus["path"],
                    "--corpus-sha256", corpus["sha256"], "--partition", name, "--output", str(output)]
            if operation == "goal_rubric":
                argv += ["--fixture", str(fixture), "--source-manifest-sha256", sha(fixture / "manifest.private.json"), "--hotpot", p["raw_hotpot"]["path"]]
            log = HERE / (name + "-" + operation + "-r3.log")
            owned = helper.run_owned(argv, ROOT, log, 60)
            unchanged = all(sha(path) == digest for path, digest in before.items()) and sha(protocol) == args.protocol_sha256 and sha(args.decision) == args.decision_sha256
            settled = helper.successful(owned) and owned.get("owned_job_forced_termination") is False
            artifact = output / ("manifest.private.json" if operation == "materialize_sources" else "rubric.private.json")
            step = {"partition": name, "operation": operation, "command": argv, "owned_result": owned,
                    "strict_normal_close": settled, "inputs_unchanged": unchanged,
                    "output_path": str(artifact), "output_sha256": sha(artifact) if artifact.exists() else None}
            if operation == "materialize_sources":
                scopes = output / "question-scopes.private.json"
                step["question_scopes_path"] = str(scopes)
                step["question_scopes_sha256"] = sha(scopes) if scopes.exists() else None
                if not scopes.is_file():
                    settled = False
                    step["strict_normal_close"] = False
            result["steps"].append(step)
            save()
            if not settled or not unchanged or not artifact.is_file():
                raise ValueError("source preparation failed; retain failed run, no unchanged retry")
        result["complete"] = True
        result["strict_normal_close"] = True
        result["inputs_unchanged"] = True
        save()
    except BaseException as error:
        result["failure_class"] = type(error).__name__
        save()
        raise
    print(json.dumps({"complete": True, "operations": len(result["steps"]), "execution_sha256": sha(receipt), "question_gold_opened_by_observer": False, "NN_invocations": 0}))

if __name__ == "__main__":
    main()
