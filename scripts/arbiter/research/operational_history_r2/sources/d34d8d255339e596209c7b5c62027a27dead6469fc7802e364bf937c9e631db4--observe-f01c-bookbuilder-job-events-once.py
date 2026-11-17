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
 value=rpc({'projects':{'after':None,'revision':None,'limit':32}})
 page=value.get('page',{});rows=page.get('projects',[]);B.require(page.get('after') is None,'scope_page_incomplete')
 matches=[row['registration'] for row in rows if isinstance(row.get('registration',{}).get('root'),str) and Path(row['registration']['root']).samefile(sources[1])]
 B.require(len(matches)==1,'expected_campaign_project_not_found');binding=matches[0]['binding']
 first=rpc({'events':{'project':binding,'cursor':None,'limit':32}})['page']
 cursor={'namespace':first['namespace'],'project':binding,'after':max(0,first['snapshot_through']-32),'through':first['snapshot_through']}
 last=rpc({'events':{'project':binding,'cursor':cursor,'limit':32}})['page']
 events={e['sequence']:e for p in (first,last) for e in p['events']}
 B.require(all(e['project']==binding for e in events.values()),'crossed_event_scope')
 grouped={}
 for e in events.values():
  if e.get('job_kind') not in ('index','vectors') or not e.get('job'):continue
  row=grouped.setdefault(e['job'],{'kind':e['job_kind'],'events':[]});row['events'].append(e)
 summary={}
 for kind in ('index','vectors'):
  jobs=[j for j in grouped.values() if j['kind']==kind];durations=[];terminal={k:0 for k in ('job_succeeded','job_failed','job_canceled')};latest=None
  for j in jobs:
   starts=[e['observed_unix_ms'] for e in j['events'] if e['kind']=='job_running'];ends=[e for e in j['events'] if e['kind'] in terminal]
   for e in ends:
    terminal[e['kind']]+=1;latest=max(latest or 0,e['observed_unix_ms'])
    before=[t for t in starts if t<=e['observed_unix_ms']]
    if before:durations.append(e['observed_unix_ms']-max(before))
  summary[kind]={'distinct_jobs_observed':len(jobs),'terminal_events':terminal,'matched_run_terminal_pairs':len(durations),'matched_elapsed_ms':durations,'latest_terminal_unix_ms':latest}
 report['kind']='bookbuilder_bounded_job_event_summary';report['source']='events_not_jobs_list';report['scope_validation_rpc_count']=1;report['job_metadata_rpc_count']=2
 report['coverage']={'first_events':len(first['events']),'tail_window_events':len(last['events']),'unique_events':len(events),'retained_prefix_lost':first['retained_prefix_lost'],'first_page_has_next':first['next'] is not None,'sampled_tail_global_sequence_width':32,'complete_job_history':False,'job_age':'not_exposed_in_JobRecord','elapsed_basis':'paired_retained_running_terminal_event_timestamps_only'}
 report['job_summary']=summary
except Exception as error:report['code']=B.failure_code(error)
report['wall_ms']=round((time.monotonic()-started)*1000,3)
out=Path('artifacts/optimizations/f01c-bookbuilder-job-events-'+datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')+'.json');out.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
