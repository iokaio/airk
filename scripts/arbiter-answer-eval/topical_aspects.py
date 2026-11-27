# SPDX-License-Identifier: Apache-2.0
"""Research prototype: further aspects of the topical index.

Evaluation tooling for ``docs/arbiter/topical-index-research.md``. The user
chose these aspects on 27 November 2026 (plan, "Aspects"). Each is a pure
function over one file's path and text, or over the finished map, so it can
be tested alone and added to ``topical_index.py`` and ``live_gather.py``
without changing what a running cost series loads.

- test linkage (4): which source file a test file exercises;
- values and units (5): literal values with the name that holds them, and
  the same quantity in other units or in words;
- build and configuration keys (6): the keys a project, script or settings
  file defines;
- rules (7): the sections of a guidance file and the folder it governs;
- entry points and visibility (9): files a trace starts from, and public
  names;
- document-to-code links (10): the code names each document section
  mentions.

Nothing here reads a model or stores file text.
"""

from __future__ import annotations

import re

import topical_index

TEST_AFFIX = re.compile(r"^(?:test_|tests_)|(?:_test|_tests|Tests|Test|\.test|\.spec|\.tests|Spec)$")
NUMBER = r"\d[\d_]*(?:\.\d+)?"
ASSIGNED = re.compile(r"([A-Za-z_][A-Za-z0-9_]{2,})\s*(?::\s*[\w<>\[\]?]+\s*)?(?:=|:)\s*(?:TimeSpan\.From(\w+)\()?\s*(" + NUMBER + r")\b")
QUANTITY = re.compile(r"\b(" + NUMBER + r")\s?(ms|milliseconds?|s|secs?|seconds?|min|mins|minutes?|h|hrs?|hours?|d|days?|"
                      r"bytes?|kb|kib|mb|mib|gb|gib|%|percent|files?|items?|rows?|tokens?)\b", re.IGNORECASE)
SECONDS = {"ms": 0.001, "millisecond": 0.001, "s": 1, "sec": 1, "second": 1, "min": 60, "minute": 60, "h": 3600, "hr": 3600,
           "hour": 3600, "d": 86400, "day": 86400}
BYTES = {"byte": 1, "kb": 1024, "kib": 1024, "mb": 1024 ** 2, "mib": 1024 ** 2, "gb": 1024 ** 3, "gib": 1024 ** 3}
NAME_UNIT = [(re.compile(r"(?i)(?:^|_)(?:ms|millis|milliseconds)$|Ms$|Milliseconds$"), ("time", 0.001)),
             (re.compile(r"(?i)(?:^|_)(?:s|secs?|seconds)$|Secs?$|Seconds$"), ("time", 1)),
             (re.compile(r"(?i)(?:^|_)(?:mins?|minutes)$|Minutes$"), ("time", 60)),
             (re.compile(r"(?i)(?:^|_)hours$|Hours$"), ("time", 3600)),
             (re.compile(r"(?i)(?:^|_)days$|Days$"), ("time", 86400)),
             (re.compile(r"(?i)(?:^|_)bytes$|Bytes$"), ("size", 1))]
XML_KEY = re.compile(r"<([A-Za-z][\w.]{2,})(?:\s[^>]*)?>[^<\s]")
JSON_KEY = re.compile(r"^\s{0,8}\"([A-Za-z_][\w.:-]{2,})\"\s*:")
TOML_KEY = re.compile(r"^\s*(?:\[+([\w.-]+)\]+|([A-Za-z_][\w.-]{2,})\s*=)")
SCRIPT_KEY = re.compile(r"\[(?:string|int|switch|bool|long|double)(?:\[\])?\]\s*\$([A-Za-z]\w{2,})|^\s*function\s+([A-Za-z][\w-]{2,})|"
                        r"^\s*(?:export\s+)?([A-Z][A-Z0-9_]{3,})=")
ENTRY_NAMES = frozenset("""program.cs startup.cs main.rs lib.rs mod.rs main.py __main__.py app.py main.ts main.js index.ts
    index.js app.ts app.vue router.ts routes.ts routes.rs rest.rs main.go""".split())
PUBLIC = re.compile(r"\b(?:public|export|pub)\b")
ROUTE = re.compile(r"""\.route\(|\[Http(?:Get|Post|Put|Delete)|@(?:app|router)\.(?:get|post|put|delete)|MapGet\(|MapPost\(|Register<|AddScoped<|AddSingleton<""")
CODE_NAME = re.compile(r"`([A-Za-z_][\w.:]{2,60})(?:\(\))?`")


def subject_stems(test_path: str) -> list[str]:
    """Stems a test file's subject may carry: ``LogFileServiceTests.cs`` gives ``LogFileService``."""
    stem = test_path.rsplit("/", 1)[-1]
    stem = stem.split(".")[0] if stem.count(".") == 1 else ".".join(stem.split(".")[:-1])
    found, current = [], stem
    for _ in range(3):
        shorter = TEST_AFFIX.sub("", current)
        if shorter == current or not shorter:
            break
        found.append(shorter)
        current = shorter
    return found


