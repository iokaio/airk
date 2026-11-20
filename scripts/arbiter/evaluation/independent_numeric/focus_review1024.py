# SPDX-License-Identifier: Apache-2.0
"""Seal both Focus source reviews using exactly the bytes parsed for their file inventories."""
import hashlib
import json
from pathlib import Path

from focus_w8_reference1024 import authenticate_review, original_assets
from maintained_helpers import worker


def snapshot(path):
    worker.regular(path)
    raw = path.read_bytes()
    return hashlib.sha256(raw).hexdigest(), json.loads(raw)


def retain(seals, path, digest):
    if seals.setdefault(str(path), digest) != digest:
        raise ValueError("reviewed source changed from original input seal")


def bind(evidence_root, adapter, adapter_pin, seals, pin, read_snapshot=snapshot):
    retain(seals, adapter, adapter_pin["sha256"])
    for path, expected in original_assets():
        item = pin(path)
        if item["sha256"] != expected:
            raise ValueError("original Focus W8 graph or external weights changed")
        retain(seals, item["path"], item["sha256"])
    contract = pin(evidence_root / "focus-w8-reference-contract.md")
    retain(seals, contract["path"], contract["sha256"])
    for historical, filename in ((True, "focus-w8-review-r1.json"),
                                 (False, "focus-w8-static1024-review.json")):
        path = evidence_root / filename
        sha, review = read_snapshot(path)
        files = review["files"]
        if historical:
            authenticate_review(sha, files)
        elif not any(entry["sha256"] == adapter_pin["sha256"] and
                     Path(entry["path"]).resolve(strict=True) == adapter for entry in files):
            raise ValueError("missing exact independently reviewed static1024 adapter source")
        for entry in files:
            item = pin(entry["path"])
            if item["sha256"] != entry["sha256"]:
                raise ValueError("independently reviewed Focus source changed")
            retain(seals, item["path"], item["sha256"])
        # The path must still name the same snapshot. The first digest, never a
        # refreshed hash, becomes the immutable parent/worker verification input.
        if pin(path)["sha256"] != sha:
            raise ValueError("review changed after reading its pinned file inventory")
        retain(seals, path, sha)

