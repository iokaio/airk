# SPDX-License-Identifier: Apache-2.0
"""Re-emit one FP16 protected-MHA Context profile with the Answer loop's four card tasks.

GPU_FIXTURE.md, "Re-emitting the FP16 profiles for the loop", describes why and when. The image is the
``cuda_l5a_diagnostics`` test binary, staged beside the CUDA payload. Every step writes fresh outputs under
``--out`` and one line in ``commands.log``; nothing is overwritten and nothing qualifies a model.

Steps:

* ``pass1``: ``l5a_prepare`` with ``L5A_LOOP_TASKS=1`` and the sentinel policy (CPU only).
* ``graph``: ``l5a_gpu`` graph diagnostic of the pass-1 profile on the GPU. Its receipt's ``bounded_policy``
  is the review pin; ``cpu_payload_check`` must be null.
* ``pass2``: ``l5a_prepare`` again with that policy.
* ``confirm``: ``l5a_gpu`` strict on the pass-2 profile: it opens through the scorer path and scores the
  frozen Context rows at ``widths``. ``cold_open_us`` is the reload time.
* ``summary``: the facts of the four receipts in one file, ``<model>-summary.json``.
* ``all``: the five in order, stopping at the first failure.
* ``focus`` (with ``--model focus``): ``l5a_gpu`` strict on the CUDA Focus entry's profile (the spec's
  ``focus``: ``profile``, ``asset_map``, ``plan``), to show that entry still opens and scores.

GPU steps run through ``scripts/arbiter-gpu-memory/launch_sampled.py``, so each has a sampled-memory summary
beside its receipt. Run as the verified human, as the GPU owner.

``python -I -B fp16_loop_emit.py --spec <inputs.json> --model compact --image <exe> --out <dir> --step all``

The spec (private) names the pinned inputs::

    {"runtime_manifest": {"path": ..., "sha256": ...}, "control_parent": ..., "luids": ["0:13C88"],
     "context_inputs": {"path": ..., "sha256": ...}, "widths": "32,1024,8192",
     "models": {"compact": {"cpu_profile": {"path": ..., "sha256": ...}, "asset_map": {"path": ..., "sha256": ...},
                            "fp16_model": ..., "fp16_weights": ..., "measurement": {...},
                            "measurement_receipt": ..., "plan": "cold,warm,headroom", "warm": 10}}}
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402

SCHEMA = "airk.private.fp16-loop-emit/1"
LAUNCHER = HERE.parent / "arbiter-gpu-memory" / "launch_sampled.py"
STEPS = ("pass1", "graph", "pass2", "confirm", "summary")
GPU_ENV = {"ORT_CUDA_GEMM_OPTIONS": "2"}


def pinned(entry: dict) -> tuple[str, str]:
    """A spec input's path and SHA-256, after checking the bytes."""
    path = Path(entry["path"])
    common.require(path.is_file() and common.sha256_file(path) == entry["sha256"], f"{path} differs from its pin")
    return str(path), entry["sha256"]


def layout(out: Path, model: str) -> dict[str, Path]:
    return {"pass1": out / f"{model}-pass1", "final": out / f"{model}-final",
            "graph": out / "runs" / f"{model}-pass1-graph.json",
            "confirm": out / "runs" / f"{model}-final-confirm.json",
            "summary": out / f"{model}-summary.json", "log": out / "commands.log"}


def prepare_env(spec: dict, model: str, output: Path, policy: str | None) -> dict[str, str]:
    """Environment of one ``l5a_prepare`` (protected, loop tasks). ``policy`` is None for pass 1."""
    row = spec["models"][model]
    profile, profile_sha = pinned(row["cpu_profile"])
    assets, assets_sha = pinned(row["asset_map"])
    manifest, manifest_sha = pinned(spec["runtime_manifest"])
    env = {"L5A_PRECISION": "protected", "L5A_LOOP_TASKS": "1",
           "L5A_CPU_PROFILE": profile, "L5A_CPU_PROFILE_SHA256": profile_sha,
           "L5A_ASSET_MAP": assets, "L5A_ASSET_MAP_SHA256": assets_sha,
           "L5A_RUNTIME_MANIFEST": manifest, "L5A_RUNTIME_MANIFEST_SHA256": manifest_sha,
           "L5A_FP16_MODEL": row["fp16_model"], "L5A_FP16_WEIGHTS": row["fp16_weights"],
           "L5A_MEASUREMENT": json.dumps(row["measurement"], separators=(",", ":")),
           "L5A_MEASUREMENT_RECEIPT": row["measurement_receipt"],
           "L5A_PLAN": row["plan"], "L5A_OUTPUT": str(output)}
    if policy is not None:
        env["L5A_POLICY"] = policy
    return env


