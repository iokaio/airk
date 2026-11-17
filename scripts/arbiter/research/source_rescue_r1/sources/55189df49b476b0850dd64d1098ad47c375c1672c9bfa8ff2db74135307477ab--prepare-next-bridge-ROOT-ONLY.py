# SPDX-License-Identifier: Apache-2.0
# Private, reviewed source-only preparation. Never runs SSH, Cargo, npm or a build.
from pathlib import Path
import argparse,copy,datetime,difflib,hashlib,importlib.util,io,json,os,re,subprocess,tarfile,time
ROOT=Path(r"D:\code\Github\airk")
E=ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z'
PREP=Path(__file__).resolve().parent
BASE='f5f9dceceb91ce3d8bace0061522dbf889612c09'
OLD=E/'neo-process-observer-batch-preparation-r1/bridge-f5f9dcec-r1'
BASE_MANIFEST_SHA='e28111f98e74f71196fa43ed3aacaf32f078d1d6db38c9eaf01195c18364a1e1'
HELPER_SHA='24e04ee1cecadd6160adb814358d3338e47abe5d225c3736ba68e6b42ec16e08'
FAILED_SHA='7cd293c7826fbf1e1533f30747543056ee91b9258659e7a4898dc9761769b2d0'
SPEC=importlib.util.spec_from_file_location('approved_custody',E/'candidate-overlay-preparation-r1/candidate_overlay.py')
h=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(h)
def fresh(path,data):
 assert h.security(path.parent)['owner_sid']==h.HUMAN
 with path.open('xb') as f:f.write(data);f.flush();os.fsync(f.fileno())
 assert h.security(path)['owner_sid']==h.HUMAN

