# SPDX-License-Identifier: Apache-2.0
"""Check the one-resident-process topology of an installed AIRK runtime.

One resident airk process per user and machine is the service. The only other
AIRK processes allowed are temporary: one `service session` per VS Code window,
at most one PDF parser child, the installer's awaited `runtime self-test`
child, a user-opened `tools shell`, and user-typed CLI one-shots. Worker roles
(`project-worker`, `derived-worker`, `source-worker`, the process guardians),
stdio MCP, legacy role aliases and a second service are failures.

Modes:
  snapshot   classify every AIRK process now and check the counts;
  watch      sample for a period and fail on any new process outside the
             allowed roles (for example while the service is stopped and
             Explorer polls);
  cli-burst  run N concurrent CLI one-shots from a plain environment, wait for
             them, then check that only the service (and existing sessions)
             remain;
  refuse-serve  run `airk serve` against a running service and require the
             typed `service_already_running` refusal and still one service.

Process collection is native on Windows (CIM) and Linux (/proc). macOS uses
`ps` and is unverified.
"""
from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import subprocess
import sys
import time
from dataclasses import asdict, dataclass
from pathlib import Path

# Image names that are AIRK processes. Only the main image is valid in a
# single-executable install; the aliases belong to retired paired layouts.
MAIN_IMAGES = {"airk.exe", "airk"}
ALIAS_IMAGES = {
    "airk-service.exe", "airk-index.exe", "airk-source.exe", "airk-embed.exe",
    "airk-launcher.exe", "airk-service", "airk-index", "airk-source", "airk-embed",
    "airk-launcher",
}
# Leading global options that take a value.
VALUED_GLOBALS = {"--root", "--service-install-dir", "--install-dir"}
SERVICE_COMMANDS = {
    ("serve",), ("runtime", "managed-serve"), ("runtime", "login-serve"),
    ("runtime", "foreground"), ("managed-serve",), ("login-serve",), ("foreground",),
    ("host",),
}
FORBIDDEN_COMMANDS = {
    ("project-worker",): "worker role process",
    ("derived-worker",): "worker role process",
    ("source-worker",): "worker role process",
    ("process-guardian",): "Unix process guardian",
    ("shared-process-guardian",): "Unix process guardian",
    ("mcp",): "stdio MCP process",
    ("bridge",): "stdio bridge process",
}


@dataclass(frozen=True)
class Process:
    pid: int
    parent: int
    image: str
    args: tuple[str, ...]


@dataclass(frozen=True)
class Classified:
    pid: int
    parent: int
    image: str
    role: str
    command: str
    problem: str | None


def command_words(args: tuple[str, ...]) -> tuple[str, ...]:
    """The subcommand words after leading global options."""
    words: list[str] = []
    index = 0
    while index < len(args):
        arg = args[index]
        if not words and arg.startswith("--"):
            name = arg.split("=", 1)[0]
            index += 2 if name in VALUED_GLOBALS and "=" not in arg else 1
            continue
        if arg.startswith("-"):
            break
        words.append(arg)
        if len(words) == 2:
            break
        index += 1
    return tuple(words)


def classify(process: Process) -> Classified:
    name = Path(process.image).name.lower()
    words = command_words(process.args)
    command = " ".join(words) or "<none>"

    def result(role: str, problem: str | None = None) -> Classified:
        return Classified(process.pid, process.parent, process.image, role, command, problem)

    if name in ALIAS_IMAGES:
        return result("alias", f"retired role alias image {name}")
    for key, reason in FORBIDDEN_COMMANDS.items():
        if words[: len(key)] == key:
            return result("forbidden", reason)
    if words[:1] == ("__airk_pdf_worker",):
        return result("pdf")
    if words in SERVICE_COMMANDS or words[:1] in SERVICE_COMMANDS:
        return result("service")
    if words == ("service", "session"):
        return result("session")
    if words == ("runtime", "self-test"):
        return result("self-test")
    if words == ("tools", "shell"):
        return result("tools-shell")
    return result("one-shot")


