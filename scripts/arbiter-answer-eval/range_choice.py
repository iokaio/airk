# SPDX-License-Identifier: Apache-2.0
"""Nominate and score individual evidence ranges within a fixed single-Arbiter pair budget."""
from collections import Counter, defaultdict
from dataclasses import dataclass
from itertools import islice, zip_longest
from math import fsum, log

from arbiter_loop_prototype import PLAIN_WORDS
from qwen_file_trial import EXCERPT_CHARS, reorder
from qwen_need_trial import DIRECT
from topical_index import words_of


@dataclass(frozen=True)
class SourceRange:
    path: str
    file_rank: int
    range_rank: int
    first: int
    lines: tuple[str, ...]

    @property
    def at(self):
        return f"{self.path}:{self.first}-{self.first + len(self.lines) - 1}"


@dataclass(frozen=True)
class Nomination:
    need: int
    source: SourceRange
    scored_at: str
    query: str
    document: str


def terms(text):
    return set(words_of(text)) - PLAIN_WORDS


def round_robin(rosters):
    for group in zip_longest(*rosters):
        yield from (row for row in group if row is not None)


def source_ranges(repository, pool):
    ranges = []
    seen = set()
    for file_rank, row in enumerate(pool, 1):
        path = row["file"]
        for range_rank, at in enumerate(row["locations"], 1):
            origin, interval = at.rsplit(":", 1)
            first, last = map(int, interval.split("-"))
            if origin != path or first < 1 or last < first or last > len(repository.texts[path]):
                raise ValueError(f"Invalid nominated source range: {at}")
            if at in seen:
                raise ValueError(f"Repeated source range: {at}")
            seen.add(at)
            lines = tuple(repository.texts[path][first - 1:last])
            ranges.append(SourceRange(path, file_rank, range_rank, first, lines))
    return ranges


def excerpt(source, wanted, idf):
    """Maximize distinct query term mass within complete source lines and a fixed character cap."""
    line_terms = [wanted.intersection(terms(line)) for line in source.lines]
    best_key, best = None, None
    for start, first_line in enumerate(source.lines):
        if not first_line.strip():
            continue
        seen, size = set(), 0
        for end in range(start, len(source.lines)):
            size += len(source.lines[end]) + int(end > start)
            if size > EXCERPT_CHARS:
                break
            seen.update(line_terms[end])
            key = (fsum(idf[word] for word in sorted(seen)), -start, size)
            if best_key is None or key > best_key:
                best_key, best = key, (start, end)
    return best


def nominate(repository, pool, arguments, pair_limit):
    """Select a prefix of the need-round-robin proposal sequence without consulting a model or references."""
    needs = arguments.get("needs") or [arguments["task"]]
    if pair_limit not in (40, 80) or not 1 <= len(needs) <= 8:
        raise ValueError("Expected 1–8 needs and 40 or 80 pairs")
    ranges = source_ranges(repository, pool)
    bags = [terms(source.path) | terms("\n".join(source.lines)) for source in ranges]
    counts = Counter(word for bag in bags for word in bag)
    idf = {word: 1.0 + log((1 + len(ranges)) / (1 + n)) for word, n in counts.items()}
    rosters = []
    for need_index, need in enumerate(needs, 1):
        wanted = terms(need)
        masses = [fsum(idf[w] for w in sorted(wanted & bag)) for bag in bags]
        ordered = sorted(range(len(ranges)), key=lambda i: (-masses[i], i))
        # Tied lexical masses have the same competition rank (especially all-zero matches).
        lexical_rank = {}
        for place, i in enumerate(ordered, 1):
            lexical_rank.setdefault(masses[i], place)
        def fused_rank(i):
            source = ranges[i]
            score = 1 / (60 + source.file_rank) + 1 / (60 + lexical_rank[masses[i]])
            return -score, source.file_rank, source.range_rank
        first, later, files = [], [], set()
        for i in sorted(ordered, key=fused_rank):
            source = ranges[i]
            (later if source.path in files else first).append(source)
            files.add(source.path)
        query = f"Task: {arguments['task']}\nNeed: {need}\nExact strings: " + "\n".join(arguments.get("anchors", []))
        rosters.append(_cards(repository, first + later, wanted, idf, query, need_index))
    return list(islice(round_robin(rosters), pair_limit))


def _cards(repository, ranges, wanted, idf, query, need_index):
    for source in ranges:
        bounds = excerpt(source, wanted, idf)
        if bounds is None:
            continue
        offset, last = bounds
        begin, end = source.first + offset, source.first + last
        names = [name for name, ns in sorted(repository.index["files"][source.path]["defines"].items())
                 if any(begin <= n <= end and name in source.lines[n - source.first] for n in ns)]
        declaration = "\nDeclaration: " + ", ".join(names) if names else ""
        text = "\n".join(source.lines[offset:last + 1])
        doc = f"Path: {source.path}\nLines: {begin}-{end}{declaration}\nSource:\n{text}"
        yield Nomination(need_index, source, f"{source.path}:{begin}-{end}", query, doc)


def assemble(needs, nominations, scores, *, byte_limit=14000, file_limit=6):
    """Each need offers its next scored range in turn. Identical source ranges consume bytes only once."""
    if byte_limit < 0 or file_limit < 0 or len(nominations) != len(scores):
        raise ValueError("Invalid evidence allocation")
    groups = defaultdict(list)
    for i, item in enumerate(nominations):
        groups[item.need].append(i)
    rosters = [reorder(indices, [scores[i] for i in indices]) for n in range(1, len(needs) + 1)
               if (indices := groups[n])]
    paths, delivered, provenances, omissions = set(), {}, [], []
    size = 0
    for i in round_robin(rosters):
        item = nominations[i]
        source, n = item.source, item.need
        if source.at not in delivered:
            text = "\n".join(source.lines) + "\n"
            extra = len(text.encode("utf-8"))
            reason = ("file_limit" if source.path not in paths and len(paths) >= file_limit
                      else "byte_limit" if size + extra > byte_limit else None)
            if reason:
                omissions.append({"need": n, "at": source.at, "reason": reason})
                continue
            size += extra
            paths.add(source.path)
            delivered[source.at] = {"at": source.at, "text": text, "needs": []}
        row = delivered[source.at]
        row["needs"].append(n)
        provenances.append({"need": n, "scored_at": item.scored_at, "returned_at": source.at})
    passages = list(delivered.values())
    evidence = []
    for n, need in enumerate(needs, 1):
        locations = [{"at": p["at"]} for p in passages if n in p["needs"]]
        evidence.append({"need": n, "text": need, "where": locations,
                         "status": "candidate_evidence" if locations else "no_candidate_evidence"})
    return {"passages": passages, "reply_needs": evidence, "scored_support": provenances,
            "passage_bytes": size, "budget": byte_limit, "omissions": omissions}
