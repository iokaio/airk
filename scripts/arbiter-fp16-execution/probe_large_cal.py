# SPDX-License-Identifier: Apache-2.0
"""Score one original R8 CAL phase on the Windows Large direct scientific graph in one owned Job.

Phases 0-11 hold eight chronological requests and phase 12 holds four. The
launcher binds the closed R8 capture, the Large compiler projection and its
whole-population compiler receipt, the same-image operand and typed-graph
receipts and the static Cargo lineage before the native selector starts.
"""
import argparse
from collections import Counter
import ctypes
import json
import math
import os
from pathlib import Path
import re
import runpy
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
RAW = HERE.parent / "arbiter/evaluation/selected_goal/cal_raw"
QUALIFICATION = "inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::"
SELECTOR = QUALIFICATION + "fp16_large::cal::large_direct_original_cal_phase"
COMPILER_SELECTOR = QUALIFICATION + "fp16_compact::cal::chronological_original_context_compiler_batch"
SCHEMA = "airk.private.large-direct-cal-cuda/1"
OUTPUT_NAME = r"large-direct-cal-r8-full100-phase([0-9]{2})-r[1-9][0-9]*"

MODEL = "arbiter-context-qwen3-4b"
ORIGINAL = "d0e3c74813347b9bb4d3e17ffa4e337bbd16574ca80a81e4b1504c7e1546b307"
# The capture ran with Compact as the active Context profile; both compilers were censused per request.
CAPTURE_PROFILE = "6e7520aa29076f0958353a158e9be3390caef48675188e14e53660d035c1068f"
TYPED = "13da8267e7490e334b5a860ced90af8318a01f5c2d03759173feac45ca598b46"
GRAPH = "c1b5776144c6d9af07d08552cae40e491d8f866e89c53bab567adeeb32ca75e4"
WEIGHTS = "5b053f02795e72ff88123b28f89d3f2c89da055e9ed2228bc60809dd0a2b4270"
MANIFEST = "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5"
AUDIT = "aa2dea22544689e785426e03d4fa4f657cfda6d5dbc86b682a8594be7bb680e1"
SERVICE = "77e70341e1da84688c8b16bc992e25b3afed32d2d3e07639a6a5c69176eb0dfa"
PLAN = "52e2fb170c4074da74c4f08a04b02364c5dfde7dd612d0f2c964e0fbd157242c"
BATCH = "fac3dd246c91ede7fef576baea1e65ff2971e26f229159878e1993ac068fe6bf"
PROJECTION_SHA = "e1fd8a2b0f2f064a0c32ef361f8bd085c58120f7eaf5ab1758a80f9bea2ea845"
COMPILER_SHA = "e990a1b8346b5cd0fcd68ffd97b0c1eee776094641bd41ef864af547971690cb"
R7_COMPILER_SHA = "6987a9beaa1656b40c18b58bc1f320a6de112bfd6b69da893eb4f498eb014f8e"
COMPILER_SEAL_SHA = "e64c46617bc4a5ee83271a9adc4e85b4f391568fb79d2a43e118c01325ccb7ec"
STAGE_SHA = "c2ac1951a3c0b5817380c1c6164b25e4e2f3b679ba569b727a7ebbbbf1ea9a35"
PREP_SHA = "f9a45a949357610f6a7310b8df2b87ae07225382422fcc56f375fba00df8ccea"
PROBE = HERE / "probe_large_cutlass.py"
PROBE_SHA = "2c12b11cea8a4705fc85811ca51d1cda49d9a36193bba558da9605e900534c0e"
R7_CAL = HERE / "compact_cutlass_cal_r7.py"
R7_CAL_SHA = "5c538b49a6c4c7c3524ad33b515cf98d3805b1179cd7cab3ddd82845f837e327"

HOST = 22 << 30
SECONDS = 120
DEVICE_FREE = 44 << 30
LAYERS = 36
PHASES = 13
REQUESTS = 100
STATUSES = ("eligible", "no_native_assessment_request", "compiler_refused_remaining_allowance")

INFERENCE = "crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification"
CUTLASS = "crates/airk-retrieval/src/arbiter/runtime/cuda/diagnostic/cutlass"
NATIVE_KEYS = (CUTLASS + ".rs", CUTLASS + "/large.rs", CUTLASS + "/large_routes.rs", CUTLASS + "/model.rs",
               INFERENCE + "/fp16_large.rs", INFERENCE + "/fp16_large/compensated.rs")