def evaluate(
    processes: list[Classified],
    *,
    services: int,
    max_sessions: int,
    allow_one_shots: bool,
) -> list[str]:
    """Every violation of the allowed topology; empty means it holds."""
    failures = [f"pid {p.pid} ({p.command}): {p.problem}" for p in processes if p.problem]
    counts: dict[str, int] = {}
    for process in processes:
        counts[process.role] = counts.get(process.role, 0) + 1
    if counts.get("service", 0) != services:
        failures.append(f"expected {services} service process(es), found {counts.get('service', 0)}")
    if counts.get("session", 0) > max_sessions:
        failures.append(
            f"expected at most {max_sessions} window session(s), found {counts.get('session', 0)}")
    if counts.get("pdf", 0) > 1:
        failures.append(f"more than one PDF parser alive: {counts['pdf']}")
    if counts.get("self-test", 0) > 1:
        failures.append(f"more than one installer self-test child: {counts['self-test']}")
    if not allow_one_shots and counts.get("one-shot", 0):
        names = sorted({p.command for p in processes if p.role == "one-shot"})
        failures.append(f"{counts['one-shot']} CLI one-shot(s) still running: {', '.join(names)}")
    return failures


def windows_argv(command_line: str) -> tuple[str, ...]:
    import ctypes
    from ctypes import wintypes

    if not command_line:
        return ()
    count = ctypes.c_int()
    shell32 = ctypes.windll.shell32
    shell32.CommandLineToArgvW.restype = ctypes.POINTER(wintypes.LPWSTR)
    argv = shell32.CommandLineToArgvW(command_line, ctypes.byref(count))
    if not argv:
        return ()
    try:
        return tuple(argv[index] for index in range(count.value))
    finally:
        ctypes.windll.kernel32.LocalFree(argv)


def collect() -> list[Process]:
    names = sorted(MAIN_IMAGES | ALIAS_IMAGES)
    if sys.platform == "win32":
        filter_ = " or ".join(f"Name='{name}'" for name in names if name.endswith(".exe"))
        script = (
            f'Get-CimInstance Win32_Process -Filter "{filter_}" | '
            "Select-Object ProcessId,ParentProcessId,ExecutablePath,CommandLine | "
            "ConvertTo-Json -Compress"
        )
        raw = subprocess.run(
            ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
            check=True, capture_output=True, text=True, timeout=60,
        ).stdout.strip()
        rows = json.loads(raw) if raw else []
        rows = rows if isinstance(rows, list) else [rows]
        return [
            Process(
                int(row["ProcessId"]), int(row["ParentProcessId"]),
                row.get("ExecutablePath") or "",
                windows_argv(row.get("CommandLine") or "")[1:],
            )
            for row in rows
        ]
    found: list[Process] = []
    if Path("/proc").is_dir():
        for entry in Path("/proc").iterdir():
            if not entry.name.isdigit():
                continue
            try:
                image = os.readlink(entry / "exe")
                if Path(image).name not in names:
                    continue
                argv = (entry / "cmdline").read_bytes().split(b"\0")
                stat = (entry / "stat").read_text().rsplit(")", 1)[1].split()
                found.append(Process(int(entry.name), int(stat[1]), image,
                                     tuple(a.decode(errors="replace") for a in argv[1:] if a)))
            except OSError:
                continue
        return found
    # macOS: unverified.
    raw = subprocess.run(["ps", "-axww", "-o", "pid=,ppid=,command="],
                         check=True, capture_output=True, text=True).stdout
    for line in raw.splitlines():
        parts = line.split()
        if len(parts) >= 3 and Path(parts[2]).name in names:
            found.append(Process(int(parts[0]), int(parts[1]), parts[2], tuple(parts[3:])))
    return found


def owned(processes: list[Process], install_root: Path | None) -> list[Process]:
    if install_root is None:
        return processes
    root = str(install_root.resolve()).lower()
    return [p for p in processes if p.image and str(Path(p.image).resolve()).lower().startswith(root)]


def snapshot(args: argparse.Namespace) -> tuple[list[Classified], list[str]]:
    deadline = time.monotonic() + args.settle
    while True:
        processes = [classify(p) for p in owned(collect(), args.install_root)]
        failures = evaluate(processes, services=args.services, max_sessions=args.max_sessions,
                            allow_one_shots=args.allow_one_shots)
        if not failures or time.monotonic() >= deadline:
            return processes, failures
        time.sleep(0.5)


