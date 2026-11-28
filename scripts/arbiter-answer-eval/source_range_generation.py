# SPDX-License-Identifier: Apache-2.0
"""Generate additional source windows within the frozen eligible files, before any model call."""
from bisect import bisect_right
from collections import Counter, defaultdict
from itertools import islice
from math import fsum, log

from range_choice import SourceRange, _cards, nominate as original_nominations, round_robin, terms

SEED_LIMIT = 4096
WINDOW_LINES = 60


def seed_lines(length):
    if length <= SEED_LIMIT:
        return range(1, length + 1)
    return [1 + i * (length - 1) // (SEED_LIMIT - 1) for i in range(SEED_LIMIT)]


def unit_window(seed, starts, length):
    """Use the current named unit, with a bounded centered window for an oversized or unnamed unit."""
    i = bisect_right(starts, seed)
    lower = starts[i - 1] if i else 1
    upper = starts[i] - 1 if i < len(starts) else length
    first = max(lower, min(seed - WINDOW_LINES // 2, upper - WINDOW_LINES + 1))
    return first, min(upper, first + WINDOW_LINES - 1)


def raw_candidates(repository, pool, needs):
    wanted = [terms(need) for need in needs]
    union = set().union(*wanted)
    per_file, frequency = [], Counter()
    total_seeds, unsampled = 0, 0
    for file_rank, row in enumerate(pool, 1):
        path, lines = row["file"], repository.texts[row["file"]]
        seeds = seed_lines(len(lines))
        total_seeds += len(seeds)
        unsampled += len(lines) - len(seeds)
        covered = set()
        for at in row["locations"]:
            first, last = map(int, at.rpartition(":")[2].split("-"))
            covered.update(range(first, last + 1))
        line_terms = {}

        def term_set(n):
            if n not in line_terms:
                line_terms[n] = terms(lines[n - 1]) & union
            return line_terms[n]

        matches = []
        for seed in seeds:
            here = term_set(seed)
            frequency.update(here)
            if here and seed not in covered:
                nearby = set().union(*(term_set(n) for n in range(max(1, seed-2), min(len(lines), seed+2)+1)))
                matches.append((seed, here, nearby))
        starts = sorted({n for positions in repository.index["files"][path]["defines"].values() for n in positions})
        per_file.append((file_rank, path, lines, starts, matches))

    weights = {w: 1 + log((1 + total_seeds)/(1 + frequency[w])) for w in union}
    candidates = [[] for _ in needs]
    audit = []
    for file_rank, path, lines, starts, matches in per_file:
        for n, need in enumerate(wanted, 1):
            ranked = [(fsum(weights[w] for w in sorted(need & nearby)), seed)
                      for seed, here, nearby in matches if here & need]
            chosen = []
            for score, seed in sorted(ranked, key=lambda p: (-p[0], p[1])):
                first, last = unit_window(seed, starts, len(lines))
                if any(a <= last and b >= first for a, b in chosen):
                    continue
                source = SourceRange(path, file_rank, len(chosen)+1, first, tuple(lines[first-1:last]))
                candidates[n-1].append((score, source))
                audit.append({"need": n, "file": path, "at": source.at, "seed": seed})
                chosen.append((first,last))
                if len(chosen) == 2:
                    break
    return candidates, weights, {"generated":audit, "sampled_seed_lines":total_seeds, "unsampled_seed_lines":unsampled}


def nominate(repository, pool, args, pair_limit):
    """Alternate original and newly generated ranges per need; a shared source remains separately scoreable."""
    needs = args.get("needs") or [args["task"]]
    original = original_nominations(repository, pool, args, pair_limit)
    old_by_need = defaultdict(list)
    for row in original:
        old_by_need[row.need].append(row)
    candidates, weights, audit = raw_candidates(repository, pool, needs)
    combined = []
    for n, (need, scored) in enumerate(zip(needs, candidates), 1):
        lexical_rank = {}
        for rank, (score, _) in enumerate(sorted(scored, key=lambda p: -p[0]), 1):
            lexical_rank.setdefault(score, rank)
        def fused(pair):
            mass, source = pair
            return (-(1/(60+lexical_rank[mass]) + 1/(60+source.file_rank)), source.file_rank, source.range_rank)
        first, later, seen = [], [], set()
        for _, source in sorted(scored, key=fused):
            (later if source.path in seen else first).append(source)
            seen.add(source.path)
        query = f"Task: {args['task']}\nNeed: {need}\nExact strings: " + "\n".join(args.get("anchors", []))
        additional = _cards(repository, first+later, terms(need), weights, query, n)
        combined.append(round_robin((old_by_need[n], additional)))
    return list(islice(round_robin(combined), pair_limit)), audit
