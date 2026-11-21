# SPDX-License-Identifier: Apache-2.0
"""Partition existing source paths by their original top-level guide directory and plan whole-guide reads."""
from collections import defaultdict
from math import ceil


def partition(paths):
    guides = defaultdict(set)
    for path in paths:
        guide, separator, rest = path.partition("/")
        if not separator or not guide or not rest or guide in (".", ".."):
            raise ValueError("published input contains a path outside a guide")
        guides[guide + "/"].add(path)
    return dict(guides)


def bundles(visible, limit, maximum_pages=160):
    weights = {prefix: max(1, ceil(len(paths) / limit)) for prefix, paths in visible.items()}
    if not weights or max(weights.values()) > maximum_pages:
        raise ValueError("a whole original guide cannot fit the measured bundle")
    count = ceil(sum(weights.values()) / maximum_pages)
    ordered = sorted(weights, key=lambda prefix: (-weights[prefix], prefix))
    while True:
        groups = [[] for _ in range(count)]
        sizes = [0] * count
        for prefix in ordered:
            i = min(range(count), key=sizes.__getitem__)
            groups[i].append(prefix)
            sizes[i] += weights[prefix]
        if max(sizes) <= maximum_pages:
            return [sorted(group) for group in groups]
        count += 1
