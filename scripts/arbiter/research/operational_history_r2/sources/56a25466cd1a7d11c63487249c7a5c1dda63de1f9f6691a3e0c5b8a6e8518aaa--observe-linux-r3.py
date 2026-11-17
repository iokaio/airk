# SPDX-License-Identifier: Apache-2.0
"""Read-only observation of this qualification's exact public static build."""
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path

BASE = Path(__file__).resolve().parent / "linux-container-qualification-r3"
PREFIX = ["wsl", "-d", "docker-desktop", "--"]
EXACT = "python3 scripts/provision-static-onnx-runtime.py --destination /build/static-onnx-runtime --download --download-tools --build --parallel 2"
rows = []
for line in subprocess.check_output(PREFIX + ["ps", "-eo", "pid,ppid,etime,comm,args"], text=True).splitlines()[1:]:
    fields = line.split(None, 4)
    if len(fields) == 5 and fields[0].isdigit() and fields[1].isdigit():
        rows.append(dict(pid=int(fields[0]), parent=int(fields[1]), elapsed=fields[2], name=fields[3], args=fields[4]))
selected = [row for row in rows if row["args"] == EXACT]
pids = {row["pid"] for row in selected}
while True:
    extra = {row["pid"] for row in rows if row["parent"] in pids}
    if extra.issubset(pids):
        break
    pids |= extra
value = {"observed_utc": datetime.now(timezone.utc).isoformat(), "static_provisioner_count": len(selected),
         "observed_descendants": [{key: row[key] for key in ("pid", "parent", "elapsed", "name")} for row in rows if row["pid"] in pids]}
if len(selected) == 1:
    pid = selected[0]["pid"]
    value["io"] = subprocess.check_output(PREFIX + ["cat", f"/proc/{pid}/io"], text=True)
    opened = subprocess.check_output(PREFIX + ["ls", "-l", f"/proc/{pid}/fd"], text=True)
    value["public_build_open_files"] = [line for line in opened.splitlines() if "/build/static-onnx-runtime" in line]
with (BASE / "progress-observations.jsonl").open("a", encoding="utf-8") as output:
    output.write(json.dumps(value) + "\n")
print(json.dumps(value))
