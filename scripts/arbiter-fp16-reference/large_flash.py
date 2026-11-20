# SPDX-License-Identifier: Apache-2.0
"""Independent CPU implementation of the SM86 Large causal Flash tile schedule."""

import math
import struct


def operand_scale():
    # ORT narrows its host softmax_scale to float before multiplying by log2(e).
    f32 = lambda x: struct.unpack("f", struct.pack("f", x))[0]
    return f32(f32(128 ** -0.5) * math.log2(math.e))


def flash_block(torch, query, keys, values, query_start, check):
    """Fold 64-column tiles from right to left in bounded CPU memory."""
    width = query.shape[-2]
    heads = query.shape[1]
    shape = (1, heads, width, 1)
    maximum = torch.full(shape, -math.inf)
    totals = torch.zeros(*shape[:-1], 4)
    accumulator = torch.zeros(*shape[:-1], 128)
    query_positions = torch.arange(query_start, query_start + width)[:, None]
    scale = operand_scale()
    for start in range(query_start, -1, -64):
        check()
        tile = keys[..., start:start + 64, :]
        scores = query @ tile.transpose(-1, -2)
        if start == query_start:
            scores.masked_fill_(query_positions < torch.arange(start, start + tile.shape[-2]), -math.inf)
        updated = torch.maximum(maximum, scores.amax(dim=-1, keepdim=True))
        if start != query_start:
            factor = torch.exp2((maximum - updated) * scale)
            totals.mul_(factor)
            accumulator.mul_(factor)
        maximum = updated
        exponents = ((scores.double() * scale) - (maximum * scale).double()).float().exp2_()
        # CuTe SM80_16x8_Row assigns a pair of adjacent columns to each of
        # four lanes. Each lane sums its columns sequentially before the
        # XOR-2, XOR-1 shuffle at final normalization.
        columns = exponents.reshape(*exponents.shape[:-1], -1, 4, 2)
        for group in columns.unbind(-3):
            totals.add_(group[..., 0])
            totals.add_(group[..., 1])
        accumulator.add_(exponents.half().float() @ values[..., start:start + 64, :])
    denominator = (totals[..., 0] + totals[..., 2]) + (totals[..., 1] + totals[..., 3])
    return (accumulator * denominator.reciprocal().unsqueeze(-1)).half()


def forward(independent, row, check):
    torch = independent.torch
    n = len(row["input_ids"])
    if n not in (128, 1024, 8192) or independent.layers != 36 or independent.qheads != 32:
        raise ValueError("Large 36-layer 32-query-head reference and a declared wider input required")
    owner = independent.forward.__func__.__globals__
    original = owner["causal_half_attention"]
    calls = 0

    def attention(torch, q, k, v, mask, *, context_flash_single_tile):
        nonlocal calls
        if (context_flash_single_tile or list(mask) != [1] * n or q.shape != (1, 32, n, 128)
                or k.shape != (1, 8, n, 128) or v.shape != k.shape
                or any(x.device.type != "cpu" or x.dtype != torch.float16 for x in (q, k, v))):
            raise ValueError("all-live CPU Large half operands with 32:8 GQA required")
        keys = k.repeat_interleave(4, dim=1).float()
        values = v.repeat_interleave(4, dim=1).float()
        output = torch.empty_like(q)
        for start in range(0, n, 64):
            output[..., start:start + 64, :] = flash_block(
                torch, q[..., start:start + 64, :].float(), keys, values, start, check)
        calls += 1
        return output

    owner["causal_half_attention"] = attention
    try:
        scores = independent.forward(row)
    finally:
        owner["causal_half_attention"] = original
    if calls != 36:
        raise ValueError("expected one governed attention call per Large layer")
    return scores
