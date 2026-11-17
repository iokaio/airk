# SPDX-License-Identifier: Apache-2.0
"""Submit each reviewed real native setup/index tuple once; never retry jobs."""
from datetime import datetime,timezone
import hashlib,json
from pathlib import Path
import subprocess,sys,time

base=Path(__file__).resolve().parent;repo=base.parents[2]
sys.path.insert(0,str(repo/'tools/retrieval-train'))
from native_goal_client import windows_process_identity

plan_path=base/'native-a6-command-plan-r6-runtime-r2.json';plan_raw=plan_path.read_bytes();plan=json.loads(plan_raw)
ready_path=Path(plan['native_fixture_ready']);ready_raw=ready_path.read_bytes();ready=json.loads(ready_raw)
assert hashlib.sha256(ready_raw).hexdigest()==plan['native_fixture_ready_sha256']
pair=Path(plan['runtime_pair']);meta_path=pair/'airk-runtime.json';meta_raw=meta_path.read_bytes();meta=json.loads(meta_raw)
assert hashlib.sha256(meta_raw).hexdigest()=='acc239e7dcec03cc8aa42e19c04e7e593c864ca49de8c5d4b911dc09f6fc0530'
sha_file=lambda path:hashlib.sha256(Path(path).read_bytes()).hexdigest()
assert sha_file(pair/'airk.exe')==meta['airk_sha256']=='63e7d3f616d57aa96549455b73caae70b6376c77946f901443f33a63c61edf43'
assert sha_file(pair/'airkctl.exe')==meta['airkctl_sha256']=='53fb8f49f2190ba6b497ab58a50d9fccc8b38d1905768723457c48d4bce5552a'
root=base/'native-a6-index-setup-r6-fixed-datadir-r1';assert not root.exists();root.mkdir()
expected=ready['health'];original=windows_process_identity(expected['process_id'])
assert original['executable_sha256']==expected['executable_sha256']
receipt={'schema':'airk.private-native-index-setup/1','complete':False,'evaluation':True,'release_qualified':False,
         'started_at_utc':datetime.now(timezone.utc).isoformat(),'ready_sha256':hashlib.sha256(ready_raw).hexdigest(),
         'pair_metadata_sha256':hashlib.sha256(meta_raw).hexdigest(),'plan_sha256':hashlib.sha256(plan_raw).hexdigest(),
         'original_process':original,'entries':[],'scope':'Actual native CLI setup+ONE keyed index submission per reviewed root. No fabricated IDs, bindings, publication or semantic readiness.'}
save=lambda:(root/'submissions.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
save()

def call(command,directory,name,timeout=60):
    start=time.monotonic();before=datetime.now(timezone.utc).isoformat()
    process=subprocess.run(command,capture_output=True,timeout=timeout,check=False)
    (directory/(name+'-stdout.bin')).write_bytes(process.stdout)
    (directory/(name+'-stderr.bin')).write_bytes(process.stderr)
    record={'command':command,'started_at_utc':before,'finished_at_utc':datetime.now(timezone.utc).isoformat(),
            'elapsed_seconds':time.monotonic()-start,'returncode':process.returncode,
            'stdout_sha256':hashlib.sha256(process.stdout).hexdigest(),'stderr_sha256':hashlib.sha256(process.stderr).hexdigest()}
    (directory/(name+'.json')).write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
    if process.returncode!=0:raise ValueError('Actual native command refused: '+name)
    return [json.loads(line) for line in process.stdout.decode().splitlines() if line.strip()]

def health(directory,name):
    assert ready_path.read_bytes()==ready_raw and meta_path.read_bytes()==meta_raw and plan_path.read_bytes()==plan_raw
    observed=windows_process_identity(expected['process_id'])
    assert observed==original,'original native process identity/birth changed'
    rows=call([str(pair/'airkctl.exe'),'--service-runtime-dir',ready['control_root'],'service','health'],directory,name)
    assert len(rows)==1 and rows[0]['status']=='health'
    actual=rows[0]['observation']
    assert all(actual.get(key)==expected.get(key) for key in ('instance','process_id','executable_sha256','version','protocol','user'))
    assert actual['catalog_clock']['namespace']==expected['catalog_clock']['namespace']

try:
    for ordinal,step in enumerate(plan['index_steps']):
        directory=root/('source-'+str(ordinal).zfill(2));directory.mkdir()
        data_root=Path(step['data_root'])
        assert data_root.resolve().is_relative_to(ready_path.parent.resolve())
        assert not data_root.resolve().is_relative_to((base/'native-human-fixtures-r2').resolve())
        data_root.mkdir(parents=True,exist_ok=False)
        health(directory,'health-before')
        applied=call(step['setup_argv'],directory,'setup')
        assert applied[-1]['status']=='catalog_index_setup_applied'
        index_id=applied[-1]['receipt']['index_id'];assert isinstance(index_id,str) and index_id
        health(directory,'health-after-setup')
        rows=call(step['index_argv'],directory,'index')
        assert len(rows)==1 and rows[0]['status']=='job'
        job=rows[0]['job'];assert isinstance(job['id'],str) and len(job['id'])==64
        assert job['state'] not in ('failed','canceled')
        health(directory,'health-after-index-submission')
        receipt['entries'].append({'source_root':step['source_root'],'data_root':step['data_root'],
                                  'collection':step['collection'],'actual_setup_index_id':index_id,
                                  'original_job_id':job['id'],'original_job_record':job,'records':str(directory)})
        save();print(json.dumps({'submitted_sources':ordinal+1,'planned_sources':len(plan['index_steps']),
                                'original_job_id':job['id'],'actual_index_id':index_id}),flush=True)
    receipt['complete']=True
except Exception as error:
    receipt['error']=str(error);raise
finally:
    receipt['finished_at_utc']=datetime.now(timezone.utc).isoformat();save()
