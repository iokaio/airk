# SPDX-License-Identifier: Apache-2.0
"""Read-only CPU worker for the first independently verified Compact FP16 row."""

import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import sys
import time


ROOT = Path(__file__).resolve().parents[2]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
FP32_ROOT = CAMPAIGN / "fp32-expanded-reference-preflight-r1/compact-current-supplemental-reference"
PUBLISHER_ROOT = ROOT / "artifacts/advanced-split/models/qwen3-challenger-assets-r1"
MODEL = "arbiter-context-qwen3-17b"
ORIGINAL_GRAPH = "c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4"
FIXTURE_SHA = "c4360d8a759c68c305fba0df7bd04001a6b043d8a2a57b36f0d8ef0f898519b9"
PRODUCER_SHA = "b3849443d0d4d737d5c3f88b323b9eaffbd1045500d97f1487c56be639884bbe"
INPUT_DIGEST = "ec7b4845c37fb7cbe2a64a866f8b6cdc03479d65593cdb0a1d01102fe56fc335"
REFERENCE_OWNER = "scripts/arbiter-fp16-reference.py"
PUBLISHER_OWNER = "scripts/arbiter/export/qwen/qwen3-challengers-r1/qwen3_reference_r2.py"
REFERENCES = {
    REFERENCE_OWNER: "fec5e8b9ed661d91103f62977374bc5c42b4b1f70a433b1b13210e7bad857bd6",
    "scripts/arbiter-fp16-reference/graph.py": "698c779c16a6bc768d260669c1daf0840bc2512ea9d56e2df7660f43f6909f72",
    PUBLISHER_OWNER: "4c6f43d0d81539cfc6bf7b6ddf5ae69273e61508d0db3d22a5e7488c60560b6b",
}
MAX_WALL_SECONDS = 600
MAX_JSON_BYTES = 32 << 20
RESULT_MARKER = "AIRK_FP16_COMPACT_REFERENCE "


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_pinned_json(path, expected, check):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path, "canonical absolute JSON path required")
    require(not any(part.is_symlink() or part.is_junction() for part in (path, *path.parents)),
            "aliased JSON path refused")
    require(path.is_file(), "regular JSON file required")
    check()
    with path.open("rb") as stream:
        raw = stream.read(MAX_JSON_BYTES + 1)
    require(0 < len(raw) <= MAX_JSON_BYTES and hashlib.sha256(raw).hexdigest() == expected,
            "bounded reviewed JSON bytes differ")
    check()
    return json.loads(raw), {"path": str(path), "bytes": len(raw), "sha256": expected}


def read_only(event, arguments, existing_cache=None):
    if event == "os.mkdir" and existing_cache is not None:
        path, _, dir_fd = arguments
        if dir_fd in (-1, None) and Path(path) == existing_cache:
            require(existing_cache.is_dir() and existing_cache.resolve(strict=True) == existing_cache
                    and not existing_cache.is_symlink() and not existing_cache.is_junction(),
                    "audited Torch cache directory changed")
            return
    modifying = {
        "os.chdir", "os.chmod", "os.chown", "os.link", "os.mkdir", "os.remove",
        "os.rename", "os.rmdir", "os.symlink", "os.truncate", "os.utime",
        "os.exec", "os.posix_spawn", "os.spawn", "os.startfile", "os.startfile/2", "os.system",
        "socket.connect", "socket.bind", "subprocess.Popen", "tempfile.mkdtemp", "tempfile.mkstemp",
        "winreg.CreateKey", "winreg.DeleteKey", "winreg.DeleteValue", "winreg.SetValue",
    }
    if event == "open":
        _path, _mode, flags = arguments
        require(not flags & (os.O_WRONLY | os.O_RDWR | os.O_CREAT | os.O_TRUNC | os.O_APPEND),
                "independent reference worker file write refused")
    require(event not in modifying, "independent reference worker mutation, spawn or network refused")


