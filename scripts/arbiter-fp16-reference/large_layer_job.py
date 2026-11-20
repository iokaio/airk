# SPDX-License-Identifier: Apache-2.0
"""Validate a private residual checkpoint chain for the offline Large Direct arithmetic."""
import hashlib
import json
import ctypes
import threading
import math
import os
from pathlib import Path
import runpy
import struct
import sys

LAYER_OWNER = Path(__file__).with_name("large_direct_layers.py")
LAYER_SHA = "a90b489c6eb97b5858bfb1cd1d87a9664a3a4633119375f05cf70313628d11dd"
BASELINE = "2f461d9c575c37fdaf8a774b72380a64fc9ed4e3afb48e632c52a534deeb7c85"


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def checked_proof(path, digest, campaign, c, check):
    require(isinstance(path,Path) and path.parent.parent == campaign and path.name == "execution.json" and digest,
            "exact campaign Job receipt required")
    pin = c["pin"](path, check)
    require(pin["sha256"] == digest, "original CPU receipt changed")
    receipt = c["read"](path)
    owned = receipt["owned"]
    require(receipt["complete"] and not receipt["whole_phase_expired"] and owned["exit_code"] == 0
            and not owned["timed_out"] and owned["owned_descendants_settled"] and owned["owned_job_final_active_processes"] == 0
            and not owned["owned_job_forced_termination"] and 0 < owned["peak_host_committed_bytes"] <= receipt["host_bytes"],
            "CPU Job did not settle under original bounds")
    proof = receipt["proof"]
    require(c["pinned"](proof, check) == path.with_name("reference.json"), "reference outside original output")
    return c["read"](proof["path"]), [pin, proof]


def predecessor(args, campaign, c, row, variant, check):
    begin, end = args.layers
    require(args.tokens in (1024, 8192) and 0 <= begin < end <= 36 and args.train_phase is None
            and not (begin == 0 and end == 36) and not args.capture_hidden and variant["protected_routes_in_model_order"] == 252,
            "fixed original Large direct layer interval required")
    pins = [c["pin"](LAYER_OWNER, check)]
    require(pins[0]["sha256"] == LAYER_SHA, "layer arithmetic source changed")
    baseline, records = checked_proof(campaign/"large-direct-fp32-independent-t1024-r1/execution.json", BASELINE,
                                      campaign, c, check)
    pins.extend(records)
    require(baseline["candidate"] == variant and baseline["tokens"] == 1024 and baseline["forward_count"] == 1,
            "original uninterrupted positive control differs")
    if args.tokens == 8192:
        control, records = checked_proof(args.layer_control, args.layer_control_sha256, campaign, c, check)
        pins.extend(records)
        study = control["layer_study"]
        require(control["candidate"] == variant and control["tokens"] == 1024 and study["interval"][1] == 36
                and study["uninterrupted_words_equal"] is True and len(study["all_layers"]) == 36
                and control["matching_fp16"] == baseline["matching_fp16"], "genuine T1024 segmentation control required")
    else:
        require(args.layer_control is None and args.layer_control_sha256 is None, "unexpected 1024 prerequisite")
    if begin == 0:
        require(args.prior_layers is None and args.prior_layers_sha256 is None, "initial layer must use original embedding")
        return pins, [], None, baseline["matching_fp16"]
    prior, records = checked_proof(args.prior_layers, args.prior_layers_sha256, campaign,c,check)
    pins.extend(records)
    study = prior["layer_study"]
    prior_rows = study["all_layers"]
    require(prior["candidate"] == variant and prior["tokens"] == args.tokens and prior["input_digest"] == row["input"]["input_digest"]
            and study["interval"][1] == begin and study["interval"][0] < begin and [r["layer"] for r in prior_rows] == list(range(begin))
            and all(r["shape"] == [1,args.tokens,2560] and r["bytes"] == args.tokens*2560*4 for r in prior_rows),
            "prior computation did not reach this input and layer")
    boundary = study["residual"]
    require(c["pinned"](boundary,check) == args.prior_layers.with_name("residual.f32")
            and boundary["bytes"] == args.tokens*2560*4 and boundary["sha256"] == prior_rows[-1]["sha256"],
            "prior CPU residual differs")
    for record in study["custody"]:
        c["pinned"](record,check)
    unique = {p["path"]:p for p in [*pins,*study["custody"],boundary]}
    return list(unique.values()), prior_rows, boundary, baseline["matching_fp16"]


def audit_hook(fallback, cache, destination):
    def observe(event, values):
        if event == "open" and isinstance(values[0],str) and Path(values[0]) == destination:
            _, mode, flags = values
            if mode == "x" and flags & os.O_EXCL and flags & os.O_CREAT and not destination.exists():
                return
        fallback(event,values,cache)
    return observe


