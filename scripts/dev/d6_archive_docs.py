# SPDX-License-Identifier: Apache-2.0
"""Step D6a: move dated documentation records into docs/history/
(docs/simplify-plan.md sections 4.5 and 9, decision D6).

MOVES is a reviewed, literal list of top-level documents that are dated
reviews, superseded plans, phase logs, spikes or experiments. Each keeps its
file name under docs/history/. PINNED names files and directories that must
never move: governing plans, release and research records, documents that
published listings link to, and documents named by code, tests or scripts.

Every edit is computed against the OLD locations before any file moves, and
works on raw bytes, so CRLF line endings, byte-order marks and non-UTF-8
files (for example the cp1252 docs/releases/release-1.4.0.md) are kept.
Only the ASCII link span that changes is replaced:

- relative links in every tracked .md/.html file: inline `](target)` and
  images, reference definitions `[label]: target`, and `href="target"` /
  `src="target"`. Anchors are kept and scheme URLs are skipped. Links from
  moved files and links between moved files are adjusted too;
- https://github.com/iokaio/airk/(blob|tree)/<ref>/docs/<old> URLs whose ref
  is a branch. Commit SHAs and tags name a fixed tree and are kept;
- exact repository-relative path strings in other tracked text files.

Markdown prose that names a moved file is left unchanged, because it is
often dated history; --plan lists it. docs/history/README.md is generated.

    python scripts/dev/d6_archive_docs.py --plan     # dry run: list moves and edits
    python scripts/dev/d6_archive_docs.py --apply    # edits, git mv, generated index
    python scripts/dev/d6_archive_docs.py --check    # exit 1 on any inconsistency
"""
import argparse
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
import posixpath
import re
import subprocess
import sys
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[2]
HISTORY = "docs/history"
INDEX = f"{HISTORY}/README.md"

# Reviewed for D6a: each has a dated title or opening, or a Historical,
# superseded or proposed marker, and no current guidance presents it as
# current. Nothing outside markdown/HTML names any of them.
MOVES = [
    "docs/additional-languages-plan.md",
    "docs/agent-tool-alignment.md",
    "docs/airk-feature-review.md",
    "docs/airkimprovements.md",
    "docs/airkservice-architecture.md",
    "docs/antigravity-review.md",
    "docs/architecture-v2-ledger.md",
    "docs/architecture-v2.md",
    "docs/change-tracing-plan.md",
    "docs/current-index-foundation.md",
    "docs/extension-content.md",
    "docs/index-self-healing-proposal.md",
    "docs/index-sharding-review.md",
    "docs/indexing-follow-up-2026-10-21.md",
    "docs/indexing-speed-review-2026-10-21.md",
    "docs/indexingenhancements.md",
    "docs/language-gaps.html",
    "docs/mcp-improvements-test-results.md",
    "docs/mcp-prioritization.md",
    "docs/mcp-recommendations.md",
    "docs/optimizations.md",
    "docs/review-0.1.0.md",
    "docs/service-cutover-qualification.md",
    "docs/service-implementation-checklist.md",
    "docs/service-macos-login.md",
    "docs/service-publication-spike.md",
    "docs/service-requirements-review.md",
    "docs/storage-reliability.md",
    "docs/unreleased-mcp-tool-retirement.md",
    "docs/v2-adapter-inventory.md",
    "docs/v2-baseline-measurements.md",
    "docs/v2-final-qualification.md",
    "docs/v2-history-controls.md",
    "docs/v2-history-storage.md",
    "docs/v2-identity-contract.md",
    "docs/v2-incremental-catalog-experiment.md",
    "docs/v2-incremental-lexical-experiment.md",
    "docs/v2-lifecycle-inventory.md",
    "docs/v2-publication-contract.md",
    "docs/v2-similarity-fixtures.md",
    "docs/validation.md",
]

# Never moved. A trailing slash names a directory. docs/reviews/,
# docs/issue-analysis/ and docs/explorer/ are step D6b, deferred.
PINNED = [
    "AGENTS.md",
    "CLAUDE.md",
    "CONTRIBUTING.md",
    "README.md",
    "docs/README.md",
    "docs/architecture.md",
    "docs/cli-reference.md",
    "docs/dependency-advisories.md",
    "docs/explorer/",
    "docs/inspiration.md",
    "docs/issue-analysis/",
    "docs/macos-build.md",
    "docs/native-signing.md",
    "docs/one-airk-completion-plan.md",
    "docs/one-airk.md",
    "docs/operations.md",
    "docs/parser-training.md",
    "docs/performance-improvements-1.8.2.md",  # published Marketplace descriptions link here
    "docs/release-verification.md",
    "docs/releases/",
    "docs/research/",
    "docs/reviews/",
    "docs/simplify-plan.md",
    "docs/simplify/",
    "docs/upgrade-rebuild-fallback.md",
    "docs/usage-evaluation.md",
]

