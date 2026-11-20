# SPDX-License-Identifier: Apache-2.0
"""Stage the Rust-issued Linux product descriptor and exact existing scientific ELF/legal bytes."""
import argparse
import json
import os
from pathlib import Path
import stat
import subprocess

from configure_ort import BASE, audit, sha
from product_projection import MANIFEST, checked_path, image_proof, pin, write
from stage_payload import copy_checked
from unpack import guarded
from verified_inputs import identity


def run(a):
    uid = identity()
    os.umask(0o077)
    root = checked_path(BASE / "linux-cuda-package-r1/runtime", True)
    image = checked_path(root / "airk-inference-scientific")
    scientific = checked_path(root / MANIFEST)
    proof = image_proof(a.image_proof, a.image_proof_sha256, pin(image))
    if sha(scientific) != proof["manifest_sha256"]:
        raise ValueError("original scientific image and manifest differ")
    if sha(a.producer) != a.producer_sha256:
        raise ValueError("GNU Cargo producer receipt differs")
    producer = json.loads(a.producer.read_bytes())
    if (producer["stage"] != "producer" or producer["success"] is not True
            or producer["reports"]["example"]["path"] != str(BASE / "target/debug/examples/linux_cuda_provenance")
            or producer["reports"]["example"]["sha256"] != a.emitter_sha256):
        raise ValueError("actual native Cargo example differs from preserved producer")
    source = a.emitter
    original = BASE / "target/debug/examples/linux_cuda_provenance"
    retained = BASE / "target/airk-linux-cuda-producers" / (a.emitter_sha256 + "-linux_cuda_provenance")
    if source != original and (source != retained or producer["reports"].get("retained_example") !=
                               {"path":str(retained),"sha256":a.emitter_sha256}):
        raise ValueError("GNU manifest author has no retained Cargo build identity")
    if source.resolve(strict=True) != source:
        raise ValueError("unexpected Cargo example path")
    checked_path(source.parent, True)
    guarded(source, uid, stat.S_ISREG)
    if sha(source) != a.emitter_sha256:
        raise ValueError("expected checked Linux manifest author")
    out = BASE / ("linux-cuda-product-r" + str(a.attempt))
    if not 1 <= a.attempt <= 99 or out.exists():
        raise ValueError("fresh product staging required")
    out.mkdir(0o700)
    target = out/"runtime"
    target.mkdir(0o700)
    emitter = out / "linux_cuda_provenance"
    copy_checked(source, emitter, a.emitter_sha256, uid)
    emitter.chmod(0o700)
    checked_path(emitter)
    private = {"HOME":str(BASE.parents[1]), "LC_ALL":"C", "PATH":"/usr/bin:/bin", "TMPDIR":str(BASE/"tmp")}
    command = [str(emitter), "--empty-product", str(scientific), proof["manifest_sha256"]]
    result = subprocess.run(command, check=True, capture_output=True, env=private, cwd=out, timeout=30)
    if len(result.stdout) > 128 << 10 or not result.stdout or sha(emitter) != a.emitter_sha256:
        raise ValueError("product manifest author output differs")
    with (target / MANIFEST).open("xb") as f:
        f.write(result.stdout)
    rows = {}
    for p in sorted(root.iterdir()):
        checked_path(p)
        if p.name == MANIFEST:
            continue
        original = pin(p)
        copy_checked(p, target / p.name, original["sha256"], uid)
        if p == image:
            (target / p.name).chmod(0o700)
        else:
            rows[p.name] = original
    audit(out, uid)
    issued = pin(target / MANIFEST)
    if issued["sha256"] == proof["manifest_sha256"]:
        raise ValueError("product and scientific manifest identities must be distinct")
    receipt = {"command":command, "emitter_sha256":a.emitter_sha256, "scientific":pin(scientific),
               "image_proof_sha256":a.image_proof_sha256,"producer_sha256":a.producer_sha256,"product":issued,
               "image":pin(target / image.name),"held_originals":rows}
    write(out / "stage.json",receipt)
    return {"stage":pin(out/"stage.json"),"image":receipt["image"],"product":issued}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--image-proof", type=Path, required=True)
    p.add_argument("--image-proof-sha256", required=True)
    p.add_argument("--producer", type=Path, required=True)
    p.add_argument("--producer-sha256", required=True)
    p.add_argument("--emitter", type=Path, required=True)
    p.add_argument("--emitter-sha256", required=True)
    p.add_argument("--attempt", type=int, default=1)
    print(json.dumps(run(p.parse_args()),sort_keys=True))
