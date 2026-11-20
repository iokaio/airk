# SPDX-License-Identifier: Apache-2.0
"""Enumerate original AWS answer occurrences for blinded evidence-set review.

Run under checked Python -I -B -S with --projection pointing to the first
source projection receipt. Only digests, source locations and review hints
are emitted. A word boundary or a lexical question match cannot label a span.
"""

import argparse
from bisect import bisect_right
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import re
import runpy
import time


PROJECTION = (57_682, "9a248fa25c34b0b0b5c4b9caf83a44b2127331971ac547f1885e0c7bc990bf9f")
PROJECTOR_SHA = "3977873dcfe12fbfc37b051634a6edebe81168a7146eedb410e672279a5e4b0d"
WORD = re.compile(r"\w+")
STOPWORDS = set("a an and are as at be by can do does for from how i in is it of on or that the to what which with".split())


def sha(data):
    return hashlib.sha256(data).hexdigest()


def terms(value):
    return set(WORD.findall(value.casefold())) - STOPWORDS


def word_character(value):
    return value.isalnum() or value == "_"


def occurrences(body, question, answer, projector):
    """Coordinates refer to raw bytes. Blank lines delimit review context."""
    if not question or not answer:
        raise ValueError("empty publisher question or answer")
    raw = body.encode("utf-8")
    needle = answer.encode("utf-8")
    question_terms = terms(question) - terms(answer)
    starts = [0, *(newline.end() for newline in re.finditer(b"\n", raw))]
    blanks = [i for i, start in enumerate(starts)
              if not raw[start:starts[i + 1] if i + 1 < len(starts) else len(raw)].strip()]
    hits = []
    for start in projector["positions"](raw, needle):
        end = start + len(needle)
        first, last = bisect_right(starts, start) - 1, bisect_right(starts, end - 1) - 1
        before = bisect_right(blanks, first - 1)
        after = bisect_right(blanks, last)
        context_start = starts[blanks[before - 1] + 1] if before else 0
        context_end = starts[blanks[after]] if after < len(blanks) else len(raw)
        line_end = starts[last + 1] if last + 1 < len(starts) else len(raw)
        lines = raw[starts[first]:line_end].decode("utf-8")
        # Inspect adjacent Unicode characters even when byte lengths differ.
        left = raw[:start].decode("utf-8")[-1:]
        right = raw[end:].decode("utf-8")[:1]
        bounded = not ((word_character(answer[0]) and word_character(left))
                       or (word_character(answer[-1]) and word_character(right)))
        context = raw[context_start:context_end]
        hits.append({
            "byte_start": start, "byte_end_exclusive": end,
            "line_start": first + 1, "line_end_inclusive": last + 1,
            "paragraph_byte_start": context_start, "paragraph_byte_end_exclusive": context_end,
            "paragraph_sha256": sha(context),
            "overlaps_markdown_fenced_code": projector["overlaps_fenced_block"](body, first + 1, last + 1),
            "word_bounded": bounded,
            "literal_question_in_paragraph": question.encode("utf-8") in context,
            "all_question_terms_on_answer_lines": len(question_terms) >= 2 and question_terms <= terms(lines),
        })
    return hits


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--projection", type=Path, required=True)
    parser.add_argument("--seconds", type=int, choices=range(1, 121), default=75, metavar="1..120")
    args = parser.parse_args()
    started = time.monotonic()
    deadline = started + args.seconds
    owner = Path(__file__).with_name("audit_aws_documentation_source_projection.py")
    if sha(owner.read_bytes()) != PROJECTOR_SHA:
        raise ValueError("AWS projection implementation pin differs")
    projector = runpy.run_path(str(owner))
    screen = owner.with_name("audit_aws_documentation_metadata.py")
    if sha(screen.read_bytes()) != projector["SCREEN_SOURCE_SHA"]:
        raise ValueError("AWS screening implementation pin differs")
    screening = runpy.run_path(str(screen))
    sealed = json.loads(projector["pinned_local"](args.projection, *PROJECTION))
    rows = [r for r in sealed["candidates"] if r["projection_status"] == "ambiguous_occurrences"]
    require = projector["require"]
    require(len(rows) == len({r["selector"]["row_index"] for r in rows}) == 24, "ambiguous roster differs")
    originals = screening["read_rows"](deadline)
    docs = {r["document"]["path"]: r["document"] for r in rows}
    with ThreadPoolExecutor(max_workers=8) as workers:
        jobs = {p: workers.submit(screening["inspect_document"], p, (d["git_blob_sha1"], d["bytes"]), deadline)
                for p, d in docs.items()}
        bodies = {p: job.result() for p, job in jobs.items()}
    results = []
    for row in rows:
        screening["check_deadline"](deadline)
        selector = row["selector"]
        original = originals[selector["row_index"]]
        question, answer = original["Question"], original["Answer_True"]
        _, path = screening["document_path"](original["Document_True"])
        body, digest = bodies[path]
        require(row["document"] == docs[path] and digest == row["document"]["sha256"]
                and selector["member"] == screening["MEMBER"]
                and sha(question.encode()) == row["question_sha256"]
                and row["match_type"] == "literal" and row["source_span"] is None,
                "ambiguous row source binding differs")
        hits = occurrences(body, question, answer, projector)
        require(len(hits) == row["occurrences_in_document"] > 1, "ambiguous occurrence count differs")
        results.append({
            "selector": selector, "document": docs[path], "question_sha256": row["question_sha256"],
            "answer_sha256": sha(answer.encode()), "locations": hits, "publisher_selected_range": None,
        })
    screening["check_deadline"](deadline)
    print(json.dumps({
        "schema": "airk.private.aws-repeated-answer-locations/1",
        "source_revision": screening["REVISION"], "source_csv_sha256": screening["CSV_PIN"][1],
        "prior_projection_sha256": PROJECTION[1], "collector_sha256": sha(Path(__file__).read_bytes()),
        "document_count": len(docs), "case_count": len(results),
        "occurrence_count": sum(len(r["locations"]) for r in results),
        "judged_sufficient": False, "native_visible_verified": False,
        "cases": results, "elapsed_seconds": time.monotonic() - started,
    }, sort_keys=True, separators=(",", ":"), allow_nan=False))


if __name__ == "__main__":
    main()
