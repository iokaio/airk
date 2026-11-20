# SPDX-License-Identifier: Apache-2.0
"""Checkpoint the campaign's exact two-worker r4 provider and seal its settled build tree."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import stat
import time

from configure_ort import BASE, CAMPAIGN, sha
from unpack import guarded
from verified_inputs import identity

ROOT = BASE / "linux-cuda-ort-r4"
BUILD = ROOT / "b/Release"
CONFIGURE = "e579de0a21f10459506187c92cd3eb3c93be5b47d10e03619a07f02419f2edf8"
BOOT = Path("/proc/sys/kernel/random/boot_id")


def owner():
    identity()
    receipt = CAMPAIGN / "linux-cuda-ort-configure-r4.json"
    if sha(receipt) != CONFIGURE:
        raise ValueError("unreviewed ORT configure receipt")
    cfg = json.loads(receipt.read_bytes())
    if cfg["root"] != str(ROOT):
        raise ValueError("unexpected configured root")
    exe = Path(cfg["tools"]["ninja"])
    return exe, [str(exe), "-C", str(BUILD), "-j2", "libonnxruntime_providers_cuda.so"]


def processes(argv):
    rows = []
    for proc in Path("/proc").iterdir():
        if not proc.name.isdigit():
            continue
        try:
            if proc.stat().st_uid != os.getuid():
                continue
            raw = (proc / "cmdline").read_bytes().split(b"\0")[:-1]
        except (FileNotFoundError, ProcessLookupError):
            continue
        if raw == [os.fsencode(s) for s in argv]:
            rows.append(proc)
    return rows


def request():
    exe, argv = owner()
    if processes([n.replace("linux-cuda-ort-r4","linux-cuda-ort-r3") for n in argv]):
        raise ValueError("r3 compiler is still running")
    candidates = processes(argv)
    if len(candidates) != 1:
        raise ValueError("expected exactly one owned r4 provider build")
    proc = candidates[0]
    pid = int(proc.name)
    fd = os.pidfd_open(pid)
    try:
        if (os.getpgid(pid) != pid or os.getsid(pid) != pid or not (proc/"exe").samefile(exe)
            or processes(argv) != [proc]):
            raise ValueError("Ninja PID, group, session, executable, or arguments changed")
        ticks = int((proc/"stat").read_text().rsplit(")",1)[1].split()[19])
        expiry = ticks / os.sysconf("SC_CLK_TCK") + 10800
        if time.clock_gettime(time.CLOCK_BOOTTIME) >= expiry:
            raise ValueError("original three-hour build window expired")
        signal.pidfd_send_signal(fd, signal.SIGINT)
        return {"pid":pid, "start_ticks":ticks, "boot_id":BOOT.read_text().strip(),
                "expires_boottime":expiry, "configure":CONFIGURE, "argv":argv}
    finally:
        os.close(fd)


def tree():
    uid = identity()
    guarded(BUILD,uid,stat.S_ISDIR)
    rows = {}
    for path in BUILD.rglob("*"):
        name = path.relative_to(BUILD).as_posix()
        if path.is_symlink():
            guarded(path,uid,stat.S_ISLNK)
            if not path.resolve(strict=True).is_relative_to(ROOT):
                raise ValueError("build symlink escapes root: "+name)
            rows[name] = {"link":os.readlink(path)}
        elif path.is_dir():
            guarded(path,uid,stat.S_ISDIR)
        else:
            guarded(path,uid,stat.S_ISREG)
            rows[name] = {"sha256":sha(path),"bytes":path.stat().st_size}
    raw = json.dumps(rows,sort_keys=True,separators=(",",":")).encode()
    return {"count":len(rows),"sha256":hashlib.sha256(raw).hexdigest(),"files":rows}


def seal(args):
    _, argv = owner()
    if sha(args.signal) != args.signal_sha256 or sha(args.build_receipt) != args.build_sha256:
        raise ValueError("checkpoint input changed")
    stopped = json.loads(args.signal.read_bytes())
    stage = json.loads(args.build_receipt.read_bytes())
    result = stage["command"]
    if (stopped["argv"] != argv or stopped["boot_id"] != BOOT.read_text().strip()
        or result["owned_leader_pid"] != stopped["pid"] or stage["built"] or not stage["rules_unchanged"]
        or result["exit_code"] == 0 or result["log_oversized"] or result["timed_out"] or not result["owned_descendants_settled"] or result["cleanup_errors"]
        or stage["authenticated_inputs_before"] != stage["authenticated_inputs_after"]
        or stage["configure"] != CONFIGURE or processes(argv)):
        raise ValueError("previous build did not settle as a controlled checkpoint")
    return {"signal_receipt":args.signal_sha256, "build_receipt":args.build_sha256,
            "boot_id":stopped["boot_id"], "expires_boottime":stopped["expires_boottime"],
            "configure":CONFIGURE, "tree":tree()}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--phase", choices=("request","seal"), required=True)
    p.add_argument("--signal", type=Path)
    p.add_argument("--signal-sha256")
    p.add_argument("--build-receipt", type=Path)
    p.add_argument("--build-sha256")
    a = p.parse_args()
    print(json.dumps(request() if a.phase == "request" else seal(a), sort_keys=True))
