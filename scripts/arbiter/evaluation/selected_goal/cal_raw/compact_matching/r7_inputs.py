# SPDX-License-Identifier: Apache-2.0
"""Read the sealed first 56 R7 Compact rows for independent CPU references."""
from pathlib import Path
import runpy
import sys

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent)]

from inputs import CAMPAIGN, SCIENCE, STAGE, STAGE_SHA, require
from r7_chronological_inputs import AUDIT, R7_ROOT, SCHEMA, SERVICE
from reference_inputs import checked, read, reference
from run_r7_compiler_batch import SELECTOR, parent, projection_inputs

PROJECTION = (CAMPAIGN/"r7-chronological-first56-r2/projection-receipt.json",
              "a28e95ac9934915add889ab83e8f091aa0a22885abc5275c0cf6798facdecb77")
COMPILER = (CAMPAIGN/"r7-chronological-native-first56-r1/native.json",
            "6987a9beaa1656b40c18b58bc1f320a6de112bfd6b69da893eb4f498eb014f8e")
GRAPH = (CAMPAIGN/"compact-cutlass-cal-r7-graph-r1/execution.json",
         "76aadef3d08f256e866cf760a152907bb324a4d43876426da33f00f8cebf948f")
IMAGE_SHA = "9e09f30b5cdb04c745361e51e92113dde704ccdf7babcf58aaaaf3de8dafbff3"
IDENTITY = {
    "model_sha256": "147be075a73ff81ca17b6c2fb965ec8f887e10908fbd33cf47a919d1694aa3d9",
    "weight_sha256": "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3",
    "typed_graph_sha256": "6142500349b0c9e72ef97e6c1f06ab5a81eb519942fe35d936a9d29020542581",
    "runtime_manifest_sha256": "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5",
    "matching_recipe_sha256": "7efd4fc72f11de95131c532467803a6a2c82c71cf84ac0bd23cabbcf3662e394",
}
SID = "S-1-5-21-3678370870-3858642417-826968105-1002"
FIRST_LIFETIME = R7_ROOT/"lifetime-00"


def canonical(pin, expected):
    path, sha = expected
    require(Path(pin["path"]) == path and pin["sha256"] == sha,
            "different sealed R7 first56 projection, compiler or graph")
    result = reference(path, sha)
    require(pin.get("bytes", result["bytes"]) == result["bytes"], "R7 receipt extent differs")
    return result


def core(pin):
    return {key: pin[key] for key in ("path", "sha256")}


def verify_jobs(native, graph, projection, expected):
    image = native["image"]
    native_assignment = native["job"]["suspended_assignment"]
    graph_assignment = graph["owned"]["suspended_assignment"]
    require(native["passed"] is True and native["NN_invocations"] == 0 and native["projection"] == projection
            and native["launch_audit_sha256"] == AUDIT and native["summary"] == expected
            and parent.settled(native["job"]) and native_assignment["pid"] == native["job"]["owned_leader_pid"]
            and native_assignment["image"] == image and native_assignment["execution_user_sid"] == SID
            and native["command"] == [image["path"], "--exact", SELECTOR, "--ignored", "--nocapture", "--test-threads=1"],
            "R7 compiler did not naturally recompile the exact projection")
    observed = graph["result"]
    require(graph["schema"] == "airk.private.compact-cutlass-owned/1" and graph["complete"] is True
            and graph["mode"] == "graph" and graph["build"] == native["build"] and graph["build_log"] == native["build_log"]
            and graph["image"]["sha256"] == image["sha256"] == IMAGE_SHA and core(graph["linked_image"]) == image
            and parent.settled(graph["owned"]) and graph_assignment["pid"] == graph["owned"]["owned_leader_pid"]
            and graph_assignment["image"] == core(graph["image"])
            and graph_assignment["execution_user_sid"] == graph_assignment["default_owner_sid"] == SID
            and graph["runtime_manifest"]["sha256"] == IDENTITY["runtime_manifest_sha256"]
            and all(observed[key] == IDENTITY[key] for key in ("model_sha256", "weight_sha256", "typed_graph_sha256"))
            and observed["placement"]["complete"] is True and observed["graph"]["complete"] is True
            and observed["score"] is None and observed["timing"] is None and graph["sources"] and graph["backend_sources"]
            and graph["typed_graph_validation"]["cuda_fp32_matmuls"] == 196
            and graph["typed_graph_validation"]["cuda_mha_fp32"] == 28,
            "R7 compiler and protected same-image typed graph differ")


