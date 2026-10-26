# SPDX-License-Identifier: Apache-2.0
"""Reproduce fixed-WAL pressure with generated code and recover using a new runtime."""
import argparse
from contextlib import closing
import hashlib
import importlib.util
import json
from pathlib import Path
import sqlite3
import tempfile
import time

spec = importlib.util.spec_from_file_location("service_clients", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)


def qualify(previous, current):
    started = time.monotonic()
    with tempfile.TemporaryDirectory(prefix="airk-storage-recovery-") as temporary:
        owner = Path(temporary).resolve()
        source, data = owner / "source", owner / "data"
        source.mkdir()
        data.mkdir()
        originals = {}
        for number in range(128):
            text = "from normalizer import normalize\n\n" + "\n".join(
                f"def polar_case_{number}_{function}(value: str) -> str:\n    return normalize(value)\n"
                for function in range(1024)
            )
            path = source / f"fixture_{number}.py"
            path.write_text(text, encoding="utf-8")
            originals[path] = hashlib.sha256(path.read_bytes()).hexdigest()
        old = clients.Fixture(previous, owner, job_timeout=600)
        try:
            old.start()
            old.register("pressure", source, data)
            failed = old.wait(old.enqueue("pressure", "index"), expected="failed")
            assert failed["outcome"]["code"] == "sqlite_storage_full", failed
            projects = dict(old.projects)
        finally:
            old.close()
        databases = list(data.glob("collections-1.4/pressure/incarnations/*/current.sqlite3"))
        assert len(databases) == 1, databases
        with closing(sqlite3.connect(databases[0].as_uri() + "?mode=ro", uri=True)) as connection:
            phase = connection.execute("SELECT phase FROM current_state").fetchone()[0]
            assert phase in ("ready", "prepared", "applying"), phase
            pending_before = connection.execute("SELECT count(*) FROM current_pending").fetchone()[0]
        print(f"Previous runtime reproduced sqlite_storage_full (phase={phase}); testing recovery and completion.", flush=True)
        new = clients.Fixture(current, owner, job_timeout=600)
        try:
            new.start()
            new.projects = projects
            recovered = new.wait(new.enqueue("pressure", "recover"))
            new.wait(new.enqueue("pressure", "index"))
            status = new.tool("pressure", "airk_status")
            assert status["files"] == 128, status
            hit = new.tool("pressure", "airk_symbols", {"name": "polar_case_127_63"})
            assert hit.get("items") or hit.get("hits"), hit
            assert all(hashlib.sha256(path.read_bytes()).hexdigest() == digest for path, digest in originals.items())
            with closing(sqlite3.connect(databases[0].as_uri() + "?mode=ro", uri=True)) as connection:
                assert connection.execute("SELECT phase FROM current_state").fetchone()[0] == "ready"
                assert connection.execute("SELECT count(*) FROM current_pending").fetchone()[0] == 0
            return {
                "schema": 1,
                "previous_version": json.loads((previous / "airk-runtime.json").read_text())["version"],
                "version": json.loads((current / "airk-runtime.json").read_text())["version"],
                "files": 128, "functions_per_file": 1024,
                "previous_failure": failed["outcome"]["code"], "previous_phase": phase,
                "pending_publications_before": pending_before,
                "recovery_state": recovered["state"], "final_phase": "ready",
                "pending_replay_rows": 0, "source_bytes_preserved": True,
                "elapsed_seconds": round(time.monotonic() - started, 3),
            }
        finally:
            new.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--previous-runtime-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    report = qualify(args.previous_runtime_dir.resolve(strict=True), args.runtime_dir.resolve(strict=True))
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
