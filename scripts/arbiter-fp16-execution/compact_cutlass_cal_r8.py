# SPDX-License-Identifier: Apache-2.0
"""Admit one R8 Compact raw phase on the existing protected scientific image."""

from pathlib import Path
import re
import runpy

HERE = Path(__file__).resolve().parent
RAW = HERE.parent / "arbiter/evaluation/selected_goal/cal_raw"
SHARED = runpy.run_path(str(HERE / "compact_cutlass_cal_r7.py"))
SOURCE_KEYS = SHARED["SOURCE_KEYS"]
SELECTOR = SHARED["SELECTOR"]
observation = SHARED["observation"]
require = SHARED["require"]
AUDIT = SHARED["AUDIT"]
SERVICE = SHARED["SERVICE"]
PROJECTION_SHA = "e1fd8a2b0f2f064a0c32ef361f8bd085c58120f7eaf5ab1758a80f9bea2ea845"
COMPILER_SHA = "e990a1b8346b5cd0fcd68ffd97b0c1eee776094641bd41ef864af547971690cb"
R7_COMPILER_SHA = "6987a9beaa1656b40c18b58bc1f320a6de112bfd6b69da893eb4f498eb014f8e"


def inputs(args, campaign, base, exact, *, audit):
    phase = args.cal_phase
    require(type(phase) is int and 0 <= phase < 13, "original R8 CAL phase required")
    require((args.cal_projection_receipt_sha256, args.cal_compiler_sha256) == (PROJECTION_SHA, COMPILER_SHA),
            "original completed R8 projection and compiler required")
    SHARED["scorer_sources"](base.read(str(args.build), 32 << 20))
    projection_root = campaign / "r8-chronological-full100-r2"
    compiler_root = campaign / "r8-chronological-native-full100-r1"
    capture = campaign / "cal100-focus1024-capture-r8"
    pins = []

    def pin(path, sha):
        audit([path])
        result = exact(path, sha, base)
        pins.append(result)
        return result

    def linked(value, path):
        require(value == pin(path, value["sha256"]), "linked R8 input path or extent differs")
        return base.read(str(path), 128 << 20)

    projection = pin(projection_root / "projection-receipt.json", PROJECTION_SHA)
    compiler = pin(compiler_root / "native.json", COMPILER_SHA)
    old_compiler = pin(campaign / "r7-chronological-native-first56-r1/native.json", R7_COMPILER_SHA)
    saved, native, previous = [base.read(item["path"]) for item in (projection, compiler, old_compiler)]
    require(saved["schema"] == "airk.private.cal100-r8-chronological-projection/1"
            and saved["service_sha256"] == SERVICE and saved["original_requests"] == 100
            and len(saved["lifetimes"]) == 2 and len(saved["terminal"]) == 3
            and saved["NN_invocations"] == saved["gain_certificates"] == 0
            and native["projection_sha256"] == PROJECTION_SHA
            and native["previous_compiler_sha256"] == R7_COMPILER_SHA
            and native["reference_preflight"] == saved["reference_preflight"]
            and native["launch_audit_sha256"] == saved["launch_audit"]["sha256"] == AUDIT
            and native["image"] == previous["image"]
            and previous["build"]["sha256"] == args.build_sha256
            and native["image"]["sha256"] == args.image_sha256
            and native["passed"] is True and native["NN_invocations"] == 0 and SHARED["settled_capture"](native["job"]),
            "foreign R8 evidence or native compiler image")
    compiler_exe = pin(Path(native["image"]["path"]), native["image"]["sha256"])
    staged_exe = pin(args.image, args.image_sha256)
    graph = base.read(pin(args.graph_receipt, args.graph_sha256)["path"])
    require(graph["complete"] is True and graph["mode"] == "graph"
            and graph["image"] == staged_exe and graph["build"] == previous["build"]
            and compiler_exe["sha256"] == staged_exe["sha256"]
            and native["job"]["suspended_assignment"]["image"] == native["image"],
            "original compiler and staged scorer do not bind the exact same verified image")
    logs = pin(compiler_root / "native.log", native["log_sha256"])
    lines = Path(logs["path"]).read_text(encoding="utf-8").splitlines()
    require(all(sum(line.endswith(summary) for line in lines) == 1 for summary in native["summary"].values())
            and any("1 passed; 0 failed;" in line for line in lines), "incomplete original R8 native compiler stdout")
    pin(RAW / "run_compiler_batch.py", native["source_sha256"])
    pin(RAW / "r8_chronological_inputs.py", saved["source_sha256"])

    cert = linked(saved["reference_preflight"], projection_root / "reference-preflight.json")
    active = linked(saved["launch_audit"], campaign / "cal100-fused-candidate-launch-r1/cal100-launch-audit.json")
    ids = linked(active["census"], Path(active["census"]["path"]))["question_ids"]
    require(cert["complete"] is True and cert["lifetime_plan_sha256"] == saved["plan_sha256"]
            and cert["capture_sha256"] == saved["terminal"][1] and [row["id"] for row in cert["rows"]] == ids,
            "independent R8 certificate is incomplete")
    model = saved["models"]["compact"]
    require(native["inputs"]["compact"] == {key: model[key] for key in ("path", "sha256")},
            "R8 compiler did not verify this Compact batch")
    batch = linked(model, projection_root / "compact.json")
    rows, phases = SHARED["validate_population"](batch, ids, 100, saved["plan_sha256"])
    selected = rows[phase * 8:(phase + 1) * 8]

    group = phase // 7
    directory = capture / f"lifetime-{group:02}"
    lifetime = pin(directory / "lifetime-closure.json", saved["lifetimes"][group])
    life = base.read(lifetime["path"])
    require(life["plan"] == saved["plan"] and life["group"] == group,
            "R8 phase has a different native owner")
    report_ref = phases[phase]
    report_path = projection_root / f"phase-{phase:02}-independent-inputs.json"
    require(Path(report_ref["path"]) == report_path, "R8 original phase report path differs")
    report = base.read(pin(report_path, report_ref["sha256"])["path"])
    SHARED["validate_phase"](report, phase, saved["plan_sha256"], lifetime, life, selected)
    owner = linked(report["phase_owner"], directory / f"phase-{phase:02}.owned.json")
    require(owner["phase"] == phase and owner["transcript"] == report["source_transcript"]
            and SHARED["settled_capture"](owner), "R8 original phase has not settled")
    transcript = directory / f"phase-{phase:02}.jsonl"
    require(Path(owner["transcript"]["path"]) == transcript, "wrong original phase transcript")
    pin(transcript, owner["transcript"]["sha256"])
    for row in selected:
        for reference in row["captures"]:
            name = Path(reference["path"]).name
            require(re.fullmatch(r"record-[0-9]{4}.json", name), "foreign original capture file")
            event = linked(reference, directory / "fixture/context-cal-admissions" / name)
            SHARED["validate_capture"](event, row, life["publication"],
                                       active["profiles"]["arbiter-focus-qwen3-reranker-06b"], active["pipeline"])
    settings = {
        "AIRK_CUDA_CAL_PHASE": str(phase),
        "AIRK_CUDA_CAL_LAUNCH_AUDIT": saved["launch_audit"]["path"],
        "AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256": AUDIT,
        "AIRK_CUDA_CAL_CHRONOLOGICAL_COMPACT": model["path"],
        "AIRK_CUDA_CAL_CHRONOLOGICAL_COMPACT_SHA256": model["sha256"],
    }
    return pins, selected, settings
