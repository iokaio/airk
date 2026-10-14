# SPDX-License-Identifier: Apache-2.0
"""Fetch missing dependency license texts at Cargo's recorded source revisions."""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import subprocess
import urllib.error
import urllib.request


def fetch(package):
    source = Path(package["manifest_path"]).parent
    candidates = [p for p in source.rglob("*") if p.is_file() and
                  p.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT"))]
    # A bundled Unicode subcomponent license is not the main Tree-sitter license.
    if candidates and package["name"] != "tree-sitter":
        return None
    vcs = json.loads((source / ".cargo_vcs_info.json").read_text(encoding="utf-8"))
    revision = vcs["git"]["sha1"]
    repository = package["repository"].removesuffix("/").removesuffix(".git")
    if not repository.startswith("https://github.com/"):
        raise ValueError("Manual license retrieval needed for " + package["name"])
    base = repository.replace("https://github.com/", "https://raw.githubusercontent.com/") + "/" + revision + "/"
    directory = Path("third_party/licenses") / (package["name"] + "-" + package["version"])
    retrieved = []
    for filename in ("LICENSE", "LICENSE.txt", "LICENSE-MIT", "LICENSE-APACHE", "COPYING"):
        url = base + filename
        try:
            with urllib.request.urlopen(url, timeout=20) as response:
                body = response.read(262145)
        except urllib.error.HTTPError as error:
            if error.code == 404:
                continue
            raise
        if len(body) > 262144:
            raise ValueError("License text exceeds limit")
        directory.mkdir(parents=True, exist_ok=True)
        (directory / filename).write_bytes(body)
        retrieved.append({"file": filename, "url": url, "sha256": hashlib.sha256(body).hexdigest()})
    if not retrieved:
        print("No separate upstream license text: " + package["name"] + "; retain publisher declaration for release review")
    if retrieved:
        print("Retrieved license text: " + package["name"])
    return {"name": package["name"], "version": package["version"], "revision": revision,
            "repository": repository, "files": retrieved, "missing_text": not retrieved}


def main():
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1"], text=True))
    members = set(metadata["workspace_members"])
    packages = [p for p in metadata["packages"] if p["id"] not in members]
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
        inventory = [record for record in pool.map(fetch, packages) if record]
    inventory.sort(key=lambda record: (record["name"], record["version"]))
    destination = Path("third_party/licenses/sources.json")
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(inventory, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
