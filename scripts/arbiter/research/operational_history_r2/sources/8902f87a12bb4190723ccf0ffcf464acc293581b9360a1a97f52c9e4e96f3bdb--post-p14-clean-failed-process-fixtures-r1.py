# SPDX-License-Identifier: Apache-2.0
"""Root-only cleanup of the four exact retained failed R1 synthetic fixtures.

No Fixture constructor, activation, source/data edits, uninstall or PID lookup.
Original receipts and command streams are preserved; all new evidence is separate.
"""
from contextlib import ExitStack, closing
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import sqlite3
import subprocess
import sys
import time

B=Path(__file__).resolve().parent
W=B/'p10-worktree'
BATCH=B/'post-p14-debug-process-batch-r1'
OUT=B/'post-p14-debug-process-cleanup-r1'
PRIVATE=B/'p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02'
HARNESS=W/'scripts/check-runtime-legacy-retirement.py'
BATCH_SHA='22021c64bab47f6331ca0fde64d33c1c18f9b4a24d43bbd9063414b7799a7256'
HARNESS_SHA='7f0321cd8b88fdb0c787ff7aacd62991e08419bbe1ea7f5b9250964c062a9cb4'
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
read=lambda p:json.loads(Path(p).read_text(encoding='utf-8'))


def normalized(path):
    text=str(path)
    if text.startswith('\\\\?\\'):
        assert re.match(r'^\\\\\?\\[A-Za-z]:\\',text),'Unexpected nonlocal namespaced path'
        text=text[4:]
    return Path(text).resolve(strict=False)


