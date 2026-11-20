# SPDX-License-Identifier: Apache-2.0
"""Read-only CU15 absolute source-pin inventory for the product custody decision."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
CU15 = Path(os.environ["LOCALAPPDATA"]) / "airk-final-exec-20261116T052942Z/cu15"
GUARD_SHA = "fe47b26e5902372ef27ad7067bc3ba256699dbadbd9abefd04941bd138e0c68c"


def hashed(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def document(path, sha):
    if hashed(path) != sha:
        raise ValueError("changed inventory input "+str(path))
    return json.loads(path.read_bytes())


def visit(obj, owner, absolute, relatives):
    if isinstance(obj, dict):
        if isinstance(obj.get("path"), str) and isinstance(obj.get("sha256"), str):
            path = Path(obj["path"])
            if path.is_absolute():
                previous = absolute.setdefault(path, (obj["sha256"], set()))
                if previous[0] != obj["sha256"]:
                    raise ValueError("conflicting absolute pins "+str(path))
                previous[1].add(owner)
            else:
                relatives.add((str(path), obj["sha256"]))
        for value in obj.values():
            visit(value, owner, absolute, relatives)
    elif isinstance(obj, list):
        for value in obj:
            visit(value, owner, absolute, relatives)


def main():
    start = time.monotonic()
    environment = document(CU15/"environment.json", "05c074577ed80c5f2ac733a5775f71732ff631c890a78932083fddde9a7b4d5c")
    proof = document(CU15/"fresh-static-build-proof.json", "5618cca298c6472e5e5bab539ca27c23bce9282eb5e17b5866858f5a34bfbd4d")
    if proof != environment["build"]:
        raise ValueError("foreign environment/build")
    source = document(CAMPAIGN/"windows-product-notice-sources-r3/product-sources.json",
                      "31dc8ddd6bbe01895d3ceddc1405810738c4c205a22ef24c07b50bcae57dffae")
    docs = {"environment":environment, "build_proof":proof, "product_sources":source}
    for name in ("build_receipt","configure_receipt","configure_recipe"):
        pin = proof[name]
        docs[name] = document(Path(pin["path"]),pin["sha256"])
    pin = source["aggregate_receipt"]
    docs["aggregate"] = document(Path(pin["path"]),pin["sha256"])
    absolute, relatives = {}, set()
    for label, value in docs.items():
        visit(value,label,absolute,relatives)
    changed, failures = [], []
    total = 0
    for path, (expected, owners) in sorted(absolute.items()):
        try:
            if path.resolve(strict=True) != path or not path.is_file():
                raise ValueError("noncanonical or unavailable source")
            before = path.stat()
            actual = hashed(path)
            after = path.stat()
            if (before.st_size,before.st_mtime_ns,before.st_ino) != (after.st_size,after.st_mtime_ns,after.st_ino):
                raise ValueError("source changed while authenticating")
            total += after.st_size
            if actual != expected:
                changed.append({"path":str(path),"expected":expected,"actual":actual,"actual_bytes":after.st_size,
                                "documents":sorted(owners)})
        except (OSError, ValueError) as error:
            failures.append({"path":str(path),"error":str(error)})
    recipe = docs["configure_recipe"]
    historical = subprocess.check_output(["git","show",recipe["head"]+":scripts/cuda-component-execution.py"],cwd=ROOT)
    if hashlib.sha256(historical).hexdigest() != GUARD_SHA or recipe["owners"]["guards"]["sha256"] != GUARD_SHA:
        raise ValueError("historical Git bytes differ from original guard pin")
    report = {"schema":"airk.private.windows-product-absolute-pin-audit/1","sources":sorted(docs),"paths":len(absolute),
              "bytes_hashed":total,"matching":len(absolute)-len(changed)-len(failures),"changed":changed,"failures":failures,
              "relative_pins_outside_absolute_census":len(relatives),"guard_historical_commit":recipe["head"],
              "guard_historical_bytes":len(historical),"guard_historical_sha256":GUARD_SHA,
              "elapsed_seconds":time.monotonic()-start,"activation":False,"qualification":False}
    print(json.dumps(report,sort_keys=True))


if __name__ == "__main__":
    main()