def report(name: str, processes: list[Classified], failures: list[str], extra: dict | None = None) -> int:
    print(json.dumps({
        "schema": "airk-process-topology/1",
        "checkpoint": name,
        "passed": not failures,
        "failures": failures,
        "processes": [asdict(p) for p in processes],
        **(extra or {}),
    }, indent=2))
    return 0 if not failures else 1


def watch(args: argparse.Namespace) -> int:
    allowed = set(args.allow_new or [])
    before = {p.pid for p in owned(collect(), args.install_root)}
    seen: dict[int, Classified] = {}
    deadline = time.monotonic() + args.seconds
    while time.monotonic() < deadline:
        for process in owned(collect(), args.install_root):
            if process.pid not in before and process.pid not in seen:
                seen[process.pid] = classify(process)
        time.sleep(args.interval)
    failures = [f"new process pid {p.pid} ({p.role}: {p.command})"
                for p in seen.values() if p.role not in allowed]
    return report("watch", list(seen.values()), failures, {"seconds": args.seconds})


def plain_environment() -> dict[str, str]:
    """A terminal outside VS Code: no editor or extension-host variables."""
    return {k: v for k, v in os.environ.items()
            if not k.upper().startswith(("VSCODE_", "ELECTRON_", "AIRK_"))}


def cli_burst(args: argparse.Namespace) -> int:
    command = [str(args.airk), *args.command]
    env = plain_environment()

    def run(_: int) -> int:
        return subprocess.run(command, env=env, stdin=subprocess.DEVNULL,
                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                              timeout=args.timeout).returncode

    with concurrent.futures.ThreadPoolExecutor(args.count) as pool:
        codes = list(pool.map(run, range(args.count)))
    processes, failures = snapshot(args)
    return report("cli-burst", processes, failures, {"command": command, "exit_codes": codes})


def refuse_serve(args: argparse.Namespace) -> int:
    result = subprocess.run(
        [str(args.airk), "serve", "--runtime-dir", str(args.runtime_dir)],
        env=plain_environment(), stdin=subprocess.DEVNULL, capture_output=True, text=True,
        timeout=args.timeout,
    )
    processes, failures = snapshot(args)
    if result.returncode == 0:
        failures.append("a second `airk serve` was not refused")
    elif "service_already_running" not in result.stderr + result.stdout:
        failures.append("the refusal is not the typed service_already_running")
    return report("refuse-serve", processes, failures,
                  {"exit_code": result.returncode, "stderr": result.stderr[-2000:]})


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--install-root", type=Path,
                        help="only count processes whose image is under this install root")
    parser.add_argument("--services", type=int, default=1)
    parser.add_argument("--max-sessions", type=int, default=0)
    parser.add_argument("--allow-one-shots", action="store_true")
    parser.add_argument("--settle", type=float, default=10.0,
                        help="seconds to wait for temporary processes to exit")
    sub = parser.add_subparsers(dest="mode", required=True)
    sub.add_parser("snapshot")
    w = sub.add_parser("watch")
    w.add_argument("--seconds", type=float, default=60.0)
    w.add_argument("--interval", type=float, default=0.25)
    w.add_argument("--allow-new", action="append", help="role allowed to appear")
    b = sub.add_parser("cli-burst")
    b.add_argument("--airk", type=Path, required=True)
    b.add_argument("--count", type=int, default=20)
    b.add_argument("--timeout", type=float, default=120.0)
    b.add_argument("command", nargs="*", default=["runtime", "location"])
    r = sub.add_parser("refuse-serve")
    r.add_argument("--airk", type=Path, required=True)
    r.add_argument("--runtime-dir", type=Path, required=True)
    r.add_argument("--timeout", type=float, default=60.0)
    args = parser.parse_args(argv)
    if args.mode == "snapshot":
        return report("snapshot", *snapshot(args))
    if args.mode == "watch":
        return watch(args)
    if args.mode == "cli-burst":
        return cli_burst(args)
    return refuse_serve(args)


if __name__ == "__main__":
    raise SystemExit(main())
