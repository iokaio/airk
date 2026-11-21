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
        self._lap = None
        self._lock = threading.Lock()
        self._stop_event = threading.Event()

    def run(self):
        while not self._stop_event.is_set():
            value = gpu_used_mib()
            if value is not None:
                with self._lock:
                    self.samples += 1
                    self.peak = value if self.peak is None else max(self.peak, value)
                    self._lap = value if self._lap is None else max(self._lap, value)
            self._stop_event.wait(self.interval)

    def lap(self, now=None):
        """The peak since the previous lap or the start, counting a reading the caller has just taken."""
        with self._lock:
            seen, self._lap = self._lap, None
        values = [value for value in (seen, now) if value is not None]
        return max(values) if values else None

    def stop(self):
        self._stop_event.set()
        self.join(timeout=15)
        return self.peak


def checked(compiled, reference, source):
    tokens = len(compiled["input_ids"])
    require(len(compiled["attention_mask"]) == tokens and len(compiled["position_ids"]) == tokens
            and len(compiled["candidate_positions"]) == len(compiled["option_mask"])
            and (reference is None or len(reference) == len(compiled["option_mask"])),
            "retained case has inconsistent shapes: " + str(source))
    return tokens


def load_case(path, reference=True):
    """Read one retained reference receipt: compiled inputs and the original FP32 scores.

    With reference=False only the inputs are used. That serves a model other than the one
    the receipt belongs to, where the receipt's scores do not apply.
    """
    path = Path(path)
    doc = json.loads(path.read_bytes())
    result = doc["result"]
    compiled = result["compiled"]
    scores = result["original_binding"]["original_fp32"] if reference else None
    tokens = checked(compiled, scores, path)
    return {"case": result["case"] + ("" if reference else " (inputs only)"),
            "receipt": {"path": str(path), "sha256": digest(path)},
            "input_digest": compiled["input_digest"], "tokens": tokens, "compiled": compiled,
            "original_fp32": scores, "matching_fp16": result.get("matching_fp16") if reference else None}


def load_fixture(path):
    """Read every row of a retained native numeric fixture: inputs and original FP32 scores."""
    path = Path(path)
    doc = json.loads(path.read_bytes())
    require(doc.get("schema") == "airk.private-context-native-fp32-probes/1", "unexpected fixture schema: " + str(path))
    pin = {"path": str(path), "sha256": digest(path)}
    cases = []
    for row in doc["rows"]:
        compiled, scores = row["input"], row["original_fp32_all_live"]
        tokens = checked(compiled, scores, path)
        cases.append({"case": row["id"], "receipt": pin, "input_digest": None, "tokens": tokens,
                      "compiled": compiled, "original_fp32": scores, "matching_fp16": None})
    return cases


def case_argument(text):
    """Split a case argument into its path and the exact padded length of an optional @N suffix."""
    head, mark, tail = text.rpartition("@")
    if mark and head and tail.isascii() and tail.isdigit():
        return Path(head), int(tail)
    return Path(text), None


def pad_length(tokens, lengths):
    """The smallest listed length that holds the tokens. A case longer than every listed length is refused."""
    fits = [length for length in lengths if length >= tokens]
    require(bool(fits), "a case of " + str(tokens) + " tokens is longer than every padded length")
    return min(fits)


def pad_plan(tokens, lengths, exact=None):
    """The padded token count of one case and the rule that gave it: an exact length, the ladder, or neither."""
    if exact is not None:
        require(exact >= tokens, "a case of " + str(tokens) + " tokens cannot be padded to " + str(exact))
        return exact, "exact"
    if lengths:
        return pad_length(tokens, lengths), "ladder"
    return None, None


