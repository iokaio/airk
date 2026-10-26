# SPDX-License-Identifier: Apache-2.0
"""Isolated process-death and optional full-volume admission campaign.

This simulates abrupt process termination, not loss of a device's volatile cache.
The full-disk case requires an explicitly isolated Linux tmpfs mount and never
fills a normal host filesystem.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import time

spec = importlib.util.spec_from_file_location("airk_fault_fixture", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def run_campaign(binary, base, attempts, full_disk):
    source, data = base / "source", base / "state"
    source.mkdir()
    data.mkdir()
    source_file = source / "requirement.md"
    source_file.write_text("REQOLDMARKER: Original requirement shall remain readable.\n", encoding="utf-8")
    with clients.Fixture(binary.parent, base) as fixture:
        assert fixture.binary == binary
        fixture.register("faults", source, data)
        fixture.rpc({"configure_watch": {"project": fixture.projects["faults"]["binding"], "enabled": False}})
        fixture.configure("faults", {"operation": "vectors", "enabled": False})
        fixture.configure("faults", {"operation": "usage", "mode": "disabled"})
        fixture.job("faults", "index")

        def tool(name, **arguments):
            if name == "airk_status":
                return fixture.published_status("faults")
            return fixture.tool("faults", name, {"response_mode": "full", **arguments})

        cases = []
        for ordinal in range(attempts):
            previous = tool("airk_read_file", path="requirement.md", start_line=1, end_line=1)
            old = previous["publication"]
            old_hash = previous["hits"][0]["citation"]["content_hash"]
            old_line = previous["hits"][0]["text"].rstrip("\r\n")
            marker = f"REQFAULT{ordinal:04}"
            source_file.write_text(f"{marker}: New requirement shall be atomic.\n" +
                                   ("A fictional requirement used only for failure qualification.\n" * 3000), encoding="utf-8")
            expected_hash = digest(source_file)
            before = {path.relative_to(data).as_posix() for path in data.rglob("*") if path.is_file()}
            # Enqueue the real durable job before terminating its actual service
            # owner. Killing a CLI that merely waits for acceptance tests no writer.
            accepted = json.loads(fixture.run(*fixture.arguments("faults"), "index").stdout)
            delay = [0.005, 0.025, 0.1, 0.3, 0.8, 1.5][ordinal % 6]
            started = time.monotonic()
            time.sleep(delay)
            seen = {path.relative_to(data).as_posix() for path in data.rglob("*") if path.is_file()}
            owned = fixture.child
            assert owned is not None and owned.poll() is None, "Owned service exited before fault injection"
            owned.kill()
            owned.wait(timeout=30)
            fixture.start()
            # Native restart drains predecessors and recovers captured work.
            # No numerical worker PID discovered from status authorizes a kill.
            read = tool("airk_read_file", path="requirement.md", start_line=1, end_line=1)
            assert len(read["hits"]) == 1, read
            hit = read["hits"][0]
            observed_hash = hit["citation"]["content_hash"]
            assert observed_hash in (old_hash, expected_hash), read
            if observed_hash == old_hash:
                assert read["publication"] == old, read
            else:
                assert read["publication"]["token"] != old["token"], read
            assert hit["citation"]["path"] == "requirement.md", hit
            expected_line = old_line if observed_hash == old_hash else f"{marker}: New requirement shall be atomic."
            assert hit["text"].rstrip("\r\n") == expected_line, hit
            assert digest(source_file) == expected_hash, "Publication changed the original source"
            recovered = fixture.wait(accepted)
            assert recovered["state"] == "succeeded"
            fixture.job("faults", "index")
            assert tool("airk_file_info", path="requirement.md")["content_hash"] == expected_hash
            assert tool("airk_search", query=marker, mode="lexical")["hits"], "Recovery lost current evidence"
            current = tool("airk_status")["publication"]
            fixture.wait(fixture.enqueue("faults", "maintain"))
            assert tool("airk_status")["publication"] == current
            cases.append({"kind": "owned_service_process_kill", "job": accepted["job"]["id"],
                          "delay_ms": delay * 1000, "elapsed_ms": round((time.monotonic() - started) * 1000),
                          "new_managed_files_observed": len(seen - before),
                          "retained_old_generation": read["publication"] == old,
                          "atomic_revision_and_recovery": True})

        if full_disk:
            # Fill only this campaign's dedicated, initially empty tmpfs mount.
            mount = Path(full_disk).resolve(strict=True)
            if platform.system() != "Linux" or not mount.is_mount() or not base.is_relative_to(mount):
                raise ValueError("ENOSPC requires the campaign's own Linux tmpfs mount")
            entries = Path("/proc/mounts").read_text().splitlines()
            entry = next((line.split() for line in entries if line.split()[1] == str(mount)), None)
            if not entry or entry[2] != "tmpfs":
                raise ValueError("Refusing to fill anything except the explicitly selected tmpfs mount")
            old = tool("airk_status")["publication"]
            old_hash = tool("airk_file_info", path="requirement.md")["content_hash"]
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
                refusal = fixture.run(*fixture.arguments("faults"), "index", check=False)
                assert refusal.returncode != 0, "Full-volume durable admission unexpectedly succeeded"
                assert not refusal.stdout.strip() or "job" not in json.loads(refusal.stdout), "Accepted work must be inspected after pressure clears"
            finally:
                filler.unlink(missing_ok=True)
            assert tool("airk_status")["publication"] == old
            assert tool("airk_file_info", path="requirement.md")["content_hash"] == old_hash
            fixture.job("faults", "index")
            assert tool("airk_search", query="REQDISKFULL", mode="lexical")["hits"]
            cases.append({"kind": "isolated_tmpfs_full_admission", "filler_bytes": written,
                          "scope": "full-volume durable admission rejection and recovery; not mid-publication ENOSPC",
                          "atomic_revision_and_recovery": True})
    return {"schema": "airk-publication-faults/2", "platform": platform.platform(),
            "binary_sha256": digest(binary), "cases": cases,
            "limitations": ["Process kills do not emulate physical power loss or device-cache loss",
                            "Timing does not prove every internal crash point was exercised",
                            "Service death exercises native predecessor draining and durable captured-work recovery",
                            "The full-volume case exhausts tmpfs before admission, not during a publication write",
                            "tmpfs ENOSPC is an OS allocation failure, not a media/controller fault"]}


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