def gpu_env(spec: dict, model: str, step: str, profile_dir: Path, output: Path) -> dict[str, str]:
    """Environment of one ``l5a_gpu`` step over an emitted profile directory."""
    common.require(step in ("graph", "strict"), "an l5a_gpu step is graph or strict")
    row = spec["models"][model]
    profile, assets = profile_dir / "profile.json", profile_dir / "asset-map.json"
    env = {"L5A_STEP": step, "L5A_PROFILE": str(profile), "L5A_PROFILE_SHA256": common.sha256_file(profile),
           "L5A_ASSET_MAP": str(assets), "L5A_ASSET_MAP_SHA256": common.sha256_file(assets),
           "L5A_PLAN": row["plan"], "L5A_CONTROL_PARENT": spec["control_parent"], "L5A_OUTPUT": str(output),
           "ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO": "1" if step == "graph" else "0", **GPU_ENV,
           "L5A_DEADLINE_S": "300" if step == "graph" else "600"}
    if step == "strict":
        inputs, inputs_sha = pinned(spec["context_inputs"])
        env.update({"L5A_WARM": str(row.get("warm", 5)), "L5A_CONTEXT_INPUTS": inputs,
                    "L5A_CONTEXT_INPUTS_SHA256": inputs_sha, "L5A_WIDTHS": spec.get("widths", "32,1024,8192")})
    return env


def reviewed_policy(graph_receipt: Path) -> str:
    """The bounded policy a complete, passing graph diagnostic observed."""
    review = common.read_json(graph_receipt)
    common.require(review.get("schema") == "airk.private.l5a-gpu/1" and review.get("step") == "graph"
                   and review.get("complete") is True and review.get("cpu_payload_check") is None
                   and "diagnostic_error" not in review and "error" not in review
                   and isinstance(review.get("bounded_policy"), str),
                   f"{graph_receipt} is not a complete, passing graph diagnostic")
    return review["bounded_policy"]


def log_command(log: Path, name: str, selector: str, image: Path, exit_code: int, elapsed: float, env: dict) -> None:
    line = (f"{name} ({selector}, image {common.sha256_file(image)}) exit={exit_code} elapsed_s={elapsed:.3f} "
            f"at={common.now_iso()} env=" + " | ".join(f"{k}={v}" for k, v in env.items()))
    with log.open("a", encoding="utf-8", newline="\n") as stream:
        stream.write(line + "\n")


def run_prepare(image: Path, env: dict, log: Path, name: str, out: Path) -> None:
    began = time.monotonic()
    done = subprocess.run([str(image), "--exact", "l5a_prepare", "--ignored", "--nocapture"],
                          env={**os.environ, **env}, capture_output=True, check=False)
    common.write_bytes(out / "logs" / f"prepare-{name}.log", done.stdout + done.stderr)
    log_command(log, name, "l5a_prepare", image, done.returncode, time.monotonic() - began, env)
    common.require(done.returncode == 0, f"{name}: l5a_prepare exit {done.returncode} (logs/prepare-{name}.log)")


