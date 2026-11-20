# SPDX-License-Identifier: Apache-2.0
"""Anonymous immutable Hub verification and explicit Arbiter release publication."""

from contextlib import ExitStack, contextmanager
import hashlib
from http.client import HTTPException
import logging
import os
from pathlib import Path
import re
import stat
import sys
import time
from urllib.error import HTTPError
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener


REPO = "iokaio/airk-arbiter-models"
ORIGIN = "https://huggingface.co"
HUB_HOST = "huggingface.co"
CDN_HOST = "us.aws.cdn.hf.co"
CARD = "README.md"
MAX_PART_BYTES = 1 << 30
MAX_CARD_BYTES = 1 << 20
CHUNK = 1 << 20
MAX_REDIRECTS = 4
REDIRECT_CODES = {301, 302, 303, 307, 308}
IO_TIMEOUT = 30
MIN_TRANSFER_RATE = 256 * 1024


class HubReleaseError(ValueError):
    """A local validation or sanitized Hub transport failure."""


def _require(condition, message):
    if not condition:
        raise HubReleaseError(message)


def _check_repo(repo):
    _require(repo == REPO, "only iokaio/airk-arbiter-models is authorized")


def _check_commit(commit):
    _require(isinstance(commit, str) and re.fullmatch(r"[0-9a-f]{40}", commit),
             "a full lowercase 40-character Hub commit is required")


def _pins(pins, *, local=False, card=None):
    _require(isinstance(pins, (list, tuple)) and pins, "an ordered nonempty release file list is required")
    keys = {"path", "bytes", "sha256"}
    if local:
        keys.add("local_path")
    rows = []
    seen = set()
    for row in [*pins, *([card] if card is not None else [])]:
        _require(isinstance(row, dict) and set(row) == keys, "exact release file pin fields are required")
        leaf, size, sha = (row[key] for key in ("path", "bytes", "sha256"))
        is_card = leaf == CARD
        _require(isinstance(leaf, str) and (is_card or re.fullmatch(r"[a-z0-9][a-z0-9._-]{0,191}", leaf)),
                 "a safe root-level Hub filename is required")
        _require(not local or is_card == (row is card), "pass the reviewed README.md through the card argument")
        _require(leaf.casefold() not in seen, "duplicate release filename")
        _require(type(size) is int and 0 < size <= (MAX_CARD_BYTES if is_card else MAX_PART_BYTES)
                 and isinstance(sha, str) and re.fullmatch(r"[0-9a-f]{64}", sha), "invalid release byte length or SHA-256")
        if local:
            _require(isinstance(row["local_path"], (str, os.PathLike)) and Path(row["local_path"]).is_absolute(),
                     "an absolute local staged file path is required")
        seen.add(leaf.casefold())
        rows.append(dict(row))
    return rows


class _NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def _redirect(location, canonical):
    _require(isinstance(location, str) and 0 < len(location) <= 16384
             and location.isascii() and not re.search(r"[\x00-\x20\x7f\\]", location)
             and "#" not in location, "Hub redirect has an invalid Location")
    if location.startswith("/") and not location.startswith("//"):
        location = ORIGIN + location
    try:
        target = urlsplit(location)
    except ValueError:
        raise HubReleaseError("Hub redirect has an invalid authority") from None
    _require(target.scheme == "https" and target.netloc in {HUB_HOST, CDN_HOST},
             "Hub redirect left the approved HTTPS authorities")
    original = urlsplit(canonical).path
    repo, commit_and_path = original[1:].split("/resolve/", 1)
    if target.netloc == HUB_HOST:
        _require(target.path in {original, f"/api/resolve-cache/models/{repo}/{commit_and_path}"},
                 "Hub redirect changed the immutable file reference")
    else:
        _require(re.fullmatch(r"/xet-bridge-us/[0-9a-f]{24}/[0-9a-f]{64}", target.path),
                 "Hub redirect left the approved Xet object route")
    return location


def _one_header(response, name):
    values = response.headers.get_all(name, [])
    _require(len(values) <= 1, f"Hub response repeated {name}")
    return values[0] if values else None


def _open_blob(url, opener, deadline):
    canonical = url
    for redirects in range(MAX_REDIRECTS + 1):
        timeout = min(IO_TIMEOUT, deadline - time.monotonic())
        _require(timeout > 0, "Hub file verification deadline expired")
        request = Request(url, headers={"Accept-Encoding": "identity", "User-Agent": "AIRK-Arbiter-Release/1"})
        try:
            response = opener(request, timeout=timeout)
        except HTTPError as error:
            response = error
        except (OSError, HTTPException):
            raise HubReleaseError("Hub file HTTPS request failed") from None
        status = response.getcode()
        if status == 200:
            return response
        with response:
            if status not in REDIRECT_CODES:
                raise HubReleaseError(f"Hub file request returned HTTP {status}")
            _require(redirects < MAX_REDIRECTS, "Hub file redirect limit exceeded")
            url = _redirect(_one_header(response, "Location"), canonical)


