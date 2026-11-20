# SPDX-License-Identifier: Apache-2.0
"""Prove copy-on-write isolation for the GNU Cargo target using the existing user's private tmpfs."""
import argparse
import ctypes
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import time

from configure_ort import CAMPAIGN, audit, sha
from shipping_build import capacity, space
from unpack import guarded
from verified_inputs import identity

SCRIPT = Path(__file__).resolve()
LIBC = ctypes.CDLL(None,use_errno=True)
USER_MAP = [1000,1000,1]


def require(ok, message):
    if not ok:
        raise ValueError(message)


def kernel(ok, name):
    if ok != 0:
        raise OSError(ctypes.get_errno(),name)


def xdg(uid):
    root = Path(os.environ["XDG_RUNTIME_DIR"])
    require(root == Path("/run/user/1000") and root.resolve(strict=True) == root,"original user runtime required")
    until = time.monotonic()+20
    while stat.S_IMODE(root.stat().st_mode) != 0o700 and time.monotonic() < until:
        time.sleep(0.1)
    guarded(root,uid,stat.S_ISDIR)
    fs = subprocess.check_output(["/usr/bin/findmnt","-rno","TARGET,FSTYPE,VFS-OPTIONS","-T",root],text=True).split()
    require(stat.S_IMODE(root.stat().st_mode) == 0o700 and fs[:2] == [str(root),"tmpfs"]
            and not {"noexec","ro"}.intersection(fs[2].split(",")),"private executable original-user tmpfs required")
    return root


def verify_namespace(user_namespace):
    uid = identity()
    for mapping in ("/proc/self/uid_map","/proc/self/gid_map"):
        require(list(map(int,Path(mapping).read_text().split())) == USER_MAP,"different or root mapped identity")
    require(os.readlink("/proc/self/ns/user") != user_namespace,"private parser requires new user namespace")


def mount_overlay(lower, upper, work, mount):
    uid = identity()
    for path in (lower, upper, work, mount):
        require(path.resolve(strict=True) == path and path.stat().st_uid == uid and not any(c in str(path) for c in ",:"),
                "noncanonical overlay path")
    options = f"lowerdir={lower},upperdir={upper},workdir={work},userxattr"
    kernel(LIBC.mount(b"overlay",os.fsencode(mount),b"overlay",0,os.fsencode(options)),"private overlay mount")


def drop_capabilities():
    uid = identity()
    # Drop all namespace-only capabilities and prevent privilege acquisition on exec.
    kernel(LIBC.prctl(38,1,0,0,0),"no new privileges")
    header = (ctypes.c_uint32 * 2)(0x20080522,0)
    data = (ctypes.c_uint32 * 6)()
    kernel(LIBC.capset(ctypes.byref(header),ctypes.byref(data)),"drop namespace capabilities")
    status = {a:b.strip() for a,b in (row.split(":",1) for row in Path("/proc/self/status").read_text().splitlines())}
    require(all(int(status[k],16) == 0 for k in ("CapEff","CapAmb","CapPrm","CapInh"))
            and status["NoNewPrivs"] == "1" and list(map(int,status["Uid"].split())) == [uid]*4,
            "ordinary original user privileges required before use")
    return status["Uid"]


def child(root, namespace):
    lower,upper,work,mount = (root/n for n in ("lower","upper","work","merged"))
    verify_namespace(namespace)
    require(root.parent == xdg(identity()) and root.name.startswith("airk-gnu-overlay-control-")
            and root.resolve(strict=True) == root and stat.S_IMODE(root.stat().st_mode) == 0o700
            and root.stat().st_uid == identity(), "private same-user overlay control root required")
    mount_overlay(lower,upper,work,mount)
    uid_fields = drop_capabilities()
    require((mount/"original").read_bytes() == b"initial","original lower copy missing")
    (mount/"original").write_bytes(b"overlaid")
    (mount/"new").write_bytes(b"new")
    require((lower/"original").read_bytes() == b"initial" and not (lower/"new").exists(),
            "underlying files changed during copy-up")
    print(json.dumps({"uids":uid_fields,"namespace":os.readlink("/proc/self/ns/user"),
                      "mount_namespace":os.readlink("/proc/self/ns/mnt"),"caps_dropped":True}))


def control():
    uid = identity()
    os.umask(0o077)
    root = xdg(uid)
    baseline = json.loads((CAMPAIGN/"linux-cuda-r4-provider-r3-space-watch.jsonl").read_text().splitlines()[0])
    before = space()
    require(capacity(baseline,before),"original disk limit already exceeded")
    unshare = Path("/usr/bin/unshare")
    info = unshare.stat()
    require(info.st_uid == 0 and stat.S_ISREG(info.st_mode) and not info.st_mode & 0o6022,"unexpected unshare executable")
    namespace = os.readlink("/proc/self/ns/user")
    with tempfile.TemporaryDirectory(prefix="airk-gnu-overlay-control-",dir=root) as scratch:
        target = Path(scratch)
        for name in ("lower","upper","work","merged"):
            (target/name).mkdir(0o700)
        original = target/"lower/original"
        original.write_bytes(b"initial")
        inode = original.stat().st_ino
        command = [str(unshare),"--user","--map-current-user","--mount","--propagation","private","--keep-caps",
                   sys.executable,"-B","-S",str(SCRIPT),"--child",str(target),"--parent-namespace",namespace]
        proc = subprocess.run(command,check=False,capture_output=True,text=True,timeout=30)
        audit(target,uid)
        if proc.returncode:
            raise ValueError("unprivileged overlay probe failed: " + proc.stderr)
        require(original.read_bytes() == b"initial" and original.stat().st_ino == inode
                and not (target/"lower/new").exists() and not list((target/"merged").iterdir())
                and (target/"upper/new").read_bytes() == b"new" and (target/"upper/original").read_bytes() == b"overlaid",
                "private mount did not settle or copy-up changed lower")
        report = json.loads(proc.stdout)
    after = space()
    require(capacity(baseline,after) and after["vhd_length"] == before["vhd_length"]
            and not target.exists(),"tmpfs cleanup failed or original backing grew")
    return {"uid":uid,"source":sha(SCRIPT),"unshare":sha(unshare),"outer_user_namespace":namespace,
            "inner":report,"lower_unchanged":True,"upper_owner_verified":True,"private_mount_settled":True,
            "temporary_removed":True,"before":before,"after":after}


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--child",type=Path)
    p.add_argument("--parent-namespace")
    a=p.parse_args()
    if a.child:
        child(a.child,a.parent_namespace)
    else:
        print(json.dumps(control(),sort_keys=True))
