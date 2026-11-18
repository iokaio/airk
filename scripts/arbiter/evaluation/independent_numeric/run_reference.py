# SPDX-License-Identifier: Apache-2.0
"""Run one independently sealed Torch reference in an owned Windows Job."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import time

from maintained_helpers import HELPER_ROOT, REPOSITORY, parent, private_root, worker


def checked_output_guard(plan, powershell):
    component = REPOSITORY / "scripts/cuda-component-execution.py"
    guard = REPOSITORY / "scripts/dev/assert-human-execution.ps1"
    powershell = Path(powershell).resolve(strict=True)
    pins = {}
    for key, path in (("component", component), ("checked_guard", guard), ("powershell", powershell)):
        expected = plan["input_seals"].get(str(path))
        if not expected:
            raise ValueError("checked output audit input is absent from the plan seal: " + str(path))
        worker.pinned({"path": str(path), "sha256": expected})
        pins[key] = {"path": str(path), "sha256": expected}
    spec = importlib.util.spec_from_file_location("independent_reference_output_guard", component)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return lambda paths, created, deadline: module.guard_many(
        pins, paths, created, deadline=deadline,
    )


def run(plan_path, plan_sha, output_root, name, powershell):
    started = time.monotonic()
    plan_path = worker.pinned({"path": str(plan_path), "sha256": plan_sha})
    plan = json.loads(plan_path.read_bytes())
    sid = worker.windows_sid()
    if sid != plan["human_sid"]:
        raise ValueError("reference launcher identity differs from the verified campaign user")
    _, template, profile, original = parent.preflight(plan)
    deadline = started + plan["limits"]["wall_seconds"]
    launcher = Path(__file__).resolve()
    if plan["input_seals"].get(str(launcher)) != worker.digest(launcher):
        raise ValueError("actual maintained reference launcher is absent from the plan seal")
    ownership = HELPER_ROOT / "reference_owned.py"
    if worker.pinned(plan["ownership_module"]) != ownership:
        raise ValueError("maintained Windows Job owner required")
    owner = worker.load_module(plan["ownership_module"], "independent_reference_job_owner")
    python = worker.pinned(plan["base_python"])
    output = private_root(output_root) / name
    if output.parent != Path(output_root) or output.name != name or output.exists():
        raise ValueError("fresh direct child of the checked private output root required")
    if time.monotonic() >= deadline:
        raise TimeoutError("reference phase expired before creating the output directory")

    audit = checked_output_guard(plan, powershell)
    audit([output], False, deadline)
    output.mkdir()
    audit([output], True, deadline)
    caches = [
        output / directory for directory in
        ("cuda-cache", "torch-cache", "hf-cache", "pycache", "tmp", "cache")
    ]
    raw_path = output / "torch-output.json"
    log_path = output / "stdout-stderr.log"
    stdin_path = output / "stdin.txt"
    receipt_path = output / "execution.json"
    audit([*caches, raw_path, log_path, stdin_path, receipt_path], False, deadline)
    for directory in caches:
        directory.mkdir()
    audit(caches, True, deadline)
    command = [
        str(python), "-B", "-S", "-X", "pycache_prefix=" + str(output / "pycache"),
        str(worker.pinned(plan["worker"])), "--plan", str(plan_path),
        "--plan-sha256", plan_sha, "--output", str(raw_path),
    ]
    env = {key: value for key, value in os.environ.items() if not key.upper().startswith("PYTHON")}
    env.update(
        PYTHONDONTWRITEBYTECODE="1", PYTHONPATH=plan["site_packages"],
        HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1", HF_HUB_DISABLE_TELEMETRY="1",
        CUBLAS_WORKSPACE_CONFIG=":4096:8", OMP_NUM_THREADS=str(plan["limits"]["threads"]),
        MKL_NUM_THREADS=str(plan["limits"]["threads"]),
        CUDA_CACHE_PATH=str(output / "cuda-cache"), TORCH_HOME=str(output / "torch-cache"),
        HF_HOME=str(output / "hf-cache"), XDG_CACHE_HOME=str(output / "cache"),
        TORCHINDUCTOR_CACHE_DIR=str(output / "torch-cache"),
        TEMP=str(output / "tmp"), TMP=str(output / "tmp"),
    )
    candidate_assets = [
        asset for asset in profile["assets"]
        if asset["role"] in ("model", "external_weights", "tokenizer")
    ]
    receipt = {
        "schema": "airk.arbiter.independent-numeric-reference-execution/1",
        "model": template["model"], "compiler": template["compiler"],
        "resource_policy_digest": template["resource_policy"], "reference_kind": plan["kind"],
        "source_assets": original["original_reference_assets"] if plan["kind"] == "original_fp32" else candidate_assets,
        "input_seals": dict(plan["input_seals"], **{str(plan_path): plan_sha}),
        "command": command, "execution_user_sid": sid, "native_airk_nn_invocations": 0,
        "complete": False, "failure": None, "post_input_drift": [], "rows": [],
        "whole_phase_deadline_not_refreshed": True, "whole_phase_expired": False,
        "no_automatic_retry": True, "other_processes_touched": False,
    }

    def before_resume(api, _job, info, child_deadline):
        return parent.suspended_authentication(
            api, info, deadline, child_deadline, sid, plan["base_python"], owner.image_proof,
        )

    try:
        stdin_path.touch(exist_ok=False)
        audit([stdin_path], True, deadline)
        with stdin_path.open("rb") as stdin, log_path.open("xb") as log:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("reference phase expired before creating its owned child")
            receipt["owned"] = owner.run_windows_owned_training(
                command, Path(plan["repository_root"]), log, remaining, env, stdin,
                plan["limits"]["host_committed_bytes"], before_resume=before_resume,
                absolute_deadline=deadline,
            )
        audit(list(output.rglob("*")), True, deadline)
        receipt["stdout"] = output_pin(log_path)
        if not parent.settled(receipt["owned"]):
            raise ValueError("independent reference Job did not naturally settle with exactly one child")
        raw_path = worker.regular(raw_path)
        result = json.loads(raw_path.read_bytes())
        parent.complete_arrays(result, template)
        receipt["rows"] = result["rows"]
        receipt["torch_forward_invocations"] = result["torch_forward_invocations"]
        receipt["raw_output"] = output_pin(raw_path)
        for path, expected in receipt["input_seals"].items():
            if time.monotonic() >= deadline:
                raise TimeoutError("reference phase expired during post-run input audit")
            if worker.digest(path) != expected:
                receipt["post_input_drift"].append(path)
        if receipt["post_input_drift"]:
            raise ValueError("sealed reference inputs changed during the owned phase")
        receipt["complete"] = True
    except BaseException as error:
        receipt["failure"] = {"type": type(error).__name__, "message": str(error)}
        if hasattr(error, "owned_cleanup"):
            receipt["actual_cleanup_failure"] = error.owned_cleanup
        raise
    finally:
        late_failure = parent.final_phase_observation(receipt, deadline, started)
        with receipt_path.open("x", encoding="utf-8") as stream:
            json.dump(receipt, stream, indent=2)
            stream.write("\n")
        if late_failure:
            raise TimeoutError("reference phase expired during final observation")
    audit([receipt_path], True, None)
    return receipt_path


def output_pin(path):
    path = worker.regular(path)
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": worker.digest(path)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--plan-sha256", required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--name", required=True)
    parser.add_argument("--powershell", type=Path, required=True)
    args = parser.parse_args()
    print(run(args.plan, args.plan_sha256, args.output_root, args.name, args.powershell))


if __name__ == "__main__":
    main()
