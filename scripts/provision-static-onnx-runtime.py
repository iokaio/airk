# SPDX-License-Identifier: Apache-2.0
"""Build pinned static CPU ORT explicitly; never download or install at AIRK startup."""
import argparse
import ctypes
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import platform
import posixpath
import re
import shutil
import signal
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import types
import urllib.parse
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
LOCK_PATH = ROOT / "scripts/static-onnx-runtime-lock.json"
LOCK = json.loads(LOCK_PATH.read_text(encoding="utf-8"))
VERSION = "1.30.0"
COMMIT = "f2c39fe2f838cf35ce7da92824f5a5e3ee6e88a7"
TARGETS = {
    "x86_64-pc-windows-msvc": ("Windows", "x86_64"),
    "x86_64-unknown-linux-gnu": ("Linux", "x86_64"),
    "aarch64-apple-darwin": ("Darwin", "aarch64"),
    "x86_64-apple-darwin": ("Darwin", "x86_64"),
}
CPU_NAMES = {"abseil_cpp", "date", "eigen", "flatbuffers", "fp16", "json",
             "microsoft_gsl", "microsoft_wil", "mp11", "onnx", "protobuf",
             "pytorch_cpuinfo", "re2", "safeint"}
REDIRECT_HOSTS = {"github.com", "codeload.github.com", "release-assets.githubusercontent.com",
                  "objects.githubusercontent.com", "raw.githubusercontent.com"}
MAX_JSON = 16 * 1024 * 1024
MAX_LOG = 64 * 1024 * 1024
STATIC_COMPONENTS = ("common", "flatbuffers", "framework", "graph", "lora", "mlas",
                     "optimizer", "providers", "session", "util")
OFF_OPTIONS = ("BUILD_SHARED_LIB", "BUILD_UNIT_TESTS", "BUILD_BENCHMARKS", "BUILD_FOR_NATIVE_MACHINE",
               "MINIMAL_BUILD", "DISABLE_CONTRIB_OPS", "DISABLE_ML_OPS", "ENABLE_PYTHON", "USE_CUDA",
               "USE_DML", "USE_VCPKG", "USE_TELEMETRY", "ENABLE_LTO", "USE_COREML", "USE_KLEIDIAI")
# One already completed, reviewed C1 source build. Import never reruns it or
# changes its original source/build times, native cleanup result or library hashes.
LEGACY_C1 = {
    "windows-static-build-result.json": "33d0c5ddfa908e84321c9d7008c47739fd5665f2370f6c96e0b44d3a87234201",
    "run-windows-static-ort.py": "b483dc6677298885f962a090e291fca527ec550a21236daf05f141dc1e2a8fa0",
    "cpu-dependencies.json": "067284e8a09958a10cf0f59f506054d6c2115ea1cecd496bff5ab53a25ab4787",
}


def stream_digest(source, algorithm="sha256"):
    value = hashlib.new(algorithm)
    for block in iter(lambda: source.read(1024 * 1024), b""):
        value.update(block)
    return value


def digest(path, algorithm="sha256"):
    with Path(path).open("rb") as source:
        return stream_digest(source, algorithm).hexdigest()


def plain(path, regular=False):
    """Reject aliases in every existing path component, including Windows reparse points."""
    path = Path(os.path.abspath(path))
    for part in (path, *path.parents):
        try:
            metadata = part.lstat()
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(metadata.st_mode) or getattr(metadata, "st_file_attributes", 0) & 0x400:
            raise ValueError("Linked static-runtime path: " + str(part))
    if regular:
        metadata = path.lstat()
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
            raise ValueError("Static-runtime input must be a regular unaliased file: " + str(path))
    return path


def relative_name(name):
    if (not isinstance(name, str) or not name or "\\" in name or ":" in name
            or any(ord(char) < 32 for char in name)):
        raise ValueError("Unsafe archive/receipt path")
    path = PurePosixPath(name)
    if path.is_absolute() or any(part in (".", "..") for part in name.split("/")):
        raise ValueError("Archive/receipt path escapes owned root")
    return path


def child(root, relative):
    relative_name(relative)
    result = plain(Path(root).joinpath(*PurePosixPath(relative).parts))
    if not result.is_relative_to(plain(root)):
        raise ValueError("Static-runtime path escaped owned root")
    return result


def read_json(path, maximum=MAX_JSON):
    path = plain(path, regular=True)
    if path.stat().st_size > maximum:
        raise ValueError("Oversized static-runtime receipt")
    return json.loads(path.read_bytes())


def write_json(path, value, replace=False):
    path = plain(path)
    data = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    if len(data) > MAX_JSON:
        raise ValueError("Static-runtime receipt budget exceeded")
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        plain(path, regular=True)
        if path.read_bytes() == data:
            return
        if not replace:
            raise ValueError("Refusing to overwrite a different static-runtime receipt: " + str(path))
    with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".ort-receipt-", delete=False) as output:
        temporary = Path(output.name)
        output.write(data)
        output.flush()
        os.fsync(output.fileno())
    if replace:
        os.replace(temporary, path)
    else:
        # Only this newly created temporary file is removed; never a build tree.
        try:
            with path.open("xb") as output:
                output.write(data)
                output.flush()
                os.fsync(output.fileno())
        finally:
            temporary.unlink()


def verify_file(path, row, system_tool=False):
    path = Path(path).resolve(strict=True) if system_tool else plain(path, regular=True)
    if system_tool and not path.is_file():
        raise ValueError("Build tool is not a regular file")
    if path.stat().st_size != row["bytes"] or digest(path) != row["sha256"]:
        raise ValueError("Pinned static-runtime file differs: " + str(path))
    if row.get("upstream_sha1") and digest(path, "sha1") != row["upstream_sha1"]:
        raise ValueError("Upstream dependency SHA1 differs: " + str(path))


def archive_url(url, source=False):
    parts = urllib.parse.urlsplit(url)
    allowed = "codeload.github.com" if source else "github.com"
    if (parts.scheme != "https" or parts.netloc != allowed or parts.query or parts.fragment
            or parts.username or parts.password or "%" in parts.path):
        raise ValueError("Only pinned official HTTPS archive URLs are supported")
    relative_name(parts.path.lstrip("/"))
    return parts


def validate_lock(lock=LOCK):
    if lock.get("schema") != 1 or lock.get("ort_version") != VERSION or lock.get("upstream_commit") != COMMIT:
        raise ValueError("Static ORT source/version lock differs")
    source = lock["source"]
    archive_url(source["url"], source=True)
    if source["url"] != "https://codeload.github.com/microsoft/onnxruntime/tar.gz/" + COMMIT:
        raise ValueError("Static ORT source commit URL differs")
    if source["archive_root"] != "onnxruntime-" + COMMIT:
        raise ValueError("Static ORT archive root differs")
    if not 0 < source["bytes"] <= 320 * 1024 * 1024 or not 0 < source["unpacked_bytes"] <= 1024 * 1024 * 1024:
        raise ValueError("Static ORT source bounds differ")
    if not 0 < source["members"] <= 20000:
        raise ValueError("Static ORT source member bound differs")
    if source["files"] + source["directories"] != source["members"] or not re.fullmatch(r"[0-9a-f]{64}", source["inventory_sha256"]):
        raise ValueError("Static ORT expanded source seal differs")
    if {row["name"] for row in lock["dependencies"]} != CPU_NAMES or len(lock["dependencies"]) != len(CPU_NAMES):
        raise ValueError("Static ORT CPU dependency roster differs")
    rows = [source, *lock["dependencies"], *lock["mac_host_dependencies"]]
    if [row["name"] for row in lock["mac_host_dependencies"]] != ["protoc_mac_universal"]:
        raise ValueError("Static ORT Mac host dependency differs")
    for row in rows:
        if not re.fullmatch(r"[0-9a-f]{64}", row["sha256"]):
            raise ValueError("Invalid static ORT SHA256")
        if row is not source:
            archive_url(row["url"])
            if not 0 < row["bytes"] <= 100 * 1024 * 1024 or not re.fullmatch(r"[0-9a-f]{40}", row["upstream_sha1"]):
                raise ValueError("Invalid static ORT dependency bound/hash")
    if len(lock.get("additional_notices", [])) != 1:
        raise ValueError("Static ORT additional full-license roster differs")
    license_row = lock["additional_notices"][0]
    notice_url(license_row)
    return lock


def notice_url(row):
    commit = row.get("source_commit", "")
    if (row.get("name") != "mp11-boost-license" or row.get("dependency") != "mp11" or row.get("license_id") != "BSL-1.0"
            or not re.fullmatch(r"[0-9a-f]{40}", commit)
            or row.get("url") != "https://raw.githubusercontent.com/boostorg/boost/" + commit + "/LICENSE_1_0.txt"
            or row.get("cache_name") != "boost-1.82.0-LICENSE_1_0.txt" or row.get("path") != "notices/mp11/LICENSE_1_0.txt"
            or not 0 < row.get("bytes", 0) <= 65536 or not re.fullmatch(r"[0-9a-f]{64}", row.get("sha256", ""))):
        raise ValueError("Invalid pinned official full-license source")


class PinnedRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, file, code, message, headers, url):
        parsed = urllib.parse.urlsplit(url)
        if parsed.scheme != "https" or parsed.hostname not in REDIRECT_HOSTS or parsed.username or parsed.password:
            raise ValueError("Unapproved static-runtime download redirect")
        return super().redirect_request(request, file, code, message, headers, url)


