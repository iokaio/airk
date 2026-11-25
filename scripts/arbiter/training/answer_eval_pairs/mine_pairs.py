# SPDX-License-Identifier: Apache-2.0
"""Mine instruction/evidence training pairs from git history for the Answer evaluation.

Three pair kinds (plan section 5.4): commit, doc and test. Repositories are read
only through read-only git plumbing (log, show, ls-tree, cat-file). Credential
and secrets files are dropped by path pattern before any content is read.
Outputs are private and must live under the repository's git-ignored
``artifacts/`` folder.

Standard library only. See README.md in this folder.
"""
from __future__ import annotations

import argparse
import fnmatch
import glob
import hashlib
import json
import os
import re
import subprocess
import sys
from collections import Counter
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath

SCHEMA = "airk.private.answer-eval-pairs/1"
HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import l7_policy

ROOT = next((p for p in HERE.parents if (p / "Cargo.toml").is_file() and (p / "AGENTS.md").is_file()), None)

# Paths that describe the Answer evaluation itself (plan section 3.1). They apply to the airk repository.
BENCHMARK_EXCLUDES = {
    "airk": [
        "docs/arbiter/final-plan-fp16.md",
        "docs/arbiter/answer-evaluation/**",
        "docs/simplify/one-process-ledger.md",
        "docs/agent-current-work.md",
        "scripts/arbiter-answer-eval/**",
    ],
}

# Credential or secrets files: never read, never cited. Matched on the lower-cased path.
CREDENTIAL_PATTERNS = [
    "aikeys.json", "**/aikeys.json", "*aikeys*",
    ".env", ".env.*", "**/.env", "**/.env.*",
    "*secret*", "*credential*", "*password*", "*.pfx", "*.p12", "*.pem", "*.key", "*.snk",
    "*.publishsettings", "*.pubxml.user", "*id_rsa*", "*id_ed25519*", "*.keystore", "*.jks",
    "appsettings*.json", "**/appsettings*.json", "*usersecrets*", "*.npmrc", "*.netrc", "*token*.json",
]

# Generated, vendored, build and lock paths. Matched on the lower-cased path.
GENERATED_PATTERNS = [
    "node_modules/**", "**/node_modules/**", "target/**", "**/target/**", "dist/**", "**/dist/**",
    "build/**", "**/build/**", "bin/**", "**/bin/**", "obj/**", "**/obj/**", "out/**", "**/out/**",
    "vendor/**", "**/vendor/**", "third_party/**", "**/third_party/**", "third-party/**", "**/third-party/**",
    "artifacts/**", "temp/**", ".airk/**", "**/.vs/**", ".vs/**", "**/coverage/**", "**/wwwroot/lib/**",
    "*.lock", "**/*.lock", "*package-lock.json", "*pnpm-lock.yaml", "*yarn.lock", "*packages.lock.json",
    "*.min.js", "*.min.css", "*.map", "*.g.cs", "*.g.i.cs", "*.designer.cs", "*.generated.*", "*.gen.ts",
    "*modelsnapshot.cs", "**/migrations/*.designer.cs", "*.snap", "*.svg",
]

BINARY_EXTENSIONS = {
    ".png", ".jpg", ".jpeg", ".gif", ".ico", ".bmp", ".webp", ".pdf", ".zip", ".gz", ".7z", ".tar",
    ".woff", ".woff2", ".ttf", ".otf", ".eot", ".exe", ".dll", ".pdb", ".so", ".dylib", ".onnx",
    ".safetensors", ".bin", ".gguf", ".docx", ".xlsx", ".pptx", ".mp3", ".mp4", ".wav", ".vsix",
    ".nupkg", ".db", ".sqlite", ".pyc", ".cache",
}

SOURCE_EXTENSIONS = {".rs", ".ts", ".tsx", ".js", ".mjs", ".cjs", ".vue", ".cs", ".py"}
DOC_EXTENSIONS = {".md"}

TRAILER_RE = re.compile(r"^(signed-off-by|co-authored-by|reviewed-by|change-id|acked-by|tested-by):", re.I)
MECHANICAL_RE = re.compile(
    r"\b(cargo fmt|rustfmt|prettier|reformat|formatting|format code|whitespace|lockfile|lock file|"
    r"bump (the )?version|version bump|regenerate|regenerated|auto-?generated|merge branch|merge pull request)\b",
    re.I,
)
GENERIC_WORDS = set(
    "a an the and or of to in on for with that this it is are do does did not no more some small minor "
    "bug bugs fix fixes fixed fixing update updates updated updating wip change changes changed misc "
    "stuff cleanup clean tweak tweaks work works working various other again latest commit commits test "
    "tests testing temp tmp code new add added adds remove removed final try trying another few".split()
)
HUNK_RE = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@")

MAX_EVIDENCE_LINES = 80
# Content-addressed source copies ("<sha256>--name.py") are retained copies, not maintained source.
HASHED_COPY_RE = re.compile(r"^[0-9a-f]{40,64}--")
NON_CODE_EVIDENCE = {".md", ".txt", ".docx", ".rst"}


def die(message: str) -> None:
    raise SystemExit(f"mine_pairs: {message}")


# ---------------------------------------------------------------- path policy

def _match(path: str, patterns) -> bool:
    low = path.lower()
    name = PurePosixPath(low).name
    for pattern in patterns:
        if fnmatch.fnmatchcase(low, pattern) or fnmatch.fnmatchcase(name, pattern):
            return True
    return False


def path_exclusion(repo: str, path: str) -> str | None:
    """Return the exclusion reason for a repository path, or None when it may be used."""
    if _match(path, CREDENTIAL_PATTERNS):
        return "credential_path"
    for pattern in BENCHMARK_EXCLUDES.get(repo, []):
        if fnmatch.fnmatchcase(path, pattern) or (pattern.endswith("/**") and path.startswith(pattern[:-2])):
            return "benchmark_path"
    if PurePosixPath(path.lower()).suffix in BINARY_EXTENSIONS:
        return "binary_path"
    if _match(path, GENERATED_PATTERNS) or HASHED_COPY_RE.match(PurePosixPath(path).name):
        return "generated_path"
    return None


