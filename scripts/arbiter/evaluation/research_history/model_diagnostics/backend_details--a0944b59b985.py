# SPDX-License-Identifier: Apache-2.0
"""Pin the specific backend implementation sources needed for feasibility review."""
import hashlib
import json
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parent / "backend-research"


def read(url):
    with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "AIRK-Arbiter-native-research"}), timeout=30) as response:
        data = response.read(524289)
    if len(data) > 524288:
        raise ValueError("bounded reference source too large")
    return data


records = []
for repository, revision, names in [
    ("microsoft/onnxruntime-genai", "0f8d01c477f289de9da5e610d701f957f1bf6ca6", ["src/python/py/models/builders/qwen.py", "src/python/py/models/docs/model-specific-options.md"]),
    ("huggingface/optimum-onnx", "46533fb66570021f52a39d77112b703762fdaada", ["optimum/exporters/onnx/model_configs.py", "optimum/exporters/onnx/model_patcher.py"]),
    ("microsoft/onnxruntime", "v1.22.1", ["docs/ContribOperators.md"]),
]:
    for name in names:
        url = f"https://raw.githubusercontent.com/{repository}/{revision}/{name}"
        try:
            data = read(url)
            destination = ROOT / repository.replace("/", "--") / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
            records.append({"url": url, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
        except Exception as error:
            records.append({"url": url, "error": str(error)})
repository = "EricLBuehler/mistral.rs"
revision = json.loads(read(f"https://api.github.com/repos/{repository}/commits/master"))["sha"]
listing = json.loads(read(f"https://api.github.com/repos/{repository}/contents/mistralrs-core/src/models?ref={revision}"))
for item in listing:
    if "qwen3" not in item["name"] or item["type"] != "file":
        continue
    name = item["path"]
    url = f"https://raw.githubusercontent.com/{repository}/{revision}/{name}"
    data = read(url)
    destination = ROOT / repository.replace("/", "--") / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(data)
    records.append({"url": url, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
(ROOT / "details-inventory.json").write_text(json.dumps(records, indent=2) + "\n")
print(json.dumps(records, indent=2))
