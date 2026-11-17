# SPDX-License-Identifier: Apache-2.0
"""Read-only exact rmcp crate-member proof; no extraction/import/network."""
import hashlib
import json
from pathlib import Path
import tarfile
import tomllib
ROOT=Path(__file__).resolve().parents[4]
ARCHIVE='artifacts/security-rmcp-2026-11-09/rmcp-3.9.0.crate'
SHA='d24a7e92f4bcaa36b97fa93c4fa7ca8f3e669eced949345b3462dba333c7f909'
MEMBERS={'rmcp-3.9.0/tests/test_with_python/client.py':'522e2401016ad7d87efa6b13a113c618cc1c1ee568b8f51bd1b92a605668b0ab','rmcp-3.9.0/tests/test_with_python/server.py':'d27a404deb187a66f6b521d2deea4743bb8dc1e64179f2bf2e3a6cfb58aaa4d9'}
MEMBERS.update({'rmcp-3.9.0/tests/test_with_js/client.js':'1f1b26431e26706dc2d380f9fc3475bc78ada1c0c0ebe7302c1c59267a64b123','rmcp-3.9.0/tests/test_with_js/server.js':'760f0721c1b9f44272cc3416a71172a9163044c12d2b58310c72d3286081df81','rmcp-3.9.0/tests/test_with_js/streamable_client.js':'af4ccaf89ec0b34cda1b2af71edf9acd04924410a4ed4f579a0ccbd1360ec18a','rmcp-3.9.0/tests/test_with_js/streamable_server.js':'01074613ae942d99028c3f31b1ff35e64817c6de30ab8beabeb3823cb97e7498'})
def pin(path):
    raw=(ROOT/path).read_bytes()
    return {'path':path,'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw)}
def main():
    archive=pin(ARCHIVE)
    if archive['sha256']!=SHA or archive['bytes']!=677294:raise ValueError('reviewed archive changed')
    lock=pin('Cargo.lock');entries=tomllib.loads((ROOT/'Cargo.lock').read_text())['package']
    match=[p for p in entries if p['name']=='rmcp' and p['version']=='3.9.0']
    if len(match)!=1 or match[0]['checksum']!=SHA or match[0]['source']!='registry+https://github.com/rust-lang/crates.io-index':raise ValueError('registry package/checksum binding changed')
    helper=pin(Path(__file__).resolve().relative_to(ROOT).as_posix());rows=[]
    with tarfile.open(ROOT/ARCHIVE,'r:gz') as tar:
        for member,expected in MEMBERS.items():
            entries=[e for e in tar.getmembers() if e.name==member]
            if len(entries)!=1 or not entries[0].isfile() or entries[0].size>4096:raise ValueError('bounded unique regular archive member required')
            raw=tar.extractfile(entries[0]).read();source='artifacts/security-rmcp-2026-11-09/source/'+member
            if raw!=(ROOT/source).read_bytes() or hashlib.sha256(raw).hexdigest()!=expected:raise ValueError('upstream member bytes differ')
            rows.append({'source':source,'source_sha256':expected,'source_bytes':len(raw),'disposition':'upstream_build_input','representation':'raw','reason':'Exact in-memory crate member equality; registry archive SHA equals the actual Cargo.lock package checksum. No code imported or extracted.','upstream':{'repository':'https://crates.io/crates/rmcp','revision':'3.9.0','path':member,'source_sha256':expected},'evidence':[archive,lock,helper]})
    print(json.dumps({'schema':'airk.source-disposition/1','qualification':False,'files':rows},indent=2))
if __name__=='__main__':main()