def main():
    assert os.name=='nt' and not OUT.exists(),'Fresh Windows cleanup evidence required'
    assert sha(BATCH/'summary.json')==BATCH_SHA and sha(HARNESS)==HARNESS_SHA
    summary=read(BATCH/'summary.json')
    assert summary['status']=='failed'
    assert [r['scenario'] for r in summary['steps']]==['source-only','deleted','returned','foreign-return']
    assert all(r['exit_code']!=0 and r['result_status']=='failed' for r in summary['steps'])
    spec=importlib.util.spec_from_file_location('retained_cleanup_pure_helpers',HARNESS)
    h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)
    OUT.mkdir()
    record={'status':'running','batch_sha256':sha(BATCH/'summary.json'),'pure_harness_helpers_sha256':sha(HARNESS),
        'scope':'Exact retained failed fixtures only; native selected-owner shutdown/quiescence; original evidence retained','fixtures':[]}
    def save():
        (OUT/'summary.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
    save()
    for row in summary['steps']:
        result={'scenario':row['scenario'],'qualified':False,'commands':[]}
        record['fixtures'].append(result)
        streams=OUT/row['scenario'];streams.mkdir()
        try:
            receipt_path=Path(row['result']).resolve(strict=True)
            assert receipt_path.parent==BATCH.resolve() and sha(receipt_path)==row['result_sha256']
            assert receipt_path.name==row['scenario']+'.json'
            receipt=read(receipt_path)
            root=Path(receipt['fixture']).resolve(strict=True)
            assert root.parent==PRIVATE.resolve(strict=True) and root.name.startswith('airk previous-version upgrade-')
            h.plain(root)
            install=h.contained(root,root/'install');state=h.contained(root,root/'state')
            control=h.contained(root,state/'control')
            selection_path=h.contained(root,install/'selected.json')
            assert selection_path.stat().st_size<=65536
            selected=read(selection_path)
            identity=selected['active']
            matches=[receipt[k] for k in ['old_payload','new_payload'] if receipt[k]['manifest_sha256']==identity]
            assert len(matches)==1 and re.fullmatch('[0-9a-f]{64}',identity)
            payload=h.contained(root,install/'runtimes'/identity)
            assert h.seal(payload)==matches[0], 'Selected installed payload differs from retained actual seal'
            binary=h.contained(root,payload/'airk.exe')
            expected_main=matches[0]['files']['airk.exe'];assert sha(binary)==expected_main
            database=h.contained(root,control/'service-1.4.sqlite3')
            with closing(sqlite3.connect(database.as_uri()+'?mode=ro',uri=True)) as db:
                db.execute('PRAGMA query_only=ON')
                registrations=db.execute('SELECT binding,substr(payload,1,20481) FROM projects ORDER BY binding').fetchall()
            assert 1<=len(registrations)<=2
            bindings=[]
            for binding,payload_bytes in registrations:
                registration=json.loads(payload_bytes)
                assert binding==registration['binding'] and re.fullmatch('[0-9a-f]{64}',binding)
                assert normalized(registration['root']).is_relative_to(root)
                assert normalized(registration['data_root']).is_relative_to(root)
                assert registration['collection'] in ['legacy-a','legacy-b']
                bindings.append(binding)
            original={str(p):sha(p) for p in [receipt_path,root/'commands.json',*root.glob('command-*.stdout'),*root.glob('command-*.stderr')]}
            result.update(fixture=str(root),receipt_sha256=sha(receipt_path),selected=identity,
                selected_binary=str(binary),selected_binary_sha256=expected_main,registry_bindings=bindings,
                original_evidence_sha256=original)
            environment=dict(os.environ,HTTP_PROXY='http://127.0.0.1:1',HTTPS_PROXY='http://127.0.0.1:1')
            def invoke(*arguments,required=True):
                ordinal=len(result['commands'])+1
                command=[str(binary),*map(str,arguments)]
                started=time.monotonic()
                value=subprocess.run(command,capture_output=True,encoding='utf-8',text=True,
                    timeout=180,creationflags=subprocess.CREATE_NO_WINDOW,env=environment)
                for suffix,body in [('stdout',value.stdout),('stderr',value.stderr)]:
                    with (streams/f'{ordinal:02d}.{suffix}').open('x',encoding='utf-8') as output:output.write(body)
                result['commands'].append({'command':command,'exit_code':value.returncode,'seconds':time.monotonic()-started})
                save()
                if required:assert value.returncode==0,value.stderr[-4096:]
                return value
            health_run=invoke('service','health','--runtime-dir',control,required=False)
            if health_run.returncode==0:
                health=json.loads(health_run.stdout)
                observation=health['observation']
                assert health['status']=='health' and observation['runtime_sha256']==identity
                assert observation['executable_sha256']==expected_main
                stopping=json.loads(invoke('service','shutdown','--runtime-dir',control).stdout)
                assert stopping['status']=='stopping'
                result['shutdown']='Exact selected sealed runtime authenticated health then ordinary shutdown'
            else:
                result['shutdown']='Health unavailable; no startup/restart/lifecycle mutation inferred; leases/native proof still required'
            assert read(selection_path)==selected,'Selected owner changed during cleanup'
            # Observe only preexisting owner files; never create a proof of absence.
            names=['instance-1.4.lock','embedding-1.4.lock','source-watch-1.4.lock']
            names += ['worker-1.4-'+binding+'.lock' for binding in bindings]
            import msvcrt
            deadline=time.monotonic()+60
            while True:
                try:
                    with ExitStack() as stack:
                        for name in names:
                            path=control/name
                            if not path.exists() and name in ['embedding-1.4.lock','source-watch-1.4.lock']:
                                continue
                            path=h.contained(root,path)
                            stream=stack.enter_context(path.open('r+b'))
                            msvcrt.locking(stream.fileno(),msvcrt.LK_NBLCK,1)
                            stack.callback(lambda f=stream:(f.seek(0),msvcrt.locking(f.fileno(),msvcrt.LK_UNLCK,1)))
                    break
                except OSError:
                    assert time.monotonic()<deadline,'Existing selected owner files did not drain'
                    time.sleep(.05)
            proof=json.loads(invoke('runtime','repair-launcher','--install-dir',install).stdout)
            assert proof=={'launcher_repaired':True,'runtime':identity}
            inspection=json.loads(invoke('runtime','inspect','--install-dir',install).stdout)
            assert inspection['payload_verified'] is True and inspection['approval']['manifest_sha256']==identity
            assert normalized(inspection['control_dir'])==control
            assert inspection['lifecycle']['pending'] is None
            assert read(selection_path)==selected and h.seal(payload)==matches[0]
            assert all(sha(path)==digest for path,digest in original.items()),'Original failed evidence was modified'
            result.update(qualified=True,proof=proof,inspection=inspection,
                claim='Native all-role quiescence and verified selected installation; not a lifecycle Stop or passing original scenario')
        except BaseException as error:
            result['error']=repr(error)
        save()
    record['status']='passed' if all(r['qualified'] for r in record['fixtures']) else 'failed'
    save();print(json.dumps({'status':record['status'],'qualified':sum(r['qualified'] for r in record['fixtures'])}))
    return 0 if record['status']=='passed' else 1


if __name__=='__main__':
    sys.exit(main())
