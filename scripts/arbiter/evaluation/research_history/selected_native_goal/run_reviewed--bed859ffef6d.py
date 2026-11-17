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
    if not (value.get("schema") == "airk.private-selected-Context-source-preparation-decision/1"
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
    result = {"schema": "airk.private-selected-Context-source-preparation-execution/1", "protocol_sha256": args.protocol_sha256,
              "decision_sha256": args.decision_sha256, "complete": False, "observer_parsed_question_gold_corpus": False,
              "neural_execution": False, "steps": [], "anchor_observation": observed.stdout}
    def save():
        receipt.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    save()
    try:
        for partition in p["partitions"]:
            corpus = partition["corpus"]
            base = anchor / partition["name"]
            fixture = base / "source-fixture"
            rubric = base / "rubric"
            for operation in ("materialize_sources", "goal_rubric"):
                argv = [seal["runtime_executable"], "-I", p["bounded_entrypoint"]["path"], "--operation", operation,
                        "--script-sha256", p["operations"][operation]["sha256"], "--", "--corpus", corpus["path"],
                        "--corpus-sha256", corpus["sha256"], "--partition", partition["name"], "--output", str(fixture if operation == "materialize_sources" else rubric)]
                if operation == "goal_rubric":
                    argv += ["--fixture", str(fixture), "--source-manifest-sha256", sha(fixture / "manifest.private.json"), "--hotpot", p["raw_hotpot"]["path"]]
                log = HERE / (partition["name"] + "-" + operation + "-r1.log")
                owned = helper.run_owned(argv, ROOT, log, 60)
                unchanged = all(sha(path) == digest for path, digest in before.items()) and sha(protocol) == args.protocol_sha256 and sha(args.decision) == args.decision_sha256
                settled = helper.successful(owned) and owned.get("owned_job_forced_termination") is False
                output = fixture / "manifest.private.json" if operation == "materialize_sources" else rubric / "rubric.private.json"
                step = {"partition": partition["name"], "operation": operation, "command": argv, "owned_result": owned,
                        "strict_normal_close": settled, "inputs_unchanged": unchanged,
                        "output_path": str(output), "output_sha256": sha(output) if output.exists() else None}
                if operation == "materialize_sources":
                    scopes = fixture / "question-scopes.private.json"
                    step["question_scopes_path"] = str(scopes)
                    step["question_scopes_sha256"] = sha(scopes) if scopes.exists() else None
                    if not scopes.is_file():
                        settled = False
                        step["strict_normal_close"] = False
                result["steps"].append(step)
                save()
                if not settled or not unchanged or not output.is_file():
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
