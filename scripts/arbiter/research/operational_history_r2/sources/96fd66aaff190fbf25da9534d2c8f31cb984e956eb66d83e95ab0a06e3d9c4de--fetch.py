# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import urllib.request
import json
import hashlib

root = Path(__file__).resolve().parent
for name, url in [
    ("index.jsonl", "https://index.crates.io/ru/st/rustls"),
    ("rustls-0.23.46.crate", "https://static.crates.io/crates/rustls/rustls-0.23.46.crate"),
]:
    request = urllib.request.Request(url, headers={"User-Agent": "AIRK dependency source review"})
    with urllib.request.urlopen(request, timeout=45) as response:
        data = response.read(4 * 1024 * 1024 + 1)
        assert len(data) <= 4 * 1024 * 1024
    (root / name).write_bytes(data)
    print(name, len(data), hashlib.sha256(data).hexdigest())
rows = [json.loads(line) for line in (root / "index.jsonl").read_bytes().splitlines()]
row = next(r for r in rows if r["vers"] == "0.23.46")
assert not row.get("yanked")
assert row["cksum"] == hashlib.sha256((root / "rustls-0.23.46.crate").read_bytes()).hexdigest()
(root / "index-0.23.46.json").write_text(json.dumps(row, indent=2) + "\n", encoding="utf-8")
print("Exact official registry/index archive checksum MATCH", row["cksum"])
