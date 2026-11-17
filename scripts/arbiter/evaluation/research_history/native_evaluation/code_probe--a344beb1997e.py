# SPDX-License-Identifier: Apache-2.0
"""Pin public reference-code trees and small scoring sources without executing them."""
import hashlib
import json
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parent
records = []
for repository in ("strands-labs/strands-decider", "jaredpalmer/kev"):
    def read(url, limit=1048576):
        with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "AIRK-model-preparation"}), timeout=30) as response:
            value = response.read(limit + 1)
        if len(value) > limit:
            raise ValueError("reference metadata exceeds bound")
        return value
    commit = json.loads(read(f"https://api.github.com/repos/{repository}/commits/main"))["sha"]
    package = "src/strands_decider" if repository.startswith("strands-labs/") else "kev"
    tree = json.loads(read(f"https://api.github.com/repos/{repository}/contents/{package}?ref={commit}"))
    sources = [row["path"] for row in tree if row["path"].endswith(".py")
               and ("model" in row["path"] or "compil" in row["path"] or "export" in row["path"])]
    directory = ROOT / "reference-code" / repository.replace("/", "--")
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "tree.json").write_text(json.dumps(tree, indent=2) + "\n")
    fetched = []
    for name in sources:
        if any(part in name.split("/") for part in ("tests", "evals", "training", "scripts")):
            continue
        data = read(f"https://raw.githubusercontent.com/{repository}/{commit}/{name}", 131072)
        destination = directory / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        fetched.append({"path": name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
    records.append({"repository": repository, "revision": commit, "scope": package, "candidate_sources": sources, "fetched": fetched})
(ROOT / "code-inventory.json").write_text(json.dumps(records, indent=2) + "\n")
print(json.dumps(records, indent=2))