class ProcessMemory:
    """Sample this CPU process while a layer runs; the owning Job measures its exact whole-run peak."""
    def __init__(self):
        from ctypes import wintypes as w
        class Counters(ctypes.Structure):
            _fields_ = [("cb",w.DWORD),("faults",w.DWORD)]+[(n,ctypes.c_size_t) for n in
                        ("peak_working","working","peak_paged","paged","peak_nonpaged","nonpaged","pagefile","peak_pagefile","private")]
        self.counters = Counters
        api = ctypes.WinDLL("kernel32",use_last_error=True)
        api.GetCurrentProcess.restype = w.HANDLE
        self.process = api.GetCurrentProcess()
        self.query = api.K32GetProcessMemoryInfo
        self.query.argtypes = [w.HANDLE,ctypes.POINTER(Counters),w.DWORD]
        self.query.restype = w.BOOL
        self.stop = threading.Event()
        self.lock = threading.Lock()
        self.failure = None
        self.thread = threading.Thread(target=self.poll)
        self.reset()

    def read(self):
        value = self.counters()
        value.cb = ctypes.sizeof(value)
        if not self.query(self.process,ctypes.byref(value),value.cb):
            raise ctypes.WinError(ctypes.get_last_error())
        return [value.private,value.working,value.peak_pagefile]

    def reset(self):
        self.first = self.read()
        self.peak = self.first.copy()
        self.count = 1

    def record(self):
        values = self.read()
        self.peak = [max(a,b) for a,b in zip(self.peak,values,strict=True)]
        self.count += 1

    def poll(self):
        try:
            while not self.stop.wait(0.05):
                with self.lock:
                    self.record()
        except BaseException as error:
            self.failure = error

    def observe(self):
        with self.lock:
            if self.failure is not None:
                raise self.failure
            self.record()
            result = {"start_private":self.first[0],"start_working_set":self.first[1],
                      "sampled_peak_private":self.peak[0],"sampled_peak_working_set":self.peak[1],
                      "process_cumulative_peak_pagefile":self.peak[2],"samples":self.count,"sampling_period_seconds":0.05}
            self.reset()
            return result

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.stop.set()
        self.thread.join()
        if self.failure is not None:
            raise self.failure


def execute(args, campaign, c, row, variant, model, check, output):
    begin, end = args.layers
    pins, prior, boundary, expected = predecessor(args,campaign,c,row,variant,check)
    require(sys.byteorder == "little", "original little-endian FP32 residual required")
    t = model.torch
    if boundary is None:
        previous = None
    else:
        payload = bytearray(Path(boundary["path"]).read_bytes())
        require(hashlib.sha256(payload).hexdigest() == boundary["sha256"], "loaded residual changed")
        previous = t.frombuffer(payload,dtype=t.float32).reshape(1,args.tokens,2560)
    rows = list(prior)
    def observe(i, hidden, seconds):
        memory = sampler.observe()
        data = hidden.contiguous().numpy().tobytes()
        require(len(data) == args.tokens*2560*4 and i == len(rows) and seconds > 0, "invalid layer observation")
        entry={"layer":i, "shape":[1,args.tokens,2560], "bytes":len(data),
               "sha256":hashlib.sha256(data).hexdigest(), "seconds":seconds,"process_memory":memory,
               "max_one_fp32_attention_score_tile_bytes":16*min(32,args.tokens)*min(128,args.tokens)*4}
        rows.append(entry)
        print("AIRK_LARGE_OFFLINE_LAYER "+json.dumps(entry),flush=True)
        check()
    f = runpy.run_path(str(LAYER_OWNER))["calculate"]
    with ProcessMemory() as sampler:
        hidden, scores = f(model, row["input"], begin, end, previous, check, observe)
    data = hidden.contiguous().numpy().tobytes()
    with output.open("xb") as stream:
        require(stream.write(data) == len(data), "short exclusive residual write")
    c["windows_owner"](output)
    pin = c["pin"](output,check)
    require(pin["sha256"] == rows[-1]["sha256"] and pin["bytes"] == rows[-1]["bytes"], "persisted residual changed")
    complete = end == 36
    values = scores[0].tolist() if complete else None
    equal = None
    if complete:
        require(len(values) == len(row["input"]["option_mask"]) and all(math.isfinite(x) for x in values),
                "sparse CPU scores differ")
        if args.tokens == 1024:
            equal = struct.pack("<8f",*values) == struct.pack("<8f",*expected)
            require(equal, "segmented T1024 differs from the original uninterrupted score words")
    return values, {"interval":[begin,end],"all_layers":rows,"custody":pins,"residual":pin,
                    "complete_chain":complete, "uninterrupted_words_equal":equal, "single_forward":False}