# Files this step must not edit: generated or captured descriptions of
# another tree. An edit needed there stops --apply.
PROTECTED = ("docs/cli-reference.md", "docs/simplify/records/", "crates/")
SELF = ("scripts/dev/d6_archive_docs.py", "scripts/tests/test_d6_archive_docs.py")
# Reviewed targets that are not repository files, with the reason.
UNRESOLVED_OK = {
    # scripts/build-downloads.py copies these images beside the built page.
    ("site/index.html", "./images/workbench-1.7.1-results.png"),
    ("site/index.html", "./images/explorer-1.7.1.png"),
}

LINK_SUFFIXES = {".md", ".html"}
TEXT_SUFFIXES = {".rs", ".py", ".ts", ".tsx", ".cjs", ".js", ".mjs", ".json", ".toml", ".yml", ".yaml",
                 ".ps1", ".sh", ".txt", ".cfg", ".ini", ".css", ".svg"}
SCHEME = re.compile(rb"^[A-Za-z][A-Za-z0-9+.-]*:")
INLINE = re.compile(rb"\]\(\s*(<[^>\r\n]*>|[^)\s]+)")
REFDEF = re.compile(rb"^[ ]{0,3}\[[^\]\r\n]+\]:[ \t]*(<[^>\r\n]*>|\S+)", re.MULTILINE)
ATTR = re.compile(rb"""\b(?:href|src)\s*=\s*(?:"([^"]*)"|'([^']*)')""", re.IGNORECASE)
GITHUB = re.compile(rb"https://github\.com/iokaio/airk/(?:blob|tree)/([^\s)\"'<>]+?)/(docs/[^\s)\"'<>#?]*)")
SHA = re.compile(r"^[0-9a-f]{7,40}$")
VERSION_TAG = re.compile(r"^v?\d+\.\d+")


def new_path(old):
    return f"{HISTORY}/{posixpath.basename(old)}"


def pinned(path):
    return any(path == item or (item.endswith("/") and path.startswith(item)) for item in PINNED)


def git(root, *args):
    return subprocess.run(["git", *args], cwd=root, capture_output=True, check=True).stdout.decode("utf-8")


def tracked(root):
    return git(root, "ls-files", "-z").split("\0")[:-1]


def tags(root):
    return set(git(root, "tag", "--list").split())


def is_branch(ref, tag_names):
    return not (SHA.match(ref) or ref in tag_names or VERSION_TAG.match(ref))


def relative(from_file, to_path):
    return posixpath.relpath(to_path, posixpath.dirname(from_file) or ".")


@dataclass
class Plan:
    moves: dict                                          # old -> new
    edits: dict = field(default_factory=dict)           # current path -> [(start, end, bytes)]
    counts: Counter = field(default_factory=Counter)    # rewrite kind -> count
    prose: list = field(default_factory=list)           # (path, line, name)
    errors: list = field(default_factory=list)


def link_targets(data, suffix):
    """(kind, start, end) spans of link targets in a .md or .html file."""
    spans = []
    if suffix == ".md":
        for match in INLINE.finditer(data):
            spans.append(("inline", match.start(1), match.end(1)))
        for match in REFDEF.finditer(data):
            spans.append(("reference", match.start(1), match.end(1)))
    for match in ATTR.finditer(data):
        group = 1 if match.group(1) is not None else 2
        spans.append(("attribute", match.start(group), match.end(group)))
    return spans


def split_target(raw):
    """(path, suffix) of an ASCII link target, or None when it is not a relative file link."""
    if raw.startswith(b"<") and raw.endswith(b">"):
        raw = raw[1:-1]
    if not raw or raw.startswith((b"#", b"/")) or SCHEME.match(raw):
        return None
    try:
        text = raw.decode("ascii")
    except UnicodeDecodeError:
        return None
    cut = min((index for index in (text.find("#"), text.find("?")) if index >= 0), default=len(text))
    return text[:cut], text[cut:]


def resolve(from_file, path):
    return posixpath.normpath(posixpath.join(posixpath.dirname(from_file), unquote(path)))