# ---------------------------------------------------------------- git access (read-only plumbing)

class Git:
    def __init__(self, repo_dir: Path, *, l7: bool = False):
        self.repo_dir = Path(repo_dir)
        self.l7 = l7
        self.env = dict(os.environ, GIT_OPTIONAL_LOCKS="0", GIT_TERMINAL_PROMPT="0", LC_ALL="C")
        if l7:
            self.env = {key: value for key, value in self.env.items() if not key.upper().startswith("GIT_")}
            self.env.update(GIT_OPTIONAL_LOCKS="0", GIT_TERMINAL_PROMPT="0", GIT_NO_REPLACE_OBJECTS="1",
                            GIT_NO_LAZY_FETCH="1", GIT_ALLOW_PROTOCOL="", GIT_CONFIG_NOSYSTEM="1",
                            GIT_CONFIG_GLOBAL=os.devnull)
        self._cat = None

    def run(self, *args: str) -> str:
        try:
            return self.run_bytes(*args).decode("utf-8", "strict" if self.l7 else "replace")
        except UnicodeDecodeError:
            die(f"L7 git {args[0]} metadata must be valid UTF-8")

    def run_bytes(self, *args: str) -> bytes:
        literal = ["--literal-pathspecs"] if self.l7 else []
        cmd = ["git", *literal, "-c", "core.quotepath=off", "-C", str(self.repo_dir), *args]
        proc = subprocess.run(cmd, capture_output=True, env=self.env, check=False)
        if proc.returncode != 0:
            die(f"git {' '.join(args[:3])} failed: {proc.stderr.decode('utf-8', 'replace').strip()[:400]}")
        return proc.stdout

    def blob(self, commit: str, path: str) -> bytes | None:
        if self._cat is None:
            self._cat = subprocess.Popen(
                ["git", "-C", str(self.repo_dir), "cat-file", "--batch"],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=self.env,
            )
        self._cat.stdin.write(f"{commit}:{path}\n".encode("utf-8"))
        self._cat.stdin.flush()
        header = self._cat.stdout.readline().decode("utf-8", "replace").split()
        if len(header) < 3 or header[1] == "missing":
            return None
        size = int(header[2])
        data = self._cat.stdout.read(size)
        self._cat.stdout.read(1)
        return data

    def close(self) -> None:
        if self._cat is not None:
            self._cat.stdin.close()
            self._cat.wait(timeout=60)
            self._cat.stdout.close()
            self._cat = None


def text_of(raw: bytes | None, max_bytes: int = 1_000_000) -> str | None:
    if raw is None or len(raw) > max_bytes or b"\0" in raw[:8192]:
        return None
    return raw.decode("utf-8", "replace")


# ---------------------------------------------------------------- shared helpers

def pair_id(repo: str, kind: str, key: str) -> str:
    return f"{repo}-{kind}-{hashlib.sha256(f'{repo}\n{kind}\n{key}'.encode()).hexdigest()[:16]}"


def split_of(pid: str, heldout_percent: int) -> str:
    bucket = int(hashlib.sha256(pid.encode()).hexdigest()[:8], 16) % 100
    return "heldout" if bucket < heldout_percent else "train"


def merge_ranges(ranges, gap: int = 3):
    out = []
    for a, b in sorted(ranges):
        if out and a <= out[-1][1] + gap + 1:
            out[-1][1] = max(out[-1][1], b)
        else:
            out.append([a, b])
    return out


def clean_message(message: str) -> str:
    lines = [ln.rstrip() for ln in message.replace("\r\n", "\n").split("\n")]
    lines = [ln for ln in lines if not TRAILER_RE.match(ln.strip())]
    return "\n".join(lines).strip()


def words(text: str):
    return re.findall(r"[A-Za-z][A-Za-z0-9']*", text)


def split_identifier(name: str):
    spaced = re.sub(r"([a-z0-9])([A-Z])", r"\1 \2", name.replace("::", " "))
    spaced = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1 \2", spaced)
    return [t.lower() for t in re.split(r"[\s_\-]+", spaced) if t]


@dataclass
class Stats:
    pairs: Counter = field(default_factory=Counter)
    excluded: Counter = field(default_factory=Counter)
    repo: str = ""

    def drop(self, kind: str, reason: str, n: int = 1) -> None:
        self.excluded[f"{self.repo}:{kind}:{reason}"] += n


# ---------------------------------------------------------------- commit pairs

def iter_commits(git: Git, before: str):
    sep_rec, sep_fld = "\x1e", "\x1f"
    out = git.run("log", "--no-merges", "--no-renames", f"--before={before}", "--numstat",
                  f"--format={sep_rec}%H{sep_fld}%cI{sep_fld}%P{sep_fld}%B{sep_fld}", "HEAD")
    for record in out.split(sep_rec)[1:]:
        sha, date, parents, body, rest = record.split(sep_fld, 4)
        files = []
        for line in rest.strip("\n").split("\n"):
            parts = line.split("\t")
            if len(parts) == 3:
                added, deleted, path = parts
                files.append((path, added, deleted))
        yield sha, date, parents.split(), body, files


REGULAR_MODES = {"100644", "100755"}


