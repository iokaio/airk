# SPDX-License-Identifier: Apache-2.0
"""Retain exact shipped v1 assets for explicitly separate ORT1.30 evaluation."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source',type=Path,default=ROOT/'target/local-vector-model');parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts'):parser.error('fresh private output required')
    catalog_path=ROOT/'crates/airk-retrieval/src/model_catalog_v1.json'
    retained={Path(__file__):Path(__file__).read_bytes(),catalog_path:catalog_path.read_bytes()};catalog=json.loads(retained[catalog_path])
    sha=lambda raw:hashlib.sha256(raw).hexdigest();rows=[]
    output.mkdir(parents=True)
    for native in catalog['profiles']:
        if native['id'] not in ('minilm-l6-v2-u8-v1','minilm-l6-v2-s8-v1'):raise ValueError('unexpected historical native profile')
        folder=output/('u8' if '-u8-' in native['id'] else 's8');folder.mkdir();source=args.source/native['id']
        for asset in native['assets']:
            path=source/asset['name'];retained[path]=path.read_bytes()
            if len(retained[path])!=asset['bytes'] or sha(retained[path])!=asset['sha256']:raise ValueError('historical shipped asset differs from native compatibility catalog')
            (folder/asset['name']).write_bytes(retained[path])
        manifest=source/'model.json';retained[manifest]=manifest.read_bytes()
        if json.loads(retained[manifest])!=native['manifest']:raise ValueError('historical shipped manifest differs from native compatibility catalog')
        (folder/'model.json').write_bytes(retained[manifest])
        origin={'original_catalog_id':native['id'],'original_catalog_runtime':native['runtime'],'actual_evaluation_runtime':'onnxruntime-cpu/1.30.0','scope':'Exact shipped artifact bytes, private controlled evaluation only. This does not enable production v1 execution under the new runtime or alter persisted index identities.'}
        (folder/'evaluation-origin.json').write_text(json.dumps(origin,indent=2)+'\n');rows.append({**origin,'assets':native['assets']})
    if any(path.read_bytes()!=raw for path,raw in retained.items()):raise ValueError('baseline asset/catalog/runner drift')
    report={'schema':'airk.development-original-shipped-encoder-baselines/1','profiles':rows,'input_seals':{str(path):sha(raw) for path,raw in retained.items()},'scope':'Original shipped bytes only; separate native evaluation descriptors and separately indexed same-pipeline fixtures still required. No human final scores or production v1 relaxation.'}
    (output/'runner.py').write_bytes(retained[Path(__file__)]);(output/'compatibility-catalog-as-consumed.json').write_bytes(retained[catalog_path]);(output/'receipt.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'profiles':len(rows),'receipt_sha256':sha((output/'receipt.json').read_bytes())}))


if __name__=='__main__':main()
