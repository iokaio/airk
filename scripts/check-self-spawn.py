# SPDX-License-Identifier: Apache-2.0
"""Fail when production Rust starts its own executable outside the reviewed roster.

One resident airk process per user and machine is the service. Production code
may start the running executable again only for the reviewed purposes below,
each a short-lived child that its owner waits for or kills. Test modules and
test-only files are excluded. The scan is per function: a function that both
names the running executable and contains a process-start primitive is a self
spawn. Every such function must be listed in ALLOWED with its reason, and every
listed function must still exist, so the roster never silently goes stale.

An image path can also reach a start through a parameter, a field or a const,
which no per-function scan can follow. So every production function that
creates any process (std/tokio commands, the image-path wrappers, native
Windows and POSIX calls, OS-manager invocations) must also be listed in STARTS
with what it starts; a new start fails until a reviewer classifies it.
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
    ("crates/airk-cli/src/runtime_install/commands.rs", "run_with"):
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

# Every production function that creates any process, reviewed. A function
# that receives an image path from its caller is invisible to the per-function
# self-spawn scan above, so each start is listed here with what it starts.
# Kinds: service (the one resident service), one-shot (an awaited AIRK image
# run), pdf, legacy (paired-payload or child-owner paths pending deletion),
# external (a program that is not AIRK), primitive (starts what its caller
# built; the caller is listed separately).
STARTS = {
    ("crates/airk-cli/src/embedding_client.rs", "spawn"):
        "legacy: paired airk-embed helper; verify_helper refuses a single executable",
    ("crates/airk-cli/src/embedding_pool_process.rs", "start_from_main"):
        "legacy: paired pool helper; runtime_pair::verify_helper refuses a single executable",
    ("crates/airk-cli/src/embedding_pool_process/owned.rs", "spawn"):
        "primitive: legacy paired pool helper start (callers build the helper command)",
    ("crates/airk-cli/src/embedding_pool_process/shared.rs", "spawn"):
        "legacy: shared guarded pool helper in a named job (paired payloads only)",
    ("crates/airk-cli/src/explorer_assistance.rs", "command"):
        "external: VS Code's own JavaScript runtime for the Explorer Assistance server "
        "named by an extension-written, hash-checked descriptor; not an airk process",
    ("crates/airk-process/src/bounded_command.rs", "execute_native_observation"):
        "primitive: bounded native command with a caller-built invocation",
    ("crates/airk-process/src/bounded_command.rs", "execute_native_program"):
        "primitive: bounded native command with a caller-built invocation",
    ("crates/airk-process/src/bounded_command.rs", "execute_with_timeout"):
        "primitive: the bounded native command runner (no shell, output and time limits)",
    ("crates/airk-process/src/bounded_command.rs", "spawn"):
        "primitive: std spawn without error dialogs; callers build the command",
    ("crates/airkctl/src/launch.rs", "launch"):
        "external: the user's Claude Code, Codex or VS Code client in the foreground "
        "(`airkctl agents launch`, U91); not an airk process",
    ("crates/airk-process/src/detached.rs", "spawn_detached"):
        "primitive: detached Windows service start that inherits no handles",
    ("crates/airk-cli/src/runtime_install.rs", "self_test_observation"):
        "one-shot: installer `runtime self-test` child of a staged image, awaited with a deadline",
    ("crates/airk-cli/src/runtime_install.rs", "start_with_observation_clock"):
        "service: on-demand `serve` only when login registration was refused; the "
        "single-instance lease refuses a second resident",
    ("crates/airk-cli/src/runtime_install/activation/catalog_upgrade/authority.rs", "qualify"):
        "one-shot: candidate `runtime catalog-authority-probe`, awaited",
    ("crates/airk-cli/src/runtime_install/commands.rs", "run_with"):
        "legacy: paired-payload `runtime foreground` forwarding and helper identity check",
    ("crates/airk-cli/src/service_manager.rs", "check_executable_identity"):
        "one-shot: `<helper> --version` identity check of a paired helper, bounded",
    ("crates/airk-cli/src/service_manager.rs", "launchctl"):
        "external: /bin/launchctl through the bounded native command runner",
    ("crates/airk-cli/src/service_manager.rs", "query"):
        "external: Windows PowerShell scheduled-task query, bounded, task identity as data",
    ("crates/airk-cli/src/service_manager.rs", "schtasks"):
        "external: schtasks.exe through the bounded native command runner",
    ("crates/airk-cli/src/service_manager.rs", "systemctl"):
        "external: /usr/bin/systemctl --user through the bounded native command runner",
    ("crates/airk-cli/src/service_manager_macos.rs", "invoke"):
        "one-shot: the verified Mac login target's `runtime` login status/register/"
        "unregister command, bounded",
    ("crates/airk-cli/src/standalone_setup/launch.rs", "launch_client"):
        "external: `airk setup --launch` runs the chosen coding client in the user's "
        "terminal and waits for it",
    ("crates/airk-cli/src/universal_launcher.rs", "run"):
        "one-shot: the universal launcher forwards a typed command to the selected "
        "installation and waits for it",
    ("crates/airk-cli/src/unix_guardian.rs", "serve"):
        "legacy: Unix process-guardian provider start for a child-process owner (step H10)",
    ("crates/airk-cli/src/unix_guardian.rs", "spawn"):
        "legacy: Unix process-guardian start for a child-process owner (step H10)",
    ("crates/airk-cli/src/unix_guardian/shared.rs", "serve"):
        "legacy: Unix shared provider guardian for paired inference (step H10)",
    ("crates/airk-cli/src/unix_guardian/shared.rs", "spawn"):
        "legacy: Unix shared provider guardian start (step H10)",
    ("crates/airk-ingest/src/pdf_worker.rs", "extract_inner"):
        "pdf: the one isolated PDF parser child",
    ("crates/airk-ingest/src/provider_process.rs", "run"):
        "primitive: bounded provider run for a caller-built ProviderCommand",
    ("crates/airk-ingest/src/provider_process.rs", "spawn_limited"):
        "primitive: provider start in a Windows job or Unix process group with limits",
    ("crates/airkctl/src/record.rs", "by_role"):
        "one-shot: `airkctl service record-error` while the service is down (U75) runs the "
        "install record's verified `airk` as the private `record-error` role and awaits it",
    ("crates/airkctl/src/start.rs", "launch"):
        "service: `airkctl service start` (split 3b.1d) starts the install record's verified "
        "`airk` as the private `service-detached` role, admitted by a single-use owner-private "
        "start request; the single-instance lease refuses a second resident",
    ("crates/airk-process/src/child_group.rs", "spawn"):
        "primitive: Unix owned process group start",
    ("crates/airk-process/src/child_job_named.rs", "spawn_guarded"):
        "primitive: Windows start inside a named job",
    ("crates/airk-process/src/child_job_named.rs", "spawn_guarded_io"):
        "primitive: CreateProcessW atomically inside a named job",
    ("crates/airk-process/src/child_job_named.rs", "spawn_guarded_with_stderr"):
        "primitive: Windows start inside a named job with a stderr handle",
}

SELF = re.compile(r"\bcurrent_exe\s*\(")
# Every way production Rust can create a process: std/tokio commands, the
# reviewed wrappers that take an image path instead of a Command, the native
# Windows and POSIX calls, and the OS-manager invocation builder.
START = re.compile(
    r"\bCommand::new\s*\(|\.spawn\s*\(\s*\)|\bspawn_provider\s*\(|\bProviderCommand\s*\{"
    r"|\bnative_process::spawn\s*\(|\brun_limited\s*\(|\bspawn_detached\s*\("
    r"|\bspawn_guarded(?:_with_stderr|_io)?\s*\(|\bCreateProcess(?:AsUser)?W\s*\("
    r"|\bShellExecute(?:Ex)?W\s*\(|\bposix_spawnp?\s*\(|\bfork\s*\(\s*\)"
    r"|\bexecv[pe]*\s*\(|\bInvocation::new\s*\("
)
# A START match that is the item's own definition (`fn run_limited(`,
# `struct ProviderCommand {`) is not a start.
DEFINITION = re.compile(r"\b(?:fn|struct|enum|impl|for)\s+$")
TEST_ATTR = re.compile(r"#\[cfg\((?:test|all\(test\b[^\]]*)\)\]")
MOD_DECL = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;", re.M)
PATH_ATTR = re.compile(r'#\[path\s*=\s*"([^"]+)"\]\s*$')
# Starting a worker role is a self spawn wherever the image path came from.
ROLE_ARG = re.compile(
    r'"(project-worker|derived-worker|source-worker|process-guardian|shared-process-guardian)"'
)
FN = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")
# The key of a match outside every function (a const, static or macro body).
MODULE_ITEM = "<module item>"


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
        elif c == "r" and re.match(r'r#*"', text[i:]) and (
            i == 0 or not text[i - 1].isalnum() or (
                # br"..." and cr"..." are raw strings too.
                text[i - 1] in "bc" and (i == 1 or not text[i - 2].isalnum()))):
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
    """End of the item that starts at `start`: its block, or its `;`.

    A `;` inside the signature's parentheses or brackets (`[u8; 4]`) does not
    end the item."""
    depth = 0
    for i in range(start, len(text)):
        c = text[i]
        if c in "([":
            depth += 1
        elif c in ")]":
            depth = max(0, depth - 1)
        elif depth == 0 and c == "{":
            return block_end(text, i)
        elif depth == 0 and c == ";":
            return i + 1
    return len(text)


def child_dir(path: Path) -> Path:
    return path.parent if path.name in ("lib.rs", "main.rs", "mod.rs") else path.with_suffix("")


def scan(root: Path, with_starts: bool = False):
    """Self-spawn sites and test-only files; with `with_starts`, also every
    production function that starts any process."""
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
    starts: dict[tuple[str, str], list[int]] = {}
    for path, text in texts.items():
        if path in test_files:
            continue
        masked = list(text)
        for m in TEST_ATTR.finditer(text):
            end = item_end(text, m.end())
            masked[m.start():end] = [ch if ch == "\n" else " " for ch in text[m.start():end]]
        text = "".join(masked)
        relative = path.relative_to(root).as_posix()

        def enclosing(position: int) -> tuple[str, int, int]:
            """The innermost function around `position`, or the module item."""
            for f in reversed(list(FN.finditer(text, 0, position))):
                end = item_end(text, f.end())
                if end > position:
                    return f.group(1), f.start(), end
            return MODULE_ITEM, 0, len(text)

        def line(position: int) -> int:
            return text.count("\n", 0, position) + 1

        def starts_in(begin: int, end: int) -> bool:
            return any(not DEFINITION.search(text, max(0, m.start() - 16), m.start())
                       for m in START.finditer(text, begin, end))

        for m in START.finditer(text):
            if DEFINITION.search(text, max(0, m.start() - 16), m.start()):
                continue
            starts.setdefault((relative, enclosing(m.start())[0]), []).append(line(m.start()))
        # Literal role names were blanked by strip(); find them in the source.
        source = original[path]
        for m in ROLE_ARG.finditer(source):
            if text[m.start()] == " ":  # inside a masked test item or comment
                continue
            name, begin, end = enclosing(m.start())
            if name == MODULE_ITEM or starts_in(begin, end):
                # A role name outside any function (a const or static) can reach
                # a start anywhere, so it is reported like a start.
                found.setdefault((relative, name), []).append(line(m.start()))
        for m in SELF.finditer(text):
            name, begin, end = enclosing(m.start())
            if starts_in(begin, end):
                found.setdefault((relative, name), []).append(line(m.start()))
    if with_starts:
        return found, test_files, starts
    return found, test_files


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args(argv)
    found, _, starts = scan(args.root.resolve(), with_starts=True)
    failures = []
    for key, lines in sorted(starts.items()):
        if key not in STARTS:
            failures.append(f"{key[0]}:{lines[0]}: {key[1]} starts a process that is not in "
                            "the reviewed start roster; review what it starts and list it")
    for key in sorted(STARTS):
        if key not in starts:
            failures.append(f"{key[0]}: reviewed process start {key[1]} no longer found; "
                            "update the roster")
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
              f"{len(LEGACY_PENDING)} legacy pending deletion, "
              f"{len(starts)} reviewed process start(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
