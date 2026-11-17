# SPDX-License-Identifier: Apache-2.0
"""Copy native synthetic scope-roster metadata; creates no Core/I1 request."""
import argparse
import hashlib
import json
from pathlib import Path

def merge(sources,pins,output):
    if len(sources)!=len(pins) or len(sources)!=2:raise ValueError('exact main and fault native source rosters required')
    roots=[];proof=[]
    for path,pin in zip(sources,pins):
        path=Path(path);raw=path.read_bytes()
        if hashlib.sha256(raw).hexdigest()!=pin:raise ValueError('original native synthetic roster changed')
        value=json.loads(raw)
        if value['schema']!='airk.private-selected-synthetic-source-scopes/1' or len(value['source_roots'])!=1:
            raise ValueError('actual single-root native synthetic metadata required')
        root=Path(value['source_roots'][0]).resolve()
        if root in roots:raise ValueError('main/fault roots must remain distinct')
        roots.append(root);proof.append({'path':str(path.resolve()),'sha256':pin})
    result={'schema':'airk.private-composed-native-synthetic-roster/1','source_roots':list(map(str,roots)),
            'original_native_scope_rosters':proof,'scope':'Only source-root metadata for existing native live Configure emitter. That owner observes and emits all real index/source/publication/enrollment identities; no Core request constructed here.'}
    with Path(output).open('x',encoding='utf-8') as stream:stream.write(json.dumps(result,indent=2)+'\n')

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--scope',type=Path,action='append',required=True);p.add_argument('--sha256',action='append',required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();merge(a.scope,a.sha256,a.output)
