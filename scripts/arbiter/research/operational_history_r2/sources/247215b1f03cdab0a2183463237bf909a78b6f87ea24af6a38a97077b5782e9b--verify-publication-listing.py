# SPDX-License-Identifier: Apache-2.0
"""Read-only listing checks for the four approved public platform VSIXs."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

parser = argparse.ArgumentParser()
parser.add_argument("--directory", type=Path, required=True)
parser.add_argument("--source", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
source = json.loads((args.source / "packages/vscode/package.json").read_text(encoding="utf-8"))
expected_image = (args.source / "docs/workbench/images/workbench-1.8.7-results.png").read_bytes()
platforms = {"win32-x64", "linux-x64", "darwin-arm64", "darwin-x64"}
results = []
seen = set()
for path in sorted(args.directory.rglob("airk-1.8.7-*.vsix")):
    matched = [p for p in platforms if path.name == f"airk-1.8.7-{p}.vsix"]
    assert len(matched) == 1 and matched[0] not in seen, path
    platform = matched[0]
    seen.add(platform)
    with zipfile.ZipFile(path) as zipped:
        names = zipped.namelist()
        assert len(names) == len(set(names)), path
        package = json.loads(zipped.read("extension/package.json"))
        assert package["version"] == source["version"] == "1.8.7", path
        assert package["description"] == source["description"], path
        readmes = [name for name in names if name.lower() == "extension/readme.md"]
        assert len(readmes) == 1, path
        readme = zipped.read(readmes[0]).decode("utf-8")
        assert "workbench-1.8.7-results.png" in readme and "Complete response" in readme, path
        images = [name for name in names if name.endswith("/workbench-1.8.7-results.png")]
        assert len(images) == 1 and zipped.read(images[0]) == expected_image, path
        assert not any(set(name.split("/")) & {"temp", "artifacts", ".airk", ".git", ".codex"} for name in names), path
    results.append(dict(platform=platform, path=str(path), version=package["version"],
                        description=package["description"], image=images[0],
                        image_sha256=hashlib.sha256(expected_image).hexdigest(), passed=True))
assert seen == platforms, seen
args.output.write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
print(json.dumps(dict(platforms=sorted(seen), listing_and_image_verified=True)))
