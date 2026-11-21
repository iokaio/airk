# SPDX-License-Identifier: Apache-2.0
"""Acquire exact existing IBM annotation members and release documentation into the checked private root."""
import argparse
import hashlib
import json
from pathlib import Path
from urllib.request import Request, urlopen

HERE = Path(__file__).resolve().parent
CAMPAIGN = HERE.parents[3]/"artifacts/v190/final-plan/exec-20261116T052942Z"
OUT = CAMPAIGN/"mtrag-cloud-human-annotations-r1"
REV = "e0970de1b1207c2a9506adf238e6e1a22037e38a"
BASE = f"https://raw.githubusercontent.com/IBM/mt-rag-benchmark/{REV}/"
TREES = f"https://api.github.com/repos/IBM/mt-rag-benchmark/git/trees/{REV}?recursive=1"
SCREEN = "779c51e74f6e73c6ff6c59749c1978b47f1ee2894f0044007e2d184e785c75f0"
META = "2bd2a972bf385d0732af3fa24a716b9f580e0ca8ec4887a2fcb47d8846493ee5"
RELEASES = ("mtrag-human","mtragun-human")


def require(ok, msg):
    if not ok:
        raise ValueError(msg)


def pinned(path, expected):
    data = path.read_bytes()
    require(hashlib.sha256(data).hexdigest() == expected,"local source digest differs")
    return data


def original_receipts():
    a,b = [json.loads(pinned(CAMPAIGN/f"techqa-style-ibm-mtrag-cloud-{leaf}-r1.json",sha))
           for leaf,sha in (("screen",SCREEN),("source-metadata",META))]
    require(a["repository_revision"] == REV and a["selected_by_release"] == {"mtrag-human":21,"mtragun-human":4}
            and b["screening_receipt"]["sha256"] == SCREEN and b["bound_cases"] == 25,"different frozen screen")
    return a,b


def get(url, limit):
    request = Request(url,headers={"User-Agent":"AIRK-retained-candidate-evidence"})
    with urlopen(request,timeout=25) as response:
        require(response.geturl() == url,"publisher redirected")
        data = response.read(limit+1)
    require(len(data)<=limit,"release member exceeds bound")
    return data


def acquire(corpus=False):
    screen, meta = original_receipts()
    require(OUT.is_dir() and OUT.resolve() == OUT,"checked private root required")
    manifest = OUT/("passage-sources.json" if corpus else "annotation-sources.json")
    require(not manifest.exists(),"preserve completed source manifest")
    specs = {"cloud.jsonl.zip":meta["official_cloud_archive"]} if corpus else {
        "LICENSE":screen["sources"]["repository_license"],
        **{release+".jsonl":meta["publisher_reference_pins"][release] for release in RELEASES}}
    if not corpus:
        tree = json.loads(get(TREES,2<<20))
        require(tree["truncated"] is False,"incomplete original repository tree")
        entries = {e["path"]:e for e in tree["tree"] if e["type"]=="blob"}
        for name in ("README.md","mtrag-human/README.md","mtragun-human/README.md","corpora/README.md"):
            if name not in entries and name == "corpora/README.md":
                continue
            item = entries[name]
            require(item["mode"] == "100644" and 0 < item["size"] <= 256<<10,"unexpected documentation object")
            specs[name.replace("/","_")] = {"url":BASE+name,"bytes":item["size"],"git_blob_sha":item["sha"]}
    retained = {}
    for name,spec in specs.items():
        url,extent = spec["url"],spec["bytes"]
        require(url.startswith(BASE) and 0 < extent <= 32<<20,"different release member")
        path = OUT/name
        data = path.read_bytes() if path.exists() else get(url,extent)
        sha = hashlib.sha256(data).hexdigest()
        oid = hashlib.sha1(f"blob {len(data)}\0".encode()+data).hexdigest()
        require(len(data)==extent and sha==spec.get("sha256",sha) and oid==spec.get("git_blob_sha",oid),
                "publisher reference or Git object differs")
        if not path.exists():
            with path.open("xb") as stream:
                stream.write(data)
        retained[name] = {"path":str(path),"bytes":extent,"sha256":sha,"git_blob_sha":oid,"url":url}
    with manifest.open("x",encoding="utf-8") as stream:
        json.dump({"revision":REV,"screen":SCREEN,"metadata":META,"members":retained},stream,indent=2)
    print(json.dumps({"bytes":sum(x["bytes"] for x in retained.values()),"members":len(retained),
                      "sha256":hashlib.sha256(manifest.read_bytes()).hexdigest()}))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--corpus",action="store_true")
    acquire(p.parse_args().corpus)
