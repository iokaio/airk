# SPDX-License-Identifier: Apache-2.0
"""Freeze small publisher reference/config/license inputs, never model weights."""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parent
NAMES = {"README.md", "LICENSE", "LICENSE.md", "LICENSE-QWEN", "config.json",
         "adapter_config.json", "lora/adapter_config.json", "strands_decider_config.json",
         "provenance.json", "training_config.json", "train_config.json", "inference.py",
         "MANIFEST.sha256", "requirements.txt", "model.safetensors.index.json"}
jobs = []
for path in ROOT.glob("*.metadata.json"):
    metadata = json.loads(path.read_text())
    for row in metadata.get("siblings", []):
        if row["rfilename"] in NAMES and row.get("size", 0) <= 131072:
            jobs.append((metadata["id"], metadata["sha"], row["rfilename"]))

def fetch(job):
    model, revision, name = job
    url = f"https://huggingface.co/{model}/resolve/{revision}/{name}"
    result = {"model": model, "revision": revision, "name": name, "url": url}
    try:
        with urllib.request.urlopen(url, timeout=30) as response:
            data = response.read(131073)
        if len(data) > 131072:
            raise ValueError("metadata source exceeds 128 KiB bound")
        destination = ROOT / "publisher" / model.replace("/", "--") / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        result.update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest(),
                      file=destination.relative_to(ROOT).as_posix())
    except Exception as error:
        result["error"] = str(error)
    return result

with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
    rows = list(pool.map(fetch, jobs))
(ROOT / "source-inventory.json").write_text(json.dumps(rows, indent=2) + "\n")
print(json.dumps({"files": len(rows), "bytes": sum(row.get("bytes", 0) for row in rows),
                  "errors": [row for row in rows if "error" in row]}, indent=2))