def run_gpu(image: Path, env: dict, log: Path, name: str, luids: list[str], deadline_s: float) -> None:
    output = Path(env["L5A_OUTPUT"])
    output.parent.mkdir(parents=True, exist_ok=True)
    command = [sys.executable, "-I", "-B", str(LAUNCHER), "--output-dir", str(output.with_suffix(".mem")),
               "--interval", "0.1", "--deadline-s", str(deadline_s)]
    for luid in luids:
        command += ["--luid", luid]
    for key, value in env.items():
        command += ["--env", f"{key}={value}"]
    command += ["--", str(image), "--exact", "l5a_gpu", "--ignored", "--nocapture"]
    began = time.monotonic()
    done = subprocess.run(command, capture_output=True, check=False)
    log_command(log, name, "l5a_gpu", image, done.returncode, time.monotonic() - began, env)
    common.require(done.returncode == 0, f"{name}: l5a_gpu exit {done.returncode}: "
                                         f"{(done.stdout + done.stderr).decode('utf-8', 'replace')[-600:]}")


def sampled_peak(receipt: Path) -> dict:
    """Sampled maximum of the child's GPU memory per adapter (current usage, never a lifetime peak)."""
    summary = common.read_json(receipt.with_suffix(".mem") / "summary.json")
    return {luid: row["sampled_max_current_usage_bytes"] for luid, row in summary["adapters"].items()}


def summarize(model: str, paths: dict[str, Path]) -> dict:
    """What the receipts of one model say, for the pair files and the report. No new claim is made here."""
    profile = common.read_json(paths["final"] / "profile.json")
    first = common.read_json(paths["pass1"] / "profile.json")
    graph, confirm = common.read_json(paths["graph"]), common.read_json(paths["confirm"])
    device = profile["device_execution"]
    common.require(confirm.get("complete") is True and confirm.get("step") == "strict",
                   f"{paths['confirm']} is not a complete strict run")
    common.require(device["placement"].get("policy") == graph.get("bounded_policy"),
                   "the final profile does not carry the reviewed policy")
    free = [row["adapter_after"]["free_bytes"] for row in confirm["rows"]]
    total = confirm["adapter_before"]["total_bytes"]
    return {"schema": SCHEMA, "model": model, "generated": common.now_iso(), "qualification": False,
            "profile": {"path": str(paths["final"] / "profile.json"),
                        "sha256": common.sha256_file(paths["final"] / "profile.json"),
                        "asset_map_sha256": common.sha256_file(paths["final"] / "asset-map.json"),
                        "id": profile["profile_id"], "manifest": profile["manifest"], "tasks": profile["tasks"],
                        "task_policies": profile["task_policies"], "calibration": profile["calibration"],
                        "math_policy": device["math_policy"], "provider_options": device["provider_options"],
                        "placement": device["placement"], "memory": device["memory"], "host_fit": device["host_fit"]},
            "pass1": {"profile_sha256": common.sha256_file(paths["pass1"] / "profile.json"),
                      "manifest": first["manifest"]},
            "review": {"path": str(paths["graph"]), "sha256": common.sha256_file(paths["graph"]),
                       "bounded_policy": graph["bounded_policy"], "diagnostic_us": graph.get("diagnostic_us"),
                       "sampled_peak_bytes": sampled_peak(paths["graph"])},
            "confirm": {"path": str(paths["confirm"]), "sha256": common.sha256_file(paths["confirm"]),
                        "image": confirm.get("image"), "cold_open_us": confirm.get("cold_open_us"),
                        "elapsed_s": confirm.get("elapsed_s"), "device_settled": confirm.get("device_settled"),
                        "process_cuda_poisoned": confirm.get("process_cuda_poisoned"),
                        "adapter_used_after_open_bytes": total - confirm["adapter_after_open"]["free_bytes"],
                        "adapter_used_peak_bytes": total - min(free) if free else None,
                        "adapter_used_before_bytes": total - confirm["adapter_before"]["free_bytes"],
                        "sampled_peak_bytes": sampled_peak(paths["confirm"]),
                        "rows": [{"id": row["id"], "tokens": row["tokens"], "live_tokens": row["live_tokens"],
                                  "first_call_us": row["first_call_us"], "warm_median_us": row["warm_median_us"],
                                  "warm_max_us": row["warm_max_us"], "warm_samples": len(row["warm_us"])}
                                 for row in confirm["rows"]]}}


