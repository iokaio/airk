# SPDX-License-Identifier: Apache-2.0
"""Inspect bounded official backend/export package source metadata, no execution."""
import hashlib
import json
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parent / "backend-research"
ROOT.mkdir(exist_ok=True)


def read(url):
    with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "AIRK-Arbiter-native-research"}), timeout=30) as response:
        data = response.read(524289)
    if len(data) > 524288:
        raise ValueError("backend source metadata exceeds 512 KiB")
    return data


records = []
for repository, branch, directories in [
    ("huggingface/candle", "main", ["candle-transformers/src/models"]),
    ("microsoft/onnxruntime-genai", "main", ["src/python/py/models"]),
    ("huggingface/optimum-onnx", "main", ["optimum/exporters/onnx"]),
]:
    revision = json.loads(read(f"https://api.github.com/repos/{repository}/commits/{branch}"))["sha"]
    record = {"repository": repository, "revision": revision, "listings": [], "sources": []}
    for directory in directories:
        listing = json.loads(read(f"https://api.github.com/repos/{repository}/contents/{directory}?ref={revision}"))
        record["listings"].append({"path": directory, "entries": [{"path": item["path"], "size": item["size"], "type": item["type"]} for item in listing]})
        for item in listing:
            if item["type"] != "file" or not ("qwen3" in item["name"].lower() or item["name"] == "README.md"):
                continue
            data = read(f"https://raw.githubusercontent.com/{repository}/{revision}/{item['path']}")
            destination = ROOT / repository.replace("/", "--") / item["path"]
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
            record["sources"].append({"path": item["path"], "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
    records.append(record)
for model in ["onnx-community/Qwen3.5-0.8B-ONNX", "onnx-community/Qwen3.5-0.8B-Text-ONNX"]:
    url = "https://huggingface.co/api/models/" + model + "?blobs=true"
    data = read(url)
    parsed = json.loads(data)
    (ROOT / (model.replace("/", "--") + ".metadata.json")).write_bytes(data)
    records.append({"repository": model, "revision": parsed["sha"], "metadata_sha256": hashlib.sha256(data).hexdigest()})
(ROOT / "inventory.json").write_text(json.dumps(records, indent=2) + "\n")
print(json.dumps(records, indent=2))
