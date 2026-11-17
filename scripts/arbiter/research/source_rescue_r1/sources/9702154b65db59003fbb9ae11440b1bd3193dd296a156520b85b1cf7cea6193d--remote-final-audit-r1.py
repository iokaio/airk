# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,os,stat,subprocess,importlib.util,time
from datetime import datetime,timezone
started=time.monotonic();uid=os.getuid();assert uid==501==os.stat('/dev/console').st_uid and uid!=0
base=Path.home()/'airk-builds/arbiter-final-20261116-r1';source=base/'source';e=base/'evidence';inputs=base/'inputs';ort=inputs/'static-ort130-aarch64'
def sha(path):
 with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
result_path=e/'prerequisite-result-r1.json';result=json.loads(result_path.read_text());assert result['passed'] is True and result['status']=='completed'
assert len(result['observed_commands'])==5 and all(r['exit_code']==0 for r in result['observed_commands'])
assert result['commands'][-1][-1]=='--verify'
manifest=json.loads((base/'transfer/source-manifest.json').read_text())
for row in manifest['files']:
 q=source/row['path'];s=q.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==uid and sha(q)==row['sha256']
assert not subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','--untracked-files=normal'],cwd=source)
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip()==manifest['commit']
assert subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=source,text=True).strip()==manifest['normalized_tree']
spec=importlib.util.spec_from_file_location('neo_final_actual_c7',source/'scripts/build-service-runtime.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);seal=module.source_identity();assert seal==manifest['raw_c7']
original_status=subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','-z','--untracked-files=normal'],cwd=Path.home()/'code/airk')
assert hashlib.sha256(original_status).hexdigest()=='943936de2803601aa34eeb0d5c974bbb94261dc762496bd44196177755734b06'
for row in result['cache_before']:
 q=Path(row['path']);s=q.stat();assert sha(q)==row['sha256'] and s.st_uid==row['uid'] and s.st_ino==row['inode']
tool_rows=[]
for name,path in result['tool_paths'].items():
 q=Path(path);assert q.stat().st_uid==uid;tool_rows.append({'name':name,'path':path,'sha256':sha(q),'uid':q.stat().st_uid})
counts={'files':0,'directories':0,'links':0}
for folder in (source,inputs,e,base/'transfer'):
 for parent,dirs,files in os.walk(folder,followlinks=False):
  parent=Path(parent);assert parent.lstat().st_uid==uid;counts['directories']+=1
  for name in [*dirs,*files]:
   q=parent/name;s=q.lstat();assert s.st_uid==uid
   if stat.S_ISLNK(s.st_mode):counts['links']+=1
   elif stat.S_ISREG(s.st_mode):counts['files']+=1
   elif not stat.S_ISDIR(s.st_mode):raise AssertionError('unexpected special output: '+str(q))
environment_path=e/'static-environment-r1.json';environment=json.loads(environment_path.read_text());fields=environment['environment']
assert set(fields)=={'LIBONNXRUNTIME_NO_PKG_CONFIG','ORT_SKIP_DOWNLOAD','ORT_PREFER_DYNAMIC_LINK','ORT_LIB_LOCATION','ORT_LIB_PROFILE'}
assert fields['LIBONNXRUNTIME_NO_PKG_CONFIG']=='1' and fields['ORT_SKIP_DOWNLOAD']=='1' and fields['ORT_PREFER_DYNAMIC_LINK']=='0'
assert Path(fields['ORT_LIB_LOCATION']).resolve().is_relative_to(ort.resolve())
build=json.loads((ort/'build-receipt.json').read_text());aggregate=json.loads((ort/'static-aggregate-receipt.json').read_text())
assert build['ort_version']=='1.30.0' and build['upstream_commit']=='f2c39fe2f838cf35ce7da92824f5a5e3ee6e88a7' and build['build_argv'][-2:]==['--parallel','2']
receipts={}
for q in [result_path,e/'source-custody-r1.json',environment_path,ort/'build-attempt.json',ort/'build-receipt.json',ort/'static-aggregate-receipt.json',e/'prerequisite-events-r1.jsonl',e/'prerequisite-start-r1.json']:
 receipts[str(q.relative_to(base))]={'bytes':q.stat().st_size,'sha256':sha(q)}
proof={'schema':'airk.neo-prerequisite-final-proof/1','utc':datetime.now(timezone.utc).isoformat(),'elapsed_seconds':time.monotonic()-started,'uid':uid,'console_uid':os.stat('/dev/console').st_uid,'passed':True,'source':str(source),'commit':manifest['commit'],'normalized_tree':manifest['normalized_tree'],'raw_c7':seal,'source_files_verified':len(manifest['files']),'source_clean':True,'dirty_original_preserved':True,'dirty_original_entries':365,'original_model_cache_assets_preserved':len(result['cache_before']),'all_generated_paths_owner_uid':uid,'output_census':counts,'tool_inputs':tool_rows,'static_ort':str(ort),'models':str(inputs/'bundled-model-v3'),'arm64_test_model':str(inputs/'bundled-model-v3/minilm-l6-v2-s8-v3'),'verified_environment_file':str(environment_path),'verified_environment':fields,'observed_commands':result['observed_commands'],'build_qualification':build.get('qualification'),'result_elapsed_seconds':result['elapsed_seconds'],'retained_receipts':receipts,'limits':['No Cargo invoked; final native link, numerical model, runtime/package and installed-journey qualification remain pending.','Existing ORT1.22/preliminary roots and dirty original checkout retained.']}
with (e/'prerequisite-final-proof-r1.json').open('x') as f:json.dump(proof,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
print(json.dumps(proof,indent=2),flush=True)
