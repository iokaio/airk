# SPDX-License-Identifier: Apache-2.0
"""Opt-in isolated current-user service lifecycle smoke; always attempts owned cleanup."""
import argparse
from contextlib import contextmanager
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


@contextmanager
def fixture_directory():
    directory = Path(tempfile.mkdtemp(prefix="airk-service-fixture-")).resolve()
    control = {"preserve": False}
    try:
        yield directory, control
    finally:
        if not control["preserve"]:
            assert directory.parent == Path(tempfile.gettempdir()).resolve()
            assert directory.name.startswith("airk-service-fixture-")
            shutil.rmtree(directory)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--allow-user-service-fixture", action="store_true", required=True,
                        help="Explicitly allow a unique temporary per-user service registration")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    observations = []
    cleanup_error = None
    with fixture_directory() as (base, cleanup):
        source, data = base / "source", base / "derived"
        source.mkdir()
        fixture = source / "fixture.txt"
        fixture.write_text("isolated service original\n", encoding="utf-8")
        common = [str(binary), "--root", str(source), "--data-dir", str(data),
                  "--collection", "service-fixture"]

        def command(*tail, check=True):
            result = subprocess.run([*common, *tail], text=True, encoding="utf-8",
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=70)
            observations.append({"command": list(tail), "code": result.returncode,
                                 "stdout": result.stdout[-16384:], "stderr": result.stderr[-4096:]})
            if check and result.returncode:
                raise RuntimeError(f"{tail}: {result.stderr}")
            return json.loads(result.stdout) if result.returncode == 0 else None

        attempted = False
        try:
            plan = command("service", "plan")
            assert plan["id"].startswith("airk-")
            assert str(data) in " ".join(plan["arguments"])
            status = command("service", "status")
            assert status["installed"] is False and status["loaded"] is False
            attempted = True
            command("service", "install")
            deadline = time.monotonic() + 45
            while True:
                indexed = command("status", check=False)
                if indexed and indexed.get("files") == 1:
                    break
                if time.monotonic() > deadline:
                    raise RuntimeError("installed worker did not index its fixture")
                time.sleep(1)
            generation = indexed["generation"]
            fixture.write_text("isolated service changed through native events\n", encoding="utf-8")
            deadline = time.monotonic() + 45
            while command("status")["generation"] <= generation:
                if time.monotonic() > deadline:
                    raise RuntimeError("installed worker did not index a native event")
                time.sleep(1)
            stopped = command("service", "stop")
            assert stopped["running"] is False
            command("service", "start")
            removed = command("service", "uninstall")
            assert removed["installed"] is False and removed["loaded"] is False
            attempted = False
            assert fixture.read_text(encoding="utf-8") == "isolated service changed through native events\n"
        finally:
            if attempted:
                try:
                    status = command("service", "status", check=False)
                    if status is None or status["loaded"] is None:
                        raise RuntimeError("Cannot establish owned fixture service status")
                    if status and status["installed"]:
                        command("service", "uninstall")
                    elif status["loaded"]:
                        raise RuntimeError("Fixture service exists without its expected ownership receipt")
                except Exception as error:
                    cleanup_error = str(error)
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps({"isolated_fixture": True,
                "fixture_root": str(base),
                "observations": observations, "cleanup_error": cleanup_error}, indent=2) + "\n", encoding="utf-8")
            if cleanup_error:
                cleanup["preserve"] = True
                raise RuntimeError("Owned fixture service cleanup failed; inspect receipt: " + cleanup_error)
    print("Isolated service lifecycle and native-event indexing passed")


if __name__ == "__main__":
    main()
