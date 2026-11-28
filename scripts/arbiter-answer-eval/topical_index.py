# SPDX-License-Identifier: Apache-2.0
"""Research prototype: a topical index of files, built without embeddings.

Evaluation tooling for ``docs/arbiter/topical-index-research.md`` (user
direction, 27 November 2026). It is a prototype for measuring an idea and
not product code: one small record per file says what the file is and what
it defines, so a request can find *which files* in one lookup and then read
the live files (``live_gather.py``). File contents are not stored.

Input: a repository folder. Output: a JSON map with, per text file, its
lines, size, role, the names it defines with their lines, and its area
words (from its path and the names it defines); and two inverted tables,
word to names and word to files.

    python -B topical_index.py build --source <repository folder> --output <map.json>
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import stat
import sys
import time

SCHEMA = "airk.private.topical-index/1"
SKIP_FOLDERS = frozenset({".git", "node_modules", "bin", "obj", "target", "dist", "build", "out", ".vs", ".idea", "__pycache__",
                          ".venv", "venv", "wwwroot", "coverage", ".next", "vendor", "packages"})
BINARY = frozenset("""png jpg jpeg gif ico bmp webp svgz pdf docx doc xlsx xls pptx ppt zip gz tar 7z rar exe dll pdb so dylib
    bin obj class jar woff woff2 ttf otf eot mp3 mp4 mov avi wav db sqlite lock snap nupkg msi cab onnx safetensors""".split())
MAX_FILE_BYTES = 1_500_000
CONFIG = frozenset("json toml yaml yml ini cfg conf config props targets csproj fsproj vbproj wixproj sln xml env editorconfig".split())
DOCUMENT = frozenset("md markdown txt rst adoc".split())
BUILD = frozenset("ps1 sh bash cmd bat mk gradle cmake dockerfile".split())
GUIDANCE = ("CLAUDE.md", "AGENTS.md", "CONTRIBUTING.md")

DEFINE = [
    re.compile(r"\b(?:class|interface|struct|enum|record|trait|type|namespace|module|def|fn|func|function|impl|message|service|rpc)"
               r"\s+([A-Za-z_]\w{2,})"),
    re.compile(r"\b(?:public|private|protected|internal|static|async|override|virtual|sealed|abstract|readonly|export|pub|const)\b"
               r"[\w<>\[\],.?:& ]*?\b([A-Za-z_]\w{2,})\s*(?:\(|=|:|\{|;|=>)"),
    re.compile(r"^\s*(?:const|let|var|val|final|static)\s+(?:mut\s+)?([A-Za-z_]\w{2,})\s*[=:]"),
    re.compile(r"^\s*([A-Z][A-Z0-9_]{3,})\s*[=:]"),
    re.compile(r"^\s*(?:async\s+)?([A-Za-z_]\w{2,})\s*\([^()]*\)\s*(?::\s*[\w<>\[\]|. ]+)?\s*\{\s*$"),
    re.compile(r"^\s*['\"]?([A-Za-z_][\w.-]{3,})['\"]?\s*:\s*(?:async\s*)?(?:\(|function\b|\{)"),
    re.compile(r"^\s*(?:it|test|describe)\(\s*['\"]([^'\"]{4,80})"),
    re.compile(r"^\s*\[(?:Fact|Theory|Test|TestMethod)[^\]]*\]\s*$"),
]
HEADING = re.compile(r"^\s{0,3}#{1,6}\s+(.{2,100}?)\s*#*\s*$")
KEYWORDS = frozenset("""return public private protected internal static async await const class interface struct function
    override virtual sealed abstract readonly export import from default this that with null true false void string number
    boolean object using namespace module where when else then self super new var let val final type impl trait enum
    record throw throws catch try finally for while switch case break continue yield get set init value task list dict""".split())
WORD = re.compile(r"[A-Z]+(?![a-z])|[A-Z]?[a-z]+|\d+")


def words_of(name: str) -> list[str]:
    """The lower-case words of an identifier, path or phrase: camelCase, snake_case and separators are split."""
    return [word.lower() for part in re.split(r"[^A-Za-z0-9]+", name) for word in WORD.findall(part) if len(word) >= 3]


def role_of(path: str) -> str:
    lower, name = path.lower(), path.rsplit("/", 1)[-1].lower()
    extension = name.rsplit(".", 1)[-1] if "." in name else ""
    if name in {item.lower() for item in GUIDANCE}:
        return "guidance"
    if re.search(r"(^|/)(tests?|__tests__|spec|specs|conformance)(/|$)", lower) or re.search(r"(\.|_|-)(test|spec)\.|tests?\.(cs|py|rs|java)$", name):
        return "test"
    if "mock" in name or "/mocks/" in lower or "/fixtures/" in lower:
        return "mock"
    if extension in DOCUMENT or "/docs/" in lower or lower.startswith("docs/"):
        return "document"
    if extension in BUILD or "/.github/" in lower or lower.startswith(".github/") or name in {"makefile", "dockerfile"}:
        return "build"
    if extension in CONFIG or name in {"cargo.toml", "package.json", "pyproject.toml"}:
        return "configuration"
    return "source"


def text_files(source: pathlib.Path):
    """Every text file under ``source`` that a request may read live, with its repository-relative path."""
    stack = [source]
    while stack:
        folder = stack.pop()
        for entry in sorted(folder.iterdir()):
            info = entry.lstat()
            if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT:
                continue
            if stat.S_ISDIR(info.st_mode):
                if entry.name not in SKIP_FOLDERS:
                    stack.append(entry)
                continue
            extension = entry.name.rsplit(".", 1)[-1].lower() if "." in entry.name else ""
            if not stat.S_ISREG(info.st_mode) or extension in BINARY or info.st_size > MAX_FILE_BYTES:
                continue
            yield entry.relative_to(source).as_posix(), entry


def read_text(path: pathlib.Path) -> str | None:
    data = path.read_bytes()
    if b"\0" in data[:4096]:
        return None
    return data.decode("utf-8", errors="replace")


def defines_of(text: str, document: bool) -> dict[str, list[int]]:
    """Names a file defines, with the lines that define them. Patterns only; no parser."""
    found: dict[str, list[int]] = {}
    pending_test = False
    for number, line in enumerate(text.splitlines(), 1):
        if len(line) > 400:
            continue
        if document:
            heading = HEADING.match(line)
            if heading:
                found.setdefault(heading.group(1).strip(), []).append(number)
            continue
        for index, pattern in enumerate(DEFINE):
            match = pattern.search(line)
            if not match:
                continue
            if index == len(DEFINE) - 1:
                pending_test = True
                break
            name = match.group(1)
            if name.lower() not in KEYWORDS:
                found.setdefault(name, []).append(number)
            break
        else:
            if pending_test and line.strip():
                pending_test = False
    return found


def file_record(path: str, text: str, size: int) -> dict:
    role = role_of(path)
    return {"lines": text.count("\n") + 1, "bytes": size, "role": role,
            "defines": defines_of(text, role in ("document", "guidance")),
            "path_words": sorted(set(words_of(path)))}


def assemble(source: pathlib.Path, files: dict, started: float) -> dict:
    word_names, word_files = {}, {}
    for path, record in files.items():
        area = set(record["path_words"])
        for name in record["defines"]:
            for word in words_of(name):
                area.add(word)
                word_names.setdefault(word, set()).add(name)
        for word in area:
            word_files.setdefault(word, set()).add(path)
    return {"schema": SCHEMA, "source": str(source), "seconds": round(time.monotonic() - started, 2), "files": files,
            "word_names": {word: sorted(names) for word, names in sorted(word_names.items())},
            "word_files": {word: sorted(paths) for word, paths in sorted(word_files.items())}}


def build(source: pathlib.Path) -> dict:
    started = time.monotonic()
    files = {}
    for path, entry in text_files(source):
        text = read_text(entry)
        if text is not None:
            files[path] = file_record(path, text, entry.stat().st_size)
    return assemble(source, files, started)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=("build",))
    parser.add_argument("--source", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    args = parser.parse_args(argv)
    index = build(args.source.resolve(strict=True))
    data = json.dumps(index, ensure_ascii=False, separators=(",", ":"))
    args.output.write_text(data, encoding="utf-8", newline="\n")
    names = sum(len(record["defines"]) for record in index["files"].values())
    print(json.dumps({"files": len(index["files"]), "defined_names": names, "words": len(index["word_names"]),
                      "map_bytes": len(data.encode("utf-8")), "seconds": index["seconds"]}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
