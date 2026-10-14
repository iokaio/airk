# SPDX-License-Identifier: Apache-2.0
"""Isolated process-death and optional full-volume admission campaign.

This simulates abrupt process termination, not loss of a device's volatile cache.
The full-disk case requires an explicitly isolated Linux tmpfs mount and never
fills a normal host filesystem.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run_campaign(binary, base, attempts, full_disk):
    source, data = base / "source", base / "state"
    source.mkdir()
    data.mkdir()
    environment = {**os.environ, "AIRK_USAGE_MODE": "disabled"}
    source_file = source / "requirement.md"
    source_file.write_text("REQOLDMARKER: Original requirement shall remain readable.\n", encoding="utf-8")

    def command(*args):
        return [str(binary), "--root", str(source), "--data-dir", str(data), *args]

    def call(*args, success=True):
        result = subprocess.run(command(*args), env=environment, capture_output=True, timeout=180)
        if success:
            if result.returncode:
                raise RuntimeError(result.stderr.decode("utf-8", errors="replace"))
            return json.loads(result.stdout)
        if result.returncode == 0:
            raise AssertionError("Expected bounded publication failure")
        return result.stderr.decode("utf-8", errors="replace")[-4096:]

    call("index")
    cases = []
    # Several timings cover startup, ingestion, component staging and publication.
    # Classify the actually observed phase; never pretend timing proves a phase.
    for ordinal in range(attempts):
        old_generation = call("status")["generation"]
        old_hash = call("info", "requirement.md")["content_hash"]
        old_line = source_file.read_text(encoding="utf-8").splitlines()[0]
        marker = f"REQFAULT{ordinal:04}"
        source_file.write_text(f"{marker}: New requirement shall be atomic.\n" +
                               ("A fictional requirement used only for failure qualification.\n" * 3000), encoding="utf-8")
        expected_hash = digest(source_file)
        before = {path.relative_to(data).as_posix() for path in data.rglob("*") if path.is_file()}
        with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
            child = subprocess.Popen(command("index"), env=environment, stdout=stdout, stderr=stderr)
            delay = [0.005, 0.025, 0.1, 0.3, 0.8, 1.5][ordinal % 6]
            started = time.monotonic()
            time.sleep(delay)
            seen = {path.relative_to(data).as_posix() for path in data.rglob("*") if path.is_file()}
            killed = child.poll() is None
            if killed:
                child.kill()
            child.wait(timeout=30)
        status = call("status")
        info = call("info", "requirement.md")
        assert (status["generation"], info["content_hash"]) in (
            (old_generation, old_hash), (old_generation + 1, expected_hash)), (status, info)
        read = call("read", "requirement.md", "--start-line", "1", "--end-line", "1")
        assert read["generation"] == status["generation"], (read, status)
        assert len(read["hits"]) == 1, read
        hit = read["hits"][0]
        assert hit["citation"]["content_hash"] == info["content_hash"], (hit, info)
        assert hit["citation"]["path"] == "requirement.md", hit
        expected_line = old_line if status["generation"] == old_generation else f"{marker}: New requirement shall be atomic."
        assert hit["text"].rstrip("\r\n") == expected_line, hit
        assert digest(source_file) == expected_hash, "Publication changed the original source"
        call("index")
        assert call("info", "requirement.md")["content_hash"] == expected_hash
        assert call("search", marker)["hits"], "Recovery lost current evidence"
        cases.append({"kind": "process_kill", "delay_ms": delay * 1000,
                      "killed_before_exit": killed, "elapsed_ms": round((time.monotonic()-started)*1000),
                      "new_managed_files_observed": len(seen-before),
                      "retained_old_generation": status["generation"] == old_generation,
                      "atomic_revision_and_recovery": True})
        call("collections", "prune", "--apply")

    if full_disk:
        # Only this script's exclusive temporary tmpfs allocation is filled.
        # Mounted filesystems below it are forbidden; symlinks are not followed.
        mount = Path(full_disk).resolve(strict=True)
        if platform.system() != "Linux" or not mount.is_mount() or not base.is_relative_to(mount):
            raise ValueError("ENOSPC requires --isolated-tmpfs pointing at the campaign's own Linux tmpfs mount")
        mount_entries = Path("/proc/mounts").read_text().splitlines()
        entry = next((line.split() for line in mount_entries if line.split()[1] == str(mount)), None)
        if not entry or entry[2] != "tmpfs":
            raise ValueError("Refusing to fill anything except the explicitly selected tmpfs mount")
        old_generation = call("status")["generation"]
        old_hash = call("info", "requirement.md")["content_hash"]
        source_file.write_text("REQDISKFULL: Replacement after isolated disk pressure.\n", encoding="utf-8")
        filler = base / "owned-filler.bin"
        written = 0
        try:
            with filler.open("xb", buffering=0) as file:
                while True:
                    try:
                        written += file.write(b"\0" * 1_048_576)
                        file.flush()
                    except OSError as error:
                        if error.errno != 28:
                            raise
                        break
            failure = call("index", success=False)
        finally:
            filler.unlink(missing_ok=True)
        assert call("status")["generation"] == old_generation
        assert call("info", "requirement.md")["content_hash"] == old_hash
        call("index")
        assert call("search", "REQDISKFULL")["hits"]
        cases.append({"kind":"isolated_tmpfs_full_admission", "filler_bytes":written,
                      "scope":"full-volume rejection and recovery; not mid-publication ENOSPC",
                      "error":failure, "atomic_revision_and_recovery":True})
    return {"schema":"airk-publication-faults/1", "platform":platform.platform(),
            "binary_sha256":digest(binary), "cases":cases,
            "limitations":["Process kills do not emulate a physical power loss or device-cache loss",
                           "Observed timing does not guarantee every internal crash point was exercised",
                           "The full-volume case exhausts tmpfs before admission; it does not force a mid-publication write failure",
                           "tmpfs ENOSPC is an OS allocation failure, not a physical media/controller fault"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--attempts", type=int, default=6)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--isolated-tmpfs", type=Path)
    args = parser.parse_args()
    if not 1 <= args.attempts <= 24:
        parser.error("attempts must be 1..24")
    binary = args.binary.resolve(strict=True)
    parent = args.isolated_tmpfs.resolve(strict=True) if args.isolated_tmpfs else None
    # The explicit isolated mount must start empty; reject host/shared tmpfs paths.
    if parent and (parent == Path("/dev/shm") or parent == Path("/tmp") or any(parent.iterdir())):
        parser.error("isolated tmpfs must be a dedicated, initially empty mount")
    with tempfile.TemporaryDirectory(prefix="airk-faults-", dir=parent) as temporary:
        report = run_campaign(binary, Path(temporary), args.attempts, parent)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Publication fault campaign passed; report: {args.output}")


if __name__ == "__main__":
    main()
