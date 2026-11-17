# SPDX-License-Identifier: Apache-2.0
"""Precommitted weak-parent max objective; no fragment-positive labels."""

def encode_checkpointed(encode, inputs, microbatch=8, checkpointed=True):
    import torch
    from torch.utils.checkpoint import checkpoint
    parts = []
    for start in range(0, inputs[0].shape[0], microbatch):
        values = tuple(value[start:start+microbatch] for value in inputs)
        parts.append(checkpoint(encode, *values, use_reentrant=False, preserve_rng_state=True)
            if checkpointed else encode(*values))
    if not parts:
        raise ValueError('empty native unit population cannot be a training bag')
    return torch.cat(parts)

def logits_for_bags(queries, units, bags, temperature=.05):
    import torch
    if not bags or any(not bag for bag in bags):
        raise ValueError('empty parent bag is not a positive/negative score')
    # Caller freezes unit-ID order. torch.max selects the FIRST equal maximum;
    # no no-grad winner selection or second stochastic encoder pass occurs.
    return torch.stack([(queries @ units[bag].T).max(dim=1).values for bag in bags], dim=1) / temperature

def loss(queries, units, bags, masked, targets):
    import torch
    logits = logits_for_bags(queries, units, bags)
    if masked.shape != logits.shape or len(targets) != len(queries):
        raise ValueError('frozen parent mask/target shape differs')
    for row, target in enumerate(targets.tolist()):
        if masked[row, target] or (~masked[row]).sum().item() < 2:
            raise ValueError('no unmasked target and genuine alternative; no empty-perfect loss')
    return torch.nn.functional.cross_entropy(logits.masked_fill(masked, -torch.inf), targets)
