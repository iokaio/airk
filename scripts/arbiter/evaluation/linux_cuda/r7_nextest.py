# SPDX-License-Identifier: Apache-2.0
"""Preserve the signed R7 Nextest profile with a private per-attempt store."""

import hashlib
import json
from pathlib import Path
import stat
import tomllib


def config_for_store(data: bytes, store: Path) -> bytes:
    original = tomllib.loads(data.decode("utf-8"))
    if "store" in original:
        raise ValueError("signed Nextest configuration already specifies a store")
    if original.get("profile", {}).get("ci", {}).get("junit", {}).get("path") != "junit.xml":
        raise ValueError("signed Nextest CI JUnit path differs")
    if not store.is_absolute() or ".." in store.parts:
        raise ValueError("private absolute Nextest store path required")
    path = json.dumps(str(store), ensure_ascii=False)
    effective = data + f'\n[store]\ndir = {path}\n'.encode("utf-8")
    parsed = tomllib.loads(effective.decode("utf-8"))
    if parsed.pop("store") != {"dir": str(store)} or parsed != original:
        raise ValueError("Nextest configuration changed outside its store path")
    return effective


def file_pin(path, data):
    return {"path": str(path), "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def prepare(repo, attempt, expected_sha256, uid):
    from unpack import guarded

    original = repo / ".config/nextest.toml"
    info = original.lstat()
    if original.resolve(strict=True) != original or info.st_uid != uid or not stat.S_ISREG(info.st_mode):
        raise ValueError("signed Nextest configuration owner or path differs")
    data = original.read_bytes()
    input_pin = file_pin(original, data)
    if input_pin["sha256"] != expected_sha256:
        raise ValueError("signed Nextest configuration changed before projection")
    guarded(attempt.parent, uid, stat.S_ISDIR)
    guarded(attempt, uid, stat.S_ISDIR)
    if attempt.resolve(strict=True) != attempt:
        raise ValueError("original private Nextest attempt path required")
    store = attempt / "nextest-store"
    effective = config_for_store(data, store)
    store.mkdir(mode=0o700)
    guarded(store, uid, stat.S_ISDIR)
    config = attempt / "nextest-config.toml"
    with config.open("xb") as output:
        output.write(effective)
    expected = file_pin(config, effective)
    guarded(config, uid, stat.S_ISREG)
    if file_pin(config, config.read_bytes()) != expected:
        raise ValueError("persisted Nextest configuration changed")
    return {"original": input_pin, "effective": expected, "store": str(store)}


def result_junit(proof, uid):
    from unpack import guarded

    config = Path(proof["effective"]["path"])
    guarded(config, uid, stat.S_ISREG)
    if file_pin(config, config.read_bytes()) != proof["effective"]:
        raise ValueError("Nextest configuration changed during the native run")
    store = Path(proof["store"])
    guarded(store, uid, stat.S_ISDIR)
    report = store / "ci" / "junit.xml"
    if not report.exists():
        return None
    guarded(report.parent, uid, stat.S_ISDIR)
    guarded(report, uid, stat.S_ISREG)
    return file_pin(report, report.read_bytes())
