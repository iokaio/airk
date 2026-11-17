# SPDX-License-Identifier: Apache-2.0
"""Attach historical Git path labels to an authenticated source inventory; read-only."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

def links(report, labels):
    rows=[]
    for row in report['candidates']:
        category=row['classification']
        if category not in ('committed_git_blob_match','normalized_text_committed_blob_match'):continue
        raw=category=='committed_git_blob_match'
        oid=row['git_blob_oid'] if raw else row['normalized_text_git_blob_oid']
        if oid not in labels:raise ValueError('inventory Git identity absent from recorded HEAD')
        rows.append({'path':row['path'],'source_sha256':row['source_sha256'],'git_blob_oid':oid,
                     'identity':'raw_bytes' if raw else 'CRLF_to_LF_text_only','historical_path_label':labels[oid],
                     'reachable_from_head':report['head'],'current_owner_or_callers_proven':False})
    return rows

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--inventory',type=Path,required=True)
    p.add_argument('--inventory-sha256',required=True)
    a=p.parse_args();raw=a.inventory.read_bytes()
    if hashlib.sha256(raw).hexdigest()!=a.inventory_sha256:raise ValueError('inventory pin mismatch')
    report=json.loads(raw)
    repo=Path(__file__).resolve().parents[4]
    data=subprocess.run(['git','-C',str(repo),'rev-list','--objects',report['head']],check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE).stdout
    labels={}
    for line in data.decode('utf-8',errors='surrogateescape').splitlines():
        oid,sep,name=line.partition(' ')
        labels.setdefault(oid,name if sep else None)
    result={'schema':'airk.private.historical-source-links/1','inventory':str(a.inventory),'inventory_sha256':a.inventory_sha256,
            'head':report['head'],'command':['git','rev-list','--objects',report['head']],
            'limitation':'rev-list path labels are historical names, not proof of a particular commit-at-path or current maintained callers.',
            'rows':links(report,labels)}
    if a.inventory.read_bytes()!=raw:raise ValueError('inventory changed')
    print(json.dumps(result,indent=2))
if __name__=='__main__':main()
