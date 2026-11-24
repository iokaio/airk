# SPDX-License-Identifier: Apache-2.0
"""Read-only probes of a ready fixture lifetime: what its service reports, kept as receipts.

Evaluation tooling, never product. A probe is one client call through the lifetime's own CLI and control root; it
starts, switches and stops nothing, and a failed call keeps the saved stdout and stderr beside the receipt. The
LH acceptance run (23 November 2026) used three one-off scripts for these reads; this is their maintained owner.

| Probe | Call | Receipt |
| --- | --- | --- |
| ``help`` | ``airk_help`` with a topic, on one repository's index; counts the words the reply names | ``help-<topic>.json`` |
| ``arbiter-status`` | ``arbiter status-legacy`` and ``arbiter status`` (the device-scoped read; the legacy read is the one the fixture's Configure keeps current) | ``<label>-status-legacy.json``, ``<label>-status-device.json`` |
| ``index-status`` | ``airk_status`` on every index of the handle | ``diag-airk-status-<repo>.json`` |

``python -I -B lifetime_probe.py --fixtures F --lifetime L --out DIR help [--repo munarium] [--topic arbiter]``
``python -I -B lifetime_probe.py --fixtures F --lifetime L --out DIR arbiter-status [--label after-switch]``
``python -I -B lifetime_probe.py --fixtures F --lifetime L --out DIR index-status``

Exit 0 when every call returned; 1 when any call failed (its error is printed and its output files kept).
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
COMMANDS_FOLDER = "probe-commands"
COMMAND_SECONDS = 300
HELP_WORDS = ("Focus", "Arbiter", "Large", "context_compact", "context_large", "separate")


def _fixture():
    spec = importlib.util.spec_from_file_location("answer_eval_fixture", HERE / "fixture.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules["answer_eval_fixture"] = module
    spec.loader.exec_module(module)
    return module


def keep_failed_output(commands, label: str, out: Path) -> None:
    """Copy the files a failed call left (its stdout, stderr and record) next to the receipts."""
    for found in sorted(Path(commands.folder).glob(f"*{label}*")):
        shutil.copyfile(found, out / found.name)


def _call(commands, label: str, argv: tuple, receipt: Path, out: Path) -> dict:
    """One CLI call: its stdout is copied to ``receipt``; a failure keeps the call's files and reports the error."""
    try:
        path, elapsed = commands.run(label, *argv)
    except Exception as error:  # the call's own refusal is the finding, not a crash of the probe
        keep_failed_output(commands, label, out)
        return {"label": label, "ok": False, "error": f"{type(error).__name__}: {str(error)[:600]}"}
    shutil.copyfile(path, receipt)
    return {"label": label, "ok": True, "receipt": receipt.name, "seconds": round(elapsed / 1e9, 2)}


def probe_help(commands, handle: dict, out: Path, repo: str, topic: str, words=HELP_WORDS) -> dict:
    """``airk_help`` for ``topic`` on ``repo``'s index, and how often the reply names each of ``words``."""
    out.mkdir(parents=True, exist_ok=True)
    index = handle["indexes"][repo]["index_id"]
    result = _call(commands, f"help-{topic}",
                   ("tools", "call", "airk_help", "--index", index, "--args", json.dumps({"topic": topic})),
                   out / f"help-{topic}.json", out)
    if result["ok"]:
        text = json.dumps(json.loads((out / result["receipt"]).read_bytes()))
        result["word_counts"] = {word: text.count(word) for word in words}
    return result


def probe_arbiter_status(commands, out: Path, label: str) -> list[dict]:
    out.mkdir(parents=True, exist_ok=True)
    return [_call(commands, f"{label}-{name}", ("arbiter", command), out / f"{label}-{name}.json", out)
            for name, command in (("status-legacy", "status-legacy"), ("status-device", "status"))]


def probe_index_status(commands, handle: dict, out: Path) -> list[dict]:
    out.mkdir(parents=True, exist_ok=True)
    return [_call(commands, f"diag-airk-status-{repo}",
                  ("tools", "call", "airk_status", "--index", row["index_id"], "--args", "{}"),
                  out / f"diag-airk-status-{repo}.json", out)
            for repo, row in handle["indexes"].items()]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--fixtures", required=True)
    parser.add_argument("--lifetime", required=True)
    parser.add_argument("--out", type=Path, required=True)
    commands = parser.add_subparsers(dest="probe", required=True)
    sub = commands.add_parser("help")
    sub.add_argument("--repo", default="munarium")
    sub.add_argument("--topic", default="arbiter")
    sub.add_argument("--word", action="append", help="a word to count in the reply (default: the Arbiter words)")
    sub = commands.add_parser("arbiter-status")
    sub.add_argument("--label", default="probe")
    commands.add_parser("index-status")
    args = parser.parse_args(argv)
    fixture = _fixture()
    life, handle = fixture.load_handle(argparse.Namespace(fixtures=args.fixtures, lifetime=args.lifetime))
    client = fixture.client_commands(life, handle, COMMANDS_FOLDER, COMMAND_SECONDS)
    if args.probe == "help":
        results = [probe_help(client, handle, args.out, args.repo, args.topic, tuple(args.word or HELP_WORDS))]
    elif args.probe == "arbiter-status":
        results = probe_arbiter_status(client, args.out, args.label)
    else:
        results = probe_index_status(client, handle, args.out)
    for result in results:
        print(json.dumps(result, sort_keys=True))
    return 0 if all(r["ok"] for r in results) else 1


if __name__ == "__main__":
    sys.exit(main())
