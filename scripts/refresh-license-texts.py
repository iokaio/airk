# SPDX-License-Identifier: Apache-2.0
"""Fetch missing dependency license texts at Cargo's recorded source revisions."""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import subprocess
import urllib.error
import urllib.request

# The published subcrate inherits its workspace license but omits repository.
# Its .cargo_vcs_info.json pins this parent repository and crates/rsqlite-vfs path.
REPOSITORY_OVERRIDES = {("rsqlite-vfs", "0.1.1"): "https://github.com/Spxg/sqlite-wasm-rs"}
# Older Cargo omitted VCS metadata. Every distributed payload file (1,389 i686;
# 1,418 x86_64) matches Git blob hashes at this exact publication commit, whose
# architecture manifests identify version 0.4.0 and the MIT/Apache-2.0 choice.
REVISION_OVERRIDES = {
    # All 12 payload files in the 0.3.1 archive match this revision's Git blobs.
    ("htmlescape", "0.3.1"): "1699b539179798e705ad8464128492a0a0092876",
    ("winapi-i686-pc-windows-gnu", "0.4.0"): "9497609ef44cc9bcd16cd2411c0ee6ccaf5483aa",
    ("winapi-x86_64-pc-windows-gnu", "0.4.0"): "9497609ef44cc9bcd16cd2411c0ee6ccaf5483aa",
}


def fetch(package):
    source = Path(package["manifest_path"]).parent
    candidates = [p for p in source.rglob("*") if p.is_file() and
                  p.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT"))]
    # A bundled Unicode subcomponent license is not the main Tree-sitter license.
    if candidates and package["name"] != "tree-sitter":
        return None
    vcs_path = source / ".cargo_vcs_info.json"
    repository = package.get("repository") or REPOSITORY_OVERRIDES.get((package["name"], package["version"]))
    revision_override = REVISION_OVERRIDES.get((package["name"], package["version"]))
    if (not vcs_path.is_file() and not revision_override) or not repository:
        print("No pinned upstream revision for " + package["name"] + "; retain publisher declaration for release review")
        return {"name": package["name"], "version": package["version"], "revision": None,
                "repository": package.get("repository"), "files": [], "missing_text": True}
    revision = json.loads(vcs_path.read_text(encoding="utf-8"))["git"]["sha1"] if vcs_path.is_file() else revision_override
    repository = repository.removesuffix("/").removesuffix(".git")
    if not repository.startswith("https://github.com/"):
        print("Manual license retrieval needed for " + package["name"])
        return {"name": package["name"], "version": package["version"], "revision": revision,
                "repository": repository, "files": [], "missing_text": True}
    base = repository.replace("https://github.com/", "https://raw.githubusercontent.com/") + "/" + revision + "/"
    directory = Path("third_party/licenses") / (package["name"] + "-" + package["version"])
    retrieved = []
    filenames = ["LICENSE", "LICENSE.txt", "LICENSE.md", "LICENSE-MIT", "LICENSE-APACHE", "COPYING",
                 "license", "license.txt", "license.md", "LICENCE", "COPYRIGHT", "NOTICE"]
    # r-efi's pinned AUTHORS file contains its MIT permission text and copyright notices.
    if package["name"] == "r-efi":
        filenames.append("AUTHORS")
    for filename in filenames:
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
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1"], text=True, encoding="utf-8"))
    members = set(metadata["workspace_members"])
    packages = [p for p in metadata["packages"] if p["id"] not in members]
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
        inventory = [record for record in pool.map(fetch, packages) if record]
    inventory.sort(key=lambda record: (record["name"], record["version"]))
    destination = Path("third_party/licenses/sources.json")
    destination.parent.mkdir(parents=True, exist_ok=True)
    # Preserve previously pinned legal records even when a package leaves the graph.
    prior = {(record["name"], record["version"]): record for record in
             json.loads(destination.read_text(encoding="utf-8"))} if destination.exists() else {}
    for record in inventory:
        key = (record["name"], record["version"])
        previous = prior.get(key, {})
        if previous.get("revision") == record.get("revision"):
            # Preserve factual review/declaration evidence for the exact same pin.
            for field in ("revision_basis", "declaration", "standalone_text_review"):
                if field in previous:
                    record[field] = previous[field]
        if record["files"] or not prior.get(key, {}).get("files"):
            prior[key] = record
    inventory = sorted(prior.values(), key=lambda record: (record["name"], record["version"]))
    destination.write_text(json.dumps(inventory, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