def focus_check(spec: dict, image: Path, out: Path) -> dict:
    """Open the CUDA Focus entry's profile through the scorer path and score its pairs (``l5a_gpu`` strict).
    The FP32 Focus digests are literals, so a source movement should not close the entry; this shows it."""
    row = spec["focus"]
    profile, profile_sha = pinned(row["profile"])
    assets, assets_sha = pinned(row["asset_map"])
    output = out / "runs" / "focus-cuda-strict.json"
    env = {"L5A_STEP": "strict", "L5A_PROFILE": profile, "L5A_PROFILE_SHA256": profile_sha,
           "L5A_ASSET_MAP": assets, "L5A_ASSET_MAP_SHA256": assets_sha, "L5A_PLAN": row["plan"],
           "L5A_CONTROL_PARENT": spec["control_parent"], "L5A_OUTPUT": str(output),
           "ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO": "0", **GPU_ENV, "L5A_DEADLINE_S": "600",
           "L5A_WARM": str(row.get("warm", 10))}
    out.mkdir(parents=True, exist_ok=True)
    run_gpu(image, env, out / "commands.log", "focus-cuda-strict", list(spec["luids"]), 900)
    receipt = common.read_json(output)
    common.require(receipt.get("complete") is True and receipt.get("rows"), f"{output} is not a complete strict run")
    first = receipt["rows"][0]
    return {"path": str(output), "sha256": common.sha256_file(output), "profile_sha256": profile_sha,
            "manifest": receipt["profile"]["manifest"], "cold_open_us": receipt.get("cold_open_us"),
            "rows": len(receipt["rows"]), "full_width": {"tokens": first["tokens"], "live_tokens": first["live_tokens"],
                                                         "first_call_us": first["first_call_us"],
                                                         "warm_median_us": first["warm_median_us"]},
            "device_settled": receipt.get("device_settled"), "sampled_peak_bytes": sampled_peak(output)}


def run(spec: dict, model: str, image: Path, out: Path, step: str) -> dict:
    common.require(image.is_file(), f"no image {image}")
    if step == "focus" or model == "focus":
        common.require(step == "focus" and model == "focus", "the focus step takes --model focus, and only it")
        return focus_check(spec, image, out)
    common.require(model in spec["models"], f"the spec has no model {model!r}")
    paths = layout(out, model)
    out.mkdir(parents=True, exist_ok=True)
    luids = list(spec["luids"])
    if step == "pass1":
        run_prepare(image, prepare_env(spec, model, paths["pass1"], None), paths["log"], f"{model}-pass1", out)
        return common.read_json(paths["pass1"] / "prepare-receipt.json")["profile"]
    if step == "graph":
        run_gpu(image, gpu_env(spec, model, "graph", paths["pass1"], paths["graph"]), paths["log"],
                f"{model}-pass1-graph", luids, 600)
        return {"bounded_policy": reviewed_policy(paths["graph"])}
    if step == "pass2":
        run_prepare(image, prepare_env(spec, model, paths["final"], reviewed_policy(paths["graph"])), paths["log"],
                    f"{model}-final", out)
        return common.read_json(paths["final"] / "prepare-receipt.json")["profile"]
    if step == "confirm":
        run_gpu(image, gpu_env(spec, model, "strict", paths["final"], paths["confirm"]), paths["log"],
                f"{model}-final-confirm", luids, 900)
        return {"complete": common.read_json(paths["confirm"]).get("complete")}
    common.require(step == "summary", f"unknown step {step!r}")
    summary = summarize(model, paths)
    common.require(not paths["summary"].exists(), f"{paths['summary']} exists")
    common.write_json(paths["summary"], summary)
    return summary


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--spec", type=Path, required=True)
    parser.add_argument("--model", required=True, choices=("compact", "large", "focus"))
    parser.add_argument("--image", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--step", required=True, choices=(*STEPS, "all", "focus"))
    args = parser.parse_args(argv)
    spec = common.read_json(args.spec)
    for step in STEPS if args.step == "all" else (args.step,):
        result = run(spec, args.model, args.image, args.out, step)
        print(json.dumps({"step": step, "model": args.model, "result": result if step != "summary" else {
            "profile": result["profile"]["sha256"], "manifest": result["profile"]["manifest"]}}), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
