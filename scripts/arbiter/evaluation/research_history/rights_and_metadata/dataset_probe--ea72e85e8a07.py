# SPDX-License-Identifier: Apache-2.0
"""Record human-dataset publisher metadata/cards/licenses only, never raw corpora."""
import hashlib
import json
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parent / "dataset-research"
ROOT.mkdir(exist_ok=True)


def read(url):
    with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "AIRK-A0-dataset-research"}), timeout=30) as response:
        data = response.read(262145)
    if len(data) > 262144:
        raise ValueError("dataset metadata exceeds 256 KiB bound")
    return data


rows = []
api = "https://huggingface.co/api/datasets/allenai/qasper?blobs=true"
data = read(api)
metadata = json.loads(data)
(ROOT / "qasper.metadata.json").write_bytes(data)
revision = metadata["sha"]
url = f"https://huggingface.co/datasets/allenai/qasper/resolve/{revision}/README.md"
card = read(url)
(ROOT / "qasper.README.md").write_bytes(card)
rows.append({"dataset": "allenai/qasper", "revision": revision, "metadata_url": api,
             "metadata_sha256": hashlib.sha256(data).hexdigest(), "card_url": url,
             "card_sha256": hashlib.sha256(card).hexdigest(), "license_metadata": metadata.get("cardData", {}).get("license"),
             "assets": metadata.get("siblings", [])})
for repository in ("github/CodeSearchNet", "allenai/scifact"):
    metadata = json.loads(read(f"https://api.github.com/repos/{repository}/commits/master"))
    revision = metadata["sha"]
    result = {"dataset": repository, "revision": revision, "sources": []}
    for name in ("README.md", "LICENSE" if repository.startswith("github/") else "LICENSE.md"):
        url = f"https://raw.githubusercontent.com/{repository}/{revision}/{name}"
        data = read(url)
        target = ROOT / (repository.replace("/", "--") + "." + name)
        target.write_bytes(data)
        result["sources"].append({"url": url, "file": target.name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
    rows.append(result)
(ROOT / "inventory.json").write_text(json.dumps(rows, indent=2) + "\n")
print(json.dumps([{key: row[key] for key in ("dataset", "revision")} for row in rows]))
