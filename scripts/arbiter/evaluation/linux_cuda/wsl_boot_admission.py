# SPDX-License-Identifier: Apache-2.0
"""Admit and record one start of the campaign's Ubuntu WSL instance from the verified Windows shell."""
import argparse
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import time

import gnu_volume_policy as policy

HERE = Path(__file__).resolve().parent
CAMPAIGN = HERE.parents[3] / "artifacts/v190/final-plan/exec-20261116T052942Z"
WSL = "C:\\Windows\\System32\\wsl.exe"
DISTRIBUTION, USER, UID, GID = "Ubuntu", "tyler", 1000, 1000
BASE = "/home/tyler/airk-builds/arbiter-final-linux-r1"
PATHS = ("/home/tyler", BASE, BASE + "/target")
KEEP_ALIVE_STATE = "Disabled"
# Retained Windows reservation for the GNU guest (linux-signed-r7-cpu-resume-boot-r1.json).
HOST_RESERVE = 12_884_901_888
NO_DISTRIBUTION = "There are no running distributions."
RUNNING = [WSL, "--list", "--running", "--quiet"]
FORBIDDEN = frozenset(("sudo", "su", "doas", "pkexec", "runuser", "apt", "apt-get"))
# The first guest command of the Linux runbook. Only single quotes occur inside it, so the Windows
# command line needs no escaped quote.
GUEST_SCRIPT = ("id; uname -r; df -B1 / /mnt/c /mnt/d; cat /proc/swaps; stat -c '%u:%g %a %n' " + " ".join(PATHS)
                + "; nproc; lscpu -e=CPU,CORE,SOCKET; free -b; pgrep -a 'ollama|cargo|rustc|nextest' || true")


def guest_command(argv, *, cd=None, distribution=DISTRIBUTION, user=USER):
    """Build the only permitted shape of a guest command and refuse everything else."""
    command = [WSL, "--distribution", distribution, "--user", user]
    if cd is not None:
        command += ["--cd", cd]
    command += ["--exec", *argv]
    check_command(command)
    return command


def check_command(command):
    if not command or command[0] != WSL:
        raise ValueError("only " + WSL + " may start the guest; a bare wsl starts the default distribution as root")
    if command[1:5] != ["--distribution", DISTRIBUTION, "--user", USER]:
        raise ValueError("the campaign guest is distribution Ubuntu as user tyler, stated in full")
    rest = command[5:]
    if rest[:1] == ["--cd"]:
        if len(rest) < 2 or not rest[1].startswith("/") or ".." in PurePosixPath(rest[1]).parts:
            raise ValueError("--cd needs an absolute guest directory")
        rest = rest[2:]
    if rest[:1] != ["--exec"] or len(rest) < 2 or not rest[1].startswith("/"):
        raise ValueError("expected --exec followed by an absolute guest program")
    for argument in rest[1:]:
        for word in re.split(r"[\s;|&()<>`$]+", argument):
            if PurePosixPath(word.strip("'\"")).name in FORBIDDEN:
                raise ValueError("privilege and package commands are refused: " + word)


def decode(raw):
    """WSL's own messages are UTF-16LE unless WSL_UTF8 is set. Accept both."""
    text = raw.decode("utf-16-le" if b"\0" in raw else "utf-8", errors="replace")
    return text.lstrip("\ufeff")


def parse_running(raw, exit_code, registered):
    names = [line.strip() for line in decode(raw).splitlines() if line.strip()]
    if names == [NO_DISTRIBUTION] or (not names and exit_code == 0):
        return []
    if exit_code != 0 or not names or any(name not in registered for name in names) or len(set(names)) != len(names):
        raise ValueError("unrecognized listing of running distributions")
    return names