def fetch(path, row, download=False, source=False, notice=False):
    path = plain(path)
    notice_url(row) if notice else archive_url(row["url"], source=source)
    if path.exists():
        verify_file(path, row)
        return
    if not download:
        raise ValueError("Missing offline static ORT input; supply verified archive or use --download: " + str(path))
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".partial")
    plain(temporary)
    request = urllib.request.Request(row["url"], headers={"User-Agent": "AIRK-static-ORT/1"})
    started = time.monotonic()
    with urllib.request.build_opener(PinnedRedirect()).open(request, timeout=60) as response, temporary.open("xb") as output:
        parsed = urllib.parse.urlsplit(response.url)
        if parsed.scheme != "https" or parsed.hostname not in REDIRECT_HOSTS:
            raise ValueError("Unapproved static-runtime download origin")
        total = 0
        while block := response.read(1024 * 1024):
            total += len(block)
            if total > row["bytes"] or time.monotonic() - started > 300:
                raise ValueError("Static ORT download exceeded pinned size/deadline; partial retained")
            output.write(block)
        output.flush()
        os.fsync(output.fileno())
    verify_file(temporary, row)
    # Existing/crossed input is never silently replaced.
    if path.exists():
        raise ValueError("Static ORT archive appeared during download")
    temporary.rename(path)


def selected_dependencies(target, lock=LOCK):
    if target not in TARGETS:
        raise ValueError("Unsupported static ORT target")
    return lock["dependencies"] + (lock["mac_host_dependencies"] if TARGETS[target][0] == "Darwin" else [])


def owner_value(target):
    return {"schema": 1, "owner": "AIRK-static-ORT", "target": target,
            "upstream_commit": COMMIT, "lock_sha256": digest(LOCK_PATH)}


def owned_destination(destination, target, create=False):
    if target not in TARGETS:
        raise ValueError("Unsupported static ORT target")
    destination = plain(destination)
    owner = destination / "owner.json"
    if owner.exists():
        if read_json(owner) != owner_value(target):
            raise ValueError("Static ORT destination has different owner/target/lock")
    elif create:
        if destination.exists() and any(destination.iterdir()):
            raise ValueError("Refusing to adopt a nonempty static ORT destination")
        destination.mkdir(parents=True, exist_ok=True)
        write_json(owner, owner_value(target))
    else:
        raise ValueError("Static ORT destination has no ownership receipt")
    return destination


def source_inventory(source, exclude_mirror=False):
    rows = []
    for path in sorted(source.rglob("*"), key=lambda entry: entry.relative_to(source).as_posix()):
        plain(path)
        relative = path.relative_to(source).as_posix()
        if exclude_mirror and (relative == "mirror" or relative.startswith("mirror/")):
            continue
        if path.is_file():
            plain(path, regular=True)
            rows.append({"path": relative, "bytes": path.stat().st_size, "sha256": digest(path)})
        elif not path.is_dir():
            raise ValueError("Unexpected source entry type")
    return rows


def extract_source(archive, destination, source_row):
    """Extract only bounded regular files/directories; preserve failures for review."""
    source = child(destination, "source")
    if source.exists():
        raise ValueError("Refusing to overwrite a source tree without its complete inventory")
    with tarfile.open(archive, "r:gz") as tar:
        members = []
        expanded = 0
        for member in tar:
            expanded += member.size
            if len(members) >= source_row["members"] or member.size < 0 or expanded > source_row["unpacked_bytes"]:
                raise ValueError("ORT source expansion exceeds pinned bounds")
            members.append(member)
        if len(members) != source_row["members"] or sum(row.size for row in members) != source_row["unpacked_bytes"]:
            raise ValueError("Pinned ORT source expansion bounds differ")
        seen = set()
        selected = []
        for member in members:
            name = relative_name(member.name)
            if name.parts[0] != source_row["archive_root"] or not (member.isfile() or member.isdir()):
                raise ValueError("ORT source has a link/special/crossed archive member")
            if len(name.parts) == 1:
                if not member.isdir():
                    raise ValueError("ORT archive root is not a directory")
                continue
            relative = PurePosixPath(*name.parts[1:]).as_posix()
            key = relative.casefold() if os.name == "nt" else relative
            if key in seen or member.size > 64 * 1024 * 1024:
                raise ValueError("Duplicate/oversized ORT source member")
            seen.add(key)
            selected.append((member, relative))
        source.mkdir()
        for member, relative in selected:
            path = child(source, relative)
            if member.isdir():
                path.mkdir(parents=True, exist_ok=True)
            else:
                path.parent.mkdir(parents=True, exist_ok=True)
                with tar.extractfile(member) as stream, path.open("xb") as output:
                    shutil.copyfileobj(stream, output, 1024 * 1024)
                if path.stat().st_size != member.size:
                    raise ValueError("ORT source member size changed")
                if os.name != "nt":
                    path.chmod(0o755 if member.mode & 0o111 else 0o644)
    rows = source_inventory(source)
    write_json(destination / "source-files.json", rows)


def verify_source(destination, target):
    source = child(destination, "source")
    rows = read_json(destination / "source-files.json")
    if len(rows) != LOCK["source"]["files"] or digest(destination / "source-files.json") != LOCK["source"]["inventory_sha256"]:
        raise ValueError("ORT source file roster differs")
    if source_inventory(source, exclude_mirror=True) != rows:
        raise ValueError("ORT source files changed after pinned extraction")
    for row in LOCK["source_metadata"]:
        verify_file(child(source, row["path"]), row)
    if child(source, "VERSION_NUMBER").read_text().strip() != VERSION:
        raise ValueError("ORT source version differs")
    text = child(source, "cmake/deps.txt").read_text()
    actual = {}
    for line in text.splitlines():
        if line and not line.startswith("#"):
            name, url, sha1 = line.split(";")
            actual[name] = (url, sha1)
    for row in selected_dependencies(target):
        if actual.get(row["name"]) != (row["url"], row["upstream_sha1"]):
            raise ValueError("Pinned dependency disagrees with ORT deps.txt")
        verify_file(child(source / "mirror", mirror_relative(row)), row)
    expected = {mirror_relative(row) for row in selected_dependencies(target)}
    actual_paths = {row["path"] for row in source_inventory(source / "mirror")}
    if actual_paths != expected:
        raise ValueError("Unexpected dependency mirror input")
    return rows


def mirror_relative(row):
    url = archive_url(row["url"])
    return url.netloc + url.path


def prepare(destination, target, download=False):
    validate_lock()
    destination = owned_destination(destination, target, create=True)
    archive = child(destination, "downloads/source.tar.gz")
    fetch(archive, LOCK["source"], download, source=True)
    if not (destination / "source-files.json").exists():
        extract_source(archive, destination, LOCK["source"])
    for row in selected_dependencies(target):
        local = child(destination, "downloads/" + row["name"] + ".zip")
        fetch(local, row, download)
        mirror = child(destination / "source/mirror", mirror_relative(row))
        if mirror.exists():
            verify_file(mirror, row)
        else:
            mirror.parent.mkdir(parents=True, exist_ok=True)
            with local.open("rb") as input_file, mirror.open("xb") as output:
                shutil.copyfileobj(input_file, output, 1024 * 1024)
    for row in LOCK["additional_notices"]:
        fetch(child(destination, "downloads/" + row["cache_name"]), row, download, notice=True)
    verify_source(destination, target)
    return destination


def run_owned(argv, cwd, log, timeout, environment=None):
    """Own a build/tool child; its finite positive observer limit stays at 10800s."""
    return _run_owned(argv, cwd, log, timeout, environment, 10800)


def run_evaluation_fixture(argv, cwd, log, timeout, environment=None):
    """Explicit private evaluation observer, declared before trial scores.

    Only the observer duration changes (finite positive, at most 21600s).
    The caller records the scientific phase and independent native fixture
    deadline before launch; this never extends model/read/resource budgets.
    Suspended launch, descendant ownership, logging and cleanup are identical.
    """
    return _run_owned(argv, cwd, log, timeout, environment, 21600)


def _run_owned(argv, cwd, log, timeout, environment, maximum_timeout):
    """Shared launch mechanism; the two named entrypoints own duration policy."""
    argv = [str(value) for value in argv]
    if (not argv or any("\0" in value for value in argv)
            or type(timeout) not in (int, float) or not 0 < timeout <= maximum_timeout):
        raise ValueError("Invalid owned command/finite positive observer deadline")
    log = plain(log)
    log.parent.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    with log.open("xb") as output:
        if os.name == "nt":
            result = run_windows_owned(argv, cwd, output, timeout, environment)
        else:
            result = run_unix_owned(argv, cwd, output, timeout, environment)
    result.update({"argv": argv, "log": str(log), "elapsed_seconds": round(time.monotonic() - started, 3)})
    return result


