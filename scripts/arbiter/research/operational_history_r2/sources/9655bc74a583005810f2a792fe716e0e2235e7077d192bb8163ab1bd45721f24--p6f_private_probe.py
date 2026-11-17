# SPDX-License-Identifier: Apache-2.0
"""Ignored native CONTROL/worker adapter. Bearers never leave child memory.

Uses the same protected native endpoints and framing as the real project client,
not the public metadata Console/CLI, which must continue rejecting credentials.
All failure diagnostics are static stage/class values; no exception text or
credential-bearing native response is serialized.
"""
import ctypes
import errno
import json
import os
import re
import socket
import struct
import sys
import time

MAX_FRAME = 4 * 1024 * 1024


class FrameInvalid(Exception):
    pass


class PeerRejected(Exception):
    pass


def process_sid(pid=None):
    from ctypes import wintypes
    advapi, kernel = ctypes.WinDLL("advapi32", use_last_error=True), ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel.LocalFree.argtypes = [ctypes.c_void_p]
    advapi.OpenProcessToken.argtypes = [wintypes.HANDLE, wintypes.DWORD, ctypes.POINTER(wintypes.HANDLE)]
    advapi.GetTokenInformation.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD)]
    advapi.ConvertSidToStringSidW.argtypes = [ctypes.c_void_p, ctypes.POINTER(wintypes.LPWSTR)]
    process = kernel.GetCurrentProcess() if pid is None else kernel.OpenProcess(0x1000, False, pid)
    if not process:
        raise PeerRejected()
    token, needed = wintypes.HANDLE(), wintypes.DWORD()
    try:
        if not advapi.OpenProcessToken(process, 8, ctypes.byref(token)):
            raise PeerRejected()
        try:
            advapi.GetTokenInformation(token, 1, None, 0, ctypes.byref(needed))
            if not 0 < needed.value <= 65536:
                raise PeerRejected()
            data = ctypes.create_string_buffer(needed.value)
            if not advapi.GetTokenInformation(token, 1, data, needed, ctypes.byref(needed)):
                raise PeerRejected()
            sid = ctypes.cast(data, ctypes.POINTER(ctypes.c_void_p))[0]
            text = wintypes.LPWSTR()
            if not advapi.ConvertSidToStringSidW(sid, ctypes.byref(text)):
                raise PeerRejected()
            try:
                return text.value
            finally:
                kernel.LocalFree(ctypes.cast(text, ctypes.c_void_p))
        finally:
            kernel.CloseHandle(token)
    finally:
        if pid is not None:
            kernel.CloseHandle(process)


def require_peer(actual_user, native_user, actual_pid=None, expected_pid=None):
    if actual_user != native_user or expected_pid is not None and actual_pid is not None and actual_pid != expected_pid:
        raise PeerRejected()
    if actual_pid is not None and actual_pid <= 1:
        raise PeerRejected()
    return {"same_native_user": True, "peer_pid": actual_pid,
            "expected_pid_verified": expected_pid is not None and actual_pid is not None}


def authenticate_peer(connection, expected_pid=None):
    if os.name == "nt":
        import msvcrt
        from ctypes import wintypes
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.GetNamedPipeServerProcessId.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.ULONG)]
        pid = wintypes.ULONG()
        if not kernel.GetNamedPipeServerProcessId(wintypes.HANDLE(msvcrt.get_osfhandle(connection.fileno())), ctypes.byref(pid)):
            raise PeerRejected()
        return require_peer(process_sid(pid.value), process_sid(), pid.value, expected_pid)
    if hasattr(socket, "SO_PEERCRED"):
        pid, user, _group = struct.unpack("iII", connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize("iII")))
        return require_peer(str(user), str(os.geteuid()), pid, expected_pid)
    # Native BSD/macOS peer credentials, not a sender-supplied UID.
    libc = ctypes.CDLL(None, use_errno=True)
    user, group = ctypes.c_uint(), ctypes.c_uint()
    libc.getpeereid.argtypes = [ctypes.c_int, ctypes.POINTER(ctypes.c_uint), ctypes.POINTER(ctypes.c_uint)]
    if libc.getpeereid(connection.fileno(), ctypes.byref(user), ctypes.byref(group)) != 0:
        raise PeerRejected()
    pid = None
    if sys.platform == "darwin":
        # Darwin SOL_LOCAL/LOCAL_PEERPID; only genuinely unavailable native
        # support may omit this additional check. Health still pins the owner.
        try:
            pid = struct.unpack("i", connection.getsockopt(0, 2, struct.calcsize("i")))[0]
        except OSError as error:
            if error.errno not in (errno.ENOPROTOOPT, errno.EINVAL, errno.ENOTSUP):
                raise
    return require_peer(str(user.value), str(os.geteuid()), pid, expected_pid)