def parse_guest(text):
    """Read the output of GUEST_SCRIPT strictly, section by section."""
    lines = text.splitlines()
    position = 0

    def take(pattern, what):
        nonlocal position
        match = re.fullmatch(pattern, lines[position]) if position < len(lines) else None
        if match is None:
            raise ValueError("unexpected guest boot output: expected " + what + " at line " + str(position + 1))
        position += 1
        return match

    def rows(pattern):
        found = []
        while position < len(lines) and re.fullmatch(pattern, lines[position]):
            found.append(take(pattern, "a row"))
        return found

    who = take(r"uid=(\d+)\(([^)]+)\) gid=(\d+)\(([^)]+)\) groups=(\S+)", "the id line")
    identity = {"uid": int(who[1]), "user": who[2], "gid": int(who[3]), "group": who[4],
                "groups": [re.sub(r"^\d+\(|\)$", "", group) for group in who[5].split(",")]}
    kernel = take(r"\S+", "the kernel release")[0]
    take(r"Filesystem\s+1B-blocks\s+Used\s+Available\s+Use%\s+Mounted on", "the df header")
    filesystems = {}
    for mount in ("/", "/mnt/c", "/mnt/d"):
        row = take(r"(.+?)\s+(\d+)\s+(\d+)\s+(\d+)\s+\d+%\s+" + re.escape(mount), "the df row of " + mount)
        filesystems[mount] = {"filesystem": row[1], "total": int(row[2]), "used": int(row[3]), "available": int(row[4])}
    take(r"Filename\s+Type\s+Size\s+Used\s+Priority", "the /proc/swaps header")
    swaps = [{"name": row[1], "type": row[2], "size_bytes": int(row[3]) * 1024, "used_bytes": int(row[4]) * 1024}
             for row in rows(r"(\S+)\s+(partition|file)\s+(\d+)\s+(\d+)\s+-?\d+")]
    paths = {}
    for path in PATHS:
        row = take(r"(\d+):(\d+) ([0-7]{3,4}) " + re.escape(path), "the stat row of " + path)
        paths[path] = {"uid": int(row[1]), "gid": int(row[2]), "mode": row[3]}
    nproc = int(take(r"\d+", "the nproc count")[0])
    take(r"CPU\s+CORE\s+SOCKET", "the lscpu header")
    cpus = [{"cpu": int(row[1]), "core": int(row[2]), "socket": int(row[3])}
            for row in rows(r"\s*(\d+)\s+(\d+)\s+(\d+)\s*")]
    take(r"\s+total\s+used\s+free\s+shared\s+buff/cache\s+available", "the free header")
    mem = take(r"Mem:\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)", "the Mem row")
    swap = take(r"Swap:\s+(\d+)\s+(\d+)\s+(\d+)", "the Swap row")
    processes = [{"pid": int(row[1]), "command": row[2]} for row in rows(r"(\d+) (.+)")]
    if position != len(lines):
        raise ValueError("unexpected guest boot output after line " + str(position))
    return {"identity": identity, "kernel": kernel, "filesystems": filesystems, "swaps": swaps, "paths": paths,
            "nproc": nproc, "cpus": cpus,
            "distinct_cores": len({(cpu["socket"], cpu["core"]) for cpu in cpus}),
            "memory": {"total": int(mem[1]), "available": int(mem[6]), "swap_total": int(swap[1]),
                       "swap_used": int(swap[2])},
            "processes": processes}


def guest_refusals(guest, minimum_cores):
    found = []
    who = guest["identity"]
    if (who["uid"], who["gid"], who["user"], who["group"]) != (UID, GID, USER, USER):
        found.append("guest identity is not tyler with UID and GID 1000")
    if not guest["kernel"].endswith("-microsoft-standard-WSL2"):
        found.append("guest kernel is not the WSL2 kernel")
    for path, fact in guest["paths"].items():
        if (fact["uid"], fact["gid"]) != (UID, GID) or int(fact["mode"], 8) & 0o022:
            found.append("guest path owner or write policy differs: " + path)
    if sum(row["size_bytes"] for row in guest["swaps"]) > policy.SWAP_RESERVE:
        found.append("guest swap exceeds the reserve kept for it on C:")
    cpus = guest["cpus"]
    if guest["nproc"] != len(cpus) or len({cpu["cpu"] for cpu in cpus}) != len(cpus) or not cpus:
        found.append("guest processor topology is inconsistent")
    if guest["distinct_cores"] < minimum_cores:
        found.append("guest has fewer distinct cores than required")
    if cargo_owners(guest):
        found.append("another Cargo owner is active in the guest")
    return found


def cargo_owners(guest):
    """Everything the pgrep pattern matched except Ollama, which is reported and not refused."""
    return [row for row in guest["processes"]
            if not PurePosixPath(row["command"].split()[0]).name.startswith("ollama")]


