# SPDX-License-Identifier: Apache-2.0
"""Keep GNU compilation writes on a size-limited personal tmpfs over the original warm target."""
import argparse
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import time

from configure_ort import BASE, CAMPAIGN, sha
from shipping_build import capacity, space
from overlay_confinement import freeze_original_mounts, verify_writable_mounts
from target_overlay import (LIBC, drop_capabilities, kernel, mount_overlay, require, verify_namespace, xdg)
from verified_inputs import identity

PRIVATE_SIZE = 1536 << 20
SCRIPT = Path(__file__).resolve()


def prepare(root, namespace):
    uid = identity()
    verify_namespace(namespace)
    require(root.parent == Path("/run/user/1000") and root.name.startswith("airk-r7-")
            and root.resolve(strict=True) == root and root.stat().st_uid == uid
            and stat.S_IMODE(root.stat().st_mode) == 0o700,"private runtime root required")
    freeze_original_mounts(root, namespace)
    private = root/"private"
    private.mkdir(0o700)
    kernel(LIBC.mount(b"tmpfs",os.fsencode(private),b"tmpfs",2|4,
                      f"size={PRIVATE_SIZE},mode=0700,uid={uid},gid={uid}".encode()),"bounded private upper tmpfs")
    require(shutil.disk_usage(private).total == PRIVATE_SIZE,"private tmpfs size differs")
    for name in ("target","cargo"):
        for suffix in ("upper","work"):
            (private/(name+"-"+suffix)).mkdir(0o700)
    for name in ("target","cargo"):
        mount_overlay(BASE/name,private/(name+"-upper"),private/(name+"-work"),BASE/name)
    writable = verify_writable_mounts(root, BASE/"target", BASE/"cargo")
    uid_fields = drop_capabilities()
    for name in ("target","cargo"):
        observed = subprocess.check_output(["/usr/bin/findmnt","-T",str(BASE/name),"-rno","TARGET,FSTYPE"],text=True).split()
        require(observed == [str(BASE/name),"overlay"],"warm target or cache missing private overlay")
    return {"uids":uid_fields,"writable_mounts":writable,"private_tmpfs_bytes":PRIVATE_SIZE,"base_readonly":True,
            "c_drive_readonly":True,"real_target":str(BASE/"target"),"real_cache":str(BASE/"cargo"),
            "caps_dropped":True}


def probe(root, namespace):
    row = prepare(root,namespace)
    expected = b"same user scratch"
    for name in ("target","cargo"):
        relative = "airk-private-overlay-probe"
        original = BASE/name/relative
        require(not original.exists(),"unexpected existing sentinel")
        with original.open("xb") as file:
            file.write(expected)
        upper = root/"private"/(name+"-upper")/relative
        require(upper.read_bytes() == expected and upper.stat().st_uid == identity(),"copy-up outside private tmpfs")
    print(json.dumps(row))


def original_budget():
    baseline=json.loads((CAMPAIGN/"linux-cuda-r4-provider-r3-space-watch.jsonl").read_text().splitlines()[0])
    result=space()
    require(capacity(baseline,result),"original global guest/backing volume limit reached")
    return result


def isolated(root, script, args):
    return ["/usr/bin/unshare","--user","--map-current-user","--mount","--propagation","private","--keep-caps",
            sys.executable,"-B","-S",str(script),"--child",str(root),"--namespace",os.readlink("/proc/self/ns/user"),
            *args]


def control():
    uid = identity()
    os.umask(0o077)
    session = xdg(uid)
    before = original_budget()
    outer = os.readlink("/proc/self/ns/mnt")
    for name in ("target","cargo"):
        require(not (BASE/name/"airk-private-overlay-probe").exists(),"unexpected persistent probe")
    with tempfile.TemporaryDirectory(prefix="airk-r7-",dir=session) as raw:
        root=Path(raw)
        cmd=isolated(root,SCRIPT,[])
        proc=subprocess.run(cmd,check=False,text=True,capture_output=True,timeout=30)
        require(proc.returncode == 0,"bounded private target probe failed: "+proc.stderr)
        inner=json.loads(proc.stdout)
        require(os.readlink("/proc/self/ns/mnt") == outer and not list((root/"private").iterdir())
                and all(not (BASE/name/"airk-private-overlay-probe").exists() for name in ("target","cargo")),
                "underlying guest target/cache or mount was changed")
    after=original_budget()
    require(after["vhd_length"] == before["vhd_length"] and not root.exists(),"backing grew or tmpfs root survived")
    return {"uid":uid,"source_sha256":sha(SCRIPT),"mount_control":inner,"temp_removed":True,
            "original_target_unchanged":True,"original_cache_unchanged":True,"before":before,"after":after}


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--child",type=Path)
    p.add_argument("--namespace")
    a=p.parse_args()
    if a.child:
        probe(a.child,a.namespace)
    else:
        print(json.dumps(control(),sort_keys=True))