def iter_l7_commits(git: Git, before: str, revision: str, max_files: int, exclude, stats: Stats):
    """Inspect tree metadata before requesting any content, including numstat."""
    revisions = git.run("log", "--no-merges", f"--before={before}", "--format=%H", revision, "--")
    for sha in revisions.splitlines():
        raw = git.run("diff-tree", "--root", "-r", "--no-commit-id", "--no-renames",
                      "--no-ext-diff", "--no-textconv", "--raw", "-z", sha, "--")
        fields = raw.split("\0")
        if fields[-1] != "" or (len(fields) - 1) % 2:
            die("malformed L7 changed-path metadata")
        paths, denied = [], False
        for header, path in zip(fields[:-1:2], fields[1:-1:2], strict=True):
            mode, new_mode, *_ = header.removeprefix(":").split()
            reason = exclude(path)
            if reason is None and (
                mode not in REGULAR_MODES | {"000000"} or new_mode not in REGULAR_MODES | {"000000"}
            ):
                reason = "nonregular_path"
            if reason:
                denied |= reason in {"protected_path", "bookbuilder_index_deny", "credential_path",
                                     "unsafe_path", "nonregular_path"}
                stats.drop("commit_file", reason)
            else:
                paths.append(path)
        if denied:
            # The message of a mixed commit can describe protected evidence too.
            stats.drop("commit", "protected_or_unsafe_change")
            continue
        if not paths:
            stats.drop("commit", "no_evidence_after_path_filter")
            continue
        if (len(fields) - 1) // 2 > max_files:
            stats.drop("commit", "too_many_files")
            continue
        raw_stats = git.run("show", "--format=", "--numstat", "-z", "--no-renames",
                            "--no-ext-diff", "--no-textconv", sha, "--", *paths)
        files = []
        for line in raw_stats.split("\0"):
            if not line:
                continue
            added, deleted, path = line.split("\t", 2)
            if path not in paths:
                die("L7 numstat returned an unapproved path")
            files.append((path, added, deleted))
        try:
            body = git.run_bytes("show", "-s", "--encoding=UTF-8", "--format=%B", sha, "--").decode("utf-8")
        except UnicodeDecodeError:
            stats.drop("commit", "invalid_message_encoding")
            continue
        yield sha, "", [], body, files


def mine_commits(repo: str, git: Git, args, stats: Stats, *, revision: str | None = None, exclude=None):
    pairs = []
    rows = (iter_l7_commits(git, args.before, revision, args.max_files, exclude, stats)
            if revision is not None else iter_commits(git, args.before))
    for sha, date, parents, body, files in rows:
        if len(parents) > 1:
            stats.drop("commit", "merge"); continue
        message = clean_message(body)
        msg_words = words(message)
        if len(message) < args.min_message_chars or len(msg_words) < args.min_message_words:
            stats.drop("commit", "message_too_short"); continue
        if all(w.lower() in GENERIC_WORDS for w in msg_words[:12]):
            stats.drop("commit", "message_generic"); continue
        subject = message.split("\n", 1)[0]
        if MECHANICAL_RE.search(subject):
            stats.drop("commit", "mechanical_message"); continue
        if not files:
            stats.drop("commit", "no_files"); continue
        if len(files) > args.max_files:
            stats.drop("commit", "too_many_files"); continue
        kept, reasons = [], Counter()
        for path, added, _deleted in files:
            reason = path_exclusion(repo, path)
            if reason is None and added == "-":
                reason = "binary_path"
            if reason is None and added == "0":
                reason = "deletion_only"
            if reason:
                reasons[reason] += 1
            else:
                kept.append(path)
        for reason, n in reasons.items():
            stats.drop("commit_file", reason, n)
        if reasons.get("credential_path") and not kept:
            stats.drop("commit", "credential_only"); continue
        if not kept:
            stats.drop("commit", "no_evidence_after_path_filter"); continue
        if len(kept) <= len(files) // 4 and len(files) >= 8:
            stats.drop("commit", "mostly_mechanical_files"); continue
        added_total = sum(int(a) for p, a, _ in files if p in kept and a.isdigit())
        if added_total > args.max_added_lines:
            stats.drop("commit", "too_many_lines"); continue
        diff_args = ("show", "--format=", "--no-color", "--no-renames", "--no-ext-diff")
        if revision is None:
            diff = git.run(*diff_args, "-U0", sha, "--", *kept)
        else:
            # Only exact admitted path headers and ASCII hunk ranges survive this parse.
            patch = git.run_bytes(*diff_args, "--no-textconv", "-U0", sha, "--", *kept)
            diff = patch.decode("utf-8", "surrogateescape")
        ranges = {}
        current = None
        for line in diff.split("\n"):
            if line.startswith("+++ "):
                target = line[4:]
                current = target[2:] if target.startswith("b/") else None
                if current not in kept:
                    current = None
                continue
            m = HUNK_RE.match(line)
            if m and current:
                start, count = int(m.group(1)), int(m.group(2) or 1)
                if count > 0:
                    ranges.setdefault(current, []).append((start, start + count - 1))
        evidence = []
        for path in kept:
            for a, b in merge_ranges(ranges.get(path, [])):
                evidence.append({"path": path, "lines": [a, b]})
        if not evidence:
            stats.drop("commit", "no_hunks"); continue
        if len(evidence) > args.max_evidence:
            stats.drop("commit", "too_many_hunks"); continue
        pid = pair_id(repo, "commit", sha)
        pairs.append({"id": pid, "repo": repo, "kind": "commit", "commit": sha,
                      "instruction": message[: args.max_instruction_chars], "evidence": evidence})
    return pairs


# ---------------------------------------------------------------- snapshot index (definitions)

