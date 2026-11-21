# SPDX-License-Identifier: Apache-2.0
"""Run a retained Context graph on an ONNX Runtime provider and compare with retained reference scores.

This is a smoke test. It qualifies nothing. See README.md in this folder.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import threading
import time


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(path):
    sha = hashlib.sha256()
    with open(path, "rb") as file:
        for chunk in iter(lambda: file.read(1 << 24), b""):
            sha.update(chunk)
    return sha.hexdigest()


def gpu_used_mib():
    """Whole-adapter memory in use, from nvidia-smi. None when the tool is absent or fails."""
    try:
        out = subprocess.run(
            ["nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"],
            capture_output=True, text=True, timeout=10, check=True).stdout
        return int(out.strip().splitlines()[0])
    except (OSError, subprocess.SubprocessError, ValueError, IndexError):
        return None


class Sampler(threading.Thread):
    """Poll adapter memory until stopped and keep the peak."""

    def __init__(self, interval=0.25):
        super().__init__(daemon=True)
        self.interval = interval
        self.peak = None
        self.samples = 0
        self._stop_event = threading.Event()

    def run(self):
        while not self._stop_event.is_set():
            value = gpu_used_mib()
            if value is not None:
                self.samples += 1
                self.peak = value if self.peak is None else max(self.peak, value)
            self._stop_event.wait(self.interval)

    def stop(self):
        self._stop_event.set()
        self.join(timeout=15)
        return self.peak


def load_case(path):
    """Read one retained reference receipt: compiled inputs and the original FP32 scores."""
    path = Path(path)
    doc = json.loads(path.read_bytes())
    result = doc["result"]
    compiled = result["compiled"]
    reference = result["original_binding"]["original_fp32"]
    tokens = len(compiled["input_ids"])
    require(len(compiled["attention_mask"]) == tokens and len(compiled["position_ids"]) == tokens
            and len(compiled["candidate_positions"]) == len(compiled["option_mask"]) == len(reference),
            "retained case has inconsistent shapes: " + str(path))
    return {"case": result["case"], "receipt": {"path": str(path), "sha256": digest(path)},
            "input_digest": compiled["input_digest"], "tokens": tokens, "compiled": compiled,
            "original_fp32": reference, "matching_fp16": result.get("matching_fp16")}


def feeds(session, compiled, np):
    values = {"input_ids": compiled["input_ids"], "attention_mask": compiled["attention_mask"],
              "position_ids": compiled["position_ids"], "decision_positions": [compiled["decision_position"]],
              "candidate_positions": compiled["candidate_positions"], "option_mask": compiled["option_mask"]}
    kinds = {"tensor(int64)": np.int64, "tensor(bool)": np.bool_}
    out = {}
    for item in session.get_inputs():
        require(item.name in values and item.type in kinds, "unexpected graph input: " + item.name + " " + item.type)
        array = np.asarray(values[item.name], dtype=kinds[item.type])
        if len(item.shape) == array.ndim + 1:
            array = array[None, ...]
        require(len(item.shape) == array.ndim, "rank mismatch for " + item.name)
        out[item.name] = array
    require(len(out) == len(values), "graph does not take the six expected inputs")
    return out


def placement(profile_path):
    """Count executed kernels by provider and operator from an ONNX Runtime profile."""
    events = json.loads(Path(profile_path).read_bytes())
    nodes = {}
    for event in events:
        args = event.get("args") or {}
        if event.get("cat") == "Node" and event.get("name", "").endswith("_kernel_time") and "provider" in args:
            nodes[event["name"]] = (args["provider"], args.get("op_name", "?"))
    providers = {}
    for provider, op in nodes.values():
        entry = providers.setdefault(provider, {"nodes": 0, "ops": {}})
        entry["nodes"] += 1
        entry["ops"][op] = entry["ops"].get(op, 0) + 1
    return providers


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--external-data-dir", type=Path)
    parser.add_argument("--case", action="append", required=True, type=Path)
    parser.add_argument("--provider", choices=("webgpu", "cpu"), default="webgpu")
    parser.add_argument("--option", action="append", default=[], help="provider option as key=value")
    parser.add_argument("--session-config", action="append", default=[], help="session config entry as key=value")
    parser.add_argument("--intra-threads", type=int, help="intra-op thread count, for CPU runs")
    parser.add_argument("--warm", type=int, default=3)
    parser.add_argument("--no-profile", action="store_true")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    require(args.output.is_dir() and not any(args.output.iterdir()), "a fresh empty output directory is required")

    import numpy as np
    import onnxruntime as ort

    options = dict(item.split("=", 1) for item in args.option)
    session_config = dict(item.split("=", 1) for item in args.session_config)
    receipt = {"schema": "airk.private.webgpu-smoke/1", "qualification": False, "provider": args.provider,
               "provider_options": options, "session_config": session_config, "onnxruntime": ort.__version__,
               "model": {"path": str(args.model), "sha256": digest(args.model)}, "cases": []}
    session_options = ort.SessionOptions()
    for key, value in session_config.items():
        session_options.add_session_config_entry(key, value)
    if args.intra_threads:
        session_options.intra_op_num_threads = args.intra_threads
        receipt["intra_threads"] = args.intra_threads
    if args.external_data_dir:
        data = args.external_data_dir / "model.onnx.data"
        receipt["external_data"] = {"path": str(data), "bytes": data.stat().st_size, "sha256": digest(data)}
        session_options.add_session_config_entry(
            "session.model_external_initializers_file_folder_path", str(args.external_data_dir))
    if not args.no_profile:
        session_options.enable_profiling = True
        session_options.profile_file_prefix = str(args.output / "profile")
    if args.provider == "webgpu":
        import onnxruntime_ep_webgpu as plugin
        ort.register_execution_provider_library("webgpu", plugin.get_library_path())
        devices = [d for d in ort.get_ep_devices() if d.ep_name == plugin.get_ep_name()]
        require(len(devices) == 1, "expected exactly one WebGPU device, found " + str(len(devices)))
        hardware = devices[0].device
        receipt["plugin"] = {"version": dict(devices[0].ep_metadata).get("version"),
                             "library": {"path": plugin.get_library_path(), "sha256": digest(plugin.get_library_path())},
                             "adapter": {"vendor": hardware.vendor, "vendor_id": hardware.vendor_id,
                                         "device_id": hardware.device_id, **dict(hardware.metadata)}}
        session_options.add_provider_for_devices(devices, options)
        providers = None
    else:
        providers = ["CPUExecutionProvider"]

    baseline = gpu_used_mib()
    sampler = Sampler()
    sampler.start()
    start = time.perf_counter()
    session = (ort.InferenceSession(str(args.model), sess_options=session_options) if providers is None
               else ort.InferenceSession(str(args.model), sess_options=session_options, providers=providers))
    receipt["load_seconds"] = round(time.perf_counter() - start, 3)
    receipt["session_providers"] = session.get_providers()
    receipt["adapter_memory_mib"] = {"baseline": baseline, "after_load": gpu_used_mib()}
    passed = True
    for path in args.case:
        case = load_case(path)
        inputs = feeds(session, case["compiled"], np)
        times = []
        scores = None
        for _ in range(1 + args.warm):
            start = time.perf_counter()
            scores = session.run(["scores"], inputs)[0]
            times.append(round(time.perf_counter() - start, 4))
        scores = np.asarray(scores, dtype=np.float64).reshape(-1)
        reference = np.asarray(case["original_fp32"], dtype=np.float64)
        finite = bool(np.isfinite(scores).all())
        raw = float(np.max(np.abs(scores - reference))) if finite else None
        winner = finite and int(np.argmax(scores)) == int(np.argmax(reference))
        passed = passed and finite and winner
        receipt["cases"].append({
            "case": case["case"], "tokens": case["tokens"], "input_digest": case["input_digest"],
            "receipt": case["receipt"], "first_seconds": times[0], "warm_seconds": times[1:],
            "scores": [float(x) for x in scores], "original_fp32": case["original_fp32"],
            "finite": finite, "max_abs_raw_difference": raw, "same_winner": bool(winner),
            "adapter_memory_mib_after": gpu_used_mib()})
    receipt["adapter_memory_mib"]["peak"] = sampler.stop()
    receipt["adapter_memory_mib"]["samples"] = sampler.samples
    if not args.no_profile:
        profile = session.end_profiling()
        receipt["profile"] = {"path": profile, "placement": placement(profile)}
    receipt["complete"] = True
    receipt["finite_and_same_winner"] = passed
    with (args.output / "smoke.json").open("x", encoding="utf-8") as file:
        json.dump(receipt, file, indent=1, allow_nan=False)
    summary = {"provider": args.provider, "load_seconds": receipt["load_seconds"],
               "memory_mib": receipt["adapter_memory_mib"],
               "cases": [{k: c[k] for k in ("case", "tokens", "first_seconds", "warm_seconds",
                                            "max_abs_raw_difference", "same_winner")} for c in receipt["cases"]]}
    print(json.dumps(summary))


if __name__ == "__main__":
    main()