def selected(projection_ref, compiler_ref, graph_ref, first, count):
    require(type(first) is int and type(count) is int and 0 <= first < 56 and 1 <= count <= 8
            and first + count <= 56, "one to eight consecutive rows from the sealed R7 first56 are required")
    require(Path(projection_ref["path"]).parent.name != "r7-chronological-full100-r1",
            "R7 full100 requires separate terminal-capture admission")
    projection_pin = canonical(projection_ref, PROJECTION)
    compiler_pin = canonical(compiler_ref, COMPILER)
    graph_pin = canonical(graph_ref, GRAPH)
    projection, native, graph = (read(pin) for pin in (projection_pin, compiler_pin, graph_pin))
    require(projection["schema"] == SCHEMA and projection["original_requests"] == 56
            and projection["NN_invocations"] == projection["gain_certificates"] == 0
            and projection["terminal"] is None and len(projection["lifetimes"]) == 1
            and projection["launch_audit"]["sha256"] == AUDIT and projection["service_sha256"] == SERVICE,
            "R7 first56 projection or completion basis differs")
    settings, expected = projection_inputs(projection_pin)
    verify_jobs(native, graph, projection_pin, expected)
    log = checked(reference(COMPILER[0].with_name("native.log"), native["log_sha256"]), limit=1 << 20)
    lines = log.decode("utf-8").splitlines()
    require(all(sum(line.endswith(summary) for line in lines) == 1 for summary in expected.values())
            and any("1 passed; 0 failed;" in line for line in lines), "R7 native compiler stdout differs")
    stage = read(reference(STAGE, STAGE_SHA), STAGE)
    require(stage["complete"] is True and stage["candidate_graph"]["sha256"] == IDENTITY["model_sha256"]
            and stage["candidate_weights"]["sha256"] == IDENTITY["weight_sha256"]
            and all(stage[key] in graph["model_inputs"] for key in ("candidate_graph", "candidate_weights")),
            "R7 same-image graph does not use the protected reference stage")
    checked(graph["runtime_manifest"])

    plan_sha = projection["plan_sha256"]
    plan = read(projection["plan"], R7_ROOT/"lifetime-plan.json")
    require(projection["plan"]["sha256"] == plan_sha and plan["input_audit"] == projection["launch_audit"]
            and plan["installation_mutex"]["sha256"] == projection["acquired_sha256"],
            "R7 predecessor plan or acquisition differs")
    life_pin = reference(FIRST_LIFETIME/"lifetime-closure.json", projection["lifetimes"][0])
    life = read(life_pin)
    closure = read(life["closed"], FIRST_LIFETIME/"fixture/closed.json")
    require(life["schema"] == "airk.private.cal100-group-lifetime/1" and life["group"] == 0
            and life["plan"] == projection["plan"] and life["phases"] == list(range(7))
            and life["end_ms"] <= plan["deadline_ms"] and closure["instance"] == life["instance"]
            and closure["shutdown_completed"] is True and closure["service_lease_reacquired"] is True,
            "R7 first native lifetime did not close naturally")

    audit = read(projection["launch_audit"])
    ids = read(audit["census"])["question_ids"]
    compact = projection["models"]["compact"]
    require(settings["AIRK_CUDA_CAL_CHRONOLOGICAL_COMPACT_SHA256"] == compact["sha256"],
            "R7 compiled Compact pin differs")
    batch = read(compact, PROJECTION[0].with_name("compact.json"))
    validator = runpy.run_path(str(SCIENCE/"compact_cutlass_cal_r7.py"))
    rows, phases = validator["validate_population"](batch, ids, 56, plan_sha)
    for phase in range(first // 8, (first + count - 1) // 8 + 1):
        phase_path = PROJECTION[0].with_name(f"phase-{phase:02}-independent-inputs.json")
        phase_pin = canonical(phases[phase], (phase_path, phases[phase]["sha256"]))
        report = read(phase_pin, phase_path)
        validator["validate_phase"](report, phase, plan_sha, life_pin, life, rows[phase*8:(phase+1)*8])
        checked(report["phase_owner"])
    chosen = rows[first:first+count]
    focus = audit["profiles"]["arbiter-focus-qwen3-reranker-06b"]
    for row in chosen:
        for item in row["captures"]:
            path = Path(item["path"])
            require(path.parent == FIRST_LIFETIME/"fixture/context-cal-admissions",
                    "R7 row has a foreign native observation")
            cap = read(item, path)
            validator["validate_capture"](cap, row, life["publication"], focus, audit["pipeline"])
    return chosen, {"mode": "r7_original", "projection": projection_pin,
                    "compiler": compiler_pin, "graph": graph_pin, "plan_sha256": plan_sha,
                    "launch_audit_sha256": AUDIT, **IDENTITY}
