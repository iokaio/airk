# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,subprocess,time,datetime,importlib.util,os
spec=importlib.util.spec_from_file_location('bench',Path('scripts/benchmark-real-corpora.py'));B=importlib.util.module_from_spec(spec);spec.loader.exec_module(B)
base=B.physical('C:/Users/tdotn/AppData/Local/Temp/airk-optimizations-f01c-r19x7kro/campaign/state');sources=[B.physical('D:/code/Github/airk'),B.physical('D:/code/Github/bookbuilder')]
B.isolate(sources,[base]);owners=[B.physical(p) for p in base.iterdir() if p.is_dir()];B.require(len(owners)==1 and B.inside(owners[0],base),'unexpected_campaign_owner');control=B.physical(owners[0]/'control');B.require(B.inside(control,owners[0]),'control_outside_owner')
binary=B.physical('C:/Users/tdotn/AppData/Local/Temp/airk-optimizations-f01c-r19x7kro/runtime/airk.exe')
import hashlib
manifest=binary.parent/'airk-install.json';B.require(manifest.stat().st_size<=1048576,'manifest_budget');B.require(hashlib.sha256(manifest.read_bytes()).hexdigest()=='356a1974171e9c01718aaf5bd17eea09719793769572d3eba32713636d85b78b','approved_runtime_mismatch')
report={'kind':'bookbuilder_metadata_snapshot_during_real_corpus_campaign','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'command_shape':'airk service rpc --runtime-dir <owned-control> --stdin','operations':[],'conditions':'extra_readonly_metadata_observation_after_first_reads_not_pristine_throughput','index_detail':'not_exposed_by_progress'}
report['runtime_binding']='approved_path_and_bounded_install_manifest_digest_no_payload_rehash';report['job_age_basis']='native_progress_elapsed_ms_not_enqueue_wall_age';report['prior_snapshot_utc']='2026-10-31T07:32:40.574283+00:00'
started=time.monotonic();deadline=started+10
phases={'starting','discovering','extracting','preparing','publishing','embedding','verifying','recovering','retiring','finished'};kinds={'inspect','plan_inspect','repair','index','reconcile','vectors','model_setup','configure','verify','rebuild','recover','maintain'};states={'pending','running','succeeded','failed','canceled'}
def rpc(method):
 stamp=time.monotonic();B.require(deadline>stamp,'metadata_deadline')
 result=subprocess.run([str(binary),'service','rpc','--runtime-dir',str(control),'--stdin'],input=json.dumps(method),capture_output=True,text=True,encoding='utf-8',timeout=deadline-stamp,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
 report['operations'].append({'name':next(iter(method)),'elapsed_ms':round((time.monotonic()-stamp)*1000,3),'returncode':result.returncode})
 B.require(result.returncode==0,'metadata_command_failed');return json.loads(result.stdout)
def job_summary(job):
 return {'kind':job.get('kind') if job.get('kind') in kinds else 'other','state':job.get('state') if job.get('state') in states else 'other','attempts':job.get('attempts') if type(job.get('attempts')) is int else None}
try:
 value=rpc({'projects':{'after':None,'revision':None,'limit':32}});page=value.get('page',{});rows=page.get('projects',[])
 report['projects_schema']={'status':value.get('status') if value.get('status')=='projects' else 'other','page_present':isinstance(value.get('page'),dict),'rows':len(rows),'has_next':page.get('after') is not None,'revision':page.get('revision') if type(page.get('revision')) is int else None,'registration_shapes':[sorted(row.get('registration',{}).keys()) for row in rows]}
 matched=[]
 for row in rows:
  registration=row.get('registration',{})
  root=registration.get('root')
  if not isinstance(root,str):continue
  for alias,source in zip(('airk','bookbuilder'),sources):
   if Path(root).samefile(source):matched.append((alias,registration))
 report['matched_aliases']=[alias for alias,_ in matched]
 book=[registration for alias,registration in matched if alias=='bookbuilder'];B.require(len(book)==1,'expected_campaign_project_not_found')
 observed=rpc({'progress':{'project':book[0]['binding']}});B.require(observed.get('project')==book[0]['binding'],'crossed_progress')
 item=observed.get('observation') or {};progress=item.get('progress') or {};job=item.get('job') or {}
 report['project_alias']='bookbuilder';report['progress_observed']=bool(item);report['job']=job_summary(job)
 report['progress']={key:progress.get(key) for key in ('sequence','paused','stalled','elapsed_ms','phase_elapsed_ms','last_advance_age_ms','unpaused_idle_ms','captured_files','extracted_files','prepared_changes','committed_changes','publications','verified_objects','indexed_files','total_files','completed_semantic_units') if type(progress.get(key)) in (int,bool) or progress.get(key) is None}
 report['progress']['phase']=progress.get('phase') if progress.get('phase') in phases else 'other';report['progress']['phase_totals_ms']={key:n for key,n in progress.get('phase_totals_ms',{}).items() if key in phases and type(n) is int and n>=0}
 report['queued_jobs']=[job_summary(j) for j in observed.get('queued_jobs',[])][:16]
 report['pending_job']=job_summary(observed['pending_job']) if isinstance(observed.get('pending_job'),dict) else None
 report['queue_truncated']=observed.get('queue_truncated') if type(observed.get('queue_truncated')) is bool else None
except Exception as error:report['code']=B.failure_code(error)
report['wall_ms']=round((time.monotonic()-started)*1000,3)
Path('artifacts/optimizations/f01c-bookbuilder-progress-'+datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')+'.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