class NativeStream:
    def __init__(self, endpoint, expected_pid=None):
        self.connection = None
        try:
            if os.name == "nt":
                self.connection = open(endpoint, "r+b", buffering=0)
            else:
                self.connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                self.connection.settimeout(10)
                self.connection.connect(endpoint)
            # Always authenticate before writing a grant OR deserializing data.
            self.peer = authenticate_peer(self.connection, expected_pid)
        except BaseException:
            self.close()
            raise

    def close(self):
        if self.connection is not None:
            self.connection.close()
            self.connection = None

    def write(self, value):
        raw = json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        if not 0 < len(raw) <= 65536:
            raise FrameInvalid()
        packet = struct.pack(">I", len(raw)) + raw
        if os.name == "nt":
            view = memoryview(packet)
            while view:
                count = self.connection.write(view)
                if not count:
                    raise FrameInvalid()
                view = view[count:]
        else:
            self.connection.sendall(packet)

    def exact(self, length, allow_eof=False):
        result = bytearray()
        while len(result) < length:
            try:
                chunk = self.connection.read(length - len(result)) if os.name == "nt" else self.connection.recv(length - len(result))
            except OSError as error:
                # Only native read-side pipe closure is EOF. Timeouts, peer
                # failures, write errors and invalid frames never become proof.
                if os.name == "nt" and getattr(error, "winerror", None) in (109, 232, 233):
                    chunk = b""
                else:
                    raise
            if not chunk:
                if allow_eof and not result:
                    return None
                raise FrameInvalid()
            result.extend(chunk)
        return bytes(result)

    def read(self, maximum=65536):
        header = self.exact(4, allow_eof=True)
        if header is None:
            return None
        length = struct.unpack(">I", header)[0]
        if not 0 < length <= maximum:
            raise FrameInvalid()
        value = json.loads(self.exact(length))
        if not isinstance(value, dict):
            raise FrameInvalid()
        return value


def safe_result(value, bearers, accepted, stage):
    def contains_bearer(item):
        if isinstance(item, str):
            return any(bearer and bearer in item for bearer in bearers)
        if isinstance(item, dict):
            return any(contains_bearer(key) or contains_bearer(part) for key, part in item.items())
        if isinstance(item, list):
            return any(contains_bearer(part) for part in item)
        return False
    if contains_bearer(value):
        return {"probe_failed": True, "stage": "output", "failure_class": "CredentialOutputRejected",
                "accepted": accepted, "replies": [], "closed": False}
    raw = json.dumps(value, ensure_ascii=False, separators=(",", ":"))
    if any(bearer and bearer in raw for bearer in bearers):
        return {"probe_failed": True, "stage": "output", "failure_class": "CredentialOutputRejected",
                "accepted": accepted, "replies": [], "closed": False}
    return value


