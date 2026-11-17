# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,time,sys,collections,datetime,importlib.util
root=Path.cwd();e=root/'artifacts/v190/final-plan/exec-20261116T052942Z';out=e/'f1-score-seal-audit-r1';s=importlib.util.spec_from_file_location('audit_custody',e/'candidate-overlay-preparation-r1/candidate_overlay.py');h=importlib.util.module_from_spec(s);sys.modules[s.name]=h;s.loader.exec_module(h);identity=h.identity()
planpath=e/'f1-score-plan-preparation-r2/plan.json';assert h.sha(planpath)=='6f1cf1c880958c2f8c465495852e6f1bf82253533593081cd1b99ccff511f8f0';plan=json.loads(planpath.read_bytes());originalpath=root/'artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1/plans-native-r2/focus-original_fp32.json';original=json.loads(originalpath.read_bytes());assert len(plan['input_seals'])==13274
site=Path(plan['reference_plan']['site_packages']);start=time.monotonic();rows=[];mismatches=[];counts=collections.Counter();hashed=0
for index,(name,expected) in enumerate(plan['input_seals'].items(),1):
 p=Path(name);row={'path':name,'expected_sha256':expected,'inherited_original_reference_seal':name in original['input_seals'],'direct_execution_source':name in plan['execution_sources']}
 if p.is_relative_to(site):kind='python_site_packages_runtime'
 elif p==Path(plan['reference_module']['path']):kind='official_reference_math_source'
 elif p.is_relative_to(root)and p.relative_to(root).parts[0]!='artifacts':kind='Root_native_or_build_context_pending_consumption_trace'
 elif p.is_relative_to(Path(plan['base_python']['path']).parent):kind='base_python_runtime'
 elif p.is_relative_to(e):kind='current_F1_data_or_source'
 else:kind='historical_model_reference_or_artifact'
 row['classification']=kind
 try:
  before=p.lstat();row['bytes']=before.st_size;row['file_id']=[before.st_dev,before.st_ino];row['mtime_ns']=before.st_mtime_ns
  if not p.is_file()or p.is_symlink()or getattr(before,'st_file_attributes',0)&0x400:raise ValueError('input not ordinary regular file')
  with p.open('rb')as stream:actual=hashlib.file_digest(stream,'sha256').hexdigest()
  after=p.lstat();row['observed_sha256']=actual;row['stable_during_hash']=(before.st_dev,before.st_ino,before.st_size,before.st_mtime_ns)==(after.st_dev,after.st_ino,after.st_size,after.st_mtime_ns);row['matches']=actual==expected and row['stable_during_hash'];hashed+=before.st_size
 except BaseException as error:row['matches']=False;row['error']={'type':type(error).__name__,'message':str(error)}
 rows.append(row);counts[kind]+=1
 if not row['matches']:mismatches.append(row)
 if index%2000==0 or index==13274:print(json.dumps({'sealed_files_audited':index,'of':13274,'mismatches_so_far':len(mismatches),'hashed_bytes':hashed,'elapsed_seconds':time.monotonic()-start}),flush=True)
(out/'all-seals.json').write_text(json.dumps(rows,indent=2)+'\n',encoding='utf-8')
report={'schema':'airk.private.F1-full-inherited-seal-audit/1','identity':identity,'plan':{'path':str(planpath),'sha256':h.sha(planpath)},'original_reference_plan':{'path':str(originalpath),'sha256':h.sha(originalpath)},'audited_files':len(rows),'hashed_bytes':hashed,'elapsed_seconds':time.monotonic()-start,'classes':dict(counts),'mismatches':mismatches,'full_inventory_sha256':h.sha(out/'all-seals.json'),'source_or_input_mutations':False,'models_torch_cargo_or_gpu_execution':False,'classification_is_not_source_epoch_acceptance':True}
(out/'audit.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8');print(json.dumps({'audit':str(out/'audit.json'),'sha256':h.sha(out/'audit.json'),'mismatch_count':len(mismatches),'elapsed_seconds':report['elapsed_seconds']}),flush=True)