RUST_DEFS = [
    re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|const|unsafe|extern\s+\"[^\"]*\")\s+)*fn\s+([A-Za-z_]\w*)"),
    re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|trait|type|union)\s+([A-Za-z_]\w*)"),
    re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const|static)\s+([A-Z_][A-Z0-9_]*)\s*:"),
    re.compile(r"^\s*macro_rules!\s*([A-Za-z_]\w*)"),
]
TS_DEFS = [
    re.compile(r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s*\*?\s*([A-Za-z_$][\w$]*)"),
    re.compile(r"^\s*(?:export\s+)?(?:default\s+)?(?:abstract\s+)?(?:declare\s+)?(?:class|interface|enum|type)\s+([A-Za-z_$][\w$]*)"),
    re.compile(r"^\s*(?:export\s+)?(?:const|let)\s+([A-Za-z_$][\w$]*)\s*(?::[^=]+)?=\s*(?:async\s*)?(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*(?::[^=]+)?=>"),
    re.compile(r"^\s{2,}(?:(?:public|private|protected|static|async|readonly|override|get|set)\s+)*([A-Za-z_$][\w$]*)\s*\([^)]*\)\s*(?::\s*[^{;]+)?\{\s*$"),
]
CS_DEFS = [
    re.compile(r"^\s*(?:(?:public|private|protected|internal|static|sealed|abstract|partial|readonly|file|unsafe|new)\s+)*(?:class|interface|struct|enum|record(?:\s+struct|\s+class)?)\s+([A-Za-z_]\w*)"),
    re.compile(r"^\s*(?:(?:public|private|protected|internal|static|async|override|virtual|sealed|abstract|partial|new|extern|unsafe|readonly)\s+)+[\w<>\[\],.?() ]*?\s([A-Za-z_]\w*)\s*(?:<[^>()]*>)?\s*\("),
]
PY_DEFS = [re.compile(r"^\s*(?:async\s+)?(?:def|class)\s+([A-Za-z_]\w*)")]
KEYWORDS = set("if for while switch catch return new else function constructor using lock foreach typeof sizeof nameof await yield match loop".split())


def language(path: str) -> str | None:
    ext = PurePosixPath(path.lower()).suffix
    return {".rs": "rust", ".ts": "ts", ".tsx": "ts", ".js": "ts", ".mjs": "ts", ".cjs": "ts", ".vue": "ts",
            ".cs": "cs", ".py": "py"}.get(ext)


def is_test_path(path: str) -> bool:
    low = path.lower()
    name = PurePosixPath(low).name
    parts = PurePosixPath(low).parts[:-1]
    return (any(p in ("tests", "test", "__tests__", "spec", "specs") or p.endswith(".tests") or p.endswith(".test")
                for p in parts)
            or re.search(r"(\.test\.|\.spec\.|^test_.*\.py$|_test\.py$|(?:^|_)tests?\.rs$|tests?\.cs$)", name) is not None)


def block_end(lines, start: int, lang: str, limit: int = MAX_EVIDENCE_LINES) -> int:
    """Approximate the last line (0-based, inclusive) of a definition starting at ``start``."""
    if lang == "py":
        indent = len(lines[start]) - len(lines[start].lstrip())
        end = start
        for i in range(start + 1, min(len(lines), start + limit)):
            stripped = lines[i].strip()
            if not stripped:
                continue
            if len(lines[i]) - len(lines[i].lstrip()) <= indent:
                break
            end = i
        return end
    depth, opened = 0, False
    for i in range(start, min(len(lines), start + limit)):
        line = re.sub(r'"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])\'|//.*$', "", lines[i])
        if not opened and ";" in line and "{" not in line and i - start < 6 and line.rstrip().endswith(";"):
            return i
        for ch in line:
            if ch == "{":
                depth += 1; opened = True
            elif ch == "}":
                depth -= 1
        if opened and depth <= 0:
            return i
    return min(len(lines), start + limit) - 1


def rust_test_regions(lines):
    regions = []
    i = 0
    while i < len(lines):
        if lines[i].strip().startswith("#[cfg(test)]"):
            j = i + 1
            while j < len(lines) and not lines[j].strip():
                j += 1
            if j < len(lines) and re.match(r"^\s*(pub\s+)?mod\s+\w+\s*\{", lines[j]):
                end = block_end(lines, j, "rust", limit=10**7)
                regions.append((i, end)); i = end + 1; continue
        i += 1
    return regions


class Snapshot:
    """Tracked files at one commit and a definition index of non-test source."""

    def __init__(self, repo: str, git: Git, commit: str, stats: Stats, *, exclude=None):
        self.repo, self.git, self.commit = repo, git, commit
        self.paths = []
        self.texts = {}
        if exclude is None:
            tree = ((None, path) for path in git.run("ls-tree", "-r", "--name-only", commit).split("\n"))
        else:
            raw = git.run("ls-tree", "-r", "--full-tree", "-z", commit, "--")
            tree = (row.split("\t", 1) for row in raw.split("\0") if row)
        for metadata, path in tree:
            if not path:
                continue
            reason = path_exclusion(repo, path) if exclude is None else exclude(path)
            if metadata is not None and metadata.split()[0] not in REGULAR_MODES:
                reason = reason or "nonregular_path"
            if reason:
                stats.drop("snapshot_file", reason)
                continue
            self.paths.append(path)
        self.path_set = set(self.paths)
        self.by_name = {}
        for p in self.paths:
            self.by_name.setdefault(PurePosixPath(p).name.lower(), []).append(p)
        self.defs = {}  # name -> list of (path, start, end) 1-based
        for path in self.paths:
            lang = language(path)
            if not lang or is_test_path(path):
                continue
            text = self.text(path)
            if text is None:
                continue
            self._index(path, text, lang)

    def text(self, path: str) -> str | None:
        if self.git.l7 and path not in self.path_set:
            return None
        if path not in self.texts:
            self.texts[path] = text_of(self.git.blob(self.commit, path))
        return self.texts[path]

    def lines(self, path: str):
        text = self.text(path)
        return None if text is None else text.split("\n")

    def _index(self, path: str, text: str, lang: str) -> None:
        lines = text.split("\n")
        skip = rust_test_regions(lines) if lang == "rust" else []
        patterns = {"rust": RUST_DEFS, "ts": TS_DEFS, "cs": CS_DEFS, "py": PY_DEFS}[lang]
        for i, line in enumerate(lines):
            if any(a <= i <= b for a, b in skip) or len(line) > 400:
                continue
            for pattern in patterns:
                m = pattern.match(line)
                if m and m.group(1) not in KEYWORDS:
                    end = block_end(lines, i, lang)
                    self.defs.setdefault(m.group(1), []).append((path, i + 1, end + 1, lang))
                    break

    def resolve_symbol(self, name: str, lang: str | None = None):
        hits = [h for h in self.defs.get(name, []) if lang is None or h[3] == lang]
        if not hits or len({h[0] for h in hits}) > 1:
            return None
        path, start, end, _ = hits[0]
        return {"path": path, "lines": [start, end]}

    def resolve_path(self, token: str, base_dir: str):
        token = token.strip().strip("<>").split("#", 1)[0].split("?", 1)[0]
        token = re.sub(r":\d+(?:[-:]\d+)?$", "", token).replace("\\", "/")
        while token.startswith("./"):
            token = token[2:]
        token = token.rstrip("/")
        if not token or token.startswith(("http:", "https:", "mailto:")):
            return None
        candidates = [token]
        if base_dir:
            candidates.append(os.path.normpath(f"{base_dir}/{token}").replace("\\", "/"))
        for cand in candidates:
            if cand in self.path_set:
                return cand
        if "/" not in token:
            hits = self.by_name.get(token.lower(), [])
            return hits[0] if len(hits) == 1 else None
        suffix = [p for p in self.paths if p.endswith("/" + token)]
        return suffix[0] if len(suffix) == 1 else None

    def file_head(self, path: str):
        lines = self.lines(path)
        if not lines:
            return None
        n = len(lines) - (1 if lines[-1] == "" else 0)
        return {"path": path, "lines": [1, max(1, min(n, MAX_EVIDENCE_LINES))]} if n > 0 else None


# ---------------------------------------------------------------- doc pairs

HEADING_RE = re.compile(r"^(#{1,6})\s+(.+?)\s*#*\s*$")
BACKTICK_RE = re.compile(r"`([^`\n]{2,200})`")
LINK_RE = re.compile(r"\]\(([^)\s]+)\)")
SYMBOL_TOKEN_RE = re.compile(r"^(?:[A-Za-z_]\w*(?:::|\.))*([A-Za-z_]\w*)(?:\(\))?$")
PATHY_RE = re.compile(r"^[\w.\-/\\]+$")


def doc_sections(text: str):
    lines = text.split("\n")
    sections, current, fence = [], None, False
    for i, line in enumerate(lines):
        if line.lstrip().startswith(("```", "~~~")):
            fence = not fence
        m = None if fence else HEADING_RE.match(line)
        if m:
            if current:
                sections.append(current)
            current = {"heading": m.group(2).strip(), "start": i + 1, "lines": [line]}
        elif current:
            current["lines"].append(line)
    if current:
        sections.append(current)
    for s in sections:
        s["end"] = s["start"] + len(s["lines"]) - 1
        s["text"] = "\n".join(s["lines"]).strip()
    return sections


def is_symbol_like(token: str) -> str | None:
    m = SYMBOL_TOKEN_RE.match(token)
    if not m:
        return None
    name = m.group(1)
    if len(name) < 4:
        return None
    if "_" in name.strip("_") or re.search(r"[a-z][A-Z]", name) or "::" in token or re.match(r"^[A-Z][a-z]+[A-Z]", name):
        return name
    return None


def mine_docs(repo: str, snap: Snapshot, args, stats: Stats):
    pairs = []
    for path in snap.paths:
        if PurePosixPath(path.lower()).suffix not in DOC_EXTENSIONS:
            continue
        text = snap.text(path)
        if text is None:
            stats.drop("doc", "unreadable_doc"); continue
        base_dir = str(PurePosixPath(path).parent) if "/" in path else ""
        for sec in doc_sections(text):
            body_words = len(words(sec["text"])) - len(words(sec["heading"]))
            if body_words < args.min_doc_words:
                stats.drop("doc", "section_too_short"); continue
            evidence, seen = [], set()
            tokens = BACKTICK_RE.findall(sec["text"]) + LINK_RE.findall(sec["text"])
            for token in tokens:
                token = token.strip()
                loc = None
                if PATHY_RE.match(token) and ("/" in token or "\\" in token or PurePosixPath(token).suffix):
                    resolved = snap.resolve_path(token, base_dir)
                    if resolved and resolved != path:
                        if path_exclusion(repo, resolved) or PurePosixPath(resolved.lower()).suffix in NON_CODE_EVIDENCE:
                            continue
                        loc = snap.file_head(resolved)
                if loc is None:
                    name = is_symbol_like(token)
                    if name:
                        loc = snap.resolve_symbol(name)
                if loc and loc["path"] != path:
                    key = (loc["path"], *loc["lines"])
                    if key not in seen:
                        seen.add(key); evidence.append(loc)
                if len(evidence) >= args.max_doc_evidence:
                    break
            if not evidence:
                stats.drop("doc", "names_no_resolvable_code"); continue
            pid = pair_id(repo, "doc", f"{path}\n{sec['start']}\n{sec['heading']}")
            pairs.append({"id": pid, "repo": repo, "kind": "doc", "commit": snap.commit,
                          "instruction": sec["text"][: args.max_instruction_chars], "evidence": evidence,
                          "source": {"path": path, "lines": [sec["start"], sec["end"]]}})
    return pairs


# ---------------------------------------------------------------- test pairs

CALL_RE = re.compile(r"\b([A-Za-z_]\w{3,})\s*(?:::<[^>]*>|<[\w, ]*>)?\s*\(")
TYPE_RE = re.compile(r"\b([A-Z][A-Za-z0-9_]{3,})\b")


def find_tests(path: str, lines, lang: str):
    """Yield (name, description, start, end) for test functions/cases in one file (0-based lines)."""
    if lang == "rust":
        for i, line in enumerate(lines):
            if re.match(r"^\s*#\[(?:tokio::|async_std::)?test\b", line):
                for j in range(i + 1, min(len(lines), i + 6)):
                    m = re.match(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_]\w*)", lines[j])
                    if m:
                        doc = []
                        k = i - 1
                        while k >= 0 and (lines[k].strip().startswith("///") or lines[k].strip().startswith("#[")):
                            if lines[k].strip().startswith("///"):
                                doc.insert(0, lines[k].strip()[3:].strip())
                            k -= 1
                        yield m.group(1), " ".join(doc), j, block_end(lines, j, lang, limit=400)
                        break
    elif lang == "cs":
        for i, line in enumerate(lines):
            if re.match(r"^\s*\[(?:Fact|Theory|Test|TestMethod|TestCase)\b", line):
                for j in range(i + 1, min(len(lines), i + 12)):
                    m = re.match(r"^\s*(?:(?:public|private|internal|async|static)\s+)+[\w<>]+\s+([A-Za-z_]\w*)\s*\(", lines[j])
                    if m:
                        doc = []
                        k = i - 1
                        while k >= 0 and lines[k].strip().startswith("///"):
                            doc.insert(0, re.sub(r"<[^>]+>", "", lines[k].strip()[3:]).strip())
                            k -= 1
                        disp = re.search(r'DisplayName\s*=\s*"([^"]+)"', line)
                        desc = " ".join([disp.group(1)] if disp else []) + " " + " ".join(doc)
                        yield m.group(1), desc.strip(), j, block_end(lines, j, lang, limit=400)
                        break
    elif lang == "py":
        for i, line in enumerate(lines):
            m = re.match(r"^\s*(?:async\s+)?def\s+(test_\w+)\s*\(", line)
            if m:
                doc = ""
                if i + 1 < len(lines):
                    dm = re.match(r'^\s*(?:"""|\'\'\')(.*?)(?:"""|\'\'\')?\s*$', lines[i + 1])
                    if dm:
                        doc = dm.group(1).strip()
                yield m.group(1), doc, i, block_end(lines, i, lang, limit=400)
    elif lang == "ts":
        stack = []  # (indent, title)
        for i, line in enumerate(lines):
            m = re.match(r"^(\s*)(describe|it|test)(?:\.(?:only|skip|each\([^)]*\)))?\s*\(\s*(['\"`])(.+?)\3", line)
            if not m:
                continue
            indent = len(m.group(1))
            while stack and stack[-1][0] >= indent:
                stack.pop()
            if m.group(2) == "describe":
                stack.append((indent, m.group(4)))
                continue
            title = " ".join([t for _, t in stack] + [m.group(4)])
            yield m.group(4), title, i, block_end(lines, i, lang, limit=400)


