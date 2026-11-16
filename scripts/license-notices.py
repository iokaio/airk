# SPDX-License-Identifier: Apache-2.0
"""Create pinned dependency, model and research notices; never local corpora."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
from urllib.parse import quote


MODEL_COMPANIONS = {
    'minilm-LICENSE.txt': ('LICENSE', 'cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30'),
}


# Maintained legal texts only. This is not a Cargo dependency/data inventory.
RESEARCH_COMPANIONS = {
    'third_party/research/ARBITER-RESEARCH-NOTICES.md': ('research/ARBITER-RESEARCH-NOTICES.md', '5fdfc53e09b8d11306371ddb1697481a4a8060dc6008aa0b22e3d5beb39956e3'),
    'third_party/research/CoCLR-MIT-LICENSE.txt': ('research/CoCLR-MIT-LICENSE.txt', '5f1ce9c03c594b38df127d923515ae50c5999076e376f01cb94d1b369c54d243'),
    'third_party/research/C-UDA-1.0.md': ('research/C-UDA-1.0.md', '69aeb2c407f2ffce7f31c3aba0f1eeffac248009ec2070fcd989586ac8229c82'),
    'third_party/research/CodeSearchNet-MIT-LICENSE.txt': ('research/CodeSearchNet-MIT-LICENSE.txt', '5ba1fd8a344040f2698ed3234aeb8f4b3e85211aa54a37048021f3eb0043be22'),
    'third_party/research/provenance.json': ('research/provenance.json', 'c01b711abf5d647ced19b516ace19ed83a53a0897d8c215963b9368ea8e7b56c'),
    'third_party/models/minilm-v2-TRAINING-NOTICES.md': ('models/minilm-v2-TRAINING-NOTICES.md', '2daba18214e777ecb68e1154c26eb9d50c9f8ef25895e4dcc8af3d9d1275e4be'),
    'third_party/models/minilm-v2-LICENSE.txt': ('models/minilm-v2-LICENSE.txt', '9cc7fbd69f78ff19a188025976df20d5c17b0cec5ed79ad5d7e16d2094e46abd'),
    'third_party/models/minilm-v2-provenance.json': ('models/minilm-v2-provenance.json', '6f6a29bff30ef64212b0c6fc56095d94df3184856e488023820d995fbb68da81'),
    'third_party/models/CC-BY-4.0.txt': ('models/CC-BY-4.0.txt', '9ba9550ad48438d0836ddab3da480b3b69ffa0aac7b7878b5a0039e7ab429411'),
    'third_party/research/ContextBench-Apache-2.0-LICENSE.txt': ('research/ContextBench-Apache-2.0-LICENSE.txt', 'c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4'),
    'third_party/research/TechQA-CDLA-Permissive-1.0.pdf': ('research/TechQA-CDLA-Permissive-1.0.pdf', 'e12775d6b5bfdba0bb33fd691dab386f0c7573853fdc4785c40b39739f120d5f'),
}


def stage_model_notices(root, output):
    """Retain complete original-model rights text without any private inputs."""
    catalog_path = root / 'crates/airk-retrieval/src/model_catalog.json'
    catalog_bytes = catalog_path.read_bytes()
    catalog = json.loads(catalog_bytes)
    verified = {}
    for source_name, (destination, expected) in MODEL_COMPANIONS.items():
        source = root / 'third_party/models' / source_name
        if source.is_symlink() or not source.is_file():
            raise ValueError('Missing regular model notice: ' + source_name)
        content = source.read_bytes()
        if hashlib.sha256(content).hexdigest() != expected:
            raise ValueError('Model notice digest mismatch: ' + source_name)
        verified[destination] = content
    license_hash = hashlib.sha256(verified['LICENSE']).hexdigest()
    if not catalog['profiles'] or any(profile['manifest']['license_sha256'] != license_hash
                                      for profile in catalog['profiles']):
        raise ValueError('Bundled model license digest mismatch')
    destination = output / 'all-MiniLM-L6-v2'
    destination.mkdir(parents=True, exist_ok=True)
    for name, content in verified.items():
        (destination / name).write_bytes(content)
    (destination / 'provenance.json').write_bytes(catalog_bytes)
    return catalog



def stage_research_notices(root, output):
    """Authenticate all maintained research/legal bytes before staging any."""
    verified = {}
    for source_name, (destination, expected) in RESEARCH_COMPANIONS.items():
        source = root / source_name
        if source.is_symlink() or not source.is_file():
            raise ValueError('Missing regular research notice: ' + source_name)
        content = source.read_bytes()
        if hashlib.sha256(content).hexdigest() != expected:
            raise ValueError('Research notice digest mismatch: ' + source_name)
        verified[destination] = content
    for name, content in verified.items():
        destination = output / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(content)
    return research_notice_inventory()


def research_notice_inventory():
    """Exact current runtime legal paths; separate from generated Cargo rows."""
    return {'notices/' + name: digest for name, digest in RESEARCH_COMPANIONS.values()}


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
    stage_model_notices(Path('.'), args.output)
    stage_research_notices(Path('.'), args.output)
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
