# SPDX-License-Identifier: Apache-2.0
"""Reject packaging until the entire Cargo build/test graph is SQLite-free."""
import argparse
import json
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def sqlite_packages(packages):
    return sorted({package["name"] for package in packages
                   if "sqlite" in package["name"].lower()})


def check(root=ROOT):
    locked = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
    forbidden = sqlite_packages(locked["package"])
    if forbidden:
        raise ValueError("Full redb cutover required before packaging; Cargo.lock retains: "
                         + ", ".join(forbidden))
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--offline", "--all-features", "--format-version", "1"],
        cwd=root, text=True, encoding="utf-8"))
    forbidden = sqlite_packages(metadata["packages"])
    if forbidden:
        raise ValueError("Cargo build/test graph retains SQLite: " + ", ".join(forbidden))
    if not any(package["name"] == "redb" for package in metadata["packages"]):
        raise ValueError("Resolved build graph does not include the selected redb engine")
    for manifest in [root / "Cargo.toml", *sorted((root / "crates").glob("*/Cargo.toml"))]:
        data = tomllib.loads(manifest.read_text(encoding="utf-8"))
        inspect_manifest(data, manifest.relative_to(root))
    return {"engine": "redb", "sqlite_free": True, "packages": len(metadata["packages"])}


def inspect_manifest(value, location):
    if not isinstance(value, dict):
        return
    for key, item in value.items():
        if key in ("dependencies", "dev-dependencies", "build-dependencies") and isinstance(item, dict):
            for alias, declaration in item.items():
                name = declaration.get("package", alias) if isinstance(declaration, dict) else alias
                if "sqlite" in name.lower():
                    raise ValueError(f"SQLite dependency remains in {location}: {alias}")
        inspect_manifest(item, location)


def main():
    argparse.ArgumentParser(description=__doc__).parse_args()
    try:
        print(json.dumps(check(), indent=2))
    except (ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error


if __name__ == "__main__":
    main()
