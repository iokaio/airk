# SPDX-License-Identifier: Apache-2.0
"""Fail when production Rust starts its own executable outside the reviewed roster.

One resident airk process per user and machine is the service. Production code
may start the running executable again only for the reviewed purposes below,
each a short-lived child that its owner waits for or kills. Test modules and
test-only files are excluded. The scan is per function: a function that both
names the running executable and contains a process-start primitive is a self
spawn. Every such function must be listed in ALLOWED with its reason, and every
listed function must still exist, so the roster never silently goes stale.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Reviewed roster: (crate-relative path, function) -> reason. Do not derive
# entries from the scan; add one only with a reviewed reason.
ALLOWED = {
    ("crates/airk-ingest/src/pdf_worker.rs", "extract_inner"):
        "one isolated PDF parse per document, in a Windows job or Unix process group "
        "with the 512 MiB / 10 s CPU / 15 s limits; killed with its job",
}
# Legacy-only sites that a single-executable runtime refuses or never reaches.
# Each is reported on every run and must be deleted by its owner; it is not
# permission to add another.
LEGACY_PENDING = {
    ("crates/airk-cli/src/embedding_client.rs", "spawn"):
        "paired 1.8.14 payload's airk-embed helper; verify_helper refuses a single "
        "executable (installer/inference owner)",
    ("crates/airk-cli/src/runtime_install/commands.rs", "run"):
        "paired-payload `runtime foreground` forwarding; a single executable serves "
        "directly (installer owner)",
    ("crates/airk-cli/src/unix_guardian/shared.rs", "serve"):
        "Unix shared provider guardian for paired legacy inference; removed with the "
        "process roles (step H10)",
    ("crates/airk-cli/src/unix_guardian/shared.rs", "spawn"):
        "Unix shared provider guardian start; removed with the process roles (step H10)",
    ("crates/airk-cli/src/unix_guardian.rs", "spawn"):
        "Unix process-guardian of a child-process project owner (`airk project-worker`); "
        "the service's in-process owners never start it; removed with the roles (step H10)",
}

SELF = re.compile(r"\bcurrent_exe\s*\(")
START = re.compile(
    r"\bCommand::new\s*\(|\.spawn\s*\(\s*\)|\bspawn_provider\s*\(|\bProviderCommand\s*\{"
    r"|\bnative_process::spawn\s*\(|\brun_limited\s*\("
)
TEST_ATTR = re.compile(r"#\[cfg\((?:test|all\(test\b[^\]]*)\)\]")
MOD_DECL = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;", re.M)
PATH_ATTR = re.compile(r'#\[path\s*=\s*"([^"]+)"\]\s*$')
# Starting a worker role is a self spawn wherever the image path came from.
ROLE_ARG = re.compile(
    r'"(project-worker|derived-worker|source-worker|process-guardian|shared-process-guardian)"'
)
FN = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")


def strip(text: str) -> str:
    """Blank comments and string/char literal contents, preserving offsets."""
    out = list(text)
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            out[i:j] = " " * (j - i)
            i = j
        elif text.startswith("/*", i):
            j = text.find("*/", i + 2)
            j = n if j < 0 else j + 2
            out[i:j] = [ch if ch == "\n" else " " for ch in text[i:j]]
            i = j
        elif c == "r" and re.match(r'r#*"', text[i:]) and (i == 0 or not text[i - 1].isalnum()):
            hashes = re.match(r"r(#*)\"", text[i:]).group(1)
            end = text.find('"' + hashes, i + 2 + len(hashes))
            end = n if end < 0 else end + 1 + len(hashes)
            out[i:end] = [ch if ch == "\n" else " " for ch in text[i:end]]
            i = end
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            j = min(j + 1, n)
            out[i + 1:j - 1] = [ch if ch == "\n" else " " for ch in text[i + 1:j - 1]]
            i = j
        elif c == "'" and re.match(r"'(\\.|[^\\'])'", text[i:]):
            m = re.match(r"'(\\.|[^\\'])'", text[i:])
            out[i + 1:i + len(m.group(0)) - 1] = " " * (len(m.group(0)) - 2)
            i += len(m.group(0))
        else:
            i += 1
    return "".join(out)


def block_end(text: str, open_brace: int) -> int:
    depth = 0
    for i in range(open_brace, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return i + 1
    return len(text)


def item_end(text: str, start: int) -> int:
    """End of the item that starts at `start`: its block, or its `;`."""
    brace, semi = text.find("{", start), text.find(";", start)
    if brace < 0 or (0 <= semi < brace):
        return semi + 1 if semi >= 0 else len(text)
    return block_end(text, brace)


def child_dir(path: Path) -> Path:
    return path.parent if path.name in ("lib.rs", "main.rs", "mod.rs") else path.with_suffix("")


def scan(root: Path) -> tuple[dict[tuple[str, str], list[int]], set[Path]]:
    files = sorted(p for p in (root / "crates").glob("*/src/**/*.rs"))
    test_files: set[Path] = set()
    original = {p: p.read_text(encoding="utf-8") for p in files}
    texts = {p: strip(text) for p, text in original.items()}
    # Test-only module files: declared under a test attribute, or inside one.
    changed = True
    while changed:
        changed = False
        for path, text in texts.items():
            for m in MOD_DECL.finditer(text):
                before = text[max(0, m.start() - 200):m.start()]
                attrs = before[before.rfind(";") + 1:] if ";" in before else before
                attrs = attrs[attrs.rfind("}") + 1:]
                if path in test_files or TEST_ATTR.search(attrs):
                    base = child_dir(path) / m.group(1)
                    candidates = [base.with_suffix(".rs"), base / "mod.rs"]
                    named = PATH_ATTR.search(original[path][max(0, m.start() - 200):m.start()])
                    if named:
                        candidates = [path.parent / named.group(1)]
                    for candidate in candidates:
                        if candidate in texts and candidate not in test_files:
                            test_files.add(candidate)
                            changed = True
    found: dict[tuple[str, str], list[int]] = {}
    for path, text in texts.items():
        if path in test_files:
            continue
        masked = list(text)
        for m in TEST_ATTR.finditer(text):
            end = item_end(text, m.end())
            masked[m.start():end] = [ch if ch == "\n" else " " for ch in text[m.start():end]]
        text = "".join(masked)
        # Literal role names were blanked by strip(); find them in the source.
        source = original[path]
        for m in ROLE_ARG.finditer(source):
            if text[m.start()] == " ":  # inside a masked test item or comment
                continue
            fns = [f for f in FN.finditer(text, 0, m.start())]
            for f in reversed(fns):
                end = item_end(text, f.end())
                if end > m.start():
                    if START.search(text, f.start(), end):
                        key = (path.relative_to(root).as_posix(), f.group(1))
                        found.setdefault(key, []).append(text.count("\n", 0, m.start()) + 1)
                    break
        for m in SELF.finditer(text):
            fns = [f for f in FN.finditer(text, 0, m.start())]
            for f in reversed(fns):
                end = item_end(text, f.end())
                if end > m.start():
                    if START.search(text, f.start(), end):
                        key = (path.relative_to(root).as_posix(), f.group(1))
                        found.setdefault(key, []).append(text.count("\n", 0, m.start()) + 1)
                    break
    return found, test_files


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args(argv)
    found, _ = scan(args.root.resolve())
    failures = []
    for key, lines in sorted(found.items()):
        if key in LEGACY_PENDING:
            print(f"{key[0]}:{lines[0]}: legacy self-spawn pending deletion: {LEGACY_PENDING[key]}")
        elif key not in ALLOWED:
            failures.append(f"{key[0]}:{lines[0]}: {key[1]} starts the running executable "
                            "outside the reviewed roster")
    for key in sorted({**ALLOWED, **LEGACY_PENDING}):
        if key not in found:
            failures.append(f"{key[0]}: reviewed self-spawn {key[1]} no longer found; "
                            "update the roster")
    for failure in failures:
        print(failure, file=sys.stderr)
    if not failures:
        print(f"self-spawn roster ok: {len(found)} reviewed production site(s), "
              f"{len(LEGACY_PENDING)} legacy pending deletion")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
