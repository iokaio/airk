# SPDX-License-Identifier: Apache-2.0
"""One reviewed Source mount set; only private materialized copies are written."""
from pathlib import Path
import hashlib,json,shutil
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
META=HERE.parent/"native-minilm-production-decision-r1/metadata"

def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def main():
    output=HERE/"coherent-source-r1"
    if output.exists():raise ValueError("fresh coherent private source set required")
    rows={}
    for prefix in [HERE/"rust",META,HERE/"consumers"]:
        for source in prefix.rglob('*'):
            if not source.is_file():continue
            name=source.relative_to(prefix).as_posix()
            # Only the current baseline049a catalog includes the metadata hooks.
            if prefix==META and name=="crates/airk-retrieval/src/model_catalog.rs":continue
            # Consumer fullcopy predates new fence tests: merge its two ID
            # literals into metadata copy, never overwrite those tests.
            if prefix==HERE/"consumers" and name=="crates/airk-retrieval/src/model/object_metadata_tests.rs":continue
            rows[name]=source.read_text()
    name="crates/airk-retrieval/src/model/object_metadata_tests.rs"
    rows[name]=rows[name].replace("minilm-l6-v2-u8-v2","minilm-l6-v2-u8-v3").replace("minilm-l6-v2-s8-v2","minilm-l6-v2-s8-v3")
    rows["crates/airk-retrieval/src/model_catalog/CONTRACT.md"]=(HERE/"CURRENT-CATALOG-CONTRACT.md").read_text()
    name="crates/airk-kernel/src/current/enrichment/CONTRACT.md"
    rows[name]=(ROOT/name).read_text()+(HERE/"ENRICHMENT-CONTRACT-addition.md").read_text()
    files=[]
    for name,text in rows.items():
        path=output/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(text)
        target=ROOT/name
        files.append({"source":str(path),"target":name,"source_sha256":sha(path),
                      "target_before_sha256":sha(target) if target.is_file() else None})
    manifest={"schema":"airk.private-original-minilm-coherent-source-mount/1",
        "recorded_decision_sha256":"cef9af82caf81a6bd755161ffb8a00943d954cf7110eeb822ba775789a4433c2",
        "files":files,"phase_A":"Root scoped Retrieval Clippy +single pure baseline emitter; no Host bundled compile until actual emitted catalog/policy and fresh v3 assets are materialized",
        "native_JSON_not_yet_emitted":["crates/airk-retrieval/src/model_catalog.json","fixtures/models/model-policy.json"],
        "legal_inventory":"Separate legal-consumers-manifest.json must land before packaging, not needed to execute pure catalog emitter",
        "tests_or_Cargo_or_NN_executed":False,"public_writes":False}
    path=HERE/"coherent-source-mount-r1.json";path.write_text(json.dumps(manifest,indent=2)+"\n")
    print(json.dumps({"manifest":str(path),"sha256":sha(path),"files":len(files),"catalog_sha256":sha(output/"crates/airk-retrieval/src/model_catalog.rs")}))

if __name__=="__main__":main()
