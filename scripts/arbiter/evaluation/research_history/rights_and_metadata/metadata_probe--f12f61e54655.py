# SPDX-License-Identifier: Apache-2.0
"""Read publisher metadata only; never downloads model weights or executes assets."""
import datetime
import hashlib
import json
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parent
MODELS = [
    "internlm/Intern-Decision-0.8B", "internlm/Intern-Decision-4B",
    "StrandsAgents/strands-decider-2B-qwen3.5-v1-2610", "jaredpalmer/kev-4b",
    "sentence-transformers/all-MiniLM-L6-v2", "Qwen/Qwen3.5-2B-Base", "Qwen/Qwen3.5-4B-Base",
]
rows = []
for model in MODELS:
    url = "https://huggingface.co/api/models/" + model + "?blobs=true"
    try:
        data = urllib.request.urlopen(url, timeout=30).read()
        doc = json.loads(data)
        name = model.replace("/", "--") + ".metadata.json"
        (ROOT / name).write_bytes(data)
        rows.append({"id": model, "revision": doc.get("sha"),
                     "license": doc.get("cardData", {}).get("license"),
                     "files": len(doc.get("siblings", [])), "file": name,
                     "sha256": hashlib.sha256(data).hexdigest(), "url": url})
    except Exception as error:
        rows.append({"id": model, "error": str(error), "url": url})
(ROOT / "metadata-inventory.json").write_text(json.dumps({
    "observed_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "models": rows,
}, indent=2) + "\n")
print(json.dumps(rows, indent=2))
