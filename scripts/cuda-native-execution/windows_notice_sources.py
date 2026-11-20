# SPDX-License-Identifier: Apache-2.0
"""Verify Windows CU15 source-archive notice bytes. Prints private evidence only."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy
import tarfile
import zipfile
import re
from pathlib import PurePosixPath

ROOT = Path(__file__).resolve().parents[2]
REQUEST = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z/fresh-cuda-configure-cu15-r1.json"
CPU = ROOT / "scripts/provision-static-onnx-runtime.py"
REQUEST_SHA = "d10eb3ced57d17901e3749f05864e1f230b0d8988558a3ddaa98d72f61c1a863"
CPU_SHA = "a1772271cfc3e585de903a7e80021b54f97da823a485e48621a60dcee564c044"


def digest(path):
    if path.resolve(strict=True) != path or not path.is_file():
        raise ValueError(f"noncanonical notice source {path}")
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def pinned(item):
    path = Path(item["path"])
    if path.stat().st_size != item["bytes"] or digest(path) != item["sha256"]:
        raise ValueError(f"changed source {path}")
    return path


def gpu_notices(request, owner):
    records = []
    def add(component, path, content, origin):
        records.append(({"component":component,"path":path,"source_artifact_sha256":origin},content))
    legal = re.compile(r"(?i)(?:licen[cs]e|copying|notice|acknowledg(?:e)?ments?|licensing|third[_ -]?party[_ -]?(?:notices|licenses))(?:[._-].*)?")
    for component in ("cutlass","cudnn_frontend"):
        item = request["dependency_archives"][component]
        with zipfile.ZipFile(pinned(item)) as archive:
            for member in archive.infolist():
                relative = PurePosixPath(member.filename)
                if member.is_dir() or not legal.fullmatch(relative.name):
                    continue
                if relative.is_absolute() or ".." in relative.parts or not 0 < member.file_size <= 2 << 20:
                    raise ValueError("invalid source notice")
                data, _ = owner["zip_notice_content"](archive,member)
                add(component,member.filename,data,item["sha256"])
    version, toolkit_inventory, extraction = request["native_inputs"][:3]
    version_path = pinned(version)
    pinned(toolkit_inventory)
    original = json.loads(pinned(extraction).read_bytes())
    source_dir = Path(extraction["path"]).parent
    archive_pin = original["source_archive"]
    published = original["rights"]["archive_license"]
    with zipfile.ZipFile(pinned(archive_pin)) as archive:
        if archive.namelist().count(published["archive_member"]) != 1:
            raise ValueError("ambiguous original cuDNN notice")
        data = archive.read(published["archive_member"])
        if len(data) != published["bytes"] or hashlib.sha256(data).hexdigest() != published["sha256"]:
            raise ValueError("cuDNN archive license differs")
        add("cudnn",published["archive_member"],data,archive_pin["sha256"])
    metadata = source_dir/"redist-LICENSE.txt"
    if digest(metadata) != original["rights"]["metadata_license_sha256"]:
        raise ValueError("cuDNN distribution metadata license differs")
    add("cudnn","redistribution-metadata/redist-LICENSE.txt",metadata.read_bytes(),archive_pin["sha256"])
    toolkit = source_dir/"cuda-toolkit-notices"
    if digest(toolkit/"inventory.json") != "b0ab371c4eb927ef896ab77d688c770bf7bd6fc80dfb833752afedfd6d1f0fec":
        raise ValueError("toolkit notice inventory differs")
    for item in json.loads((toolkit/"inventory.json").read_bytes()):
        leaf = item["path"]
        if Path(leaf).name != leaf or Path(item["source"]) != version_path.parent/leaf:
            raise ValueError("unexpected toolkit source")
        data = pinned(dict(item,path=item["source"])).read_bytes()
        if pinned(dict(item,path=str(toolkit/leaf))).read_bytes() != data:
            raise ValueError("toolkit notice copy differs")
        if leaf in {"EULA.txt","LICENSE","README"}:
            add("cuda",leaf,data,toolkit_inventory["sha256"])
    if {row["component"] for row,_ in records} != {"cuda","cudnn","cutlass","cudnn_frontend"}:
        raise ValueError("incomplete NVIDIA and source dependency notices")
    return records, original["rights"]


def verify(stage=None):
    if digest(REQUEST) != REQUEST_SHA or digest(CPU) != CPU_SHA:
        raise ValueError("CU15 acquisition or CPU notice owner changed")
    request = json.loads(REQUEST.read_bytes())
    original = request["source_archive"]
    source = Path(original["path"])
    if source.stat().st_size != original["bytes"] or digest(source) != original["sha256"]:
        raise ValueError("original ONNX Runtime distribution changed")
    base = source.parents[1]
    expected = {"onnxruntime": original["sha256"]}
    for component, item in request["dependency_archives"].items():
        if component in {"cutlass", "cudnn_frontend"}:
            continue
        archive = Path(item["path"])
        if archive != base/"downloads"/(component+".zip") or archive.stat().st_size != item["bytes"] or digest(archive) != item["sha256"]:
            raise ValueError(f"original CU15 archive differs for {component}")
        expected[component] = item["sha256"]
    owner = runpy.run_path(str(CPU))
    observed = owner["notice_items"](base, "x86_64-pc-windows-msvc")
    # The CPU owner reads these two files in the original extracted source.
    # Authenticate their published bytes directly against the pinned source tar.
    originals = {}
    with tarfile.open(source,"r|gz") as bundle:
        for member in bundle:
            names = member.name.split("/")
            if len(names) == 2 and names[1] in {"LICENSE","ThirdPartyNotices.txt"}:
                if names[1] in originals or not member.isfile() or not 0 < member.size <= 2 << 20:
                    raise ValueError("ambiguous original ONNX Runtime notice")
                originals[names[1]] = bundle.extractfile(member).read()
    if set(originals) != {"LICENSE","ThirdPartyNotices.txt"}:
        raise ValueError("original source notices missing")
    originals_to_stage = []
    rows = []
    for row, data in observed:
        component = row.get("archive","onnxruntime")
        if component == "onnxruntime" and originals[Path(row["path"]).name] != data:
            raise ValueError("extracted ONNX Runtime notice differs from original archive")
        if hashlib.sha256(data).hexdigest() != row["sha256"]:
            raise ValueError("notice digest changed")
        rows.append(dict(row,component=component,source_artifact_sha256=expected[component]))
        originals_to_stage.append((rows[-1],data))
    if {row["component"] for row in rows} != set(expected):
        raise ValueError("dependency notice coverage differs")
    extra, rights = gpu_notices(request,owner)
    originals_to_stage.extend(extra)
    files = {}
    for row, content in originals_to_stage:
        component, origin = row["component"], row["path"]
        suffix = hashlib.sha256(origin.encode()).hexdigest()[:16]
        leaf = re.sub(r"[^a-z0-9._-]+","-",PurePosixPath(origin).name.lower())
        name = f"airk-cuda-notice-{component}-{suffix}-{leaf}"
        if not name.endswith((".txt",".md",".html")):
            name += ".txt"
        if name in files or len(name)>128:
            raise ValueError("ambiguous package notice destination")
        files[name] = (dict(row,package_name=name,bytes=len(content),sha256=hashlib.sha256(content).hexdigest()),content)
    if stage is not None:
        if stage.resolve(strict=True) != stage or not stage.is_relative_to(REQUEST.parent) or any(stage.iterdir()):
            raise ValueError("prechecked empty campaign notice stage required")
        for name, (row,content) in files.items():
            with (stage/name).open("xb") as dest:
                dest.write(content)
            if digest(stage/name) != row["sha256"]:
                raise ValueError("staged notice changed")
    return {"request_sha256":REQUEST_SHA,"notice_owner_sha256":CPU_SHA,"originals":[r for r,_ in files.values()],
            "cudnn_rights":rights,"provenance_only":True,"qualification":False,"activation":False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage",type=Path)
    parser.add_argument("--native-inventory",action="store_true",help="emit the native legal-owner array")
    args = parser.parse_args()
    evidence = verify(args.stage)
    if args.native_inventory:
        evidence = [{key:row[key] for key in ("component","package_name","bytes","sha256","source_artifact_sha256")}
                    for row in evidence["originals"]]
    print(json.dumps(evidence,sort_keys=True,allow_nan=False))
