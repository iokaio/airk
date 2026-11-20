# SPDX-License-Identifier: Apache-2.0
"""Retire exactly the verified disposable smoke extraction and staged installer archive after their D handoff."""
import argparse
import json
from pathlib import Path

from configure_ort import BASE, REPO, sha
from product_projection import checked_path
from shipping_build import document, space
from verified_inputs import identity


def reclaim(args):
    uid = identity()
    native = document(args.native,args.native_sha256)
    installer = document(args.installer,args.installer_sha256)
    handoff = document(args.handoff,args.handoff_sha256)
    build = document(args.build,args.build_sha256)
    if (not all(x["success"] is True for x in (build,native,installer))
            or native["stage"] != "native" or installer["stage"] != "standalone"
            or not all(x["original_build"] == args.build_sha256 for x in (native,installer))
            or handoff["original"].lower() != args.installer_sha256 or handoff["files"] != installer["publish"]):
        raise ValueError("the completed native package and installer handoff evidence differ")
    source = BASE/"linux-cuda-service-source-r1/source/artifacts"
    native_dir = checked_path(Path(native["output"]),True)
    install_dir = checked_path(Path(installer["output"]),True)
    if native_dir != source/"linux-cuda-shipping-native-r1" or install_dir != source/"linux-cuda-shipping-standalone-r2":
        raise ValueError("unexpected native scratch output root")
    handoff_root = REPO/handoff["destination"]
    if (handoff_root != REPO/"artifacts/standalone/linux-cuda-descriptive-1.8.207-r1"
            or handoff_root != handoff_root.resolve(strict=True) or handoff_root.stat().st_uid != uid):
        raise ValueError("unexpected installed archive handoff root")
    for name,row in handoff["files"].items():
        p = handoff_root/name
        if p.parent != handoff_root or p != p.resolve(strict=True) or not p.is_file() or p.stat().st_uid != uid or sha(p) != row["sha256"]:
            raise ValueError("retained D product differs: "+name)
    tar = checked_path(install_dir/"publish/airk-1.8.207-x86_64-unknown-linux-gnu-installer.tar.gz")
    if sha(tar) != installer["publish"][tar.name]["sha256"]:
        raise ValueError("native installed archive differs from its preserved D copy")
    smoke = checked_path(native_dir/"smoke",True)
    files, dirs = {}, []
    for item in smoke.rglob("*"):
        key = item.relative_to(smoke).as_posix()
        if item.is_dir():
            dirs.append(checked_path(item,True))
        else:
            path = checked_path(item)
            files[key] = {"bytes":path.stat().st_size,"sha256":sha(path)}
    if files != build["runtime_files"]:
        raise ValueError("native smoke extraction differs from the complete original pair")
    before = space()
    for name in files:
        (smoke/name).unlink()
    for path in sorted(dirs,key=lambda p:len(p.parts),reverse=True):
        path.rmdir()
    smoke.rmdir()
    tar.unlink()
    return {"uid":uid,"removed_smoke_files":len(files),"removed_smoke_bytes":sum(r["bytes"] for r in files.values()),
            "removed_tar_bytes":installer["publish"][tar.name]["bytes"],"native":args.native_sha256,
            "installer":args.installer_sha256,"retained_d_handoff":args.handoff_sha256,"before":before,"after":space()}


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ("build","native","installer","handoff"):
        parser.add_argument("--"+name,type=Path,required=True)
        parser.add_argument("--"+name+"-sha256",required=True)
    print(json.dumps(reclaim(parser.parse_args()),sort_keys=True))
