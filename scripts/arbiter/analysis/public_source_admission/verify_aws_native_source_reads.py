# SPDX-License-Identifier: Apache-2.0
"""Index the checked AWS corpus in a disposable native service and verify each cited range."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[4]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
CORPUS = CAMPAIGN / "aws-unique-source-corpus-r1"
MANIFEST = "ef2844ba2406de3b57ac1f5a6b13e582320bacae27779b704d38d4786375e882"
PLAN = "898f2134862baa90671be369b37cede7093cda54cb0f8766c617ebf62a8aaa53"
WRAPPER = ROOT / "scripts/arbiter/evaluation/run_compact_answer.py"
WRAPPER_SHA = "9625bb14ca85eaf5057b3b272d92eb99589fa13e47ea1eeef56d35b0fcb09496"
OWNER_SHA = "a1772271cfc3e585de903a7e80021b54f97da823a485e48621a60dcee564c044"
SECONDS = 240


def require(test, message):
    if not test:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def checked(path, expected):
    with path.open("rb") as stream:
        require(hashlib.file_digest(stream, "sha256").hexdigest() == expected, f"pin differs: {path.name}")


def verified_lines(case, body, response):
    span = case["source_span"]
    first, last = span["line_start"], span["line_end_inclusive"]
    # Preserve all source line terminators. The native reader may omit the
    # final LF when materializing a line range.
    lines = body.split(b"\n")
    raw = b"\n".join(lines[first - 1:last])
    text = raw.decode("utf-8")
    require(len(response["hits"]) == 1 and not response["truncated"], "incomplete native read")
    hit = response["hits"][0]
    citation = hit["citation"]
    require(not hit["truncated"] and citation["path"] == case["document"]["path"]
            and citation["content_hash"] == case["document"]["sha256"]
            and citation["location"] == {"Lines": {"start": first, "end": last}}
            and (citation["start_line"], citation["end_line"]) == (first, last), "native citation differs")
    require(hit["text"] in (text, text + "\n"), "returned native line bytes differ")
    offset = sum(len(line) + 1 for line in lines[:first - 1])
    span_bytes = hit["text"].encode()[span["byte_start"] - offset:span["byte_end_exclusive"] - offset]
    require(digest(span_bytes) == span["span_sha256"], "answer span absent from native text")
    return {"citation": citation, "span_sha256": digest(span_bytes), "native_text_sha256": digest(hit["text"].encode())}


def execute(w, owner, output):
    plan_path = CAMPAIGN / "fp32-paired-answer-r1/cpu-launch-manifest-bound.json"
    checked(plan_path, PLAN)
    plan = w["read_json"](plan_path)
    paths = {k: w["pin"](owner, plan[k]) for k in ("service", "cli", "inputs")}
    manifest_path = CORPUS / "manifest.private.json"
    checked(manifest_path, MANIFEST)
    cases = w["read_json"](manifest_path)["cases"]
    tree = CORPUS / "corpus"
    source = {c["document"]["path"]: c["document"] for c in cases}
    require({p.relative_to(tree).as_posix() for p in tree.rglob("*") if p.is_file()} == set(source),
            "indexed source inventory differs")
    for name, row in source.items():
        checked(owner.plain(tree / name, regular=True), row["sha256"])
    work = output / "fixture"
    runs = output / "commands"
    runs.mkdir()
    deadline = time.monotonic() + SECONDS
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith(("AIRK_", "RUST_LOG"))}
    env.update(AIRK_ARBITER_EVALUATION_INPUTS=str(paths["inputs"]),
               AIRK_ARBITER_EVALUATION_INPUTS_SHA256=plan["inputs"]["sha256"],
               AIRK_ARBITER_EVALUATION_WORK=str(work), AIRK_ARBITER_EVALUATION_SECONDS=str(SECONDS),
               AIRK_ARBITER_EVALUATION_RESOURCE_CONSENT="throughput64gib4threads", AIRK_MCP_RESPONSE_MODE="full")

    def command(label, *args):
        return w["run_logged"]([paths["cli"], "--output", "json", "--service-runtime-dir", work / "control", *args],
                               runs, label, env, deadline)[0]

    def cli(label, *args):
        return w["read_json"](command(label, *args))

    rows = []
    publication = None
    with (output / "service.log").open("xb") as log:
        with subprocess.Popen(w["fixture_command"](paths["service"], "cpu"), cwd=ROOT, env=env,
                              stdout=log, stderr=log, stdin=subprocess.DEVNULL,
                              creationflags=subprocess.CREATE_NO_WINDOW) as service:
            w["wait_observed"](output, lambda: True if (work / "ready.json").is_file() else None,
                               service, deadline, "private service readiness")
            ready = w["read_json"](work / "ready.json")
            require(ready["health"]["process_id"] == service.pid
                    and ready["health"]["executable_sha256"] == plan["service"]["sha256"]
                    and ready["inputs_sha256"] == plan["inputs"]["sha256"]
                    and (work / "control").samefile(ready["control_root"]), "private service identity differs")
            try:
                data = output / "index"
                data.mkdir()
                result = command("setup", "--root", tree, "--data-dir", data, "library", "setup",
                                 "--name", "AWS source visibility", "--watch", "off", "--apply")
                with result.open("rb") as stream:
                    draft, applied = map(json.loads, stream)
                require("draft" in draft, "missing setup draft")
                index = applied["receipt"]["index_id"]
                project = applied["receipt"]["request"]["registration"]["binding"]
                job = cli("index", "--service-project", project, "index")["job"]
                w["wait_job"](cli, job["id"], "index")
                for case in cases:
                    i = case["selector"]["row_index"]
                    span = case["source_span"]
                    path = case["document"]["path"]
                    args = dict(path=path, start_line=span["line_start"], end_line=span["line_end_inclusive"],
                                max_bytes=65536, response_mode="full", include_status=True)
                    response = w["tool_body"](cli(f"read-{i}", "tools", "call", "airk_read_file", "--index", index,
                                                  "--args", json.dumps(args)))
                    token = response["publication"]["token"]
                    require(token and publication in (None, token), "native publication changed")
                    publication = token
                    observation = verified_lines(case, (tree / path).read_bytes(), response)
                    rows.append({"selector": case["selector"], **observation})
            finally:
                if service.poll() is None:
                    command("shutdown", "service", "shutdown")
            require(service.wait(timeout=max(0, deadline - time.monotonic())) == 0, "fixture did not exit naturally")
    closed = w["read_json"](work / "closed.json")
    require(closed["instance"] == ready["health"]["instance"] and closed["shutdown_completed"]
            and closed["service_lease_reacquired"], "private fixture settlement differs")
    require(len(rows) == 41, "missing native span reads")
    w["write_json"](output / "native-reads.private.json", {
        "schema": "airk.private.aws-native-source-reads/1", "corpus_manifest_sha256": MANIFEST,
        "source_commit": "2d06ee9011f3192b2ff38c09f04e01f1ea9e0191", "fixture_plan_sha256": PLAN,
        "service_sha256": plan["service"]["sha256"], "cli_sha256": plan["cli"]["sha256"],
        "index_id": index, "publication": publication, "fixture_instance": closed["instance"],
        "cases": rows, "complete_candidate_menu_frozen": False, "answer_scored": False,
    })


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    checked(WRAPPER, WRAPPER_SHA)
    checked(ROOT / "scripts/provision-static-onnx-runtime.py", OWNER_SHA)
    w = runpy.run_path(str(WRAPPER))
    owner = w["owner_module"]()
    output = owner.plain(args.output)
    require(output.is_absolute() and output.parent == CAMPAIGN and output.name.startswith("aws-native-source-reads-"),
            "private campaign output required")
    if args.worker:
        require(os.environ.get("AIRK_AWS_READS_OWNED") == str(output), "owned worker required")
        with w["capped_worker_job"](output):
            execute(w, owner, output)
        return
    require(not output.exists(), "fresh run output required")
    output.mkdir()
    handler = Path(__file__).resolve()
    sealed = owner.digest(handler)
    result = owner.run_evaluation_fixture(
        [sys.executable, "-I", "-B", "-S", handler, "--output", output, "--worker"], ROOT, output / "outer.log",
        SECONDS, os.environ | {"AIRK_AWS_READS_OWNED": str(output)})
    result["source_sha256"] = sealed
    result["source_seals_unchanged"] = sealed == owner.digest(handler) and owner.digest(WRAPPER) == WRAPPER_SHA
    w["write_json"](output / "owned.json", result)
    require(result["exit_code"] == 0 and not result["timed_out"] and not result["log_oversized"]
            and result["child_assigned_before_resume"] and result["owned_descendants_settled"]
            and not result["owned_job_forced_termination"] and result["owned_job_final_active_processes"] == 0
            and result["owned_job_final_member_pids"] == [] and result["source_seals_unchanged"],
            "native run failed; inspect private output")
    print(owner.digest(output / "native-reads.private.json"))


if __name__ == "__main__":
    main()