def load_module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def loaded_runtime(files, pin, check, modules=None):
    observed = {}
    for module in list(sys.modules.values() if modules is None else modules):
        namespace = getattr(module, "__dict__", {})
        value = namespace.get("__file__")
        if not value:
            # Torch registers generated namespaces whose __getattr__ fabricates __file__.
            spec = namespace.get("__spec__")
            value = spec.origin if spec and spec.has_location else None
        if not value or value.startswith("<"):
            continue
        path = Path(value).resolve(strict=True)
        require(path.suffix != ".pyc", "cached reference module import refused")
        row = files.get(str(path))
        require(row is not None and pin(path, check) == row,
                "actual imported reference module outside sealed closure: " + str(path))
        observed[str(path)] = row["sha256"]
    return observed


def authenticated_conversion(args, check):
    receipt, receipt_pin = read_pinned_json(args.conversion_receipt, args.conversion_receipt_sha256, check)
    plan, plan_pin = read_pinned_json(args.conversion_plan, args.conversion_plan_sha256, check)
    wrapper = ROOT / "scripts/arbiter-fp16-execution.py"
    require(plan["wrapper"]["path"] == str(wrapper), "maintained conversion owner required")
    with wrapper.open("rb") as stream:
        wrapper_bytes = stream.read(1 << 20)
    require(len(wrapper_bytes) == plan["wrapper"]["bytes"]
            and hashlib.sha256(wrapper_bytes).hexdigest() == plan["wrapper"]["sha256"],
            "conversion owner bytes changed")
    conversion = load_module(wrapper, "compact_conversion_owner")
    require(plan["schema"] == conversion.SCHEMA and plan["model"] == "compact"
            and plan["repository_root"] == str(ROOT) and plan["limits"] == conversion.LIMITS
            and plan["human_sid"] == conversion.SID, "reviewed Compact conversion plan required")
    require(receipt["schema"] == "airk.private.fp16-conversion-execution/1"
            and receipt["complete"] is True and receipt["failure"] is None
            and receipt["qualification"] is receipt["activation"] is receipt["model_inference"] is False
            and receipt["whole_phase_expired"] is False
            and 0 <= receipt["elapsed_seconds"] < conversion.LIMITS["wall_seconds"]
            and receipt["plan"] == plan_pin and receipt["limits"] == plan["limits"],
            "completed unqualified Compact conversion required")
    proofs = receipt["final_proofs"]
    require(receipt["final_errors"] == [] and set(proofs) == set(conversion.PROOFS)
            and proofs["inputs"] == plan_pin
            and receipt["observed_proofs"] == {key: proofs[key] for key in proofs if key != "inputs"},
            "complete retained conversion proofs required")

    token = conversion.windows_owner()
    output = Path(plan["output_root"])
    require(output.is_absolute() and output.resolve(strict=True) == output
            and output.is_relative_to(ROOT / "artifacts") and args.conversion_receipt == output / "execution.json",
            "retained conversion output root differs")
    candidate = output / "candidate"
    for path in (output, candidate, args.conversion_receipt, args.conversion_plan):
        conversion.windows_owner(path)
    require(not output.is_symlink() and not output.is_junction() and not candidate.is_symlink()
            and not candidate.is_junction() and {path.name for path in candidate.iterdir()} == conversion.OUTPUT_NAMES,
            "candidate output directory or roster differs")
    names = {"graph": candidate / "model.onnx", "weights": candidate / "weights.bin",
             "manifest": candidate / "export-manifest.private.json",
             "worker": output / "worker.json", "stdout": output / "stdout.log"}
    for key, path in names.items():
        require(proofs[key]["path"] == str(path), "conversion proof escaped its retained leaf")
        conversion.windows_owner(path)
        conversion.pinned(proofs[key], check)

    owner_root = ROOT / "scripts/cuda-native-execution/helpers"
    for key in ("reference_parent_no_site_r2.py", "reference_worker.py"):
        row = plan["helpers"][key]
        require(row["path"] == str(owner_root / key) and row["sha256"] == conversion.HELPERS[key],
                "maintained conversion ownership helper differs")
        conversion.pinned(row, check)
    sys.path.insert(0, str(owner_root))
    parent = load_module(owner_root / "reference_parent_no_site_r2.py", "compact_conversion_parent")
    require(Path(parent.worker.__file__) == owner_root / "reference_worker.py",
            "conversion ownership dependency shadowed")
    owned = receipt["owned"]
    require(parent.settled(owned)
            and 0 < owned["peak_host_committed_bytes"] <= plan["limits"]["host_committed_bytes"],
            "actual conversion process or host settlement absent")
    suspended = owned["suspended_assignment"]
    require(suspended["pid"] == owned["owned_leader_pid"] and suspended["token"] == token
            and suspended["execution_user_sid"] == conversion.SID
            and suspended["image"] == {key: plan["python"][key] for key in ("path", "sha256")},
            "retained conversion suspended identity differs")
    expected_command = [
        plan["python"]["path"], "-B", "-S", "-X", "pycache_prefix=" + plan["cache_prefix"],
        str(wrapper), "--worker", "--plan", str(args.conversion_plan),
        "--plan-sha256", args.conversion_plan_sha256, "--deadline",
        repr(receipt["original_deadline_monotonic"]),
    ]
    require(receipt["command"] == expected_command, "retained conversion worker command differs")
    worker, worker_pin = read_pinned_json(names["worker"], proofs["worker"]["sha256"], check)
    require(worker_pin == proofs["worker"] and worker["complete"] is True
            and worker["model_inference"] is worker["qualification"] is False
            and worker["pid"] == owned["owned_leader_pid"] and worker["token"] == token,
            "retained conversion worker result differs")
    manifest, manifest_pin = read_pinned_json(names["manifest"], proofs["manifest"]["sha256"], check)
    require(manifest_pin == proofs["manifest"], "retained conversion manifest pin differs")
    conversion.validate_manifest(manifest, plan, proofs["graph"], proofs["weights"])
    return conversion, plan, plan_pin, receipt_pin, proofs, token


