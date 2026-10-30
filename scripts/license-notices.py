# SPDX-License-Identifier: Apache-2.0
"""Create a third-party license bundle from the locked Cargo graph, not local corpora."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
from urllib.parse import quote


def declaration_provenance(source, record):
    """Authenticate a pinned publisher declaration; this does not supply legal text."""
    if not record or not record.get("declaration"):
        return {}
    manifest = source / "Cargo.toml.orig"
    if not manifest.is_file():
        manifest = source / "Cargo.toml"
    if hashlib.sha256(manifest.read_bytes()).hexdigest() != record["declaration"]["sha256"]:
        raise ValueError("Pinned publisher declaration digest mismatch: " + record["name"])
    return {key: record[key] for key in ("revision", "revision_basis", "declaration", "standalone_text_review",
                                         "release_disposition") if key in record}


def release_disposition(record, version):
    """A maintainer decision for exactly this package version and pinned revision, or None.
    A changed dependency never inherits an approval recorded for another version."""
    disposition = (record or {}).get("release_disposition")
    if not disposition:
        return None
    if (disposition.get("decision") != "accepted-publisher-declaration" or disposition.get("version") != version
            or disposition.get("revision") != record.get("revision") or not disposition.get("date")
            or not disposition.get("decided_by") or not disposition.get("evidence")):
        return None
    return disposition


def software_bill_of_materials(metadata):
    """CycloneDX 1.6 inventory of Cargo's resolved build graph, including dev/host crates."""
    purls = {package["id"]: "pkg:cargo/" + quote(package["name"], safe="") + "@" + quote(package["version"], safe="")
             for package in metadata["packages"]}
    # Cargo IDs for workspace/path dependencies contain the build machine's absolute path.
    # Use public package identity plus a source discriminator, never those local paths.
    references = {package["id"]: purls[package["id"]] + "#" + hashlib.sha256(
        (package.get("source") or "workspace").encode()).hexdigest()[:16] for package in metadata["packages"]}
    components = []
    for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"], p["id"])):
        component = {"type": "library", "bom-ref": references[package["id"]], "name": package["name"],
                     "version": package["version"], "purl": purls[package["id"]]}
        if package.get("license"):
            component["licenses"] = [{"expression": package["license"]}]
        components.append(component)
    dependencies = [{"ref": references[node["id"]], "dependsOn": sorted({references[dep["pkg"]] for dep in node["deps"]})}
                    for node in (metadata.get("resolve") or {}).get("nodes", [])]
    return {"$schema": "http://cyclonedx.org/schema/bom-1.6.schema.json", "bomFormat": "CycloneDX",
            "specVersion": "1.6", "version": 1,
            "metadata": {"properties": [{"name": "airk:inventory-scope", "value":
                         "Cargo resolved build graph including workspace, development, build and platform-conditional crates; not a linked-binary or container OS inventory"}]},
            "components": components, "dependencies": dependencies}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--release", action="store_true",
                        help="fail when a package without separate license text has no matching maintainer disposition")
    args = parser.parse_args()
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--format-version", "1", "--locked"
    ], text=True, encoding="utf-8"))
    workspace_ids = set(metadata["workspace_members"])
    pinned_path=Path("third_party/licenses/sources.json")
    pinned_records = {}
    if pinned_path.exists():
        for record in json.loads(pinned_path.read_text(encoding="utf-8")):
            pinned_records[(record["name"], record["version"])] = record
            for item in record["files"]:
                path=pinned_path.parent/(record["name"]+"-"+record["version"])/item["file"]
                if hashlib.sha256(path.read_bytes()).hexdigest()!=item["sha256"]:
                    raise SystemExit("Pinned license digest mismatch: "+str(path))
    args.output.mkdir(parents=True, exist_ok=True)
    inventory = []
    missing = []
    unresolved = []
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
            record = pinned_records.get((package["name"], package["version"]))
            declaration.update(declaration_provenance(source, record))
            disposition = release_disposition(record, package["version"])
            declaration["release_review"] = "accepted" if disposition else "unresolved"
            (destination / "UPSTREAM-LICENSE-DECLARATION.json").write_text(json.dumps(declaration, indent=2) + "\n", encoding="utf-8")
            if disposition:
                print(f"License declaration accepted for release on {disposition['date']}: {package['name']} {package['version']}")
            else:
                unresolved.append(package["name"] + " " + package["version"])
                print("Release review needed for license declaration without separate text: " + package["name"])
    catalog = json.loads(Path("crates/airk-retrieval/src/model_catalog.json").read_text(encoding="utf-8"))
    profile = catalog["profiles"][0]
    model_notices = args.output / "all-MiniLM-L6-v2"
    model_notices.mkdir(exist_ok=True)
    license_bytes = Path("third_party/models/minilm-LICENSE.txt").read_bytes()
    if hashlib.sha256(license_bytes).hexdigest() != profile["manifest"]["license_sha256"]:
        raise SystemExit("Bundled model license digest mismatch")
    (model_notices / "LICENSE").write_bytes(license_bytes)
    (model_notices / "provenance.json").write_text(json.dumps(catalog, indent=2) + "\n", encoding="utf-8")
    (args.output / "inventory.json").write_text(json.dumps(inventory, indent=2) + "\n", encoding="utf-8")
    bom = software_bill_of_materials(metadata)
    runtime = json.loads(Path("crates/airk-retrieval/src/ort_runtime_catalog.json").read_text(encoding="utf-8"))
    reference = "pkg:nuget/Microsoft.ML.OnnxRuntime@" + runtime["distribution_version"]
    bom["components"].append({"type": "library", "bom-ref": reference, "purl": reference,
                              "name": "Microsoft.ML.OnnxRuntime", "version": runtime["distribution_version"],
                              "licenses": [{"license": {"id": "MIT"}}],
                              "externalReferences": [{"type": "distribution", "url": runtime["url"],
                                                      "hashes": [{"alg": "SHA-256", "content": runtime["archive_sha256"]}]}]})
    bom["dependencies"].append({"ref": reference, "dependsOn": []})
    bom["metadata"]["properties"][0]["value"] += "; plus pinned native ONNX Runtime CPU distribution"
    (args.output / "bom.cdx.json").write_text(json.dumps(bom, indent=2) + "\n", encoding="utf-8")
    if missing:
        raise SystemExit("Missing license declarations: " + ", ".join(missing))
    if args.release and unresolved:
        raise SystemExit("Release license review unresolved: " + ", ".join(unresolved))
    print(f"Bundled notices for {len(inventory)} third-party packages in {args.output}")


if __name__ == "__main__":
    main()
