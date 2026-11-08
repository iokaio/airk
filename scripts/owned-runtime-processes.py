# SPDX-License-Identifier: Apache-2.0
"""Read-only, test-only predecessor process evidence; never signal or stop a PID."""
import ctypes
import errno
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import select
import stat
import sys
import time

MAX_SCANNED = 16384
MAX_MATCHES = 64
HEX = re.compile(r"[a-f0-9]{64}\Z")


def _key(path):
    return os.path.normcase(os.path.abspath(path)).removeprefix("\\\\?\\")


def _safe(path, directory=False):
    path = Path(path)
    if not path.is_absolute() or _key(path.resolve(strict=True)) != _key(path):
        raise RuntimeError("Census path must be absolute and canonical")
    for entry in [path, *path.parents]:
        metadata = entry.lstat()
        if stat.S_ISLNK(metadata.st_mode) or getattr(metadata, "st_file_attributes", 0) & 0x400:
            raise RuntimeError("Census path contains a link or reparse point")
    metadata = path.stat()
    if directory:
        if not stat.S_ISDIR(metadata.st_mode):
            raise RuntimeError("Census root is not a directory")
    elif not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
        raise RuntimeError("Census input must be a single unlinked regular file")
    return path


def _read(path, maximum):
    path = _safe(path)
    before = path.stat()
    if before.st_size > maximum:
        raise RuntimeError("Census input exceeds its byte bound")
    with path.open("rb") as stream:
        opened = os.fstat(stream.fileno())
        if (before.st_dev, before.st_ino) != (opened.st_dev, opened.st_ino):
            raise RuntimeError("Census input changed before read")
        data = stream.read(maximum + 1)
        after = os.fstat(stream.fileno())
    if len(data) != before.st_size or len(data) > maximum or (
            opened.st_size, opened.st_mtime_ns, opened.st_ctime_ns) != (
            after.st_size, after.st_mtime_ns, after.st_ctime_ns):
        raise RuntimeError("Census input changed during read")
    return data


def _digest(path):
    path = _safe(path)
    before = path.stat()
    if before.st_size > 1 << 30:
        raise RuntimeError("Census executable exceeds 1 GiB")
    with path.open("rb") as stream:
        opened = os.fstat(stream.fileno())
        if (before.st_dev, before.st_ino) != (opened.st_dev, opened.st_ino):
            raise RuntimeError("Census executable changed before hash")
        result = hashlib.file_digest(stream, "sha256").hexdigest()
        after = os.fstat(stream.fileno())
    if (opened.st_size, opened.st_mtime_ns, opened.st_ctime_ns) != (
            after.st_size, after.st_mtime_ns, after.st_ctime_ns):
        raise RuntimeError("Census executable changed during hash")
    return result


