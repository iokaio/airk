# SPDX-License-Identifier: Apache-2.0

from pathlib import Path
import json,os,sys,stat,hashlib,subprocess,tarfile,importlib.util
from datetime import datetime,timezone
assert os.getuid()==os.geteuid()==501==os.stat('/dev/console').st_uid and os.getuid()!=0
b=Path.home()/'airk-builds/arbiter-final-20261116-r1';source=b/'source';e=b/'evidence';launch=e/'platform-admin-repair-launch-r2';expected={'neo-next-integrated-platform-batch-r1.py': {'bytes': 8529, 'sha256': '5bc1bbec67655fb6bdd9fa843be816d0a669b2d4d7fa38a49add8afa7965140e'}, 'frozen-neo-config-r1.json': {'bytes': 6750, 'sha256': 'd8449ff9cfd261de8344f4cff8915bf601e1d99e057f60903836c3c3d58799a1'}}
assert not launch.exists() and not (e/'platform-admin-repair-r2').exists() and not (source/'target').exists()
for q in (b,e,source):assert q.lstat().st_uid==501 and not q.is_symlink() and os.access(q,os.W_OK|os.X_OK)
def sha(q):
 with q.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip()=='c547e6547b1312c2e113bd1ddc9ab84969c229c0'
assert not subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','--untracked-files=all'],cwd=source)
assert sha(e/'source-bridge-c547e654-r1.json')=='8289c892f37f8695330b0e8e49a7d795ae66cc3788506cfa15a0971f49bb3847'
assert sha(e/'static-environment-r1.json')=='e811e5facb90d2f919dd18ae78326ca05e1497e59e004c562a6e36616897d9fa'
assert sha(b/'inputs/static-ort130-aarch64/aggregate/libonnxruntime.a')=='d6bebed75eb18f83ce2eacb04fd150025a8d50f9c52268008dfa8a935798245a'
assert sha(e/'platform-integrated-dda749cb-r1/batch-result.json')=='287dc4601a5868dd775327e934448c34d409bcc127f9a996daefefba7436c9e0'
assert sha(e/'platform-admin-repair-r1/batch-result.json')=='72aca246ab496dc784c21ecd7bd43e42d8de51ed8c4628f345d5125ea8dda807'
launch.mkdir(mode=0o700);received=[]
with tarfile.open(fileobj=sys.stdin.buffer,mode='r|gz') as stream:
 for member in stream:
  assert member.name in expected and member.name not in received and member.isfile() and member.size==expected[member.name]['bytes']
  q=launch/member.name
  with stream.extractfile(member) as src,q.open('xb') as out:
   for block in iter(lambda:src.read(65536),b''):out.write(block)
   out.flush();os.fsync(out.fileno())
  assert sha(q)==expected[member.name]['sha256'] and q.stat().st_uid==501 and q.stat().st_nlink==1;received.append(member.name)
assert set(received)==set(expected)
cfg=json.loads((launch/'frozen-neo-config-r1.json').read_text());assert cfg['commit']=='c547e6547b1312c2e113bd1ddc9ab84969c229c0' and cfg['raw_c7']=={'sha256': 'a8d7983b0f8bfdb701c7659f3b7a424505359a0dbe52446eb10c31de0b1a3dfe', 'files': 1941}
assert sha(Path(cfg['manifest']))==cfg['manifest_sha256']=='41a70c2bd4239d55ff95c372778e5fdcf10f6cff30326144a818175fe51169c7'
spec=importlib.util.spec_from_file_location('actual_source_c7',source/'scripts/build-service-runtime.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);assert module.source_identity()==cfg['raw_c7']
argv=[cfg['python'],'-I','-B',str(launch/'neo-next-integrated-platform-batch-r1.py'),str(launch/'frozen-neo-config-r1.json')]
receipt={'schema':'airk.Neo-reviewed-helper-transfer/1','utc':datetime.now(timezone.utc).isoformat(),'uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid,'files':expected,'launch_directory':str(launch),'launch_argv':argv,'commit':cfg['commit'],'raw_c7':cfg['raw_c7'],'source_bridge_proof_sha256':sha(e/'source-bridge-c547e654-r1.json'),'static_environment_unchanged':True,'actual_static_aggregate_unchanged':True,'original_failed_batch_unchanged':True,'target_absent':True,'fresh_batch_output_absent':True,'Cargo_started':False,'scope':'Exact reviewed private orchestration files copied only. Execution remains Root sole-owner action.'}
with (launch/'launch-ready-r1.json').open('x') as out:json.dump(receipt,out,indent=2);out.write('\n');out.flush();os.fsync(out.fileno())
print(json.dumps(receipt,indent=2),flush=True)