def host_refusals(host, expected_sid, required_memory):
    found = []
    who = host["identity"]
    if who["sid"] != expected_sid or who["owner_sid"] != expected_sid or who["interactive_owner_sids"] != [expected_sid]:
        found.append("Windows identity is not the expected human")
    if who["elevated"] is not False:
        found.append("Windows shell is elevated")
    ubuntu = [row for row in host["distributions"] if row["name"] == DISTRIBUTION]
    if (len(ubuntu) != 1 or ubuntu[0]["default_uid"] != UID or ubuntu[0]["version"] != 2
            or not str(ubuntu[0]["vhd_path"]).startswith("C:\\") or type(ubuntu[0]["vhd_length"]) is not int):
        found.append("registered Ubuntu distribution, default UID 1000 or C: virtual disk differs")
    if host["keep_alive_task"]["state"] != KEEP_ALIVE_STATE:
        found.append("keep-alive task is not Disabled")
    if any(row["name"].lower() == "com.docker.backend" for row in host["processes"]):
        found.append("Docker Desktop backend is running")
    swap = host["swap_files"]
    if len(swap) > 1 or any(not row["path"].startswith("C:\\") or row["length"] > policy.SWAP_RESERVE for row in swap):
        found.append("swap file count, volume or length differs from the C: reserve")
    memory = host["memory"]
    if min(memory["physical_available_bytes"], memory["commit_available_bytes"]) < required_memory:
        found.append("Windows physical or commit memory is below the required reserve")
    return found


def volumes(host):
    ubuntu = [row for row in host["distributions"] if row["name"] == DISTRIBUTION]
    return {"utc": host["utc"], "c_free": host["c_free"], "d_free": host["d_free"],
            "vhd": ubuntu[0]["vhd_length"] if len(ubuntu) == 1 else None,
            "swap": sum(row["length"] for row in host["swap_files"]), "swap_files": host["swap_files"],
            "memory": host["memory"], "keep_alive_task": host["keep_alive_task"], "processes": host["processes"]}


def receipt_path(path):
    path = Path(path)
    if path.parent != CAMPAIGN or re.fullmatch(r"linux-[a-z0-9-]+-boot-r[1-9][0-9]*\.json", path.name) is None:
        raise ValueError("receipt must be linux-<name>-boot-r<N>.json directly inside the campaign root")
    if os.path.lexists(path):
        raise ValueError("fresh boot receipt required")
    return path


def host_command(pwsh, options, *, created=False):
    return [pwsh, "-NoProfile", "-NonInteractive", "-File", str(HERE / "wsl_boot_host.ps1"), "-ExpectedSid",
            options.expected_sid, "-VolumeLog", str(options.volume_log), "-ReceiptPath", str(options.receipt),
            *(["-Created"] if created else [])]


def permitted(command, pwsh):
    """The only three command shapes this wrapper starts: the host observer, the listing and the guest."""
    observer = [pwsh, "-NoProfile", "-NonInteractive", "-File", str(HERE / "wsl_boot_host.ps1")]
    if command != RUNNING and command[:5] != observer:
        check_command(command)


def launch(command, timeout):
    """Start one command with the caller's own token and no shell."""
    done = subprocess.run(command, capture_output=True, timeout=timeout, env=os.environ | {"WSL_UTF8": "1"})
    return done.returncode, done.stdout, done.stderr