def _images(install, identity):
    install = _safe(install, directory=True)
    if not isinstance(identity, str) or not HEX.fullmatch(identity):
        raise RuntimeError("Invalid exact predecessor identity")
    runtime = _safe(install / "runtimes" / identity, directory=True)
    raw = _read(runtime / "airk-install.json", 1 << 20)
    if hashlib.sha256(raw).hexdigest() != identity:
        raise RuntimeError("Predecessor inventory digest mismatch")
    inventory = json.loads(raw)
    if not isinstance(inventory, dict) or set(inventory) != {"schema", "version", "os", "arch", "files"} or inventory["schema"] != 1:
        raise RuntimeError("Unsupported predecessor inventory")
    files = inventory["files"]
    if not isinstance(files, dict) or not 1 <= len(files) <= 2048:
        raise RuntimeError("Invalid predecessor file inventory")
    for name, digest in files.items():
        if not isinstance(name, str):
            raise RuntimeError("Unsafe predecessor inventory entry")
        parts = PurePosixPath(name)
        if (not isinstance(name, str) or not name or "\\" in name or ":" in name
                or parts.is_absolute() or any(part in (".", "..") for part in name.split("/"))
                or not isinstance(digest, str) or not HEX.fullmatch(digest)):
            raise RuntimeError("Unsafe predecessor inventory entry")
    suffix = {"windows": ".exe", "linux": ""}.get(inventory["os"])
    if suffix is None:
        raise RuntimeError("Process census supports Windows and Linux predecessors only")
    pair_raw = _read(runtime / "airk-runtime.json", 65536)
    if hashlib.sha256(pair_raw).hexdigest() != files.get("airk-runtime.json"):
        raise RuntimeError("Predecessor runtime metadata is not inventory-bound")
    pair = json.loads(pair_raw)
    if (not isinstance(pair, dict) or set(pair) != {"schema", "version", "os", "arch", "airk_sha256", "embed_sha256"}
            or pair["schema"] != 1 or any(pair[key] != inventory[key] for key in ("version", "os", "arch"))):
        raise RuntimeError("A genuine schema-1 predecessor pair is required")
    images = {}
    def add(path, expected):
        if not isinstance(expected, str) or not HEX.fullmatch(expected) or _digest(path) != expected:
            raise RuntimeError("Predecessor executable seal mismatch")
        images[_key(path)] = {"path": str(path), "executable_sha256": expected}
    for name, field in [("airk" + suffix, "airk_sha256"), ("airk-embed" + suffix, "embed_sha256")]:
        if files.get(name) != pair[field]:
            raise RuntimeError("Predecessor main/helper inventory mismatch")
        add(runtime / name, pair[field])
    for name in ("airk-service", "airk-index", "airk-source"):
        path = runtime / (name + suffix)
        if path.exists() or path.is_symlink():
            add(path, pair["airk_sha256"])
    launcher, receipt = install / ("airk-launcher" + suffix), install / "launcher.json"
    if launcher.exists() or launcher.is_symlink():
        saved = json.loads(_read(receipt, 4096))
        if not isinstance(saved, dict) or set(saved) != {"schema", "sha256"} or saved["schema"] != 1:
            raise RuntimeError("Unknown stable launcher receipt")
        add(launcher, saved["sha256"])
    return images