def exact_reference(check):
    fixture, fixture_pin = read_pinned_json(FP32_ROOT / "numeric-fixture.json", FIXTURE_SHA, check)
    producer, producer_pin = read_pinned_json(FP32_ROOT / "reference-producer.json", PRODUCER_SHA, check)
    require(fixture["schema"] == "airk.arbiter.native-numeric/2"
            and producer["schema"] == "airk.arbiter.independent-native-numeric-reference/2"
            and fixture["reference_producer"] == {key: producer_pin[key] for key in ("path", "sha256")}
            and fixture["model"] == producer["model"] == MODEL
            and fixture["compiler"] == producer["compiler"]
            and fixture["resource_policy"] == producer["resource_policy_digest"]
            and fixture["original_reference_assets"] == producer["original_reference_assets"],
            "retained independent original FP32 lineage differs")
    rows = [row for row in fixture["rows"] if row["name"] == "diagnostic-all-live-32"]
    require(len(rows) == 1, "exact retained T32 row required")
    row = rows[0]
    require(row["input"] == {
        "prompt": "", "input_ids": list(range(1, 33)), "attention_mask": [1] * 32,
        "position_ids": list(range(32)), "decision_position": 31,
        "candidate_positions": list(range(32, 40)), "option_mask": [True] * 8,
        "input_digest": INPUT_DIGEST,
    }, "retained T32/N8 native input differs")
    scores = row["reference_original_fp32"]
    require(len(scores) == 8 and all(type(x) in (int, float) and math.isfinite(x) for x in scores),
            "complete finite original FP32 scores required")
    proof = {"fixture": fixture_pin, "producer": producer_pin, "name": row["name"], "input": row["input"],
             "model": MODEL, "compiler": fixture["compiler"], "resource_policy": fixture["resource_policy"],
             "original_fp32": scores}
    return proof, producer["original_source_files"]


