# SPDX-License-Identifier: Apache-2.0
"""Independent Large reference for the original half operands widened around Q/K/V GEMMs."""

import ctypes
import json
import os
from pathlib import Path


STAGE_SHA = "0ae64faa17aecc33642c3245b3bf55b6f44a961de50128114ecf5ad8442e295c"
CANDIDATE = "1448e83dcd209b120bd22e1dec084092f56adc7e8eec5dcd323e179b800467aa"
RECIPE = "large-qkv-fp32-sm86-causal-flash-single-tile"


def stage(campaign, pin, check):
    path = campaign / "large-fp32-qkv-stage-r1/stage.json"
    p = pin(path,check)
    if p["sha256"] != STAGE_SHA:
        raise ValueError("measured Large Q/K/V stage changed")
    value = json.loads(path.read_bytes())
    model = Path(value["graph"]["path"])
    if (model != path.with_name("model.onnx") or pin(model,check) != value["graph"] or value["graph"]["sha256"] != CANDIDATE
            or value["new_weight_bytes"] != 0 or value["verification"]["projection_count"] != 108):
        raise ValueError("Large Q/K/V candidate graph identity differs")
    return p,value


def cpu_limits(mask):
    from ctypes import wintypes as w
    k = ctypes.WinDLL("kernel32", use_last_error=True)
    k.GetCurrentProcess.restype = w.HANDLE
    k.GetPriorityClass.argtypes = [w.HANDLE]
    k.SetPriorityClass.argtypes = [w.HANDLE,w.DWORD]
    k.SetProcessAffinityMask.argtypes = [w.HANDLE,ctypes.c_size_t]
    k.GetProcessAffinityMask.argtypes = [w.HANDLE,ctypes.POINTER(ctypes.c_size_t),ctypes.POINTER(ctypes.c_size_t)]
    proc = k.GetCurrentProcess()
    if not isinstance(mask,int) or mask.bit_count()!=2 or not k.SetPriorityClass(proc,0x4000) or not k.SetProcessAffinityMask(proc,mask):
        raise OSError(ctypes.get_last_error(),"below-normal two-core admission failed")
    actual,system = ctypes.c_size_t(),ctypes.c_size_t()
    if not k.GetProcessAffinityMask(proc,ctypes.byref(actual),ctypes.byref(system)) or actual.value != mask or k.GetPriorityClass(proc)!=0x4000:
        raise ValueError("measured affinity or priority differs")
    return {"process_affinity":hex(actual.value),"priority_class":"below_normal","torch_intraop_threads":2,
            "torch_interop_threads":1,"pid":os.getpid()}


def score(independent, row, check):
    torch = independent.torch
    if (independent.layers,independent.hidden,independent.qheads,independent.kvheads,independent.dim) != (36,2560,32,8,128):
        raise ValueError("exact Large publisher geometry required")
    routes = tuple(f"layers.{n}.self_attn.{r}_proj.weight" for n in range(36) for r in ("q","k","v"))
    expected = set(routes)
    calls = []
    original = independent._linear_half
    torch.set_float32_matmul_precision("highest")
    def protected(value, name):
        if name not in expected:
            return original(value,name)
        weight = independent.weights[name]
        if value.device.type != "cpu" or weight.device.type != "cpu" or value.dtype != torch.float16 or weight.dtype != torch.float16:
            raise ValueError("independent original FP16 operands required")
        check()
        calls.append(name)
        return torch.nn.functional.linear(value.float(),weight.float()).half()
    independent._linear_half = protected
    try:
        result = independent.forward(row,context_flash_single_tile=True)
    finally:
        del independent._linear_half
    if tuple(calls) != routes:
        raise ValueError("expected exactly Q/K/V in all 36 Large layers")
    return result