def padded(compiled, length, pad_token_id):
    """Right-pad compiled inputs to a token count.

    A pad token takes the pad id and a zero attention mask, and the position ids continue the sequence. The
    decision position and the candidates are unchanged, so the graph still scores the original last token. The
    result holds the six inputs only: the retained input digest belongs to the unpadded input.
    """
    tokens = len(compiled["input_ids"])
    require(1 <= tokens <= length, "cannot pad " + str(tokens) + " tokens to " + str(length))
    extra = length - tokens
    following = compiled["position_ids"][-1] + 1
    return {"input_ids": compiled["input_ids"] + [pad_token_id] * extra,
            "attention_mask": compiled["attention_mask"] + [0] * extra,
            "position_ids": compiled["position_ids"] + list(range(following, following + extra)),
            "decision_position": compiled["decision_position"],
            "candidate_positions": compiled["candidate_positions"], "option_mask": compiled["option_mask"]}


def padding_basis(graph):
    """What a graph's structure says about right-padding: what reads each input, and how attention runs.

    Right-padding is supported only when the attention mask feeds nothing but Shape nodes and every attention node
    is a causal GroupQueryAttention with no past state and no explicit bias. Then the mask's values are never
    read and no position sees a later one. This is a structural reading, not a proof: compare scores as well.
    """
    stored = {tensor.name for tensor in graph.initializer}
    readers = {item.name: sorted(node.op_type for node in graph.node if item.name in node.input)
               for item in graph.input if item.name not in stored}
    attention = [node for node in graph.node if node.op_type == "GroupQueryAttention"]
    # Inputs 3 and 4 are the past key and value. Input 10 is the explicit attention bias.
    plain = [node for node in attention
             if any(a.name == "causal" and a.i == 1 for a in node.attribute)
             and not any(node.input[3:5]) and not any(node.input[10:])]
    mask = readers.get("attention_mask", [])
    return {"input_readers": readers, "attention_nodes": len(attention),
            "causal_without_past_or_bias": len(plain),
            "supports_right_padding": bool(mask) and set(mask) == {"Shape"}
            and bool(attention) and len(plain) == len(attention)}


def padding_setup(model, lengths, pad_token_id):
    """The receipt block of a padded run. A graph whose structure does not support right-padding is refused."""
    require(pad_token_id is not None and pad_token_id >= 0, "padding needs --pad-token-id")
    require(all(length >= 1 for length in lengths), "a padded length must be at least 1")
    import onnx

    basis = padding_basis(onnx.load(str(model), load_external_data=False).graph)
    require(basis["supports_right_padding"],
            "the graph reads the attention mask's values or its attention is not plainly causal: " + str(basis))
    return {"lengths": sorted(set(lengths)), "pad_token_id": pad_token_id, "attention_mask_value": 0,
            "position_ids": "continued", "graph": basis}