def main():
 actor=h.identity();parser=argparse.ArgumentParser(description=__doc__)
 parser.add_argument('--commit',required=True);args=parser.parse_args();commit=args.commit
 assert re.fullmatch('[0-9a-f]{40}',commit) and commit!=BASE
 assert h.git(ROOT,'rev-parse','HEAD').decode().strip()==commit
 assert not h.git(ROOT,'status','--porcelain=v1','--untracked-files=all'), 'Root must be clean at exact supplied commit'
 subprocess.run(['git','--no-optional-locks','merge-base','--is-ancestor',BASE,commit],cwd=ROOT,check=True)
 index=ROOT/'.git/index';index_sha=h.sha(index);oldfile=OLD/'transfer/source-bridge-manifest.json';assert h.sha(oldfile)==BASE_MANIFEST_SHA
 old=json.loads(oldfile.read_text());assert old['commit']==BASE
 failed=E/'neo-independent-monitor-f5-r1/completed-phase-custody-20261116T204042Z/receipts/neo-f5-independent-workspace-tests-r1.json';assert h.sha(failed)==FAILED_SHA
 applied={'scripts/dev/receipt_runner.py':'7dd351a193342cb72214fc417fc2d6a52fb5078169d6420c76b4f7b34c8489a4','scripts/tests/test_receipt_runner.py':'23c5e5a5a29566f610ad416d38f47d114b446c43acb843c8ac2cea5f9fd284d7','CONTRIBUTING.md':'e0eb52d6dd2ae61260de8906a370eaa9bf16531ff4e1724c2ac9b714952a3459'}
 for p,d in applied.items():assert h.sha(ROOT/p)==d, 'reviewed classifier/contract bytes changed: '+p
 rows=[]
 for raw in h.git(ROOT,'ls-files','--stage','-z').split(b'\0'):
  if not raw:continue
  info,name=raw.split(b'\t',1);mode,blob,stage=info.decode().split();p=os.fsdecode(name)
  assert stage=='0' and mode in ('100644','100755')
  assert not Path(p).is_absolute() and '..' not in Path(p).parts and '\\' not in p and '\n' not in p
  assert p.split('/')[0] not in ('artifacts','temp','.airk','.git'), 'private/output path cannot transfer: '+p
  path=ROOT/p;h.regular(path);assert path.resolve().is_relative_to(ROOT)
  rows.append({'path':p,'bytes':path.stat().st_size,'sha256':h.sha(path),'mode':mode,'blob':blob})
 assert len(rows)==3509, 'exactRoot3509publicclosure required'
 rows.sort(key=lambda x:x['path']);assert len({r['path'].casefold() for r in rows})==len(rows)
 names=[r['path'] for r in rows];wire=b''.join(x.encode()+b'\0' for x in names)
 attrs=h.git(ROOT,'check-attr','-z','--stdin','filter',input=wire).split(b'\0')
 assert len(attrs)==len(names)*3+1 and all(attrs[x+2]==b'unspecified' for x in range(0,len(attrs)-1,3)), 'no unreviewed clean filters'
 normalized=subprocess.run(['git','--no-optional-locks','hash-object','--stdin-paths'],cwd=ROOT,input=('\n'.join(names)+'\n').encode(),capture_output=True,check=True).stdout.decode().splitlines()
 assert len(normalized)==len(rows) and all(blob==r['blob'] for blob,r in zip(normalized,rows)), 'raw bytes must normalize to committed tree'
 current={r['path']:r for r in rows};prior={r['path']:r for r in old['files']}
 changed=[p for p in names if p not in prior or any(current[p][k]!=prior[p][k] for k in ('sha256','mode'))]
 deleted=sorted(set(prior)-set(current));assert changed, 'no changed source to transfer'
 tree=h.git(ROOT,'rev-parse','HEAD^{tree}').decode().strip();seal=h.native_seal(ROOT)
 assert commit!=BASE, 'new reviewed clean DCO source required'
 for name,digest in {'crates/airk-cli/tests/cli_commands.rs': '84fdd7e8a45aa6d87e27c6d79d0a45378c6feb1178624ee57776c6cf10269807', 'crates/airk-installer/src/runtime_install/replacement.rs': '8474abd795f3d5448cc88de5a3cbf883d020afbc07dfac66d9c767ba5762af73', 'crates/airk-installer/src/runtime_install/replacement/operation.rs': '3fffdc043346f3122cdabcdcb0e2c57ab00e08195c8c828031f375d9d1469a54', 'crates/airk-installer/src/runtime_install/replacement/tests.rs': '8970fc01751fe421e1c06bb653d57170161f136014c9eab388be70c886b6718e', 'crates/airk-cli/tests/combined_reads/reference_refresh.rs': 'e7c35d1d6af7a12cd862d39f69a3ebed1051e2367c665d6e00c227e4c51481f7'}.items():assert h.sha(ROOT/name)==digest, 'reviewed diagnostic source changed: '+name
 out=PREP/('bridge-'+commit[:8]+'-r1');assert h.security(PREP)['owner_sid']==h.HUMAN;out.mkdir();transfer=out/'transfer';transfer.mkdir();receipts=out/'receipts';receipts.mkdir()
 manifest={'schema':'airk.neo-reviewed-source-bridge/1','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'actor':actor,'base_commit':BASE,'commit':commit,'base_manifest_sha256':BASE_MANIFEST_SHA,'base_raw_c7':old['raw_c7'],'raw_c7':seal,'normalized_tree':tree,'files':rows,'raw_or_mode_changed':changed,'deleted':deleted}
 fresh(transfer/'source-bridge-manifest.json',(json.dumps(manifest,indent=2)+'\n').encode())
 with (transfer/'raw-source-delta.tar').open('xb') as stream:
  with tarfile.open(fileobj=stream,mode='w') as archive:
   for p in changed:
    data=(ROOT/p).read_bytes();assert hashlib.sha256(data).hexdigest()==current[p]['sha256']
    item=tarfile.TarInfo(p);item.size=len(data);item.mode=int(current[p]['mode'],8)&0o777;item.uid=501;item.gid=20;item.uname='';item.gname='';item.mtime=0
    archive.addfile(item,io.BytesIO(data))
 bundle='f5f9dcec-to-'+commit[:8]+'.bundle';command=['git','--no-optional-locks','bundle','create',str(transfer/bundle),'HEAD','^'+BASE]
 begin=time.monotonic();run=subprocess.run(command,cwd=ROOT,capture_output=True,check=True);elapsed=time.monotonic()-begin
 verify=subprocess.run(['git','--no-optional-locks','bundle','verify',str(transfer/bundle)],cwd=ROOT,capture_output=True,check=True)
 assert commit in verify.stdout.decode()+verify.stderr.decode()
 files={p.name:{'name':p.name,'bytes':p.stat().st_size,'sha256':h.sha(p)} for p in (transfer/'source-bridge-manifest.json',transfer/'raw-source-delta.tar',transfer/bundle)}
 receiver=(OLD/'transfer/remote-source-bridge-r1.py').read_text()
 replacements={"old_path=transfer/'bridge-c547e654-r1/source-bridge-manifest.json'": "old_path=transfer/'bridge-f5f9dcec-r1/source-bridge-manifest.json'", "sha(old_path)=='41a70c2bd4239d55ff95c372778e5fdcf10f6cff30326144a818175fe51169c7'": "sha(old_path)=='e28111f98e74f71196fa43ed3aacaf32f078d1d6db38c9eaf01195c18364a1e1'", "old['commit']=='c547e6547b1312c2e113bd1ddc9ab84969c229c0'": "old['commit']=='f5f9dceceb91ce3d8bace0061522dbf889612c09'"}
 replacements.update({
  "incoming=transfer/'bridge-f5f9dcec-r1'":"incoming=transfer/'bridge-"+commit[:8]+"-r1'",
  "manifest['commit']=='f5f9dceceb91ce3d8bace0061522dbf889612c09'":"manifest['commit']=='"+commit+"'",
  'c547e654-to-f5f9dcec.bundle':bundle,
  "e/'source-bridge-f5f9dcec-r1.json'":"e/'source-bridge-"+commit[:8]+"-r1.json'"})
 for before,after in replacements.items():assert before in receiver;receiver=receiver.replace(before,after)
 receiver,count=re.subn(r'^expected=.*$',lambda m:'expected='+repr(files),receiver,flags=re.MULTILINE);assert count==1
 compile(receiver,'remote-source-bridge-r1.py','exec');fresh(transfer/'remote-source-bridge-r1.py',receiver.encode())
 original_receiver=(OLD/'transfer/remote-source-bridge-r1.py').read_text()
 fresh(out/'receiver-review.diff',''.join(difflib.unified_diff(original_receiver.splitlines(True),receiver.splitlines(True),fromfile='reviewed-c547-receiver.py',tofile='next-source-receiver.py')).encode())
 helper=PREP/'neo-next-integrated-platform-batch-r1.py';assert h.sha(helper)==HELPER_SHA
 cfg=json.loads((PREP/'next-config-NOT-EXECUTABLE.json').read_text())
 cfg.update(status='frozen_root_commit',commit=commit,tree=tree,raw_c7=seal,manifest=cfg['base']+'/transfer/bridge-'+commit[:8]+'-r1/source-bridge-manifest.json',manifest_sha256=files['source-bridge-manifest.json']['sha256'])
 assert [c['name'] for c in cfg['commands']]==['scoped-clippy','exact-cli-replacement','exact-reference-refresh']
 fresh(out/'frozen-neo-config-r1.json',(json.dumps(cfg,indent=2)+'\n').encode());fresh(out/'neo-next-integrated-platform-batch-r1.py',helper.read_bytes())
 remote_launch=cfg['base']+'/evidence/platform-current-diagnostics-launch-r1'
 launch='set -eu\ne=$(id -u); r=$(id -ru); h=$(stat -f %u /dev/console)\n[ "$e" = 501 ] && [ "$r" = 501 ] && [ "$e" = "$h" ] && [ "$e" -ne 0 ] || exit 21\nexport PYTHONDONTWRITEBYTECODE=1\nexec '+cfg['python']+' -I -B '+remote_launch+'/neo-next-integrated-platform-batch-r1.py '+remote_launch+'/frozen-neo-config-r1.json\n'
 fresh(out/'launch-next-batch-ROOT-ONLY-r1.sh',launch.encode())
 assert not h.git(ROOT,'status','--porcelain=v1','--untracked-files=all') and h.sha(index)==index_sha
 assert h.sha(failed)==FAILED_SHA and all(h.sha(ROOT/r['path'])==r['sha256'] for r in rows)
 proof={'schema':'airk.neo-next-local-source-preparation/1','actor':actor,'status':'LOCAL ONLY; Root concrete review required before remote source or Cargo','base_commit':BASE,'commit':commit,'tree':tree,'raw_c7':seal,'source_files':len(rows),'source_delta_files':len(changed),'deleted':deleted,'Root_clean':True,'Root_index_sha256_unchanged':index_sha,'bundle_command':command,'bundle_elapsed_seconds':elapsed,'bundle_exit_code':run.returncode,'bundle_verify_stdout':verify.stdout.decode(),'bundle_verify_stderr':verify.stderr.decode(),'transfer_files':files,'receiver_sha256':h.sha(transfer/'remote-source-bridge-r1.py'),'config_sha256':h.sha(out/'frozen-neo-config-r1.json'),'helper_sha256':h.sha(helper),'old_failure_sha256_unchanged':FAILED_SHA,'native_compilation_claim':'None during preparation. New source gets scoped installer+CLI Clippy and two independent exact diagnostics; historical F5 failures retained. No full-workspace qualification inferred.'}
 fresh(receipts/'local-preparation-r1.json',(json.dumps(proof,indent=2)+'\n').encode());print(json.dumps({'out':str(out),'proof_sha256':h.sha(receipts/'local-preparation-r1.json'),**proof},indent=2))
if __name__=='__main__':main()
