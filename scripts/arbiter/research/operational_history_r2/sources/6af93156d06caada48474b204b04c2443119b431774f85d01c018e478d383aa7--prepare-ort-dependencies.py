# SPDX-License-Identifier: Apache-2.0
"""Fetch only pinned CPU build inputs; no extraction or build execution."""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import time
import urllib.parse
import urllib.request

BASE = Path(__file__).resolve().parent
NAMES = {"abseil_cpp", "date", "mp11", "json", "pytorch_cpuinfo", "microsoft_gsl",
         "safeint", "flatbuffers", "onnx", "protobuf", "re2", "eigen", "microsoft_wil", "fp16"}
rows = []
for line in (BASE / "upstream-metadata/cmake/deps.txt").read_text().splitlines():
    if not line or line.startswith("#"):
        continue
    name, url, sha1 = line.split(";")
    if name in NAMES:
        rows.append((name, url, sha1))
assert {row[0] for row in rows} == NAMES

def fetch(row):
    name, url, expected_sha1 = row
    parts = urllib.parse.urlsplit(url)
    assert parts.scheme == "https" and parts.netloc == "github.com" and not parts.query
    relative = Path(parts.netloc) / parts.path.lstrip("/")
    assert ".." not in relative.parts
    target = BASE / "dependency-mirror" / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    if not target.exists():
        temporary = target.with_name(target.name + ".partial")
        request = urllib.request.Request(url, headers={"User-Agent": "AIRK-static-ORT-feasibility/1"})
        with urllib.request.urlopen(request, timeout=60) as response, temporary.open("xb") as output:
            final = urllib.parse.urlsplit(response.url)
            assert final.scheme == "https" and final.hostname in {"github.com", "codeload.github.com", "release-assets.githubusercontent.com", "objects.githubusercontent.com"}
            count = 0
            while block := response.read(1024 * 1024):
                count += len(block)
                if count > 100 * 1024 * 1024:
                    raise ValueError("Bounded dependency exceeded100MiB: " + name)
                output.write(block)
        if hashlib.file_digest(temporary.open("rb"), "sha1").hexdigest() != expected_sha1:
            raise ValueError("Pinned upstream SHA1 differs; preserve partial: " + name)
        temporary.rename(target)
    assert hashlib.file_digest(target.open("rb"), "sha1").hexdigest() == expected_sha1
    result = {"name": name, "url": url, "upstream_sha1": expected_sha1,
              "sha256": hashlib.file_digest(target.open("rb"), "sha256").hexdigest(),
              "bytes": target.stat().st_size, "mirror_relative_path": relative.as_posix(),
              "elapsed_seconds": round(time.monotonic() - started, 3)}
    print(json.dumps({"verified": name, "bytes": result["bytes"]}), flush=True)
    return result

started = time.monotonic()
results = []
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
    futures = {pool.submit(fetch, row): row[0] for row in rows}
    for future in concurrent.futures.as_completed(futures):
        results.append(future.result())
assert sum(row["bytes"] for row in results) < 512 * 1024 * 1024
(BASE / "cpu-dependencies.json").write_text(json.dumps({"schema": 1, "upstream_commit": "89746dc19a0a1ae59ebf4b16df9acab8f99f3925", "dependencies": sorted(results, key=lambda row: row["name"]), "elapsed_seconds": round(time.monotonic() - started, 3), "extracted_or_executed": False}, indent=2) + "\n")
