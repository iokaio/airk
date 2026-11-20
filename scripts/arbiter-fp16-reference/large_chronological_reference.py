# SPDX-License-Identifier: Apache-2.0
"""CPU reference adapter for original Context Large inputs and the 64-column causal Flash schedule."""
import copy
import hashlib
from pathlib import Path
import runpy
from types import FunctionType, MethodType


def frozen_function(filename, sha256, name):
    path = Path(__file__).with_name(filename)
    if hashlib.sha256(path.read_bytes()).hexdigest() != sha256:
        raise ValueError("Large reference source differs: " + filename)
    return runpy.run_path(str(path))[name]


flash_block = frozen_function("large_flash.py", "24d551de98080eb338f602a725161ed35a95456646860575e1bf16a28d3284cb", "flash_block")
protected_qkv = frozen_function("large_qkv_reference.py", "70d2a406c8fc42de7059c2d5ed15d5a5a9af53a00d9915fe780438109fe5d7ce", "score")


def causal_attention(torch, query, keys, values, mask, check):
    n = query.shape[-2]
    if (not 1 <= n <= 8192 or list(mask) != [1] * n or query.shape != (1, 32, n, 128)
            or keys.shape != (1, 8, n, 128) or values.shape != keys.shape
            or any(x.device.type != "cpu" or x.dtype != torch.float16 for x in (query, keys, values))):
        raise ValueError("expected all-live original Large Context input with 32:8 CPU half heads")
    # Native out-of-range columns in the final KV tile are masked. Supply zero storage
    # for these absent operands so the frozen eight-column lane reduction stays intact.
    tail = (-n) % 64
    output = torch.empty_like(query)
    for kv in range(8):
        heads = slice(4 * kv, 4 * kv + 4)
        key = torch.nn.functional.pad(keys[:, kv:kv + 1].float(), (0, 0, 0, tail))
        value = torch.nn.functional.pad(values[:, kv:kv + 1].float(), (0, 0, 0, tail))
        for start in range(0, n, 64):
            block = query[:, heads, start:start + 64].float()
            output[:, heads, start:start + 64] = flash_block(torch, block, key, value, start, check)
    return output


def score(independent, row, check, *, widen_qkv):
    """Compose a verified original model with a separately selected projection treatment.

    The caller authenticates publisher weights, graph and observed native attention dispatch.
    This routine has no artifact-registration or release authority.
    """
    if type(widen_qkv) is not bool or independent.focus or independent.layers != 36:
        raise ValueError("explicit Large Context projection treatment required")
    torch = independent.torch
    base = independent.forward.__func__
    validate = base.__globals__["validate_native"]
    validate(torch, independent.config, False, row)
    n = len(row["input_ids"])
    if n <= 32:
        result = (protected_qkv(independent, row, check) if widen_qkv
                  else independent.forward(row, context_flash_single_tile=True))
    else:
        calls = 0

        def attention(torch, q, k, v, mask, *, context_flash_single_tile):
            nonlocal calls
            if context_flash_single_tile:
                raise ValueError("unexpected short-only attention dispatch")
            calls += 1
            return causal_attention(torch, q, k, v, mask, check)

        # Bind the original forward bytecode to this instance and its attention implementation.
        # Keep its other arithmetic and caller's global namespace intact.
        model = copy.copy(independent)
        implementation = FunctionType(base.__code__, {**base.__globals__, "causal_half_attention": attention},
                                      base.__name__, base.__defaults__, base.__closure__)
        implementation.__kwdefaults__ = base.__kwdefaults__
        model_forward = MethodType(implementation, model)

        def wide_forward(row, *, context_flash_single_tile=False):
            if context_flash_single_tile != widen_qkv:
                raise ValueError("unexpected projection treatment entry")
            return model_forward(row)

        model.forward = wide_forward
        result = protected_qkv(model, row, check) if widen_qkv else model.forward(row)
        if calls != 36:
            raise ValueError("incomplete 36-layer Large attention reference")
    if (result.device.type != "cpu" or result.dtype != torch.float32 or result.shape != (1, len(row["option_mask"]))
            or not torch.isfinite(result).all().item()):
        raise ValueError("incomplete or nonfinite original-card CPU vector")
    return result
