# SPDX-License-Identifier: Apache-2.0
"""Inspect official released operator catalogs, without downloading a runtime."""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import time
import urllib.request

ROOT = Path(__file__).resolve().parent / "backend-research"
VERSIONS = ["1.22.1", "1.23.0", "1.24.0", "1.25.0", "1.26.0", "1.27.0", "1.28.0", "1.29.0", "1.30.0"]
OPERATORS = ["LinearAttention", "LinearAttentionGate", "CausalConvWithState", "GatedDeltaNet"]


def fetch(version):
    url = f"https://raw.githubusercontent.com/microsoft/onnxruntime/v{version}/docs/ContribOperators.md"
    result = {"version": version, "url": url}
    try:
        with urllib.request.urlopen(url, timeout=30) as response:
            data = response.read(524289)
        if len(data) > 524288:
            raise ValueError("operator catalog exceeds 512 KiB bound")
        text = data.decode("utf-8")
        destination = ROOT / f"ort-{version}-ContribOperators.md"
        destination.write_bytes(data)
        result.update(sha256=hashlib.sha256(data).hexdigest(), bytes=len(data),
                      present={operator: operator in text for operator in OPERATORS})
    except Exception as error:
        result["error"] = str(error)
    return result


with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
    rows = list(pool.map(fetch, VERSIONS))
output = ROOT / f"operator-version-observations-{time.time_ns()}.json"
output.write_text(json.dumps(rows, indent=2) + "\n")
print(json.dumps(rows, indent=2))
