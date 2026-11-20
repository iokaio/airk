# SPDX-License-Identifier: Apache-2.0
"""Request-weighted paired evidence sensitivity using the plan's fixed-cluster Hoeffding bound."""
from collections import Counter
from dataclasses import dataclass
import math


@dataclass(frozen=True)
class Pair:
    request: str
    cluster: str
    fp16: bool | None
    fp32: bool | None


def half_width(sizes, alpha=0.05):
    """Independent groups, fixed request counts, arbitrary within-group dependence."""
    if not sizes or any(type(n) is not int or n < 1 for n in sizes):
        raise ValueError("positive integer cluster sizes required")
    if not 0 < alpha < 1:
        raise ValueError("invalid one-sided tail probability")
    n = sum(sizes)
    return 1.98 * math.sqrt(math.log(1 / alpha) * sum((m/n)**2 for m in sizes) / 2)


def lower_bound(contrast, sizes, alpha=0.05):
    if not -0.98 <= contrast <= 1:
        raise ValueError("invalid contrast")
    return max(-0.98, contrast - half_width(sizes, alpha))


def sensitivity(roster, pairs):
    """Retain unknown arms, count all nine cells, and bracket the complete contrast."""
    seen = set()
    sizes = Counter()
    cells = Counter()
    low = high = 0
    observed = {}
    for p in pairs:
        if p.request in observed or roster.get(p.request) != p.cluster:
            raise ValueError("duplicate or off-roster input")
        observed[p.request] = p
    for request, cluster in roster.items():
        p = observed.get(request, Pair(request, cluster, None, None))
        if not p.request or p.request in seen or not p.cluster:
            raise ValueError("one frozen request ID and cluster per row required")
        if any(x is not None and type(x) is not bool for x in (p.fp16, p.fp32)):
            raise ValueError("arms must be true, false or unknown")
        seen.add(p.request)
        sizes[p.cluster] += 1
        cells[p.fp16, p.fp32] += 1
        low += 100 * (p.fp16 is True) - 98 * (p.fp32 is not False)
        high += 100 * (p.fp16 is not False) - 98 * (p.fp32 is True)
    if not seen:
        raise ValueError("no eligible paired requests")
    denom = 100 * len(seen)
    labels = {True:"success", False:"failure", None:"unknown"}
    return {"requests": len(seen), "clusters": len(sizes),
            "cells_fp16_then_fp32": {labels[a]+"/"+labels[b]: cells[a,b] for a in labels for b in labels},
            "sample_contrast_interval": [low/denom, high/denom],
            "worst_completion_lower_95": lower_bound(low/denom, list(sizes.values()))}
