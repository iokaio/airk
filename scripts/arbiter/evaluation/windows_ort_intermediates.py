# SPDX-License-Identifier: Apache-2.0
"""Enumerate only immutable original Ninja intermediates in a successfully verified, closed Windows ORT build."""
import argparse
import hashlib
import json
from pathlib import Path
import stat

ROOT = Path(__file__).resolve().parents[3]
E = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
BUILD = E / "q/b/Release"


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def intermediate_outputs(manifest):
    for line in manifest.splitlines():
        if not line.startswith("build "):
            continue
        outputs, sep, _ = line[6:].partition(": ")
        if not sep:
            continue
        for token in outputs.split():
            if "$" in token or not token.lower().endswith((".obj", ".pch")):
                continue
            path = Path(token)
            if path.is_absolute() or ".." in path.parts:
                raise ValueError("Ninja output escapes original build")
            yield path


def inventory():
    stamp = E / "windows-cpu-ort-custody-r3.json"
    checked = json.loads(stamp.read_bytes())
    if checked["owner_sid"] != "S-1-5-21-3678370870-3858642417-826968105-1002":
        raise ValueError("Verified build custody required")
    checked_inputs = {Path(name): value.lower() for name, value in checked["inputs"].items()}
    for name in (E/"q/build-receipt.json", E/"q/static-aggregate-receipt.json",
                 E/"windows-fresh-cpu-ort-r3/execution.json"):
        if sha(name) != checked_inputs[name]:
            raise ValueError("Verified build changed")
    manifest = BUILD/"build.ninja"
    relative = sorted(set(intermediate_outputs(manifest.read_text("utf-8"))))
    items = []
    for path in relative:
        obj = BUILD/path
        if not obj.exists():
            continue
        s = obj.lstat()
        if (obj.resolve(strict=True) != obj or not obj.is_relative_to(BUILD) or
            not stat.S_ISREG(s.st_mode) or s.st_nlink != 1 or
            s.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT):
            raise ValueError("Original build output is linked or escaped")
        items.append({"path":str(obj),"bytes":s.st_size,"sha256":sha(obj)})
    if not items:
        raise ValueError("No genuine original Ninja intermediate remains")
    return {"input_custody":sha(stamp),"ninja_sha256":sha(manifest),"root":str(BUILD),"items":items,
            "bytes":sum(i["bytes"] for i in items)}


if __name__ == "__main__":
    argparse.ArgumentParser(description=__doc__).parse_args()
    print(json.dumps(inventory(),sort_keys=True))