SOURCE_KEYS = (INFERENCE + "/fp16_large/cal.rs", CUTLASS + "/large/cal.rs", INFERENCE + "/fp16_compact/cal.rs")
NATIVE_ENVIRONMENT = frozenset((
    "AIRK_PRIVATE_CENSUS_BASE", "AIRK_LARGE_CUTLASS_EXPIRES_NS", "AIRK_LARGE_CUTLASS_TEST_IMAGE_SHA",
    "AIRK_CUDA_CAL_PHASE", "AIRK_CUDA_CAL_LAUNCH_AUDIT", "AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256",
    "AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE", "AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE_SHA256",
    "ORT_CUDA_GEMM_OPTIONS", "ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO", "CUDA_CACHE_PATH"))
RESERVED = ("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_")


def require(ok, message):
    if not ok:
        raise ValueError(message)


def phase_span(phase):
    """First and one-past-last chronological ordinal of one original phase."""
    require(type(phase) is int and 0 <= phase < PHASES, "original R8 CAL phase required")
    return phase * 8, min(phase * 8 + 8, REQUESTS)


def command(image):
    return [str(image), "--exact", SELECTOR, "--ignored", "--nocapture"]


def environment(inherited, temp, image_sha, expires_ns, settings):
    """Exact native environment: no inherited AIRK/ORT/CUDA value and no pressure grant."""
    env = {key: value for key, value in inherited.items() if not key.upper().startswith(RESERVED)}
    env.update(PATH=r"C:\Windows\System32;C:\Windows", TEMP=str(temp), TMP=str(temp), TMPDIR=str(temp),
               AIRK_PRIVATE_CENSUS_BASE=str(temp), CUDA_CACHE_PATH=str(temp),
               ORT_CUDA_GEMM_OPTIONS="2", ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO="1",
               AIRK_LARGE_CUTLASS_EXPIRES_NS=str(expires_ns), AIRK_LARGE_CUTLASS_TEST_IMAGE_SHA=image_sha)
    env.update(settings)
    reserved = {key for key in env if key.upper().startswith(RESERVED)}
    require(reserved == NATIVE_ENVIRONMENT and type(expires_ns) is int and expires_ns > 0
            and re.fullmatch("[0-9a-f]{64}", image_sha) is not None
            and env["AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE_SHA256"] == BATCH
            and env["AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256"] == AUDIT
            and env["AIRK_CUDA_CAL_PHASE"] in {str(phase) for phase in range(PHASES)},
            "Large CAL native environment differs from the reviewed selector inputs")
    return env


def device_precondition(snapshot):
    """Refuse before the Job exists when the A6000 cannot give the selector its 44 GiB."""
    free = snapshot["nvidia_smi_mib"]["memory.free"]
    require(type(free) is int and free << 20 >= DEVICE_FREE,
            "A6000 reports less than 44 GiB free; a Large CAL phase needs the device to itself")
    return {"required_free_bytes": DEVICE_FREE, "observed_free_bytes": free << 20,
            "observation": "nvidia-smi memory.free before load"}


def scorer_sources(files):
    """The scorer image must have been built with the Large CAL selector and the readers it calls."""
    missing = [key for key in (*NATIVE_KEYS, *SOURCE_KEYS) if not re.fullmatch("[0-9a-f]{64}", str(files.get(key, "")))]
    require(not missing, "scorer image lacks Large CAL selector sources: " + ", ".join(missing))


def validate_population(batch, ids, plan_sha):
    rows, phases = batch["rows"], batch["phases"]
    require(batch["model"] == MODEL and batch["original_manifest"] == ORIGINAL
            and batch["phase_plan_sha256"] == plan_sha == PLAN and batch["launch_audit_sha256"] == AUDIT
            and batch["NN_invocations"] == 0 and batch["original_requests"] == len(rows) == REQUESTS
            and len(ids) == len(set(ids)) == REQUESTS and [row["id"] for row in rows] == ids
            and [row["ordinal"] for row in rows] == list(range(REQUESTS)) and len(phases) == PHASES
            and batch["statuses"] == dict(Counter(row["status"] for row in rows)),
            "wrong chronological R8 Large population")
    require(all(row["status"] in STATUSES and ("compiled" in row) == (row["status"] == "eligible")
                and len(row["captures"]) <= 1 and (row["status"] != "eligible" or len(row["captures"]) == 1)
                for row in rows), "invalid R8 request or refusal shapes")
    return rows, phases