def darwin_group_members(group):
    """Bounded exact-group observation while our unreaped leader pins its ID.

    Public SDK proc_bsdinfo ABI (136 bytes), PROC_PGRP_ONLY=2,
    PROC_PIDTBSDINFO=3. A failed observation never means an empty group.
    """
    class BsdInfo(ctypes.Structure):
        _fields_ = [(name, ctypes.c_uint32) for name in
                    ("flags", "status", "xstatus", "pid", "ppid", "uid", "gid", "ruid", "rgid", "svuid", "svgid", "rfu1")] + [
                    ("comm", ctypes.c_char * 16), ("name", ctypes.c_char * 32)] + [
                    (name, ctypes.c_uint32) for name in ("nfiles", "pgid", "pjobc", "e_tdev", "e_tpgid")] + [
                    ("nice", ctypes.c_int32), ("start_sec", ctypes.c_uint64), ("start_usec", ctypes.c_uint64)]
    if ctypes.sizeof(BsdInfo) != 136:
        raise ValueError("Unsupported Darwin proc_bsdinfo ABI")
    api = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
    api.proc_listpids.argtypes = [ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p, ctypes.c_int]
    api.proc_listpids.restype = ctypes.c_int
    api.proc_pidinfo.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64, ctypes.c_void_p, ctypes.c_int]
    api.proc_pidinfo.restype = ctypes.c_int
    buffer = (ctypes.c_int * 1024)()
    ctypes.set_errno(0)
    count = api.proc_listpids(2, group, buffer, ctypes.sizeof(buffer))
    if count < 0 or (count == 0 and ctypes.get_errno()) or count >= ctypes.sizeof(buffer) or count % ctypes.sizeof(ctypes.c_int):
        raise ValueError("Darwin owned group observation denied/truncated")
    rows = []
    missing = []
    deadline = time.monotonic() + 2
    for pid in sorted(set(buffer[:count // ctypes.sizeof(ctypes.c_int)]) - {0}):
        if time.monotonic() >= deadline:
            raise ValueError("Darwin owned group observation deadline exceeded")
        info = BsdInfo()
        observed = api.proc_pidinfo(pid, 3, 0, ctypes.byref(info), ctypes.sizeof(info))
        if observed != ctypes.sizeof(info):
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                continue  # Exited between bounded census and detail read.
            # Darwin's public libproc detail API excludes zombies although its
            # group enumeration includes them. Ask the OS ps utility for this
            # one exact PID; accept ONLY an explicit same-user/group zombie.
            # This is observation, never a signal or a generic EPERM waiver.
            missing.append(pid)
            continue
        if info.pid != pid or info.pgid != group or info.uid != os.getuid() or not 1 <= info.status <= 6:
            raise ValueError("Darwin owned group identity/status changed")
        rows.append({"pid": pid, "pgid": group, "uid": info.uid, "status": info.status,
                     "start_sec": info.start_sec, "start_usec": info.start_usec})
    if missing:
        rows.extend(darwin_zombie_members(missing, group, deadline))
    return rows


def darwin_zombie_members(pids, group, deadline):
    remaining = deadline - time.monotonic()
    if not 0 < remaining <= 2 or len(pids) > 1024 or len(set(pids)) != len(pids):
        raise ValueError("Darwin owned zombie observation budget exceeded")
    result = subprocess.run(["/bin/ps", "-p", ",".join(str(pid) for pid in pids), "-o", "pid=,pgid=,uid=,stat="],
                            stdin=subprocess.DEVNULL, capture_output=True, timeout=remaining,
                            env={**os.environ, "LC_ALL": "C"})
    if result.returncode != 0 or len(result.stdout) > 64 * 1024 or len(result.stderr) > 4096:
        raise ValueError("Darwin owned zombie observation unavailable")
    rows = []
    seen = set()
    for line in result.stdout.decode("ascii", errors="strict").splitlines():
        values = line.split()
        if (len(values) != 4 or not values[0].isdigit() or int(values[0]) not in pids or values[0] in seen
                or values[1:3] != [str(group), str(os.getuid())] or not values[3].startswith("Z") or not values[3].isalpha()):
            raise ValueError("Darwin owned zombie identity/status changed")
        seen.add(values[0])
        rows.append({"pid": int(values[0]), "pgid": group, "uid": os.getuid(), "status": 5, "observation": "os_ps_exact_zombie"})
    if seen != {str(pid) for pid in pids}:
        raise ValueError("Darwin owned zombie observation incomplete")
    return rows


def run_unix_owned(argv, cwd, output, timeout, environment):
    if all(hasattr(os, name) for name in ("waitid", "P_PID", "WNOWAIT", "WEXITED", "WNOHANG")):
        observe = lambda pid: os.waitid(os.P_PID,pid,os.WEXITED | os.WNOHANG | os.WNOWAIT)
        observation_backend = "python-os-waitid-wnowait"
    elif sys.platform == "darwin":
        observe = load_script("darwin-waitid.py").observer()
        observation_backend = "darwin64-libc-waitid-wnowait"
    else:
        raise ValueError("This Unix host cannot retain owned child identity through descendant cleanup")
    process = subprocess.Popen(argv, cwd=cwd, env=environment, stdin=subprocess.DEVNULL,
                               stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
    end = time.monotonic() + timeout
    timed_out = oversized = forced = False
    observed = None
    cleanup_errors = []
    censuses = []
    return_code = None
    try:
        while True:
            # WNOWAIT holds the leader's actual OS identity until group cleanup;
            # its numerical group ID cannot be recycled for an unrelated job.
            observed = observe(process.pid)
            oversized = os.fstat(output.fileno()).st_size > MAX_LOG
            timed_out = time.monotonic() >= end
            if observed is not None or timed_out or oversized:
                break
            time.sleep(0.05)
    except (OSError, ValueError) as error:
        cleanup_errors.append({"stage": "leader_observation", "error": str(error)})
    finally:
        # A group signal addresses only this unreaped session leader and its
        # descendants. No process names, ambient PIDs or global tree sweeps.
        for signal_value, delay in ((signal.SIGTERM, 0.25), (signal.SIGKILL, 0)):
            try:
                if sys.platform == "darwin":
                    members = darwin_group_members(process.pid)
                    censuses.append(members)
                    if not any(row["status"] != 5 for row in members):
                        # Darwin returns EPERM for signaling a zombie-only
                        # group. A complete native census proves no signal is
                        # necessary; generic EPERM never establishes this.
                        break
                os.killpg(process.pid, signal_value)
                forced = True
            except ProcessLookupError:
                break
            except (OSError, ValueError) as error:
                cleanup_errors.append({"stage": "group_cleanup", "error": str(error)})
                break
            if delay:
                time.sleep(delay)
        try:
            return_code = process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            cleanup_errors.append({"stage": "leader_wait", "error": "owned leader did not exit"})
    # Never signal the numerical group again after reaping its leader. A
    # read-only existence probe can conservatively refuse a reused group ID,
    # but cannot authorize killing an unrelated process.
    settled = False
    if return_code is not None:
        cleanup_end = time.monotonic() + 5
        while True:
            try:
                os.killpg(process.pid, 0)
            except ProcessLookupError:
                settled = True
                break
            except OSError as error:
                cleanup_errors.append({"stage": "post_reap_readonly_probe", "error": str(error)})
                break
            if time.monotonic() >= cleanup_end:
                cleanup_errors.append({"stage": "post_reap_readonly_probe", "error": "owned group not absent"})
                break
            time.sleep(0.05)
    return {"exit_code": return_code, "timed_out": timed_out, "log_oversized": oversized,
            "leader_observation_backend": observation_backend,
            "owned_descendants_settled": settled and not cleanup_errors, "owned_group_signaled": forced,
            "owned_leader_pid": process.pid, "observed_exit": None if observed is None else
            {"pid": observed.si_pid, "code": observed.si_code, "status": observed.si_status},
            "darwin_group_censuses": censuses, "cleanup_errors": cleanup_errors}


def run_windows_owned(argv, cwd, output, timeout, environment):
    """Drain actual owned members within one five-second cleanup deadline.

    Leader exit alone is not job settlement. Retain native member handles for
    event waits, then require fresh empty PID and accounting observations. A
    forced or unobserved cleanup remains a failed qualification; emergency
    finally cleanup never upgrades that result.
    """
    from ctypes import wintypes as w
    import msvcrt

    class Startup(ctypes.Structure):
        _fields_ = [("cb", w.DWORD), ("reserved", w.LPWSTR), ("desktop", w.LPWSTR),
                    ("title", w.LPWSTR), ("x", w.DWORD), ("y", w.DWORD), ("xsize", w.DWORD),
                    ("ysize", w.DWORD), ("xchars", w.DWORD), ("ychars", w.DWORD),
                    ("fill", w.DWORD), ("flags", w.DWORD), ("show", w.WORD), ("reserved2", w.WORD),
                    ("reserved_bytes", ctypes.c_void_p), ("stdin", w.HANDLE), ("stdout", w.HANDLE), ("stderr", w.HANDLE)]
    class ExtendedStartup(ctypes.Structure):
        _fields_ = [("startup", Startup), ("attributes", ctypes.c_void_p)]
    class ProcessInfo(ctypes.Structure):
        _fields_ = [("process", w.HANDLE), ("thread", w.HANDLE), ("pid", w.DWORD), ("tid", w.DWORD)]
    class Limits(ctypes.Structure):
        _fields_ = [("process_time", ctypes.c_longlong), ("job_time", ctypes.c_longlong),
                    ("flags", w.DWORD), ("min_working", ctypes.c_size_t), ("max_working", ctypes.c_size_t),
                    ("active_limit", w.DWORD), ("affinity", ctypes.c_size_t), ("priority", w.DWORD), ("scheduling", w.DWORD)]
    class IO(ctypes.Structure):
        _fields_ = [(name, ctypes.c_ulonglong) for name in ("read_ops", "write_ops", "other_ops", "read_bytes", "write_bytes", "other_bytes")]
    class ExtendedLimits(ctypes.Structure):
        _fields_ = [("basic", Limits), ("io", IO), ("process_memory", ctypes.c_size_t), ("job_memory", ctypes.c_size_t),
                    ("peak_process", ctypes.c_size_t), ("peak_job", ctypes.c_size_t)]
    class JobAccounting(ctypes.Structure):
        _fields_ = [(name, ctypes.c_longlong) for name in ("user", "kernel", "period_user", "period_kernel")] + [(name, w.DWORD) for name in ("faults", "total", "active", "terminated")]

    api = ctypes.WinDLL("kernel32", use_last_error=True)
    signatures = {
        "CreateJobObjectW": ([ctypes.c_void_p, w.LPCWSTR], w.HANDLE),
        "SetInformationJobObject": ([w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD], w.BOOL),
        "AssignProcessToJobObject": ([w.HANDLE, w.HANDLE], w.BOOL),
        "QueryInformationJobObject": ([w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD, ctypes.c_void_p], w.BOOL),
        "TerminateJobObject": ([w.HANDLE, w.UINT], w.BOOL),
        "TerminateProcess": ([w.HANDLE, w.UINT], w.BOOL),
        "CloseHandle": ([w.HANDLE], w.BOOL),
        "CreateProcessW": ([w.LPCWSTR, w.LPWSTR, ctypes.c_void_p, ctypes.c_void_p, w.BOOL, w.DWORD, ctypes.c_void_p, w.LPCWSTR, ctypes.c_void_p, ctypes.c_void_p], w.BOOL),
        "WaitForSingleObject": ([w.HANDLE, w.DWORD], w.DWORD),
        "GetExitCodeProcess": ([w.HANDLE, ctypes.POINTER(w.DWORD)], w.BOOL),
        "OpenProcess": ([w.DWORD, w.BOOL, w.DWORD], w.HANDLE),
        "IsProcessInJob": ([w.HANDLE, w.HANDLE, ctypes.POINTER(w.BOOL)], w.BOOL),
        "ResumeThread": ([w.HANDLE], w.DWORD),
        "InitializeProcThreadAttributeList": ([ctypes.c_void_p, w.DWORD, w.DWORD, ctypes.POINTER(ctypes.c_size_t)], w.BOOL),
        "UpdateProcThreadAttribute": ([ctypes.c_void_p, w.DWORD, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_void_p], w.BOOL),
        "DeleteProcThreadAttributeList": ([ctypes.c_void_p], None),
    }
    for name, (arguments, result) in signatures.items():
        getattr(api, name).argtypes = arguments
        getattr(api, name).restype = result
    def checked(value):
        if not value:
            raise ctypes.WinError(ctypes.get_last_error())
        return value
    job = checked(api.CreateJobObjectW(None, None))
    info = ProcessInfo()
    assigned = resumed = False
    attributes_initialized = False
    emergency_members = []
    try:
        limits = ExtendedLimits()
        limits.basic.flags = 0x2000  # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        checked(api.SetInformationJobObject(job, 9, ctypes.byref(limits), ctypes.sizeof(limits)))
        # Inherit exactly the output and null-input handles, not the runner's
        # account/token/pipe handles. Parent is never assigned to this job.
        with open(os.devnull, "rb") as input_file:
            handles = (w.HANDLE * 2)(msvcrt.get_osfhandle(input_file.fileno()), msvcrt.get_osfhandle(output.fileno()))
            for handle in handles:
                os.set_handle_inheritable(handle, True)
            size = ctypes.c_size_t()
            api.InitializeProcThreadAttributeList(None, 1, 0, ctypes.byref(size))
            buffer = ctypes.create_string_buffer(size.value)
            checked(api.InitializeProcThreadAttributeList(buffer, 1, 0, ctypes.byref(size)))
            attributes_initialized = True
            checked(api.UpdateProcThreadAttribute(buffer, 0, 0x20002, handles, ctypes.sizeof(handles), None, None))
            startup = ExtendedStartup()
            startup.startup.cb = ctypes.sizeof(startup)
            startup.startup.flags = 0x100  # STARTF_USESTDHANDLES
            startup.startup.stdin, startup.startup.stdout = handles[0], handles[1]
            startup.startup.stderr = handles[1]
            startup.attributes = ctypes.cast(buffer, ctypes.c_void_p)
            environment_block = None
            if environment is not None:
                environment_block = ctypes.create_unicode_buffer("\0".join(key + "=" + value for key, value in sorted(environment.items(), key=lambda row: row[0].upper())) + "\0\0")
            command = ctypes.create_unicode_buffer(subprocess.list2cmdline(argv))
            try:
                checked(api.CreateProcessW(None, command, None, None, True,
                        0x4 | 0x8000000 | 0x80000 | 0x400, environment_block, str(cwd), ctypes.byref(startup), ctypes.byref(info)))
            finally:
                for handle in handles:
                    os.set_handle_inheritable(handle, False)
            # The child is suspended: it cannot spawn an unowned compiler in
            # the interval between creation and native job assignment.
            checked(api.AssignProcessToJobObject(job, info.process))
            assigned = True
            if api.ResumeThread(info.thread) == 0xFFFFFFFF:
                raise ctypes.WinError(ctypes.get_last_error())
            resumed = True
        end = time.monotonic() + timeout
        timed_out = oversized = False
        while True:
            waited = api.WaitForSingleObject(info.process, 50)
            if waited not in (0, 258):
                raise ctypes.WinError(ctypes.get_last_error())
            oversized = os.fstat(output.fileno()).st_size > MAX_LOG
            timed_out = time.monotonic() >= end
            if waited == 0 or timed_out or oversized:
                break
        code = w.DWORD()
        checked(api.GetExitCodeProcess(info.process, ctypes.byref(code)))
        # A signaled leader can leave a console host finishing natural shutdown.
        # Observe every actual job member under the SAME five-second cleanup
        # deadline; names/images never exempt a member from the empty-job proof.
        cleanup_end = time.monotonic() + 5
        accounting = JobAccounting()

        def job_members(deadline=None):
            deadline = cleanup_end if deadline is None else deadline
            capacity = 16
            while True:
                class Members(ctypes.Structure):
                    _fields_ = [("assigned", w.DWORD), ("listed", w.DWORD),
                                ("pids", ctypes.c_size_t * capacity)]
                members = Members()
                observed = api.QueryInformationJobObject(job, 3, ctypes.byref(members), ctypes.sizeof(members), None)
                if observed:
                    if members.listed > capacity:
                        raise ValueError("Owned job PID census was truncated")
                    return [int(members.pids[index]) for index in range(members.listed)]
                if ctypes.get_last_error() != 234:  # ERROR_MORE_DATA: grow, never truncate.
                    raise ctypes.WinError(ctypes.get_last_error())
                if time.monotonic() >= deadline:
                    raise ValueError("Owned job PID census exceeded cleanup deadline")
                capacity = max(capacity * 2, int(members.assigned))

        def observe_job():
            members = job_members()
            checked(api.QueryInformationJobObject(job, 1, ctypes.byref(accounting), ctypes.sizeof(accounting), None))
            return members, int(accounting.active)

        def retain_owned_members(members):
            retained = []
            try:
                for pid in members:
                    handle = api.OpenProcess(0x100000 | 0x1000, False, pid)
                    if not handle:
                        if ctypes.get_last_error() == 87:
                            continue  # Exited before open; recensus the job.
                        raise ctypes.WinError(ctypes.get_last_error())
                    belongs = w.BOOL()
                    try:
                        checked(api.IsProcessInJob(handle, job, ctypes.byref(belongs)))
                    except BaseException:
                        api.CloseHandle(handle)
                        raise
                    if belongs.value:
                        retained.append(handle)
                    else:
                        api.CloseHandle(handle)  # A reused foreign PID is never waited on.
                return retained
            except BaseException:
                for handle in retained:
                    api.CloseHandle(handle)
                raise

        forced = timed_out or oversized
        if forced:
            checked(api.TerminateJobObject(job, 77))
        while True:
            members, active = observe_job()
            if not members and active == 0:
                settled = True
                break
            if time.monotonic() >= cleanup_end:
                if not forced:
                    # Retain actual handles BEFORE forcing: immediate accounting
                    # can disappear before inherited log handles finish closing.
                    emergency_members.extend(retain_owned_members(members))
                    checked(api.TerminateJobObject(job, 77))
                    forced = True
                # No fresh grace period after forcing. An immediate fresh proof
                # can establish settlement; otherwise retain an explicit refusal.
                members, active = observe_job()
                settled = not members and active == 0
                break
            handles = retain_owned_members(members)
            try:
                for handle in handles:
                    remaining = cleanup_end - time.monotonic()
                    if remaining <= 0:
                        break
                    milliseconds = max(1, int(remaining * 1000))
                    observed = api.WaitForSingleObject(handle, milliseconds)
                    if observed not in (0, 258):
                        raise ctypes.WinError(ctypes.get_last_error())
                    if observed == 258:
                        break
            finally:
                for handle in handles:
                    api.CloseHandle(handle)
        checked(api.GetExitCodeProcess(info.process, ctypes.byref(code)))
        result = {"exit_code": int(code.value), "timed_out": timed_out, "log_oversized": oversized,
                  "owned_descendants_settled": settled, "owned_job_forced_termination": forced,
                  "owned_job_final_active_processes": active, "owned_job_final_member_pids": members,
                  "parent_assigned_to_job": False, "child_assigned_before_resume": True}
        if not settled:
            result["cleanup_refusal"] = "Owned job did not settle within the original five-second cleanup deadline"
        return result
    finally:
        # Emergency cleanup has its original single five-second deadline, shared
        # by ALL retained handles. It never upgrades the qualification result.
        emergency_end = time.monotonic() + 5
        try:
            if info.process:
                if assigned:
                    if "retain_owned_members" in locals():
                        emergency_members.extend(retain_owned_members(job_members(emergency_end)))
                    api.TerminateJobObject(job, 77)
                elif not resumed:
                    api.TerminateProcess(info.process, 77)
                for handle in (info.process, *emergency_members):
                    remaining = max(0, emergency_end - time.monotonic())
                    observed = api.WaitForSingleObject(handle, int(remaining * 1000))
                    if observed != 0:
                        raise ValueError("Emergency owned process exit was not observed; retain all evidence")
        finally:
            if attributes_initialized:
                api.DeleteProcThreadAttributeList(buffer)
            for handle in (*emergency_members, info.thread, info.process, job):
                if handle:
                    api.CloseHandle(handle)


def host_target():
    machine = {"amd64": "x86_64", "arm64": "aarch64"}.get(platform.machine().lower(), platform.machine().lower())
    for target, identity in TARGETS.items():
        if identity == (platform.system(), machine):
            return target
    raise ValueError("Unsupported native static ORT host")


def successful(result):
    return (result["exit_code"] == 0 and not result["timed_out"] and not result["log_oversized"]
            and result["owned_descendants_settled"]
            and not result.get("owned_job_forced_termination", False)
            and "cleanup_refusal" not in result)


def program_path(value, name):
    value = value or shutil.which(name)
    if not value:
        raise ValueError("Missing explicit static ORT prerequisite: " + name)
    path = Path(value).resolve(strict=True)
    if not path.is_file():
        raise ValueError("Invalid static ORT tool: " + name)
    return path


def tool_record(path):
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": digest(path)}


def probe_log(destination, name):
    """Give each read-only probe its own evidence; build stages stay exclusive."""
    if not re.fullmatch(r"[a-z][a-z0-9-]*", name):
        raise ValueError("Invalid tool probe name")
    logs = child(destination, "logs")
    logs.mkdir(parents=True, exist_ok=True)
    attempt = Path(tempfile.mkdtemp(prefix="probe-" + name + "-", dir=logs))
    return child(attempt, "output.log")


def probe_version(path, expected, destination, name):
    log = probe_log(destination, name)
    result = run_owned([path, "--version"], destination, log, 15)
    write_json(log.parent / "result.json", result)
    text = log.read_text(errors="replace")[:4096]
    if not successful(result) or not re.search(r"(?<![0-9])" + re.escape(expected) + r"(?![0-9.])", text):
        raise ValueError("Static ORT requires " + name + " " + expected + "; see " + str(log))
    return {**tool_record(path), "version": expected, "probe": result}


def toolchain(args, destination, target):
    if target != host_target():
        raise ValueError("Static ORT recipe supports native host builds only; no implicit cross compiler")
    operating_system = TARGETS[target][0]
    vs = None
    if operating_system == "Windows":
        candidates = [args.vs_root] if args.vs_root else [
            Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "Microsoft Visual Studio/2022" / edition
            for edition in ("Enterprise", "Professional", "Community", "BuildTools")]
        vs = next((Path(path).resolve() for path in candidates if path and (Path(path) / "VC/Auxiliary/Build/vcvars64.bat").is_file()), None)
        if vs is None:
            raise ValueError("Static ORT requires VS2022 C++ x64 tools; supply --vs-root")
        cmake_default = vs / "Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe"
        ninja_default = vs / "Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja/ninja.exe"
    else:
        cmake_default = ninja_default = None
    cmake = program_path(args.cmake or (cmake_default if cmake_default and cmake_default.is_file() else None), "cmake")
    ninja = program_path(args.ninja or (ninja_default if ninja_default and ninja_default.is_file() else None), "ninja")
    result = {"cmake": probe_version(cmake, "3.31.6", destination, "cmake"),
              "ninja": probe_version(ninja, "1.12.1", destination, "ninja"),
              "python": tool_record(Path(sys.executable).resolve())}
    if sys.version_info < (3, 9):
        raise ValueError("Static ORT provisioning requires Python3.9 or later")
    result["python"]["version"] = platform.python_version()
    if vs:
        vcvars = vs / "VC/Auxiliary/Build/vcvars64.bat"
        compiler = vs / "VC/Tools/MSVC" / args.msvc_toolset / "bin/Hostx64/x64/cl.exe"
        if not compiler.is_file():
            raise ValueError("Pinned MSVC toolset missing: " + args.msvc_toolset)
        patch_default = Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "Git/usr/bin/patch.exe"
        patch = program_path(args.patch or (patch_default if patch_default.is_file() else None), "patch")
        result.update({"vcvars": tool_record(vcvars), "compiler": tool_record(compiler),
                       "patch": tool_record(patch), "msvc_toolset": args.msvc_toolset, "windows_sdk": args.windows_sdk})
    else:
        cc = program_path(args.cc, "cc")
        cxx = program_path(args.cxx, "c++")
        patch = program_path(args.patch, "patch")
        result.update({"compiler": tool_record(cc), "cxx": tool_record(cxx), "patch": tool_record(patch)})
        for name, compiler in (("compiler", cc), ("cxx", cxx)):
            log = probe_log(destination, name)
            observed = run_owned([compiler, "--version"], destination, log, 15)
            if not successful(observed):
                raise ValueError("Static ORT compiler probe failed")
            result[name]["version_output"] = log.read_text(errors="replace")[:4096]
            result[name]["probe"] = observed
        if operating_system == "Darwin":
            log = probe_log(destination, "sdk")
            sdk = run_owned(["/usr/bin/xcrun", "--sdk", "macosx", "--show-sdk-path"], destination, log, 15)
            if not successful(sdk):
                raise ValueError("Missing macOS SDK")
            result["macos_sdk"] = str(Path(log.read_text().strip()).resolve(strict=True))
            result["macos_deployment_target"] = "13.3"
    if getattr(args, "static_tools", None):
        selected = args.static_tools
        result["bootstrap"] = {"path": selected["receipt_path"], "sha256": digest(selected["receipt_path"])}
    return result


def configure_argv(destination, target, tools):
    destination = plain(destination)
    command = [tools["cmake"]["path"], "-S", str(destination / "source/cmake"),
               "-B", str(destination / "b/Release"), "-G", "Ninja", "-DCMAKE_BUILD_TYPE=Release",
               "-DCMAKE_MAKE_PROGRAM=" + tools["ninja"]["path"], "-DFETCHCONTENT_TRY_FIND_PACKAGE_MODE=NEVER",
               "-DPatch_EXECUTABLE=" + tools["patch"]["path"],
               "-DPYTHON_EXECUTABLE=" + tools["python"]["path"],
               "-DPython3_EXECUTABLE=" + tools["python"]["path"]]
    command.extend("-Donnxruntime_" + name + "=OFF" for name in OFF_OPTIONS)
    operating_system, architecture = TARGETS[target]
    if operating_system == "Windows":
        command.extend(["-DCMAKE_SYSTEM_VERSION=" + tools["windows_sdk"], "-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL",
                        "-DONNX_USE_MSVC_STATIC_RUNTIME=OFF", "-Dprotobuf_MSVC_STATIC_RUNTIME=OFF", "-DABSL_MSVC_STATIC_RUNTIME=OFF"])
    else:
        command.extend(["-DCMAKE_C_COMPILER=" + tools["compiler"]["path"], "-DCMAKE_CXX_COMPILER=" + tools["cxx"]["path"]])
        if architecture == "x86_64":
            command.extend(["-DCMAKE_C_FLAGS=-march=x86-64 -mtune=generic", "-DCMAKE_CXX_FLAGS=-march=x86-64 -mtune=generic"])
        if operating_system == "Darwin":
            command.extend(["-DCMAKE_OSX_ARCHITECTURES=" + ("arm64" if architecture == "aarch64" else "x86_64"),
                            "-DCMAKE_OSX_DEPLOYMENT_TARGET=" + tools["macos_deployment_target"], "-DCMAKE_OSX_SYSROOT=" + tools["macos_sdk"]])
    return command


def windows_command(command, tools, destination, stage):
    # These strings enter cmd's grammar only in an owned script. Refuse shell
    # metacharacters rather than treating JSON/list2cmdline as cmd escaping.
    values = [*command, tools["vcvars"]["path"], tools["windows_sdk"], tools["msvc_toolset"], str(destination)]
    if any(re.search(r'[\x00-\x1f"%!?^&|<>]', value) for value in values):
        raise ValueError("Unsafe character in Windows static-build command path")
    script = child(destination, "commands/" + stage + ".cmd")
    script.parent.mkdir(parents=True, exist_ok=True)
    text = ('@echo off\r\ncall "' + tools["vcvars"]["path"] + '" ' + tools["windows_sdk"] +
            ' -vcvars_ver=' + tools["msvc_toolset"] + ' > "' + str(destination / "logs" / (stage + "-toolchain.log")) + '" 2>&1\r\n' +
            'if errorlevel 1 exit /b %errorlevel%\r\n' + subprocess.list2cmdline(command) + '\r\nexit /b %errorlevel%\r\n')
    with script.open("x", newline="") as output:
        output.write(text)
    return [str(Path(os.environ.get("SystemRoot", "C:/Windows")) / "System32/cmd.exe"), "/d", "/c", str(script)]


def zip_notice_content(zipped, info):
    """Materialize a legal-text alias as bytes, never as a filesystem link."""
    origin = relative_name(info.filename)
    seen = set()
    for _hop in range(5):
        if info.filename in seen or info.is_dir():
            raise ValueError("Cyclic/directory legal-text archive reference")
        seen.add(info.filename)
        mode = (info.external_attr >> 16) & 0o170000
        if mode != 0o120000:
            if mode not in (0, 0o100000) or info.file_size > 2 * 1024 * 1024:
                raise ValueError("Legal-text archive target type/size differs")
            with zipped.open(info) as input_file:
                content = input_file.read(2 * 1024 * 1024 + 1)
            if len(content) != info.file_size:
                raise ValueError("Dependency notice expansion differs")
            return content, info.filename if info.filename != origin.as_posix() else None
        if not 0 < info.file_size <= 4096:
            raise ValueError("Legal-text archive reference exceeds budget")
        with zipped.open(info) as input_file:
            target = input_file.read(4097).decode("utf-8", errors="strict")
        if not target or target.startswith("/") or any(char in target for char in ("\\", ":")) or any(ord(char) < 32 for char in target):
            raise ValueError("Unsafe legal-text archive reference")
        target = posixpath.normpath(posixpath.join(posixpath.dirname(info.filename), target))
        target_path = relative_name(target)
        if len(target_path.parts) < 2 or target_path.parts[0] != origin.parts[0]:
            raise ValueError("Legal-text archive reference escapes dependency root")
        matches = [candidate for candidate in zipped.infolist() if candidate.filename == target]
        if len(matches) != 1:
            raise ValueError("Missing/ambiguous legal-text archive reference")
        info = matches[0]
    raise ValueError("Legal-text archive reference depth exceeded")


def notice_items(destination, target, source_root=None, dependency_root=None):
    """Read and authenticate the complete selected full-license/notice roster."""
    items = []
    covered = set()
    def retain(relative, content, **metadata):
        if len(content) > 2 * 1024 * 1024 or len(items) >= 512:
            raise ValueError("Dependency notice content/count bound exceeded")
        row = {"path": relative, "bytes": len(content), "sha256": hashlib.sha256(content).hexdigest(), **metadata}
        items.append((row, content))
    for source_name in ("LICENSE", "ThirdPartyNotices.txt"):
        content = child(source_root or destination / "source", source_name).read_bytes()
        retain("notices/onnxruntime/" + source_name, content)
    for dependency in selected_dependencies(target):
        archive = child(dependency_root, mirror_relative(dependency)) if dependency_root else child(destination, "downloads/" + dependency["name"] + ".zip")
        verify_file(archive, dependency)
        count = 0
        if tarfile.is_tarfile(archive):
            with tarfile.open(archive, "r:*") as tar:
                entries = tar.getmembers()
                if len(entries) > 50000 or sum(entry.size for entry in entries) > 512 * 1024 * 1024:
                    raise ValueError("Dependency legal-text archive expansion limit")
                for info in entries:
                    name = relative_name(info.name.rstrip("/"))
                    if info.isdir() or not re.fullmatch(r"(?i)((LICENSE|COPYING|NOTICE)([._-].*)?|ThirdPartyNotices\.txt)", name.name):
                        continue
                    if not info.isfile() or info.size > 2 * 1024 * 1024 or count >= 128:
                        raise ValueError("Dependency notice size/type bound exceeded")
                    count += 1
                    with tar.extractfile(info) as input_file:
                        content = input_file.read(2 * 1024 * 1024 + 1)
                    if len(content) != info.size:
                        raise ValueError("Dependency notice expansion differs")
                    retain("notices/" + dependency["name"] + "/" + name.as_posix(), content,
                           archive=dependency["name"])
                    if re.match(r"(?i)(LICENSE|COPYING)", name.name) and name.suffix.lower() != ".json":
                        covered.add(dependency["name"])
            continue
        with zipfile.ZipFile(archive) as zipped:
            for info in zipped.infolist():
                name = relative_name(info.filename.rstrip("/"))
                if info.is_dir() or not re.fullmatch(r"(?i)((LICENSE|COPYING|NOTICE)([._-].*)?|ThirdPartyNotices\.txt)", name.name):
                    continue
                if info.file_size > 2 * 1024 * 1024 or count >= 128:
                    raise ValueError("Dependency notice size/type bound exceeded")
                count += 1
                content, link_target = zip_notice_content(zipped, info)
                metadata = {"archive": dependency["name"]}
                if link_target:
                    metadata["archive_link_target"] = link_target
                retain("notices/" + dependency["name"] + "/" + name.as_posix(), content, **metadata)
                if re.match(r"(?i)(LICENSE|COPYING)", name.name) and name.suffix.lower() != ".json":
                    covered.add(dependency["name"])
    for license_row in LOCK["additional_notices"]:
        notice_url(license_row)
        cached = child(destination if dependency_root else destination / "downloads", license_row["cache_name"])
        verify_file(cached, license_row)
        content = cached.read_bytes()
        retain(license_row["path"], content, archive=license_row["dependency"], source_url=license_row["url"],
               source_commit=license_row["source_commit"], license_id=license_row["license_id"])
        covered.add(license_row["dependency"])
    missing = CPU_NAMES - covered
    if missing:
        raise ValueError("Pinned dependencies lack retained full license text: " + ", ".join(sorted(missing)))
    if sum(len(content) for _row, content in items) > 16 * 1024 * 1024:
        raise ValueError("Selected dependency notice aggregate exceeds budget")
    return items


def collect_notices(destination, target, verify_only=False, source_root=None, dependency_root=None):
    """Audit all legal inputs, then resume only exact existing owned bytes."""
    items = notice_items(destination, target, source_root, dependency_root)
    seen = set()
    pending = []
    # Validate the entire existing/expected roster before writing any missing
    # file. A corrupt/foreign partial never gets overwritten or deleted.
    for row, content in items:
        if row["path"] in seen:
            raise ValueError("Duplicate selected dependency notice")
        seen.add(row["path"])
        path = child(destination, row["path"])
        if path.exists():
            verify_file(path, row)
        elif verify_only:
            raise ValueError("Missing retained static dependency notice: " + row["path"])
        else:
            pending.append((path, content))
    for path, content in pending:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("xb") as output:
            output.write(content)
            output.flush()
            os.fsync(output.fileno())
    return [row for row, _content in items]


def verify_cmake_cache(path, command):
    cache = {}
    for line in plain(path, regular=True).read_text(errors="strict").splitlines():
        if not line or line.startswith(("#", "//")) or "=" not in line or ":" not in line.split("=", 1)[0]:
            continue
        key, value = line.split("=", 1)
        cache[key.split(":", 1)[0]] = value
    for flag in command:
        if not flag.startswith("-D"):
            continue
        key, expected = flag[2:].split("=", 1)
        actual = cache.get(key)
        if key.endswith(("_EXECUTABLE", "_PROGRAM", "_COMPILER", "_SYSROOT")) and actual is not None:
            equal = os.path.normcase(os.path.normpath(actual)) == os.path.normcase(os.path.normpath(expected))
        else:
            equal = actual == expected
        if not equal:
            raise ValueError("Actual static ORT CMake cache differs: " + key)
    return cache


def library_inventory(destination, target, build_relative="b/Release", include_supplement=False):
    build = child(destination, build_relative)
    suffix = ".lib" if TARGETS[target][0] == "Windows" else ".a"
    files = [*build.glob("*" + suffix), *(build / "_deps").rglob("*" + suffix)]
    if not 10 <= len(files) <= 512:
        raise ValueError("Static ORT component archive count differs")
    rows = []
    for path in sorted(files):
        plain(path, regular=True)
        size = path.stat().st_size
        if not 0 < size <= 1024 * 1024 * 1024:
            raise ValueError("Static ORT library size bound exceeded")
        with path.open("rb") as archive:
            if archive.read(8) != b"!<arch>\n":
                raise ValueError("Missing genuine static archive header: " + path.name)
        rows.append({"path": path.relative_to(destination).as_posix(), "bytes": size, "sha256": digest(path)})
    for component in STATIC_COMPONENTS:
        name = ("" if suffix == ".lib" else "lib") + "onnxruntime_" + component + suffix
        row = next((row for row in rows if row["path"] == build_relative + "/" + name), None)
        if row is None or (suffix == ".lib" and row["bytes"] < 65536):
            raise ValueError("Missing genuine static component (a small import library is insufficient): " + name)
    if not include_supplement:
        rows = load_script("static-onnx-link-closure.py").original_inventory(destination, target, build_relative, rows, core_api())
    return rows


def build(args, destination, target):
    destination = owned_destination(destination, target)
    if (destination / "build-receipt.json").exists():
        return finish_build(destination, target)
    if (destination / "b").exists():
        raise ValueError("Existing incomplete static build retained; no automatic retry or overwrite")
    verify_source(destination, target)
    tools = toolchain(args, destination, target)
    command = configure_argv(destination, target, tools)
    # Static archive targets do not recursively pull every link dependency.
    # Keep the reviewed original default-all producer and explicitly add re2,
    # which upstream marks EXCLUDE_FROM_ALL but ort-sys rc10 always links.
    build_command = [tools["cmake"]["path"], "--build", str(destination / "b/Release"), "--config", "Release", "--target", "all", "re2", "--parallel", "2"]
    receipt = {"schema": 1, "result": "running", "ort_version": VERSION, "upstream_commit": COMMIT,
               "target": target, "lock_sha256": digest(LOCK_PATH), "toolchain": tools,
               "configure_argv": command, "build_argv": build_command,
               "source_inventory_sha256": digest(destination / "source-files.json"), "commands": [],
               "qualification": {"final_static_link": False, "actual_model": False, "cross_platform": False}}
    running = child(destination, "build-attempt.json")
    write_json(running, receipt)
    environment = dict(os.environ)
    for key in list(environment):
        if key in {"CC", "CXX", "CFLAGS", "CXXFLAGS", "CPPFLAGS", "LDFLAGS", "PYTHONPATH", "PYTHONHOME"} or key.startswith("CMAKE_"):
            environment.pop(key)
    environment["PYTHONNOUSERSITE"] = "1"
    environment["PATH"] = str(Path(sys.executable).resolve().parent) + os.pathsep + environment.get("PATH", "")
    if TARGETS[target][0] == "Darwin":
        environment["MACOSX_DEPLOYMENT_TARGET"] = "13.3"
    started = time.monotonic()
    try:
        for stage, argv, timeout in (("configure", command, 600), ("build", build_command, 10800)):
            actual = windows_command(argv, tools, destination, stage) if os.name == "nt" else argv
            result = run_owned(actual, destination, child(destination, "logs/" + stage + ".log"), timeout, environment)
            result["stage"] = stage
            receipt["commands"].append(result)
            write_json(running, receipt, replace=True)
            if not successful(result):
                raise ValueError("Static ORT " + stage + " failed; complete attempt/logs retained")
        receipt["libraries"] = library_inventory(destination, target)
        verify_cmake_cache(destination / "b/Release/CMakeCache.txt", command)
        receipt["notices"] = collect_notices(destination, target)
        receipt["result"] = "libraries_built_not_yet_model_or_static_link_qualified"
        receipt["elapsed_seconds"] = round(time.monotonic() - started, 3)
        write_json(running, receipt, replace=True)
        write_json(destination / "build-receipt.json", receipt)
    except Exception as error:
        receipt["result"] = "failed"
        receipt["error"] = str(error)
        receipt["elapsed_seconds"] = round(time.monotonic() - started, 3)
        write_json(running, receipt, replace=True)
        raise
    return finish_build(destination, target)


def finish_build(destination, target):
    if VERSION == "1.30.0":
        # This owner verifies the original completed C++ receipt before any
        # additive archive work and refuses incomplete aggregation attempts.
        return load_script("static-onnx-aggregate.py").aggregate(destination, target, core_api())
    return verify_build(destination, target)


def verify_build(destination, target):
    result = verify_original_build(destination, target)
    if (destination / "static-aggregate-receipt.json").exists():
        return load_script("static-onnx-aggregate.py").verify(destination, target, result, core_api())
    if VERSION == "1.30.0":
        raise ValueError("Completed ORT1.30 component archives require explicit --aggregate-static before read-only --verify")
    return load_script("static-onnx-link-closure.py").consumer(destination, target, result, core_api())


def verify_original_build(destination, target):
    """Machine-verifiable Cargo handoff; this is not final executable qualification."""
    validate_lock()
    destination = owned_destination(destination, target)
    receipt = read_json(destination / "build-receipt.json")
    if receipt.get("build_mode") == "adopted_reviewed_neo_arm64":
        return load_script("adopt-neo-static-onnx-runtime.py").verify(destination, target, receipt, core_api())
    if receipt.get("build_mode") == "adopted_reviewed_windows_c1":
        return verify_legacy_build(destination, target, receipt)
    if (receipt.get("schema") != 1 or receipt.get("result") != "libraries_built_not_yet_model_or_static_link_qualified"
            or receipt.get("ort_version") != VERSION or receipt.get("upstream_commit") != COMMIT
            or receipt.get("target") != target or receipt.get("lock_sha256") != digest(LOCK_PATH)):
        raise ValueError("Static ORT build receipt identity/result differs")
    fetch(destination / "downloads/source.tar.gz", LOCK["source"], source=True)
    for row in selected_dependencies(target):
        verify_file(child(destination, "downloads/" + row["name"] + ".zip"), row)
    verify_source(destination, target)
    if receipt["source_inventory_sha256"] != digest(destination / "source-files.json"):
        raise ValueError("Static ORT source inventory receipt changed")
    tools = receipt["toolchain"]
    if "bootstrap" in tools:
        selected = load_script("provision-static-onnx-tools.py").verify_tools(destination, target)
        if tools["bootstrap"] != {"path": selected["receipt_path"], "sha256": digest(selected["receipt_path"])} or tools["cmake"]["path"] != selected["cmake"] or tools["ninja"]["path"] != selected["ninja"]:
            raise ValueError("Static ORT prepared tool identity changed")
    for value in tools.values():
        if isinstance(value, dict) and "path" in value and "bytes" in value:
            verify_file(value["path"], value, system_tool=True)
    if tools["cmake"]["version"] != "3.31.6" or tools["ninja"]["version"] != "1.12.1":
        raise ValueError("Static ORT build tool version differs")
    if receipt["configure_argv"] != configure_argv(destination, target, tools):
        raise ValueError("Static ORT configure policy changed")
    verify_cmake_cache(destination / "b/Release/CMakeCache.txt", receipt["configure_argv"])
    expected_build = [tools["cmake"]["path"], "--build", str(destination / "b/Release"), "--config", "Release", "--target", "all", "re2", "--parallel", "2"]
    if receipt["build_argv"] != expected_build or [row["stage"] for row in receipt["commands"]] != ["configure", "build"] or not all(successful(row) for row in receipt["commands"]):
        raise ValueError("Static ORT actual build stages did not pass/settle")
    if receipt["libraries"] != library_inventory(destination, target):
        raise ValueError("Static ORT library content/roster changed")
    if receipt["notices"] != collect_notices(destination, target, verify_only=True):
        raise ValueError("Static ORT selected license content/roster changed")
    return {"receipt": receipt, "environment": {"LIBONNXRUNTIME_NO_PKG_CONFIG": "1", "ORT_SKIP_DOWNLOAD": "1",
            "ORT_PREFER_DYNAMIC_LINK": "0", "ORT_LIB_LOCATION": str(destination / "b"), "ORT_LIB_PROFILE": "Release"}}


def legacy_inputs(destination):
    """Recognize exactly one reviewed C1 campaign, never arbitrary supplied logs."""
    for name, expected in LEGACY_C1.items():
        if digest(plain(destination / name, regular=True)) != expected:
            raise ValueError("Unrecognized or changed original C1 evidence: " + name)
    original = read_json(destination / "windows-static-build-result.json")
    if (original["upstream_revision"] != COMMIT or original["ort_version"] != VERSION
            or original["result"] != "libraries_built_not_yet_model_or_static_link_qualified"
            or len(original["libraries"]) != 82
            or [row["stage"] for row in original["commands"]] != ["configure", "build"]
            or any(row["exit_code"] != 0 for row in original["commands"])):
        raise ValueError("Original C1 source/configure/build evidence is incomplete")
    for row in LOCK["dependencies"]:
        verify_file(child(destination / "onnxruntime-source/mirror", mirror_relative(row)), row)
    if {row["path"] for row in source_inventory(destination / "onnxruntime-source/mirror")} != {mirror_relative(row) for row in LOCK["dependencies"]}:
        raise ValueError("Original C1 dependency mirror has unexpected inputs")
    if plain(destination / "onnxruntime-source/.git/HEAD", regular=True).read_text().strip() != COMMIT:
        raise ValueError("Original C1 Git checkout revision differs")
    for row in original["libraries"]:
        verify_file(child(destination / "b/Release", row["path"].replace("\\", "/")), row)
    if len(library_inventory(destination, "x86_64-pc-windows-msvc")) != 82:
        raise ValueError("Original C1 library roster changed")
    cache = verify_cmake_cache(destination / "b/Release/CMakeCache.txt", original["configure"] +
                               ["-Donnxruntime_USE_COREML=OFF", "-Donnxruntime_USE_KLEIDIAI=OFF"])
    if cache.get("CMAKE_MSVC_RUNTIME_LIBRARY") != "MultiThreadedDLL" or cache.get("CMAKE_SYSTEM_VERSION") != "10.0.26100.0":
        raise ValueError("Original C1 dynamic CRT/SDK policy differs")
    compiler_metadata = child(destination, "b/Release/CMakeFiles/3.31.6-msvc6/CMakeCXXCompiler.cmake")
    text = plain(compiler_metadata, regular=True).read_text()
    if 'set(CMAKE_CXX_COMPILER_VERSION "19.44.35217.0")' not in text or "MSVC/14.44.35207/bin/Hostx64/x64/cl.exe" not in text:
        raise ValueError("Original C1 compiler version evidence differs")
    dependencies = read_json(destination / "cpu-dependencies.json")
    expected = {row["name"]: row for row in LOCK["dependencies"]}
    if len(dependencies["dependencies"]) != len(expected):
        raise ValueError("Original C1 dependency roster differs")
    for row in dependencies["dependencies"]:
        if any(row[key] != expected[row["name"]][key] for key in ("url", "bytes", "sha256", "upstream_sha1")):
            raise ValueError("Original C1 dependency seal differs")
    return original, cache


def legacy_source_inventory(destination):
    archive = plain(destination / "onnxruntime-1.22.1-source.tar.gz", regular=True)
    verify_file(archive, LOCK["source"])
    source = plain(destination / "onnxruntime-source")
    # The source archive is sealed. Check every compiled working-tree byte,
    # explicitly preserving Git's Windows LF-to-CRLF representation when it
    # is the only difference in a UTF-8 text file. Never normalize binaries.
    raw_rows = []
    upstream_rows = []
    with tarfile.open(archive, "r:gz") as tar:
        for member in tar:
            if not member.isfile():
                continue
            name = relative_name(member.name)
            if name.parts[0] != LOCK["source"]["archive_root"] or len(name.parts) < 2 or member.size > 64 * 1024 * 1024:
                raise ValueError("Original source archive member differs")
            relative = PurePosixPath(*name.parts[1:]).as_posix()
            with tar.extractfile(member) as input_file:
                upstream = input_file.read(64 * 1024 * 1024 + 1)
            compiled_path = plain(child(source, relative), regular=True)
            if compiled_path.stat().st_size > 128 * 1024 * 1024:
                raise ValueError("Original compiled source file exceeds bound")
            compiled = compiled_path.read_bytes()
            normalized = checkout_representation(upstream, compiled, relative)
            upstream_hash = hashlib.sha256(upstream).hexdigest()
            upstream_rows.append({"path": relative, "bytes": len(upstream), "sha256": upstream_hash})
            raw_rows.append({"path": relative, "bytes": len(compiled), "sha256": hashlib.sha256(compiled).hexdigest(),
                             "upstream_sha256": upstream_hash, "checkout_eol": normalized})
    upstream_rows.sort(key=lambda row: row["path"])
    upstream_seal = hashlib.sha256((json.dumps(upstream_rows, indent=2, sort_keys=True) + "\n").encode()).hexdigest()
    if upstream_seal != LOCK["source"]["inventory_sha256"]:
        raise ValueError("Original compiled source is not the complete sealed upstream tree")
    actual = set()
    for path in source.rglob("*"):
        relative = path.relative_to(source).as_posix()
        if relative == ".git" or relative.startswith((".git/", "mirror/")) or relative == "mirror":
            continue
        plain(path)
        if path.is_file():
            actual.add(relative)
    if actual != {row["path"] for row in raw_rows}:
        raise ValueError("Unrecognized extra/missing original compiled source input")
    return sorted(raw_rows, key=lambda row: row["path"])


def checkout_representation(upstream, compiled, relative):
    if compiled == upstream:
        return "none"
    try:
        upstream.decode("utf-8", errors="strict")
    except UnicodeDecodeError as error:
        raise ValueError("Original compiled binary source differs: " + relative) from error
    if b"\0" in upstream or b"\r" in upstream or compiled != upstream.replace(b"\n", b"\r\n"):
        raise ValueError("Original compiled source has changes beyond Git text EOLs: " + relative)
    return "git_lf_to_crlf"


def legacy_environment(destination):
    return {"LIBONNXRUNTIME_NO_PKG_CONFIG": "1", "ORT_SKIP_DOWNLOAD": "1", "ORT_PREFER_DYNAMIC_LINK": "0",
            "ORT_LIB_LOCATION": str(destination / "b"), "ORT_LIB_PROFILE": "Release"}


def adopt_legacy(destination, target):
    """Import exactly the completed C1 build; never relabel it as a fresh build."""
    validate_lock()
    if os.name != "nt" or target != "x86_64-pc-windows-msvc":
        raise ValueError("The reviewed legacy static campaign is Windows x64 only")
    destination = plain(destination)
    if (destination / "owner.json").exists():
        return verify_build(destination, target)
    original, cache = legacy_inputs(destination)
    raw_rows = legacy_source_inventory(destination)
    # All expensive validation precedes mutation of the old campaign directory.
    adoption = Path(tempfile.mkdtemp(prefix="static-adoption-", dir=destination))
    write_json(adoption / "compiled-source-files.json", raw_rows)
    tools = {"cmake": probe_version(Path(original["configure"][0]), "3.31.6", adoption, "cmake"),
             "ninja": probe_version(Path(cache["CMAKE_MAKE_PROGRAM"]), "1.12.1", adoption, "ninja"),
             "compiler": tool_record(Path(cache["CMAKE_CXX_COMPILER"])), "python": tool_record(Path(cache["PYTHON_EXECUTABLE"]))}
    # Preserve original source/build command records and the anomalous parent-
    # in-job cleanup exactly. Their runner exit is not reported as a new pass.
    receipt = {"schema": 1, "build_mode": "adopted_reviewed_windows_c1", "result": original["result"],
               "ort_version": VERSION, "upstream_commit": COMMIT, "target": target, "lock_sha256": digest(LOCK_PATH),
               "original_build_receipt_sha256": LEGACY_C1["windows-static-build-result.json"],
               "original_build": original, "original_compiler_version": "19.44.35217.0",
               "original_runner_postflight_exit_code": 1, "original_cleanup_forced_job_termination": True,
               "adopted_unix_seconds": time.time(), "adoption_directory": adoption.relative_to(destination).as_posix(),
               "compiled_source_inventory_sha256": digest(adoption / "compiled-source-files.json"),
               "adoption_tool_observations": tools, "libraries": library_inventory(destination, target),
               "notices": collect_notices(destination, target, source_root=destination / "onnxruntime-source", dependency_root=destination / "onnxruntime-source/mirror"),
               "qualification": {"fresh_build": False, "final_static_link": False, "actual_model": False, "cross_platform": False}}
    write_json(destination / "owner.json", owner_value(target))
    write_json(destination / "build-receipt.json", receipt)
    return verify_legacy_build(destination, target, receipt)


def verify_legacy_build(destination, target, receipt):
    if (target != "x86_64-pc-windows-msvc" or receipt.get("target") != target or receipt.get("lock_sha256") != digest(LOCK_PATH)
            or receipt.get("schema") != 1 or receipt.get("ort_version") != VERSION or receipt.get("upstream_commit") != COMMIT
            or receipt.get("result") != "libraries_built_not_yet_model_or_static_link_qualified"
            or receipt.get("original_compiler_version") != "19.44.35217.0"):
        raise ValueError("Adopted static ORT target/input lock differs")
    original, _cache = legacy_inputs(destination)
    if receipt.get("original_build") != original or receipt.get("original_build_receipt_sha256") != LEGACY_C1["windows-static-build-result.json"]:
        raise ValueError("Adopted static ORT original identity changed")
    if receipt.get("original_runner_postflight_exit_code") != 1 or receipt.get("original_cleanup_forced_job_termination") is not True or receipt.get("qualification") != {"fresh_build": False, "final_static_link": False, "actual_model": False, "cross_platform": False}:
        raise ValueError("Adopted static ORT overstates original validation")
    inventory = child(destination, receipt["adoption_directory"] + "/compiled-source-files.json")
    if digest(plain(inventory, regular=True)) != receipt["compiled_source_inventory_sha256"] or read_json(inventory) != legacy_source_inventory(destination):
        raise ValueError("Original compiled static ORT source bytes changed")
    if receipt.get("libraries") != library_inventory(destination, target):
        raise ValueError("Original compiled static ORT archives changed")
    for tool in receipt["adoption_tool_observations"].values():
        verify_file(tool["path"], tool, system_tool=True)
    notices = collect_notices(destination, target, verify_only=True, source_root=destination / "onnxruntime-source", dependency_root=destination / "onnxruntime-source/mirror")
    if receipt["notices"] != notices:
        raise ValueError("Adopted static ORT licenses changed")
    return {"receipt": receipt, "environment": legacy_environment(destination)}


def github_environment(result):
    if os.environ.get("GITHUB_ACTIONS") != "true" or not os.environ.get("RUNNER_TEMP") or not os.environ.get("GITHUB_ENV"):
        raise ValueError("--github-env requires the explicit GitHub runner environment")
    root = plain(os.environ["RUNNER_TEMP"])
    path = plain(os.environ["GITHUB_ENV"], regular=True)
    if not path.is_relative_to(root) or path.stat().st_size > 1024 * 1024:
        raise ValueError("GITHUB_ENV is not an owned bounded runner command file")
    approved = {"LIBONNXRUNTIME_NO_PKG_CONFIG", "ORT_SKIP_DOWNLOAD", "ORT_PREFER_DYNAMIC_LINK", "ORT_LIB_LOCATION", "ORT_LIB_PROFILE"}
    environment = result["environment"]
    if set(environment) != approved or any(not isinstance(value, str) or any(ord(char) < 32 for char in value) for value in environment.values()):
        raise ValueError("Invalid static ORT GitHub environment export")
    with path.open("a", encoding="utf-8", newline="\n") as output:
        output.write("\n" + "".join(key + "=" + value + "\n" for key, value in sorted(environment.items())))


def load_script(name):
    spec = importlib.util.spec_from_file_location("airk_" + name.replace("-", "_").replace(".", "_"), ROOT / "scripts" / name)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def core_api():
    # Artifact adapters receive the actual checked-in validator functions and
    # lock, not serialized authority flags or a reduced parallel contract.
    return types.SimpleNamespace(**globals())


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS, default=None)
    parser.add_argument("--destination", type=Path)
    parser.add_argument("--download", action="store_true")
    parser.add_argument("--download-tools", action="store_true", help="Prepare pinned build-only CMake and compile pinned Ninja in the owned destination")
    parser.add_argument("--build-cmake-source", action="store_true", help="Mac arm64 only: use pinned official CMake3.31.6 source provenance")
    parser.add_argument("--build", action="store_true")
    parser.add_argument("--aggregate-static", action="store_true", help="Package genuine completed component archives into rc10's monolithic static layout; never rebuild sources")
    parser.add_argument("--supplement-link-closure", action="store_true", help="Build only the real missing pinned re2 target; preserve original receipts and archives")
    parser.add_argument("--recover-link-supplement", action="store_true", help="Adopt only the exact reviewed Neo supplemental archive after its owned runner failed; never build")
    parser.add_argument("--verify", action="store_true")
    parser.add_argument("--parallel", type=int, choices=[2], default=2)
    parser.add_argument("--environment-file", type=Path)
    parser.add_argument("--github-env", action="store_true")
    parser.add_argument("--adopt-legacy-receipt", action="store_true", help="Import only the exact reviewed Windows C1 campaign; never rebuild it")
    parser.add_argument("--adopt-neo-receipt", type=Path, help="Import only the exact reviewed completed Neo ARM64 campaign; never rebuild it")
    for name in ("cmake", "ninja", "patch", "cc", "cxx", "vs-root"):
        parser.add_argument("--" + name, type=Path)
    parser.add_argument("--windows-sdk", default="10.0.26100.0", choices=["10.0.26100.0"])
    parser.add_argument("--msvc-toolset", default="14.44.35207", choices=["14.44.35207"])
    args = parser.parse_args(argv)
    target = args.target or host_target()
    destination = args.destination or ROOT / "target/static-onnx-runtime" / target
    if args.aggregate_static and (args.download or args.download_tools or args.build or args.verify or args.supplement_link_closure or args.recover_link_supplement or args.adopt_legacy_receipt or args.adopt_neo_receipt):
        parser.error("Static aggregation never downloads, configures, rebuilds or adopts old inputs")
    if args.recover_link_supplement and (args.supplement_link_closure or args.verify or args.download or args.build or args.adopt_legacy_receipt or args.adopt_neo_receipt or args.download_tools):
        parser.error("Reviewed recovery never downloads, configures or builds")
    if args.supplement_link_closure and (args.verify or args.download or args.build or args.adopt_legacy_receipt or args.adopt_neo_receipt or args.download_tools):
        parser.error("Supplemental closure build never configures, downloads or rebuilds the original campaign")
    if args.verify and (args.download or args.build or args.adopt_legacy_receipt or args.adopt_neo_receipt or args.download_tools):
        parser.error("--verify is read-only and cannot download/build")
    if args.build_cmake_source and (not args.download_tools or args.verify):
        parser.error("--build-cmake-source requires explicit --download-tools preparation")
    if args.aggregate_static:
        result = load_script("static-onnx-aggregate.py").aggregate(destination, target, core_api())
    elif args.recover_link_supplement:
        result = load_script("static-onnx-link-closure.py").recover(destination, target, core_api())
    elif args.supplement_link_closure:
        result = load_script("static-onnx-link-closure.py").supplement(destination, target, core_api())
    elif args.adopt_legacy_receipt or args.adopt_neo_receipt:
        if args.download or args.build or args.download_tools or (args.adopt_legacy_receipt and args.adopt_neo_receipt):
            parser.error("Legacy adoption never downloads or builds")
        if args.adopt_neo_receipt:
            result = load_script("adopt-neo-static-onnx-runtime.py").adopt(destination, target, args.adopt_neo_receipt, core_api())
        else:
            result = adopt_legacy(destination, target)
    elif args.verify:
        result = verify_build(destination, target)
    else:
        destination = prepare(destination, target, args.download)
        if args.download_tools:
            if args.cmake or args.ninja:
                parser.error("--download-tools selects its own pinned CMake/Ninja; omit external tool overrides")
            selected = load_script("provision-static-onnx-tools.py").provision_tools(destination, target, True, args)
            args.cmake, args.ninja = Path(selected["cmake"]), Path(selected["ninja"])
            args.static_tools = selected
        result = build(args, destination, target) if args.build else {"prepared": True, "target": target, "directory": str(destination)}
    if args.environment_file or args.github_env:
        if "environment" not in result:
            raise ValueError("Environment exports require a verified completed static library build")
        if args.environment_file:
            write_json(args.environment_file, result)
        if args.github_env:
            github_environment(result)
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, OSError, KeyError, tarfile.TarError, zipfile.BadZipFile) as error:
        print("Static ORT provisioning failed: " + str(error), file=sys.stderr)
        raise SystemExit(1)
