# SPDX-License-Identifier: Apache-2.0
"""Create a third-party license bundle from the locked Cargo graph, not local corpora."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--format-version", "1", "--locked"
    ], text=True))
    workspace_ids = set(metadata["workspace_members"])
    pinned_path=Path("third_party/licenses/sources.json")
    if pinned_path.exists():
        import hashlib
        for record in json.loads(pinned_path.read_text(encoding="utf-8")):
            for item in record["files"]:
                path=pinned_path.parent/(record["name"]+"-"+record["version"])/item["file"]
                if hashlib.sha256(path.read_bytes()).hexdigest()!=item["sha256"]:
                    raise SystemExit("Pinned license digest mismatch: "+str(path))
    args.output.mkdir(parents=True, exist_ok=True)
    inventory = []
    missing = []
    for package in metadata["packages"]:
        if package["id"] in workspace_ids:
            continue
        source = Path(package["manifest_path"]).parent
        destination = args.output / (package["name"] + "-" + package["version"])
        destination.mkdir(exist_ok=True)
        license_files = []
        candidates = [p for p in source.rglob("*") if p.is_file() and
                      p.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT"))]
        if package.get("license_file"):
            candidates.append(source / package["license_file"])
        fallback = Path("third_party/licenses") / (package["name"] + "-" + package["version"])
        if fallback.exists():
            candidates.extend(p for p in fallback.iterdir() if p.is_file())
        for item in sorted(set(candidates)):
            if item.is_file():
                name = item.name
                if item.is_relative_to(source) and item.parent != source:
                    name = "__".join(item.relative_to(source).parts)
                shutil.copyfile(item, destination / name)
                license_files.append(name)
        inventory.append({"name": package["name"], "version": package["version"],
                          "license": package.get("license"), "repository": package.get("repository"),
                          "source": package.get("source"), "license_files": license_files})
        if not package.get("license") and not package.get("license_file"):
            missing.append(package["name"])
        if not license_files:
            declaration = {"name": package["name"], "version": package["version"],
                           "license": package.get("license"), "authors": package.get("authors"),
                           "repository": package.get("repository"),
                           "note": "Publisher declares a license but supplies no separate license text. Review before publishing."}
            (destination / "UPSTREAM-LICENSE-DECLARATION.json").write_text(json.dumps(declaration, indent=2) + "\n", encoding="utf-8")
            print("Release review needed for license declaration without separate text: " + package["name"])
    (args.output / "inventory.json").write_text(json.dumps(inventory, indent=2) + "\n", encoding="utf-8")
    if missing:
        raise SystemExit("Missing license declarations: " + ", ".join(missing))
    print(f"Bundled notices for {len(inventory)} third-party packages in {args.output}")


if __name__ == "__main__":
    main()
