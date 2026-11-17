# SPDX-License-Identifier: Apache-2.0
"""Capture deduplicated exact recovery metadata on stdout; no vendor writes."""
import argparse
import hashlib
import io
import json
import tarfile
import zipfile
import project as owner
from project_patches import selected_members
from recovery import create,recover
DEST='scripts/arbiter/research/ort_dependency_recovery_r1'

def capture(path,expected):
    receipt,receipt_pin=owner.document(path,expected)
    if receipt.get('schema')!='airk.private.ort-patch-replay/1':raise ValueError('wrong diagnostic schema')
    inputs=[r for r in receipt['files'] if not r['exact']]
    if len(inputs)>100 or len({r['source'] for r in inputs})!=len(inputs):raise ValueError('source roster differs')
    groups={}
    for row in inputs:groups.setdefault(row['archive']['path'],[]).append(row)
    outputs={};entries=[]
    def output(leaf,body):
        if leaf in outputs and outputs[leaf]!=body.decode('utf-8'):raise ValueError('output identity conflict')
        outputs[leaf]=body.decode('utf-8')
        return {'path':DEST+'/'+leaf,'sha256':hashlib.sha256(body).hexdigest(),'bytes':len(body)}
    for archive,rows in sorted(groups.items()):
        pin=rows[0]['archive']
        if any(r['archive']!=pin for r in rows):raise ValueError('archive identity conflict')
        raw,actual_pin=owner.authenticated_bytes(archive,pin['sha256'])
        if actual_pin!=pin:raise ValueError('archive metadata differs')
        tar=raw[:2]==b'\x1f\x8b'
        originals=selected_members(raw,{r['member'] for r in rows},tar)
        notice_pins=[]
        with (tarfile.open(fileobj=io.BytesIO(raw),mode='r:gz') if tar else zipfile.ZipFile(io.BytesIO(raw))) as package:
            for item in (package.getmembers() if tar else package.infolist()):
                if (not item.isfile()) if tar else item.is_dir():continue
                name=item.name if tar else item.filename;pieces=name.split('/')
                if len(pieces)!=2 or not pieces[1].upper().startswith(('LICENSE','COPYING','NOTICE','AUTHORS')):continue
                size=item.size if tar else item.file_size
                if size>1<<20:raise ValueError('notice exceeds bound')
                with (package.extractfile(item) if tar else package.open(item)) as stream:body=stream.read((1<<20)+1)
                if len(body)!=size:raise ValueError('notice size differs')
                notice=output('notices/'+hashlib.sha256(body).hexdigest()+'.txt',body)
                notice_pins.append({'archive_member':name,**notice})
        if not notice_pins:raise ValueError('upstream root notice unavailable')
        for row in rows:
            base=originals[row['member']]
            if hashlib.sha256(base).hexdigest()!=row['original_member_sha256']:raise ValueError('base member differs')
            target,target_pin=owner.authenticated_bytes(row['source'],row['source_sha256'])
            if len(target)!=row['source_bytes'] or max(len(base),len(target))>1<<20:raise ValueError('target bytes/bound differs')
            record=create(base,target)
            inserted=sum(len(__import__('base64').b64decode(r['insert_base64'])) for r in record['edits'])
            if inserted>=len(target):raise ValueError('recovery would retain entire vendor target')
            body=owner.canonical(record);record_pin=output('records/'+hashlib.sha256(body).hexdigest()+'.json',body)
            if recover(base,record)!=target:raise ValueError('roundtrip differs')
            entries.append({'source':row['source'],'source_sha256':target_pin['sha256'],'source_bytes':len(target),'dependency':row['dependency'],'legacy':row['legacy'],'base':{'archive':pin,'member':row['member'],'sha256':record['base_sha256'],'bytes':len(base)},'recovery':record_pin,'inserted_bytes':inserted,'notices':notice_pins,'historical_upstream_patches':row['declared_chain']})
    manifest={'schema':'airk.observed-source-recovery/1','scope':'Exact observed dependency changes, attribution unchanged; no build selection or historical execution claim.','diagnostic':receipt_pin,'files':entries,'code':[owner.file_pin('scripts/arbiter/analysis/upstream_ort/'+n) for n in ['capture_recovery.py','recovery.py','project.py','project_patches.py']],'whole_directory_coverage':False}
    output('source-recovery.json',owner.canonical(manifest))
    readme="""# Observed ORT dependency recovery

These compact records preserve exact observed dependency changes whose historical patch application was not reproduced by the strict diagnostic. They do not assert AIRK authorship, alter attribution or select build inputs. The original archive, member, base bytes and observed target are independently SHA-256 pinned in source-recovery.json.

Each deduplicated record stores only replaced byte ranges and inserted bytes. scripts/arbiter/analysis/upstream_ort/recovery.py authenticates the entire base, each deletion and the entire result, without fuzzy matching. Reproduce in memory with recover(base_member_bytes, json.loads(record_bytes)). Obtain the exact base member from the pinned archive. Neither an archive nor a full vendor source copy is included here.

Original upstream license, copyright and author notices from authenticated archives are retained under notices/ and linked per source. These notices continue to govern the corresponding upstream code and changes; this archive does not relicense them. Earlier upstream patches remain provenance evidence, not proof that a historical command ran.

Capture: python -S -B scripts/arbiter/analysis/upstream_ort/capture_recovery.py --receipt <diagnostic.json> --receipt-sha256 <SHA256>. This emits metadata on stdout only; guarded materialization is separate. Qualification and full-directory coverage are not claimed.
"""
    output('README.md',readme.encode())
    for path,pin in dict(owner.CHECKED).items():
        if owner.file_pin(path)!=pin:raise ValueError('capture input changed')
    return {'schema':'airk.recovery-capture/1','destination':DEST,'files':outputs,'sources':len(entries),'records':sum(p.startswith('records/') for p in outputs),'roundtrip_verified':True}

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--receipt',required=True);parser.add_argument('--receipt-sha256',required=True);args=parser.parse_args();print(json.dumps(capture(args.receipt,args.receipt_sha256),indent=2))
