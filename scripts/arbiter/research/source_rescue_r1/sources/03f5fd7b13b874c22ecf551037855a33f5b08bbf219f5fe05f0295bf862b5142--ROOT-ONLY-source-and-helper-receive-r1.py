# SPDX-License-Identifier: Apache-2.0
# ROOT ONLY. Three exact existing SSH stages; source/helper transfer only. No Cargo or batch launch.
from pathlib import Path
import importlib.util,json,base64,io,tarfile,subprocess,shlex,time,datetime,hashlib,shutil,os
ROOT=Path(r"D:\code\Github\airk");E=ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z';OUT=Path(__file__).resolve().parent;R=OUT/'receipts'
s=importlib.util.spec_from_file_location('custody',E/'candidate-overlay-preparation-r1/candidate_overlay.py');h=importlib.util.module_from_spec(s);s.loader.exec_module(h);actor=h.identity();assert h.security(R)['owner_sid']==h.HUMAN
assert h.sha(OUT/'transfer/remote-source-bridge-r1.py')=='137b20dd08d9caf496a60717646d27fe9c2c0582d122f77ca91cb69bf9bcdd94'
assert h.sha(OUT/'frozen-neo-config-r1.json')=='72fc046685b6912746700fd89d9548f5f8306f9d2bbae6a913da12acd7c2adc1'
assert h.sha(OUT/'neo-next-integrated-platform-batch-r1.py')=='3ed4179fc78e062e1a0149e1f3098688dd38816910c36319d71375e37af1da20'
prior=E/'neo-next-batch-preparation-r2/bridge-c547e654-r1';transport=json.loads((prior/'receipts/remote-source-transfer-r1.json').read_text())['command'][:-1];cfg=json.loads((OUT/'frozen-neo-config-r1.json').read_text());proof=json.loads((R/'local-preparation-r1.json').read_text())
def execute(label,code,payload=b''):
 encoded=base64.b64encode(code.encode()).decode();runner='import base64;exec(compile(base64.b64decode('+repr(encoded)+'),"Root-reviewed-'+label+'","exec"))'
 guard='set -eu; e=$(id -u); r=$(id -ru); h=$(stat -f %u /dev/console); [ "$e" = 501 ] && [ "$r" = 501 ] && [ "$h" = 501 ] && [ "$e" -ne 0 ] || exit 21; printf "remote_identity effective=%s real=%s console=%s\n" "$e" "$r" "$h"; exec '+shlex.quote(cfg['python'])+' -I -B -c '+shlex.quote(runner)
 command=[*transport,'/bin/sh -c '+shlex.quote(guard)];start=datetime.datetime.now(datetime.timezone.utc).isoformat()
 with (R/('ROOT-'+label+'-start.json')).open('x',encoding='utf-8') as f:json.dump({'status':'started','passed':False,'actor':actor,'utc':start,'command':command,'remote_code_sha256':hashlib.sha256(code.encode()).hexdigest(),'stdin_sha256':hashlib.sha256(payload).hexdigest(),'Cargo_executed':False},f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
 begin=time.monotonic();done=subprocess.run(command,cwd=ROOT,input=payload,capture_output=True,timeout=600);elapsed=time.monotonic()-begin
 (R/('ROOT-'+label+'.stdout.log')).write_bytes(done.stdout);(R/('ROOT-'+label+'.stderr.log')).write_bytes(done.stderr)
 with (R/('ROOT-'+label+'.json')).open('x',encoding='utf-8') as f:json.dump({'actor':actor,'utc':start,'command':command,'code':code,'exit_code':done.returncode,'elapsed_seconds':elapsed,'stdout_sha256':hashlib.sha256(done.stdout).hexdigest(),'stderr_sha256':hashlib.sha256(done.stderr).hexdigest(),'Cargo_executed':False},f,indent=2);f.write('\n')
 assert done.returncode==0, label+' failed; STOP, no replay; retained logs: '+done.stderr.decode(errors='replace')[:500]
 identity,raw=done.stdout.split(b'\n',1);assert identity.strip()==b'remote_identity effective=501 real=501 console=501'
 value=json.loads(raw)
 with (R/('ROOT-'+label+'-proof.json')).open('xb') as f:f.write(raw)
 print(json.dumps({'stage':label,'elapsed_seconds':elapsed,'proof_sha256':h.sha(R/('ROOT-'+label+'-proof.json')),'result':value}),flush=True)
 return value,raw
preflight="""from pathlib import Path
import os,json,subprocess,hashlib,shutil
assert os.getuid()==os.geteuid()==501==os.stat('/dev/console').st_uid and os.getuid()!=0
b=Path.home()/'airk-builds/arbiter-final-20261116-r1';s=b/'source';e=b/'evidence'
for p in (b,s,e):assert p.lstat().st_uid==501 and not p.is_symlink()
assert subprocess.check_output(['git','--no-optional-locks','rev-parse','HEAD'],cwd=s,text=True).strip()=='c547e6547b1312c2e113bd1ddc9ab84969c229c0'
assert not subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','--untracked-files=all'],cwd=s)
assert not(s/'target').exists() and not(e/'platform-process-observer-repair-r1').exists() and not(e/'platform-process-observer-repair-launch-r1').exists() and not(b/'transfer/bridge-f5f9dcec-r1').exists()
dirty=hashlib.sha256(subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','-z','--untracked-files=normal'],cwd=Path.home()/'code/airk')).hexdigest();assert dirty=='943936de2803601aa34eeb0d5c974bbb94261dc762496bd44196177755734b06'
usage=shutil.disk_usage(b);print(json.dumps({'uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid,'source':'c547e654','source_clean':True,'target_absent':True,'all_new_destinations_absent':True,'disk_free_bytes':usage.free,'disk_total_bytes':usage.total,'original_dirty_status_sha256':dirty,'Cargo_executed':False}))
"""
execute('preflight',preflight)
def archive_files(files):
 wire=io.BytesIO()
 with tarfile.open(fileobj=wire,mode='w:gz') as archive:
  for name,row in files.items():
   p=(OUT/'transfer'/name) if name in proof['transfer_files'] else OUT/name;assert h.sha(p)==row['sha256'] and p.stat().st_size==row['bytes'];data=p.read_bytes();member=tarfile.TarInfo(name);member.size=len(data);member.mode=0o600;archive.addfile(member,io.BytesIO(data))
 return wire.getvalue()
actual,raw=execute('source-receive',(OUT/'transfer/remote-source-bridge-r1.py').read_text(),archive_files(proof['transfer_files']))
assert actual['commit']==cfg['commit'] and actual['raw_c7']==cfg['raw_c7'] and actual['source_files_verified']==3509 and actual['source_delta_files_received']==10 and actual['source_clean'] and actual['Cargo_or_build_invoked'] is False
source_sha=hashlib.sha256(raw).hexdigest();expected={name:{'bytes':(OUT/name).stat().st_size,'sha256':h.sha(OUT/name)} for name in ('neo-next-integrated-platform-batch-r1.py','frozen-neo-config-r1.json')}
copy=json.loads((prior/'receipts/remote-helper-transfer-r1.json').read_text())['guarded_receiver_script'];start=copy.index(';expected=')+len(';expected=');end=copy.index('\n',start);copy=copy[:start]+repr(expected)+copy[end:]
oldcfg=json.loads((prior/'frozen-neo-config-r1.json').read_text())
replacements={'platform-admin-repair-launch-r2':'platform-process-observer-repair-launch-r1',"(e/'platform-admin-repair-r2')":"(e/'platform-process-observer-repair-r1')",'c547e6547b1312c2e113bd1ddc9ab84969c229c0':cfg['commit'],'source-bridge-c547e654-r1.json':'source-bridge-f5f9dcec-r1.json','8289c892f37f8695330b0e8e49a7d795ae66cc3788506cfa15a0971f49bb3847':source_sha,repr(oldcfg['raw_c7']):repr(cfg['raw_c7']),'41a70c2bd4239d55ff95c372778e5fdcf10f6cff30326144a818175fe51169c7':cfg['manifest_sha256']}
for before,after in replacements.items():assert before in copy;copy=copy.replace(before,after)
marker='launch.mkdir(mode=0o700);received=[]';assert marker in copy;copy=copy.replace(marker,"assert sha(e/'platform-admin-repair-r2/batch-result.json')=='5ceb627b9b4e4572d85272e9f16bc6ce31deb7f415a355e990118990e9975840'\n"+marker)
compile(copy,'Root-helper-receive','exec');ready,raw=execute('helper-receive',copy,archive_files(expected))
assert ready['commit']==cfg['commit'] and ready['raw_c7']==cfg['raw_c7'] and ready['Cargo_started'] is False
print(json.dumps({'ROOT_LAUNCH_READY':True,'source_commit':cfg['commit'],'raw_c7':cfg['raw_c7'],'remote_launch_argv':ready['launch_argv'],'local_launch_shell':str(OUT/'launch-next-batch-ROOT-ONLY-r1.sh'),'Cargo_started':False}),flush=True)
