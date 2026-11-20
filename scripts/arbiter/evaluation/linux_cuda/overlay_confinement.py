# SPDX-License-Identifier: Apache-2.0
"""Confine private same-UID GNU build mounts to tmpfs before running any Cargo process."""
import ctypes
import os
from pathlib import Path

from target_overlay import LIBC, kernel, require, verify_namespace


class MountAttr(ctypes.Structure):
    _fields_ = [(field, ctypes.c_uint64) for field in ("attr_set", "attr_clr", "propagation", "userns_fd")]


def mounts():
    result = []
    for line in Path("/proc/self/mountinfo").read_text().splitlines():
        fields, fs = line.split(" - ", 1)
        parts = fields.split()
        result.append((Path(parts[4]), parts[5].split(","), fs.split()[0], parts[6:]))
    return result


def require_original_test_mounts(entries, runtime):
    """The original user namespace must retain genuine system file owners during runtime tests."""
    writable = {(path, fs) for path, flags, fs, _ in entries if "rw" in flags}
    require(writable == {(runtime, "tmpfs")},
            "original-namespace Nextest requires a read-only persistent mount tree and only the private runtime tmpfs writable")


def freeze_original_mounts(root, namespace):
    verify_namespace(namespace)
    require(os.uname().machine == "x86_64" and root.parent == Path("/run/user/1000")
            and root.resolve(strict=True) == root and root.stat().st_uid == 1000
            and root.stat().st_mode & 0o777 == 0o700, "fresh private original-user tmpfs required")
    require(not any(any(flag.startswith(("shared:", "master:")) for flag in optional)
                    for _, _, _, optional in mounts()), "mount propagation must be private")
    require(os.statvfs(root).f_flag & os.ST_RDONLY == 0, "private user runtime is read-only")
    # Clone only this tmpfs child. The clone stays detached while all inherited mounts become read-only.
    name = os.fsencode(root)
    scratch = LIBC.syscall(428, -100, name, 1 | os.O_CLOEXEC)  # open_tree(OPEN_TREE_CLONE)
    if scratch < 0:
        raise OSError(ctypes.get_errno(), "clone private XDG scratch mount")
    try:
        attr = MountAttr(1, 0, 0, 0)  # MOUNT_ATTR_RDONLY
        kernel(LIBC.syscall(442, -100, b"/", 0x8000, ctypes.byref(attr), ctypes.sizeof(attr)),
               "make the private original mount tree recursively read-only")
        require(all("ro" in flags for _, flags, _, _ in mounts()), "writable original mount remains")
        kernel(LIBC.syscall(429, scratch, b"", -100, name, 4), "restore only detached private tmpfs scratch")
    finally:
        os.close(scratch)
    writable = [(path, fs) for path, flags, fs, _ in mounts() if "rw" in flags]
    require(writable == [(root, "tmpfs")], "unexpected writable filesystem after restoring private scratch")


def verify_writable_mounts(root, target, cargo):
    allowed = {(root, "tmpfs"), (root / "private", "tmpfs"), (target, "overlay"), (cargo, "overlay")}
    actual = {(path, fs) for path, flags, fs, _ in mounts() if "rw" in flags}
    require(actual == allowed, "writable mounts differ from the private tmpfs and target/cache copy-ups")
    return sorted((str(path), fs) for path, fs in actual)
