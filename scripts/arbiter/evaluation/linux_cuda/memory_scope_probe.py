# SPDX-License-Identifier: Apache-2.0
"""Probe an ephemeral human-owned systemd scope for a real delegated memory limit."""
import argparse
import json
import os
from pathlib import Path
import pwd
import stat
import subprocess
import sys


def checked_user():
    if (os.getuid(), os.geteuid(), os.getgid(), pwd.getpwuid(os.getuid()).pw_name) != (1000,1000,1000,"tyler"):
        raise ValueError("expected registered Linux human")


def observed():
    checked_user()
    rows = Path("/proc/self/cgroup").read_text().splitlines()
    limits = {}
    for row in rows:
        _, controllers, where = row.split(":", 2)
        if not where.startswith("/") or ".." in Path(where).parts:
            raise ValueError("unexpected cgroup root")
        if "memory" in controllers.split(","):
            directory = Path("/sys/fs/cgroup/memory") / where.lstrip("/")
            value = directory / "memory.limit_in_bytes"
        elif not controllers:
            directory = Path("/sys/fs/cgroup/unified") / where.lstrip("/")
            value = directory / "memory.max"
        else:
            continue
        limits[str(value)] = value.read_text().strip() if value.is_file() else None
    return {"cgroups":rows,"observed_kernel_limits":limits,
            "memory_limit_enforced":str(64<<20) in limits.values()}


def probe():
    checked_user()
    for directory in (Path("/run/user/1000"),Path("/run/user/1000/systemd")):
        info = directory.lstat()
        if info.st_uid != os.getuid() or not stat.S_ISDIR(info.st_mode) or info.st_mode & 0o022:
            raise ValueError("unsafe user manager output parent")
    unit = "airk-cuda-memory-probe-" + str(os.getpid()) + ".scope"
    env = {"PATH":"/usr/bin:/bin","XDG_RUNTIME_DIR":"/run/user/1000","LC_ALL":"C","HOME":"/home/tyler"}
    cmd = ["/usr/bin/systemd-run","--user","--scope","--collect","--quiet","--unit",unit,
           "--property=MemoryMax=64M","--",sys.executable,"-I","-B","-S",str(Path(__file__).resolve()),"--inside"]
    result = subprocess.run(cmd, text=True, capture_output=True, env=env, timeout=15)
    observation = json.loads(result.stdout) if result.returncode == 0 else None
    state = subprocess.run(["/usr/bin/systemctl","--user","show",unit,"-p","ActiveState","-p","LoadState"],
                           text=True, capture_output=True, env=env, timeout=5)
    return {"scope":unit,"argv":cmd,"exit":result.returncode,"stderr":result.stderr,
            "observed":observation,"settled_state":state.stdout.strip()}


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--inside",action="store_true")
    print(json.dumps(observed() if p.parse_args().inside else probe(),sort_keys=True))
