# SPDX-License-Identifier: Apache-2.0
"""Isolate the Compact T32 layer-0 CUDA half projections on sealed operands."""

import ctypes
import os
from pathlib import Path

# CUDA 12.8 headers: library_types.h, driver_types.h, and cublas_api.h.
_CUDA_R_16F = 2
_HOST_TO_DEVICE = 1
_DEVICE_TO_HOST = 2
_CUBLAS_OP_N = 0
_CUBLAS_POINTER_MODE_HOST = 0
_CUBLAS_GEMM_DEFAULT = -1
_MODES = {
    "GEMM2": (16, 68),  # DISALLOW_REDUCED_PRECISION_REDUCTION, COMPUTE_32F
    "FP32_PEDANTIC": (2, 69),  # PEDANTIC_MATH, COMPUTE_32F_PEDANTIC
}
_TOKENS = 32
_HIDDEN = 2048
_WIDTHS = {"k": 1024, "v": 1024, "q": 2048}
_HALF_BYTES = 2


def _load_dll(path, signatures):
    # Search the pinned DLL's own directory for its siblings and System32.
    dll = ctypes.WinDLL(os.fspath(path), winmode=0x100 | 0x800)
    for name, argtypes in signatures.items():
        function = getattr(dll, name)
        function.argtypes = argtypes
        function.restype = ctypes.c_int
    return dll


def _checked(function, *args):
    status = function(*args)
    if status != 0:
        raise RuntimeError(f"{function.__name__} failed with status {status}")


def _allocate(runtime, buffers, length):
    pointer = ctypes.c_void_p()
    _checked(runtime.cudaMalloc, ctypes.byref(pointer), length)
    if not pointer.value:
        raise RuntimeError("cudaMalloc returned a null pointer")
    buffers.append(pointer)
    return pointer


def _cleanup(runtime, blas, handle, buffers):
    errors = []

    def attempt(function, *args):
        try:
            _checked(function, *args)
        except Exception as error:
            errors.append(str(error))
            return False
        return True

    if (handle.value or buffers) and attempt(runtime.cudaDeviceSynchronize):
        if handle.value:
            attempt(blas.cublasDestroy_v2, handle)
        for pointer in reversed(buffers):
            attempt(runtime.cudaFree, pointer)
    return errors


def run_modes(
    cudart_path: str | os.PathLike,
    cublas_path: str | os.PathLike,
    input_half: bytes,
    weights_by_name: dict[str, tuple[int, bytes]],
) -> dict[str, dict[str, bytes]]:
    """Return row-major binary16 K/V/Q results from CUDA device 0 for both modes."""
    input_length = _TOKENS * _HIDDEN * _HALF_BYTES
    if not isinstance(input_half, bytes) or len(input_half) != input_length:
        raise ValueError("Compact T32 activation must contain exactly [32,2048] half words")
    if not isinstance(weights_by_name, dict) or set(weights_by_name) != _WIDTHS.keys():
        raise ValueError("exact Compact K, V and Q weights required")
    for name, expected_width in _WIDTHS.items():
        width, raw = weights_by_name[name]
        if (type(width) is not int or width != expected_width or not isinstance(raw, bytes)
                or len(raw) != _HIDDEN * width * _HALF_BYTES):
            raise ValueError(f"Compact {name} projection width or binary16 weights differ")

    if os.name != "nt" or ctypes.sizeof(ctypes.c_void_p) != 8:
        raise OSError("the pinned CUDA diagnostic requires 64-bit Windows")
    cudart_path, cublas_path = Path(cudart_path), Path(cublas_path)
    if not (cudart_path.is_absolute() and cublas_path.is_absolute()
            and cudart_path.name.lower() == "cudart64_12.dll"
            and cublas_path.name.lower() == "cublas64_12.dll"
            and cudart_path.parent == cublas_path.parent):
        raise ValueError("absolute sibling CUDA 12 DLL paths required")

    pointer, integer = ctypes.c_void_p, ctypes.c_int
    runtime = _load_dll(cudart_path, {
        "cudaSetDevice": [integer],
        "cudaMalloc": [ctypes.POINTER(pointer), ctypes.c_size_t],
        "cudaFree": [pointer],
        "cudaMemcpy": [pointer, pointer, ctypes.c_size_t, integer],
        "cudaDeviceSynchronize": [],
    })
    blas = _load_dll(cublas_path, {
        "cublasCreate_v2": [ctypes.POINTER(pointer)],
        "cublasDestroy_v2": [pointer],
        "cublasSetPointerMode_v2": [pointer, integer],
        "cublasSetMathMode": [pointer, integer],
        "cublasGetMathMode": [pointer, ctypes.POINTER(integer)],
        "cublasGemmEx": [
            pointer, integer, integer, integer, integer, integer,
            pointer, pointer, integer, integer, pointer, integer,
            integer, pointer, pointer, integer, integer, integer, integer,
        ],
    })

    _checked(runtime.cudaSetDevice, 0)
    handle = pointer()
    buffers = []
    primary = None
    try:
        _checked(blas.cublasCreate_v2, ctypes.byref(handle))
        if not handle.value:
            raise RuntimeError("cublasCreate_v2 returned a null handle")
        _checked(blas.cublasSetPointerMode_v2, handle, _CUBLAS_POINTER_MODE_HOST)
        device_input = _allocate(runtime, buffers, input_length)
        max_width = max(_WIDTHS.values())
        device_weight = _allocate(runtime, buffers, _HIDDEN * max_width * _HALF_BYTES)
        device_output = _allocate(runtime, buffers, _TOKENS * max_width * _HALF_BYTES)
        _checked(runtime.cudaMemcpy, device_input, ctypes.c_char_p(input_half),
                 input_length, _HOST_TO_DEVICE)

        alpha, beta = ctypes.c_float(1.0), ctypes.c_float(0.0)
        results = {}
        for mode, (math_mode, compute_type) in _MODES.items():
            _checked(blas.cublasSetMathMode, handle, math_mode)
            observed = integer()
            _checked(blas.cublasGetMathMode, handle, ctypes.byref(observed))
            if observed.value != math_mode:
                raise RuntimeError(f"cuBLAS math mode differs for {mode}: {observed.value}")
            results[mode] = result = {}
            for name in _WIDTHS:
                width, weights = weights_by_name[name]
                _checked(runtime.cudaMemcpy, device_weight, ctypes.c_char_p(weights),
                         len(weights), _HOST_TO_DEVICE)
                # cuBLAS is column-major. Swap operands and compute (XW)^T.
                _checked(
                    blas.cublasGemmEx, handle, _CUBLAS_OP_N, _CUBLAS_OP_N,
                    width, _TOKENS, _HIDDEN, ctypes.byref(alpha),
                    device_weight, _CUDA_R_16F, width,
                    device_input, _CUDA_R_16F, _HIDDEN, ctypes.byref(beta),
                    device_output, _CUDA_R_16F, width, compute_type, _CUBLAS_GEMM_DEFAULT,
                )
                _checked(runtime.cudaDeviceSynchronize)
                length = _TOKENS * width * _HALF_BYTES
                host_output = (ctypes.c_ubyte * length)()
                _checked(runtime.cudaMemcpy, host_output, device_output, length, _DEVICE_TO_HOST)
                result[name] = bytes(host_output)
        return results
    except BaseException as error:
        primary = error
        raise
    finally:
        failures = _cleanup(runtime, blas, handle, buffers)
        if failures:
            message = "CUDA projection cleanup failed: " + ", ".join(failures)
            if primary is None:
                raise RuntimeError(message)
            primary.add_note(message)
