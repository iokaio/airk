# SPDX-License-Identifier: Apache-2.0
"""Package the measured native GNU runtime under the original producer pins and physical volume reservation."""
import argparse
import json
import os
from pathlib import Path
import subprocess

from cargo_gate import ENV_PIN
from configure_ort import BASE, CAMPAIGN, CPU, CPU_SHA, REPO, audit, sha
from product_projection import checked_path
from shipping_build import CORE, PROJECTION, document, monitored
from shipping_source import owner
from unpack import directory
from verified_inputs import identity

PACKAGER_REV = "de57cebb3a84311e5cc72ff7d189dad6a985dce3"
TARGET = "x86_64-unknown-linux-gnu"


def packager(root):
    name = "scripts/package-native.py"
    path = checked_path(root/name)
    def blob(rev):
        return subprocess.check_output(["/usr/bin/git","-C",str(REPO),"show",rev+":"+name],timeout=20)
    reviewed, prior = blob(PACKAGER_REV), blob(PACKAGER_REV+"^")
    if path.read_bytes().replace(b"\r\n",b"\n") not in (reviewed,prior):
        raise ValueError("native packager source differs from the two reviewed revisions")
    if path.read_bytes() != reviewed:
        path.write_bytes(reviewed)
    checked_path(path)
    if path.read_bytes() != reviewed:
        raise ValueError("reviewed native packager projection differs")
    return sha(path)


def run(args):
    uid = identity()
    os.umask(0o077)
    receipt = document(args.build,args.build_sha256)
    if receipt["success"] is not True or receipt["product_projection"] != PROJECTION or sha(CPU) != CPU_SHA:
        raise ValueError("verified original native pair required")
    source = document(CAMPAIGN/"linux-cuda-shipping-source-r2.json",receipt["source_receipt"])
    root = checked_path(Path(source["source"]),True)
    runtime = checked_path(Path(receipt["output"]),True)
    if (runtime != BASE/"linux-cuda-descriptive-service-r3/runtime" or source["identity"] != receipt["source"]
            or owner(root)["source_identity"]() != receipt["source"]):
        raise ValueError("original pair or build source differs")
    selected = packager(root)
    if owner(root)["source_identity"]() != receipt["source"]:
        raise ValueError("separate packaging tool change modified original pair identity")
    files = receipt["runtime_files"]
    for name,row in files.items():
        path = checked_path(runtime/name)
        if path.stat().st_size != row["bytes"] or sha(path) != row["sha256"]:
            raise ValueError("original Linux runtime file changed: " + name)
    size = sum(row["bytes"] for row in files.values())
    actual = {p.relative_to(runtime).as_posix() for p in runtime.rglob("*") if p.is_file()}
    if actual != set(files):
        raise ValueError("runtime roster changed")
    native = document(CAMPAIGN/"linux-cuda-product-projection-r1.json",PROJECTION)
    pins = ["--cuda-environment",str(CAMPAIGN/"linux-cuda-ort-final-core-r1.json"),"--cuda-environment-sha256",CORE,
            "--cuda-projection",native["projection"]["path"],"--cuda-projection-sha256",native["projection"]["sha256"],
            "--cuda-native-execution-receipt",native["execution"]["path"],
            "--cuda-native-execution-receipt-sha256",native["execution"]["sha256"]]
    if not 1 <= args.attempt <= 99:
        raise ValueError("invalid attempt")
    suffix = args.stage+"-r"+str(args.attempt)
    stage = BASE/("linux-cuda-shipping-"+suffix)
    output = root/"artifacts"/("linux-cuda-shipping-"+suffix)
    if stage.exists() or output.exists():
        raise ValueError("fresh native package receipt and output required")
    checked_path(BASE,True)
    stage.mkdir(0o700)
    checked_path(stage,True)
    directory(output.parent,root,uid)
    if args.stage == "native":
        script = root/"scripts/package-native.py"
        specific = ["--runtime-dir",str(runtime),"--target",TARGET]
        reserve = size*2 + (64 << 20)
    else:
        evidence = document(args.native,args.native_sha256)
        if evidence["success"] is not True or evidence["original_build"] != args.build_sha256:
            raise ValueError("original verified native archive required")
        prior = checked_path(Path(evidence["output"])/"publish",True)
        for name,record in evidence["publish"].items():
            if sha(checked_path(prior/name)) != record["sha256"]:
                raise ValueError("original native publication bytes changed")
        script = root/"scripts/package-standalone-linux.py"
        if sha(script) != "2275b3bfaf5fb37f41fd1133b34a03dcaf16d7b04a7e5e52e48eef3a7b0e43fc":
            raise ValueError("standalone packager differs from the reviewed streaming source")
        smoke = checked_path(Path(evidence["output"])/"smoke",True)
        specific = ["--native-dir",str(prior),"--verified-runtime-dir",str(smoke)]
        reserve = size + (64 << 20)
    cmd = ["/usr/bin/python3","-B","-S",str(script),*specific,"--output",str(output),*pins]
    env = document(CAMPAIGN/"linux-driver-fix-checks-r1/gate-preflight.json",ENV_PIN)["environment"]
    env.update(HOME=str(BASE.parents[1]),LC_ALL="C",CARGO_NET_OFFLINE="true",CARGO_BUILD_JOBS="1",
               CARGO_INCREMENTAL="0",RAYON_NUM_THREADS="1",PYTHONDONTWRITEBYTECODE="1")
    proc,samples,stopped,success = monitored(cmd,root,stage,env,reserve,uid, address_space=(2 if args.stage == "standalone" else 8) << 30)
    if output.exists():
        audit(output,uid)
    publish = output/"publish"
    return {"success":success,"stage":args.stage,"original_build":args.build_sha256,"packager_revision":PACKAGER_REV,
            "package_native_sha256":selected,"selected_packager_sha256":sha(script),"source":receipt["source"],
            "command":proc,"stopped_on_space":stopped,"volume_observations":len(samples),"first":samples[0],
            "last":samples[-1],"volumes_sha256":sha(stage/"volumes.json"),"log_sha256":sha(stage/"build.log"),
            "output":str(output),"publish":({p.name:{"bytes":p.stat().st_size,"sha256":sha(checked_path(p))}
            for p in publish.iterdir()} if success else {})}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage",choices=("native","standalone"))
    parser.add_argument("--attempt",type=int,default=1)
    parser.add_argument("--build",type=Path,required=True)
    parser.add_argument("--build-sha256",required=True)
    parser.add_argument("--native",type=Path)
    parser.add_argument("--native-sha256")
    print(json.dumps(run(parser.parse_args()),sort_keys=True))
