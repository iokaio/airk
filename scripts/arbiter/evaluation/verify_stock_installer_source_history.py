# SPDX-License-Identifier: Apache-2.0
"""Recreate the three original Windows evidence producer hashes from maintained LF source."""
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ORIGINAL = {
    "windows_stock_cpu_installer.py": "01982a4ec63edb6834d77a1ac40b91d8a7376423d54f4631bd0a1c93326aae06",
    "windows_iscc_path.iss": "03c9a2ca3466b3c766e6e87464eef94b4b0f91b994712a4a4c430dc1fbae8a9a",
    "windows_iscc_path_controls.py": "30a7a13e7fee4cc7dc67644cb03d9d39d2c8177c1cf44a35c33d979ce285c0b5",
}


def verify():
    rows = {}
    for name, expected in ORIGINAL.items():
        source = (HERE / name).read_bytes()
        if not source.endswith(b"\n") or source.endswith(b"\n\n") or b"\r" in source:
            raise ValueError(f"Expected a single terminal LF in {name}")
        issued = hashlib.sha256(source + b"\n").hexdigest()
        if issued != expected:
            raise ValueError(f"Original evidence source differs for {name}")
        rows[name] = {"repository_sha256": hashlib.sha256(source).hexdigest(), "issued_sha256": issued}
    return rows


if __name__ == "__main__":
    print(json.dumps(verify(), sort_keys=True))
