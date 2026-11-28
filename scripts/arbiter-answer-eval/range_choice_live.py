# SPDX-License-Identifier: Apache-2.0
"""Live proposal for the frozen bundled40 + spare16 single-Arbiter finalist."""
from collections import defaultdict
from dataclasses import dataclass

import adaptive_passage_budget as budget
import bundled_range_extension as extension
import live_gather
import qwen_file_trial as qwen


@dataclass(frozen=True)
class Candidate:
    at: str
    text: str
    supports: tuple = ()


def propose(repository, arguments, score):
    trace = {}
    live_gather.gather(repository, arguments, trace, top_files=qwen.WIDTH)
    paths = [path for path, _ in trace["ranked"][:qwen.WIDTH]]
    if len(set(paths)) != len(paths):
        raise ValueError("Duplicate candidate file")
    locations = defaultdict(list)
    for entry in trace["all_locations"]:
        at = entry["at"]
        path = at.rsplit(":", 1)[0]
        if path not in paths:
            raise ValueError("Source range outside scored candidate pool")
        locations[path].append(at)
    query = "\n".join([arguments["task"], *arguments.get("needs", []), *arguments.get("anchors", [])])
    pairs = [(query, qwen.document(repository, path, locations[path])) for path in paths]
    scores = score(qwen.INSTRUCTION, pairs) if pairs else []
    ranked = qwen.reorder(paths, scores)
    first, rest, seen = [], [], set()
    for path in ranked[:16]:
        for at in locations[path]:
            begin, end = map(int, at.rsplit(":", 1)[1].split("-"))
            passage = extension.make_passage(path, begin, end, repository.texts[path])
            if path not in seen and len(seen) < 6:
                first.append(passage)
                seen.add(path)
            else:
                rest.append(passage)
    stream = first + rest
    selected = budget.allocation(stream, 14000)
    by_at = {p.at: p for p in stream}
    if len(by_at) != len(stream):
        raise ValueError("Duplicate candidate extent")
    result = [Candidate(row["at"], by_at[row["at"]].text) for row in selected["passages"]]
    audit = {"policy": "bundled40-spare16", "attribution": "bundled_file", "spare_context": True,
             "passage_limit": 14000, "passage_bytes": selected["passage_bytes"],
             "omissions": selected["omissions"],
             "pool": [{"file": path, "locations": locations[path], "score": value,
                       "source_sha256": repository.digests[path].hex()} for path, value in zip(paths, scores)]}
    return result, len(stream), audit