def test_links(files: dict) -> dict[str, list[str]]:
    """Source path to the test files that exercise it, by name: the same stem, or a stem the test's stem starts with."""
    stems: dict[str, list[str]] = {}
    for path, record in files.items():
        if record["role"] in ("source", "configuration"):
            stems.setdefault(path.rsplit("/", 1)[-1].split(".")[0].lower(), []).append(path)
    links: dict[str, list[str]] = {}
    for path, record in files.items():
        if record["role"] != "test":
            continue
        for stem in subject_stems(path):
            for subject in stems.get(stem.lower(), []):
                links.setdefault(subject, []).append(path)
            if stem.lower() in stems:
                break
    return {subject: sorted(tests) for subject, tests in sorted(links.items())}


def unit_of(name: str) -> tuple[str, float] | None:
    for pattern, unit in NAME_UNIT:
        if pattern.search(name):
            return unit
    return None


def values_of(text: str) -> list[dict]:
    """Named literal values of a file: the name, its line, the number, and the quantity in a base unit when the
    name or a wrapper says the unit (``MAX_TTL_SECS = 86_400`` is 86,400 seconds of time)."""
    found = []
    for number, line in enumerate(text.splitlines(), 1):
        if len(line) > 300:
            continue
        for match in ASSIGNED.finditer(line):
            name, wrapper, literal = match.groups()
            value = float(literal.replace("_", ""))
            unit = unit_of(wrapper or "") or unit_of(name)
            row = {"name": name, "line": number, "value": value}
            if unit:
                row["kind"], row["base"] = unit[0], value * unit[1]
            found.append(row)
    return found


def quantities_of(text: str) -> list[dict]:
    """Quantities a text states in words (``24 h``, ``10 days``, ``500 files``), as a kind and a base amount."""
    found = []
    for number, line in enumerate(text.splitlines(), 1):
        for match in QUANTITY.finditer(line):
            value, unit = float(match.group(1).replace("_", "")), match.group(2).lower().rstrip("s") or "s"
            unit = {"percent": "%", "hr": "h"}.get(unit, unit)
            if unit in SECONDS:
                found.append({"line": number, "kind": "time", "base": value * SECONDS[unit], "text": match.group(0)})
            elif unit in BYTES:
                found.append({"line": number, "kind": "size", "base": value * BYTES[unit], "text": match.group(0)})
            else:
                found.append({"line": number, "kind": unit, "base": value, "text": match.group(0)})
    return found


def config_keys(path: str, text: str) -> dict[str, list[int]]:
    """Keys a project, settings or script file defines, with their lines."""
    name = path.rsplit("/", 1)[-1].lower()
    extension = name.rsplit(".", 1)[-1] if "." in name else ""
    pattern = (XML_KEY if extension in {"xml", "csproj", "fsproj", "vbproj", "wixproj", "props", "targets", "config", "wxs"}
               else JSON_KEY if extension == "json" else TOML_KEY if extension in {"toml", "ini", "cfg", "yaml", "yml", "env"}
               else SCRIPT_KEY if extension in {"ps1", "sh", "bash", "cmd", "bat"} else None)
    found: dict[str, list[int]] = {}
    if pattern is None:
        return found
    for number, line in enumerate(text.splitlines(), 1):
        if len(line) > 300:
            continue
        for match in pattern.finditer(line):
            key = next((group for group in match.groups() if group), None)
            if key and len(found) < 400:
                found.setdefault(key, []).append(number)
    return found


def sections_of(text: str) -> list[dict]:
    """A document's sections: heading, first and last line, and the words of the heading and body."""
    lines = text.splitlines()
    heads = [(number, match.group(1).strip()) for number, line in enumerate(lines, 1) if (match := topical_index.HEADING.match(line))]
    if not heads or heads[0][0] > 1:
        heads.insert(0, (1, ""))
    sections = []
    for position, (start, heading) in enumerate(heads):
        end = heads[position + 1][0] - 1 if position + 1 < len(heads) else len(lines)
        body = "\n".join(lines[start - 1:end])
        sections.append({"heading": heading, "start": start, "end": max(end, start),
                         "words": sorted(set(topical_index.words_of(body)))[:400],
                         "names": sorted({name.split(".")[-1].split(":")[-1] for name in CODE_NAME.findall(body)})[:80]})
    return sections


def governing(path: str, files: dict) -> list[str]:
    """Guidance files that govern ``path``: those in its folder and every folder above it, nearest first."""
    folder = path.rsplit("/", 1)[0] if "/" in path else ""
    found = []
    while True:
        prefix = folder + "/" if folder else ""
        found += sorted(candidate for candidate, record in files.items()
                        if record["role"] == "guidance" and candidate.startswith(prefix) and "/" not in candidate[len(prefix):])
        if not folder:
            return found
        folder = folder.rsplit("/", 1)[0] if "/" in folder else ""


def entry_point(path: str, text: str) -> bool:
    """Whether a trace is likely to start in this file: a conventional entry name, or a table of routes or registrations."""
    return path.rsplit("/", 1)[-1].lower() in ENTRY_NAMES or len(ROUTE.findall(text)) >= 4


def public_names(text: str, defines: dict[str, list[int]]) -> list[str]:
    """Defined names whose defining line marks them public."""
    lines = text.splitlines()
    return sorted(name for name, numbers in defines.items()
                  if any(number <= len(lines) and PUBLIC.search(lines[number - 1]) for number in numbers))