def convention_targets(path: str):
    p = PurePosixPath(path)
    name = p.name
    out = []
    for pattern, repl in ((r"^(.*)\.(?:test|spec)\.(\w+)$", r"\1.\2"), (r"^test_(.*\.py)$", r"\1"),
                          (r"^(.*)_test\.py$", r"\1.py"), (r"^(.*)_tests?\.rs$", r"\1.rs"),
                          (r"^(.*?)Tests?\.cs$", r"\1.cs")):
        if re.match(pattern, name):
            out.append(re.sub(pattern, repl, name))
    return out


TEST_NAME_STOPWORDS = set(
    "test tests should returns return when with the and new get set from into for that this then given "
    "does not can are has have was will must when after before only all any".split()
)


def mine_tests(repo: str, snap: Snapshot, args, stats: Stats):
    pairs, found = [], []
    for path in snap.paths:
        lang = language(path)
        if not lang:
            continue
        lines = snap.lines(path)
        if not lines:
            continue
        in_test_file = is_test_path(path)
        if lang == "ts" and not in_test_file:
            continue
        if lang in ("cs", "py") and not in_test_file and not any(re.search(r"\[(Fact|Theory|Test|TestMethod)\b|def test_", ln) for ln in lines[:4000]):
            continue
        for name, desc, start, end in find_tests(path, lines, lang):
            title_words = split_identifier(name) if lang != "ts" else words(name)
            if lang == "py" and title_words and title_words[0] == "test":
                title_words = title_words[1:]
            if len(title_words) < args.min_test_words and len(words(desc)) < args.min_test_words:
                stats.drop("test", "name_too_short"); continue
            body = "\n".join(lines[start:end + 1])
            name_tokens = {t for t in set(split_identifier(name)) | {w.lower() for w in words(desc)}
                           if len(t) >= 3 and t not in TEST_NAME_STOPWORDS}
            candidates, order = [], {}
            for m in list(CALL_RE.finditer(body)) + list(TYPE_RE.finditer(body)):
                ident = m.group(1)
                if ident == name or ident in order:
                    continue
                order[ident] = len(order)
            for ident, pos in order.items():
                loc = snap.resolve_symbol(ident, lang)
                if not loc or loc["path"] == path and loc["lines"][0] - 1 <= start <= loc["lines"][1]:
                    continue
                shared = len(set(split_identifier(ident)) & name_tokens)
                candidates.append((-shared, pos, loc))
            candidates.sort(key=lambda c: (c[0], c[1]))
            found.append((path, lang, name, desc, title_words, start, end, candidates))

    # A definition reached by many tests without sharing a word with their names is a shared
    # helper, not the code under test.
    hub = Counter()
    for *_, candidates in found:
        if candidates and candidates[0][0] == 0:
            hub[(candidates[0][2]["path"], *candidates[0][2]["lines"])] += 1
    for path, lang, name, desc, title_words, start, end, candidates in found:
        named = [c for c in candidates if c[0] < 0]
        if named:
            chosen = named
        else:
            chosen = [c for c in candidates if hub[(c[2]["path"], *c[2]["lines"])] <= args.hub_limit][:1]
            if candidates and not chosen:
                stats.drop("test", "shared_helper_only")
        evidence, seen = [], set()
        for _, _, loc in chosen:
            key = (loc["path"], *loc["lines"])
            if key not in seen:
                seen.add(key); evidence.append(loc)
            if len(evidence) >= args.max_test_evidence:
                break
        how = "named_calls" if named else "first_call"
        if not evidence and not candidates:
            for target in convention_targets(path):
                hits = [p for p in snap.by_name.get(target.lower(), []) if not is_test_path(p)]
                if len(hits) == 1:
                    head = snap.file_head(hits[0])
                    if head:
                        evidence.append(head); how = "naming"
                    break
        if not evidence:
            if not candidates:
                stats.drop("test", "no_code_under_test")
            continue
        stats.pairs[f"{repo}:test_resolved_by:{how}"] += 1
        readable = " ".join(title_words) if lang != "ts" else desc
        instruction = f"Test `{name}`: {readable}."
        if desc and lang != "ts":
            instruction += f" {desc}"
        pid = pair_id(repo, "test", f"{path}\n{name}\n{start + 1}")
        pairs.append({"id": pid, "repo": repo, "kind": "test", "commit": snap.commit,
                      "instruction": instruction[: args.max_instruction_chars], "evidence": evidence,
                      "source": {"path": path, "lines": [start + 1, end + 1]}})
    return pairs


