# SPDX-License-Identifier: Apache-2.0
"""Isolated process-death and optional full-volume admission campaign.

This simulates abrupt process termination, not loss of a device's volatile cache.
The full-disk case requires an explicitly isolated Linux tmpfs mount and never
fills a normal host filesystem.
"""
import argparse
from contextlib import contextmanager
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import select
import subprocess
import tempfile
import time

spec = importlib.util.spec_from_file_location("airk_fault_fixture", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)
process_spec = importlib.util.spec_from_file_location("fault_process_identity", Path(__file__).with_name("owned-runtime-processes.py"))
processes = importlib.util.module_from_spec(process_spec)
process_spec.loader.exec_module(processes)


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


@contextmanager
def owned_fault_fixture(parent=None):
    if parent is None:
        with clients.owned_client_fixture() as owner:
            yield owner
        return
    # An explicitly selected isolated tmpfs remains the only full-volume target.
    temporary = tempfile.TemporaryDirectory(prefix="airk-faults-", dir=parent)
    try:
        yield temporary.name
    except BaseException:
        temporary._finalizer.detach()
        raise
    else:
        temporary.cleanup()


def drain_initial_owner(fixture):
    """Observe exit of the authenticated initial owner without sending any signal."""
    health = fixture.managed_health
    pid = health["process_id"]
    assert type(pid) is int and pid > 0
    held = queue = None
    try:
        if platform.system() == "Darwin":
            queue = select.kqueue()
            queue.control([select.kevent(pid, filter=select.KQ_FILTER_PROC,
                flags=select.KQ_EV_ADD | select.KQ_EV_ONESHOT, fflags=select.KQ_NOTE_EXIT)], 0, 0)
        else:
            backend = processes._backend()
            held = backend.open(pid)
            observed = held.observation()
            assert not held.gone() and observed["user"] == backend.user and not observed.get("image_deleted")
            assert Path(observed["path"]).samefile(fixture.binary)
        fixture.assert_alive()  # Revalidate native instance after binding the exit observer.
        response = json.loads(fixture.run("service", "shutdown", "--runtime-dir", fixture.control).stdout)
        assert response["status"] == "stopping", response
        deadline = time.monotonic() + 45
        while True:
            if held is not None:
                exited = held.gone()
            else:
                events = queue.control([], 1, 0)
                assert all(event.ident == pid and event.filter == select.KQ_FILTER_PROC
                           and event.fflags & select.KQ_NOTE_EXIT and not event.flags & select.KQ_EV_ERROR for event in events)
                exited = bool(events)
            if exited:
                break
            assert time.monotonic() < deadline, "Initial owned service did not exit after transient drain"
            time.sleep(0.025)
        assert fixture.run("service", "health", "--runtime-dir", fixture.control, check=False).returncode != 0
        return {"instance": health["instance"], "process_id": pid, "observed_exit": True, "signals_sent": 0}
    finally:
        if held is not None:
            held.close()
        if queue is not None:
            queue.close()


class FaultFixture(clients.Fixture):
    """Canonical installed foreground entry, supervised by this retained Popen."""
    def start(self):
        if not self.single_image and self.catalog_authority:
            raise ValueError("Historical catalog fault injection has no supported retained-child startup protocol")
        if not self.single_image:
            return super().start()
        if not self.managed_started:
            super().start()
            drained = drain_initial_owner(self)
            (self.owner / "initial-owner-drain.json").write_text(json.dumps(drained, indent=2) + "\n")
        previous = self.managed_health
        assert self.child is None or self.child.poll() is not None, "Previous foreground child is still alive"
        # The service observation stays on airk (U93 interim role on a split pair).
        self.child = subprocess.Popen(self.service_command("runtime", "foreground", "--install-dir", self.install),
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=self.log, cwd=self.runtime, env=self.environment,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            assert self.child.poll() is None, "Owned foreground service exited during startup"
            result = self.run("service", "health", "--runtime-dir", self.control, check=False)
            if result.returncode == 0:
                health = json.loads(result.stdout)["observation"]
                assert health["process_id"] == self.child.pid and health["instance"] != previous["instance"]
                assert health["runtime_sha256"] == previous["runtime_sha256"] and health["executable_sha256"] == digest(self.binary)
                self.managed_health = health
                self.assert_alive()
                return
            time.sleep(0.05)
        raise AssertionError("Owned foreground service did not become healthy")

    def close(self):
        if not self.single_image:
            return super().close()
        super().close()  # Exact native lifecycle Stop; uncertain outcomes remain failures.
        if self.child is not None:
            self.child.wait(timeout=45)
            assert self.child.returncode == 0, "Owned foreground service did not exit cleanly"

    def inject_death(self):
        owned = self.child
        assert owned is not None and owned.poll() is None, "Owned service exited before fault injection"
        self.assert_alive()
        if self.single_image:
            assert self.managed_health["process_id"] == owned.pid
            assert self.binary.resolve(strict=True).is_relative_to(self.install.resolve(strict=True))
        before = self.managed_health if self.single_image else {"process_id": owned.pid}
        # Popen retains the Windows process handle or unreaped POSIX child identity.
        owned.kill()
        owned.wait(timeout=30)
        proof = {"owner": before, "retained_child_pid": owned.pid, "exit_code": owned.returncode,
                 "scope": "owned_foreground_supervisor" if self.single_image else "owned_legacy_service", "observed_exit": True}
        with (self.owner / "fault-process-receipts.jsonl").open("a", encoding="utf-8") as stream:
            stream.write(json.dumps(proof) + "\n")
        return proof


def run_campaign(binary, base, attempts, full_disk, cli=None):
    source, data = base / "source", base / "index-data"
    source.mkdir()
    data.mkdir()
    source_file = source / "requirement.md"
    source_file.write_text("REQOLDMARKER: Original requirement shall remain readable.\n", encoding="utf-8")
    with FaultFixture(binary.parent, base, cli=cli) as fixture:
        assert digest(fixture.binary) == digest(binary)
        fixture.assert_alive()
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
            accepted = fixture.enqueue("faults", "index")
            delay = [0.005, 0.025, 0.1, 0.3, 0.8, 1.5][ordinal % 6]
            started = time.monotonic()
            time.sleep(delay)
            seen = {path.relative_to(data).as_posix() for path in data.rglob("*") if path.is_file()}
            fault_receipt = fixture.inject_death()
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
                          "process_fault": fault_receipt,
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
                registration = fixture.projects["faults"]
                refusal = fixture.run("--service-runtime-dir", fixture.control, "--service-project", registration["binding"],
                    "--root", registration["root"], "--data-dir", registration["data_root"], "--collection", registration["collection"], "index", check=False)
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
                            "Modern service death exercises the canonical foreground entry under this fixture's supervisor, not on-demand restart policy",
                            "Service death exercises native predecessor draining and durable captured-work recovery",
                            "The full-volume case exhausts tmpfs before admission, not during a publication write",
                            "tmpfs ENOSPC is an OS allocation failure, not a media/controller fault"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True, help="The runtime's airk service image")
    parser.add_argument("--airkctl", type=Path, help="A split pair's own airkctl (verified; derived from the pair when omitted)")
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
    with owned_fault_fixture(parent) as temporary:
        report = run_campaign(binary, Path(temporary), args.attempts, parent, args.airkctl)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Publication fault campaign passed; report: {args.output}")


if __name__ == "__main__":
    main()
