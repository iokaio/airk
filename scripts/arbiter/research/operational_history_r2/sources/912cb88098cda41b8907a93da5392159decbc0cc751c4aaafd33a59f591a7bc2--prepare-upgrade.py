# SPDX-License-Identifier: Apache-2.0
"""Stage verified public VSIX files for local upgrade qualification only."""
import hashlib
import json
from pathlib import Path
import zipfile

base = Path(__file__).resolve().parent
old = base.parent / "release-1-0-0-publication/rehearsal-packages"
new = base / "rehearsal-packages"
for source, label in ((old, "old-extension"), (new, "new-extension")):
    receipts = [(p, json.loads(p.read_text(encoding="utf-8"))) for p in source.glob("*/manifest.json")]
    [(receipt_path, receipt)] = [(p, r) for p, r in receipts if r["platform"] == "win32-x64"]
    archive = receipt_path.parent / receipt["archive"]
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == receipt["archive_sha256"]
    names = ["native/airk.exe", "native/manifest.json"]
    if label == "new-extension":
        names.extend(["out/native.js", "out/model.js"])
    with zipfile.ZipFile(archive) as zipped:
        for name in names:
            destination = base / label / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            with destination.open("xb") as output:
                output.write(zipped.read("extension/" + name))
    print(f"Staged verified {receipt['version']} Windows helper: {base / label}")