def verify(args):
    started = time.monotonic()
    prefix = sys.pycache_prefix
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode
            and prefix and Path(prefix).is_absolute() and not Path(prefix).exists()
            and os.environ.get("CUDA_VISIBLE_DEVICES") == "",
            "isolated Python, absent bytecode prefix and disabled CUDA required")
    require(math.isfinite(args.deadline_monotonic) and 0 < args.deadline_monotonic - started <= MAX_WALL_SECONDS,
            "original bounded reference deadline required")
    def check():
        if time.monotonic() >= args.deadline_monotonic:
            raise TimeoutError("original Compact reference deadline expired")

    cache = Path(os.environ.get("TORCHINDUCTOR_CACHE_DIR", ""))
    require(cache.is_absolute() and cache.is_relative_to(CAMPAIGN)
            and cache.resolve(strict=True) == cache and cache.is_dir()
            and not cache.is_symlink() and not cache.is_junction(),
            "existing audited Torch cache directory required")
    sys.addaudithook(lambda event, arguments: read_only(event, arguments, cache))
    conversion, plan, plan_pin, receipt_pin, proofs, token = authenticated_conversion(args, check)
    conversion.windows_owner(cache)
    require(conversion.pin(sys.executable, check) == plan["python"]
            and sys.pycache_prefix == plan["cache_prefix"] and not Path(sys.pycache_prefix).exists(),
            "actual pinned interpreter or absent bytecode prefix differs")
    original, selected = conversion.source_facts(plan["source_plan"], "compact", check)
    require(plan["source_graph"] == selected["original_graph"] and plan["source_graph"]["sha256"] == ORIGINAL_GRAPH
            and plan["source_closure"] == selected["source_closure"]
            and plan["expected_weights_bytes"] == selected["expected_weights_bin_bytes"]
            and plan["expected_attention_count"] == len(selected["expected_attention_nodes"]) == 28
            and plan["python"] == original["python"], "original Compact export identity differs")
    original_graph = Path(plan["source_graph"]["path"])
    original_files, closure_pin = read_pinned_json(plan["source_closure"]["path"], plan["source_closure"]["sha256"], check)
    require(closure_pin == plan["source_closure"], "original source closure length differs")

    reference, publisher_files = exact_reference(check)
    publisher_directory = PUBLISHER_ROOT / "candidate-assets/Qwen--Qwen3-1.7B"
    for name, row in publisher_files.items():
        require(Path(name).name == name and row["path"] == str(publisher_directory / name),
                "publisher source file escaped the retained roster")
        conversion.pinned(row, check)

    sources = {}
    for relative, expected in REFERENCES.items():
        path = ROOT / relative
        row = conversion.pin(path, check)
        require(row["sha256"] == expected, "maintained independent reference source changed")
        sources[str(path)] = row
    input_files = {row["path"]: row for row in plan["input_files"]}
    runtime, runtime_pin = read_pinned_json(plan["runtime_manifest"]["path"], plan["runtime_manifest"]["sha256"], check)
    require(runtime_pin == plan["runtime_manifest"] and input_files.get(runtime_pin["path"]) == runtime_pin,
            "runtime manifest omitted from the reviewed input census")
    python_home = Path(plan["python"]["path"]).parent
    expected_paths = [str(python_home / "Lib"), str(python_home / "DLLs"), original["python_site"]]
    require(runtime["roots"] == runtime["import_paths"] == expected_paths
            and all(path == row["path"] and input_files.get(path) == row for path, row in runtime["files"].items()),
            "reference dependency closure differs from the reviewed conversion runtime")
    sys.path.extend(runtime["import_paths"])
    import onnx
    import torch

    half = load_module(ROOT / REFERENCE_OWNER, "compact_independent_half")
    source_and_runtime = dict(input_files, **sources)
    script = conversion.pin(Path(__file__).resolve(), check)
    source_and_runtime[script["path"]] = script
    loaded_runtime(source_and_runtime, conversion.pin, check)

    torch.set_num_threads(2)
    torch.set_num_interop_threads(1)
    independent, publisher = half.from_authenticated_original(torch, "compact", ROOT / PUBLISHER_OWNER, PUBLISHER_ROOT, "cpu")
    check()
    candidate = {key: proofs[key] for key in ("graph", "weights")}
    graph = Path(candidate["graph"]["path"])
    weight = Path(candidate["weights"]["path"])
    verification = half.verify_actual_half_parameters(
        independent, graph, onnx, {weight.name: weight}, candidate_sha256=candidate["graph"]["sha256"],
        original_graph=original_graph, original_files=original_files,
    )
    check()
    scores = independent.forward(reference["input"])
    require(scores.dtype == torch.float32 and scores.device.type == "cpu" and tuple(scores.shape) == (1, 8)
            and not torch.cuda.is_initialized(), "one CPU-only FP32-ABI T32 reference vector required")
    reference["matching_fp16"] = scores[0].tolist()
    require(all(math.isfinite(item) for item in reference["matching_fp16"]), "nonfinite matching-half scores")

    for row in (plan_pin, receipt_pin, plan["source_graph"], closure_pin, *candidate.values(),
                *publisher_files.values(), *sources.values(), reference["fixture"], reference["producer"]):
        conversion.pinned(row, check)
    for name, row in original_files.items():
        conversion.pinned(dict(row, path=str(original_graph.parent / name)), check)
    loaded = loaded_runtime(source_and_runtime, conversion.pin, check)
    check()
    return {
        "schema": "airk.private.compact-fp16-independent-proof/1", "model": MODEL, "complete": True,
        "conversion_receipt": receipt_pin, "conversion_plan": plan_pin, "candidate": candidate,
        "original_graph": plan["source_graph"], "original_closure": closure_pin,
        "publisher": publisher, "publisher_files": publisher_files, "verification": verification,
        "reference": reference, "reference_sources": sources, "loaded_runtime": loaded,
        "execution_token": token, "pid": os.getpid(), "reference_device": "cpu",
        "original_deadline_monotonic": args.deadline_monotonic, "elapsed_seconds": time.monotonic() - started,
        "model_inference": True, "torch_forward_invocations": 1, "native_execution": False, "qualification": False,
    }


