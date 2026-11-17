# SPDX-License-Identifier: Apache-2.0
"""Read-only Git filter/raw-byte audit for explicit authenticated source attributes."""
import hashlib
import json
from pathlib import Path
import subprocess

def git(root,args,data=None):
    return subprocess.run(['git','-C',str(root),*args],input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=True).stdout

def audit(root):
    root=Path(root).resolve()
    attrs=(root/'.gitattributes').read_bytes()
    paths=sorted({line.split()[0][1:] for line in attrs.decode().splitlines() if line.startswith('/') and line.endswith(' -text')})
    expected={
        'scripts/cuda-native-execution/helpers/reference_owned.py':'2d7a31a64eaba745e32ed83adfbeedb75983ab6f9b5c4cc82714e9371c1895a1',
        'scripts/cuda-native-execution/helpers/reference_parent_no_site_r2.py':'fe4cc990649d92e041b79cccb3fd7ef1129979a26432b7f02b2d8d28b586ed8d',
        'scripts/cuda-native-execution/helpers/reference_worker.py':'9c04be28ef06c0a06b8e2cffd4c274348ff141046114cc493d725ce527b694c7',
    }; maps=[]
    owners=list((root/'scripts/arbiter').rglob('source-map.json'))+list((root/'scripts/arbiter').rglob('source-provenance.json'))+[root/'scripts/cuda-static-runtime/build/source-map.json']
    for owner in sorted(owners):
        raw=owner.read_bytes(); data=json.loads(raw)
        maps.append({'path':owner.relative_to(root).as_posix(),'sha256':hashlib.sha256(raw).hexdigest()})
        for row in data.get('files',[]):
            path=row.get('destination',row.get('proposed_target')); sha=row.get('maintained_sha256')
            if path and sha:
                if path in expected and expected[path]!=sha: raise ValueError('conflicting mapped target: '+path)
                expected[path]=sha
        for row in data.get('maintained_utilities',[]):
            expected[row['path']]=row['sha256']
    paths=sorted(set(paths)|set(expected))
    snapshots={p:(root/p).read_bytes() for p in paths if (root/p).is_file()}
    missing=[p for p in paths if p not in snapshots]
    fmt=git(root,['rev-parse','--show-object-format']).decode().strip()
    selected=list(snapshots)
    selections=git(root,['check-attr','-z','--stdin','filter'],('\0'.join(selected)+'\0').encode()).decode().split('\0')
    if selections[-1]!='' or len(selections)!=len(selected)*3+1:raise ValueError('partial Git filter attribute observation')
    for i,path in enumerate(selected):
        seen,attribute,value=selections[i*3:i*3+3]
        if seen!=path or attribute!='filter' or value not in ('unspecified','unset'):
            raise ValueError('external Git filter is not authorized for byte audit: '+path)
    filtered=git(root,['hash-object','--stdin-paths','--filters'],('\n'.join(selected)+'\n').encode()).decode().splitlines()
    if len(filtered)!=len(selected):raise ValueError('partial Git filter results')
    rows=[]
    for path,oid in zip(selected,filtered):
        raw=snapshots[path]; sha=hashlib.sha256(raw).hexdigest()
        if path in expected and sha!=expected[path]:raise ValueError('mapped target changed: '+path)
        calculated=hashlib.new(fmt,b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()
        if calculated!=oid:raise ValueError('Git filter changes authenticated bytes: '+path)
        rows.append({'path':path,'sha256':sha,'mapped_sha256':expected.get(path),'bytes':len(raw),'crlf':raw.count(b'\r\n'),'raw_git_oid':calculated,'filtered_git_oid':oid})
    for path,raw in snapshots.items():
        if (root/path).read_bytes()!=raw:raise ValueError('source drift during audit: '+path)
    if (root/'.gitattributes').read_bytes()!=attrs:raise ValueError('attributes drift during audit')
    for row in maps:
        if hashlib.sha256((root/row['path']).read_bytes()).hexdigest()!=row['sha256']:raise ValueError('map drift')
    return {'schema':'airk.git-authenticated-byte-audit/1','passed':True,'git_object_format':fmt,'attributes_sha256':hashlib.sha256(attrs).hexdigest(),'mapped_targets':len(expected),'present_attribute_paths':len(rows),'absent_attribute_paths':missing,'maps':maps,'files':rows,'commands':['git rev-parse --show-object-format','git hash-object --stdin-paths --filters'],'git_mutations':False}

if __name__=='__main__':
    print(json.dumps(audit(Path(__file__).resolve().parents[4]),indent=2))
