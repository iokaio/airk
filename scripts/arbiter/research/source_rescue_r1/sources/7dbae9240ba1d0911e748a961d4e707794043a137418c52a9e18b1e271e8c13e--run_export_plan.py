# SPDX-License-Identifier: Apache-2.0
# PRIVATE command adapter for the existing reviewed Windows owner; NOT executed.
# No new process framework, native DTO, gate verdict or qualification authority.
import argparse,hashlib,importlib.util,json,os,pathlib,sys,time

def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def load(path):return json.loads(path.read_bytes())
def main():
 p=argparse.ArgumentParser();p.add_argument('--plan',type=pathlib.Path,required=True);p.add_argument('--plan-sha256',required=True);p.add_argument('--model',choices=['focus','compact','large','all'],required=True);args=p.parse_args()
 assert digest(args.plan)==args.plan_sha256,'reviewed plan changed';plan=load(args.plan);root=pathlib.Path(__file__).resolve().parents[5]
 # Root must first release the actual FP32/nativeG5 gates and verify same-human
 # shell policy. This script never manufactures a positive gate receipt.
 for entry in plan['maintained_sources']:
  path=root/entry['target'];assert path.is_file() and digest(path)==entry['proposed']['sha256'],'maintained source not applied/frozen'
 assert digest(pathlib.Path(plan['python']['path']))==plan['python']['sha256']
 for dep in plan['dependency_metadata']:assert digest(pathlib.Path(dep['path']))==dep['sha256']
 helper=plan['existing_owned_process_helper'];hp=pathlib.Path(helper['path']);assert digest(hp)==helper['sha256']
 spec=importlib.util.spec_from_file_location('owned_export_process',hp);owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
 selected=[row for row in plan['models'] if args.model=='all' or row['model']==args.model];assert selected
 parent=pathlib.Path(selected[0]['output']).parent;assert not parent.exists(),'fresh whole export output required'
 import shutil
 assert shutil.disk_usage(parent.parent).free>=plan['minimum_free_bytes'],'insufficient planned export reserve'
 for row in selected:
  assert digest(pathlib.Path(row['source_closure']['path']))==row['source_closure']['sha256']
  assert digest(pathlib.Path(row['original_graph']['path']))==row['original_graph']['sha256']
 parent.mkdir(exist_ok=False)
 for row in selected:
  seconds=plan['per_export_tool_deadline_seconds_proposed'];started=time.monotonic();deadline=started+seconds
  argv=[plan['python']['path'],'-S','-B',str(root/'scripts/export-arbiter-fp16.py'),'--model',row['model'],'--source',row['original_graph']['path'],'--source-pins',row['source_closure']['path'],'--source-pins-sha256',row['source_closure']['sha256'],'--python-site',plan['python_site'],'--output',row['output'],'--deadline-monotonic-ns',str(int(deadline*1_000_000_000))]
  record={'argv':argv,'original_deadline_monotonic_ns':int(deadline*1_000_000_000),'model_inference':False,'qualification':False}
  log=parent/(row['model']+'.export.log')
  with log.open('xb') as output:record['owned']=owner.run_windows_owned(argv,root,output,seconds,dict(os.environ),absolute_deadline=deadline)
  record['elapsed_seconds']=time.monotonic()-started;record['log']={'path':str(log),'sha256':digest(log)}
  result=record['owned'];success=result['exit_code']==0 and result['timed_out'] is False and result['log_oversized'] is False and result['owned_descendants_settled'] is True and result['owned_job_forced_termination'] is False and result['owned_job_final_active_processes']==0 and result['owned_job_final_member_pids']==[]
  if success:
   destination=pathlib.Path(row['output']);manifest=load(destination/'export-manifest.private.json');expected=row['expected_weights_bin_bytes'];success=(destination/'weights.bin').stat().st_size==expected and manifest['attention_nodes']==row['expected_attention_nodes'] and manifest['qualification'] is False
  record['complete']=success
  with (parent/(row['model']+'.export-execution.json')).open('x',encoding='utf-8') as handle:json.dump(record,handle,indent=2);handle.write('\n')
  if not success:raise SystemExit('Export failed; original artifacts/logs retained; later models were not started')
 print('Selected graph conversions completed; native/model qualification remains separate')
if __name__=='__main__':main()