def build_plan(root, moves=None):
    moves = {old: new_path(old) for old in (moves if moves is not None else MOVES)}
    plan = Plan(moves)
    for old, new in moves.items():
        if pinned(old):
            plan.errors.append(f"move touches pinned path {old}")
        if not (root / old).is_file():
            plan.errors.append(f"missing source {old}")
        if (root / new).exists():
            plan.errors.append(f"destination exists {new}")
    files = tracked(root)
    tag_names = tags(root)
    names = {posixpath.basename(old): old for old in moves}
    path_strings = re.compile(rb"(?<![\w./-])(" + b"|".join(re.escape(old.encode()) for old in sorted(moves, key=len, reverse=True)) + rb")(?![\w/-]|\.\w)") if moves else None
    mention = re.compile(rb"(?<![\w.-])(" + b"|".join(re.escape(name.encode()) for name in sorted(names, key=len, reverse=True)) + rb")(?![\w-]|\.\w)") if names else None
    for path in files:
        suffix = posixpath.splitext(path)[1].lower()
        if suffix not in LINK_SUFFIXES and suffix not in TEXT_SUFFIXES:
            continue
        if path in SELF or not (root / path).is_file():
            continue
        data = (root / path).read_bytes()
        final = moves.get(path, path)
        edits, covered = [], []
        if suffix in LINK_SUFFIXES:
            for kind, start, end in link_targets(data, suffix):
                covered.append((start, end))
                parts = split_target(data[start:end])
                if parts is None:
                    continue
                target, tail = parts
                if not target:
                    continue
                resolved = resolve(path, target)
                if resolved.startswith("../"):
                    continue
                destination = moves.get(resolved, resolved)
                if destination == resolved and final == path:
                    continue
                updated = relative(final, destination)
                if target.endswith("/") and not updated.endswith("/"):
                    updated += "/"
                if posixpath.normpath(updated) == posixpath.normpath(unquote(target)):
                    continue
                raw = data[start:end]
                inner_start = start + 1 if raw.startswith(b"<") else start
                edits.append((inner_start, inner_start + len(target.encode()), updated.encode()))
                plan.counts[kind] += 1
        for match in GITHUB.finditer(data):
            covered.append((match.start(), match.end()))
            ref, target = match.group(1).decode("ascii", "replace"), match.group(2).decode("ascii", "replace")
            if target in moves and is_branch(ref, tag_names):
                edits.append((match.start(2), match.end(2), moves[target].encode()))
                plan.counts["github_url"] += 1
        if suffix not in LINK_SUFFIXES and path_strings:
            for match in path_strings.finditer(data):
                if any(start <= match.start() < end for start, end in covered):
                    continue
                edits.append((match.start(1), match.end(1), moves[match.group(1).decode()].encode()))
                plan.counts["path_string"] += 1
        if suffix in LINK_SUFFIXES and mention:
            for match in mention.finditer(data):
                if any(start <= match.start() < end for start, end in covered):
                    continue
                plan.prose.append((path, data.count(b"\n", 0, match.start()) + 1, match.group(1).decode()))
        if edits:
            if path.startswith(PROTECTED):
                plan.errors.append(f"edit needed in protected file {path}: {len(edits)} spans")
            plan.edits[path] = sorted(edits)
    return plan


def apply_edits(data, edits):
    for start, end, value in sorted(edits, reverse=True):
        data = data[:start] + value + data[end:]
    return data


def title_of(data, suffix, fallback):
    text = data.decode("utf-8", "replace")
    if suffix == ".html":
        match = re.search(r"<title>(.*?)</title>", text, re.IGNORECASE | re.DOTALL)
    else:
        match = re.search(r"^#[ \t]+(.+?)[ \t#]*$", text, re.MULTILINE)
    title = " ".join(match.group(1).split()) if match else fallback
    return title.replace("|", "\\|")


def render_index(root, moves):
    lines = [
        "# Documentation history",
        "",
        "These are dated records: reviews, superseded plans, phase logs, spikes",
        "and experiments. They are kept unchanged apart from link updates, and",
        "they are not current guidance. Statements of status, scope or",
        "remaining work describe the date and tree each record names. Start",
        "from the [documentation index](../README.md) for current behavior.",
        "",
        "Generated by `scripts/dev/d6_archive_docs.py` (step D6a of the",
        "[simplification plan](../simplify-plan.md)); do not edit it by hand.",
        "",
        "| Record | File | Original path |",
        "|---|---|---|",
    ]
    for old in sorted(moves):
        new = moves[old]
        location = root / new if (root / new).is_file() else root / old
        name = posixpath.basename(new)
        title = title_of(location.read_bytes(), posixpath.splitext(name)[1].lower(), name)
        lines.append(f"| {title} | [{name}]({name}) | `{old}` |")
    return ("\n".join(lines) + "\n").encode("utf-8")