class _LinuxOwner:
    def __init__(self, pid):
        self.fd = os.pidfd_open(pid, 0)
        self.pid = pid

    def observation(self):
        directory = Path("/proc") / str(self.pid)
        uid = directory.stat().st_uid
        text = (directory / "stat").read_text(encoding="utf-8")
        # comm may contain spaces/parentheses; field 22 is 19 after the final ')'.
        start = int(text[text.rfind(")") + 2:].split()[19])
        path = os.readlink(directory / "exe")
        deleted = path.endswith(" (deleted)")
        return {"pid": self.pid, "start": str(start), "path": path.removesuffix(" (deleted)"), "user": str(uid), "image_deleted": deleted}

    def gone(self):
        poll = select.poll()
        poll.register(self.fd, select.POLLIN)
        return bool(poll.poll(0))

    def close(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None


class _Linux:
    user = str(os.geteuid()) if hasattr(os, "geteuid") else None

    def pids(self):
        return sorted(int(entry.name) for entry in Path("/proc").iterdir() if entry.name.isdecimal())

    def open(self, pid):
        return _LinuxOwner(pid)


class _Windows:
    def __init__(self):
        from ctypes import wintypes as w
        self.w = w
        self.kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        self.advapi = ctypes.WinDLL("advapi32", use_last_error=True)
        k, a = self.kernel, self.advapi
        signatures = [(k.OpenProcess, [w.DWORD, w.BOOL, w.DWORD], w.HANDLE),
                      (k.GetCurrentProcess, [], w.HANDLE),
                      (k.CloseHandle, [w.HANDLE], w.BOOL),
                      (k.QueryFullProcessImageNameW, [w.HANDLE, w.DWORD, w.LPWSTR, ctypes.POINTER(w.DWORD)], w.BOOL),
                      (k.GetProcessTimes, [w.HANDLE, ctypes.POINTER(w.FILETIME), ctypes.POINTER(w.FILETIME), ctypes.POINTER(w.FILETIME), ctypes.POINTER(w.FILETIME)], w.BOOL),
                      (k.WaitForSingleObject, [w.HANDLE, w.DWORD], w.DWORD),
                      (k.LocalFree, [ctypes.c_void_p], ctypes.c_void_p),
                      (a.OpenProcessToken, [w.HANDLE, w.DWORD, ctypes.POINTER(w.HANDLE)], w.BOOL),
                      (a.GetTokenInformation, [w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD, ctypes.POINTER(w.DWORD)], w.BOOL),
                      (a.ConvertSidToStringSidW, [ctypes.c_void_p, ctypes.POINTER(w.LPWSTR)], w.BOOL)]
        for function, arguments, result in signatures:
            function.argtypes, function.restype = arguments, result
        self.psapi = ctypes.WinDLL("psapi", use_last_error=True)
        self.psapi.EnumProcesses.argtypes = [ctypes.POINTER(w.DWORD), w.DWORD, ctypes.POINTER(w.DWORD)]
        self.psapi.EnumProcesses.restype = w.BOOL
        self.user = self.user_for(k.GetCurrentProcess())

    def user_for(self, process):
        token = self.w.HANDLE()
        if not self.advapi.OpenProcessToken(process, 8, ctypes.byref(token)):
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            size = self.w.DWORD()
            self.advapi.GetTokenInformation(token, 1, None, 0, ctypes.byref(size))
            if not 1 <= size.value <= 65536:
                raise RuntimeError("Process user token exceeds census bound")
            data = ctypes.create_string_buffer(size.value)
            if not self.advapi.GetTokenInformation(token, 1, data, size, ctypes.byref(size)):
                raise ctypes.WinError(ctypes.get_last_error())
            sid = ctypes.cast(data, ctypes.POINTER(ctypes.c_void_p))[0]
            text = self.w.LPWSTR()
            if not self.advapi.ConvertSidToStringSidW(sid, ctypes.byref(text)):
                raise ctypes.WinError(ctypes.get_last_error())
            try:
                return text.value
            finally:
                self.kernel.LocalFree(ctypes.cast(text, ctypes.c_void_p))
        finally:
            self.kernel.CloseHandle(token)

    def pids(self):
        entries = (self.w.DWORD * (MAX_SCANNED + 1))()
        used = self.w.DWORD()
        if not self.psapi.EnumProcesses(entries, ctypes.sizeof(entries), ctypes.byref(used)):
            raise ctypes.WinError(ctypes.get_last_error())
        count = used.value // ctypes.sizeof(self.w.DWORD)
        if count > MAX_SCANNED:
            raise RuntimeError("Process census exceeds scan bound")
        return sorted(pid for pid in entries[:count] if pid)

    def open(self, pid):
        # No PROCESS_TERMINATE or write permission is ever requested.
        handle = self.kernel.OpenProcess(0x1000 | 0x100000, False, pid)
        if not handle:
            raise ctypes.WinError(ctypes.get_last_error())
        return _WindowsOwner(self, pid, handle)


class _WindowsOwner:
    def __init__(self, backend, pid, handle):
        self.backend, self.pid, self.handle = backend, pid, handle

    def observation(self):
        b, w = self.backend, self.backend.w
        buffer, size = ctypes.create_unicode_buffer(32768), w.DWORD(32768)
        if not b.kernel.QueryFullProcessImageNameW(self.handle, 0, buffer, ctypes.byref(size)):
            raise ctypes.WinError(ctypes.get_last_error())
        creation, exit_time, kernel, user = w.FILETIME(), w.FILETIME(), w.FILETIME(), w.FILETIME()
        if not b.kernel.GetProcessTimes(self.handle, ctypes.byref(creation), ctypes.byref(exit_time), ctypes.byref(kernel), ctypes.byref(user)):
            raise ctypes.WinError(ctypes.get_last_error())
        return {"pid": self.pid, "start": str((creation.dwHighDateTime << 32) | creation.dwLowDateTime),
                "path": buffer.value, "user": b.user_for(self.handle)}

    def gone(self):
        result = self.backend.kernel.WaitForSingleObject(self.handle, 0)
        if result not in (0, 258):
            raise RuntimeError("Held process identity wait failed")
        return result == 0

    def close(self):
        if self.handle is not None:
            self.backend.kernel.CloseHandle(self.handle)
            self.handle = None


def _backend():
    if sys.platform == "win32":
        return _Windows()
    if sys.platform == "linux" and hasattr(os, "pidfd_open"):
        return _Linux()
    raise RuntimeError("Held-identity census requires Windows or Linux pidfd support")


class Census:
    def __init__(self, install, identity, required_pids):
        if (not isinstance(required_pids, set) or len(required_pids) > MAX_MATCHES
                or any(type(pid) is not int or pid <= 0 for pid in required_pids)):
            raise RuntimeError("Invalid required predecessor PID set")
        self.images = _images(Path(install), identity)
        self.install = Path(install)
        self.runtime = self.install / "runtimes" / identity
        self.identity = identity
        inventory = json.loads(_read(self.runtime / "airk-install.json", 1 << 20))
        self.pair_identity = inventory["files"]["airk-runtime.json"]
        self.backend = _backend()
        self.owners = []
        self.closed = False
        try:
            self.before = self._scan(hold=True)
            if not required_pids.issubset({row["pid"] for row in self.before["processes"]}):
                raise RuntimeError("Required predecessor process identity was not captured")
            self.before.update({"schema": 1, "install": str(install), "old_identity": identity,
                                "required_pids": sorted(required_pids),
                                "images": list(self.images.values())})
        except BaseException:
            self.close()
            raise

    def _scan(self, hold=False):
        pids = self.backend.pids()
        if len(pids) > MAX_SCANNED or len(set(pids)) != len(pids):
            raise RuntimeError("Process census exceeds scan bound or repeats PIDs")
        matches, unavailable = [], 0
        for pid in pids:
            owner = None
            retained = False
            matched = False
            try:
                owner = self.backend.open(pid)
                if owner.gone():
                    continue
                row = owner.observation()
                image = self.images.get(_key(row["path"]))
                if image is None:
                    continue
                matched = True
                if row.get("image_deleted"):
                    raise RuntimeError("Exact predecessor mapped image has been removed")
                if row["user"] != self.backend.user:
                    raise RuntimeError("Exact predecessor image is owned by another user")
                # Bind the held process/start identity to the sealed private
                # executable path. This hashes the approved on-disk file, not
                # process memory or a second independently mapped-image handle.
                if _digest(Path(row["path"])) != image["executable_sha256"]:
                    raise RuntimeError("Captured predecessor image seal changed")
                again = owner.observation()
                if again != row:
                    raise RuntimeError("Captured predecessor identity changed")
                if owner.gone():
                    continue
                matches.append({**row, "executable_sha256": image["executable_sha256"]})
                if len(matches) > MAX_MATCHES:
                    raise RuntimeError("Process census exceeds match bound")
                if hold:
                    self.owners.append(owner)
                    retained = True
            except OSError as error:
                if matched:
                    raise RuntimeError("Exact predecessor identity became unavailable") from error
                if error.errno not in (errno.ENOENT, errno.ESRCH, errno.EACCES, errno.EPERM) and getattr(error, "winerror", None) not in (5, 87, 1168):
                    raise
                unavailable += 1
            finally:
                if owner is not None and not retained:
                    owner.close()
        return {"user": self.backend.user, "processes": matches, "enumerated": len(pids),
                "unobserved": unavailable, "global_completeness": "not_certified",
                "executable_sha256_basis": "sealed_private_path_file_not_process_memory",
                "held_identity_authoritative": hold}

    def verify_gone(self, timeout=10):
        if self.closed or type(timeout) not in (int, float) or not math.isfinite(timeout) or not 0 <= timeout <= 60:
            raise RuntimeError("Invalid closed census or wait bound")
        deadline = time.monotonic() + timeout
        while any(not owner.gone() for owner in self.owners):
            if time.monotonic() >= deadline:
                raise RuntimeError("Captured predecessor process is still alive")
            time.sleep(min(0.025, max(0, deadline - time.monotonic())))
        # The immutable rollback runtime must remain intact. Only the separate
        # stable launcher may be legitimately retired by the modern candidate.
        for name, expected, bound in [("airk-install.json", self.identity, 1 << 20),
                                      ("airk-runtime.json", self.pair_identity, 65536)]:
            if hashlib.sha256(_read(self.runtime / name, bound)).hexdigest() != expected:
                raise RuntimeError("Immutable predecessor runtime receipt changed")
        for image in self.images.values():
            path = Path(image["path"])
            if path.parent == self.runtime and _digest(path) != image["executable_sha256"]:
                raise RuntimeError("Immutable predecessor rollback image changed")
        after = self._scan()
        if after["processes"]:
            raise RuntimeError("Exact predecessor executable survivors remain")
        return {"schema": 1, "captured_identities_exited": True,
                "captured_count": len(self.owners), "after": after,
                "signals_sent": 0, "numeric_pid_reuse_is_not_exit_evidence": True}

    def close(self):
        for owner in self.owners:
            owner.close()
        self.owners.clear()
        self.closed = True

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


def capture(install: Path, old_identity: str, required_pids: set[int]) -> Census:
    return Census(install, old_identity, required_pids)
