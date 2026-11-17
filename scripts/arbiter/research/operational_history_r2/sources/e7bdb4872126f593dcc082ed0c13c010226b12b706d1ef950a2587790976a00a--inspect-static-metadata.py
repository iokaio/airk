# SPDX-License-Identifier: Apache-2.0
"""Read only pinned upstream metadata; never execute upstream build code."""
import hashlib
import json
from pathlib import Path
import subprocess

BASE = Path(__file__).resolve().parent
SOURCE = BASE / "onnxruntime-source"
REVISION = "89746dc19a0a1ae59ebf4b16df9acab8f99f3925"
FILES = [".gitmodules", "cmake/deps.txt", "cmake/CMakeLists.txt",
         "cmake/external/onnxruntime_external_deps.cmake", "tools/ci_build/build.py",
         "tools/ci_build/build_args.py", "LICENSE", "ThirdPartyNotices.txt"]
command = ["git", "-c", "safe.directory=" + SOURCE.as_posix(), "-C", str(SOURCE)]
actual = subprocess.run(command + ["rev-parse", "HEAD"], check=True, capture_output=True, timeout=30).stdout.decode().strip()
assert actual == REVISION
records = []
for name in FILES:
    completed = subprocess.run(command + ["show", REVISION + ":" + name], check=True, capture_output=True, timeout=90)
    assert len(completed.stdout) <= 1048576
    destination = BASE / "upstream-metadata" / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(completed.stdout)
    records.append({"path": name, "bytes": len(completed.stdout), "sha256": hashlib.sha256(completed.stdout).hexdigest()})
tree = subprocess.run(command + ["ls-tree", "-r", REVISION], check=True, capture_output=True, timeout=30).stdout.decode()
submodules = [line for line in tree.splitlines() if line.startswith("160000 ")]
(BASE / "upstream-metadata.json").write_text(json.dumps({"source": "https://github.com/microsoft/onnxruntime.git", "revision": REVISION, "files": records, "submodules": submodules, "upstream_build_executed": False}, indent=2) + "\n")
print(json.dumps({"revision": REVISION, "metadata_files": len(records), "submodules": submodules, "upstream_build_executed": False}))
