# SPDX-License-Identifier: Apache-2.0
"""Normalize the two development walkthroughs as evaluator-only observations."""

import re


_CASE = re.compile(r"^#### (Bookbuilder|Munarium) ([CWTA]\d)\b")
_HEADING = re.compile(r"^#{1,4} ")
_OPENAI_STEP = re.compile(r"^\*\*(\d+)\.(?:\*\*| .+?\*\*)")
_ANTHROPIC_STEP = re.compile(r"^(\d+)\. \*\*Turn (\d+)\b")
_MARKER = re.compile(r"\*\*Found\.\*\*|\*(?:Expect|Got):\*")
_FENCE = re.compile(r"(?ms)^(?P<mark>~~~|\x60\x60\x60)(?P<language>[^\n]*)\n"
                    r"(?P<text>.*?)^(?P=mark)[ \t]*$")


def _source(lines: list[str], start: int, end: int) -> dict:
    while end > start and not lines[end - 1].strip():
        end -= 1
    return {"document_lines": [start + 1, end], "source_wording": "\n".join(lines[start:end])}


def _step(lines: list[str], start: int, end: int, number: int) -> dict:
    step = _source(lines, start, end)
    body = step["source_wording"]
    commands = []
    for match in _FENCE.finditer(body):
        first_line = start + body.count("\n", 0, match.start("text")) + 1
        command_text = match["text"].removesuffix("\n")
        commands.append({
            "language": match["language"].strip(),
            "document_lines": [first_line, first_line + command_text.count("\n")],
            "source_wording": command_text,
        })
    observations = []
    markers = list(_MARKER.finditer(body))
    for index, match in enumerate(markers):
        limit = markers[index + 1].start() if index + 1 < len(markers) else len(body)
        prose = re.split(r"\n[ \t]*[-+*] |\n\s*\n", body[match.end():limit], maxsplit=1)[0].strip()
        observations.append({
            "kind": "expected" if match.group() == "*Expect:*" else "observed",
            "document_line": start + body.count("\n", 0, match.start()) + 1,
            "source_wording": prose,
        })
    step.update(number=number, commands=commands, observations=observations)
    return step


def parse_traces(author: str, text: str) -> dict[tuple[str, str], dict]:
    """Parse case headings and numbered groups, preserving source Markdown.

    Coordinates refer to the walkthrough document. Commands, summaries and
    numbers embedded in them do not confer source-range coverage.
    """
    if author == "openai":
        step_pattern = _OPENAI_STEP
        handoff_marker, stop_marker = "**Handoff.**", "**Handoff.**"
    elif author == "anthropic":
        step_pattern = _ANTHROPIC_STEP
        handoff_marker, stop_marker = "**Context in hand.**", "**Stop.**"
    else:
        raise ValueError(f"Unknown walkthrough author {author!r}")

    lines = text.splitlines()
    headings = [i for i, line in enumerate(lines) if _HEADING.match(line)] + [len(lines)]
    cases = {}
    for start, end in zip(headings, headings[1:]):
        header = _CASE.match(lines[start])
        if not header:
            continue
        key = (header[1].lower(), header[2])
        label = f"{author}/{key[0]}-{key[1]}"
        if key in cases:
            raise ValueError(f"Duplicate walkthrough case {label}")

        quote_start = start + 1
        while quote_start < end and not lines[quote_start].strip():
            quote_start += 1
        quote_end = quote_start
        while quote_end < end and lines[quote_end].startswith(">"):
            quote_end += 1
        quote = _source(lines, quote_start, quote_end)
        instruction = " ".join(" ".join(line[1:] for line in lines[quote_start:quote_end]).split())
        if not instruction:
            raise ValueError(f"Missing walkthrough instruction in {label}")

        steps = []
        handoffs = []
        stops = []
        for i in range(quote_end, end):
            match = step_pattern.match(lines[i])
            if match:
                number = int(match[1])
                if number != len(steps) + 1 or (author == "anthropic" and int(match[2]) != number):
                    raise ValueError(f"Nonconsecutive walkthrough step in {label} at line {i + 1}")
                steps.append(i)
            if lines[i].startswith(handoff_marker):
                handoffs.append(i)
            if lines[i].startswith(stop_marker):
                stops.append(i)
        if not steps or len(handoffs) != 1 or len(stops) != 1 or not steps[-1] < handoffs[0] <= stops[0]:
            raise ValueError(f"Missing or misplaced steps, handoff or stop in {label}")

        introduction_start = quote_end
        while introduction_start < steps[0] and not lines[introduction_start].strip():
            introduction_start += 1
        handoff_end = stops[0] if author == "anthropic" else end
        handoff = _source(lines, handoffs[0], handoff_end)
        stop = _source(lines, stops[0], end)
        cases[key] = {
            "author": author,
            "instruction": instruction,
            "instruction_source": quote,
            "prerequisite": (_source(lines, introduction_start, steps[0])
                             if introduction_start < steps[0] else None),
            "steps": [_step(lines, i, j, number) for number, (i, j) in
                      enumerate(zip(steps, steps[1:] + handoffs), 1)],
            "handoff": handoff,
            "stop": stop,
            "proof_limits": {
                "source_range_coverage": "requires_independent_verification",
                "byte_cost": "unrecorded",
                "closing_qualification": stop["source_wording"],
            },
        }
    if not cases:
        raise ValueError(f"No walkthrough cases found for {author}")
    return cases