def validate_capture(cap, row, published, focus, pipeline):
    require(cap["schema"] == "airk.private-selected-Context-native-admission-observation/1"
            and cap["Context_NN_dispatch"] is False and cap["Focus_configuration"] == "ON"
            and cap["actual_Context_profile"]["manifest"] == CAPTURE_PROFILE and cap["actual_Focus_profile"] == focus
            and cap["MiniLM_pipeline"] == pipeline and cap["request"]["scope"]["publication"] == published,
            "foreign R8 native admission profile or publication")
    if row["status"] == "eligible":
        req = cap["request"]
        census = [entry for entry in cap["compiler_census"] if entry["model"] == MODEL and entry["profile"] == ORIGINAL]
        require(len(census) == 1, "one original Large compiler census required")
        typed = census[0]
        require(row["remaining_ms"] == min(req["remaining_ms"], cap["remaining_ns_at_observation"] // 1_000_000)
                and row["remaining_input_tokens"] == req["remaining_input_tokens"] and row["roster"] == req["roster"]
                and typed["compiled"] == row["compiled"] and typed["fits_remaining_allowance"] is True
                and typed["NN_invocations"] == 0, "R8 native request, Large compiler budget or card roster changed")


def inputs(phase, campaign, read, exact, audit, text=None):
    """Bind one Large phase to the closed R8 capture, projection and whole-population compiler receipt.

    `read(path, limit)` returns bounded JSON, `exact(path, sha256)` returns a
    byte pin and `audit(paths)` runs the checked custody guard.
    """
    first, last = phase_span(phase)
    if text is None:
        text = lambda path: Path(path).read_text(encoding="utf-8")
    shared = runpy.run_path(str(R7_CAL))
    projection_root = campaign / "r8-chronological-full100-r2"
    compiler_root = campaign / "r8-chronological-native-full100-r1"
    capture = campaign / "cal100-focus1024-capture-r8"
    directory = capture / f"lifetime-{phase // 7:02}"
    refs = {}

    def keep(files):
        files = [(Path(path), sha) for path, sha in files]
        todo = list(dict.fromkeys(path for path, _ in files if path not in refs))
        if todo:
            audit(todo)
            wanted = dict(files)
            for path in todo:
                refs[path] = exact(path, wanted[path])
        for path, sha in files:
            require(refs[path]["sha256"] == sha, "contradictory R8 input pin")
        return [refs[path] for path, _ in files]

    def linked(value, path):
        pin, = keep([(path, value["sha256"])])
        require({"path", "sha256"} <= set(value) <= {"path", "bytes", "sha256"}
                and all(pin[key] == value[key] for key in value), "linked R8 input path or extent differs")
        return pin

    projection, compiler, old_compiler, seal = keep([
        (projection_root / "projection-receipt.json", PROJECTION_SHA),
        (compiler_root / "native.json", COMPILER_SHA),
        (campaign / "r7-chronological-native-first56-r1/native.json", R7_COMPILER_SHA),
        (campaign / "compact-cutlass-cal-r7-seal-r1/execution.json", COMPILER_SEAL_SHA)])
    saved, native, previous, sealed = (read(item["path"], 16 << 20) for item in (projection, compiler, old_compiler, seal))
    model = saved["models"]["large"]
    require(saved["schema"] == "airk.private.cal100-r8-chronological-projection/1"
            and saved["service_sha256"] == SERVICE and saved["original_requests"] == REQUESTS
            and len(saved["lifetimes"]) == 2 and len(saved["terminal"]) == 3
            and saved["NN_invocations"] == saved["gain_certificates"] == 0
            and saved["plan_sha256"] == saved["plan"]["sha256"] == PLAN
            and native["projection_sha256"] == PROJECTION_SHA
            and native["previous_compiler_sha256"] == R7_COMPILER_SHA
            and native["reference_preflight"] == saved["reference_preflight"]
            and native["launch_audit_sha256"] == saved["launch_audit"]["sha256"] == AUDIT
            and native["passed"] is True and native["NN_invocations"] == 0 and shared["settled_capture"](native["job"])
            and native["job"]["suspended_assignment"]["image"] == native["image"] == previous["image"]
            and native["command"] == [native["image"]["path"], "--exact", COMPILER_SELECTOR,
                                      "--ignored", "--nocapture", "--test-threads=1"]
            and model["sha256"] == BATCH
            and native["inputs"]["large"] == {key: model[key] for key in ("path", "sha256")},
            "foreign R8 evidence, Large batch or native compiler receipt")
    # The compiler ran the Cargo-emitted R7 Compact image. A later build reuses that file name,
    # so its bytes are authenticated through the static seal's retained staged copy.
    require(sealed["schema"] == "airk.private.compact-static-cargo-custody/1" and sealed["complete"] is True
            and sealed["native_invocations"] == 0 and sealed["build"] == previous["build"]
            and sealed["linked_image"]["path"] == native["image"]["path"]
            and sealed["linked_image"]["sha256"] == sealed["image"]["sha256"] == native["image"]["sha256"]
            and Path(sealed["image"]["path"]).parent == campaign / "windows-cu15-scientific-provider-stage-r1",
            "R8 compiler image is not the sealed R7 Compact image")
    keep([(projection_root / "reference-preflight.json", saved["reference_preflight"]["sha256"]),
          (campaign / "cal100-fused-candidate-launch-r1/cal100-launch-audit.json", AUDIT),
          (projection_root / "large.json", BATCH),
          (directory / "lifetime-closure.json", saved["lifetimes"][phase // 7]),
          (compiler_root / "native.log", native["log_sha256"]),
          (RAW / "run_compiler_batch.py", native["source_sha256"]),
          (RAW / "r8_chronological_inputs.py", saved["source_sha256"]),
          (Path(sealed["image"]["path"]), sealed["image"]["sha256"])])
    compiler_image = linked(sealed["image"], Path(sealed["image"]["path"]))
    preflight = linked(saved["reference_preflight"], projection_root / "reference-preflight.json")
    launch = linked(saved["launch_audit"], campaign / "cal100-fused-candidate-launch-r1/cal100-launch-audit.json")
    batch_pin = linked(model, projection_root / "large.json")
    lifetime, = keep([(directory / "lifetime-closure.json", saved["lifetimes"][phase // 7])])
    log, = keep([(compiler_root / "native.log", native["log_sha256"])])
    require(0 < log["bytes"] <= 1 << 20, "unbounded R8 compiler stdout")
    lines = text(log["path"]).splitlines()
    require(all(sum(line.endswith(summary) for line in lines) == 1 for summary in native["summary"].values())
            and any("1 passed; 0 failed;" in line for line in lines), "incomplete original R8 native compiler stdout")

    cert, active = read(preflight["path"], 128 << 20), read(launch["path"], 2 << 20)
    census_path = Path(active["census"]["path"])
    require(census_path.parent == campaign, "launch census is outside the campaign")
    ids = read(linked(active["census"], census_path)["path"], 2 << 20)["question_ids"]
    require(cert["complete"] is True and cert["lifetime_plan_sha256"] == saved["plan_sha256"]
            and cert["capture_sha256"] == saved["terminal"][1] and [row["id"] for row in cert["rows"]] == ids,
            "independent R8 certificate is incomplete")
    rows, phases = validate_population(read(batch_pin["path"], 128 << 20), ids, saved["plan_sha256"])
    eligible = sum(row["status"] == "eligible" for row in rows)
    require(native["summary"]["large"] ==
            f"ContextQwenLarge original_requests={REQUESTS} native_recompiled={eligible} retained_refusals={REQUESTS - eligible}",
            "original native compiler census disagrees with the Large population")
    selected = rows[first:last]
    require(any(row["status"] == "eligible" for row in selected), "phase has no eligible Large request")

    life = read(lifetime["path"], 16 << 20)
    require(life["schema"] == "airk.private.cal100-group-lifetime/1" and life["plan"] == saved["plan"]
            and life["group"] == phase // 7, "R8 phase has a different native owner")
    report_path = projection_root / f"phase-{phase:02}-independent-inputs.json"
    require(Path(phases[phase]["path"]) == report_path, "R8 original phase report path differs")
    report = read(linked(phases[phase], report_path)["path"], 32 << 20)
    shared["validate_phase"](report, phase, saved["plan_sha256"], lifetime, life, selected)

    owner_path, transcript = directory / f"phase-{phase:02}.owned.json", directory / f"phase-{phase:02}.jsonl"
    captures = []
    for row in selected:
        for reference in row["captures"]:
            name = Path(reference["path"]).name
            require(re.fullmatch(r"record-[0-9]{4}\.json", name) is not None, "foreign original capture file")
            captures.append((row, reference, directory / "fixture/context-cal-admissions" / name))
    keep([(owner_path, report["phase_owner"]["sha256"]), (transcript, report["source_transcript"]["sha256"]),
          *[(path, reference["sha256"]) for _, reference, path in captures]])
    owner = read(linked(report["phase_owner"], owner_path)["path"], 16 << 20)
    require(owner["phase"] == phase and owner["transcript"] == report["source_transcript"]
            and shared["settled_capture"](owner), "R8 original phase has not settled")
    linked(report["source_transcript"], transcript)
    for row, reference, path in captures:
        validate_capture(read(linked(reference, path)["path"], 8 << 20), row, life["publication"],
                         active["profiles"]["arbiter-focus-qwen3-reranker-06b"], active["pipeline"])
    settings = {
        "AIRK_CUDA_CAL_PHASE": str(phase),
        "AIRK_CUDA_CAL_LAUNCH_AUDIT": saved["launch_audit"]["path"],
        "AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256": AUDIT,
        "AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE": model["path"],
        "AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE_SHA256": model["sha256"],
    }
    facts = {"compiler_receipt": compiler, "compiler_image": compiler_image, "compiler_image_seal": seal,
             "rows_recompiled_by_compiler_image": eligible, "scorer_recompiles_each_scored_row": True}
    return list(refs.values()), selected, settings, facts


def journal_rows(events, selected, expiry, projection, pid, image):
    require(len(events) == 2 * len(selected), "missing or extra original Large CAL operation")
    previous = 0
    for n, (src, score) in enumerate(selected):
        start, stop = events[2 * n:2 * n + 2]
        for j, (event, stage) in enumerate(((start, "started"), (stop, "returned"))):
            require(event["schema"] == "airk.arbiter.native-operation/1" and event["phase"] == stage
                    and event["sequence"] == 2 * n + j + 1 and event["kind"] == "numeric"
                    and event["ordinal"] == 1 + src["ordinal"] % 8
                    and event["clock"] == "windows_performance_counter" and event["expires_monotonic_ns"] == expiry
                    and event["operation_input_digest"] == src["compiled"]["input_digest"]
                    and event["identity"] == {"input_sha256": projection, "profile": TYPED, "process_id": pid,
                                              "native_binary_sha256": image},
                    "operation ordinal, clock, source, graph or owned PID differs")
        require(previous <= start["started_monotonic_ns"] == start["observed_monotonic_ns"] == stop["started_monotonic_ns"]
                <= stop["observed_monotonic_ns"] < expiry and start["native_call_elapsed_us"] is None
                and start["native_success"] is None and stop["native_success"] is True
                and stop["native_call_elapsed_us"] == score["native_call_elapsed_us"] > 0
                and score["inference_us"] >= score["native_call_elapsed_us"]
                and score["compile_us"] + score["inference_us"] < src["remaining_ms"] * 1000,
                "unsynchronized, late or mismatched original observation")
        previous = stop["observed_monotonic_ns"]


def observation(raw, events, original, env, pid):
    """Check one settled selector's stdout and operation journal against the phase's original rows."""
    lines = raw.decode("utf-8").replace("\x1b[36m", "").replace("\x1b[0m", "").splitlines()
    marker = "AIRK_LARGE_COMPENSATED="
    matched = [line[len(marker):] for line in lines if line.startswith(marker)]
    require(len(matched) == 1 and any("1 passed; 0 failed" in line for line in lines),
            "one successful original Large CAL selector required")
    payload = json.loads(matched[0])
    job, native = payload["job"], payload["native"]
    result = native["result"]
    cal = result["cal"]
    seen = cal["observation"]
    require(job["process_memory_limit_bytes"] == job["job_memory_limit_bytes"] == HOST,
            "native process or Job limit is not 22 GiB")
    require(result["typed_graph_sha256"] == TYPED and result["model_sha256"] == GRAPH
            and result["weight_sha256"] == WEIGHTS and result["route_refusal"] is None and result["cpu_refusal"] is None
            and result["graph"] is None and result["score"] is None and result["timing"] is None
            and native["placement"]["complete"] is True and native["pressure"] is None
            and native["qualification"] is False and native["before"]["free_bytes"] >= DEVICE_FREE
            and seen["phase"] == int(env["AIRK_CUDA_CAL_PHASE"])
            and seen["qualification"] is False and seen["calibration_fitted"] is False,
            "Large graph, placement, device headroom or CAL role differs")
    for name, ref in (("AIRK_CUDA_CAL_LAUNCH_AUDIT", seen["launch_audit"]),
                      ("AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE", seen["original_projection"])):
        require(ref == {"path": env[name], "sha256": env[name + "_SHA256"]}, "native CAL source pin differs")
    require(len(seen["rows"]) == len(original), "native CAL phase has a different number of requests")
    selected = []
    for row, score in zip(original, seen["rows"], strict=True):
        require(score["ordinal"] == row["ordinal"] and score["id"] == row["id"], "changed native CAL chronology")
        if row["status"] != "eligible":
            require(score == {"ordinal": row["ordinal"], "id": row["id"], "status": row["status"]},
                    "scores invented for original refusal")
            continue
        require(score["status"] == "raw_scores_only"
                and all(row[key] == score[key] for key in ("compiled", "captures", "roster", "remaining_ms",
                                                            "remaining_input_tokens"))
                and score["input_digest"] == row["compiled"]["input_digest"]
                and len(row["compiled"]["input_ids"]) <= row["remaining_input_tokens"]
                and len(score["scores"]) == len(row["roster"]["cards"]) == len(row["compiled"]["option_mask"])
                and all(type(value) in (int, float) and math.isfinite(value) for value in score["scores"]),
                "changed input, incomplete vector or lost source budget")
        selected.append((row, score))
    require(cal["native_forwards"] == len(selected) > 0, "native forward count differs from the eligible requests")
    journal_rows(events, selected, int(env["AIRK_LARGE_CUTLASS_EXPIRES_NS"]),
                 env["AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE_SHA256"], pid, env["AIRK_LARGE_CUTLASS_TEST_IMAGE_SHA"])
    records = [line for line in lines if line.startswith("Operator=")]
    scratch = [line for line in lines if line.startswith("flash=")]
    expected = {f"Operator=MultiHeadAttention Node=/airk_large_cutlass/{layer}/efficient DataType=fp32 "
                "SdpaKernel=EFFICIENT_ATTENTION" for layer in range(LAYERS)}
    require(len(records) == len(scratch) == LAYERS * len(selected)
            and all(set(records[at:at + LAYERS]) == expected for at in range(0, len(records), LAYERS))
            and all("efficient=1" in line and "has_qkv_workspace=0, workspace=0" in line for line in scratch),
            "native Large phase lacks 36 efficient FP32 attention kernels per original forward")
    return payload, records, scratch


def run(args):
    started = time.monotonic()
    end = started + SECONDS
    shared = runpy.run_path(str(HERE / "probe_compact.py"))
    components = runpy.run_path(str(ROOT / "scripts/cuda-component-execution.py"))
    capacity = runpy.run_path(str(ROOT / "scripts/cuda-component-execution/wddm_capacity_snapshot.py"))
    wddm = runpy.run_path(str(ROOT / "scripts/cuda-component-execution/wddm_process_samples.py"))
    base = components["base"]()
    parent, owner = base.helpers({"ownership_helper_root": str(base.MAINTAINED_HELPER_ROOT)})
    exact = lambda path, digest: shared["exact_pin"](path, digest, base)
    require(os.name == "nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode
            and parent.worker.windows_sid() == base.SID == components["token_owner"](ctypes.c_void_p(-1)),
            "verified isolated human required")
    guard = {"checked_guard": base.pin(ROOT / "scripts/dev/assert-human-execution.ps1"),
             "powershell": exact(args.powershell, args.powershell_sha256)}

    def audit(paths, created=True, *, deadline=None):
        components["guard_many"](guard, list(paths), created, deadline=end if deadline is None else deadline)

    phase, out = args.cal_phase, args.output
    name = re.fullmatch(OUTPUT_NAME, out.name)
    require(out.is_absolute() and out.parent == CAMPAIGN and out.parent.resolve(strict=True) == out.parent
            and not out.exists() and name is not None and int(name.group(1)) == phase,
            "fresh private output named for this phase required")
    require((args.cal_projection_receipt_sha256, args.cal_compiler_sha256) == (PROJECTION_SHA, COMPILER_SHA),
            "original completed R8 projection and compiler required")
    manifest_path = args.image.with_name("airk-cuda-runtime.json")
    audit([args.build, args.image, args.snapshot, args.lineage, manifest_path, args.operand_receipt, args.graph_receipt])
    image, build = exact(args.image, args.image_sha256), exact(args.build, args.build_sha256)
    manifest = exact(manifest_path, MANIFEST)
    lineage = exact(args.lineage, args.lineage_sha256)
    chain = base.read(lineage["path"])
    require(chain["schema"] == "airk.private.compact-static-cargo-custody/1" and chain["complete"]
            and chain["native_invocations"] == 0 and chain["image"] == image and chain["build"] == build
            and chain["runtime_manifest"] == manifest and len(chain["compile_time_cuda_approvals"]) == 3,
            "wrong static linked-image chain")
    log, linked, sources = shared["linked_image"](build, image, base, audit, "metadata",
                                                   source_snapshot=args.snapshot, cargo_chain=chain)
    approvals = shared["decision_build_approvals"](log, linked, manifest, base, audit, cargo_chain=chain)
    files = base.read(build["path"], 32 << 20)["groups"]["native"]["files"]
    scorer_sources(files)
    retained = {key: args.snapshot / (files[key] + ".source") for key in (*NATIVE_KEYS, *SOURCE_KEYS)}
    audit(list(dict.fromkeys(retained.values())))
    for key, path in retained.items():
        sources[key] = exact(path, files[key])

    probe_pin = exact(PROBE, PROBE_SHA)
    large = runpy.run_path(probe_pin["path"])
    backend = runpy.run_path(str(HERE / "compact_cutlass_attention.py"))["backend_sources"](args.image.parent)
    backend = {key: backend[key] for key in ("producer", "provider_sources", "sm80_gemm_type", "internal_math")}
    pins = [backend["producer"], backend["sm80_gemm_type"], *backend["provider_sources"].values(), lineage,
            base.pin(Path(__file__).resolve()), probe_pin, exact(R7_CAL, R7_CAL_SHA),
            base.pin(HERE / "compact_cutlass_cal.py"), base.pin(HERE / "probe_compact.py"),
            base.pin(HERE / "large_direct_placement.py"), base.pin(HERE / "compact_cutlass_attention.py")]
    pins += [base.pin(path) for path in (
        ROOT / "scripts/cuda-component-execution.py", ROOT / "scripts/cuda-native-execution.py",
        ROOT / "scripts/cuda-component-execution/wddm_capacity_snapshot.py",
        ROOT / "scripts/cuda-component-execution/wddm_process_samples.py")]
    pins += [exact(base.MAINTAINED_HELPER_ROOT / leaf, sha) for leaf, sha in base.HELPERS.items()]
    pins.append(guard["checked_guard"])
    prep = exact(CAMPAIGN / "large-cutlass-t32-operands-r1/prepare.json", PREP_SHA)
    pins += [prep, *base.read(prep["path"])["assets"].values()]
    stage = exact(CAMPAIGN / "large-direct-fp32-stage-r2/stage.json", STAGE_SHA)
    arm = base.read(stage["path"])
    require(arm["graph"]["sha256"] == GRAPH and arm["weights"]["sha256"] == WEIGHTS,
            "direct stage differs from the selector's graph or half weights")
    pins += [stage, arm["graph"], arm["weights"]]
    control_pin = exact(args.operand_receipt, args.operand_sha256)
    control = base.read(control_pin["path"])
    require(control["mode"] == "operand" and control["complete"] and parent.settled(control["owned"])
            and control["build"] == build and control["image"] == image
            and control["result"]["result"]["values"] == 131072 and control["operand_matches"],
            "same-image genuine operand control required")
    graph_pin = exact(args.graph_receipt, args.graph_sha256)
    graph = base.read(graph_pin["path"], 64 << 20)
    measured = graph["result"]["result"]
    require(graph["mode"] == "graph" and graph["complete"] and parent.settled(graph["owned"])
            and graph["build"] == build and graph["image"] == image and graph.get("direct", False) is True
            and graph["typed_graph_validation"] == large["reviewed"](graph["result"], shared, True)
            and measured["typed_graph_sha256"] == TYPED and measured["model_sha256"] == GRAPH
            and measured["weight_sha256"] == WEIGHTS, "same-image measured Large direct graph required")
    pins += [control_pin, graph_pin]
    cal_pins, rows, settings, facts = inputs(phase, CAMPAIGN, base.read, exact, audit)
    pins += cal_pins
    unique = {}
    for pin in pins:
        require(pin == unique.setdefault(pin["path"], pin), "inconsistent custody for shared source")
    pins = list(unique.values())
    audit([Path(pin["path"]) for pin in pins])
    custody = [image, build, manifest, log, linked, guard["powershell"], *approvals, *sources.values(), *pins]
    for pin in pins:
        base.pinned(pin)
    if args.check_only:
        # Same custody as a run, without the device, the Job or any output.
        print(json.dumps({"check_only": True, "phase": phase, "ordinals": [row["ordinal"] for row in rows],
                          "eligible": sum(row["status"] == "eligible" for row in rows),
                          "custody_inputs": len(custody), "image_sha256": image["sha256"],
                          "typed_graph_sha256": TYPED, "custody_seconds": round(time.monotonic() - started, 3)}))
        return

    argv = command(args.image)
    receipt = {"schema": SCHEMA, "complete": False, "qualification": False, "calibration_fitted": False,
               "mode": "cal", "direct": True, "phase": phase, "command": argv, "image": image, "build": build,
               "linked_image": linked, "compile_time_cuda_approvals": approvals, "build_log": log,
               "runtime_manifest": manifest, "powershell": guard["powershell"], "sources": sources,
               "backend_sources": backend, "inputs": pins, "host_bytes": HOST, "seconds": SECONDS,
               "original_limits": {"seconds": SECONDS, "host_bytes": HOST, "device_free_bytes": DEVICE_FREE},
               "graph_receipt": graph_pin, "operand_receipt": control_pin, "compiler_custody": facts,
               "cal_inputs": settings, "result": None}
    components["mkdir"](guard, out, deadline=end)
    temp, stdin, stdout, dst = (out / leaf for leaf in ("temp", "stdin.txt", "stdout-stderr.log", "execution.json"))
    journal = temp / "protected.operations.jsonl"
    try:
        components["mkdir"](guard, temp, deadline=end)
        audit([stdin, stdout, dst, journal], False)
        env = environment(os.environ, temp, image["sha256"], int(end * 1_000_000_000), settings)
        begin = capacity["snapshot"](None)
        luid = begin["dxgi"]["luid"]
        receipt["before_load_adapter"] = begin
        receipt["device_precondition"] = device_precondition(begin)
        baseline = None

        def suspended(api, job, info, child_end):
            nonlocal baseline
            proof = parent.suspended_authentication(api, info, end, child_end, base.SID, image, owner.image_proof)
            require(components["token_owner"](info.process) == base.SID
                    and owner.job_snapshot(api, job) == ([info.pid], 1, 1), "wrong suspended owner or Job")
            with wddm["ProcessSampler"](info, luid, segment_fields=False) as sampler:
                baseline = sampler.sample()
            limits = shared["job_limits"](api, job, HOST)
            return dict(proof, default_owner_sid=base.SID, observed_limits=limits)

        def observing(api, job, info, expiry):
            return {"wddm": wddm["observe_process"](api, info, expiry, luid, baseline, segment_fields=False)}, []

        with stdin.open("xb"):
            pass
        with stdin.open("rb") as src, stdout.open("xb") as sink:
            audit([stdin, stdout])
            receipt["owned"] = owner.run_windows_owned_training(
                argv, ROOT, sink, end - time.monotonic(), env, src, HOST,
                before_resume=suspended, after_resume=observing, absolute_deadline=end, cleanup_seconds=5)
        require(parent.settled(receipt["owned"]), "native Large CAL lifetime did not settle successfully")
        audit([journal])
        events = [json.loads(line) for line in journal.read_bytes().splitlines()]
        payload, records, scratch = observation(stdout.read_bytes(), events, rows, env,
                                                receipt["owned"]["owned_leader_pid"])
        receipt.update(result=payload["native"], actual_job=payload["job"], kernel_records=records,
                       scratch_records=scratch, operations=base.pin(journal))
        receipt["after_release_adapter"] = capacity["snapshot"](None)
        require(receipt["after_release_adapter"]["dxgi"]["luid"] == luid, "physical adapter changed")
        for item in custody:
            base.pinned(item)
        receipt["complete"] = True
    except Exception as exc:
        receipt["error"] = type(exc).__name__ + ": " + str(exc)
    finally:
        if stdout.exists():
            receipt["original_stdout"] = base.pin(stdout)
        created = list(out.rglob("*"))
        if created:
            audit(created, deadline=time.monotonic() + 15)
        with dst.open("x", encoding="utf8") as file:
            json.dump(receipt, file, allow_nan=False)
            file.write("\n")
        audit([dst], deadline=time.monotonic() + 10)
    require(receipt["complete"], receipt.get("error", "invalid Large CAL receipt"))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cal-phase", type=int, choices=range(PHASES), required=True)
    parser.add_argument("--cal-projection-receipt-sha256", required=True)
    parser.add_argument("--cal-compiler-sha256", required=True)
    for field in ("image", "build", "powershell", "lineage", "operand-receipt", "graph-receipt"):
        parser.add_argument("--" + field, type=Path, required=True)
        parser.add_argument("--" + field.replace("-receipt", "") + "-sha256", required=True)
    for field in ("snapshot", "output"):
        parser.add_argument("--" + field, type=Path, required=True)
    parser.add_argument("--check-only", action="store_true",
                        help="verify every input and stop before the device, the Job and any output")
    run(parser.parse_args())