# ---------------------------------------------------------------- note overlap filter

def load_notes(patterns, repo_names):
    notes = []
    files = sorted({f for pattern in patterns for f in glob.glob(pattern)})
    for file in files:
        try:
            data = json.loads(Path(file).read_text(encoding="utf-8"))
        except (OSError, ValueError) as exc:
            notes.append({"error": f"{file}: {exc}"})
            continue
        repo = data.get("repository") or Path(file).parent.name
        for note in data.get("notes", []):
            for place in [note, *note.get("alternatives", [])]:
                if place.get("path"):
                    lines = place.get("lines") or [1, 10**9]
                    notes.append({"repo": repo, "path": place["path"].replace("\\", "/"),
                                  "lines": [int(lines[0]), int(lines[-1])], "scenario": data.get("id", file)})
    return files, notes


def overlaps(place, note, margin: int) -> bool:
    if place["path"] != note["path"]:
        return False
    a, b = place["lines"]
    return a <= note["lines"][1] + margin and note["lines"][0] - margin <= b


def apply_note_filter(pairs, notes, margin: int, stats: Stats):
    kept = []
    by_path = {}
    for n in notes:
        if "error" not in n:
            by_path.setdefault((n["repo"], n["path"]), []).append(n)
    for pair in pairs:
        places = list(pair["evidence"]) + ([pair["source"]] if pair.get("source") else [])
        hit = any(overlaps(pl, n, margin) for pl in places for n in by_path.get((pair["repo"], pl["path"]), []))
        if hit:
            stats.drop(pair["kind"], "overlaps_scenario_note")
            continue
        if any((pair["repo"], pl["path"]) in by_path for pl in places):
            stats.pairs[f"{pair['repo']}:kept_sharing_path_with_note"] += 1
        kept.append(pair)
    return kept