def run_owned(args):
    started = time.monotonic()
    deadline = started + MAX_WALL_SECONDS

    def check():
        if time.monotonic() >= deadline:
            raise TimeoutError("original Compact independent proof deadline expired")

    plan, plan_pin = read_pinned_json(args.conversion_plan, args.conversion_plan_sha256, check)
    prior, prior_pin = read_pinned_json(args.conversion_receipt, args.conversion_receipt_sha256, check)
    wrapper = ROOT / "scripts/arbiter-fp16-execution.py"
    require(str(wrapper) == plan["wrapper"]["path"], "maintained conversion owner required")
    check()
    raw = wrapper.read_bytes()
    require(len(raw) == plan["wrapper"]["bytes"] and hashlib.sha256(raw).hexdigest() == plan["wrapper"]["sha256"],
            "reviewed conversion wrapper differs")
    conversion = load_module(wrapper, "compact_reference_conversion_owner")
    token = conversion.windows_owner()
    require(plan["schema"] == conversion.SCHEMA and plan["model"] == "compact"
            and plan["repository_root"] == str(ROOT) and plan["human_sid"] == conversion.SID
            and prior["complete"] is True and prior["plan"] == plan_pin
            and conversion.pin(sys.executable, check) == plan["python"]
            and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "reviewed Compact conversion and actual isolated parent required")
    helper_root = ROOT / "scripts/cuda-native-execution/helpers"
    for key, expected in conversion.HELPERS.items():
        pin = plan["helpers"][key]
        require(pin["path"] == str(helper_root / key) and pin["sha256"] == expected,
                "maintained original ownership helper differs")
        conversion.pinned(pin, check)
    sys.path.insert(0, str(helper_root))
    parent = load_module(helper_root / "reference_parent_no_site_r2.py", "compact_reference_parent")
    owner = load_module(helper_root / "reference_owned.py", "compact_reference_owner")
    require(Path(parent.worker.__file__) == helper_root / "reference_worker.py",
            "original ownership dependency shadowed")
    out = args.owned_output
    require(out.is_absolute() and out.resolve(strict=False) == out
            and out.is_relative_to(CAMPAIGN) and out.parent.is_dir() and not out.exists(),
            "fresh checked Compact reference output required")
    conversion.guard(plan, out, deadline=deadline)
    out.mkdir()
    conversion.guard(plan, out, True, deadline=deadline)
    temporary = out / "temp"
    conversion.guard(plan, temporary, deadline=deadline)
    temporary.mkdir()
    conversion.guard(plan, temporary, True, deadline=deadline)
    stdin = out / "stdin.txt"
    stdout = out / "stdout-stderr.log"
    proof = out / "compact-independent-proof.json"
    receipt_path = out / "execution.json"
    for path in (stdin, stdout, proof, receipt_path):
        conversion.guard(plan, path, deadline=deadline)
    worker_pin = conversion.pin(Path(__file__).resolve(), check)
    argv = [plan["python"]["path"], "-I", "-B", "-S", "-X", "pycache_prefix=" + plan["cache_prefix"],
            worker_pin["path"], "--conversion-receipt", prior_pin["path"],
            "--conversion-receipt-sha256", prior_pin["sha256"], "--conversion-plan", plan_pin["path"],
            "--conversion-plan-sha256", plan_pin["sha256"], "--deadline-monotonic", repr(deadline)]
    environment = {key: value for key, value in os.environ.items()
                   if not key.upper().startswith(("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_", "TORCH_", "TRITON_",
                                                  "HF_", "HUGGINGFACE_"))}
    environment.update(CUDA_VISIBLE_DEVICES="", HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1",
                       TOKENIZERS_PARALLELISM="false", TORCH_HOME=str(temporary),
                       TORCHINDUCTOR_CACHE_DIR=str(temporary), TRITON_CACHE_DIR=str(temporary),
                       TEMP=str(temporary), TMP=str(temporary), TMPDIR=str(temporary))
    host_bytes = 32 << 30
    receipt = {"schema": "airk.private.owned-compact-fp16-independent-proof/1", "complete": False,
               "qualification": False, "failure": None, "original_deadline_monotonic": deadline,
               "host_bytes": host_bytes, "command": argv, "execution_token": token,
               "conversion_plan": plan_pin, "conversion_receipt": prior_pin, "worker": worker_pin}

    def suspended(api, job, info, child_deadline):
        observed = parent.suspended_authentication(api, info, deadline, child_deadline, conversion.SID,
                                                   plan["python"], owner.image_proof)
        require(conversion.windows_owner(process=info.process) == token, "suspended default owner differs")
        require(owner.job_snapshot(api, job) == ([int(info.pid)], 1, 1), "exact single suspended Job member required")
        return dict(observed, token=token)

    try:
        with stdin.open("xb"):
            pass
        conversion.guard(plan, stdin, True, deadline=deadline)
        with stdin.open("rb") as source, stdout.open("xb") as destination:
            conversion.guard(plan, stdout, True, deadline=deadline)
            receipt["owned"] = owner.run_windows_owned_training(
                argv, ROOT, destination, deadline - time.monotonic(), environment, source, host_bytes,
                before_resume=suspended, absolute_deadline=deadline, cleanup_seconds=5,
            )
        owned = receipt["owned"]
        require(parent.settled(owned) and type(owned.get("peak_host_committed_bytes")) is int
                and 0 < owned["peak_host_committed_bytes"] <= host_bytes,
                "reference worker did not pass within the original host bound, exit="
                + str(owned.get("exit_code")))
        conversion.guard(plan, stdout, True, deadline=deadline)
        require(0 < stdout.stat().st_size <= 64 << 20, "bounded reference stdout required")
        for path in temporary.rglob("*"):
            conversion.guard(plan, path, True, deadline=deadline)
        require(not any(temporary.iterdir()), "read-only reference worker created unexpected cache outputs")
        marker = RESULT_MARKER.encode("ascii")
        with stdout.open("rb") as stream:
            results = [line[len(marker):] for line in stream if line.startswith(marker)]
        require(len(results) == 1 and 0 < len(results[0]) <= 16 << 20,
                "exact bounded independent worker observation missing")
        observed = json.loads(results[0])
        require(observed["schema"] == "airk.private.compact-fp16-independent-proof/1"
                and observed["complete"] is observed["model_inference"] is True
                and observed["native_execution"] is observed["qualification"] is False
                and observed["conversion_plan"] == plan_pin and observed["conversion_receipt"] == prior_pin
                and observed["original_deadline_monotonic"] == deadline
                and observed["pid"] == owned["owned_leader_pid"] and observed["execution_token"] == token
                and observed["reference_device"] == "cpu" and observed["torch_forward_invocations"] == 1
                and observed["reference"]["input"]["input_digest"] == INPUT_DIGEST,
                "independent worker output differs from the owned request")
        check()
        with proof.open("xb") as stream:
            stream.write(results[0])
        conversion.guard(plan, proof, True, deadline=deadline)
        receipt["proof"] = conversion.pin(proof, check)
        receipt["stdout"] = conversion.pin(stdout, check)
        for pin in (plan_pin, prior_pin, worker_pin):
            conversion.pinned(pin, check)
        require({path.name for path in out.iterdir()} == {"temp", stdin.name, stdout.name, proof.name},
                "unexpected owned reference output")
        receipt["complete"] = True
    except BaseException as error:
        receipt["failure"] = {"type": type(error).__name__, "message": str(error)}
        if hasattr(error, "owned_cleanup"):
            receipt["owned_cleanup"] = error.owned_cleanup
    finally:
        prior_failure = receipt["failure"]
        if stdout.is_file() and "stdout" not in receipt and time.monotonic() < deadline:
            try:
                conversion.guard(plan, stdout, True, deadline=deadline)
                if 0 < stdout.stat().st_size <= 64 << 20:
                    receipt["stdout"] = conversion.pin(stdout, check)
            except (OSError, ValueError, TimeoutError) as error:
                receipt["stdout_retention_error"] = str(error)
        parent.final_phase_observation(receipt, deadline, started)
        receipt["failure"] = receipt["failure"] or prior_failure
        retention = time.monotonic() + 10
        conversion.guard(plan, receipt_path, deadline=retention)
        with receipt_path.open("x", encoding="utf-8") as stream:
            json.dump(receipt, stream, indent=2, allow_nan=False)
            stream.write("\n")
        conversion.guard(plan, receipt_path, True, deadline=retention)
    require(receipt["complete"], str(receipt["failure"]))
    return receipt["proof"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("conversion-receipt", "conversion-plan"):
        parser.add_argument("--" + name, type=Path, required=True)
        parser.add_argument("--" + name + "-sha256", required=True)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--deadline-monotonic", type=float)
    mode.add_argument("--owned-output", type=Path)
    args = parser.parse_args()
    if args.owned_output is not None:
        print(json.dumps({"proof": run_owned(args), "qualification": False}))
        return
    encoded = json.dumps(verify(args), separators=(",", ":"), allow_nan=False)
    require(len(encoded.encode("utf-8")) <= 16 << 20, "independent proof output exceeds its bound")
    require(time.monotonic() < args.deadline_monotonic, "original reference deadline expired before retention")
    print(RESULT_MARKER + encoded, flush=True)


if __name__ == "__main__":
    main()