def _verify_blob(repo, commit, pin, opener):
    filename = pin["path"]
    url = f"{ORIGIN}/{repo}/resolve/{commit}/{filename}"
    deadline = time.monotonic() + max(900, (pin["bytes"] + MIN_TRANSFER_RATE - 1) // MIN_TRANSFER_RATE)
    try:
        with _open_blob(url, opener, deadline) as response:
            length = _one_header(response, "Content-Length")
            _require(length is not None and re.fullmatch(r"[0-9]{1,20}", length)
                     and int(length) == pin["bytes"], "Hub Content-Length differs from the approved length")
            encoding = _one_header(response, "Content-Encoding")
            _require(encoding is None or encoding == "identity", "Hub returned an encoded response")
            _require(_one_header(response, "Content-Range") is None, "Hub returned a partial file response")
            digest = hashlib.sha256()
            received = 0
            while True:
                _require(time.monotonic() < deadline, "Hub file verification deadline expired")
                block = response.read(min(CHUNK, pin["bytes"] + 1 - received))
                if not block:
                    break
                received += len(block)
                _require(received <= pin["bytes"], "Hub response exceeded the approved length")
                digest.update(block)
            _require(time.monotonic() < deadline, "Hub file verification deadline expired")
            _require(received == pin["bytes"] and digest.hexdigest() == pin["sha256"],
                     "Hub file length or SHA-256 differs from the approved pin")
    except HubReleaseError as error:
        raise HubReleaseError(f"{filename}: {error}") from None
    except (OSError, EOFError, HTTPException):
        raise HubReleaseError(f"{filename}: reading the Hub file response failed") from None
    return {"path": filename, "bytes": received, "sha256": digest.hexdigest()}


def verify_hub(repo, commit, pins, *, opener=None):
    """Stream every approved immutable file anonymously and return verified remote pins."""
    _check_repo(repo)
    _check_commit(commit)
    rows = _pins(pins)
    if opener is None:
        opener = build_opener(ProxyHandler({}), _NoRedirect()).open
    return {"repo": repo, "commit": commit,
            "files": [_verify_blob(repo, commit, pin, opener) for pin in rows]}


def _remote_pin(pin):
    return {key: pin[key] for key in ("path", "bytes", "sha256")}


def _file_identity(info):
    return info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns


def _regular(info):
    return stat.S_ISREG(info.st_mode) and not getattr(info, "st_file_attributes", 0) & 0x400


def _open_local(path):
    if os.name != "nt":
        import fcntl

        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        try:
            fcntl.flock(descriptor, fcntl.LOCK_SH | fcntl.LOCK_NB)
            return os.fdopen(descriptor, "rb")
        except BaseException:
            os.close(descriptor)
            raise

    import ctypes
    from ctypes import wintypes as w
    import msvcrt

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    create = kernel.CreateFileW
    create.argtypes = (w.LPCWSTR, w.DWORD, w.DWORD, w.LPVOID, w.DWORD, w.DWORD, w.HANDLE)
    create.restype = w.HANDLE
    # Keep an original regular file open for reads while denying writes, moves and deletion.
    handle = create(str(path), 0x80000000, 1, None, 3, 0x00200000, None)
    if handle == ctypes.c_void_p(-1).value:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        descriptor = msvcrt.open_osfhandle(handle, os.O_RDONLY | os.O_BINARY)
    except BaseException:
        kernel.CloseHandle.argtypes = (w.HANDLE,)
        kernel.CloseHandle(handle)
        raise
    try:
        return os.fdopen(descriptor, "rb")
    except BaseException:
        os.close(descriptor)
        raise


@contextmanager
def _local_file(pin):
    path = Path(pin["local_path"])
    try:
        before = path.lstat()
        _require(path.resolve(strict=True) == path and _regular(before)
                 and before.st_size == pin["bytes"], "staged file path, type or length differs")
        file = _open_local(path)
    except (OSError, RuntimeError):
        raise HubReleaseError(f"{pin['path']}: staged file could not be opened exclusively for publication reads") from None
    with file:
        original = _file_identity(before)
        _require(_file_identity(os.fstat(file.fileno())) == original and _regular(os.fstat(file.fileno())),
                 f"{pin['path']}: staged file identity changed")
        digest = hashlib.sha256()
        left = pin["bytes"]
        while left:
            block = file.read(min(CHUNK, left))
            _require(block, f"{pin['path']}: staged file length differs")
            digest.update(block)
            left -= len(block)
        _require(not file.read(1) and digest.hexdigest() == pin["sha256"]
                 and _file_identity(os.fstat(file.fileno())) == original, f"{pin['path']}: staged file pin differs")
        file.seek(0)
        yield file


@contextmanager
def _quiet_sdk():
    # SDK exception details and HTTP debug logs can include signed URLs or token-bearing requests.
    previous = logging.root.manager.disable
    logging.disable(logging.CRITICAL)
    try:
        yield
    finally:
        logging.disable(previous)


def _sdk(api, add_operation):
    if api is not None and add_operation is not None:
        return api, add_operation
    try:
        from huggingface_hub import CommitOperationAdd, HfApi
    except ImportError:
        raise HubReleaseError("publication requires the official huggingface_hub Python SDK") from None
    return api if api is not None else HfApi(endpoint=ORIGIN, token=False), add_operation or CommitOperationAdd


def _existing(api, repo, head):
    try:
        with _quiet_sdk():
            info = api.model_info(repo, revision="main", token=False)
            _require(info.id == repo and info.private is False and info.gated is False,
                     "the exact Hub model repo must be public and ungated")
            _require(info.sha == head, "Hub main differs from the reviewed parent commit; reconcile before publication")
            return set(api.list_repo_files(repo, repo_type="model", revision=head, token=False))
    except HubReleaseError:
        raise
    except Exception:
        raise HubReleaseError("could not confirm the public Hub repo and file census at the reviewed parent commit") from None


def _sdk_path(pin, stream):
    # Xet upload accepts paths and bytes, while some SDK versions silently skip open streams.
    path = Path(pin["local_path"])
    if sys.platform.startswith("linux") and Path("/proc/self/fd").is_dir():
        path = Path("/proc/self/fd") / str(stream.fileno())
    _require(_file_identity(path.stat()) == _file_identity(os.fstat(stream.fileno())),
             f"{pin['path']}: staged SDK file no longer names the held source")
    return path


def _commit(api, add_operation, repo, head, credential, missing):
    try:
        with _quiet_sdk():
            operations = []
            for pin, stream in missing:
                operation = add_operation(path_in_repo=pin["path"], path_or_fileobj=_sdk_path(pin, stream))
                _require(operation.upload_info.size == pin["bytes"]
                         and operation.upload_info.sha256 == bytes.fromhex(pin["sha256"]),
                         f"{pin['path']}: SDK upload bytes differ from the approved pin")
                operations.append(operation)
    except HubReleaseError:
        raise
    except Exception:
        raise HubReleaseError("could not prepare the exact staged files for Hub publication") from None
    try:
        with _quiet_sdk():
            result = api.create_commit(repo_id=repo, repo_type="model", revision="main", parent_commit=head,
                                       operations=operations, token=credential, create_pr=False,
                                       commit_message="Publish verified AIRK Arbiter model assets")
        commit = result.oid
        _check_commit(commit)
        _require(commit != head and result.commit_url == f"{ORIGIN}/{repo}/commit/{commit}",
                 "Hub commit result did not name the exact authorized repository")
    except Exception:
        raise HubReleaseError("Hub commit was not confirmed; inspect main and reconcile the exact file pins before retrying") from None
    return commit


def publish_hub(repo, pins, *, expected_head, credential, card=None, opener=None, api=None, add_operation=None):
    """Add only missing verified files against the reviewed Hub main, then verify the returned commit."""
    _check_repo(repo)
    _check_commit(expected_head)
    _require(isinstance(credential, str) and 0 < len(credential) <= 1024
             and all(0x21 <= ord(ch) <= 0x7e for ch in credential), "a Hub publisher credential is required")
    local_pins = _pins(pins, local=True, card=card)
    remote_pins = [_remote_pin(pin) for pin in local_pins]
    with ExitStack() as stack:
        sources = {pin["path"]: stack.enter_context(_local_file(pin)) for pin in local_pins}
        api, add_operation = _sdk(api, add_operation)
        existing = _existing(api, repo, expected_head)
        for pin in remote_pins:
            prefix = pin["path"].casefold() + "/"
            _require(not any(path.casefold().startswith(prefix) or (path.casefold() == pin["path"].casefold()
                         and path != pin["path"]) for path in existing),
                     f"{pin['path']}: Hub path conflicts with another path at the reviewed commit")
        present = [pin for pin in remote_pins if pin["path"] in existing]
        if present:
            try:
                previous = verify_hub(repo, expected_head, present, opener=opener)
            except HubReleaseError as error:
                raise HubReleaseError(f"existing immutable Hub filename could not be approved at {expected_head}; {error}") from None
            if len(present) == len(local_pins):
                return dict(previous, changed=False)
        missing = [(pin, sources[pin["path"]]) for pin in local_pins if pin["path"] not in existing]
        commit = _commit(api, add_operation, repo, expected_head, credential, missing)
    try:
        return dict(verify_hub(repo, commit, remote_pins, opener=opener), changed=True)
    except HubReleaseError as error:
        raise HubReleaseError(f"Hub commit {commit} exists but anonymous verification failed; {error}") from None