# ---------------------------------------------------------------- output

def require_private_output(out: Path) -> Path:
    out = Path(out).resolve()
    if ROOT is None:
        die("cannot locate the AIRK repository root")
    private = (ROOT / "artifacts").resolve()
    if not out.is_relative_to(private):
        die(f"output must be under the git-ignored {private}")
    return out


def write_jsonl(path: Path, rows) -> None:
    tmp = path.with_suffix(path.suffix + ".tmp")
    with open(tmp, "w", encoding="utf-8", newline="\n") as fh:
        for row in rows:
            fh.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")
    os.replace(tmp, path)


def read_jsonl(path: Path):
    with open(path, encoding="utf-8") as fh:
        return [json.loads(line) for line in fh if line.strip()]


def finish(out: Path, repos, unfiltered, args, stats: Stats, mine_record) -> dict:
    files, notes = load_notes(args.exclude_notes or [], repos)
    summary = {"schema": SCHEMA, "written_utc": datetime.now(timezone.utc).isoformat(timespec="seconds"),
               "heldout_percent": args.heldout_percent, "overlap_margin_lines": args.overlap_margin,
               "note_files": files, "note_places": sum(1 for n in notes if "error" not in n),
               "note_errors": [n["error"] for n in notes if "error" in n], "mine": mine_record,
               "counts": {}, "exclusions": {}}
    for repo in repos:
        stats.repo = repo
        pairs = apply_note_filter(unfiltered[repo], notes, args.overlap_margin, stats)
        for p in pairs:
            p["split"] = split_of(p["id"], args.heldout_percent)
        pairs.sort(key=lambda p: p["id"])
        pair_path = out / f"{repo}.jsonl"
        write_jsonl(pair_path, pairs)
        if mine_record.get("mode") == "l7":
            with pair_path.open("rb") as source:
                summary.setdefault("pair_sha256", {})[repo] = hashlib.file_digest(source, "sha256").hexdigest()
        counts = Counter()
        for p in pairs:
            counts[f"{p['kind']}:{p['split']}"] += 1
            counts[f"total:{p['split']}"] += 1
        summary["counts"][repo] = dict(sorted(counts.items()))
    summary["exclusions"] = dict(sorted(stats.excluded.items()))
    summary["notes_on_kept"] = dict(sorted(stats.pairs.items()))
    tmp = out / "stats.json.tmp"
    tmp.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    os.replace(tmp, out / "stats.json")
    return summary


def parse_repo(value: str):
    name, sep, path = value.partition("=")
    if not sep or not name or not path:
        raise argparse.ArgumentTypeError("expected NAME=PATH")
    return name, Path(path)


def revision_arg(value: str) -> tuple[str, str]:
    repo, sep, oid = value.partition("=")
    if not sep or repo not in l7_policy.REPOS or not re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", oid):
        raise argparse.ArgumentTypeError("expected bookbuilder or munarium and a full lowercase Git commit ID")
    return repo, oid


def l7_excluder(repo: str, protected: dict, bookbuilder_denies):
    def exclude(path: str) -> str | None:
        if not l7_policy.safe_relative_path(path):
            return "unsafe_path"
        if l7_policy.normalized_path(path) in protected[repo]:
            return "protected_path"
        if repo == "bookbuilder" and l7_policy.denied_by_fixture(path, bookbuilder_denies):
            return "bookbuilder_index_deny"
        return path_exclusion(repo, path)

    return exclude


