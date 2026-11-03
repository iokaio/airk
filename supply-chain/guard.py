# SPDX-License-Identifier: Apache-2.0
"""Enforce reviewed dependency source identities in addition to cargo-vet criteria.

No updates, downloads, exemption regeneration or certification occur here.
The initial inventory is an explicitly unreviewed existing-dependency baseline;
changing it requires source-review evidence alongside cargo-vet audit evidence.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]
MAX_BYTES = 4 * 1024 * 1024
FIELDS = {"name", "version", "source", "checksum"}
MAX_MEMBERS = 128


def local_manifest(root, path):
    """Admit a real, in-workspace path, never a symlink/junction alias."""
    root = root.resolve()
    absolute = Path(os.path.abspath(path))
    if not absolute.is_relative_to(root) or path.resolve() != absolute:
        raise ValueError("First-party source path escapes the workspace or uses an alias")
    for part in (absolute, *absolute.parents):
        if part == root:
            break
        if part.is_symlink() or (hasattr(part, "is_junction") and part.is_junction()):
            raise ValueError("First-party source path uses a linked alias")
    return absolute


def validate_workspace_paths(root, manifest, first_party, version):
    """Check AIRK's closed workspace shape, not a second Cargo resolver.

    Supported membership is explicit relative directories or a final /* glob.
    Excludes, nested workspaces and other glob forms require a policy-code review.
    All local dependency declarations must name an approved member manifest.
    Cargo remains responsible for resolving external dependencies and features.
    """
    root = root.resolve()
    table = manifest["workspace"]
    members = table.get("members")
    if ("package" in manifest or table.get("exclude") or table.get("default-members")
            or not isinstance(members, list) or not members or len(members) > MAX_MEMBERS):
        raise ValueError("Unsupported closed workspace membership; explicit source review required")
    approved, manifests = {}, [(root / "Cargo.toml", manifest)]
    for name, relative in first_party.items():
        if (not isinstance(name, str) or not isinstance(relative, str)
                or Path(relative).is_absolute() or ".." in Path(relative).parts):
            raise ValueError("First-party source path escapes the workspace or is invalid")
        path = local_manifest(root, root / relative)
        if path.name != "Cargo.toml" or path in approved:
            raise ValueError("Duplicate or invalid first-party manifest path")
        data = tomllib.loads(bounded(path).decode("utf-8"))
        package = data.get("package", {})
        if (not isinstance(package, dict) or package.get("name") != name
                or package.get("version") not in (version, {"workspace": True})
                or "workspace" in data or "workspace" in package):
            raise ValueError(f"First-party package identity changed: {name}")
        approved[path] = name
        manifests.append((path, data))
    expanded = set()
    for member in members:
        if not isinstance(member, str) or not re.fullmatch(r"[A-Za-z0-9_-]+(?:/[A-Za-z0-9_-]+)*(?:/\*)?", member):
            raise ValueError("Unsupported workspace member path; explicit source review required")
        if member.endswith("/*"):
            directory = local_manifest(root, root / member[:-2])
            candidates = directory.iterdir()
        else:
            candidates = [root / member]
        for index, candidate in enumerate(candidates):
            if index >= MAX_MEMBERS:
                raise ValueError("Workspace membership exceeds source-policy bound")
            candidate = local_manifest(root, candidate)
            if candidate.is_dir():
                expanded.add(candidate / "Cargo.toml")
    if expanded != set(approved):
        raise ValueError("Effective workspace members differ from the recorded first-party manifest inventory")
    for path, data in manifests:
        containers = [data]
        if path == root / "Cargo.toml":
            containers.append(table)
        targets = data.get("target", {})
        if not isinstance(targets, dict):
            raise ValueError("Malformed target dependency tables")
        containers.extend(targets.values())
        dependency_tables = []
        for container in containers:
            if not isinstance(container, dict):
                raise ValueError("Malformed dependency container")
            dependency_tables.extend(container.get(key, {}) for key in
                                     ("dependencies", "dev-dependencies", "build-dependencies"))
        patches = data.get("patch", {})
        if not isinstance(patches, dict):
            raise ValueError("Malformed patch tables")
        dependency_tables.extend(patches.values())
        dependency_tables.append(data.get("replace", {}))
        for dependencies in dependency_tables:
            if not isinstance(dependencies, dict):
                raise ValueError("Malformed dependency table")
            for alias, dependency in dependencies.items():
                if isinstance(dependency, dict) and "path" in dependency:
                    relative = dependency["path"]
                    if not isinstance(relative, str) or not relative or Path(relative).is_absolute():
                        raise ValueError("Invalid local dependency path")
                    target = local_manifest(root, path.parent / relative / "Cargo.toml")
                    name = dependency.get("package", alias.split(":", 1)[0])
                    if target not in approved or approved[target] != name:
                        raise ValueError("Unapproved local/path dependency manifest or package identity")


def bounded(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_BYTES:
        raise ValueError(f"Invalid source-policy input: {path.name}")
    with path.open("rb") as stream:
        data = stream.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise ValueError(f"Source-policy input exceeds bound: {path.name}")
    return data


def identity(row):
    if not isinstance(row, dict) or set(row) != FIELDS:
        raise ValueError("Dependency source identity has unknown or missing fields")
    if any(not isinstance(row[key], str) or not row[key] or len(row[key]) > 4096
           for key in ("name", "version", "source")):
        raise ValueError("Invalid dependency name, version or source")
    if row["source"].startswith("registry+"):
        if not isinstance(row["checksum"], str) or not re.fullmatch(r"[0-9a-f]{64}", row["checksum"]):
            raise ValueError("Registry source requires its exact SHA-256 checksum")
    elif row["source"].startswith("git+"):
        if row["checksum"] is not None or not re.search(r"#[0-9a-f]{40}$", row["source"]):
            raise ValueError("Git source requires an exact commit identity, not registry checksum equivalence")
    else:
        raise ValueError("Unsupported dependency source kind; explicit source review required")
    return tuple(row[key] for key in ("name", "version", "source", "checksum"))


def validate_lock_sources(lock_path=ROOT / "Cargo.lock", policy_path=None):
    """Return complete inventory counts, or raise on an unapproved source change."""
    lock_path = Path(lock_path)
    root = lock_path.parent
    policy_path = Path(policy_path) if policy_path else root / "supply-chain/source-policy.json"
    lock_bytes, policy_bytes = bounded(lock_path), bounded(policy_path)
    lock = tomllib.loads(lock_bytes.decode("utf-8"))
    policy = json.loads(policy_bytes)
    if (not isinstance(policy, dict) or policy.get("schema") != "airk.dependency-source-policy/1"
            or not isinstance(policy.get("packages"), list) or not isinstance(policy.get("first_party"), dict)
            or not isinstance(lock.get("package"), list) or type(lock.get("version")) is not int or lock["version"] != 4):
        raise ValueError("Unsupported dependency source policy or Cargo.lock format")
    approved = set()
    for row in policy["packages"]:
        value = identity(row)
        if value in approved:
            raise ValueError("Duplicate dependency source approval")
        approved.add(value)
    workspace_manifest = tomllib.loads(bounded(root / "Cargo.toml").decode("utf-8"))
    workspace_table = workspace_manifest.get("workspace", {})
    if not isinstance(workspace_table, dict):
        raise ValueError("Workspace metadata is malformed")
    workspace = workspace_table.get("package", {})
    if not isinstance(workspace, dict) or not isinstance(workspace.get("version"), str):
        raise ValueError("Workspace version metadata is missing or malformed")
    first_party = policy["first_party"]
    validate_workspace_paths(root, workspace_manifest, first_party, workspace["version"])
    actual, local, counts = set(), set(), {"registry": 0, "git": 0}
    changes = []
    for package in lock["package"]:
        if not isinstance(package, dict) or not isinstance(package.get("name"), str) or not isinstance(package.get("version"), str):
            raise ValueError("Malformed package metadata in Cargo.lock")
        if "source" not in package:
            if package["name"] not in first_party or package["version"] != workspace["version"]:
                changes.append(f"Unapproved local/path package: {package['name']} {package['version']}")
            if package["name"] in local:
                raise ValueError("Duplicate local package identity")
            local.add(package["name"])
            continue
        row = {key: package.get(key) for key in FIELDS}
        value = identity(row)
        if value in actual:
            raise ValueError("Duplicate Cargo.lock source identity")
        actual.add(value)
        counts["git" if row["source"].startswith("git+") else "registry"] += 1
        if value not in approved:
            changes.append(f"Dependency source review required: {row['name']} {row['version']} ({row['source']})")
    if local != set(first_party):
        changes.append("Locked workspace package set differs from the recorded first-party source inventory")
    if changes:
        raise ValueError("\n".join(changes))
    return {"schema": "airk.dependency-source-check/1", "conclusion": "success",
            "packages": len(actual), "registry_packages": counts["registry"], "git_packages": counts["git"],
            "first_party_packages": len(local), "unused_approvals": len(approved - actual),
            "cargo_lock_sha256": hashlib.sha256(lock_bytes).hexdigest(),
            "policy_sha256": hashlib.sha256(policy_bytes).hexdigest(),
            "baseline_is_security_audit": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lockfile", type=Path, default=ROOT / "Cargo.lock")
    parser.add_argument("--policy", type=Path)
    args = parser.parse_args()
    try:
        result = validate_lock_sources(args.lockfile, args.policy)
    except (ValueError, KeyError, TypeError) as error:
        print(json.dumps({"schema": "airk.dependency-source-check/1", "conclusion": "failure", "error": str(error)}))
        return 1
    except OSError as error:
        print(json.dumps({"schema": "airk.dependency-source-check/1", "conclusion": "unavailable", "error": str(error)}))
        return 2
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
