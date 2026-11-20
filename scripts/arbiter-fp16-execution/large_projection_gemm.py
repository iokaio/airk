# SPDX-License-Identifier: Apache-2.0
"""Run the original cuBLAS half mode and a protected FP32 K/V/Q on actual Large operands."""

import ctypes
import hashlib
import json
import os
import runpy
from pathlib import Path


HIDDEN = 2560
TOKENS = 32
WIDTHS = {"k": 1024, "v": 1024, "q": 4096}
MANIFEST = "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5"
HELPER = "c227310f2efcc9afe5fbb6c3c7b00ec4f3824ab844d4976edfaf9fc7058bee09"


def gemms(np, stage, activation, weights):
    """Compare both operations on exactly the same row-major 32-token input."""
    if os.name != "nt" or os.environ.get("CUDA_VISIBLE_DEVICES") != "GPU-08e2b3f6-6a39-3456-77cb-a57287a702ec" or tuple(activation.shape) != (32, 2560):
        raise ValueError("expected the selected Windows adapter and original T32 input")
    if set(weights) != set(WIDTHS) or activation.dtype != np.dtype("<f2") or any(
            weights[k].dtype != np.dtype("<f2") or weights[k].shape != (HIDDEN, width) for k,width in WIDTHS.items()):
        raise ValueError("Large K/V/Q operands must retain their original FP16 shape")
    manifest = stage / "airk-cuda-runtime.json"
    helper = Path(__file__).with_name("projection_gemm_cuda.py")
    def digest(path):
        with path.open("rb") as file:
            return hashlib.file_digest(file, "sha256").hexdigest()
    if digest(manifest) != MANIFEST or digest(helper) != HELPER:
        raise ValueError("staged provider or cuBLAS call owner changed")
    info = json.loads(manifest.read_bytes())
    libraries = {f["name"].lower(): f for f in info["files"]}
    closure = ("cudart64_12.dll", "cublas64_12.dll", "cublaslt64_12.dll")
    def authenticate():
        for name in closure:
            path, row = stage / name, libraries[name]
            if path.stat().st_size != row["bytes"] or digest(path) != row["sha256"]:
                raise ValueError("Large projection CUDA library changed: " + name)
            imports = {key.lower() for key in row["imports"] + row["delay_imports"]}
            if not imports.intersection(libraries).issubset(closure):
                raise ValueError("Large projection library dependency is outside its pinned closure")
    authenticate()
    methods = runpy.run_path(str(helper))
    checked = methods["_checked"]
    ptr, num = ctypes.c_void_p, ctypes.c_int
    byref = ctypes.byref
    with os.add_dll_directory(str(stage)):
        runtime = methods["_load_dll"](stage / closure[0], {
            "cudaSetDevice": [num], "cudaDeviceSynchronize": [], "cudaMalloc": [ctypes.POINTER(ptr), ctypes.c_size_t],
            "cudaFree": [ptr], "cudaMemcpy": [ptr,ptr,ctypes.c_size_t,num]})
        blas = methods["_load_dll"](stage / closure[1], {
            "cublasCreate_v2": [ctypes.POINTER(ptr)], "cublasDestroy_v2": [ptr],
            "cublasSetPointerMode_v2":[ptr,num], "cublasSetMathMode":[ptr,num],
            "cublasSgemm_v2": [ptr,num,num,num,num,num,ptr,ptr,num,ptr,num,ptr,ptr,num],
            "cublasGemmEx": [ptr,num,num,num,num,num,ptr,ptr,num,num,ptr,num,num,ptr,ptr,num,num,num,num]})
        checked(runtime.cudaSetDevice, 0)
        handle, buffers = ptr(), []
        output = {}
        try:
            checked(blas.cublasCreate_v2, byref(handle))
            checked(blas.cublasSetPointerMode_v2, handle, 0)
            alloc = lambda size: methods["_allocate"](runtime, buffers, size)
            x = alloc(TOKENS * HIDDEN * 4)
            w = alloc(max(WIDTHS.values()) * HIDDEN * 4)
            y = alloc(TOKENS * max(WIDTHS.values()) * 4)
            alpha, beta = ctypes.c_float(1), ctypes.c_float(0)
            for mode, dtype in (("original", "<f2"), ("fp32", "<f4")):
                # ORT_CUDA_GEMM_OPTIONS=2 selects float accumulation with reduced
                # precision reduction disallowed. The FP32 path uses ordinary SGEMM.
                checked(blas.cublasSetMathMode, handle, 16 if mode == "original" else 0)
                src = np.ascontiguousarray(activation, dtype=dtype)
                checked(runtime.cudaMemcpy, x, src.ctypes.data, src.nbytes, 1)
                output[mode] = {}
                for name, width in WIDTHS.items():
                    wt = np.ascontiguousarray(weights[name], dtype=dtype)
                    checked(runtime.cudaMemcpy, w, wt.ctypes.data, wt.nbytes, 1)
                    if mode == "original":
                        checked(blas.cublasGemmEx, handle, 0,0, width,TOKENS,HIDDEN, byref(alpha),
                                w,2,width, x,2,HIDDEN, byref(beta), y,2,width, 68,-1)
                    else:
                        checked(blas.cublasSgemm_v2, handle, 0,0, width,TOKENS,HIDDEN, byref(alpha),
                                w,width, x,HIDDEN, byref(beta), y,width)
                    checked(runtime.cudaDeviceSynchronize)
                    result = np.empty((TOKENS,width), dtype=dtype)
                    checked(runtime.cudaMemcpy, result.ctypes.data, y, result.nbytes, 2)
                    output[mode][name] = result.astype("<f2").view("<u2")
        finally:
            failures = methods["_cleanup"](runtime, blas, handle, buffers)
            if failures:
                raise RuntimeError("Large cuBLAS cleanup failed: " + ", ".join(failures))
    authenticate()
    return output, {name: libraries[name]["sha256"] for name in closure}