def check_l7_inputs(args, out: Path):
    repos = dict(args.repo)
    revisions = dict(args.revision)
    if set(repos) != l7_policy.REPOS or len(args.repo) != len(repos):
        die("L7 requires exactly one repository each for Bookbuilder and Munarium")
    if set(revisions) != l7_policy.REPOS or len(args.revision) != len(revisions):
        die("L7 requires one exact revision for each repository")
    if args.exclude_notes or args.overlap_margin or not args.protected_paths:
        die("L7 requires a pinned whole-path inventory instead of legacy note inputs")
    if out.exists() or not out.parent.is_dir():
        die("L7 requires a fresh output directory under an existing private parent")
    roots = {name: path.resolve(strict=True) for name, path in args.repo}
    if len(set(roots.values())) != len(roots) or any(out.is_relative_to(path) for path in roots.values()):
        die("L7 needs separate source trees and an output outside both repositories")
    inventory = args.protected_paths.resolve(strict=True)
    if not inventory.is_relative_to((ROOT / "artifacts").resolve()):
        die("L7 protected path inventory must be private")
    try:
        protected = l7_policy.load_protected_paths(inventory, args.protected_paths_sha256)
        denies = l7_policy.bookbuilder_denies()
    except (OSError, ValueError, SyntaxError):
        die("invalid pinned L7 path inventory or Bookbuilder deny policy")
    for name, path in roots.items():
        git = Git(path, l7=True)
        oid = revisions[name]
        if git.run("rev-parse", "--show-prefix").strip():
            die("L7 repository path must name the repository root")
        if git.run("rev-parse", "--is-shallow-repository").strip() != "false":
            die("L7 requires complete repository history")
        if git.run("rev-parse", "--verify", f"{oid}^{{commit}}").strip() != oid:
            die("L7 revision must be an exact commit, not a tag or ref")
        at = datetime.fromisoformat(git.run("show", "-s", "--format=%cI", oid, "--").strip())
        if at > datetime.fromisoformat(args.before):
            die("L7 revision is after the cutoff")
    args.repo = list(roots.items())
    return revisions, protected, denies


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("command", choices=["mine", "filter"], help="mine: read repositories and filter; filter: re-apply the note filter to unfiltered/")
    ap.add_argument("--repo", action="append", type=parse_repo, default=[], help="NAME=PATH (repeatable)")
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--before", help="ISO date; commits after it are not used (required for mine)")
    ap.add_argument("--l7", action="store_true", help="pin the two L7 histories and exclude protected whole files")
    ap.add_argument("--revision", action="append", type=revision_arg, default=[], help="NAME=FULL_GIT_COMMIT (L7 only)")
    ap.add_argument("--protected-paths", type=Path, help="private machine-readable path inventory (L7 only)")
    ap.add_argument("--protected-paths-sha256", help="exact hash of the path inventory bytes (L7 only)")
    ap.add_argument("--exclude-notes", action="append", default=[], help="glob of scenario JSON files (repeatable)")
    ap.add_argument("--overlap-margin", type=int, default=0)
    ap.add_argument("--heldout-percent", type=int, default=15)
    ap.add_argument("--max-files", type=int, default=40)
    ap.add_argument("--max-added-lines", type=int, default=4000)
    ap.add_argument("--max-evidence", type=int, default=60)
    ap.add_argument("--min-message-words", type=int, default=5)
    ap.add_argument("--min-message-chars", type=int, default=24)
    ap.add_argument("--min-doc-words", type=int, default=12)
    ap.add_argument("--max-doc-evidence", type=int, default=8)
    ap.add_argument("--min-test-words", type=int, default=3)
    ap.add_argument("--max-test-evidence", type=int, default=3)
    ap.add_argument("--hub-limit", type=int, default=25, help="max tests a name-unrelated helper may serve as sole evidence")
    ap.add_argument("--max-instruction-chars", type=int, default=2000)
    ap.add_argument("--kinds", default="commit,doc,test")
    args = ap.parse_args(argv)
    l7_options = args.revision or args.protected_paths is not None or args.protected_paths_sha256 is not None
    if (l7_options and not args.l7) or (args.l7 and args.command != "mine"):
        die("the L7 options require mine --l7")

    out = require_private_output(args.out)
    unf_dir = out / "unfiltered"
    stats = Stats()
    if args.command == "mine":
        if not args.before or not args.repo:
            die("mine requires --before and at least one --repo")
        before = datetime.fromisoformat(args.before.replace("Z", "+00:00"))
        if before.tzinfo is None:
            die("--before must carry a time zone")
        args.before = before.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")
        kinds = set(args.kinds.split(","))
        record = {"before": args.before, "kinds": sorted(kinds), "repos": {}}
        if args.l7:
            revisions, protected, denies = check_l7_inputs(args, out)
            record.update(mode="l7", protected_paths_sha256=args.protected_paths_sha256,
                          bookbuilder_deny_policy_sha256=l7_policy.deny_policy_sha256(denies))
        else:
            out.mkdir(parents=True, exist_ok=True)
            unf_dir.mkdir(exist_ok=True)
        unfiltered = {}
        for name, path in args.repo:
            stats.repo = name
            git = Git(path, l7=args.l7)
            try:
                revision = revisions[name] if args.l7 else "HEAD"
                exclude = l7_excluder(name, protected, denies) if args.l7 else None
                head = revision if args.l7 else git.run("rev-parse", "HEAD").strip()
                snap_commit = revision if args.l7 else git.run("rev-list", "-1", f"--before={args.before}", revision).strip()
                if not snap_commit:
                    die(f"{name}: no commit before {args.before}")
                merges = int(git.run("rev-list", "--count", "--merges", f"--before={args.before}", revision).strip())
                after = int(git.run("rev-list", "--count", f"--since={args.before}", revision).strip())
                pairs = []
                if "commit" in kinds:
                    pairs += mine_commits(name, git, args, stats, revision=revision if args.l7 else None, exclude=exclude)
                if kinds & {"doc", "test"}:
                    snap = Snapshot(name, git, snap_commit, stats, exclude=exclude)
                    if "doc" in kinds:
                        pairs += mine_docs(name, snap, args, stats)
                    if "test" in kinds:
                        pairs += mine_tests(name, snap, args, stats)
            finally:
                git.close()
            seen, unique = set(), []
            for p in pairs:
                if p["id"] not in seen:
                    seen.add(p["id"]); unique.append(p)
            unique.sort(key=lambda p: p["id"])
            unfiltered[name] = unique
            if not args.l7:
                write_jsonl(unf_dir / f"{name}.jsonl", unique)
            record["repos"][name] = {"head": head, "snapshot_commit": snap_commit, "unfiltered": len(unique),
                                     "merges_skipped": merges, "commits_after_cutoff_skipped": after}
        if args.l7:
            out.mkdir()
        else:
            (unf_dir / "mine.json").write_text(json.dumps({"record": record, "exclusions": dict(stats.excluded),
                                                           "notes_on_kept": dict(stats.pairs)}, indent=2, sort_keys=True) + "\n",
                                               encoding="utf-8", newline="\n")
    else:
        meta = json.loads((unf_dir / "mine.json").read_text(encoding="utf-8"))
        record = meta["record"]
        stats.excluded.update(meta["exclusions"])
        stats.pairs.update({k: v for k, v in meta["notes_on_kept"].items() if not k.endswith("kept_sharing_path_with_note")})
        unfiltered = {name: read_jsonl(unf_dir / f"{name}.jsonl") for name in record["repos"]}
    summary = finish(out, list(unfiltered), unfiltered, args, stats, record)
    print(json.dumps({"counts": summary["counts"], "note_files": len(summary["note_files"])}, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