def prepare_session(ort, model, provider, options, session_config, intra_threads, receipt, library_pin,
                    configure=None):
    """Prepare a session on the CPU provider, or on the WebGPU plugin's one device, and return the call that opens it.

    Opening is left to the caller, so that each script keeps its own boundary for load time and baseline memory.
    The CPU provider takes no provider options. `library_pin` gives the receipt entry of the plugin library, and
    `configure` may add to the session options before a provider is bound. The runtime answers a provider that
    fails to start, such as one given an unknown option value, by opening the session on the CPU provider instead.
    A WebGPU session that did not bind the plugin is therefore refused.
    """
    session_options = ort.SessionOptions()
    for key, value in session_config.items():
        session_options.add_session_config_entry(key, value)
    if intra_threads:
        session_options.intra_op_num_threads = intra_threads
        receipt["intra_threads"] = intra_threads
    if configure:
        configure(session_options)
    if provider == "cpu":
        return lambda: ort.InferenceSession(str(model), sess_options=session_options, providers=["CPUExecutionProvider"])
    import onnxruntime_ep_webgpu as plugin
    ort.register_execution_provider_library("webgpu", plugin.get_library_path())
    devices = [d for d in ort.get_ep_devices() if d.ep_name == plugin.get_ep_name()]
    require(len(devices) == 1, "expected exactly one WebGPU device, found " + str(len(devices)))
    hardware = devices[0].device
    receipt["plugin"] = {"version": dict(devices[0].ep_metadata).get("version"),
                         "library": library_pin(plugin.get_library_path()),
                         "adapter": {"vendor": hardware.vendor, "vendor_id": hardware.vendor_id,
                                     "device_id": hardware.device_id, **dict(hardware.metadata)}}
    session_options.add_provider_for_devices(devices, options)

    def create():
        session = ort.InferenceSession(str(model), sess_options=session_options)
        require(plugin.get_ep_name() in session.get_providers(),
                "the session did not bind the WebGPU provider: " + str(session.get_providers()))
        return session

    return create


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
    parser.add_argument("--case", action="append", default=[], type=case_argument,
                        help="a retained reference receipt: inputs and original FP32 scores; PATH@N pads it to N")
    parser.add_argument("--fixture", action="append", default=[], type=Path,
                        help="a retained native numeric fixture: every row is a case")
    parser.add_argument("--inputs-only", action="append", default=[], type=case_argument,
                        help="a retained reference receipt used for its inputs alone; PATH@N pads it to N")
    parser.add_argument("--pad-to", action="append", default=[], type=int,
                        help="a padded length; each case is right-padded to the smallest one that holds it")
    parser.add_argument("--pad-token-id", type=int, help="the token id of a pad token; required when padding")
    parser.add_argument("--unpadded-too", action="store_true",
                        help="also run each padded case unpadded first and record how far the scores differ")
    parser.add_argument("--provider", choices=("webgpu", "cpu"), default="webgpu")
    parser.add_argument("--option", action="append", default=[], help="provider option as key=value")
    parser.add_argument("--session-config", action="append", default=[], help="session config entry as key=value")
    parser.add_argument("--intra-threads", type=int, help="intra-op thread count, for CPU runs")
    parser.add_argument("--warm", type=int, default=3)
    parser.add_argument("--no-profile", action="store_true")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    require(args.output.is_dir() and not any(args.output.iterdir()), "a fresh empty output directory is required")
    cases = [case | {"exact": None} for path in args.fixture for case in load_fixture(path)]
    cases += [load_case(path) | {"exact": exact} for path, exact in args.case]
    cases += [load_case(path, reference=False) | {"exact": exact} for path, exact in args.inputs_only]
    require(bool(cases), "give at least one --case, --fixture or --inputs-only")
    for case in cases:
        case["padded_tokens"], case["padded_by"] = pad_plan(case["tokens"], args.pad_to, case["exact"])
    padding = None
    if any(case["padded_tokens"] is not None for case in cases):
        padding = padding_setup(args.model, args.pad_to, args.pad_token_id)

    import numpy as np
    import onnxruntime as ort

    options = dict(item.split("=", 1) for item in args.option)
    session_config = dict(item.split("=", 1) for item in args.session_config)
    receipt = {"schema": "airk.private.webgpu-smoke/1", "qualification": False, "provider": args.provider,
               "provider_options": options, "session_config": session_config, "onnxruntime": ort.__version__,
               "model": {"path": str(args.model), "sha256": digest(args.model)}, "padding": padding, "cases": []}

    def configure(session_options):
        if args.external_data_dir:
            data = args.external_data_dir / "model.onnx.data"
            receipt["external_data"] = {"path": str(data), "bytes": data.stat().st_size, "sha256": digest(data)}
            session_options.add_session_config_entry(
                "session.model_external_initializers_file_folder_path", str(args.external_data_dir))
        if not args.no_profile:
            session_options.enable_profiling = True
            session_options.profile_file_prefix = str(args.output / "profile")

    create = prepare_session(ort, args.model, args.provider, options, session_config, args.intra_threads, receipt,
                             lambda path: {"path": path, "sha256": digest(path)}, configure)

    baseline = gpu_used_mib()
    sampler = Sampler()
    sampler.start()
    start = time.perf_counter()
    session = create()
    receipt["load_seconds"] = round(time.perf_counter() - start, 3)
    receipt["session_providers"] = session.get_providers()
    receipt["adapter_memory_mib"] = {"baseline": baseline, "after_load": gpu_used_mib()}
    sampler.lap()

    def forwards(compiled):
        """First and warm forward times of one input, and the scores of the last forward."""
        inputs = feeds(session, compiled, np)
        times, scores = [], None
        for _ in range(1 + args.warm):
            start = time.perf_counter()
            scores = session.run(["scores"], inputs)[0]
            times.append(round(time.perf_counter() - start, 4))
        return times, np.asarray(scores, dtype=np.float64).reshape(-1)

    passed = True
    for case in cases:
        unpadded = None
        compiled = case["compiled"]
        if case["padded_tokens"] is not None:
            if args.unpadded_too:
                unpadded = forwards(compiled)
            compiled = padded(compiled, case["padded_tokens"], args.pad_token_id)
        times, scores = forwards(compiled)
        finite = bool(np.isfinite(scores).all())
        if case["original_fp32"] is None:
            # No reference applies to this model for this input: record scores and timing only.
            raw, winner = None, None
            passed = passed and finite
        else:
            reference = np.asarray(case["original_fp32"], dtype=np.float64)
            raw = float(np.max(np.abs(scores - reference))) if finite else None
            winner = finite and int(np.argmax(scores)) == int(np.argmax(reference))
            passed = passed and finite and winner
        after = gpu_used_mib()
        entry = {
            "case": case["case"], "tokens": case["tokens"], "padded_tokens": case["padded_tokens"],
            "padded_by": case["padded_by"], "input_digest": case["input_digest"],
            "receipt": case["receipt"], "first_seconds": times[0], "warm_seconds": times[1:],
            "scores": [float(x) for x in scores], "original_fp32": case["original_fp32"],
            "finite": finite, "max_abs_raw_difference": raw,
            "same_winner": None if winner is None else bool(winner),
            "adapter_memory_mib_after": after, "adapter_memory_mib_peak": sampler.lap(after)}
        if unpadded is not None:
            both = finite and bool(np.isfinite(unpadded[1]).all())
            passed = passed and both
            entry["unpadded"] = {
                "first_seconds": unpadded[0][0], "warm_seconds": unpadded[0][1:],
                "scores": [float(x) for x in unpadded[1]],
                "max_abs_difference_from_padded": float(np.max(np.abs(scores - unpadded[1]))) if both else None,
                "same_winner_as_padded": both and int(np.argmax(scores)) == int(np.argmax(unpadded[1]))}
        receipt["cases"].append(entry)
    receipt["adapter_memory_mib"]["peak"] = sampler.stop()
    receipt["adapter_memory_mib"]["samples"] = sampler.samples
    if not args.no_profile:
        profile = session.end_profiling()
        receipt["profile"] = {"path": profile, "placement": placement(profile)}
    receipt["complete"] = True
    receipt["finite_and_same_winner"] = passed
    with (args.output / "smoke.json").open("x", encoding="utf-8") as file:
        json.dump(receipt, file, indent=1, allow_nan=False)
    shown = ("case", "tokens", "padded_tokens", "first_seconds", "warm_seconds", "max_abs_raw_difference",
             "same_winner", "adapter_memory_mib_peak")
    summary = {"provider": args.provider, "load_seconds": receipt["load_seconds"],
               "memory_mib": receipt["adapter_memory_mib"],
               "cases": [{k: c[k] for k in shown} | ({"unpadded": {
                   k: c["unpadded"][k] for k in ("first_seconds", "warm_seconds", "max_abs_difference_from_padded")}}
                   if "unpadded" in c else {}) for c in receipt["cases"]]}
    print(json.dumps(summary))


if __name__ == "__main__":
    main()
