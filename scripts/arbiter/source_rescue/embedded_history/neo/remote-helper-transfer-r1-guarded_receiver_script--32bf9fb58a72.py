# SPDX-License-Identifier: Apache-2.0

from pathlib import Path
import json,os,sys,stat,hashlib,subprocess,tarfile
from datetime import datetime,timezone
assert os.getuid()==os.geteuid()==501==os.stat('/dev/console').st_uid and os.getuid()!=0
b=Path.home()/'airk-builds/arbiter-final-20261116-r1';source=b/'source';e=b/'evidence';launch=e/'platform-admin-repair-launch-r1';expected={'neo-next-integrated-platform-batch-r1.py': {'bytes': 8529, 'sha256': '5bc1bbec67655fb6bdd9fa843be816d0a669b2d4d7fa38a49add8afa7965140e'}, 'frozen-neo-config-r1.json': {'bytes': 6750, 'sha256': 'aa5ba68d6b2bcd44805132221fea1e3646a14f559f8c45f14fa1f33b8b81e1a0'}}
assert not launch.exists() and not (e/'platform-admin-repair-r1').exists() and not (source/'target').exists()
for q in (b,e,source):assert q.lstat().st_uid==501 and not q.is_symlink() and os.access(q,os.W_OK|os.X_OK)
def sha(q):
 with q.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip()=='6c2410791999fa7691b9be97441f371d4048d870'
assert not subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','--untracked-files=all'],cwd=source)
assert sha(e/'source-bridge-6c241079-r1.json')=='394f4a8e7c5db37a6c4ecd19afb30027330aebec596ee627325bea56592fd672'
assert sha(e/'static-environment-r1.json')=='e811e5facb90d2f919dd18ae78326ca05e1497e59e004c562a6e36616897d9fa'
assert sha(b/'inputs/static-ort130-aarch64/aggregate/libonnxruntime.a')=='d6bebed75eb18f83ce2eacb04fd150025a8d50f9c52268008dfa8a935798245a'
assert sha(e/'platform-integrated-dda749cb-r1/batch-result.json')=='287dc4601a5868dd775327e934448c34d409bcc127f9a996daefefba7436c9e0'
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
cfg=json.loads((launch/'frozen-neo-config-r1.json').read_text());assert cfg['commit']=='6c2410791999fa7691b9be97441f371d4048d870' and cfg['raw_c7']=={'sha256':'f4353a13aef8d5d908944fb643ff833f59af0f215cbd73bad79723b063ea8b1f','files':1937}
assert sha(Path(cfg['manifest']))==cfg['manifest_sha256']=='1f8b6040bf162c85cccda1f94ce31381c8a4657bb951cdd63e5932fb8f5dfe5b'
argv=[cfg['python'],'-I','-B',str(launch/'neo-next-integrated-platform-batch-r1.py'),str(launch/'frozen-neo-config-r1.json')]
receipt={'schema':'airk.Neo-reviewed-helper-transfer/1','utc':datetime.now(timezone.utc).isoformat(),'uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid,'files':expected,'launch_directory':str(launch),'launch_argv':argv,'commit':cfg['commit'],'raw_c7':cfg['raw_c7'],'source_bridge_proof_sha256':sha(e/'source-bridge-6c241079-r1.json'),'static_environment_unchanged':True,'actual_static_aggregate_unchanged':True,'original_failed_batch_unchanged':True,'target_absent':True,'fresh_batch_output_absent':True,'Cargo_started':False,'scope':'Exact reviewed private orchestration files copied only. Execution remains Root sole-owner action.'}
with (launch/'launch-ready-r1.json').open('x') as out:json.dump(receipt,out,indent=2);out.write('\n');out.flush();os.fsync(out.fileno())
print(json.dumps(receipt,indent=2),flush=True)