def exchange(payload, endpoint):
    """Runs ONLY in the parent's bounded private probe subprocess."""
    stage, accepted, replies, bearers, streams = "control_health", False, [], [], []
    ttl = None
    try:
        expected = payload["expected_health"]
        selection = payload["selection"]
        native_user = process_sid() if os.name == "nt" else str(os.geteuid())
        control = NativeStream(endpoint(payload["control"], "control"), expected["process_id"])
        streams.append(control)
        control.write({"protocol": 1, "expected_instance": expected["instance"], "method": "health"})
        health = control.read()
        assert health is not None and health["status"] == "health"
        actual = health["observation"]
        for field in ("protocol", "version", "instance", "process_id", "executable_sha256"):
            assert actual[field] == expected[field]
        assert type(actual["protocol"]) is int and actual["protocol"] == 1 and actual["user"] == native_user
        assert type(actual["process_id"]) is int
        assert actual["catalog_clock"]["namespace"] == selection["namespace"]
        control_peer = control.peer
        control.close()
        stage = "control_scoped_grant"
        broker = NativeStream(endpoint(payload["control"], "control"), expected["process_id"])
        streams.append(broker)
        broker.write({"protocol": 1, "expected_instance": actual["instance"], "method": {"connect_scoped": {"request": selection}}})
        issued = broker.read()
        assert issued is not None and issued["status"] == "connection"
        grant = issued["grant"]
        assert set(grant) == {"protocol", "identity", "caller", "token", "ttl_ms", "client", "scope"}
        token = grant["token"]
        assert isinstance(token, str) and re.fullmatch(r"[0-9a-f]{64}", token)
        bearers.append(token)
        identity, scope = grant["identity"], grant["scope"]
        ttl = grant["ttl_ms"]
        assert type(grant["protocol"]) is int and grant["protocol"] == 1 and type(ttl) is int and 1 <= ttl <= 30000
        assert re.fullmatch(r"[0-9a-f]{32}", grant["caller"]) and isinstance(grant["client"], str)
        assert set(identity) == {"service", "worker", "project", "fencing_generation"}
        assert identity["service"] == actual["instance"] and identity["project"] == payload["expected_backing"]
        assert re.fullmatch(r"[0-9a-f]{32}", identity["worker"]) and type(identity["fencing_generation"]) is int and identity["fencing_generation"] > 0
        assert set(scope) == {"schema", "lease_generation", "index_id", "grant_id", "grant_revision", "fingerprint"}
        assert type(scope["schema"]) is int and scope["schema"] == 1
        assert type(scope["lease_generation"]) is int and scope["lease_generation"] > 0
        assert type(scope["grant_revision"]) is int
        assert scope["index_id"] == selection["index_id"] and scope["grant_id"] == selection["grant_id"]
        assert scope["grant_revision"] == selection["expected_grant_revision"] and re.fullmatch(r"[0-9a-f]{64}", scope["fingerprint"])
        assert issued["role"] == "q-" + identity["worker"]
        broker.close()
        if payload.get("expire"):
            # No worker socket is opened until after the genuine issued TTL;
            # the worker's initial-frame timeout cannot stand in for expiry.
            time.sleep((ttl + 500) / 1000)
        stage = "worker_peer"
        worker = NativeStream(endpoint(payload["control"], issued["role"]))
        streams.append(worker)
        if payload.get("forge_purpose"):
            grant["purpose"] = "native_administrative_read"
        stage = "worker_ack"
        worker.write(grant)
        ack = worker.read()
        if ack is None:
            if not (payload.get("expire") or payload.get("forge_purpose")):
                raise FrameInvalid()
            return safe_result({"probe_failed": False, "accepted": False, "replies": [], "closed": True,
                                "closure_stage": "worker_ack", "ttl_ms": ttl, "peer_checks": {"control": control_peer, "worker": worker.peer}}, bearers, accepted, stage)
        assert set(ack) == {"identity"} and ack["identity"] == identity
        assert type(ack["identity"]["fencing_generation"]) is int
        accepted = True
        assert not payload.get("expire") and not payload.get("forge_purpose"), "unexpected_ack"
        stage = "worker_query"
        for query in payload["queries"]:
            worker.write(query)
            response = worker.read(MAX_FRAME)
            if response is None:
                return safe_result({"probe_failed": False, "accepted": accepted, "replies": replies, "closed": True,
                                    "closure_stage": "worker_query", "ttl_ms": ttl}, bearers, accepted, stage)
            replies.append(response)
        return safe_result({"probe_failed": False, "accepted": accepted, "replies": replies, "closed": False,
                            "ttl_ms": ttl, "peer_checks": {"control": control_peer, "worker": worker.peer}}, bearers, accepted, stage)
    except BaseException as error:
        # No exception text, native response or credential ever leaves this lane.
        return safe_result({"probe_failed": True, "stage": stage, "failure_class": type(error).__name__,
                            "accepted": accepted, "replies": [], "closed": False, "ttl_ms": ttl}, bearers, accepted, stage)
    finally:
        close_failed = False
        for stream in streams:
            try:
                stream.close()
            except BaseException:
                close_failed = True
        if close_failed:
            return safe_result({"probe_failed": True, "stage": "cleanup", "failure_class": "NativeCloseFailed",
                                "accepted": accepted, "replies": replies, "closed": False, "ttl_ms": ttl}, bearers, accepted, "cleanup")
