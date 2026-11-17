# SPDX-License-Identifier: Apache-2.0
"""Reuse the reviewed current-SID producer/observer; never manufacture a lease."""
from pathlib import Path
import hashlib
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[4]
MODELS = ROOT / "artifacts/advanced-split/models"
PRODUCER = MODELS / "a1-fresh-final-materializer-r5/prepare-anchor.ps1"
OBSERVER = MODELS / "observe-qasper-final-anchor-r3.ps1"
BRIDGE = Path(__file__).with_name("anchor_bridge.ps1")

def sha(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()

def verified_sources(packet, pins):
    paths = [Path(__file__).resolve(), PRODUCER, OBSERVER, BRIDGE, Path(packet["anchor_powershell"])]
    for path in paths:
        if not path.is_absolute() or pins.get(str(path)) != sha(path):
            raise ValueError("actual pinned anchor helper/image changed")
    return paths[-1]

def anchor_path(packet, stage):
    prefix = Path(packet["private_anchor_prefix"])
    allowed = {"emit", "numeric", "native-parent-bag-r1-u8", "native-parent-bag-r1-s8",
               "original-shipped-u8-ort130-evaluation", "original-mixed-u8-ort130-evaluation"}
    if stage not in allowed or not prefix.is_absolute() or prefix.parent.resolve() != MODELS.resolve():
        raise ValueError("fresh absolute private model-anchor prefix required")
    if not prefix.name.startswith("native-parent-bag-native-private-anchor-"):
        raise ValueError("foreign private-anchor purpose")
    return prefix.with_name(prefix.name + "-" + stage)

def require_fresh(path):
    if path.exists() or path.is_symlink():
        raise ValueError("private native stage cannot adopt an existing anchor")

def validate_proof(proof, path):
    if (proof.get("schema") != "airk.private-qasper-final-anchor-observation/1"
        or os.path.normcase(proof.get("canonical_path", "")) != os.path.normcase(str(path))
        or not proof.get("current_sid") or proof.get("current_sid") != proof.get("owner_sid")
        or proof.get("protected") is not True or proof.get("no_reparse") is not True
        or proof.get("current_owner_only") is not True or not proof.get("rules") or not proof.get("sddl")):
        raise ValueError("actual current-owner protected anchor proof refused")
    return proof

def observe(packet, pins, path, create=False):
    powershell = verified_sources(packet, pins)
    if os.name != "nt":
        raise ValueError("this owned Windows native launcher requires Windows")
    if create: require_fresh(path)
    argv = [str(powershell), "-NoProfile", "-NonInteractive", "-File", str(BRIDGE),
            "-Mode", "Create" if create else "Observe", "-Anchor", str(path),
            "-Producer", str(PRODUCER), "-Observer", str(OBSERVER)]
    actual = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True, check=True, timeout=30)
    return validate_proof(json.loads(actual.stdout), path)

def require_same(before, after):
    if before != after:
        raise ValueError("private native anchor ownership/ACL/path changed")

def create(packet, pins, path):
    produced = observe(packet, pins, path, True)
    checked = observe(packet, pins, path)
    require_same(produced, checked)
    return checked