def run(options, *, launch=launch, clock=time.time, pwsh=None):
    binding = policy.selected(options)
    if binding is None or min(options.reserve_bytes, options.other_host_bytes) < 0 or options.min_distinct_cores < 1:
        raise ValueError("the volume policy binding and non-negative reservations are required")
    target = receipt_path(options.receipt)
    pwsh = pwsh or shutil.which("pwsh")
    if pwsh is None:
        raise ValueError("pwsh is required for the Windows identity guard")
    guest_argv = guest_command(["/bin/sh", "-c", GUEST_SCRIPT])
    if options.plan:
        return {"plan": [host_command(pwsh, options), RUNNING, guest_argv, host_command(pwsh, options), RUNNING,
                         host_command(pwsh, options, created=True)]}
    required_memory = HOST_RESERVE + options.other_host_bytes
    report = {"schema": "airk.private.wsl-boot-admission/1", "sid": options.expected_sid,
              "distribution": DISTRIBUTION, "user": USER, "command": guest_argv, "guest_workload_started": False,
              "swap_reservation": policy.SWAP_RESERVE, "guest_vhd_reservation": options.reserve_bytes,
              "windows_host_reservation": required_memory, "volume_policy": binding.receipt(),
              "guest_started": False}

    def start(command, timeout):
        permitted(command, pwsh)
        return launch(command, timeout)

    def host(created=False):
        code, out, error = start(host_command(pwsh, options, created=created), 120)
        if code != 0:
            raise RuntimeError("Windows identity or host observation failed: " + decode(error).strip()[-400:])
        return json.loads(out)

    def ubuntu(observation):
        return next(row for row in observation["distributions"] if row["name"] == DISTRIBUTION)

    def monitor(observation):
        log = Path(options.volume_log)
        # The host observer has already checked that the human owns this log.
        _, last = policy.read_monitor(log, CAMPAIGN, log.lstat().st_uid, binding.first_row_sha256, clock(),
                                      binding.not_after)
        return last, (min(observation["c_free"], last["backing_free"]), min(observation["d_free"], last["windows_free"]),
                      max(ubuntu(observation)["vhd_length"], last["vhd_bytes"]))

    def running(registered):
        code, out, _ = start(RUNNING, 60)
        return parse_running(out, code, registered)

    def before_boot(before):
        found = host_refusals(before, options.expected_sid, required_memory)
        if found:
            return found
        report.update(registered_uid=ubuntu(before)["default_uid"], vhd_path=ubuntu(before)["vhd_path"])
        report["monitor_before"], seen = monitor(before)
        found = policy.host_refusals(*seen, options.reserve_bytes)
        if found:
            return found
        report["before"]["running_distributions"] = running(registered)
        return [] if set(report["before"]["running_distributions"]) <= {DISTRIBUTION} else [
            "another distribution is running"]

    def boot():
        report["guest_started"] = True
        code, out, error = start(guest_argv, 180)
        report.update(exit_code=code, guest=decode(out).splitlines(), guest_stderr=decode(error).splitlines())
        after = host()
        swap = after["swap_files"]
        report.update(after=volumes(after), post_boot_physical=after["memory"]["physical_available_bytes"],
                      post_boot_virtual=after["memory"]["commit_available_bytes"],
                      swap_path=swap[0]["path"] if len(swap) == 1 else None)
        found = host_refusals(after, options.expected_sid, 0)
        if code != 0:
            return found + ["guest command exited " + str(code)]
        guest = parse_guest(decode(out))
        report.update(guest_checks=guest, uid=guest["identity"]["uid"],
                      guest_ollama=[row for row in guest["processes"] if row not in cargo_owners(guest)])
        found += guest_refusals(guest, options.min_distinct_cores)
        report["after"]["running_distributions"] = running(registered)
        if report["after"]["running_distributions"] != [DISTRIBUTION]:
            found.append("only Ubuntu may be running after the boot")
        report["monitor"], seen = monitor(after)
        sample = {"guest_free": guest["filesystems"]["/"]["available"], "backing_free": seen[0],
                  "windows_free": seen[1], "vhd_length": seen[2]}
        report["headroom"] = policy.headroom(sample, report["monitor"])
        return found + policy.refusals(sample, report["monitor"], sample, options.reserve_bytes, admitting=True)

    def stage(action, *arguments):
        """A refusal ends the admission. The receipt is still written so the reason is retained."""
        try:
            return action(*arguments)
        except (ValueError, KeyError, OSError, subprocess.TimeoutExpired) as error:
            return [type(error).__name__ + ": " + str(error)]

    # A wrong identity raises here, before this wrapper has created anything.
    first = host()
    who = first["identity"]
    if (who["sid"], who["owner_sid"]) != (options.expected_sid, options.expected_sid):
        raise RuntimeError("Windows identity is not the expected human")
    registered = [row["name"] for row in first["distributions"]]
    report.update(utc=first["utc"], before=volumes(first), wslconfig=first["wslconfig"],
                  registered_distributions=first["distributions"])
    report["refusals"] = stage(before_boot, first)
    if not report["refusals"]:
        report["refusals"] = stage(boot)
    report["admitted"] = report["guest_started"] and not report["refusals"]
    with target.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(report, stream, indent=2)
        stream.write("\n")
    host(created=True)
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--expected-sid", required=True)
    parser.add_argument("--receipt", type=Path, required=True)
    parser.add_argument("--reserve-bytes", type=int, default=0, help="storage the first job after boot will reserve")
    parser.add_argument("--other-host-bytes", type=int, default=0, help="memory other running jobs have reserved")
    parser.add_argument("--min-distinct-cores", type=int, default=8)
    parser.add_argument("--plan", action="store_true", help="print the six commands and start nothing")
    policy.arguments(parser)
    outcome = run(parser.parse_args())
    print(json.dumps(outcome, sort_keys=True))
    sys.exit(0 if outcome.get("admitted") or "plan" in outcome else 1)
