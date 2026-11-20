# SPDX-License-Identifier: Apache-2.0
"""Stage unchanged original legal notices from the authenticated Linux producer inputs."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import pwd
import re
import runpy
import stat
import tarfile
import zipfile

from configure_ort import BASE, CAMPAIGN, CONFIG_SHA, CPU, CPU_SHA, audit, sha, source_path
from unpack import guarded


def legal_name(name):
    return bool(re.fullmatch(r"(?i)(?:(?:(?:licen[cs]e|copying|notice|acknowledg(?:e)?ments?|licensing)(?:[._-].*)?)|third[_ -]?party[_ -]?(?:notices|licenses)(?:[._-].*)?)", name))


def collect(out):
    uid = os.getuid()
    if uid != 1000 or os.geteuid() != uid or pwd.getpwuid(uid).pw_name != "tyler":
        raise ValueError("registered non-root Linux human required")
    os.umask(0o077)
    if out.parent != BASE or os.path.lexists(out) or sha(CPU) != CPU_SHA:
        raise ValueError("fresh private notices root and pinned owner required")
    for path in (BASE.parents[1], BASE.parent, BASE):
        guarded(path, uid, stat.S_ISDIR)
    spec = CAMPAIGN / "fresh-cuda-configure-cu15-r1.json"
    nv = CAMPAIGN / "linux-cuda-build-inputs-r1.json"
    if sha(spec) != CONFIG_SHA or sha(nv) != "b24ddaa3c8c69f731cc1a6c79d2026eff01c9f9f62bb9c73652c84219954ab3a":
        raise ValueError("unknown producer source/redistribution receipts")
    repo = json.loads(spec.read_bytes())
    packages = json.loads(nv.read_bytes())
    pins = {"onnxruntime":repo["source_archive"]["sha256"],
            **{name: row["sha256"] for name,row in repo["dependency_archives"].items()}}
    original = []
    cpu = runpy.run_path(str(CPU))
    root = BASE / "linux-cuda-ort-r3"
    for row, content in cpu["notice_items"](BASE/"inputs/static-ort/downloads","x86_64-unknown-linux-gnu",root/"source",root/"source/mirror"):
        name = row.get("archive","onnxruntime")
        original.append((name,row["path"],content))
    for name in ("cutlass","cudnn_frontend"):
        archive = repo["dependency_archives"][name]
        path = source_path(archive["path"])
        cpu["verify_file"](path,archive)
        with zipfile.ZipFile(path) as zipped:
            for item in zipped.infolist():
                relative = PurePosixPath(item.filename)
                if item.is_dir() or not legal_name(relative.name):
                    continue
                if relative.is_absolute() or ".." in relative.parts or item.file_size > 2 << 20:
                    raise ValueError("invalid original CUDA source notice")
                content, _ = cpu["zip_notice_content"](zipped,item)
                original.append((name,relative.as_posix(),content))
    acquisition = CAMPAIGN/"linux-cuda-redistributions-r1/acquisition.json"
    if sha(acquisition) != "07f177d5faf4b365d1d1a9351b62b8123e96572cc6a2362fb1cec44375fb6b5c":
        raise ValueError("CUDA archive acquisition differs")
    dist = {row["name"]: row for row in json.loads(acquisition.read_bytes())["archives"]}
    for row in packages["packages"]:
        archive_name = row["name"]
        source = dist[archive_name]
        name = archive_name.split("-linux-x86_64-",1)[0]
        if name in pins or source["sha256"] != row["sha256"]:
            raise ValueError("ambiguous or changed CUDA component")
        pins[name] = row["sha256"]
        folder = BASE/"cuda-build-inputs-r1/notices"/name
        guarded(folder,uid,stat.S_ISDIR)
        staged = {p.name:p for p in folder.iterdir() if legal_name(p.name)}
        matched = set()
        archive = acquisition.parent/archive_name
        with archive.open("rb") as stream:
            if os.fstat(stream.fileno()).st_size != source["bytes"] or hashlib.file_digest(stream,"sha256").hexdigest() != source["sha256"]:
                raise ValueError("original NVIDIA archive does not match its publisher pin")
            stream.seek(0)
            with tarfile.open(fileobj=stream,mode="r|xz") as tar:
                for member in tar:
                    relative = PurePosixPath(member.name)
                    if len(relative.parts) != 2 or not legal_name(relative.name):
                        continue
                    if (relative.parts[0] != archive_name.removesuffix(".tar.xz") or not member.isfile()
                            or not 0 < member.size <= 2 << 20 or relative.name in matched):
                        raise ValueError("invalid NVIDIA notice archive member")
                    path = staged.get(relative.name)
                    if path is None:
                        raise ValueError("original legal member missing from staged package")
                    guarded(path,uid,stat.S_ISREG)
                    with tar.extractfile(member) as inf:
                        content = inf.read((2<<20)+1)
                    if len(content) != member.size or path.read_bytes() != content:
                        raise ValueError("staged notice differs from authenticated NVIDIA archive member")
                    matched.add(relative.name)
                    original.append((name,relative.as_posix(),content))
        if not matched or matched != set(staged):
            raise ValueError("staged notice inventory differs from authentic archive")
    if {k for k,_,_ in original} != set(pins) or len(original)>512 or sum(len(b) for _,_,b in original)>16 << 20:
        raise ValueError("missing or excessive original Linux producer notices")
    out.mkdir(0o700)
    records = []
    for component, origin, data in sorted(original):
        slug = hashlib.sha256(origin.encode()).hexdigest()[:16]
        leaf = re.sub(r"[^a-z0-9_.-]", "-", PurePosixPath(origin).name.lower())[:50].strip(".")
        ext = "" if leaf.endswith((".txt",".html",".md")) else ".txt"
        name = f"airk-linux-cuda-notice-{component}-{slug}-{leaf}{ext}"
        with (out/name).open("xb") as f:
            f.write(data)
        records.append({"component":component,"archive_sha256":pins[component],"original_path":origin,
                        "staged_name":name,"bytes":len(data),"sha256":sha(out/name)})
    audit(out, uid)
    return {"originals":records}


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output",type=Path,required=True)
    print(json.dumps(collect(p.parse_args().output),sort_keys=True))
