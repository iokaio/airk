# SPDX-License-Identifier: Apache-2.0
"""Retain QASPER methodological publication; no candidate paper corpus acquisition."""
import hashlib
import json
from pathlib import Path
import urllib.request
root = Path(__file__).resolve().parent / "qasper-methodological-publisher-r1"
root.mkdir(exist_ok=False)
url = "https://aclanthology.org/2021.naacl-main.365.pdf"
with urllib.request.urlopen(url, timeout=20) as response:
    data = response.read(4 * 1024 * 1024 + 1)
    assert response.url == url
assert data.startswith(b"%PDF-") and len(data) <= 4 * 1024 * 1024
path = root / "2021.naacl-main.365.pdf"
path.write_bytes(data)
receipt = {"url": url, "file": path.name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(), "scope": "Methodological paper only; section2.1/page4600 notes2/3 is lineage evidence, not exact extraction-version proof. Published example families may have incidental exposure; none in proposed14 TEST pool."}
(root / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
print(json.dumps(receipt))
