# SPDX-License-Identifier: Apache-2.0
"""Authenticate reuse of an existing Windows offline Cargo cache after lockfile changes."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

from windows_cargo_seed import digest, plan


def verify(shared: Path) -> dict:
    original = shared / "execution.json"
    original_seed = shared / "seed.json"
    cache = shared / "home"
    owner = Path(__file__).with_name("use-windows-offline-cargo.ps1")
    receipt = json.loads(original.read_bytes())
    original_pin = digest(original_seed)
    if not (receipt["passed"] is True and receipt["inventory_sha256"].lower() == original_pin
            and Path(receipt["root"]) == cache and receipt["environment_owner"].lower() == digest(owner)):
        raise ValueError("The original checked Cargo cache receipt does not bind the actual cache")
    old = json.loads(original_seed.read_bytes())
    current = plan()
    if old["lock_sha256"] != receipt["lock_sha256"] or old["git_commit"] != current["git_commit"]:
        raise ValueError("The original lock or Git dependency identity differs")
    if any(old["files"].get(path) != pin for path, pin in current["files"].items()):
        raise ValueError("Current lock requires unprovisioned or changed immutable dependencies")
    for name, pin in old["files"].items():
        path = cache / name
        if path.resolve(strict=True) != path or not path.is_file() or path.stat().st_size != pin["bytes"]:
            raise ValueError(f"Missing or linked original Cargo input: {name}")
        if digest(path) != pin["sha256"]:
            raise ValueError(f"Original Cargo input digest differs: {name}")
    return {"schema": "airk.private.windows-cargo-lock-reuse/1", "root": str(cache),
            "owner": receipt["owner"], "lock_sha256": current["lock_sha256"],
            "original_receipt_sha256": digest(original), "original_seed_sha256": original_pin,
            "immutable_files_checked": len(old["files"]), "locked_registry_packages": current["packages"],
            "git_commit": current["git_commit"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("shared", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        result = verify(args.shared.resolve(strict=True))
        raw = json.dumps(result, sort_keys=True, indent=2).encode("utf-8") + b"\n"
        if args.output:
            with args.output.open("xb") as output:
                output.write(raw)
        print(f"Authenticated {result['immutable_files_checked']} original dependency inputs "
              f"for {result['locked_registry_packages']} currently locked registry packages")
    except (KeyError, OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
