# SPDX-License-Identifier: Apache-2.0
"""Additive SHA-only grader correction; original22export/91training sources untouched."""
from pathlib import Path
import difflib,hashlib,json
HERE=Path(__file__).resolve().parent
PARENT=HERE.parent/"native-parent-bag-export-source-r1/grade_dev.py"

def main():
    output=HERE/"grade_dev.py"
    if output.exists():raise ValueError("fresh additive grader required")
    original=PARENT.read_text()
    old="def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()"
    new='''def sha(p):
    digest=hashlib.sha256()
    with Path(p).open("rb") as stream:
        for block in iter(lambda:stream.read(1024*1024),b""):digest.update(block)
    return digest.hexdigest()'''
    if original.count(old)!=1:raise ValueError("exact original SHA helper required")
    amended=original.replace(old,new)
    # Keep the original frozen source directory's output-isolation check exact.
    declaration='ROOT=Path(__file__).resolve().parents[4];HERE=Path(__file__).resolve().parent;MODELS=ROOT/"artifacts/advanced-split/models"'
    if amended.count(declaration)!=1:raise ValueError("exact original source-root declaration required")
    amended=amended.replace(declaration,'ROOT=Path(__file__).resolve().parents[4];MODELS=ROOT/"artifacts/advanced-split/models";HERE=MODELS/"native-parent-bag-export-source-r1"')
    output.write_text(amended)
    (HERE/"grader-streaming.patch").write_text("".join(difflib.unified_diff(original.splitlines(True),amended.splitlines(True),fromfile="frozen22/grade_dev.py",tofile="additive-streaming/grade_dev.py")))
    (HERE/"source-review.json").write_text(json.dumps({"parent":str(PARENT),"parent_sha256":hashlib.sha256(PARENT.read_bytes()).hexdigest(),
        "amended_sha256":hashlib.sha256(output.read_bytes()).hexdigest(),"changes":["stream every SHA input with fixed1MiB reads","retain original frozen source-root output-isolation boundary despite additive file location"],
        "wall_seconds":60,"host_bytes":268435456,"all64_20quality_44refusals":True,"grade_logic_or_floors_changed":False,"NN":False},indent=2)+"\n")
    print((HERE/"source-review.json").read_text())

if __name__=="__main__":main()
