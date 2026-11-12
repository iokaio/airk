# SPDX-License-Identifier: Apache-2.0
"""Check the sealed admin bundle and write its inventory (split C7 owner of `admin-assets.json`).

The admin shell is built by `npm run build` in packages/admin into
crates/airk-cli/admin-assets/, which `airk` embeds. This script runs the locked
install and the drift check (`npm run check`), verifies the committed bundle
against its manifest, and writes the C7 `airk-admin-assets/1` inventory that
native receipts bind (build receipt schema 2, native candidate /3, CI runtime /2).
It never rewrites the committed bundle.

Digests: `inventory_sha256` and `bundle_sha256` are the framed I4 digests that
the `airk` image computes from its embedded bundle
(crates/airk-cli/src/admin_plane/assets.rs, `embedded_asset_digests`): files in
name order; inventory = name, SHA-256 and content type; bundle = name and bytes.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
PACKAGE = ROOT / "packages/admin"
ASSETS = ROOT / "crates/airk-cli/admin-assets"
MANIFEST_SCHEMA = "airk-admin-assets/1"
LOCKS = ("packages/admin/package-lock.json",)
OUTPUT = ROOT / "artifacts/admin/admin-assets.json"


def _receipts():
    spec = importlib.util.spec_from_file_location("airk_split_receipts", ROOT / "scripts/split_receipts.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def framed_digest(domain, fields):
    """airk_core::identity::framed_digest: magic, then each field with a u64 BE length."""
    hash = hashlib.sha256(b"AIRK-DIGEST-1\0")
    for field in [domain, *fields]:
        hash.update(len(field).to_bytes(8, "big"))
        hash.update(field)
    return hash.hexdigest()


def _file_digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def verify_committed(assets=None):
    """The committed bundle matches its manifest exactly; returns the manifest rows in name order.

    A stale or tampered file, a missing file or an unlisted file is an error (rebuild with
    `npm run build` in packages/admin)."""
    assets = ASSETS if assets is None else assets
    manifest = json.loads((assets / "manifest.json").read_text(encoding="utf-8"))
    if not isinstance(manifest, dict) or set(manifest) != {"schema", "files"} or manifest["schema"] != MANIFEST_SCHEMA:
        raise RuntimeError("Unexpected admin asset manifest schema")
    rows = manifest["files"]
    if not isinstance(rows, dict) or not rows:
        raise RuntimeError("Admin asset manifest lists no files")
    present = {path.name for path in assets.iterdir()} - {"manifest.json"}
    if any(path.is_dir() or path.is_symlink() for path in assets.iterdir()):
        raise RuntimeError("Admin asset directory contains a link or subdirectory")
    if present != set(rows):
        raise RuntimeError("Admin asset files differ from the manifest: "
                           + ", ".join(sorted(present ^ set(rows))))
    ordered = []
    for name in sorted(rows):
        entry = rows[name]
        if not isinstance(entry, dict) or set(entry) != {"sha256", "content_type"}:
            raise RuntimeError(f"Invalid admin manifest row {name}")
        path = assets / name
        if _file_digest(path) != entry["sha256"]:
            raise RuntimeError(f"Stale admin asset {name}; rebuild the admin bundle")
        ordered.append((name, entry, path))
    return ordered


def digests(assets=None):
    """The framed I4 admin digests of the committed bundle."""
    inventory, bundle = [], []
    for name, entry, path in verify_committed(assets):
        inventory += [name.encode(), entry["sha256"].encode(), entry["content_type"].encode()]
        bundle += [name.encode(), path.read_bytes()]
    return {"inventory_sha256": framed_digest(b"airk/admin-asset-inventory/1", inventory),
            "bundle_sha256": framed_digest(b"airk/admin-asset-bundle/1", bundle)}


def inventory_document(node, npm, assets=None):
    """The C7 `airk-admin-assets/1` inventory receipt, validated before it is returned."""
    rows = verify_committed(assets)
    document = {"schema": MANIFEST_SCHEMA,
                "files": [{"name": name, "sha256": entry["sha256"], "bytes": path.stat().st_size,
                           "content_type": entry["content_type"]} for name, entry, path in rows],
                "bundle_sha256": digests(assets)["bundle_sha256"],
                "toolchain": {"node": node, "npm": npm}}
    _receipts().validate_admin_assets(document)
    return document


def build_block(node, npm, assets=None):
    """The `admin_assets` block of `airk-build.json` schema 2."""
    block = {**digests(assets), "node": node, "npm": npm,
             "locks": {name: _file_digest(ROOT / name) for name in LOCKS}}
    return {key: block[key] for key in ("bundle_sha256", "inventory_sha256", "node", "npm", "locks")}


def block_from_inventory(document, assets=None):
    """Rebind a checked inventory (written by this script after `npm run check`) to the build receipt.

    The inventory must equal the one recomputed now from the committed bundle, so a stale or
    edited bundle, or an inventory from another tree, is refused."""
    if not isinstance(document, dict) or not isinstance(document.get("toolchain"), dict):
        raise RuntimeError("Invalid admin asset inventory")
    toolchain = document["toolchain"]
    if document != inventory_document(toolchain.get("node"), toolchain.get("npm"), assets):
        raise RuntimeError("Admin asset inventory differs from the committed bundle; rerun scripts/build-admin-assets.py")
    return build_block(toolchain["node"], toolchain["npm"], assets)


def _resolve(tool):
    found = shutil.which(tool)
    if found is None:
        raise SystemExit(f"Missing prerequisite: {tool}")
    return found


def run(command, cwd):
    """The one command runner (tests replace it). Fails fast with the command's exit code."""
    result = subprocess.run([_resolve(command[0]), *command[1:]], cwd=cwd, stdin=subprocess.DEVNULL)
    if result.returncode:
        raise SystemExit(f"{' '.join(command)} failed ({result.returncode})")


def capture(command):
    return subprocess.check_output([_resolve(command[0]), *command[1:]], text=True, encoding="utf-8",
                                   stdin=subprocess.DEVNULL).strip()


def check(output, skip_install=False, runner=None, capturer=None):
    """npm ci (unless skipped) and `npm run check`, then verify and write the inventory."""
    runner = run if runner is None else runner
    capturer = capture if capturer is None else capturer
    node, npm = capturer(["node", "--version"]), capturer(["npm", "--version"])
    if not skip_install:
        runner(["npm", "ci", "--ignore-scripts", "--no-audit", "--no-fund"], PACKAGE)
    runner(["npm", "run", "check"], PACKAGE)
    document = inventory_document(node, npm)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
    return document


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--output", type=Path, default=OUTPUT, help="Inventory path (default artifacts/admin/admin-assets.json)")
    parser.add_argument("--skip-install", action="store_true", help="Reuse the existing packages/admin node_modules")
    args = parser.parse_args(argv)
    document = check(args.output.absolute(), args.skip_install)
    block = digests()
    print(f"Admin assets: {len(document['files'])} files, inventory {block['inventory_sha256']}, "
          f"bundle {block['bundle_sha256']} (node {document['toolchain']['node']}, npm {document['toolchain']['npm']})")
    print(f"Inventory: {args.output.absolute()}")


if __name__ == "__main__":
    sys.exit(main())
