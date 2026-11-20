# SPDX-License-Identifier: Apache-2.0
"""Freeze exposed Windows development Answer requests and corpora for matched warm timings."""
import hashlib
import json
from pathlib import Path
import runpy


HERE = Path(__file__).resolve().parent
E = HERE.parents[3] / "artifacts/v190/final-plan/exec-20261116T052942Z"
INPUTS = E / "compact-development-native-inputs-r1"
CORPUS = E / "compact-development-source-menus-r2"
ROSTER = "586f4d2d22c2cc4daef0c6b71bef5035f8165f9f8bde546a093d08d474049d67"
MENU = "39fe28355b0ea98e970794607db0f7d8052f0e484eae781006744fbfcc31cdf9"
EXPOSED = tuple("TRAIN_Q" + number for number in "103 111 117 202 227 260 263 379 425 454 524 541 543".split())


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sealed(path, expected=None):
    require(path.resolve(strict=True) == path and not any(p.is_junction() or p.is_symlink() for p in (path, *path.parents)),
            "noncanonical input path")
    raw = path.read_bytes()
    seal = hashlib.sha256(raw).hexdigest()
    require(expected is None or seal == expected, "changed source " + path.name)
    return raw, seal


def freeze():
    roster = json.loads(sealed(INPUTS / "roster.private.json", ROSTER)[0])
    documents = json.loads(sealed(CORPUS / "manifest.private.json", MENU)[0])["files"]
    require(tuple(row["case"] for row in roster[:len(EXPOSED)]) == EXPOSED, "original exposed order changed")
    selected = []
    policy = runpy.run_path(str(HERE / "focus1024.py"))
    for entry in roster[:len(EXPOSED)]:
        case = entry["case"]
        directory = INPUTS / case
        descriptor = json.loads(sealed(directory / "pilot.private.json", entry["descriptor_sha256"])[0])
        active = policy["bind"](descriptor, entry["descriptor_sha256"])
        source = CORPUS / case
        request_path = directory / "request.private.json"
        request_bytes, request_sha = sealed(request_path, descriptor["private_files"][request_path.name])
        request = json.loads(request_bytes)
        scopes, scope_sha = sealed(directory / "source-scopes.private.json", descriptor["private_files"]["source-scopes.private.json"])
        require(request["budget"] == {"input_tokens": 8192, "deadline_ms": 120000, "rounds": 3, "calls": 8,
                "candidates": 32, "fanout": 8, "max_bytes": 262144} and descriptor["source_root"] == str(source)
                and json.loads(scopes)["source_roots"] == [str(source)], "original request budgets or scope changed")
        members = {doc["id"]: doc for doc in documents if doc["case"] == case}
        ordered = descriptor["publisher_order"]
        require(len(members) == len(set(ordered)) == len(ordered) == 50 and set(ordered) == members.keys(),
                "incomplete isolated original source menu")
        ordered_pins = []
        for document in ordered:
            doc = members[document]
            path = CORPUS / doc["path"]
            require(path.parent == source, "source member escaped original case")
            data, seal = sealed(path, doc["sha256"])
            require(len(data) == doc["bytes"], "source byte count changed")
            ordered_pins.append({"file": path.name, "sha256": seal, "bytes": len(data)})
        require({p.name for p in source.iterdir()} == {doc["file"] for doc in ordered_pins}, "extra source members")
        attempt = 2 if case == "TRAIN_Q541" else 1
        run = E / f"compact-development-{case}-focus1024-run-r{attempt}"
        command, command_sha = sealed(run / "commands/answer.command.json")
        command = json.loads(command)
        require(command["argv"][-7:-3] == ["tools", "call", "airk_answer", "--index"]
                and command["argv"][-2:] == ["--args-file", str(request_path)] and command["exit_code"] == 0,
                "original request exposure cannot be established")
        selected.append({"case": case, "request": str(request_path), "request_sha256": request_sha,
                         "source_root": str(source), "source_scopes_sha256": scope_sha, "ordered_source_files": ordered_pins,
                         "prior_request_command_sha256": command_sha, "budget": request["budget"],
                         "focus_profile": active["cpu_focus_manifest"], "resource_policy": active["focus1024_resource_policy_digest"],
                         "pipeline": active["required_minilm_pipeline"]})
    return {"role": "exposed development timing preparation", "original_roster_sha256": ROSTER,
            "original_menu_sha256": MENU, "focus1024_launch_audit_sha256": policy["AUDIT_SHA"],
            "cases": selected, "warm_sample_order": list(EXPOSED) + list(EXPOSED[:7]),
            "cold_load_in_warm_denominator": False, "warm_sample_denominator_per_arm": 20,
            "warm_p95_nearest_rank": 19, "timeout_or_defer_remains_in_denominator": True,
            "timed_out_request_is_failure": True, "release_gate_with_comparable_cpu": {"fraction_saved": .25, "seconds_saved": 1},
            "large_without_comparable_cpu_max_seconds": 120, "actual_complete_answer_timing_samples": 0}


if __name__ == "__main__":
    print(json.dumps(freeze(), sort_keys=True, allow_nan=False))