def apply(root, plan):
    if plan.errors:
        raise SystemExit("refusing to apply:\n" + "\n".join(plan.errors))
    # 1. Compute every new byte string against the old locations.
    contents = {path: apply_edits((root / path).read_bytes(), edits) for path, edits in plan.edits.items()}
    index = render_index(root, plan.moves)
    # 2. Move with history, then write the precomputed bytes at final locations.
    (root / HISTORY).mkdir(parents=True, exist_ok=True)
    for old, new in sorted(plan.moves.items()):
        subprocess.run(["git", "mv", old, new], cwd=root, check=True)
    for path, data in contents.items():
        (root / plan.moves.get(path, path)).write_bytes(data)
    (root / INDEX).write_bytes(index)
    subprocess.run(["git", "add", "--", INDEX, *sorted(plan.moves.values())], cwd=root, check=True)


def check(root, moves=None, pins=None):
    moves = {old: new_path(old) for old in (moves if moves is not None else MOVES)}
    problems = []
    files = set(tracked(root))
    for old, new in sorted(moves.items()):
        if pinned(old):
            problems.append(f"move touches pinned path {old}")
        if old in files:
            problems.append(f"old path still tracked: {old}")
        if new not in files or not (root / new).is_file():
            problems.append(f"new path missing: {new}")
    for item in PINNED if pins is None else pins:
        target = root / item.rstrip("/")
        if not (target.is_dir() if item.endswith("/") else target.is_file()):
            problems.append(f"pinned path missing: {item}")
    tag_names = tags(root)
    for path in sorted(files):
        suffix = posixpath.splitext(path)[1].lower()
        if path in SELF or not (root / path).is_file():
            continue
        if suffix not in LINK_SUFFIXES and suffix not in TEXT_SUFFIXES:
            continue
        data = (root / path).read_bytes()
        if suffix in LINK_SUFFIXES:
            for kind, start, end in link_targets(data, suffix):
                parts = split_target(data[start:end])
                if not parts or not parts[0] or "${" in parts[0] or (path, parts[0]) in UNRESOLVED_OK:
                    continue  # empty, a template expression, or a reviewed generated target
                if not (root / resolve(path, parts[0])).exists():
                    line = data.count(b"\n", 0, start) + 1
                    problems.append(f"broken {kind} link {path}:{line}: {parts[0]}")
        for match in GITHUB.finditer(data):
            if match.group(2).decode("ascii", "replace") in moves and is_branch(match.group(1).decode("ascii", "replace"), tag_names):
                problems.append(f"branch URL names a moved path in {path}: {match.group(0).decode('ascii', 'replace')}")
    index = root / INDEX
    if not index.is_file() or index.read_bytes() != render_index(root, moves):
        problems.append(f"{INDEX} is missing or stale")
    agents, claude = root / "AGENTS.md", root / "CLAUDE.md"
    if agents.is_file() and claude.is_file() and agents.read_bytes() != claude.read_bytes():
        problems.append("AGENTS.md and CLAUDE.md differ")
    return problems


def report(plan):
    for old, new in sorted(plan.moves.items()):
        print(f"move {old} -> {new}")
    for path, edits in sorted(plan.edits.items()):
        print(f"edit {path}: {len(edits)} spans")
    for path, line, name in plan.prose:
        print(f"prose {path}:{line}: {name} (left unchanged)")
    counts = ", ".join(f"{kind} {plan.counts[kind]}" for kind in ("inline", "reference", "attribute", "github_url", "path_string"))
    print(f"{len(plan.moves)} moves, {len(plan.edits)} edited files; rewrites: {counts}; {len(plan.prose)} prose mentions")
    crates = [path for path in plan.edits if path.startswith("crates/")]
    if crates:
        print("STOP: edits would touch crates/: " + ", ".join(crates))
    for error in plan.errors:
        print(f"error: {error}")


def main(argv=None, root=ROOT):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--plan", action="store_true")
    mode.add_argument("--apply", action="store_true")
    mode.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    if args.check:
        problems = check(root)
        for problem in problems:
            print(problem)
        print(f"d6 archive check: {len(problems)} problems")
        return 1 if problems else 0
    plan = build_plan(root)
    report(plan)
    if args.plan:
        return 1 if plan.errors else 0
    apply(root, plan)
    print(f"applied {len(plan.moves)} moves")
    return 0


if __name__ == "__main__":
    sys.exit(main())
