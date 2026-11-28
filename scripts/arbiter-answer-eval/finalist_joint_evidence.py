# SPDX-License-Identifier: Apache-2.0
"""Attach separately scored need evidence while protecting the original bundled passages."""
from dataclasses import dataclass, replace
from math import isfinite


@dataclass(frozen=True)
class Evidence:
    at: str
    text: str
    supports: tuple = ()
    file_scored_at: str | None = None


def extent(at):
    path, span = at.rsplit(":", 1)
    first, last = map(int, span.split("-"))
    if not path or first < 1 or last < first:
        raise ValueError("Invalid source extent")
    return path, first, last


def contains(enclosing, inner):
    path, first, last = extent(enclosing)
    other, start, end = extent(inner)
    return path == other and first <= start <= end <= last


def overlaps(left, right):
    path, first, last = extent(left)
    other, start, end = extent(right)
    return path == other and max(first, start) <= min(last, end)


def proposals(records, need_count):
    groups = [[] for _ in range(need_count)]
    for i, row in enumerate(records):
        owner, score = row["need"], row["score"]
        if type(owner) is not int or not 1 <= owner <= need_count or not isfinite(score) or not 0 <= score <= 1:
            raise ValueError("Invalid need-reranking score")
        if not contains(row["returned_at"], row["scored_at"]):
            raise ValueError("Scored input extends outside its source range")
        # This is the yes/no token preference, not a coverage or acceptance judgment.
        if score > 0.5:
            groups[owner-1].append((-score, i, row))
    return [[item[2] for item in sorted(g)] for g in groups]


def attach(repository, seed, records, need_count, frame, *, passage_limit, preserve_file_score=True):
    """Offer one link per need, then one new lead for each remaining need, without dropping baseline passages."""
    current = [Evidence(p.at, p.text, (), getattr(p, "file_scored_at", None) if preserve_file_score else None)
               for p in seed]
    initial, keep = frame(current)
    if initial is None or list(keep) != current:
        return None
    groups = proposals(records, need_count)
    accepted, represented = [], set()
    wire = initial
    for n, group in enumerate(groups, 1):
        for r in group:
            candidates = [i for i, p in enumerate(current) if contains(p.at, r["scored_at"])]
            if not candidates:
                continue
            index = candidates[0]
            p = current[index]
            link = {"need": n, "scored_at": r["scored_at"]}
            proposal = [*current]
            proposal[index] = replace(p, supports=(*p.supports, link))
            trial_wire, kept = frame(proposal)
            if trial_wire is not None and list(kept) == proposal:
                wire, current = trial_wire, proposal
                accepted.append({"kind": "existing", **r})
                represented.add(n)
                break
    for n, group in enumerate(groups, 1):
        if n in represented:
            continue
        for r in group:
            at, scored = r["returned_at"], r["scored_at"]
            path, first, last = extent(at)
            files = {extent(p.at)[0] for p in current}
            if path not in files and len(files) >= 6 or any(overlaps(at, p.at) for p in current):
                continue
            lines = repository.texts[path]
            if last > len(lines):
                raise ValueError("Proposed passage beyond source")
            text = "\n".join(lines[first-1:last]) + "\n"
            if sum(len(p.text.encode()) for p in current) + len(text.encode()) > passage_limit:
                continue
            added = Evidence(at, text, ({"need": n, "scored_at": scored},))
            proposal = [*current, added]
            trial_wire, kept = frame(proposal)
            if trial_wire is not None and list(kept) == proposal:
                wire, current = trial_wire, proposal
                accepted.append({"kind": "new", **r})
                represented.add(n)
                break
    return wire, current, accepted
